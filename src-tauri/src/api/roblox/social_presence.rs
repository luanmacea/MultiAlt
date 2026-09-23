pub async fn join_group(security_token: &str, group_id: i64) -> Result<(), String> {
    let csrf = crate::api::auth::get_csrf_token(security_token).await?;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{}/v1/groups/{}/users", endpoints::host("groups"), group_id))
        .header(COOKIE, cookie_header(security_token))
        .header("X-CSRF-TOKEN", &csrf)
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if response.status().is_success() {
        Ok(())
    } else {
        let body = response.text().await.unwrap_or_default();
        Err(format!("Failed to join group: {}", body))
    }
}

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

pub async fn get_presence(user_ids: &[i64]) -> Result<Vec<UserPresence>, String> {
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{}/v1/presence/users", endpoints::host("presence")))
        .json(&serde_json::json!({ "userIds": user_ids }))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Failed to get presence (status {})", response.status().as_u16()));
    }

    let body: serde_json::Value = response.json().await.map_err(|e| format!("Failed to parse: {}", e))?;

    Ok(body["userPresences"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect())
        .unwrap_or_default())
}

#[cfg(test)]
mod social_presence_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{mock_path, mock_server};
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, ResponseTemplate};

    #[tokio::test]
    async fn reads_user_presences() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("presence", "/v1/presence/users")))
            .and(body_partial_json(serde_json::json!({ "userIds": [11, 22] })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "userPresences": [
                    {
                        "userPresenceType": 2,
                        "lastLocation": "Jailbreak",
                        "placeId": 606849621,
                        "rootPlaceId": 606849621,
                        "gameId": "job-1",
                        "universeId": 606,
                        "userId": 11,
                        "lastOnline": "2024-01-01T00:00:00Z"
                    },
                    { "userPresenceType": 0, "userId": 22 }
                ]
            })))
            .mount(server)
            .await;

        let presences = get_presence(&[11, 22]).await.expect("presences");
        assert_eq!(presences.len(), 2);
        assert_eq!(presences[0].user_presence_type, 2);
        assert_eq!(presences[0].game_id.as_deref(), Some("job-1"));
        assert_eq!(presences[1].user_id, 22);
    }
}

/// Group joining and the error/empty branches of the presence call.
#[cfg(test)]
mod social_presence_extra_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server, mount_csrf};
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    #[tokio::test]
    async fn joining_a_group_posts_an_empty_object() {
        let server = mock_server().await;
        mount_csrf("group-ok", "csrf-group-ok").await;

        Mock::given(method("POST"))
            .and(path(mock_path("groups", "/v1/groups/1234/users")))
            .and(header("cookie", cookie_of("group-ok")))
            .and(header("x-csrf-token", "csrf-group-ok"))
            .respond_with(ResponseTemplate::new(200))
            .mount(server)
            .await;

        assert!(join_group("group-ok", 1234).await.is_ok());
    }

    #[tokio::test]
    async fn a_refused_group_join_echoes_the_body() {
        let server = mock_server().await;
        mount_csrf("group-bad", "csrf-group-bad").await;

        Mock::given(method("POST"))
            .and(path(mock_path("groups", "/v1/groups/1235/users")))
            .and(header("cookie", cookie_of("group-bad")))
            .respond_with(ResponseTemplate::new(403).set_body_string("group is locked"))
            .mount(server)
            .await;

        assert_eq!(
            join_group("group-bad", 1235).await.unwrap_err(),
            "Failed to join group: group is locked"
        );
    }

    #[tokio::test]
    async fn presence_reports_the_status() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("presence", "/v1/presence/users")))
            .and(body_partial_json(serde_json::json!({ "userIds": [4040] })))
            .respond_with(ResponseTemplate::new(503))
            .mount(server)
            .await;

        assert_eq!(
            get_presence(&[4040]).await.unwrap_err(),
            "Failed to get presence (status 503)"
        );
    }

    #[tokio::test]
    async fn a_payload_without_presences_is_an_empty_list() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("presence", "/v1/presence/users")))
            .and(body_partial_json(serde_json::json!({ "userIds": [4041] })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(server)
            .await;

        assert!(get_presence(&[4041]).await.expect("presence").is_empty());
    }

    /// An entry that cannot deserialize (no `userId`) is skipped instead of
    /// failing the whole call.
    #[tokio::test]
    async fn an_unparseable_presence_entry_is_skipped() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("presence", "/v1/presence/users")))
            .and(body_partial_json(serde_json::json!({ "userIds": [4042] })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "userPresences": [
                    { "userPresenceType": 1 },
                    { "userPresenceType": 1, "userId": 4042 }
                ]
            })))
            .mount(server)
            .await;

        let presences = get_presence(&[4042]).await.expect("presence");
        assert_eq!(presences.len(), 1);
        assert_eq!(presences[0].user_id, 4042);
        assert!(presences[0].last_location.is_empty());
        assert!(presences[0].place_id.is_none());
    }

    #[tokio::test]
    async fn a_malformed_presence_body_is_a_parse_error() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("presence", "/v1/presence/users")))
            .and(body_partial_json(serde_json::json!({ "userIds": [4043] })))
            .respond_with(ResponseTemplate::new(200).set_body_raw("nope", "text/plain"))
            .mount(server)
            .await;

        let err = get_presence(&[4043]).await.unwrap_err();
        assert!(err.starts_with("Failed to parse: "), "unexpected: {}", err);
    }
}
