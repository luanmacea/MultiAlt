use crate::api::endpoints;
use reqwest::header::{COOKIE, REFERER};
use serde::{Deserialize, Serialize};

/// Referer Roblox expects on the auth-ticket endpoints. A function rather than
/// a const because the host comes from `endpoints`.
fn referer_url() -> String {
    format!("{}/games/2753915549/Blox-Fruits", endpoints::host("www"))
}

fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
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
        .map_err(|e| format!("Request failed: {}", e))?;

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
        .map_err(|e| format!("Request failed: {}", e))?;

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
        .map_err(|e| format!("Request failed: {}", e))?;

    if let Some(ticket) = response
        .headers()
        .get("rbx-authentication-ticket")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
    {
        return Ok(ticket);
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(format!(
        "Failed to get authentication ticket (status {}): {}",
        status.as_u16(),
        body
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
        .map_err(|e| format!("Request failed: {}", e))?;

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

    let response = client
        .post(format!("{}/v1/account/pin/unlock", endpoints::host("auth")))
        .header(COOKIE, cookie_header(security_token))
        .header(REFERER, format!("{}/", endpoints::host("www")))
        .header("X-CSRF-TOKEN", &csrf)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!("pin={}", pin))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

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

    let response = client
        .post(format!("{}/authentication/signoutfromallsessionsandreauthenticate", endpoints::host("www")))
        .header(COOKIE, cookie_header(security_token))
        .header(REFERER, format!("{}/", endpoints::host("www")))
        .header("X-CSRF-TOKEN", &csrf)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

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

    let response = client
        .post(format!("{}/v2/user/passwords/change", endpoints::host("auth")))
        .header(COOKIE, cookie_header(security_token))
        .header(REFERER, format!("{}/", endpoints::host("www")))
        .header("X-CSRF-TOKEN", &csrf)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!(
            "currentPassword={}&newPassword={}",
            urlencoding::encode(current_password),
            urlencoding::encode(new_password)
        ))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

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

    let response = client
        .post(format!("{}/v1/email", endpoints::host("accountsettings")))
        .header(COOKIE, cookie_header(security_token))
        .header(REFERER, format!("{}/", endpoints::host("www")))
        .header("X-CSRF-TOKEN", &csrf)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!(
            "password={}&emailAddress={}",
            urlencoding::encode(password),
            urlencoding::encode(new_email)
        ))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

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

    let response = client
        .post(format!("{}/auth-token-service/v1/login/enterCode", endpoints::host("apis")))
        .header(COOKIE, cookie_header(security_token))
        .header("X-CSRF-TOKEN", &csrf)
        .json(&serde_json::json!({ "code": normalized_code }))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

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

    let response = client
        .post(format!("{}/auth-token-service/v1/login/validateCode", endpoints::host("apis")))
        .header(COOKIE, cookie_header(security_token))
        .header("X-CSRF-TOKEN", &csrf)
        .json(&serde_json::json!({ "code": normalized_code }))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

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

    let response = client
        .patch(&format!(
            "{}/v1/users/{}/display-names",
            endpoints::host("users"),
            user_id
        ))
        .header(COOKIE, cookie_header(security_token))
        .header("X-CSRF-TOKEN", &csrf)
        .json(&serde_json::json!({ "newDisplayName": display_name }))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

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

