pub async fn set_avatar(security_token: &str, avatar_json: serde_json::Value) -> Result<Vec<i64>, String> {
    let csrf = crate::api::auth::get_csrf_token(security_token).await?;
    let client = http_client::client();
    let mut invalid_assets = Vec::new();

    if let Some(avatar_type) = avatar_json.get("playerAvatarType") {
        let request = client
            .post(format!("{}/v1/avatar/set-player-avatar-type", endpoints::host("avatar")))
            .header(COOKIE, cookie_header(security_token))
            .json(&serde_json::json!({ "playerAvatarType": avatar_type }));
        crate::api::auth::send_with_csrf_retry(request, &csrf).await?;
    }

    let scales = avatar_json.get("scales").or_else(|| avatar_json.get("scale"));
    if let Some(scale_obj) = scales {
        let request = client
            .post(format!("{}/v1/avatar/set-scales", endpoints::host("avatar")))
            .header(COOKIE, cookie_header(security_token))
            .json(scale_obj);
        crate::api::auth::send_with_csrf_retry(request, &csrf).await?;
    }

    if let Some(body_colors) = avatar_json.get("bodyColors") {
        let request = client
            .post(format!("{}/v1/avatar/set-body-colors", endpoints::host("avatar")))
            .header(COOKIE, cookie_header(security_token))
            .json(body_colors);
        crate::api::auth::send_with_csrf_retry(request, &csrf).await?;
    }

    if let Some(assets) = avatar_json.get("assets") {
        let request = client
            .post(format!("{}/v2/avatar/set-wearing-assets", endpoints::host("avatar")))
            .header(COOKIE, cookie_header(security_token))
            .json(&serde_json::json!({ "assets": assets }));
        let response = crate::api::auth::send_with_csrf_retry(request, &csrf).await?;

        // Recusa do Roblox é erro: devolver lista vazia fazia quem chamou
        // anunciar "aplicado" com o avatar intacto.
        if !response.status().is_success() {
            return Err(format!("Failed to wear assets (status {})", response.status().as_u16()));
        }
        if let Ok(body) = response.json::<serde_json::Value>().await {
            if let Some(ids) = body.get("invalidAssetIds").and_then(|v| v.as_array()) {
                for id in ids {
                    if let Some(n) = id.as_i64() {
                        invalid_assets.push(n);
                    }
                }
            }
        }
    }

    Ok(invalid_assets)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutfitInfo {
    pub id: i64,
    pub name: String,
}

pub async fn get_outfits(user_id: i64) -> Result<Vec<OutfitInfo>, String> {
    let client = http_client::client();

    let response = client
        .get(format!("{}/v1/users/{}/outfits?page=1&itemsPerPage=50", endpoints::host("avatar"), user_id))
        .send()
        .await
        .map_err(|e| http_client::describe_error(&e))?;

    if !response.status().is_success() {
        return Err(format!("Failed to get outfits (status {})", response.status().as_u16()));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse outfits: {}", e))?;

    Ok(body["data"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect())
        .unwrap_or_default())
}

pub async fn get_outfit_details(outfit_id: i64) -> Result<serde_json::Value, String> {
    let client = http_client::client();

    let response = client
        .get(format!("{}/v1/outfits/{}/details", endpoints::host("avatar"), outfit_id))
        .send()
        .await
        .map_err(|e| http_client::describe_error(&e))?;

    if !response.status().is_success() {
        return Err(format!("Failed to get outfit details (status {})", response.status().as_u16()));
    }

    response.json().await.map_err(|e| format!("Failed to parse outfit details: {}", e))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceDetails {
    #[serde(rename = "placeId")]
    pub place_id: i64,
    #[serde(rename = "universeId")]
    pub universe_id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "sourceName", default)]
    pub source_name: String,
    #[serde(rename = "sourceDescription", default)]
    pub source_description: String,
    #[serde(default)]
    pub url: String,
    #[serde(rename = "reasonProhibited", default)]
    pub reason_prohibited: String,
}

pub async fn get_place_details(place_ids: &[i64], security_token: Option<&str>) -> Result<Vec<PlaceDetails>, String> {
    if place_ids.is_empty() {
        return Ok(Vec::new());
    }

    let client = http_client::client();
    let mut all_details = Vec::new();

    for chunk in place_ids.chunks(50) {
        let query: String = chunk.iter().map(|id| format!("placeIds={}", id)).collect::<Vec<_>>().join("&");

        let mut request = client.get(format!("{}/v1/games/multiget-place-details?{}", endpoints::host("games"), query));

        if let Some(token) = security_token {
            request = request.header(COOKIE, cookie_header(token));
        }

        let response = request.send().await.map_err(|e| http_client::describe_error(&e))?;

        if response.status().is_success() {
            let details: Vec<PlaceDetails> = response.json().await.map_err(|e| format!("Failed to parse: {}", e))?;
            all_details.extend(details);
        }
    }

    Ok(all_details)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerData {
    pub id: String,
    #[serde(rename = "maxPlayers")]
    pub max_players: i32,
    pub playing: i32,
    #[serde(rename = "playerTokens", default)]
    pub player_tokens: Vec<String>,
    #[serde(default)]
    pub fps: f64,
    #[serde(default)]
    pub ping: Option<i64>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "vipServerId", default)]
    pub vip_server_id: Option<i64>,
    #[serde(rename = "accessCode", default)]
    pub access_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServersResponse {
    pub data: Vec<ServerData>,
    #[serde(rename = "nextPageCursor")]
    pub next_page_cursor: Option<String>,
}

pub async fn get_servers(
    place_id: i64,
    server_type: &str,
    cursor: Option<&str>,
    security_token: Option<&str>,
) -> Result<ServersResponse, String> {
    get_servers_page(place_id, server_type, cursor, security_token, "Asc", false).await
}

/// A lista de servidores com a ordem e o filtro de lotados explícitos.
///
/// A ordem vem da **API**, não de um `sort` local: a resposta traz no máximo
/// 100 servidores por página, e um jogo grande tem milhares. Reordenar a
/// primeira página de `sortOrder=Asc` mostra "os mais cheios entre os mais
/// vazios" — que foi exatamente o bug de a aba Servers mostrar 3/13 em tudo
/// com o filtro "Fullest" ligado.
///
/// `Asc` = menos jogadores primeiro, `Desc` = mais (confirmado contra a API).
pub async fn get_servers_page(
    place_id: i64,
    server_type: &str,
    cursor: Option<&str>,
    security_token: Option<&str>,
    sort_order: &str,
    exclude_full: bool,
) -> Result<ServersResponse, String> {
    let client = http_client::client();
    let limit = if server_type == "VIP" { 25 } else { 100 };
    let sort_order = if sort_order.eq_ignore_ascii_case("desc") { "Desc" } else { "Asc" };
    let mut url = format!(
        "{}/v1/games/{}/servers/{}?sortOrder={}&limit={}",
        endpoints::host("games"),
        place_id, server_type, sort_order, limit
    );

    if exclude_full {
        url.push_str("&excludeFullGames=true");
    }
    if let Some(c) = cursor {
        url.push_str(&format!("&cursor={}", c));
    }

    let response = send_with_retry(|| {
        let mut request = client.get(&url);
        if let Some(token) = security_token {
            request = request.header(COOKIE, cookie_header(token));
        }
        if server_type == "VIP" {
            request = request.header("Accept", "application/json");
        }
        request
    })
    .await?;

    if !response.status().is_success() {
        return Err(format!("Failed to get servers (status {})", response.status().as_u16()));
    }

    response.json().await.map_err(|e| format!("Failed to parse servers: {}", e))
}

pub async fn join_game_instance(
    security_token: &str,
    place_id: i64,
    game_id: &str,
    is_teleport: bool,
) -> Result<serde_json::Value, String> {
    let csrf = crate::api::auth::get_csrf_token(security_token).await?;
    let client = game_join_client();

    let mut body = serde_json::json!({
        "gameId": game_id,
        "placeId": place_id,
    });

    if is_teleport {
        body["isTeleport"] = serde_json::json!(true);
    }

    let request = client
        .post(format!("{}/v1/join-game-instance", endpoints::host("gamejoin")))
        .header(COOKIE, cookie_header(security_token))
        .header("Content-Type", "application/json")
        .json(&body);
    let response = crate::api::auth::send_with_csrf_retry(request, &csrf).await?;

    if !response.status().is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(format!("Failed to join game instance: {}", text));
    }

    response.json().await.map_err(|e| format!("Failed to parse join response: {}", e))
}

