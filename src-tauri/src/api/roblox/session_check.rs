// O cookie da conta ainda abre sessão? Leitura usada pelo "Check accounts".
//
// `GET users.roblox.com/v1/users/authenticated` com o cookie: 200 devolve
// `{ id, name, displayName }` (só o `id` é lido); 401 = cookie morto. É
// leitura pura — quem chama passa por `read_without_refresh`, nunca pelo
// refresh que desloga a conta de todas as sessões.

/// Resultado da conferência de um cookie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionCheck {
    /// Sessão viva; o `id` que o Roblox diz ser o dono do cookie.
    Valid(i64),
    /// 401: o cookie não abre mais sessão.
    Invalid,
    /// 429: o Roblox está limitando — tentar depois, nunca "inválida".
    RateLimited,
    /// Rede, 5xx, resposta estranha: também "tentar depois".
    Unknown(String),
}

#[derive(Deserialize)]
struct AuthenticatedUser {
    id: i64,
}

pub async fn check_session(security_token: &str, client: &reqwest::Client) -> SessionCheck {
    let response = match client
        .get(format!("{}/v1/users/authenticated", endpoints::host("users")))
        .header(COOKIE, cookie_header(security_token))
        .header("Accept", "application/json")
        .send_noting()
        .await
    {
        Ok(response) => response,
        Err(e) => return SessionCheck::Unknown(http_client::describe_error(&e)),
    };
    match response.status().as_u16() {
        200..=299 => match response.json::<AuthenticatedUser>().await {
            Ok(user) => SessionCheck::Valid(user.id),
            Err(_) => SessionCheck::Unknown("Unexpected response".to_string()),
        },
        401 => SessionCheck::Invalid,
        429 => SessionCheck::RateLimited,
        status => SessionCheck::Unknown(format!("status {status}")),
    }
}

#[cfg(test)]
mod session_check_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    async fn mount(token: &str, response: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path(mock_path("users", "/v1/users/authenticated")))
            .and(header("cookie", cookie_of(token)))
            .respond_with(response)
            .mount(mock_server().await)
            .await;
    }

    async fn check(token: &str) -> SessionCheck {
        check_session(token, &http_client::client()).await
    }

    #[tokio::test]
    async fn a_live_cookie_returns_its_user_id() {
        mount(
            "sess-ok",
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "id": 4242, "name": "alt", "displayName": "Alt" })),
        )
        .await;
        assert_eq!(check("sess-ok").await, SessionCheck::Valid(4242));
    }

    #[tokio::test]
    async fn a_401_is_a_dead_cookie() {
        mount("sess-401", ResponseTemplate::new(401)).await;
        assert_eq!(check("sess-401").await, SessionCheck::Invalid);
    }

    /// 429 é "tente depois": nunca pode virar "inválida".
    #[tokio::test]
    async fn a_429_is_try_later() {
        mount("sess-429", ResponseTemplate::new(429)).await;
        assert_eq!(check("sess-429").await, SessionCheck::RateLimited);
    }

    #[tokio::test]
    async fn a_server_error_is_unknown_not_invalid() {
        mount("sess-503", ResponseTemplate::new(503)).await;
        assert!(matches!(check("sess-503").await, SessionCheck::Unknown(_)));
    }
}
