#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserLookupResult {
    pub id: i64,
    pub name: String,
    #[serde(rename = "displayName", default)]
    pub display_name: String,
}

pub async fn get_user_id(security_token: Option<&str>, username: &str) -> Result<UserLookupResult, String> {
    let client = reqwest::Client::new();

    let mut request = client
        .post(format!("{}/v1/usernames/users", endpoints::host("users")))
        .json(&serde_json::json!({ "usernames": [username] }));

    if let Some(token) = security_token {
        request = request.header(COOKIE, cookie_header(token));
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Failed to look up user (status {})", response.status().as_u16()));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let user = body["data"]
        .as_array()
        .and_then(|arr| arr.first())
        .ok_or_else(|| format!("User '{}' not found", username))?;

    serde_json::from_value(user.clone())
        .map_err(|e| format!("Failed to parse user data: {}", e))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: i64,
    pub name: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "description", default)]
    pub description: String,
    #[serde(rename = "created", default)]
    pub created: String,
    #[serde(rename = "isBanned", default)]
    pub is_banned: bool,
    #[serde(rename = "hasVerifiedBadge", default)]
    pub has_verified_badge: bool,
}

pub async fn get_user_info(security_token: Option<&str>, user_id: i64) -> Result<UserInfo, String> {
    let client = reqwest::Client::new();

    let mut request = client
        .get(format!("{}/v1/users/{}", endpoints::host("users"), user_id));

    if let Some(token) = security_token {
        request = request.header(COOKIE, cookie_header(token));
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Failed to get user info (status {})", response.status().as_u16()));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse user info: {}", e))
}

pub async fn get_robux(security_token: &str) -> Result<i64, String> {
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/v1/user/currency", endpoints::host("economy")))
        .header(COOKIE, cookie_header(security_token))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if response.status().is_success() {
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;
        return Ok(body["robux"].as_i64().unwrap_or(0));
    }

    let client = no_redirect_client();

    let response = client
        .get(format!("{}/mobileapi/userinfo", endpoints::host("www")))
        .header(COOKIE, cookie_header(security_token))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Failed to get robux (status {}) {}", status, body.chars().take(200).collect::<String>()));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(body["RobuxBalance"].as_i64().unwrap_or(0))
}

#[allow(dead_code)]
pub async fn get_email_info(security_token: &str) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/v1/email", endpoints::host("accountsettings")))
        .header(COOKIE, cookie_header(security_token))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Failed to get email info (status {})", response.status().as_u16()));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse email info: {}", e))
}

pub async fn send_friend_request(security_token: &str, target_user_id: i64) -> Result<(), String> {
    let csrf = crate::api::auth::get_csrf_token(security_token).await?;
    send_friend_request_with_csrf(security_token, &csrf, target_user_id).await
}

/// Same as `send_friend_request` but uses a caller-supplied CSRF token so a
/// batch of requests from one account can reuse a single token instead of
/// fetching a fresh one per call.
pub async fn send_friend_request_with_csrf(
    security_token: &str,
    csrf: &str,
    target_user_id: i64,
) -> Result<(), String> {
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{}/v1/users/{}/request-friendship", endpoints::host("friends"), target_user_id))
        .header(COOKIE, cookie_header(security_token))
        .header("X-CSRF-TOKEN", csrf)
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if response.status().is_success() {
        Ok(())
    } else {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        Err(format!("Failed to send friend request (status {}): {}", status, body))
    }
}

/// Returns the user IDs of an account's current friends. Used to skip pairs
/// that are already friends and to verify newly-formed friendships.
pub async fn get_friend_ids(security_token: &str, user_id: i64) -> Result<Vec<i64>, String> {
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/v1/users/{}/friends", endpoints::host("friends"), user_id))
        .header(COOKIE, cookie_header(security_token))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Failed to get friends (status {})", response.status().as_u16()));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse friends: {}", e))?;

    let ids = body["data"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|u| u["id"].as_i64()).collect())
        .unwrap_or_default();

    Ok(ids)
}

