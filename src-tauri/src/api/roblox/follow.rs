// Seguir um usuário (o "Follow" de rede social do perfil, não o "seguir no
// jogo" do launch). Usado pelo lote `follow_user_with_accounts`
// (commands/follow_user.rs): várias contas passam a seguir um mesmo alvo, que
// não precisa estar no MultiAlt.

/// Faz a conta do cookie seguir `target_user_id`, com um CSRF já em mãos (o
/// lote busca um por conta). 403 com `rblx-challenge-id` é o Roblox pedindo
/// captcha: a mensagem diz isso em vez de um "status 403" mudo.
pub async fn follow_user_with_csrf(
    security_token: &str,
    csrf: &str,
    target_user_id: i64,
) -> Result<(), String> {
    let client = http_client::client();

    let request = client
        .post(format!("{}/v1/users/{}/follow", endpoints::host("friends"), target_user_id))
        .header(COOKIE, cookie_header(security_token))
        .header("Content-Type", "application/json");
    let response = crate::api::auth::send_with_csrf_retry(request, csrf).await?;

    if response.status().is_success() {
        return Ok(());
    }
    let status = response.status().as_u16();
    let challenge = response.headers().contains_key("rblx-challenge-id");
    let body = response.text().await.unwrap_or_default();
    if challenge {
        return Err(format!(
            "Roblox asked for a verification (captcha) before following (status {})",
            status
        ));
    }
    Err(format!(
        "Failed to follow (status {}): {}",
        status,
        body.chars().take(200).collect::<String>()
    ))
}

/// Se a conta do cookie já segue `target_user_id`. Serve para pular quem já
/// segue e para conferir, no fim, quem passou a seguir.
pub async fn is_following_with_csrf(
    security_token: &str,
    csrf: &str,
    target_user_id: i64,
) -> Result<bool, String> {
    let client = http_client::client();

    let request = client
        .post(format!("{}/v1/user/following-exists", endpoints::host("friends")))
        .header(COOKIE, cookie_header(security_token))
        .json(&serde_json::json!({ "targetUserIds": [target_user_id] }));
    let response = crate::api::auth::send_with_csrf_retry(request, csrf).await?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to check following (status {})",
            response.status().as_u16()
        ));
    }
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse following: {}", e))?;
    Ok(parse_following_exists(&body, target_user_id))
}

/// `{"followings":[{"isFollowing":true,"userId":1}]}` → se segue o alvo.
/// Entrada sem o alvo conta como "não segue": na dúvida o lote tenta seguir.
fn parse_following_exists(body: &serde_json::Value, target_user_id: i64) -> bool {
    body["followings"]
        .as_array()
        .map(|entries| {
            entries.iter().any(|entry| {
                entry["userId"].as_i64() == Some(target_user_id)
                    && entry["isFollowing"].as_bool() == Some(true)
            })
        })
        .unwrap_or(false)
}
