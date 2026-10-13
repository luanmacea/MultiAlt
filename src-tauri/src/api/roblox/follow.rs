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

#[cfg(test)]
mod follow_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server};
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    #[tokio::test]
    async fn follow_posts_with_the_supplied_csrf_token() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("friends", "/v1/users/9001/follow")))
            .and(header("cookie", cookie_of("follow-ok")))
            .and(header("x-csrf-token", "follow-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .expect(1)
            .mount(server)
            .await;

        follow_user_with_csrf("follow-ok", "follow-token", 9001)
            .await
            .expect("follow");
    }

    #[tokio::test]
    async fn a_rejected_follow_reports_status_and_body() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("friends", "/v1/users/9002/follow")))
            .and(header("cookie", cookie_of("follow-bad")))
            .respond_with(ResponseTemplate::new(429).set_body_string("Too many requests"))
            .mount(server)
            .await;

        assert_eq!(
            follow_user_with_csrf("follow-bad", "tok", 9002).await.unwrap_err(),
            "Failed to follow (status 429): Too many requests"
        );
    }

    /// Captcha: o 403 vem com `rblx-challenge-id` e sem CSRF novo. A mensagem
    /// tem que dizer o que aconteceu, não só o número.
    #[tokio::test]
    async fn a_challenge_is_named_as_a_captcha() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("friends", "/v1/users/9003/follow")))
            .and(header("cookie", cookie_of("follow-captcha")))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("rblx-challenge-id", "abc")
                    .set_body_string("{\"errors\":[{\"code\":0,\"message\":\"Challenge is required\"}]}"),
            )
            .mount(server)
            .await;

        let err = follow_user_with_csrf("follow-captcha", "tok", 9003)
            .await
            .unwrap_err();
        assert!(err.contains("captcha"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn following_exists_reads_the_target_entry() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("friends", "/v1/user/following-exists")))
            .and(header("cookie", cookie_of("following-yes")))
            .and(body_partial_json(serde_json::json!({ "targetUserIds": [9010] })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "followings": [{ "isFollowing": true, "isFollowed": false, "userId": 9010 }]
            })))
            .mount(server)
            .await;

        assert!(is_following_with_csrf("following-yes", "tok", 9010)
            .await
            .expect("check"));
    }

    #[tokio::test]
    async fn following_exists_reports_the_status() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("friends", "/v1/user/following-exists")))
            .and(header("cookie", cookie_of("following-err")))
            .respond_with(ResponseTemplate::new(500))
            .mount(server)
            .await;

        assert_eq!(
            is_following_with_csrf("following-err", "tok", 9011)
                .await
                .unwrap_err(),
            "Failed to check following (status 500)"
        );
    }
}

#[cfg(test)]
mod follow_parse_tests {
    use super::*;

    #[test]
    fn only_a_true_entry_for_the_target_counts() {
        let body = serde_json::json!({
            "followings": [
                { "isFollowing": true, "userId": 1 },
                { "isFollowing": false, "userId": 2 }
            ]
        });
        assert!(parse_following_exists(&body, 1));
        assert!(!parse_following_exists(&body, 2));
        assert!(!parse_following_exists(&body, 3));
    }

    #[test]
    fn a_body_without_followings_is_not_following() {
        assert!(!parse_following_exists(&serde_json::json!({}), 1));
        assert!(!parse_following_exists(&serde_json::json!({ "followings": null }), 1));
    }
}