pub async fn block_user(security_token: &str, target_user_id: i64) -> Result<(), String> {
    let csrf = crate::api::auth::get_csrf_token(security_token).await?;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{}/user-blocking-api/v1/users/{}/block-user", endpoints::host("apis"), target_user_id))
        .header(COOKIE, cookie_header(security_token))
        .header("X-CSRF-TOKEN", &csrf)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("Failed to block user (status {})", response.status().as_u16()))
    }
}

pub async fn unblock_user(security_token: &str, target_user_id: i64) -> Result<(), String> {
    let csrf = crate::api::auth::get_csrf_token(security_token).await?;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{}/user-blocking-api/v1/users/{}/unblock-user", endpoints::host("apis"), target_user_id))
        .header(COOKIE, cookie_header(security_token))
        .header("X-CSRF-TOKEN", &csrf)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("Failed to unblock user (status {})", response.status().as_u16()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockedUser {
    #[serde(rename = "userId")]
    pub user_id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "displayName", default)]
    pub display_name: String,
}

pub async fn get_blocked_users(security_token: &str) -> Result<Vec<BlockedUser>, String> {
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/user-blocking-api/v1/users/get-blocked-users", endpoints::host("apis")))
        .header(COOKIE, cookie_header(security_token))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Failed to get blocked users (status {}) {}", status, body.chars().take(200).collect::<String>()));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse blocked users: {}", e))?;

    if let Some(blocked) = body["blockedUsers"].as_array() {
        let mut users = Vec::new();
        for item in blocked {
            if let Some(user_id) = item["userId"].as_i64() {
                users.push(BlockedUser {
                    user_id,
                    name: item["name"].as_str().unwrap_or_default().to_string(),
                    display_name: item["displayName"].as_str().unwrap_or_default().to_string(),
                });
            }
        }
        if users.iter().all(|u| u.name.is_empty()) && !users.is_empty() {
            let ids: Vec<i64> = users.iter().map(|u| u.user_id).collect();
            if let Ok(infos) = lookup_user_names(&ids).await {
                for user in &mut users {
                    if let Some(info) = infos.iter().find(|i| i.id == user.user_id) {
                        user.name = info.name.clone();
                        user.display_name = info.display_name.clone();
                    }
                }
            }
        }
        return Ok(users);
    }

    Ok(Vec::new())
}

async fn lookup_user_names(user_ids: &[i64]) -> Result<Vec<UserLookupResult>, String> {
    let client = reqwest::Client::new();
    let mut results = Vec::new();

    for chunk in user_ids.chunks(100) {
        let response = client
            .post(format!("{}/v1/users", endpoints::host("users")))
            .json(&serde_json::json!({ "userIds": chunk }))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if response.status().is_success() {
            let body: serde_json::Value = response.json().await.unwrap_or_default();
            if let Some(data) = body["data"].as_array() {
                for user in data {
                    if let Ok(u) = serde_json::from_value::<UserLookupResult>(user.clone()) {
                        results.push(u);
                    }
                }
            }
        }
    }

    Ok(results)
}

pub async fn unblock_all_users(security_token: &str) -> Result<i32, String> {
    let blocked = get_blocked_users(security_token).await?;
    let mut count = 0;

    for user in &blocked {
        if unblock_user(security_token, user.user_id).await.is_ok() {
            count += 1;
        }
    }

    Ok(count)
}

pub async fn set_follow_privacy(security_token: &str, privacy: &str) -> Result<(), String> {
    let csrf = crate::api::auth::get_csrf_token(security_token).await?;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{}/account/settings/follow-me-privacy", endpoints::host("www")))
        .header(COOKIE, cookie_header(security_token))
        .header("Referer", format!("{}/my/account", endpoints::host("www")))
        .header("X-CSRF-TOKEN", &csrf)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!("FollowMePrivacy={}", privacy))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("Failed to set follow privacy (status {})", response.status().as_u16()))
    }
}

