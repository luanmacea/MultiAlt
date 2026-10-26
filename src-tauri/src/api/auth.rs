// `send_noting` no lugar de `send`: o `.ROBLOSECURITY` novo que o Roblox
// devolver em qualquer resposta fica registrado para a conta (ver
// `api::cookie_rotation`).
use crate::api::cookie_rotation::SendNoting;
use crate::api::endpoints;
use crate::api::http_client;
use reqwest::header::{COOKIE, REFERER};
use serde::{Deserialize, Serialize};

/// Referer Roblox expects on the auth-ticket endpoints. A function rather than
/// a const because the host comes from `endpoints`.
fn referer_url() -> String {
    format!("{}/games/2753915549/Blox-Fruits", endpoints::host("www"))
}

/// O cliente dos endpoints de autenticação. O teto de tempo vem de
/// [`http_client::builder`]: sem ele um `auth.roblox.com` que aceita e não
/// responde prendia o pedido de auth ticket — a primeira chamada de rede de todo
/// launch — e com ele a reserva da sequência de launch.
fn build_client() -> reqwest::Client {
    http_client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120 Safari/537.36")
        .build()
        .unwrap()
}

fn cookie_header(security_token: &str) -> String {
    format!(".ROBLOSECURITY={}", security_token)
}

fn normalize_quick_login_code(code: &str) -> String {
    code.chars().filter(|c| c.is_ascii_digit()).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    #[serde(alias = "UserId")]
    pub user_id: i64,
    #[serde(alias = "Name")]
    pub name: String,
    #[serde(alias = "DisplayName")]
    pub display_name: String,
    #[serde(alias = "UserEmail", default)]
    pub user_email: Option<String>,
    #[serde(alias = "IsEmailVerified", default)]
    pub is_email_verified: bool,
    #[serde(alias = "AgeBracket", default)]
    pub age_bracket: i32,
    #[serde(alias = "UserAbove13", default)]
    pub user_above_13: bool,
}

pub async fn validate_cookie(security_token: &str) -> Result<AccountInfo, String> {
    let client = build_client();

    let response = client
        .get(format!("{}/my/account/json", endpoints::host("www")))
        .header(COOKIE, cookie_header(security_token))
        .send_noting()
        .await
        .map_err(|e| http_client::describe_error(&e))?;

    // Conta moderada (banida, advertida, em análise): o site redireciona a
    // página para `/not-approved` em vez de responder. O cookie é bom; quem
    // diz de quem ele é passa a ser a API de usuários, que não redireciona.
    if response.status().is_redirection() {
        let to_moderation = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|loc| loc.contains("not-approved"));
        return validate_cookie_via_users_api(security_token, to_moderation).await;
    }

    if !response.status().is_success() {
        return Err(format!(
            "Invalid cookie (status {})",
            response.status().as_u16()
        ));
    }

    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    serde_json::from_str::<AccountInfo>(&body).map_err(|e| {
        format!(
            "Failed to parse account info: {} (body: {})",
            e,
            body.chars().take(200).collect::<String>()
        )
    })
}

/// Reserva do [`validate_cookie`] quando o site redireciona: a API de
/// usuários diz id e nome sem passar pela página que a moderação bloqueia.
/// `to_moderation` = o redirecionamento ia para `/not-approved`.
/// O erro de `validate_cookie` para conta moderada: o cookie pode ser bom, mas
/// o Roblox não diz de quem ele é pelas consultas de sempre.
pub fn is_moderated_account_error(error: &str) -> bool {
    error.contains("Roblox is restricting this account")
}

async fn validate_cookie_via_users_api(
    security_token: &str,
    to_moderation: bool,
) -> Result<AccountInfo, String> {
    #[derive(Deserialize)]
    struct AuthenticatedUser {
        id: i64,
        name: String,
        #[serde(rename = "displayName", default)]
        display_name: String,
    }

    let response = build_client()
        .get(format!("{}/v1/users/authenticated", endpoints::host("users")))
        .header(COOKIE, cookie_header(security_token))
        .send_noting()
        .await
        .map_err(|e| http_client::describe_error(&e))?;
    let status = response.status();

    if status.is_success() {
        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;
        let user = serde_json::from_str::<AuthenticatedUser>(&body)
            .map_err(|e| format!("Failed to parse account info: {}", e))?;
        return Ok(AccountInfo {
            user_id: user.id,
            display_name: if user.display_name.is_empty() { user.name.clone() } else { user.display_name },
            name: user.name,
            user_email: None,
            is_email_verified: false,
            age_bracket: 0,
            user_above_13: false,
        });
    }
    if status.as_u16() == 401 || !to_moderation {
        return Err(format!("Invalid cookie (status {})", status.as_u16()));
    }
    Err("Roblox is restricting this account (moderated: banned, warned or under review).          Check it at roblox.com/not-approved, then add it again."
        .to_string())
}

