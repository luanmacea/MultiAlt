#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailRequest {
    #[serde(rename = "requestId")]
    pub request_id: String,
    #[serde(rename = "type", alias = "thumbnailType")]
    pub thumbnail_type: String,
    #[serde(rename = "targetId")]
    pub target_id: i64,
    pub size: String,
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailResponse {
    #[serde(rename = "targetId")]
    pub target_id: i64,
    #[serde(rename = "imageUrl")]
    pub image_url: Option<String>,
    #[serde(rename = "errorCode", default)]
    pub error_code: i32,
    #[serde(rename = "requestId", default)]
    pub request_id: String,
    #[serde(default)]
    pub state: String,
}

pub async fn batch_thumbnails(requests: Vec<ThumbnailRequest>) -> Result<Vec<ThumbnailResponse>, String> {
    if requests.is_empty() {
        return Ok(Vec::new());
    }

    let client = reqwest::Client::new();
    let mut all_results = Vec::new();

    for chunk in requests.chunks(100) {
        let response = send_with_retry(|| {
            client
                .post(format!("{}/v1/batch", endpoints::host("thumbnails")))
                .json(&chunk)
        })
        .await?;

        if !response.status().is_success() {
            return Err(format!("Batch request failed (status {})", response.status().as_u16()));
        }

        let body: serde_json::Value = response.json().await.map_err(|e| format!("Failed to parse: {}", e))?;

        if let Some(data) = body["data"].as_array() {
            for item in data {
                if let Ok(t) = serde_json::from_value::<ThumbnailResponse>(item.clone()) {
                    all_results.push(t);
                }
            }
        }
    }

    Ok(all_results)
}

pub async fn get_avatar_headshots(user_ids: &[i64], size: &str) -> Result<Vec<ThumbnailResponse>, String> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }

    let client = reqwest::Client::new();
    let ids: String = user_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");

    let url = format!(
        "{}/v1/users/avatar-headshot?size={}&format=png&userIds={}",
        endpoints::host("thumbnails"),
        size, ids
    );
    let response = send_with_retry(|| client.get(&url)).await?;

    if !response.status().is_success() {
        return Err(format!("Failed to get headshots (status {})", response.status().as_u16()));
    }

    let body: serde_json::Value = response.json().await.map_err(|e| format!("Failed to parse: {}", e))?;

    Ok(body["data"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect())
        .unwrap_or_default())
}

pub async fn get_asset_thumbnails(
    asset_ids: &[i64],
    size: &str,
    security_token: Option<&str>,
) -> Result<Vec<ThumbnailResponse>, String> {
    if asset_ids.is_empty() {
        return Ok(Vec::new());
    }

    let client = reqwest::Client::new();
    let ids: String = asset_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");

    let mut request = client.get(format!(
        "{}/v1/assets?assetIds={}&returnPolicy=PlaceHolder&size={}&format=Png&isCircular=false",
        endpoints::host("thumbnails"),
        ids, size
    ));

    if let Some(token) = security_token {
        request = request.header(COOKIE, cookie_header(token));
    }

    let response = request.send().await.map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Failed to get asset thumbnails (status {})", response.status().as_u16()));
    }

    let body: serde_json::Value = response.json().await.map_err(|e| format!("Failed to parse: {}", e))?;

    Ok(body["data"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect())
        .unwrap_or_default())
}

#[cfg(test)]
mod thumbnail_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{mock_path, mock_server};
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, ResponseTemplate};

    #[tokio::test]
    async fn reads_avatar_headshots() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("thumbnails", "/v1/users/avatar-headshot")))
            .and(query_param("userIds", "11,22"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {
                        "targetId": 11,
                        "state": "Completed",
                        "imageUrl": "https://tr.rbxcdn.com/11.png"
                    },
                    {
                        "targetId": 22,
                        "state": "Completed",
                        "imageUrl": "https://tr.rbxcdn.com/22.png"
                    }
                ]
            })))
            .mount(server)
            .await;

        let shots = get_avatar_headshots(&[11, 22], "48x48")
            .await
            .expect("headshots");
        assert_eq!(shots.len(), 2);
        assert_eq!(shots[0].target_id, 11);
        assert_eq!(
            shots[1].image_url.as_deref(),
            Some("https://tr.rbxcdn.com/22.png")
        );
    }

    /// Public-API view of the bounded retry budget in `http.rs`: after the last
    /// 429 the throttled response reaches the caller and becomes an error.
    #[tokio::test]
    async fn headshots_fail_after_the_retry_budget() {
        let server = mock_server().await;
        let throttled = Mock::given(method("GET"))
            .and(path(mock_path("thumbnails", "/v1/users/avatar-headshot")))
            .and(query_param("userIds", "999"))
            .respond_with(ResponseTemplate::new(429))
            .expect(3)
            .named("headshots are retried three times")
            .mount_as_scoped(server)
            .await;

        let err = get_avatar_headshots(&[999], "48x48").await.unwrap_err();
        assert_eq!(err, "Failed to get headshots (status 429)");

        drop(throttled);
    }

    #[tokio::test]
    async fn batch_thumbnails_short_circuits_on_an_empty_request() {
        // No mock is mounted: an empty batch must not touch the network.
        assert!(batch_thumbnails(Vec::new()).await.expect("empty").is_empty());
    }
}
