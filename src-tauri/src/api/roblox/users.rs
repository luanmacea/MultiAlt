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
        .post("https://users.roblox.com/v1/usernames/users")
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
        .get(format!("https://users.roblox.com/v1/users/{}", user_id));

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
        .get("https://economy.roblox.com/v1/user/currency")
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
        .get("https://www.roblox.com/mobileapi/userinfo")
        .header(COOKIE, cookie_header(security_token))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Failed to get robux (status {}) {}", status, &body[..body.len().min(200)]));
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
        .get("https://accountsettings.roblox.com/v1/email")
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
    let client = reqwest::Client::new();

    let response = client
        .post(format!("https://friends.roblox.com/v1/users/{}/request-friendship", target_user_id))
        .header(COOKIE, cookie_header(security_token))
        .header("X-CSRF-TOKEN", &csrf)
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if response.status().is_success() {
        Ok(())
    } else {
        let body = response.text().await.unwrap_or_default();
        Err(format!("Failed to send friend request: {}", body))
    }
}

pub async fn block_user(security_token: &str, target_user_id: i64) -> Result<(), String> {
    let csrf = crate::api::auth::get_csrf_token(security_token).await?;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("https://apis.roblox.com/user-blocking-api/v1/users/{}/block-user", target_user_id))
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
        .post(format!("https://apis.roblox.com/user-blocking-api/v1/users/{}/unblock-user", target_user_id))
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
        .get("https://apis.roblox.com/user-blocking-api/v1/users/get-blocked-users")
        .header(COOKIE, cookie_header(security_token))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Failed to get blocked users (status {}) {}", status, &body[..body.len().min(200)]));
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
            .post("https://users.roblox.com/v1/users")
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
        .post("https://www.roblox.com/account/settings/follow-me-privacy")
        .header(COOKIE, cookie_header(security_token))
        .header("Referer", "https://www.roblox.com/my/account")
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
        .get("https://accountsettings.roblox.com/v1/privacy")
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
        .patch("https://accountsettings.roblox.com/v1/privacy")
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
        Err(format!("Failed to set privacy: {}", &body[..body.len().min(200)]))
    }
}
