// Jogos recentes **de cada conta**, para a aba Recentes da Escolher jogo. É o
// "Continuar" da página inicial do Roblox: a lista de jogos em que a conta
// entrou por último, venha de onde vier (app, site, celular).
//
// Medido em 13/10/2026 com contas de teste: `POST apis/discovery-api/
// omni-recommendation` com `{"pageType":"Home"}` devolve várias faixas; a do
// "Continuar" tem `topicId` 100000003 (o `topic` vem traduzido: "Continue",
// "Continuar"...). Cada item é `{"contentType":"Game","contentId":<universeId>}`.

/// `topicId` da faixa "Continuar" da página inicial.
pub const CONTINUE_SORT_TOPIC_ID: i64 = 100_000_003;

/// Os universos da faixa "Continuar", na ordem do Roblox (o mais recente
/// primeiro), sem repetir. Resposta sem a faixa = lista vazia.
pub fn parse_continue_universes(body: &serde_json::Value) -> Vec<i64> {
    let mut seen = std::collections::HashSet::new();
    body["sorts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|sort| sort["topicId"].as_i64() == Some(CONTINUE_SORT_TOPIC_ID))
        .flat_map(|sort| sort["recommendationList"].as_array().cloned().unwrap_or_default())
        .filter(|item| item["contentType"].as_str() == Some("Game"))
        .filter_map(|item| item["contentId"].as_i64())
        .filter(|id| *id > 0 && seen.insert(*id))
        .collect()
}

/// Identificador de sessão que a página inicial manda (o Roblox não confere o
/// formato além de ser um UUID).
fn home_session_id() -> String {
    let mut bytes = [0u8; 16];
    let _ = getrandom::fill(&mut bytes);
    let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

/// O "Continuar" da conta do cookie. Só leitura: nada de refresh de sessão.
pub async fn get_continue_universes(security_token: &str) -> Result<Vec<i64>, String> {
    let csrf = crate::api::auth::get_csrf_token(security_token).await?;
    let client = http_client::client();
    let request = client
        .post(format!("{}/discovery-api/omni-recommendation", endpoints::host("apis")))
        .header(COOKIE, cookie_header(security_token))
        .json(&serde_json::json!({ "pageType": "Home", "sessionId": home_session_id() }));
    let response = crate::api::auth::send_with_csrf_retry(request, &csrf).await?;
    if !response.status().is_success() {
        return Err(format!("Failed to read recent games (status {})", response.status().as_u16()));
    }
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse recent games: {}", e))?;
    Ok(parse_continue_universes(&body))
}

/// Nome e lugar de entrada de um universo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UniverseSummary {
    #[serde(rename = "id")]
    pub universe_id: i64,
    #[serde(default)]
    pub root_place_id: i64,
    #[serde(default)]
    pub name: String,
}

/// Nome e `rootPlaceId` de vários universos (`GET games/v1/games`, sem
/// cookie), em lotes de 50. Universo que não vier fica de fora.
pub async fn get_universe_summaries(universe_ids: &[i64]) -> Result<Vec<UniverseSummary>, String> {
    let client = http_client::client();
    let mut out = Vec::new();
    for chunk in universe_ids.chunks(50) {
        let ids = chunk.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        let url = format!("{}/v1/games?universeIds={}", endpoints::host("games"), ids);
        let response = send_with_retry(|| client.get(&url)).await?;
        if !response.status().is_success() {
            return Err(format!("Failed to read the games (status {})", response.status().as_u16()));
        }
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse the games: {}", e))?;
        out.extend(
            body["data"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|item| serde_json::from_value::<UniverseSummary>(item.clone()).ok())
                .filter(|game| game.root_place_id > 0),
        );
    }
    Ok(out)
}

#[cfg(test)]
mod recent_games_parse_tests {
    use super::*;

    #[test]
    fn only_the_continue_sort_counts_in_its_order() {
        let body = serde_json::json!({
            "sorts": [
                { "topic": "Recommended For You", "topicId": 100000000, "recommendationList": [
                    { "contentType": "Game", "contentId": 1 }
                ]},
                { "topic": "Continuar", "topicId": 100000003, "recommendationList": [
                    { "contentType": "Game", "contentId": 30 },
                    { "contentType": "Game", "contentId": 10 },
                    { "contentType": "CatalogAsset", "contentId": 99 },
                    { "contentType": "Game", "contentId": 30 }
                ]}
            ]
        });
        assert_eq!(parse_continue_universes(&body), vec![30, 10]);
    }

    #[test]
    fn no_continue_sort_is_an_empty_list() {
        assert!(parse_continue_universes(&serde_json::json!({ "sorts": [] })).is_empty());
        assert!(parse_continue_universes(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn the_session_id_looks_like_a_uuid() {
        let id = home_session_id();
        assert_eq!(id.len(), 36);
        assert_eq!(id.matches('-').count(), 4);
    }
}

#[cfg(test)]
mod recent_games_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server, mount_csrf};
    use wiremock::matchers::{body_partial_json, header, method, path, query_param};
    use wiremock::{Mock, ResponseTemplate};

    #[tokio::test]
    async fn the_continue_sort_is_read_with_the_accounts_cookie() {
        let server = mock_server().await;
        mount_csrf("recent-ok", "csrf-recent-ok").await;
        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/discovery-api/omni-recommendation")))
            .and(header("cookie", cookie_of("recent-ok")))
            .and(header("x-csrf-token", "csrf-recent-ok"))
            .and(body_partial_json(serde_json::json!({ "pageType": "Home" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "sorts": [{ "topicId": 100000003, "recommendationList": [
                    { "contentType": "Game", "contentId": 7326934954_i64 }
                ]}]
            })))
            .mount(server)
            .await;

        assert_eq!(get_continue_universes("recent-ok").await.unwrap(), vec![7_326_934_954]);
    }

    #[tokio::test]
    async fn a_refused_read_reports_the_status() {
        let server = mock_server().await;
        mount_csrf("recent-bad", "csrf-recent-bad").await;
        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/discovery-api/omni-recommendation")))
            .and(header("cookie", cookie_of("recent-bad")))
            .respond_with(ResponseTemplate::new(401))
            .mount(server)
            .await;

        assert_eq!(
            get_continue_universes("recent-bad").await.unwrap_err(),
            "Failed to read recent games (status 401)"
        );
    }

    #[tokio::test]
    async fn universes_become_names_and_root_places() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("games", "/v1/games")))
            .and(query_param("universeIds", "551,552"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    { "id": 551, "rootPlaceId": 79546208627805_i64, "name": "99 Nights" },
                    { "id": 552, "rootPlaceId": 0, "name": "no place" }
                ]
            })))
            .mount(server)
            .await;

        let games = get_universe_summaries(&[551, 552]).await.unwrap();
        assert_eq!(
            games,
            vec![UniverseSummary { universe_id: 551, root_place_id: 79_546_208_627_805, name: "99 Nights".into() }]
        );
    }
}
