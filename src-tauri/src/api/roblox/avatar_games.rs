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
        .send_noting()
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
        .send_noting()
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

        let response = request.send_noting().await.map_err(|e| http_client::describe_error(&e))?;

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

/// Dono de um servidor privado, como a API devolve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivateServerOwner {
    pub id: i64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
}

/// Um servidor privado que a conta pode acessar.
///
/// Diferente de [`ServerData`]: servidor privado sem ninguém dentro vem com
/// `id` (Job ID) nulo e sem `playing`, e o que importa para entrar é o
/// `accessCode`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivateServerData {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "vipServerId", default)]
    pub vip_server_id: Option<i64>,
    #[serde(rename = "accessCode", default)]
    pub access_code: Option<String>,
    #[serde(rename = "maxPlayers", default)]
    pub max_players: i32,
    #[serde(default)]
    pub playing: i32,
    #[serde(default)]
    pub owner: Option<PrivateServerOwner>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivateServersResponse {
    #[serde(default)]
    pub data: Vec<PrivateServerData>,
    #[serde(rename = "nextPageCursor", default)]
    pub next_page_cursor: Option<String>,
}

/// Servidores privados de um place que **esta conta** pode acessar
/// (`GET games/v1/games/{place}/private-servers`, com o cookie dela).
///
/// Só leitura: quem chama passa o cookie sem `run_with_session_retry` (o
/// refresh derrubaria as sessões abertas da conta).
pub async fn get_private_servers(
    place_id: i64,
    cursor: Option<&str>,
    security_token: &str,
) -> Result<PrivateServersResponse, String> {
    let client = http_client::client();
    let mut url = format!(
        "{}/v1/games/{}/private-servers?sortOrder=Asc&limit=25",
        endpoints::host("games"),
        place_id
    );
    if let Some(c) = cursor.filter(|c| !c.is_empty()) {
        url.push_str(&format!("&cursor={}", urlencoding::encode(c)));
    }

    let response = send_with_retry(|| {
        client
            .get(&url)
            .header(COOKIE, cookie_header(security_token))
            .header("Accept", "application/json")
    })
    .await?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to get private servers (status {})",
            response.status().as_u16()
        ));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse private servers: {}", e))
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

        let response = request.send_noting().await.map_err(|e| http_client::describe_error(&e))?;

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
