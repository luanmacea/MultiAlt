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

/// Folga deixada depois que o lote entra.
///
/// "Mais cheio que caiba" sem folga significa entrar num servidor que fica
/// lotado no instante seguinte — qualquer jogador que tente entrar depois
/// (inclusive uma conta que caiu e voltou) não consegue. Uma vaga de sobra
/// resolve isso sem esvaziar o servidor.
pub const FREE_SEAT_BUFFER: i32 = 1;

/// O que o usuário escolheu na UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerPreference {
    /// Não escolhe nada: Job ID vazio e o Roblox decide (comportamento antigo).
    None,
    /// O mais cheio que ainda caiba o lote **com uma vaga de folga**. É o
    /// padrão: joga junto de outras pessoas sem arriscar deixar conta de fora.
    BestFit,
    /// Qualquer servidor com vaga (comportamento antigo do `shuffleJob`).
    Random,
    /// Menos jogadores primeiro.
    Emptiest,
    /// Mais jogadores primeiro (sem contar os lotados).
    Fullest,
}

impl ServerPreference {
    /// `sortOrder` que a API deve usar para já vir na ordem certa.
    ///
    /// `BestFit` pede `Desc` porque a busca desce dos mais cheios até achar a
    /// faixa que cabe o lote; pedir `Asc` traria milhares de servidores vazios
    /// antes de chegar lá.
    pub fn sort_order(self) -> &'static str {
        match self {
            ServerPreference::Fullest | ServerPreference::BestFit => "Desc",
            _ => "Asc",
        }
    }
}

/// Texto da UI → preferência. Desconhecido (e vazio) vira `BestFit`, que é o
/// padrão — nenhuma preferência inválida pode impedir o launch.
///
/// `"default"` era o nome antigo de "não escolhe nada" e hoje significa o
/// padrão novo; quem quer o comportamento antigo escolhe `"none"`.
pub fn parse_server_preference(value: &str) -> ServerPreference {
    match value.trim().to_ascii_lowercase().as_str() {
        "none" | "off" => ServerPreference::None,
        "random" | "shuffle" => ServerPreference::Random,
        "emptiest" | "empty" | "fewest" => ServerPreference::Emptiest,
        "fullest" | "full" | "most" => ServerPreference::Fullest,
        _ => ServerPreference::BestFit,
    }
}

/// Quantos jogadores um servidor pode ter para o lote caber com folga.
///
/// Nunca negativo: um servidor de 4 lugares com um lote de 8 daria `-5` e
/// qualquer comparação com isso viraria bug.
pub fn best_fit_cap(max_players: i32, accounts: usize) -> i32 {
    (max_players - accounts.max(1) as i32 - FREE_SEAT_BUFFER).max(0)
}