pub async fn get_csrf_token(security_token: &str) -> Result<String, String> {
    let client = build_client();

    let response = client
        .post(format!("{}/v1/authentication-ticket/", endpoints::host("auth")))
        .header(COOKIE, cookie_header(security_token))
        .header(REFERER, referer_url())
        .header("RBXAuthenticationNegotiation", "1")
        .send_noting()
        .await
        .map_err(|e| http_client::describe_error(&e))?;

    if let Some(token) = response
        .headers()
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
    {
        return Ok(token);
    }

    // Historically Roblox returns 403 and includes the x-csrf-token header.
    // If the status code or behavior changes, surface the response to help debug.
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(format!(
        "[{} {}] {}",
        status.as_u16(),
        status.canonical_reason().unwrap_or(""),
        body
    ))
}

/// Sends a request that needs a CSRF token, retrying once when the service
/// answers `403` with a token of its own.
///
/// Roblox's XSRF tokens are **per service**: the token `get_csrf_token` reads
/// from `auth.roblox.com` is rejected by `apis.roblox.com` with
/// `{"code":0,"message":"XSRF token invalid"}`, and the 403 carries, in the
/// `x-csrf-token` header, the token that service does accept. Retrying with it
/// is what the web client does — without this, share links, blocking and the
/// other `apis.roblox.com` calls fail every time.
///
/// The builder passed in must **not** carry `X-CSRF-TOKEN` already; this adds
/// it (reqwest appends headers, so a second one would be sent as well).
pub async fn send_with_csrf_retry(
    builder: reqwest::RequestBuilder,
    csrf: &str,
) -> Result<reqwest::Response, String> {
    let retry = builder.try_clone();
    let response = builder
        .header("X-CSRF-TOKEN", csrf)
        .send_noting()
        .await
        .map_err(|e| http_client::describe_error(&e))?;

    if response.status() != reqwest::StatusCode::FORBIDDEN {
        return Ok(response);
    }

    let fresh = response
        .headers()
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != csrf);

    match (fresh, retry) {
        (Some(fresh), Some(retry)) => retry
            .header("X-CSRF-TOKEN", fresh)
            .send_noting()
            .await
            .map_err(|e| http_client::describe_error(&e)),
        // No token to retry with (or a streaming body): keep the original 403
        // so the caller reports Roblox's own message.
        _ => Ok(response),
    }
}

pub async fn get_auth_ticket(security_token: &str) -> Result<String, String> {
    let csrf = get_csrf_token(security_token).await?;

    let client = build_client();

    let response = client
        .post(format!("{}/v1/authentication-ticket/", endpoints::host("auth")))
        .header(COOKIE, cookie_header(security_token))
        .header("x-csrf-token", &csrf)
        .header(REFERER, referer_url())
        .header("RBXAuthenticationNegotiation", "1")
        .header("Content-Type", "application/json")
        .body("")
        .send_noting()
        .await
        .map_err(|e| http_client::describe_error(&e))?;

    if let Some(ticket) = response
        .headers()
        .get("rbx-authentication-ticket")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
    {
        return Ok(ticket);
    }

    if let Some(message) = challenge_message(response.headers()) {
        return Err(message);
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(format!(
        "Failed to get authentication ticket (status {}): {}",
        status.as_u16(),
        body
    ))
}

/// Nome curto do tipo de verificação que o Roblox pediu (`rblx-challenge-type`).
/// Tipo desconhecido vira "a security check": a mensagem continua útil mesmo
/// quando o Roblox inventa um desafio novo.
fn challenge_kind_label(kind: &str) -> &'static str {
    match kind.trim().to_ascii_lowercase().as_str() {
        "twostepverification" | "forcetwostepverification" => "2-step verification",
        "captcha" => "a CAPTCHA",
        "reauthentication" => "your password again",
        _ => "a security check",
    }
}