#[cfg(test)]
mod auth_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server, mount_csrf};
    use wiremock::matchers::{header, header_exists, method, path};
    use wiremock::{Mock, ResponseTemplate};

    #[tokio::test]
    async fn validate_cookie_parses_account_info() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("www", "/my/account/json")))
            .and(header("cookie", cookie_of("valid-cookie")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "UserId": 1234,
                "Name": "alt_one",
                "DisplayName": "Alt One",
                "UserEmail": "a***@example.com",
                "IsEmailVerified": true,
                "AgeBracket": 0,
                "UserAbove13": true
            })))
            .mount(server)
            .await;

        let info = validate_cookie("valid-cookie").await.expect("account info");
        assert_eq!(info.user_id, 1234);
        assert_eq!(info.name, "alt_one");
        assert_eq!(info.display_name, "Alt One");
        assert!(info.is_email_verified);
        assert!(info.user_above_13);
    }

    #[tokio::test]
    async fn validate_cookie_rejects_unauthorized() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("www", "/my/account/json")))
            .and(header("cookie", cookie_of("expired-cookie")))
            .respond_with(ResponseTemplate::new(401))
            .mount(server)
            .await;

        let err = validate_cookie("expired-cookie").await.unwrap_err();
        assert_eq!(err, "Invalid cookie (status 401)");
    }

    #[tokio::test]
    async fn validate_cookie_reports_non_json_body() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("www", "/my/account/json")))
            .and(header("cookie", cookie_of("html-cookie")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw("<!DOCTYPE html><html>login</html>", "text/html"),
            )
            .mount(server)
            .await;

        let err = validate_cookie("html-cookie").await.unwrap_err();
        assert!(
            err.starts_with("Failed to parse account info: "),
            "unexpected error: {}",
            err
        );
        // The body is echoed (truncated) so the user can tell a login page from
        // a real API error.
        assert!(err.contains("<!DOCTYPE html>"), "unexpected error: {}", err);
    }

    #[tokio::test]
    async fn get_csrf_token_reads_the_header_off_a_403() {
        mount_csrf("csrf-account", "csrf-token-abc").await;

        let token = get_csrf_token("csrf-account").await.expect("csrf token");
        assert_eq!(token, "csrf-token-abc");
    }

    #[tokio::test]
    async fn get_csrf_token_errors_when_header_is_missing() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("auth", "/v1/authentication-ticket/")))
            .and(header("cookie", cookie_of("no-csrf-account")))
            .respond_with(ResponseTemplate::new(403).set_body_string("Token Validation Failed"))
            .mount(server)
            .await;

        let err = get_csrf_token("no-csrf-account").await.unwrap_err();
        assert!(err.starts_with("[403 Forbidden]"), "unexpected error: {}", err);
        assert!(err.contains("Token Validation Failed"), "unexpected error: {}", err);
    }

    #[tokio::test]
    async fn get_auth_ticket_returns_the_ticket_header() {
        let server = mock_server().await;
        mount_csrf("ticket-account", "csrf-for-ticket").await;

        Mock::given(method("POST"))
            .and(path(mock_path("auth", "/v1/authentication-ticket/")))
            .and(header("cookie", cookie_of("ticket-account")))
            .and(header_exists("x-csrf-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("rbx-authentication-ticket", "ticket-xyz"),
            )
            .mount(server)
            .await;

        let ticket = get_auth_ticket("ticket-account").await.expect("ticket");
        assert_eq!(ticket, "ticket-xyz");
    }

    #[tokio::test]
    async fn get_auth_ticket_surfaces_a_moderated_account() {
        let server = mock_server().await;
        mount_csrf("moderated-account", "csrf-for-moderated").await;

        Mock::given(method("POST"))
            .and(path(mock_path("auth", "/v1/authentication-ticket/")))
            .and(header("cookie", cookie_of("moderated-account")))
            .and(header_exists("x-csrf-token"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "errors": [{ "code": 0, "message": "User is moderated" }]
            })))
            .mount(server)
            .await;

        let err = get_auth_ticket("moderated-account").await.unwrap_err();
        assert!(
            err.starts_with("Failed to get authentication ticket (status 403): "),
            "unexpected error: {}",
            err
        );
        // The launch flow buckets the account into the "moderadas" group based
        // on this exact string; keep the two in sync.
        assert!(
            crate::is_moderated_error(&err),
            "launch_shared::is_moderated_error should classify: {}",
            err
        );
    }
}
