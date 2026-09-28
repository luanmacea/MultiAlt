// Região de um servidor do Roblox.
//
// A lista de servidores (`games/v1/games/{place}/servers/Public`) **não** traz
// região — nunca trouxe. O único caminho que funciona (o mesmo usado pelas
// extensões de navegador) é indireto:
//
// 1. `POST gamejoin/v1/join-game-instance` para aquele Job ID devolve o
//    `joinScript.MachineAddress`, que é o IP da máquina que hospeda o servidor;
// 2. esse IP é geolocalizado por um serviço externo.
//
// Isso custa **uma** chamada de join por servidor, e o `join-game-instance` tem
// rate limit. Por isso nada aqui resolve região sozinho: quem chama decide
// quantos servidores quer resolver, com pausa entre eles, e todo IP já visto
// sai do cache (memória + disco) sem nova consulta.

/// Região de um servidor, já resolvida a partir do IP da máquina.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IpRegion {
    pub ip: String,
    pub city: String,
    /// Estado/província, quando o serviço informa.
    pub region: String,
    pub country: String,
    pub country_code: String,
}

impl IpRegion {
    /// Só com o IP: é o que sobra quando a geolocalização falha. Mostrar o IP
    /// cru é melhor que mostrar "erro" — ainda dá para comparar servidores.
    fn unknown(ip: &str) -> Self {
        Self {
            ip: ip.to_string(),
            city: String::new(),
            region: String::new(),
            country: String::new(),
            country_code: String::new(),
        }
    }
}

/// Um servidor com a região resolvida (ou o motivo de não ter sido).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerRegion {
    pub job_id: String,
    pub region: Option<IpRegion>,
    /// Texto já formatado pelo template de `General.ServerRegionFormat`.
    pub label: String,
    pub error: Option<String>,
    /// O Roblox recusou o join **por falta de permissão** (`status` 12, o erro
    /// 524 do cliente) para a conta que fez a consulta. Vem da mesma chamada da
    /// região: marcar não custa nada a mais.
    #[serde(default)]
    pub denied: bool,
}

/// `status` do `join-game-instance` para "Unauthorized": o cliente mostra "You
/// do not have permission to join this experience" (erro 524). É pelo código,
/// não pelo texto, porque a mensagem vem no idioma da conta.
const JOIN_STATUS_UNAUTHORIZED: i64 = 12;

/// O join de um servidor foi recusado: a mensagem do Roblox e o `status`,
/// quando a resposta trouxe um.
#[derive(Debug, Clone, PartialEq)]
pub struct JoinRefusal {
    pub message: String,
    pub status: Option<i64>,
}

impl JoinRefusal {
    /// Recusa por falta de permissão (o erro 524), e não por servidor cheio,
    /// sumido ou falha de rede.
    pub fn denied(&self) -> bool {
        self.status == Some(JOIN_STATUS_UNAUTHORIZED)
    }
}

/// Progresso de `resolve_server_regions`, para a UI não ficar parada.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerRegionProgress {
    pub done: usize,
    pub total: usize,
}

/// Pausa entre chamadas de `join-game-instance`. O endpoint é o mesmo que o
/// cliente usa para entrar no jogo; disparar em rajada toma 429 e, pior, marca
/// a conta.
pub const REGION_LOOKUP_DELAY_MS: u64 = 900;

/// Teto de servidores por pedido. Resolver 100 servidores levaria minutos e
/// não é o que o usuário quer — ele quer achar UM servidor na região dele.
pub const REGION_LOOKUP_MAX: usize = 40;

/// Aplica o template de `General.ServerRegionFormat`.
///
/// Placeholders: `<city>`, `<region>`, `<country>`, `<countryCode>`, `<ip>`.
/// Um template que não resolve nada (ou uma região sem dados) cai no IP, para
/// a coluna nunca ficar vazia.
pub fn format_region(region: &IpRegion, template: &str) -> String {
    let filled = template
        .replace("<city>", &region.city)
        .replace("<region>", &region.region)
        .replace("<countryCode>", &region.country_code)
        .replace("<country>", &region.country)
        .replace("<ip>", &region.ip);

    // Sobrou só pontuação do template (ex.: ", " quando cidade e país vieram
    // vazios)? Então não há nada de útil para mostrar.
    let has_content = filled.chars().any(|c| c.is_alphanumeric());
    if has_content {
        filled.trim().trim_matches(',').trim().to_string()
    } else {
        region.ip.clone()
    }
}