/// Mensagem para quando o Roblox responde com um desafio (`rblx-challenge-id` /
/// `rblx-challenge-type`) em vez do ticket. O app **nunca** tenta resolver o
/// desafio: só diz onde terminá-lo. O texto evita de propósito as palavras que
/// `is_auth_session_error` (renovaria a sessão) e `is_moderated_error` (moveria
/// a conta para "moderadas") reconhecem.
fn challenge_message(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let read = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let id = read("rblx-challenge-id");
    let kind = read("rblx-challenge-type");
    if id.is_none() && kind.is_none() {
        return None;
    }
    Some(format!(
        "Roblox wants to verify this account ({}). Open it in the browser (account panel › Tools › Browser), finish the check there, then try again.",
        challenge_kind_label(kind.as_deref().unwrap_or(""))
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinInfo {
    #[serde(rename = "isEnabled")]
    pub is_enabled: bool,
    #[serde(rename = "unlockedUntil", default)]
    pub unlocked_until: Option<serde_json::Value>,
}

pub async fn check_pin(security_token: &str) -> Result<bool, String> {
    let _csrf = get_csrf_token(security_token).await?;

    let client = build_client();

    let response = client
        .get(format!("{}/v1/account/pin/", endpoints::host("auth")))
        .header(COOKIE, cookie_header(security_token))
        .header(REFERER, format!("{}/", endpoints::host("www")))
        .send_noting()
        .await
        .map_err(|e| http_client::describe_error(&e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to check pin (status {})",
            response.status().as_u16()
        ));
    }

    let info: PinInfo = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse pin info: {}", e))?;

    if !info.is_enabled {
        return Ok(true);
    }

    match &info.unlocked_until {
        Some(serde_json::Value::Number(n)) if n.as_i64().unwrap_or(0) > 0 => Ok(true),
        _ => Ok(false),
    }
}

pub async fn unlock_pin(security_token: &str, pin: &str) -> Result<bool, String> {
    if pin.len() != 4 {
        return Err("Pin must be 4 digits".to_string());
    }

    let csrf = get_csrf_token(security_token).await?;

    let client = build_client();

    let request = client
        .post(format!("{}/v1/account/pin/unlock", endpoints::host("auth")))
        .header(COOKIE, cookie_header(security_token))
        .header(REFERER, format!("{}/", endpoints::host("www")))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!("pin={}", pin));
    let response = send_with_csrf_retry(request, &csrf).await?;

    if !response.status().is_success() {
        return Ok(false);
    }

    let info: PinInfo = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse pin response: {}", e))?;

    Ok(info.is_enabled
        && matches!(&info.unlocked_until, Some(serde_json::Value::Number(n)) if n.as_i64().unwrap_or(0) > 0))
}

pub struct RefreshResult {
    pub success: bool,
    pub new_cookie: Option<String>,
}

pub async fn log_out_other_sessions(security_token: &str) -> Result<RefreshResult, String> {
    let csrf = get_csrf_token(security_token).await?;

    let client = build_client();

    // O `www.roblox.com/authentication/signoutfromallsessionsandreauthenticate`
    // saiu do ar (404, conferido em 25/10/2026): o "Atualizar cookies" falhava
    // em toda conta, inclusive nas de sessão boa. O endereço atual é este.
    let request = client
        .post(format!("{}/v1/logoutfromallsessionsandreauthenticate", endpoints::host("auth")))
        .header(COOKIE, cookie_header(security_token))
        .header(REFERER, format!("{}/", endpoints::host("www")))
        .header("Content-Type", "application/json")
        .body("{}");
    let response = send_with_csrf_retry(request, &csrf).await?;

    // Roblox may return redirects for this endpoint while still setting cookies.
    if !(response.status().is_success() || response.status().is_redirection()) {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Failed to sign out other sessions (status {}): {}",
            status.as_u16(),
            body
        ));
    }

    let new_cookie = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .find_map(|v| {
            let s = v.to_str().ok()?;
            if s.starts_with(".ROBLOSECURITY=") {
                let value = s.strip_prefix(".ROBLOSECURITY=")?;
                let value = value.split(';').next()?;
                Some(value.to_string())
            } else {
                None
            }
        });

    Ok(RefreshResult {
        success: true,
        new_cookie,
    })
}