/// Requests a join for a specific game instance.
///
/// Note: the response body is only trusted on a success status — Roblox
/// answers 403 with an error payload that would otherwise parse as a join.
pub async fn join_game(security_token: &str, place_id: i64) -> Result<serde_json::Value, String> {
    let csrf = crate::api::auth::get_csrf_token(security_token).await?;
    let client = game_join_client();

    let request = client
        .post(format!("{}/v1/join-game", endpoints::host("gamejoin")))
        .header(COOKIE, cookie_header(security_token))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "placeId": place_id }));
    let response = crate::api::auth::send_with_csrf_retry(request, &csrf).await?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Failed to join game (status {}) {}",
            status,
            body.chars().take(200).collect::<String>()
        ));
    }

    response.json().await.map_err(|e| format!("Failed to parse join response: {}", e))
}

pub async fn search_games(security_token: Option<&str>, keyword: &str, _start: i32) -> Result<serde_json::Value, String> {
    let client = http_client::client();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let session_id = format!("{:x}", now);

    if keyword.is_empty() {
        let url = format!(
            "{}/explore-api/v1/get-sorts?sessionId={}",
            endpoints::host("apis"),
            session_id
        );
        let response = send_with_retry(|| {
            let mut req = client.get(&url);
            if let Some(token) = security_token {
                req = req.header(COOKIE, cookie_header(token));
            }
            req
        })
        .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Failed to get games (status {}) {}", status, body.chars().take(200).collect::<String>()));
        }

        return response.json().await.map_err(|e| format!("Failed to parse: {}", e));
    }

    let url = format!(
        "{}/search-api/omni-search?searchQuery={}&sessionId={}",
        endpoints::host("apis"),
        urlencoding::encode(keyword),
        session_id
    );
    let response = send_with_retry(|| {
        let mut req = client.get(&url);
        if let Some(token) = security_token {
            req = req.header(COOKIE, cookie_header(token));
        }
        req
    })
    .await?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Failed to search games (status {}) {}", status, body.chars().take(200).collect::<String>()));
    }

    response.json().await.map_err(|e| format!("Failed to parse: {}", e))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniversePlace {
    pub id: i64,
    #[serde(default)]
    pub name: String,
}