pub async fn get_private_server_invite_privacy(security_token: &str) -> Result<String, String> {
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/v1/privacy", endpoints::host("accountsettings")))
        .header(COOKIE, cookie_header(security_token))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Failed to get privacy settings (status {})", response.status().as_u16()));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse privacy settings: {}", e))?;

    if let Some(val) = body.get("privateServerInvitePrivacy").and_then(|v| v.as_str()) {
        return Ok(val.to_string());
    }

    Ok(body.to_string())
}

pub async fn set_private_server_invite_privacy(security_token: &str, privacy: &str) -> Result<(), String> {
    let csrf = crate::api::auth::get_csrf_token(security_token).await?;
    let client = reqwest::Client::new();

    let response = client
        .patch(format!("{}/v1/privacy", endpoints::host("accountsettings")))
        .header(COOKIE, cookie_header(security_token))
        .header("X-CSRF-TOKEN", &csrf)
        .json(&serde_json::json!({
            "privateServerInvitePrivacy": privacy
        }))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if response.status().is_success() {
        Ok(())
    } else {
        let body = response.text().await.unwrap_or_default();
        Err(format!("Failed to set privacy: {}", body.chars().take(200).collect::<String>()))
    }
}

#[cfg(test)]
mod user_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server};
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    #[tokio::test]
    async fn looks_up_a_user_by_name() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("users", "/v1/usernames/users")))
            .and(body_partial_json(
                serde_json::json!({ "usernames": ["alt_one"] }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "id": 1234, "name": "alt_one", "displayName": "Alt One" }]
            })))
            .mount(server)
            .await;

        let user = get_user_id(None, "alt_one").await.expect("user");
        assert_eq!(user.id, 1234);
        assert_eq!(user.name, "alt_one");
        assert_eq!(user.display_name, "Alt One");
    }

    #[tokio::test]
    async fn reads_user_info_with_a_cookie() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("users", "/v1/users/777")))
            .and(header("cookie", cookie_of("user-info-account")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 777,
                "name": "seven",
                "displayName": "Seven",
                "description": "hi",
                "created": "2020-01-01T00:00:00Z",
                "isBanned": false,
                "hasVerifiedBadge": true
            })))
            .mount(server)
            .await;

        let info = get_user_info(Some("user-info-account"), 777)
            .await
            .expect("user info");
        assert_eq!(info.id, 777);
        assert_eq!(info.display_name, "Seven");
        assert!(info.has_verified_badge);
        assert!(!info.is_banned);
    }

    #[tokio::test]
    async fn user_info_surfaces_the_status_on_failure() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("users", "/v1/users/404404")))
            .respond_with(ResponseTemplate::new(404))
            .mount(server)
            .await;

        let err = get_user_info(None, 404_404).await.unwrap_err();
        assert_eq!(err, "Failed to get user info (status 404)");
    }
}