/// Prefixo estável dos erros de troca de senha/e-mail. A tela reconhece o tipo
/// por ele e mostra a frase traduzida (`credentialErrorText` no frontend).
/// Formato: `credential-error:<tipo>:<status>:<detalhe do Roblox>`.
pub const CREDENTIAL_ERROR_PREFIX: &str = "credential-error";

/// Por que o Roblox recusou a troca de senha ou de e-mail. Antes o app jogava a
/// resposta fora e dizia só "Failed to change password" (relato do dono,
/// 15/10/2026: no lote, umas davam certo e outras não, sem explicação).
///
/// O detalhe nunca leva as palavras que `is_auth_session_error` reconhece
/// ("unauthorized", "authorization has been denied", "status 401"): elas
/// renovariam a sessão (`signoutfromallsessions`) e derrubariam as outras
/// sessões da conta — e uma troca de credencial nunca fez isso.
pub fn classify_credential_failure(status: u16, challenge_type: Option<&str>, has_challenge: bool, body: &str) -> String {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("errors")
                .and_then(|e| e.get(0))
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .map(|m| m.trim().to_string())
        })
        .unwrap_or_default();
    let lower = message.to_ascii_lowercase();

    let kind = if has_challenge {
        match challenge_type.unwrap_or("").trim().to_ascii_lowercase().as_str() {
            "captcha" => "verification-captcha",
            "twostepverification" | "forcetwostepverification" => "verification-2sv",
            "reauthentication" => "verification-password",
            _ => "verification-other",
        }
    } else if status == 429 || lower.contains("too many") || lower.contains("flood") || lower.contains("try again later") {
        "rate-limited"
    } else if status == 401 || lower.contains("authorization has been denied") || lower.contains("not authenticated") {
        "session-expired"
    } else if lower.contains("pin") && lower.contains("lock") {
        "pin-locked"
    } else if lower.contains("password")
        && (lower.contains("incorrect") || lower.contains("wrong") || lower.contains("current"))
    {
        "wrong-current-password"
    } else if lower.contains("password")
        && (lower.contains("weak")
            || lower.contains("simple")
            || lower.contains("common")
            || lower.contains("username")
            || lower.contains("invalid")
            || lower.contains("not valid")
            || lower.contains("length")
            || lower.contains("short"))
    {
        "invalid-new-password"
    } else if lower.contains("email") && (lower.contains("invalid") || lower.contains("already") || lower.contains("in use")) {
        "invalid-email"
    } else {
        "unknown"
    };

    // Detalhe do Roblox, sem as palavras que renovariam a sessão.
    let detail = if lower.contains("unauthorized")
        || lower.contains("authorization has been denied")
        || lower.contains("not authenticated")
        || lower.contains("401")
    {
        String::new()
    } else {
        message.replace(':', " -").chars().take(200).collect()
    };
    format!("{CREDENTIAL_ERROR_PREFIX}:{kind}:{status}:{detail}")
}

async fn credential_failure(response: reqwest::Response) -> String {
    let status = response.status().as_u16();
    let challenge_type = response
        .headers()
        .get("rblx-challenge-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let has_challenge = challenge_type.is_some() || response.headers().contains_key("rblx-challenge-id");
    let body = response.text().await.unwrap_or_default();
    classify_credential_failure(status, challenge_type.as_deref(), has_challenge, &body)
}

pub async fn change_password(
    security_token: &str,
    current_password: &str,
    new_password: &str,
) -> Result<Option<String>, String> {
    let csrf = get_csrf_token(security_token).await?;

    let client = build_client();

    let request = client
        .post(format!("{}/v2/user/passwords/change", endpoints::host("auth")))
        .header(COOKIE, cookie_header(security_token))
        .header(REFERER, format!("{}/", endpoints::host("www")))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!(
            "currentPassword={}&newPassword={}",
            urlencoding::encode(current_password),
            urlencoding::encode(new_password)
        ));
    let response = send_with_csrf_retry(request, &csrf).await?;

    if !response.status().is_success() {
        return Err(credential_failure(response).await);
    }

    let new_cookie = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .find_map(|v| {
            let s = v.to_str().ok()?;
            if s.starts_with(".ROBLOSECURITY=") {
                let value = s.strip_prefix(".ROBLOSECURITY=")?;
                let value = value.split(';').next()?;
                Some(value.to_string())
            } else {
                None
            }
        });

    Ok(new_cookie)
}