pub async fn get_universe_places(universe_id: i64, security_token: Option<&str>) -> Result<Vec<UniversePlace>, String> {
    let client = http_client::client();
    let mut all_places = Vec::new();
    let mut cursor = String::new();

    loop {
        let url = if cursor.is_empty() {
            format!(
                "{}/v1/universes/{}/places?sortOrder=Asc&limit=100",
                endpoints::host("develop"),
                universe_id
            )
        } else {
            format!(
                "{}/v1/universes/{}/places?sortOrder=Asc&limit=100&cursor={}",
                endpoints::host("develop"),
                universe_id, cursor
            )
        };

        let mut request = client.get(&url);

        if let Some(token) = security_token {
            request = request.header(COOKIE, cookie_header(token));
        }

        let response = request.send().await.map_err(|e| http_client::describe_error(&e))?;

        if !response.status().is_success() {
            break;
        }

        let body: serde_json::Value = response.json().await.map_err(|e| format!("Failed to parse: {}", e))?;

        if let Some(data) = body["data"].as_array() {
            for place in data {
                if let Ok(p) = serde_json::from_value::<UniversePlace>(place.clone()) {
                    all_places.push(p);
                }
            }
        }

        match body["nextPageCursor"].as_str() {
            Some(c) if !c.is_empty() => cursor = c.to_string(),
            _ => break,
        }
    }

    Ok(all_places)
}

#[cfg(test)]
mod avatar_games_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{mock_path, mock_server};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};

    #[tokio::test]
    async fn lists_outfits() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("avatar", "/v1/users/5150/outfits")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    { "id": 1, "name": "Outfit A" },
                    { "id": 2, "name": "Outfit B" }
                ]
            })))
            .mount(server)
            .await;

        let outfits = get_outfits(5150).await.expect("outfits");
        assert_eq!(outfits.len(), 2);
        assert_eq!(outfits[0].name, "Outfit A");
        assert_eq!(outfits[1].id, 2);
    }

    #[tokio::test]
    async fn lists_public_servers() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path(
                "games",
                "/v1/games/606849621/servers/Public",
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{
                    "id": "job-1",
                    "maxPlayers": 12,
                    "playing": 7,
                    "playerTokens": [],
                    "fps": 59.5,
                    "ping": 42
                }],
                "nextPageCursor": "cursor-2"
            })))
            .mount(server)
            .await;

        let servers = get_servers(606_849_621, "Public", None, None)
            .await
            .expect("servers");
        assert_eq!(servers.next_page_cursor.as_deref(), Some("cursor-2"));
        assert_eq!(servers.data.len(), 1);
        assert_eq!(servers.data[0].id, "job-1");
        assert_eq!(servers.data[0].playing, 7);
    }
}

