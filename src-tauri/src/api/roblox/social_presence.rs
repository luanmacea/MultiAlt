// `join_group` mora em `groups.rs`, junto da entrada com resultado tipado.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPresence {
    #[serde(rename = "userPresenceType")]
    pub user_presence_type: i32,
    #[serde(rename = "lastLocation", default)]
    pub last_location: String,
    #[serde(rename = "placeId")]
    pub place_id: Option<i64>,
    #[serde(rename = "rootPlaceId")]
    pub root_place_id: Option<i64>,
    #[serde(rename = "gameId")]
    pub game_id: Option<String>,
    #[serde(rename = "universeId")]
    pub universe_id: Option<i64>,
    #[serde(rename = "userId")]
    pub user_id: i64,
    #[serde(rename = "lastOnline", default)]
    pub last_online: String,
}

/// Presença sem autenticação: a API devolve a versão resumida, com `gameId`
/// sempre nulo. Para obter o job id use [`get_presence_as`] com o cookie.
pub async fn get_presence(user_ids: &[i64]) -> Result<Vec<UserPresence>, String> {
    get_presence_as(None, user_ids).await
}

/// Presença dos usuários, opcionalmente autenticada.
///
/// O `gameId` (job id do servidor, o que permite entrar) **só** vem quando a
/// requisição leva o cookie de uma conta; sem cookie a Roblox omite o campo.
pub async fn get_presence_as(
    security_token: Option<&str>,
    user_ids: &[i64],
) -> Result<Vec<UserPresence>, String> {
    let client = http_client::client();

    let mut request = client
        .post(format!("{}/v1/presence/users", endpoints::host("presence")))
        .json(&serde_json::json!({ "userIds": user_ids }));

    if let Some(token) = security_token {
        request = request.header(COOKIE, cookie_header(token));
    }

    let response = request
        .send_noting()
        .await
        .map_err(|e| http_client::describe_error(&e))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        // No 429 o `Retry-After` (em segundos) vai junto na mensagem: a espera da
        // aba Seguir lê esse número e não pergunta de novo antes da hora.
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok());
        return Err(match retry_after {
            Some(secs) if status == 429 => {
                format!("Failed to get presence (status 429, retry after {} s)", secs)
            }
            _ => format!("Failed to get presence (status {})", status),
        });
    }

    let body: serde_json::Value = response.json().await.map_err(|e| format!("Failed to parse: {}", e))?;

    Ok(body["userPresences"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect())
        .unwrap_or_default())
}