pub async fn change_email(
    security_token: &str,
    password: &str,
    new_email: &str,
) -> Result<(), String> {
    let csrf = get_csrf_token(security_token).await?;

    let client = build_client();

    let request = client
        .post(format!("{}/v1/email", endpoints::host("accountsettings")))
        .header(COOKIE, cookie_header(security_token))
        .header(REFERER, format!("{}/", endpoints::host("www")))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!(
            "password={}&emailAddress={}",
            urlencoding::encode(password),
            urlencoding::encode(new_email)
        ));
    let response = send_with_csrf_retry(request, &csrf).await?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(credential_failure(response).await)
    }
}

pub async fn quick_login_enter_code(
    security_token: &str,
    code: &str,
) -> Result<serde_json::Value, String> {
    let normalized_code = normalize_quick_login_code(code);
    if normalized_code.len() != 6 {
        return Err("Code must be 6 digits".to_string());
    }

    let csrf = get_csrf_token(security_token).await?;
    let client = build_client();

    let request = client
        .post(format!("{}/auth-token-service/v1/login/enterCode", endpoints::host("apis")))
        .header(COOKIE, cookie_header(security_token))
        .json(&serde_json::json!({ "code": normalized_code }));
    let response = send_with_csrf_retry(request, &csrf).await?;

    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Failed to enter code: {}", body));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

pub async fn quick_login_validate_code(security_token: &str, code: &str) -> Result<(), String> {
    let normalized_code = normalize_quick_login_code(code);
    if normalized_code.len() != 6 {
        return Err("Code must be 6 digits".to_string());
    }

    let csrf = get_csrf_token(security_token).await?;
    let client = build_client();

    let request = client
        .post(format!("{}/auth-token-service/v1/login/validateCode", endpoints::host("apis")))
        .header(COOKIE, cookie_header(security_token))
        .json(&serde_json::json!({ "code": normalized_code }));
    let response = send_with_csrf_retry(request, &csrf).await?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err("Failed to validate code".to_string())
    }
}

// ── Quick Login do lado de quem **entra** (ideia 12) ────────────────────────
//
// O fluxo oficial do Roblox para entrar num aparelho novo sem digitar senha:
// 1. `auth-token-service/v1/login/create` devolve um código curto e uma chave
//    privada (a chave nunca sai do backend);
// 2. a pessoa digita o código num aparelho já logado (roblox.com/crossdevicelogin
//    ou Configurações › Quick Log In) e confirma;
// 3. `login/status` passa de `Created` para `UserLinked` e depois `Validated`;
// 4. `auth.roblox.com/v2/login` com `ctype: AuthToken` troca código + chave pela
//    sessão (`.ROBLOSECURITY` no `Set-Cookie`).
//
// Se o Roblox pedir verificação (captcha ou outro desafio) no passo 4, o app
// **para** com uma mensagem — não tenta resolver nem contornar o desafio.

/// Um Quick Login em andamento. A chave privada fica só aqui.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickLoginSession {
    pub code: String,
    pub private_key: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum QuickLoginStatus {
    /// Esperando o código ser digitado no outro aparelho.
    Pending,
    /// Código digitado; esperando a confirmação lá.
    #[serde(rename_all = "camelCase")]
    Linked { account_name: Option<String> },
    /// Confirmado: já dá para trocar pela sessão.
    Validated,
    Cancelled,
    Expired,
}

/// Mensagem de quando o Roblox pede verificação para concluir o Quick Login.
pub const QUICK_LOGIN_CHALLENGE_MESSAGE: &str =
    "Roblox asked for an extra check (like a CAPTCHA) to finish this sign-in, and MultiAlt doesn't solve those. Add the account with Browser Login instead.";

/// POST sem conta que precisa de XSRF: a primeira resposta é 403 com o token
/// no `x-csrf-token`, e o pedido é repetido uma vez com ele.
async fn post_with_csrf_handshake(
    url: String,
    body: &serde_json::Value,
) -> Result<reqwest::Response, String> {
    let client = build_client();
    let first = client
        .post(&url)
        .json(body)
        .send_noting()
        .await
        .map_err(|e| http_client::describe_error(&e))?;
    if first.status() != reqwest::StatusCode::FORBIDDEN {
        return Ok(first);
    }
    let Some(token) = first
        .headers()
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    else {
        return Ok(first);
    };
    client
        .post(&url)
        .header("X-CSRF-TOKEN", token)
        .json(body)
        .send_noting()
        .await
        .map_err(|e| http_client::describe_error(&e))
}

