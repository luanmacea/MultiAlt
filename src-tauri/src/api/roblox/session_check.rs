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