/// O servidor cabe o lote **e** deixa a folga?
pub fn fits_with_buffer(server: &ServerData, accounts: usize) -> bool {
    server.max_players > 0 && server.playing <= best_fit_cap(server.max_players, accounts)
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
        ServerPreference::BestFit => {
            // Com folga primeiro, do mais cheio para o mais vazio; sem nenhum
            // com folga, vale o que couber (a folga é preferência, não regra).
            let with_buffer: Vec<&ServerData> = candidates
                .iter()
                .copied()
                .filter(|s| fits_with_buffer(s, accounts))
                .collect();
            if !with_buffer.is_empty() {
                candidates = with_buffer;
            }
            candidates.sort_by_key(|s| -s.playing);
        }
        // Aleatório e "não escolhe" mantêm a ordem da API; quem sorteia é
        // `pick_from_list`.
        ServerPreference::Random | ServerPreference::None => {}
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
    let servers = collect_servers_for(
        place_id,
        Some(security_token),
        preference,
        accounts,
        BEST_FIT_MAX_PAGES,
    )
    .await?;
    let ranked = rank_servers(&servers, preference, accounts);
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
            ServerPreference::Random | ServerPreference::None => {
                ranked[(nanos as usize) % ranked.len()]
            }
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

/// Páginas que a busca do "melhor encaixe" percorre antes de desistir.
pub const BEST_FIT_MAX_PAGES: usize = 6;

/// Junta servidores até achar os que cabem o lote com folga.
///
/// Com `Desc` a primeira página traz os mais cheios, e num jogo popular eles
/// estão todos com uma ou duas vagas. Descer as páginas chega na faixa em que o
/// lote cabe. Sem isso, "melhor encaixe" nunca via um servidor utilizável num
/// jogo grande — e cair no `Asc` traria os servidores vazios, que é justamente
/// o que o usuário não quer.
///
/// As outras preferências pedem uma página só: a ordem da API já entrega o que
/// elas querem no topo.
pub async fn collect_servers_for(
    place_id: i64,
    security_token: Option<&str>,
    preference: ServerPreference,
    accounts: usize,
    max_pages: usize,
) -> Result<Vec<ServerData>, String> {
    let pages = if preference == ServerPreference::BestFit {
        max_pages.clamp(1, BEST_FIT_MAX_PAGES)
    } else {
        1
    };

    let mut all: Vec<ServerData> = Vec::new();
    let mut cursor: Option<String> = None;

    for _ in 0..pages {
        let page = get_servers_page(
            place_id,
            "Public",
            cursor.as_deref(),
            security_token,
            preference.sort_order(),
            true,
        )
        .await?;
        let next = page.next_page_cursor.clone();
        all.extend(page.data);

        if preference != ServerPreference::BestFit {
            break;
        }
        if all.iter().any(|s| fits_with_buffer(s, accounts)) {
            break;
        }
        match next {
            Some(c) if !c.trim().is_empty() => cursor = Some(c),
            _ => break,
        }
    }

    Ok(all)
}

/// Atalho para a lista pública já ordenada e sem servidores lotados.
pub async fn get_servers_sorted(
    place_id: i64,
    sort_order: &str,
    security_token: Option<&str>,
) -> Result<ServersResponse, String> {
    get_servers_page(place_id, "Public", None, security_token, sort_order, true).await
}

#[cfg(test)]
mod server_preference_tests {
    use super::*;

    fn server(id: &str, playing: i32, max_players: i32) -> ServerData {
        ServerData {
            id: id.to_string(),
            max_players,
            playing,
            player_tokens: Vec::new(),
            fps: 60.0,
            ping: None,
            name: None,
            vip_server_id: None,
            access_code: None,
        }
    }

    #[test]
    fn the_preference_is_parsed_from_the_ui_text() {
        assert_eq!(parse_server_preference("emptiest"), ServerPreference::Emptiest);
        assert_eq!(parse_server_preference("  Fullest "), ServerPreference::Fullest);
        assert_eq!(parse_server_preference("random"), ServerPreference::Random);
        assert_eq!(parse_server_preference("none"), ServerPreference::None);
        // Desconhecido, vazio e o nome antigo caem no padrão novo.
        assert_eq!(parse_server_preference(""), ServerPreference::BestFit);
        assert_eq!(parse_server_preference("default"), ServerPreference::BestFit);
        assert_eq!(parse_server_preference("qualquer coisa"), ServerPreference::BestFit);
    }

    #[test]
    fn the_sort_order_follows_the_preference() {
        assert_eq!(ServerPreference::Emptiest.sort_order(), "Asc");
        assert_eq!(ServerPreference::Fullest.sort_order(), "Desc");
        assert_eq!(ServerPreference::Random.sort_order(), "Asc");
        // "Melhor encaixe" desce dos mais cheios até a faixa que cabe o lote.
        assert_eq!(ServerPreference::BestFit.sort_order(), "Desc");
    }

    /// O pedido do usuário: 4 contas num servidor de 12 devem cair num de 7,
    /// não num de 1 nem num de 8 (que ficaria lotado no instante seguinte).
    #[test]
    fn best_fit_takes_the_fullest_server_that_still_leaves_a_free_seat() {
        let servers = vec![
            server("lotado", 11, 12),
            server("sem-folga", 8, 12),
            server("ideal", 7, 12),
            server("ok", 5, 12),
            server("vazio", 1, 12),
        ];
        assert_eq!(
            pick_from_list(&servers, ServerPreference::BestFit, 4, 0).as_deref(),
            Some("ideal")
        );
    }

    #[test]
    fn the_cap_is_never_negative() {
        // Lote maior que o servidor inteiro: o teto vira 0, não -5.
        assert_eq!(best_fit_cap(4, 8), 0);
        assert_eq!(best_fit_cap(12, 4), 7);
        assert_eq!(best_fit_cap(13, 4), 8);
        assert_eq!(best_fit_cap(0, 1), 0);
    }

    /// A folga é preferência, não regra: sem nenhum servidor com folga, o lote
    /// entra no que couber em vez de não entrar em nada.
    #[test]
    fn best_fit_falls_back_to_an_exact_fit_when_no_server_has_a_spare_seat() {
        let servers = vec![server("justo", 8, 12), server("lotado", 12, 12)];
        assert_eq!(
            pick_from_list(&servers, ServerPreference::BestFit, 4, 0).as_deref(),
            Some("justo")
        );
    }

    #[test]
    fn best_fit_orders_from_the_fullest_that_fits_downwards() {
        let servers = vec![server("a", 2, 12), server("b", 7, 12), server("c", 5, 12)];
        let ids: Vec<&str> = rank_servers(&servers, ServerPreference::BestFit, 4)
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(ids, vec!["b", "c", "a"]);
    }

    #[test]
    fn the_emptiest_server_is_the_one_with_fewest_players() {
        let servers = vec![server("a", 10, 30), server("b", 2, 30), server("c", 25, 30)];
        assert_eq!(
            pick_from_list(&servers, ServerPreference::Emptiest, 1, 0).as_deref(),
            Some("b")
        );
    }

    #[test]
    fn the_fullest_server_is_the_one_with_most_players_that_still_fits() {
        let servers = vec![server("a", 10, 30), server("b", 2, 30), server("c", 25, 30)];
        assert_eq!(
            pick_from_list(&servers, ServerPreference::Fullest, 1, 0).as_deref(),
            Some("c")
        );
    }

    /// A regra que o lote depende: o servidor tem que caber TODAS as contas.
    #[test]
    fn a_server_without_room_for_the_whole_batch_is_skipped() {
        let servers = vec![server("quase-cheio", 28, 30), server("vazio", 1, 30)];
        assert_eq!(
            pick_from_list(&servers, ServerPreference::Fullest, 8, 0).as_deref(),
            Some("vazio"),
            "o mais cheio não cabe 8 contas"
        );
        // Com uma conta só, o mais cheio volta a ser válido.
        assert_eq!(
            pick_from_list(&servers, ServerPreference::Fullest, 1, 0).as_deref(),
            Some("quase-cheio")
        );
    }

    /// Nenhum servidor cabe o lote inteiro: em vez de falhar, usa os que têm
    /// alguma vaga.
    #[test]
    fn when_nothing_fits_the_batch_the_servers_with_any_room_are_used() {
        let servers = vec![server("a", 29, 30), server("b", 28, 30)];
        assert_eq!(
            pick_from_list(&servers, ServerPreference::Emptiest, 16, 0).as_deref(),
            Some("b")
        );
    }

    #[test]
    fn full_servers_are_never_picked() {
        let servers = vec![server("cheio", 30, 30), server("outro-cheio", 31, 30)];
        assert!(pick_from_list(&servers, ServerPreference::Emptiest, 1, 0).is_none());
        assert!(pick_from_list(&servers, ServerPreference::Random, 1, 7).is_none());
    }

    #[test]
    fn a_server_without_max_players_is_ignored() {
        let servers = vec![server("sem-limite", 0, 0), server("bom", 5, 30)];
        assert_eq!(
            pick_from_list(&servers, ServerPreference::Emptiest, 1, 0).as_deref(),
            Some("bom")
        );
    }

    /// Aleatório percorre a lista conforme a semente, sem sair dos candidatos.
    #[test]
    fn random_stays_inside_the_candidates() {
        let servers = vec![server("a", 1, 30), server("b", 2, 30), server("cheio", 30, 30)];
        for nanos in 0..6u128 {
            let picked = pick_from_list(&servers, ServerPreference::Random, 1, nanos)
                .expect("algum servidor");
            assert!(picked == "a" || picked == "b", "escolheu {}", picked);
        }
    }

    #[test]
    fn an_empty_list_picks_nothing() {
        assert!(pick_from_list(&[], ServerPreference::Emptiest, 1, 0).is_none());
        assert!(rank_servers(&[], ServerPreference::Random, 4).is_empty());
    }

    /// `rank_servers` devolve a ordem completa — é ela que a busca por região
    /// percorre, então a ordem precisa estar certa e não só o primeiro.
    #[test]
    fn the_ranking_is_ordered_for_the_region_search() {
        let servers = vec![server("a", 10, 30), server("b", 2, 30), server("c", 25, 30)];
        let ids: Vec<&str> = rank_servers(&servers, ServerPreference::Emptiest, 1)
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(ids, vec!["b", "a", "c"]);

        let ids: Vec<&str> = rank_servers(&servers, ServerPreference::Fullest, 1)
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(ids, vec!["c", "a", "b"]);
    }
}

/// `pick_server` contra o mock: ordenação vinda da API, filtro de região e o
/// fallback quando a região pedida não aparece.
#[cfg(test)]
mod server_pick_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server, mount_csrf};
    use wiremock::matchers::{body_partial_json, header, method, path, query_param};
    use wiremock::{Mock, ResponseTemplate};

    const TEMPLATE: &str = "<city>, <countryCode>";

    fn server_json(id: &str, playing: i32, max_players: i32) -> serde_json::Value {
        serde_json::json!({ "id": id, "playing": playing, "maxPlayers": max_players })
    }

    /// A lista de servidores daquele place, casada pelo `sortOrder` pedido.
    async fn mount_servers(place_id: i64, sort_order: &str, data: Vec<serde_json::Value>) {
        Mock::given(method("GET"))
            .and(path(mock_path(
                "games",
                &format!("/v1/games/{}/servers/Public", place_id),
            )))
            .and(query_param("sortOrder", sort_order))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": data,
                "nextPageCursor": null
            })))
            .mount(mock_server().await)
            .await;
    }

    async fn mount_region(token: &str, job_id: &str, ip: &str, country_code: &str) {
        Mock::given(method("POST"))
            .and(path(mock_path("gamejoin", "/v1/join-game-instance")))
            .and(header("cookie", cookie_of(token)))
            .and(body_partial_json(serde_json::json!({ "gameId": job_id })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "joinScript": { "MachineAddress": ip }
            })))
            .mount(mock_server().await)
            .await;

        Mock::given(method("GET"))
            .and(path(format!("/ipwhois/{}", ip)))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "city": "Cidade",
                "country_code": country_code
            })))
            .mount(mock_server().await)
            .await;
    }

    #[tokio::test]
    async fn the_emptiest_preference_picks_the_server_with_fewest_players() {
        mount_csrf("pick-empty", "csrf-pick-empty").await;
        mount_servers(
            7001,
            "Asc",
            vec![server_json("cheio", 25, 30), server_json("vazio", 3, 30)],
        )
        .await;

        let picked = pick_server(
            "pick-empty",
            7001,
            ServerPreference::Emptiest,
            2,
            "",
            TEMPLATE,
            10,
        )
        .await
        .expect("servidor");
        assert_eq!(picked.job_id, "vazio");
        assert_eq!(picked.playing, 3);
        assert!(!picked.region_fallback);
        assert!(picked.region.is_none(), "sem filtro, nenhuma região é resolvida");
    }

    #[tokio::test]
    async fn the_fullest_preference_asks_the_api_for_the_descending_order() {
        mount_csrf("pick-full", "csrf-pick-full").await;
        mount_servers(
            7002,
            "Desc",
            vec![server_json("quase-cheio", 27, 30), server_json("vazio", 1, 30)],
        )
        .await;

        let picked = pick_server("pick-full", 7002, ServerPreference::Fullest, 1, "", TEMPLATE, 10)
            .await
            .expect("servidor");
        assert_eq!(picked.job_id, "quase-cheio");
    }

    /// O caso do usuário: garantir um servidor brasileiro. Percorre na ordem da
    /// preferência e para no primeiro BR.
    #[tokio::test]
    async fn the_region_filter_walks_the_list_until_it_matches() {
        mount_csrf("pick-br", "csrf-pick-br").await;
        mount_servers(
            7003,
            "Asc",
            vec![server_json("us", 1, 30), server_json("br", 4, 30)],
        )
        .await;
        mount_region("pick-br", "us", "1.0.0.1", "US").await;
        mount_region("pick-br", "br", "1.0.0.2", "BR").await;

        let picked = pick_server(
            "pick-br",
            7003,
            ServerPreference::Emptiest,
            1,
            "BR",
            TEMPLATE,
            10,
        )
        .await
        .expect("servidor");
        assert_eq!(picked.job_id, "br");
        assert_eq!(picked.region.as_ref().unwrap().region.as_ref().unwrap().country_code, "BR");
        assert!(!picked.region_fallback);
    }

    /// Nenhum servidor na região: devolve o melhor disponível marcado, para a
    /// UI perguntar em vez de entrar escondido no lugar errado.
    #[tokio::test]
    async fn no_server_in_the_region_comes_back_flagged_as_a_fallback() {
        mount_csrf("pick-nobr", "csrf-pick-nobr").await;
        mount_servers(7004, "Asc", vec![server_json("us-1", 2, 30)]).await;
        mount_region("pick-nobr", "us-1", "1.0.0.3", "US").await;

        let picked = pick_server(
            "pick-nobr",
            7004,
            ServerPreference::Emptiest,
            1,
            "BR",
            TEMPLATE,
            5,
        )
        .await
        .expect("servidor");
        assert_eq!(picked.job_id, "us-1");
        assert!(picked.region_fallback);
    }

    /// Num jogo popular a primeira página de `Desc` é toda de servidores quase
    /// lotados. O "melhor encaixe" tem que descer as páginas até a faixa que
    /// cabe o lote, em vez de desistir ou cair nos vazios.
    #[tokio::test]
    async fn best_fit_follows_the_cursor_until_a_server_fits_the_batch() {
        mount_csrf("pick-bestfit", "csrf-pick-bestfit").await;
        let server = mock_server().await;
        let route = mock_path("games", "/v1/games/7006/servers/Public");

        // Página 1: todos sem espaço para 4 contas com folga.
        Mock::given(method("GET"))
            .and(path(route.clone()))
            .and(query_param("sortOrder", "Desc"))
            .and(|req: &wiremock::Request| !req.url.query_pairs().any(|(k, _)| k == "cursor"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [server_json("quase-cheio", 11, 12), server_json("sem-folga", 9, 12)],
                "nextPageCursor": "pagina-2"
            })))
            .mount(server)
            .await;

        // Página 2: o servidor com 7, que é o que o usuário quer.
        Mock::given(method("GET"))
            .and(path(route.clone()))
            .and(query_param("cursor", "pagina-2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [server_json("ideal", 7, 12), server_json("vazio", 1, 12)],
                "nextPageCursor": serde_json::Value::Null
            })))
            .mount(server)
            .await;

        let picked = pick_server(
            "pick-bestfit",
            7006,
            ServerPreference::BestFit,
            4,
            "",
            TEMPLATE,
            5,
        )
        .await
        .expect("servidor");
        assert_eq!(picked.job_id, "ideal");
        assert_eq!(picked.playing, 7);
    }

    #[tokio::test]
    async fn a_place_without_room_is_an_error_instead_of_a_silent_launch() {
        mount_csrf("pick-vazio", "csrf-pick-vazio").await;
        mount_servers(7005, "Asc", vec![server_json("cheio", 30, 30)]).await;

        let err = pick_server("pick-vazio", 7005, ServerPreference::Emptiest, 1, "", TEMPLATE, 5)
            .await
            .unwrap_err();
        assert_eq!(err, "No public server with room was found");
    }
}
