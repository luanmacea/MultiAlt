// Escolha automática do servidor público de um lote.
//
// Antes só existia o `shuffleJob` (servidor aleatório). Agora o usuário escolhe
// uma preferência e ela vale para o **lote inteiro**: o servidor é resolvido
// UMA vez e todas as contas selecionadas entram nele — senão cada conta cairia
// num servidor diferente e o lote deixaria de jogar junto.
//
// A ordenação vem da própria API (confirmado contra ela):
// `sortOrder=Asc` devolve os servidores com MENOS jogadores primeiro e `Desc`
// com mais. `excludeFullGames=true` tira os lotados.

/// O que o usuário escolheu na UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerPreference {
    /// Qualquer servidor com vaga (comportamento antigo do `shuffleJob`).
    Random,
    /// Menos jogadores primeiro.
    Emptiest,
    /// Mais jogadores primeiro (sem contar os lotados).
    Fullest,
}

impl ServerPreference {
    /// `sortOrder` que a API deve usar para já vir na ordem certa.
    pub fn sort_order(self) -> &'static str {
        match self {
            ServerPreference::Fullest => "Desc",
            _ => "Asc",
        }
    }
}

/// Texto da UI → preferência. Desconhecido (e vazio) vira `Random`, que é o
/// comportamento antigo — nenhuma preferência inválida pode impedir o launch.
pub fn parse_server_preference(value: &str) -> ServerPreference {
    match value.trim().to_ascii_lowercase().as_str() {
        "emptiest" | "empty" | "fewest" => ServerPreference::Emptiest,
        "fullest" | "full" | "most" => ServerPreference::Fullest,
        _ => ServerPreference::Random,
    }
}

/// Servidores que cabem o lote inteiro, na ordem da preferência.
///
/// Regras:
/// - só entra servidor com vaga para **todas** as contas (`playing + contas <=
///   maxPlayers`) — mandar 8 contas para um servidor com 2 vagas deixaria 6 de
///   fora, cada uma num servidor aleatório;
/// - se nenhum couber inteiro, cai para os que têm **alguma** vaga, senão o
///   launch ficaria travado num jogo cheio;
/// - a ordenação é refeita aqui e não é assumida da API: a resposta vem
///   ordenada, mas paginação e cache do Roblox já devolveram listas fora de
///   ordem.
pub fn rank_servers(
    servers: &[ServerData],
    preference: ServerPreference,
    accounts: usize,
) -> Vec<&ServerData> {
    let needed = accounts.max(1) as i32;
    let fits_all: Vec<&ServerData> = servers
        .iter()
        .filter(|s| s.max_players > 0 && s.playing + needed <= s.max_players)
        .collect();

    let mut candidates = if fits_all.is_empty() {
        servers
            .iter()
            .filter(|s| s.max_players > 0 && s.playing < s.max_players)
            .collect::<Vec<_>>()
    } else {
        fits_all
    };

    match preference {
        ServerPreference::Emptiest => candidates.sort_by_key(|s| s.playing),
        ServerPreference::Fullest => candidates.sort_by_key(|s| -s.playing),
        // Aleatório mantém a ordem da API; quem sorteia é `pick_from_list`.
        ServerPreference::Random => {}
    }
    candidates
}

/// Escolhe um servidor da lista. Puro (o `nanos` é a fonte de aleatoriedade),
/// para poder ser testado sem rede nem relógio.
pub fn pick_from_list(
    servers: &[ServerData],
    preference: ServerPreference,
    accounts: usize,
    nanos: u128,
) -> Option<String> {
    let ranked = rank_servers(servers, preference, accounts);
    let chosen = match preference {
        ServerPreference::Random => ranked.get((nanos as usize) % ranked.len().max(1))?,
        _ => ranked.first()?,
    };
    Some(chosen.id.clone())
}

/// O servidor escolhido, com o que a UI precisa dizer ao usuário.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickedServer {
    pub job_id: String,
    pub playing: i32,
    pub max_players: i32,
    /// Preenchido só quando houve filtro de região.
    pub region: Option<ServerRegion>,
    /// `true` quando o filtro de região não achou ninguém e a escolha caiu no
    /// melhor servidor disponível. A UI pergunta antes de entrar.
    pub region_fallback: bool,
}

fn picked_from(server: &ServerData, region: Option<ServerRegion>, fallback: bool) -> PickedServer {
    PickedServer {
        job_id: server.id.clone(),
        playing: server.playing,
        max_players: server.max_players,
        region,
        region_fallback: fallback,
    }
}

/// Escolhe o servidor do lote, opcionalmente exigindo um país.
///
/// Sem filtro de região é uma única chamada à lista de servidores. Com filtro,
/// percorre os candidatos **na ordem da preferência** resolvendo a região de um
/// por vez (cada um custa um `join-game-instance`) e para no primeiro que bate
/// — achar o servidor certo cedo é o caso comum, e o teto de `max_lookups`
/// impede que uma região inexistente vire uma varredura infinita.
pub async fn pick_server(
    security_token: &str,
    place_id: i64,
    preference: ServerPreference,
    accounts: usize,
    country_code: &str,
    region_template: &str,
    max_lookups: usize,
) -> Result<PickedServer, String> {
    let response = get_servers_sorted(place_id, preference.sort_order(), Some(security_token)).await?;
    let ranked = rank_servers(&response.data, preference, accounts);
    if ranked.is_empty() {
        return Err("No public server with room was found".to_string());
    }

    let wanted = country_code.trim();
    if wanted.is_empty() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let chosen = match preference {
            ServerPreference::Random => ranked[(nanos as usize) % ranked.len()],
            _ => ranked[0],
        };
        return Ok(picked_from(chosen, None, false));
    }

    let budget = max_lookups.min(REGION_LOOKUP_MAX).max(1);
    for server in ranked.iter().take(budget) {
        let region =
            resolve_server_region(security_token, place_id, &server.id, region_template).await;
        if region_matches(&region, wanted) {
            return Ok(picked_from(server, Some(region), false));
        }
        tokio::time::sleep(std::time::Duration::from_millis(REGION_LOOKUP_DELAY_MS)).await;
    }

    // Nada na região pedida dentro do teto: devolve o melhor disponível
    // marcado como fallback. Quem decide se entra assim é o usuário.
    Ok(picked_from(ranked[0], None, true))
}

/// `get_servers` com `sortOrder`/`excludeFullGames` explícitos.
///
/// O `get_servers` original fixa `sortOrder=Asc` e é usado pelo navegador de
/// servidores; esta variante existe para a preferência poder pedir `Desc`.
pub async fn get_servers_sorted(
    place_id: i64,
    sort_order: &str,
    security_token: Option<&str>,
) -> Result<ServersResponse, String> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1/games/{}/servers/Public?sortOrder={}&excludeFullGames=true&limit=100",
        endpoints::host("games"),
        place_id,
        sort_order
    );

    let response = send_with_retry(|| {
        let mut request = client.get(&url);
        if let Some(token) = security_token {
            request = request.header(COOKIE, cookie_header(token));
        }
        request
    })
    .await?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to get servers (status {})",
            response.status().as_u16()
        ));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse servers: {}", e))
}
