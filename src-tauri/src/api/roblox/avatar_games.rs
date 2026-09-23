pub async fn set_avatar(security_token: &str, avatar_json: serde_json::Value) -> Result<Vec<i64>, String> {
    let csrf = crate::api::auth::get_csrf_token(security_token).await?;
    let client = reqwest::Client::new();
    let mut invalid_assets = Vec::new();

    if let Some(avatar_type) = avatar_json.get("playerAvatarType") {
        client
            .post(format!("{}/v1/avatar/set-player-avatar-type", endpoints::host("avatar")))
            .header(COOKIE, cookie_header(security_token))
            .header("X-CSRF-TOKEN", &csrf)
            .json(&serde_json::json!({ "playerAvatarType": avatar_type }))
            .send()
            .await
            .map_err(|e| format!("Failed to set avatar type: {}", e))?;
    }

    let scales = avatar_json.get("scales").or_else(|| avatar_json.get("scale"));
    if let Some(scale_obj) = scales {
        client
            .post(format!("{}/v1/avatar/set-scales", endpoints::host("avatar")))
            .header(COOKIE, cookie_header(security_token))
            .header("X-CSRF-TOKEN", &csrf)
            .json(scale_obj)
            .send()
            .await
            .map_err(|e| format!("Failed to set scales: {}", e))?;
    }

    if let Some(body_colors) = avatar_json.get("bodyColors") {
        client
            .post(format!("{}/v1/avatar/set-body-colors", endpoints::host("avatar")))
            .header(COOKIE, cookie_header(security_token))
            .header("X-CSRF-TOKEN", &csrf)
            .json(body_colors)
            .send()
            .await
            .map_err(|e| format!("Failed to set body colors: {}", e))?;
    }

    if let Some(assets) = avatar_json.get("assets") {
        let response = client
            .post(format!("{}/v2/avatar/set-wearing-assets", endpoints::host("avatar")))
            .header(COOKIE, cookie_header(security_token))
            .header("X-CSRF-TOKEN", &csrf)
            .json(&serde_json::json!({ "assets": assets }))
            .send()
            .await
            .map_err(|e| format!("Failed to set wearing assets: {}", e))?;

        if response.status().is_success() {
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
    }

    Ok(invalid_assets)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutfitInfo {
    pub id: i64,
    pub name: String,
}

pub async fn get_outfits(user_id: i64) -> Result<Vec<OutfitInfo>, String> {
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/v1/users/{}/outfits?page=1&itemsPerPage=50", endpoints::host("avatar"), user_id))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

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
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/v1/outfits/{}/details", endpoints::host("avatar"), outfit_id))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

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

    let client = reqwest::Client::new();
    let mut all_details = Vec::new();

    for chunk in place_ids.chunks(50) {
        let query: String = chunk.iter().map(|id| format!("placeIds={}", id)).collect::<Vec<_>>().join("&");

        let mut request = client.get(format!("{}/v1/games/multiget-place-details?{}", endpoints::host("games"), query));

        if let Some(token) = security_token {
            request = request.header(COOKIE, cookie_header(token));
        }

        let response = request.send().await.map_err(|e| format!("Request failed: {}", e))?;

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
    let client = reqwest::Client::new();
    let limit = if server_type == "VIP" { 25 } else { 100 };
    let mut url = format!(
        "{}/v1/games/{}/servers/{}?sortOrder=Asc&limit={}",
        endpoints::host("games"),
        place_id, server_type, limit
    );

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

    let response = client
        .post(format!("{}/v1/join-game-instance", endpoints::host("gamejoin")))
        .header(COOKIE, cookie_header(security_token))
        .header("X-CSRF-TOKEN", &csrf)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(format!("Failed to join game instance: {}", text));
    }

    response.json().await.map_err(|e| format!("Failed to parse join response: {}", e))
}

pub async fn join_game(security_token: &str, place_id: i64) -> Result<serde_json::Value, String> {
    let csrf = crate::api::auth::get_csrf_token(security_token).await?;
    let client = game_join_client();

    let response = client
        .post(format!("{}/v1/join-game", endpoints::host("gamejoin")))
        .header(COOKIE, cookie_header(security_token))
        .header("X-CSRF-TOKEN", &csrf)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "placeId": place_id }))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    response.json().await.map_err(|e| format!("Failed to parse join response: {}", e))
}

pub async fn search_games(security_token: Option<&str>, keyword: &str, _start: i32) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();
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
    let client = reqwest::Client::new();
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

        let response = request.send().await.map_err(|e| format!("Request failed: {}", e))?;

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