/// The rest of `users.rs`: robux fallback, friends, blocking and the privacy
/// switches. Each test owns a `.ROBLOSECURITY` token so its mocks cannot match
/// another test on the shared mock server.
#[cfg(test)]
mod user_api_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server, mount_csrf};
    use wiremock::matchers::{body_string_contains, header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    #[tokio::test]
    async fn an_empty_lookup_result_names_the_username() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("users", "/v1/usernames/users")))
            .and(body_string_contains("ghost_user"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "data": [] })),
            )
            .mount(server)
            .await;

        assert_eq!(
            get_user_id(None, "ghost_user").await.unwrap_err(),
            "User 'ghost_user' not found"
        );
    }

    #[tokio::test]
    async fn a_failed_lookup_reports_the_status() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("users", "/v1/usernames/users")))
            .and(body_string_contains("throttled_user"))
            .respond_with(ResponseTemplate::new(429))
            .mount(server)
            .await;

        assert_eq!(
            get_user_id(None, "throttled_user").await.unwrap_err(),
            "Failed to look up user (status 429)"
        );
    }

    #[tokio::test]
    async fn a_non_json_lookup_body_is_reported_as_a_parse_error() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("users", "/v1/usernames/users")))
            .and(body_string_contains("html_user"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("<html/>", "text/html"))
            .mount(server)
            .await;

        let err = get_user_id(None, "html_user").await.unwrap_err();
        assert!(
            err.starts_with("Failed to parse response: "),
            "unexpected error: {}",
            err
        );
    }

    /// The economy endpoint is the happy path (covered in `economy_http_tests`);
    /// when it refuses the cookie the mobile API answers instead.
    #[tokio::test]
    async fn robux_falls_back_to_the_mobile_api() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("economy", "/v1/user/currency")))
            .and(header("cookie", cookie_of("robux-fallback")))
            .respond_with(ResponseTemplate::new(403))
            .mount(server)
            .await;

        Mock::given(method("GET"))
            .and(path(mock_path("www", "/mobileapi/userinfo")))
            .and(header("cookie", cookie_of("robux-fallback")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "RobuxBalance": 55 })),
            )
            .mount(server)
            .await;

        assert_eq!(get_robux("robux-fallback").await.expect("robux"), 55);
    }

    #[tokio::test]
    async fn robux_is_zero_when_the_payload_has_no_balance() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("economy", "/v1/user/currency")))
            .and(header("cookie", cookie_of("robux-empty")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(server)
            .await;

        assert_eq!(get_robux("robux-empty").await.expect("robux"), 0);
    }

    #[tokio::test]
    async fn robux_reports_both_endpoints_failing() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("economy", "/v1/user/currency")))
            .and(header("cookie", cookie_of("robux-dead")))
            .respond_with(ResponseTemplate::new(401))
            .mount(server)
            .await;

        Mock::given(method("GET"))
            .and(path(mock_path("www", "/mobileapi/userinfo")))
            .and(header("cookie", cookie_of("robux-dead")))
            .respond_with(ResponseTemplate::new(401).set_body_string("logged out"))
            .mount(server)
            .await;

        let err = get_robux("robux-dead").await.unwrap_err();
        assert!(err.starts_with("Failed to get robux (status 401)"), "{}", err);
        assert!(err.contains("logged out"), "{}", err);
    }

    #[tokio::test]
    async fn email_info_is_returned_verbatim() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("accountsettings", "/v1/email")))
            .and(header("cookie", cookie_of("email-info")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "emailAddress": "a***@example.com",
                "verified": true
            })))
            .mount(server)
            .await;

        let info = get_email_info("email-info").await.expect("email info");
        assert_eq!(info["verified"], serde_json::json!(true));
    }

    #[tokio::test]
    async fn email_info_reports_the_status() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("accountsettings", "/v1/email")))
            .and(header("cookie", cookie_of("email-info-bad")))
            .respond_with(ResponseTemplate::new(401))
            .mount(server)
            .await;

        assert_eq!(
            get_email_info("email-info-bad").await.unwrap_err(),
            "Failed to get email info (status 401)"
        );
    }

    /// The `_with_csrf` variant exists so a batch of friend requests reuses one
    /// token: it must send exactly the token it was handed and never fetch one.
    #[tokio::test]
    async fn a_friend_request_reuses_the_supplied_csrf_token() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("friends", "/v1/users/501/request-friendship")))
            .and(header("cookie", cookie_of("friend-reuse")))
            .and(header("x-csrf-token", "reused-token"))
            .respond_with(ResponseTemplate::new(200))
            .mount(server)
            .await;

        assert!(
            send_friend_request_with_csrf("friend-reuse", "reused-token", 501)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn a_rejected_friend_request_reports_status_and_body() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("friends", "/v1/users/502/request-friendship")))
            .and(header("cookie", cookie_of("friend-reject")))
            .respond_with(ResponseTemplate::new(429).set_body_string("Too many requests"))
            .mount(server)
            .await;

        let err = send_friend_request_with_csrf("friend-reject", "tok", 502)
            .await
            .unwrap_err();
        assert_eq!(
            err,
            "Failed to send friend request (status 429): Too many requests"
        );
    }

    /// The plain entry point fetches its own CSRF token first.
    #[tokio::test]
    async fn send_friend_request_fetches_a_csrf_token_first() {
        let server = mock_server().await;
        mount_csrf("friend-csrf", "csrf-friend").await;

        Mock::given(method("POST"))
            .and(path(mock_path("friends", "/v1/users/503/request-friendship")))
            .and(header("cookie", cookie_of("friend-csrf")))
            .and(header("x-csrf-token", "csrf-friend"))
            .respond_with(ResponseTemplate::new(200))
            .mount(server)
            .await;

        assert!(send_friend_request("friend-csrf", 503).await.is_ok());
    }

    #[tokio::test]
    async fn friend_ids_are_read_off_the_data_array() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("friends", "/v1/users/600/friends")))
            .and(header("cookie", cookie_of("friends-list")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "id": 1 }, { "id": 2 }, { "name": "no id" }]
            })))
            .mount(server)
            .await;

        let ids = get_friend_ids("friends-list", 600).await.expect("friends");
        assert_eq!(ids, vec![1, 2]);
    }

    #[tokio::test]
    async fn a_missing_friends_array_is_an_empty_list() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("friends", "/v1/users/601/friends")))
            .and(header("cookie", cookie_of("friends-empty")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(server)
            .await;

        assert!(get_friend_ids("friends-empty", 601)
            .await
            .expect("friends")
            .is_empty());
    }

    #[tokio::test]
    async fn friends_reports_the_status() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("friends", "/v1/users/602/friends")))
            .and(header("cookie", cookie_of("friends-error")))
            .respond_with(ResponseTemplate::new(403))
            .mount(server)
            .await;

        assert_eq!(
            get_friend_ids("friends-error", 602).await.unwrap_err(),
            "Failed to get friends (status 403)"
        );
    }

    #[tokio::test]
    async fn blocking_maps_the_status_to_a_result() {
        let server = mock_server().await;
        mount_csrf("block-ok", "csrf-block-ok").await;
        Mock::given(method("POST"))
            .and(path(mock_path(
                "apis",
                "/user-blocking-api/v1/users/700/block-user",
            )))
            .and(header("cookie", cookie_of("block-ok")))
            .and(header("x-csrf-token", "csrf-block-ok"))
            .respond_with(ResponseTemplate::new(200))
            .mount(server)
            .await;

        assert!(block_user("block-ok", 700).await.is_ok());

        mount_csrf("block-bad", "csrf-block-bad").await;
        Mock::given(method("POST"))
            .and(path(mock_path(
                "apis",
                "/user-blocking-api/v1/users/701/block-user",
            )))
            .and(header("cookie", cookie_of("block-bad")))
            .respond_with(ResponseTemplate::new(400))
            .mount(server)
            .await;

        assert_eq!(
            block_user("block-bad", 701).await.unwrap_err(),
            "Failed to block user (status 400)"
        );
    }

    #[tokio::test]
    async fn unblocking_maps_the_status_to_a_result() {
        let server = mock_server().await;
        mount_csrf("unblock-ok", "csrf-unblock-ok").await;
        Mock::given(method("POST"))
            .and(path(mock_path(
                "apis",
                "/user-blocking-api/v1/users/710/unblock-user",
            )))
            .and(header("cookie", cookie_of("unblock-ok")))
            .respond_with(ResponseTemplate::new(200))
            .mount(server)
            .await;

        assert!(unblock_user("unblock-ok", 710).await.is_ok());

        mount_csrf("unblock-bad", "csrf-unblock-bad").await;
        Mock::given(method("POST"))
            .and(path(mock_path(
                "apis",
                "/user-blocking-api/v1/users/711/unblock-user",
            )))
            .and(header("cookie", cookie_of("unblock-bad")))
            .respond_with(ResponseTemplate::new(500))
            .mount(server)
            .await;

        assert_eq!(
            unblock_user("unblock-bad", 711).await.unwrap_err(),
            "Failed to unblock user (status 500)"
        );
    }

    #[tokio::test]
    async fn the_blocked_list_keeps_the_names_it_is_given() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path(
                "apis",
                "/user-blocking-api/v1/users/get-blocked-users",
            )))
            .and(header("cookie", cookie_of("blocked-named")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "blockedUsers": [
                    { "userId": 801, "name": "eight_o_one", "displayName": "Eight" },
                    { "userId": 802, "name": "eight_o_two", "displayName": "Two" },
                    { "displayName": "no id at all" }
                ]
            })))
            .mount(server)
            .await;

        let blocked = get_blocked_users("blocked-named").await.expect("blocked");
        assert_eq!(blocked.len(), 2);
        assert_eq!(blocked[0].user_id, 801);
        assert_eq!(blocked[0].name, "eight_o_one");
        assert_eq!(blocked[1].display_name, "Two");
    }

    /// Roblox sometimes returns ids only; the names are then filled in with a
    /// second lookup.
    #[tokio::test]
    async fn the_blocked_list_fills_in_missing_names() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path(
                "apis",
                "/user-blocking-api/v1/users/get-blocked-users",
            )))
            .and(header("cookie", cookie_of("blocked-nameless")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "blockedUsers": [{ "userId": 811 }, { "userId": 812 }]
            })))
            .mount(server)
            .await;

        Mock::given(method("POST"))
            .and(path(mock_path("users", "/v1/users")))
            .and(body_string_contains("811"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    { "id": 811, "name": "eleven", "displayName": "Eleven" },
                    { "id": 812, "name": "twelve", "displayName": "Twelve" }
                ]
            })))
            .mount(server)
            .await;

        let blocked = get_blocked_users("blocked-nameless")
            .await
            .expect("blocked");
        assert_eq!(blocked.len(), 2);
        assert_eq!(blocked[0].name, "eleven");
        assert_eq!(blocked[1].display_name, "Twelve");
    }

    #[tokio::test]
    async fn a_payload_without_blocked_users_is_an_empty_list() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path(
                "apis",
                "/user-blocking-api/v1/users/get-blocked-users",
            )))
            .and(header("cookie", cookie_of("blocked-none")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(server)
            .await;

        assert!(get_blocked_users("blocked-none")
            .await
            .expect("blocked")
            .is_empty());
    }

    #[tokio::test]
    async fn the_blocked_list_reports_status_and_body() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path(
                "apis",
                "/user-blocking-api/v1/users/get-blocked-users",
            )))
            .and(header("cookie", cookie_of("blocked-error")))
            .respond_with(ResponseTemplate::new(403).set_body_string("nope"))
            .mount(server)
            .await;

        let err = get_blocked_users("blocked-error").await.unwrap_err();
        assert!(err.starts_with("Failed to get blocked users (status 403)"), "{}", err);
        assert!(err.contains("nope"), "{}", err);
    }

    /// Unblocking everyone counts only the calls that actually succeeded.
    #[tokio::test]
    async fn unblock_everyone_counts_the_successful_calls() {
        let server = mock_server().await;
        mount_csrf("unblock-all", "csrf-unblock-all").await;

        Mock::given(method("GET"))
            .and(path(mock_path(
                "apis",
                "/user-blocking-api/v1/users/get-blocked-users",
            )))
            .and(header("cookie", cookie_of("unblock-all")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "blockedUsers": [
                    { "userId": 901, "name": "ok_one" },
                    { "userId": 902, "name": "fails" }
                ]
            })))
            .mount(server)
            .await;

        Mock::given(method("POST"))
            .and(path(mock_path(
                "apis",
                "/user-blocking-api/v1/users/901/unblock-user",
            )))
            .and(header("cookie", cookie_of("unblock-all")))
            .respond_with(ResponseTemplate::new(200))
            .mount(server)
            .await;

        Mock::given(method("POST"))
            .and(path(mock_path(
                "apis",
                "/user-blocking-api/v1/users/902/unblock-user",
            )))
            .and(header("cookie", cookie_of("unblock-all")))
            .respond_with(ResponseTemplate::new(500))
            .mount(server)
            .await;

        assert_eq!(unblock_all_users("unblock-all").await.expect("count"), 1);
    }

    #[tokio::test]
    async fn follow_privacy_is_sent_as_a_form_field() {
        let server = mock_server().await;
        mount_csrf("follow-privacy", "csrf-follow-privacy").await;

        Mock::given(method("POST"))
            .and(path(mock_path("www", "/account/settings/follow-me-privacy")))
            .and(header("cookie", cookie_of("follow-privacy")))
            .and(body_string_contains("FollowMePrivacy=NoOne"))
            .respond_with(ResponseTemplate::new(200))
            .mount(server)
            .await;

        assert!(set_follow_privacy("follow-privacy", "NoOne").await.is_ok());
    }

    #[tokio::test]
    async fn follow_privacy_reports_the_status() {
        let server = mock_server().await;
        mount_csrf("follow-privacy-bad", "csrf-follow-privacy-bad").await;

        Mock::given(method("POST"))
            .and(path(mock_path("www", "/account/settings/follow-me-privacy")))
            .and(header("cookie", cookie_of("follow-privacy-bad")))
            .respond_with(ResponseTemplate::new(400))
            .mount(server)
            .await;

        assert_eq!(
            set_follow_privacy("follow-privacy-bad", "NoOne")
                .await
                .unwrap_err(),
            "Failed to set follow privacy (status 400)"
        );
    }

    #[tokio::test]
    async fn the_invite_privacy_setting_is_read_off_its_field() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("accountsettings", "/v1/privacy")))
            .and(header("cookie", cookie_of("privacy-field")))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({ "privateServerInvitePrivacy": "AllUsers" }),
            ))
            .mount(server)
            .await;

        assert_eq!(
            get_private_server_invite_privacy("privacy-field")
                .await
                .expect("privacy"),
            "AllUsers"
        );
    }

    /// Without the field the whole payload is handed back so the caller can at
    /// least show something.
    #[tokio::test]
    async fn a_payload_without_the_field_is_returned_as_json_text() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("accountsettings", "/v1/privacy")))
            .and(header("cookie", cookie_of("privacy-other")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "other": "value" })),
            )
            .mount(server)
            .await;

        assert_eq!(
            get_private_server_invite_privacy("privacy-other")
                .await
                .expect("privacy"),
            "{\"other\":\"value\"}"
        );
    }

    #[tokio::test]
    async fn reading_the_invite_privacy_reports_the_status() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("accountsettings", "/v1/privacy")))
            .and(header("cookie", cookie_of("privacy-error")))
            .respond_with(ResponseTemplate::new(500))
            .mount(server)
            .await;

        assert_eq!(
            get_private_server_invite_privacy("privacy-error")
                .await
                .unwrap_err(),
            "Failed to get privacy settings (status 500)"
        );
    }

    #[tokio::test]
    async fn writing_the_invite_privacy_patches_the_field() {
        let server = mock_server().await;
        mount_csrf("privacy-set", "csrf-privacy-set").await;

        Mock::given(method("PATCH"))
            .and(path(mock_path("accountsettings", "/v1/privacy")))
            .and(header("cookie", cookie_of("privacy-set")))
            .and(body_string_contains("privateServerInvitePrivacy"))
            .respond_with(ResponseTemplate::new(200))
            .mount(server)
            .await;

        assert!(
            set_private_server_invite_privacy("privacy-set", "Friends")
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn writing_the_invite_privacy_echoes_the_error_body() {
        let server = mock_server().await;
        mount_csrf("privacy-set-bad", "csrf-privacy-set-bad").await;

        Mock::given(method("PATCH"))
            .and(path(mock_path("accountsettings", "/v1/privacy")))
            .and(header("cookie", cookie_of("privacy-set-bad")))
            .respond_with(ResponseTemplate::new(400).set_body_string("invalid value"))
            .mount(server)
            .await;

        assert_eq!(
            set_private_server_invite_privacy("privacy-set-bad", "Nobody")
                .await
                .unwrap_err(),
            "Failed to set privacy: invalid value"
        );
    }
}
