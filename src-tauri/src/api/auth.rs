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
        .send()
        .await
        .map_err(|e| http_client::describe_error(&e))?;

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

pub async fn get_csrf_token(security_token: &str) -> Result<String, String> {
    let client = build_client();

    let response = client
        .post(format!("{}/v1/authentication-ticket/", endpoints::host("auth")))
        .header(COOKIE, cookie_header(security_token))
        .header(REFERER, referer_url())
        .header("RBXAuthenticationNegotiation", "1")
        .send()
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
        .send()
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
            .send()
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
        .send()
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
        .send()
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

    let request = client
        .post(format!("{}/authentication/signoutfromallsessionsandreauthenticate", endpoints::host("www")))
        .header(COOKIE, cookie_header(security_token))
        .header(REFERER, format!("{}/", endpoints::host("www")))
        .header("Content-Type", "application/x-www-form-urlencoded");
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
        return Err("Failed to change password".to_string());
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
        Err("Failed to change email".to_string())
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