/// Avatar, place, server-list and game-search calls that
/// `avatar_games_http_tests` does not reach: chunking, cursor paging, the VIP
/// server-list variant and the error branches.
#[cfg(test)]
mod avatar_games_extra_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server, mount_csrf};
    use wiremock::matchers::{body_string_contains, header, method, path, query_param};
    use wiremock::{Mock, ResponseTemplate};

    /// `set_avatar` fans one payload out over four endpoints and reports the
    /// asset ids Roblox refused.
    #[tokio::test]
    async fn set_avatar_applies_every_section_and_returns_invalid_assets() {
        let server = mock_server().await;
        mount_csrf("avatar-set", "csrf-avatar-set").await;

        for endpoint in [
            "/v1/avatar/set-player-avatar-type",
            "/v1/avatar/set-scales",
            "/v1/avatar/set-body-colors",
        ] {
            Mock::given(method("POST"))
                .and(path(mock_path("avatar", endpoint)))
                .and(header("cookie", cookie_of("avatar-set")))
                .and(header("x-csrf-token", "csrf-avatar-set"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
                .expect(1)
                .mount(server)
                .await;
        }

        Mock::given(method("POST"))
            .and(path(mock_path("avatar", "/v2/avatar/set-wearing-assets")))
            .and(header("cookie", cookie_of("avatar-set")))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({ "invalidAssetIds": [1234, 5678, "not a number"] }),
            ))
            .mount(server)
            .await;

        let invalid = set_avatar(
            "avatar-set",
            serde_json::json!({
                "playerAvatarType": "R15",
                "scales": { "height": 1.0 },
                "bodyColors": { "headColorId": 1 },
                "assets": [{ "id": 1234 }]
            }),
        )
        .await
        .expect("set avatar");
        assert_eq!(invalid, vec![1234, 5678]);
    }

    /// A payload with nothing in it must not call anything and returns no
    /// invalid assets.
    #[tokio::test]
    async fn set_avatar_with_an_empty_payload_touches_only_the_csrf_call() {
        mount_csrf("avatar-empty", "csrf-avatar-empty").await;

        let invalid = set_avatar("avatar-empty", serde_json::json!({}))
            .await
            .expect("set avatar");
        assert!(invalid.is_empty());
    }

    /// Wearing refused by Roblox is an error, not an empty list of invalid
    /// assets: the caller would report "applied" for an avatar that never changed.
    #[tokio::test]
    async fn set_avatar_reports_a_refused_wearing_call() {
        let server = mock_server().await;
        mount_csrf("avatar-wear-refused", "csrf-avatar-wear-refused").await;

        Mock::given(method("POST"))
            .and(path(mock_path("avatar", "/v2/avatar/set-wearing-assets")))
            .and(header("cookie", cookie_of("avatar-wear-refused")))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "errors": [{ "code": 0, "message": "BadRequest" }]
            })))
            .expect(1)
            .mount(server)
            .await;

        let error = set_avatar(
            "avatar-wear-refused",
            serde_json::json!({ "assets": [{ "id": 1234 }] }),
        )
        .await
        .expect_err("a refused wearing call must be an error");
        assert_eq!(error, "Failed to wear assets (status 400)");
    }

    /// The older payloads spell the scales key `scale`.
    #[tokio::test]
    async fn set_avatar_accepts_the_legacy_scale_key() {
        let server = mock_server().await;
        mount_csrf("avatar-scale", "csrf-avatar-scale").await;

        Mock::given(method("POST"))
            .and(path(mock_path("avatar", "/v1/avatar/set-scales")))
            .and(header("cookie", cookie_of("avatar-scale")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .expect(1)
            .mount(server)
            .await;

        let invalid = set_avatar(
            "avatar-scale",
            serde_json::json!({ "scale": { "height": 1.0 } }),
        )
        .await
        .expect("set avatar");
        assert!(invalid.is_empty());
    }

    #[tokio::test]
    async fn outfits_report_the_status() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("avatar", "/v1/users/5151/outfits")))
            .respond_with(ResponseTemplate::new(500))
            .mount(server)
            .await;

        assert_eq!(
            get_outfits(5151).await.unwrap_err(),
            "Failed to get outfits (status 500)"
        );
    }

    #[tokio::test]
    async fn a_payload_without_outfits_is_an_empty_list() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("avatar", "/v1/users/5152/outfits")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(server)
            .await;

        assert!(get_outfits(5152).await.expect("outfits").is_empty());
    }

    #[tokio::test]
    async fn outfit_details_are_returned_verbatim() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("avatar", "/v1/outfits/77/details")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "id": 77, "assets": [] })),
            )
            .mount(server)
            .await;

        let details = get_outfit_details(77).await.expect("details");
        assert_eq!(details["id"], serde_json::json!(77));
    }

    #[tokio::test]
    async fn outfit_details_report_the_status() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("avatar", "/v1/outfits/78/details")))
            .respond_with(ResponseTemplate::new(404))
            .mount(server)
            .await;

        assert_eq!(
            get_outfit_details(78).await.unwrap_err(),
            "Failed to get outfit details (status 404)"
        );
    }

    #[tokio::test]
    async fn place_details_short_circuit_on_an_empty_list() {
        // Nothing is mounted: an empty request must not touch the network.
        assert!(get_place_details(&[], None).await.expect("empty").is_empty());
    }

    #[tokio::test]
    async fn place_details_are_parsed_with_their_defaults() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("games", "/v1/games/multiget-place-details")))
            .and(query_param("placeIds", "4001"))
            .and(header("cookie", cookie_of("place-details")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "placeId": 4001, "universeId": 40, "name": "Place" }
            ])))
            .mount(server)
            .await;

        let details = get_place_details(&[4001], Some("place-details"))
            .await
            .expect("details");
        assert_eq!(details.len(), 1);
        assert_eq!(details[0].universe_id, 40);
        assert_eq!(details[0].name, "Place");
        // Absent strings default instead of failing the whole call.
        assert!(details[0].source_name.is_empty());
        assert!(details[0].reason_prohibited.is_empty());
    }

    /// A chunk that fails is skipped rather than failing the whole call.
    #[tokio::test]
    async fn a_failed_place_details_chunk_yields_no_entries() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("games", "/v1/games/multiget-place-details")))
            .and(query_param("placeIds", "4002"))
            .respond_with(ResponseTemplate::new(503))
            .mount(server)
            .await;

        assert!(get_place_details(&[4002], None)
            .await
            .expect("details")
            .is_empty());
    }

    /// VIP listings use a smaller page size and ask for JSON explicitly.
    #[tokio::test]
    async fn vip_server_listings_use_the_smaller_limit() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("games", "/v1/games/4100/servers/VIP")))
            .and(query_param("limit", "25"))
            .and(header("accept", "application/json"))
            .and(header("cookie", cookie_of("vip-servers")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{
                    "id": "vip-job",
                    "maxPlayers": 10,
                    "playing": 1,
                    "vipServerId": 99,
                    "accessCode": "code-99",
                    "name": "My server"
                }],
                "nextPageCursor": serde_json::Value::Null
            })))
            .mount(server)
            .await;

        let servers = get_servers(4100, "VIP", None, Some("vip-servers"))
            .await
            .expect("servers");
        assert!(servers.next_page_cursor.is_none());
        assert_eq!(servers.data[0].vip_server_id, Some(99));
        assert_eq!(servers.data[0].access_code.as_deref(), Some("code-99"));
        assert_eq!(servers.data[0].name.as_deref(), Some("My server"));
    }

    /// Regressão: a ordem e o filtro de lotados vão para a **API**. A resposta
    /// traz 100 servidores no máximo, então reordenar a primeira página de
    /// `Asc` mostra "os mais cheios entre os mais vazios" — foi o que fez a aba
    /// Servers exibir 3/13 em tudo com o filtro "Fullest" ligado.
    #[tokio::test]
    async fn the_sort_order_and_the_full_game_filter_go_to_the_api() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("games", "/v1/games/4200/servers/Public")))
            .and(query_param("sortOrder", "Desc"))
            .and(query_param("excludeFullGames", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "id": "cheio", "playing": 12, "maxPlayers": 13 }],
                "nextPageCursor": serde_json::Value::Null
            })))
            .mount(server)
            .await;

        let servers = get_servers_page(4200, "Public", None, None, "Desc", true)
            .await
            .expect("servers");
        assert_eq!(servers.data[0].id, "cheio");
        assert_eq!(servers.data[0].playing, 12);
    }

    /// `get_servers` continua pedindo `Asc` sem filtro — é o que o resto do app
    /// (webserver, shuffle) espera.
    #[tokio::test]
    async fn the_plain_listing_keeps_asking_for_the_ascending_order() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("games", "/v1/games/4201/servers/Public")))
            .and(query_param("sortOrder", "Asc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "id": "vazio", "playing": 1, "maxPlayers": 13 }],
                "nextPageCursor": serde_json::Value::Null
            })))
            .mount(server)
            .await;

        let servers = get_servers(4201, "Public", None, None).await.expect("servers");
        assert_eq!(servers.data[0].id, "vazio");
    }

    #[tokio::test]
    async fn a_cursor_is_appended_to_the_server_listing_url() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("games", "/v1/games/4101/servers/Public")))
            .and(query_param("cursor", "page-2"))
            .and(query_param("limit", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [],
                "nextPageCursor": serde_json::Value::Null
            })))
            .mount(server)
            .await;

        let servers = get_servers(4101, "Public", Some("page-2"), None)
            .await
            .expect("servers");
        assert!(servers.data.is_empty());
    }

    #[tokio::test]
    async fn server_listings_report_the_status() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("games", "/v1/games/4102/servers/Public")))
            .respond_with(ResponseTemplate::new(404))
            .mount(server)
            .await;

        assert_eq!(
            get_servers(4102, "Public", None, None).await.unwrap_err(),
            "Failed to get servers (status 404)"
        );
    }

    #[tokio::test]
    async fn a_malformed_server_listing_is_a_parse_error() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("games", "/v1/games/4103/servers/Public")))
            .respond_with(ResponseTemplate::new(200).set_body_raw("not json", "text/plain"))
            .mount(server)
            .await;

        let err = get_servers(4103, "Public", None, None).await.unwrap_err();
        assert!(
            err.starts_with("Failed to parse servers: "),
            "unexpected error: {}",
            err
        );
    }

    #[tokio::test]
    async fn joining_an_instance_sends_the_place_and_job() {
        let server = mock_server().await;
        mount_csrf("join-instance", "csrf-join-instance").await;

        Mock::given(method("POST"))
            .and(path(mock_path("gamejoin", "/v1/join-game-instance")))
            .and(header("cookie", cookie_of("join-instance")))
            .and(body_string_contains("job-42"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "status": 2, "joinScriptUrl": "x" })),
            )
            .mount(server)
            .await;

        let body = join_game_instance("join-instance", 4200, "job-42", false)
            .await
            .expect("join");
        assert_eq!(body["status"], serde_json::json!(2));
    }

    /// The teleport flag is only sent when asked for.
    #[tokio::test]
    async fn a_teleport_join_sets_the_flag() {
        let server = mock_server().await;
        mount_csrf("join-teleport", "csrf-join-teleport").await;

        Mock::given(method("POST"))
            .and(path(mock_path("gamejoin", "/v1/join-game-instance")))
            .and(header("cookie", cookie_of("join-teleport")))
            .and(body_string_contains("\"isTeleport\":true"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "status": 2 })),
            )
            .mount(server)
            .await;

        assert!(join_game_instance("join-teleport", 4201, "job-43", true)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn a_refused_instance_join_echoes_the_body() {
        let server = mock_server().await;
        mount_csrf("join-refused", "csrf-join-refused").await;

        Mock::given(method("POST"))
            .and(path(mock_path("gamejoin", "/v1/join-game-instance")))
            .and(header("cookie", cookie_of("join-refused")))
            .respond_with(ResponseTemplate::new(403).set_body_string("server full"))
            .mount(server)
            .await;

        assert_eq!(
            join_game_instance("join-refused", 4202, "job-44", false)
                .await
                .unwrap_err(),
            "Failed to join game instance: server full"
        );
    }

    #[tokio::test]
    async fn join_game_returns_the_payload() {
        let server = mock_server().await;
        mount_csrf("join-game", "csrf-join-game").await;

        Mock::given(method("POST"))
            .and(path(mock_path("gamejoin", "/v1/join-game")))
            .and(header("cookie", cookie_of("join-game")))
            .and(body_string_contains("4300"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "status": 2 })),
            )
            .mount(server)
            .await;

        let body = join_game("join-game", 4300).await.expect("join");
        assert_eq!(body["status"], serde_json::json!(2));
    }

    /// Documented behaviour, not necessarily desired: `join_game` parses the
    /// body whatever the status is, so an error payload comes back as `Ok`.
    #[tokio::test]
    async fn join_game_reports_an_error_status_instead_of_parsing_the_body() {
        let server = mock_server().await;
        mount_csrf("join-game-error", "csrf-join-game-error").await;

        Mock::given(method("POST"))
            .and(path(mock_path("gamejoin", "/v1/join-game")))
            .and(header("cookie", cookie_of("join-game-error")))
            .respond_with(ResponseTemplate::new(403).set_body_json(
                serde_json::json!({ "status": 12, "message": "not authorized" }),
            ))
            .mount(server)
            .await;

        // A 403 payload must never look like a successful join.
        let err = join_game("join-game-error", 4301).await.expect_err("refused");
        assert!(err.contains("403"), "{err}");
        assert!(err.contains("not authorized"), "{err}");
    }

    /// An empty keyword means "give me the front page sorts", not a search.
    #[tokio::test]
    async fn an_empty_keyword_asks_for_the_explore_sorts() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("apis", "/explore-api/v1/get-sorts")))
            .and(header("cookie", cookie_of("search-sorts")))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "sorts": [] })),
            )
            .mount(server)
            .await;

        let body = search_games(Some("search-sorts"), "", 0)
            .await
            .expect("sorts");
        assert!(body["sorts"].is_array());
    }

    #[tokio::test]
    async fn a_keyword_is_url_encoded_into_the_omni_search() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("apis", "/search-api/omni-search")))
            .and(query_param("searchQuery", "blox fruits"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "searchResults": [] })),
            )
            .mount(server)
            .await;

        let body = search_games(None, "blox fruits", 0).await.expect("search");
        assert!(body["searchResults"].is_array());
    }

    #[tokio::test]
    async fn a_failed_search_reports_status_and_body() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("apis", "/search-api/omni-search")))
            .and(query_param("searchQuery", "boom"))
            .respond_with(ResponseTemplate::new(500).set_body_string("upstream"))
            .mount(server)
            .await;

        let err = search_games(None, "boom", 0).await.unwrap_err();
        assert!(err.starts_with("Failed to search games (status 500)"), "{}", err);
        assert!(err.contains("upstream"), "{}", err);
    }

    #[tokio::test]
    async fn a_failed_sorts_call_reports_status_and_body() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("apis", "/explore-api/v1/get-sorts")))
            .and(header("cookie", cookie_of("sorts-fail")))
            .respond_with(ResponseTemplate::new(429).set_body_string("slow down"))
            .mount(server)
            .await;

        let err = search_games(Some("sorts-fail"), "", 0).await.unwrap_err();
        assert!(err.starts_with("Failed to get games (status 429)"), "{}", err);
    }

    /// Universe places are paged: the cursor of page one must be followed and
    /// an empty cursor ends the loop.
    #[tokio::test]
    async fn universe_places_follow_the_page_cursor() {
        let server = mock_server().await;

        Mock::given(method("GET"))
            .and(path(mock_path("develop", "/v1/universes/5000/places")))
            .and(query_param("cursor", "page-2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "id": 3, "name": "Third" }],
                "nextPageCursor": ""
            })))
            .mount(server)
            .await;

        Mock::given(method("GET"))
            .and(path(mock_path("develop", "/v1/universes/5000/places")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "id": 1, "name": "First" }, { "id": 2 }],
                "nextPageCursor": "page-2"
            })))
            .mount(server)
            .await;

        let places = get_universe_places(5000, None).await.expect("places");
        assert_eq!(places.len(), 3);
        assert_eq!(places[0].name, "First");
        // A place without a name keeps the default instead of being dropped.
        assert_eq!(places[1].id, 2);
        assert!(places[1].name.is_empty());
        assert_eq!(places[2].id, 3);
    }

    /// A failing page stops the loop and keeps whatever was collected.
    #[tokio::test]
    async fn universe_places_stop_on_a_failing_page() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("develop", "/v1/universes/5001/places")))
            .and(header("cookie", cookie_of("universe-fail")))
            .respond_with(ResponseTemplate::new(403))
            .mount(server)
            .await;

        assert!(get_universe_places(5001, Some("universe-fail"))
            .await
            .expect("places")
            .is_empty());
    }
}