pub async fn quick_login_create() -> Result<QuickLoginSession, String> {
    let response = post_with_csrf_handshake(
        format!("{}/auth-token-service/v1/login/create", endpoints::host("apis")),
        &serde_json::json!({}),
    )
    .await?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "Roblox couldn't start Quick Login (status {}).",
            status.as_u16()
        ));
    }
    parse_quick_login_session(&body)
}

fn parse_quick_login_session(body: &str) -> Result<QuickLoginSession, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|_| "Unexpected Quick Login answer from Roblox.".to_string())?;
    let text = |key: &str| {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    match (text("code"), text("privateKey")) {
        (Some(code), Some(private_key)) => Ok(QuickLoginSession {
            code,
            private_key,
            expires_at: text("expirationTime"),
        }),
        _ => Err("Unexpected Quick Login answer from Roblox.".to_string()),
    }
}

/// O que a resposta do `login/status` quer dizer. 400 é código inválido ou
/// vencido (o Roblox responde `CodeInvalid`); estado desconhecido conta como
/// "ainda esperando".
fn parse_quick_login_status(status: u16, body: &str) -> Result<QuickLoginStatus, String> {
    if status == 400 {
        return Ok(QuickLoginStatus::Expired);
    }
    if !(200..300).contains(&status) {
        return Err(format!("Roblox returned status {status} while checking Quick Login."));
    }
    let value: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    let field = |key: &str| value.get(key).and_then(|v| v.as_str()).map(str::to_string);
    Ok(match field("status").as_deref() {
        Some("Validated") => QuickLoginStatus::Validated,
        Some("UserLinked") => QuickLoginStatus::Linked {
            account_name: field("accountName").filter(|s| !s.trim().is_empty()),
        },
        Some("Cancelled") => QuickLoginStatus::Cancelled,
        Some("CodeInvalid") | Some("Expired") => QuickLoginStatus::Expired,
        _ => QuickLoginStatus::Pending,
    })
}

pub async fn quick_login_status(session: &QuickLoginSession) -> Result<QuickLoginStatus, String> {
    let response = post_with_csrf_handshake(
        format!("{}/auth-token-service/v1/login/status", endpoints::host("apis")),
        &serde_json::json!({ "code": session.code, "privateKey": session.private_key }),
    )
    .await?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    parse_quick_login_status(status, &body)
}

/// O `.ROBLOSECURITY` de um `Set-Cookie`, se houver.
fn roblosecurity_from_headers(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|cookie| {
            cookie
                .strip_prefix(".ROBLOSECURITY=")
                .map(|rest| rest.split(';').next().unwrap_or_default().to_string())
        })
        .filter(|token| !token.is_empty())
}

/// Troca um Quick Login confirmado pela sessão da conta.
pub async fn quick_login_redeem(session: &QuickLoginSession) -> Result<String, String> {
    let response = post_with_csrf_handshake(
        format!("{}/v2/login", endpoints::host("auth")),
        &serde_json::json!({
            "ctype": "AuthToken",
            "cvalue": session.code,
            "password": session.private_key,
        }),
    )
    .await?;
    if let Some(token) = roblosecurity_from_headers(response.headers()) {
        return Ok(token);
    }
    if challenge_message(response.headers()).is_some() {
        return Err(QUICK_LOGIN_CHALLENGE_MESSAGE.to_string());
    }
    Err(format!(
        "Roblox didn't finish the sign-in (status {}). Try again with a new code, or use Browser Login.",
        response.status().as_u16()
    ))
}

pub async fn set_display_name(
    security_token: &str,
    user_id: i64,
    display_name: &str,
) -> Result<(), String> {
    let csrf = get_csrf_token(security_token).await?;

    let client = build_client();

    let request = client
        .patch(&format!(
            "{}/v1/users/{}/display-names",
            endpoints::host("users"),
            user_id
        ))
        .header(COOKIE, cookie_header(security_token))
        .json(&serde_json::json!({ "newDisplayName": display_name }));
    let response = send_with_csrf_retry(request, &csrf).await?;

    if response.status().is_success() {
        Ok(())
    } else {
        let body = response.text().await.unwrap_or_default();
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
            if let Some(msg) = json["errors"][0]["message"].as_str() {
                return Err(msg.to_string());
            }
        }
        Err(format!("Failed to set display name: {}", body))
    }
}
