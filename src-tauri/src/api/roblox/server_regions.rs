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

#[cfg_attr(test, allow(dead_code))]
fn region_cache_path() -> std::path::PathBuf {
    crate::data::settings::get_runtime_data_dir().join("ServerRegionCache.json")
}

static REGION_CACHE: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, IpRegion>>,
> = std::sync::OnceLock::new();

fn region_cache() -> &'static std::sync::Mutex<std::collections::HashMap<String, IpRegion>> {
    REGION_CACHE.get_or_init(|| std::sync::Mutex::new(load_region_cache()))
}

/// O cache em disco fica desligado nos testes: `get_runtime_data_dir` aponta
/// para a pasta real do usuário, e uma suíte de testes não escreve lá.
#[cfg(not(test))]
fn load_region_cache() -> std::collections::HashMap<String, IpRegion> {
    std::fs::read_to_string(region_cache_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

#[cfg(test)]
fn load_region_cache() -> std::collections::HashMap<String, IpRegion> {
    std::collections::HashMap::new()
}

#[cfg(not(test))]
fn persist_region_cache(map: &std::collections::HashMap<String, IpRegion>) {
    if let Ok(text) = serde_json::to_string(map) {
        let _ = std::fs::write(region_cache_path(), text);
    }
}

#[cfg(test)]
fn persist_region_cache(_map: &std::collections::HashMap<String, IpRegion>) {}

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
) -> Result<String, String> {
    let response = join_game_instance(security_token, place_id, job_id, false).await?;
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
    let message = response["message"]
        .as_str()
        .filter(|m| !m.trim().is_empty())
        .map(|m| m.to_string())
        .unwrap_or_else(|| match response["status"].as_i64() {
            Some(status) => format!("Join status {}", status),
            None => "Server address not returned".to_string(),
        });
    Err(message)
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
            }
        }
        Err(error) => ServerRegion {
            job_id: job_id.to_string(),
            region: None,
            label: String::new(),
            error: Some(error),
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

#[cfg(test)]
mod server_region_format_tests {
    use super::*;

    fn sample() -> IpRegion {
        IpRegion {
            ip: "128.116.0.1".to_string(),
            city: "São Paulo".to_string(),
            region: "Sao Paulo".to_string(),
            country: "Brazil".to_string(),
            country_code: "BR".to_string(),
        }
    }

    #[test]
    fn the_default_template_is_city_and_country_code() {
        assert_eq!(format_region(&sample(), "<city>, <countryCode>"), "São Paulo, BR");
    }

    #[test]
    fn every_placeholder_is_supported() {
        assert_eq!(format_region(&sample(), "<country>"), "Brazil");
        assert_eq!(format_region(&sample(), "<region>"), "Sao Paulo");
        assert_eq!(format_region(&sample(), "<ip>"), "128.116.0.1");
        assert_eq!(
            format_region(&sample(), "<city>/<region>/<country> (<ip>)"),
            "São Paulo/Sao Paulo/Brazil (128.116.0.1)"
        );
    }

    /// Sem dados de geolocalização o template vira pontuação solta; mostrar o
    /// IP é sempre melhor que mostrar uma célula vazia.
    #[test]
    fn a_region_without_data_falls_back_to_the_ip() {
        let unknown = IpRegion::unknown("10.0.0.7");
        assert_eq!(format_region(&unknown, "<city>, <countryCode>"), "10.0.0.7");
        assert_eq!(format_region(&unknown, ""), "10.0.0.7");
    }

    #[test]
    fn trailing_separators_are_trimmed() {
        let region = IpRegion {
            city: String::new(),
            ..sample()
        };
        assert_eq!(format_region(&region, "<city>, <countryCode>"), "BR");
    }

    #[test]
    fn the_region_filter_compares_the_country_code_case_insensitively() {
        let matching = ServerRegion {
            job_id: "job-1".to_string(),
            label: "São Paulo, BR".to_string(),
            region: Some(sample()),
            error: None,
        };
        assert!(region_matches(&matching, "br"));
        assert!(region_matches(&matching, "BR"));
        assert!(!region_matches(&matching, "US"));
        // Filtro vazio aceita tudo.
        assert!(region_matches(&matching, "  "));

        let failed = ServerRegion {
            job_id: "job-2".to_string(),
            region: None,
            label: String::new(),
            error: Some("Join status 3".to_string()),
        };
        assert!(!region_matches(&failed, "BR"));
        assert!(region_matches(&failed, ""));
    }
}

#[cfg(test)]
mod server_region_payload_tests {
    use super::*;

    #[test]
    fn ipwhois_payloads_are_read() {
        let body = serde_json::json!({
            "success": true,
            "city": "São Paulo",
            "region": "Sao Paulo",
            "country": "Brazil",
            "country_code": "BR"
        });
        let region = parse_ipwhois("1.2.3.4", &body).expect("region");
        assert_eq!(region.country_code, "BR");
        assert_eq!(region.city, "São Paulo");
        assert_eq!(region.ip, "1.2.3.4");
    }

    #[test]
    fn an_unsuccessful_ipwhois_payload_is_rejected() {
        let body = serde_json::json!({ "success": false, "message": "reserved range" });
        assert!(parse_ipwhois("10.0.0.1", &body).is_none());
        // Sucesso implícito mas sem nenhum dado também não serve.
        assert!(parse_ipwhois("10.0.0.1", &serde_json::json!({})).is_none());
    }

    #[test]
    fn ipapi_payloads_are_read_and_the_failure_status_is_rejected() {
        let body = serde_json::json!({
            "status": "success",
            "city": "Ashburn",
            "regionName": "Virginia",
            "country": "United States",
            "countryCode": "US"
        });
        let region = parse_ipapi("8.8.8.8", &body).expect("region");
        assert_eq!(region.country_code, "US");
        assert_eq!(region.region, "Virginia");

        assert!(parse_ipapi("8.8.8.8", &serde_json::json!({ "status": "fail" })).is_none());
    }
}

/// A resolução de região ponta a ponta contra o mock: join → IP →
/// geolocalização, com cache, reserva e degradação para "só o IP".
#[cfg(test)]
mod server_region_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server, mount_csrf};
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    const TEMPLATE: &str = "<city>, <countryCode>";

    /// O `join-game-instance` daquela conta devolvendo o IP da máquina.
    async fn mount_join(token: &str, job_id: &str, machine_address: &str) {
        mount_csrf(token, &format!("csrf-{}", token)).await;
        Mock::given(method("POST"))
            .and(path(mock_path("gamejoin", "/v1/join-game-instance")))
            .and(header("cookie", cookie_of(token)))
            .and(body_partial_json(serde_json::json!({ "gameId": job_id })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "joinScript": { "MachineAddress": machine_address }
            })))
            .mount(mock_server().await)
            .await;
    }

    async fn mount_ipwhois(ip: &str, body: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path(format!("/ipwhois/{}", ip)))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(mock_server().await)
            .await;
    }

    #[tokio::test]
    async fn a_server_is_resolved_to_a_city_and_country() {
        mount_join("region-ok", "job-br", "200.10.0.1").await;
        mount_ipwhois(
            "200.10.0.1",
            serde_json::json!({
                "success": true,
                "city": "São Paulo",
                "region": "Sao Paulo",
                "country": "Brazil",
                "country_code": "BR"
            }),
        )
        .await;

        let resolved = resolve_server_region("region-ok", 606849621, "job-br", TEMPLATE).await;
        assert_eq!(resolved.job_id, "job-br");
        assert_eq!(resolved.label, "São Paulo, BR");
        assert_eq!(resolved.region.as_ref().unwrap().country_code, "BR");
        assert!(resolved.error.is_none());
        assert!(region_matches(&resolved, "BR"));
    }

    /// Cada IP é consultado uma vez só — é o que torna o filtro de região
    /// viável depois da primeira varredura.
    #[tokio::test]
    async fn an_ip_is_geolocated_only_once() {
        mount_ipwhois(
            "200.10.0.2",
            serde_json::json!({ "success": true, "city": "Rio", "country_code": "BR" }),
        )
        .await;

        let first = lookup_ip_region("200.10.0.2").await;
        let second = lookup_ip_region("200.10.0.2").await;
        assert_eq!(first, second);

        let calls = mock_server()
            .await
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|r| r.url.path() == "/ipwhois/200.10.0.2")
            .count();
        assert_eq!(calls, 1, "o segundo pedido tinha que sair do cache");
    }

    /// O serviço principal fora do ar não pode derrubar a coluna Region — foi
    /// exatamente o que aconteceu quando o `ipapi.co` passou a exigir desafio.
    #[tokio::test]
    async fn the_backup_service_answers_when_the_first_one_fails() {
        Mock::given(method("GET"))
            .and(path("/ipwhois/200.10.0.3"))
            .respond_with(ResponseTemplate::new(403).set_body_string("challenge"))
            .mount(mock_server().await)
            .await;
        Mock::given(method("GET"))
            .and(path("/ipapi/json/200.10.0.3"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "success",
                "city": "Ashburn",
                "regionName": "Virginia",
                "country": "United States",
                "countryCode": "US"
            })))
            .mount(mock_server().await)
            .await;

        let region = lookup_ip_region("200.10.0.3").await;
        assert_eq!(region.country_code, "US");
        assert_eq!(region.city, "Ashburn");
    }

    /// Os dois serviços fora: mostra o IP, não um erro.
    #[tokio::test]
    async fn without_geolocation_the_ip_itself_is_the_region() {
        Mock::given(method("GET"))
            .and(path("/ipwhois/200.10.0.4"))
            .respond_with(ResponseTemplate::new(500))
            .mount(mock_server().await)
            .await;
        Mock::given(method("GET"))
            .and(path("/ipapi/json/200.10.0.4"))
            .respond_with(ResponseTemplate::new(500))
            .mount(mock_server().await)
            .await;

        let region = lookup_ip_region("200.10.0.4").await;
        assert_eq!(region.country_code, "");
        assert_eq!(format_region(&region, TEMPLATE), "200.10.0.4");
    }

    /// O join responde 200 com payload de erro quando o servidor encheu; a
    /// mensagem dele tem que chegar ao usuário.
    #[tokio::test]
    async fn a_join_without_an_address_reports_the_roblox_message() {
        mount_csrf("region-full", "csrf-region-full").await;
        Mock::given(method("POST"))
            .and(path(mock_path("gamejoin", "/v1/join-game-instance")))
            .and(header("cookie", cookie_of("region-full")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": 12,
                "message": "This game is full"
            })))
            .mount(mock_server().await)
            .await;

        let resolved = resolve_server_region("region-full", 1, "job-cheio", TEMPLATE).await;
        assert_eq!(resolved.error.as_deref(), Some("This game is full"));
        assert!(resolved.region.is_none());
        assert!(!region_matches(&resolved, "BR"));
    }

    /// O lote informa progresso e respeita o teto de servidores.
    #[tokio::test]
    async fn the_batch_reports_progress_and_caps_the_number_of_lookups() {
        mount_join("region-batch", "job-a", "200.10.0.5").await;
        mount_ipwhois(
            "200.10.0.5",
            serde_json::json!({ "success": true, "city": "Santiago", "country_code": "CL" }),
        )
        .await;

        let mut seen: Vec<(usize, usize)> = Vec::new();
        let jobs = vec!["job-a".to_string(), "  ".to_string()];
        let results = resolve_server_regions(
            "region-batch",
            1,
            &jobs,
            TEMPLATE,
            0,
            |done, total| seen.push((done, total)),
        )
        .await;

        assert_eq!(results.len(), 1, "o Job ID em branco é descartado");
        assert_eq!(results[0].label, "Santiago, CL");
        assert_eq!(seen, vec![(0, 1), (1, 1)]);
    }
}