// ---------------------------------------------------------------------------
// Cache de IP → região
// ---------------------------------------------------------------------------
//
// Os datacenters do Roblox são poucos e os IPs se repetem entre servidores e
// entre sessões. O cache em disco é um arquivo à parte, **fora** de
// `DATA_FILES`: é cache descartável, não entra em backup nem em migração.

fn region_cache_path() -> std::path::PathBuf {
    crate::data::settings::get_runtime_data_dir().join("ServerRegionCache.json")
}

static REGION_CACHE: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, IpRegion>>,
> = std::sync::OnceLock::new();

fn region_cache() -> &'static std::sync::Mutex<std::collections::HashMap<String, IpRegion>> {
    REGION_CACHE.get_or_init(|| std::sync::Mutex::new(load_region_cache()))
}

fn load_region_cache() -> std::collections::HashMap<String, IpRegion> {
    std::fs::read_to_string(region_cache_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn persist_region_cache(map: &std::collections::HashMap<String, IpRegion>) {
    if let Ok(text) = serde_json::to_string(map) {
        let _ = std::fs::write(region_cache_path(), text);
    }
}

fn cached_region(ip: &str) -> Option<IpRegion> {
    region_cache().lock().ok()?.get(ip).cloned()
}

/// Guarda o IP resolvido. A escrita em disco é best-effort: perder o cache só
/// custa uma consulta a mais depois.
fn cache_region(region: &IpRegion) {
    let Ok(mut map) = region_cache().lock() else {
        return;
    };
    map.insert(region.ip.clone(), region.clone());
    persist_region_cache(&map);
}

// ---------------------------------------------------------------------------
// Geolocalização
// ---------------------------------------------------------------------------

/// `ipwho.is` — gratuito, HTTPS, sem chave.
fn parse_ipwhois(ip: &str, body: &serde_json::Value) -> Option<IpRegion> {
    if body["success"].as_bool() == Some(false) {
        return None;
    }
    let country_code = body["country_code"].as_str().unwrap_or_default();
    let city = body["city"].as_str().unwrap_or_default();
    if country_code.is_empty() && city.is_empty() {
        return None;
    }
    Some(IpRegion {
        ip: ip.to_string(),
        city: city.to_string(),
        region: body["region"].as_str().unwrap_or_default().to_string(),
        country: body["country"].as_str().unwrap_or_default().to_string(),
        country_code: country_code.to_string(),
    })
}

/// `ip-api.com` — reserva, só HTTP no plano grátis. Usado quando o primeiro
/// serviço sai do ar (já aconteceu: o `ipapi.co` que o app usava antes passou a
/// responder com desafio do Cloudflare e a coluna Region parou de funcionar).
fn parse_ipapi(ip: &str, body: &serde_json::Value) -> Option<IpRegion> {
    if body["status"].as_str() != Some("success") {
        return None;
    }
    Some(IpRegion {
        ip: ip.to_string(),
        city: body["city"].as_str().unwrap_or_default().to_string(),
        region: body["regionName"].as_str().unwrap_or_default().to_string(),
        country: body["country"].as_str().unwrap_or_default().to_string(),
        country_code: body["countryCode"].as_str().unwrap_or_default().to_string(),
    })
}

async fn fetch_json(url: &str) -> Option<serde_json::Value> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json().await.ok()
}

/// Geolocaliza um IP, com cache. Nunca falha por causa do serviço externo: sem
/// resposta, devolve a região "só IP".
pub async fn lookup_ip_region(ip: &str) -> IpRegion {
    let ip = ip.trim();
    if ip.is_empty() {
        return IpRegion::unknown("");
    }
    if let Some(hit) = cached_region(ip) {
        return hit;
    }

    let primary = fetch_json(&format!("{}/{}", endpoints::external_host("ipwhois"), ip))
        .await
        .and_then(|body| parse_ipwhois(ip, &body));

    let resolved = match primary {
        Some(region) => Some(region),
        None => fetch_json(&format!(
            "{}/json/{}?fields=status,country,countryCode,regionName,city",
            endpoints::external_host("ipapi"),
            ip
        ))
        .await
        .and_then(|body| parse_ipapi(ip, &body)),
    };

    match resolved {
        Some(region) => {
            cache_region(&region);
            region
        }
        // Não cacheia a falha: o serviço pode voltar.
        None => IpRegion::unknown(ip),
    }
}

/// IP da máquina que hospeda um Job ID, lido do join script.
pub async fn server_machine_address(
    security_token: &str,
    place_id: i64,
    job_id: &str,
) -> Result<String, JoinRefusal> {
    let response = join_game_instance(security_token, place_id, job_id, false)
        .await
        .map_err(|message| JoinRefusal { message, status: None })?;
    let address = response["joinScript"]["MachineAddress"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();

    if !address.is_empty() {
        return Ok(address);
    }

    // O join responde 200 com um payload de erro quando o servidor encheu ou
    // sumiu; a mensagem dele é mais útil que "sem endereço".
    let status = response["status"].as_i64();
    let message = response["message"]
        .as_str()
        .filter(|m| !m.trim().is_empty())
        .map(|m| m.to_string())
        .unwrap_or_else(|| match status {
            Some(status) => format!("Join status {}", status),
            None => "Server address not returned".to_string(),
        });
    Err(JoinRefusal { message, status })
}

/// A conta pode entrar naquele servidor? É o mesmo pedido de entrada da
/// região, sem a geolocalização. `Err` com `denied()` = sem permissão.
pub async fn join_access(
    security_token: &str,
    place_id: i64,
    job_id: &str,
) -> Result<(), JoinRefusal> {
    server_machine_address(security_token, place_id, job_id)
        .await
        .map(|_| ())
}

/// Região de um servidor: join → IP → geolocalização.
pub async fn resolve_server_region(
    security_token: &str,
    place_id: i64,
    job_id: &str,
    template: &str,
) -> ServerRegion {
    match server_machine_address(security_token, place_id, job_id).await {
        Ok(ip) => {
            let region = lookup_ip_region(&ip).await;
            ServerRegion {
                job_id: job_id.to_string(),
                label: format_region(&region, template),
                region: Some(region),
                error: None,
                denied: false,
            }
        }
        Err(refusal) => ServerRegion {
            job_id: job_id.to_string(),
            region: None,
            label: String::new(),
            denied: refusal.denied(),
            error: Some(refusal.message),
        },
    }
}

/// Resolve vários servidores **em sequência**, com pausa entre eles, chamando
/// `on_progress(feitas, total)` a cada um. Servidores já em cache de IP ainda
/// custam o join (o IP vem do join), então o teto continua valendo.
pub async fn resolve_server_regions<F>(
    security_token: &str,
    place_id: i64,
    job_ids: &[String],
    template: &str,
    delay_ms: u64,
    mut on_progress: F,
) -> Vec<ServerRegion>
where
    F: FnMut(usize, usize),
{
    let wanted: Vec<&String> = job_ids
        .iter()
        .filter(|j| !j.trim().is_empty())
        .take(REGION_LOOKUP_MAX)
        .collect();
    let total = wanted.len();
    let delay = std::time::Duration::from_millis(delay_ms);
    let mut out = Vec::with_capacity(total);

    on_progress(0, total);
    for (index, job_id) in wanted.iter().enumerate() {
        out.push(resolve_server_region(security_token, place_id, job_id, template).await);
        on_progress(index + 1, total);
        if index + 1 < total && !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
    }
    out
}

/// O servidor está na região pedida? Compara pelo código de país (`BR`), que é
/// o que o usuário escolhe na UI.
pub fn region_matches(region: &ServerRegion, country_code: &str) -> bool {
    let wanted = country_code.trim();
    if wanted.is_empty() {
        return true;
    }
    region
        .region
        .as_ref()
        .map(|r| r.country_code.eq_ignore_ascii_case(wanted))
        .unwrap_or(false)
}
