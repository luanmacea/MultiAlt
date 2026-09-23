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

/// `batch_thumbnails` chunking plus the asset-thumbnail call, which
/// `thumbnail_http_tests` above does not cover.
///
/// Both this module and `api/batch.rs` POST to `/thumbnails/v1/batch`, so every
/// mock here matches on a target id that no other test uses.
#[cfg(test)]
mod thumbnail_batch_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server};
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, Request, ResponseTemplate};

    fn request_for(target_id: i64) -> ThumbnailRequest {
        ThumbnailRequest {
            request_id: format!("{}:req", target_id),
            thumbnail_type: "Avatar".to_string(),
            target_id,
            size: "150x150".to_string(),
            format: "png".to_string(),
        }
    }

    /// Matches a batch whose JSON body mentions this target id.
    fn body_mentions(target_id: i64) -> impl Fn(&Request) -> bool {
        let needle = format!("\"targetId\":{}", target_id);
        move |req: &Request| String::from_utf8_lossy(&req.body).contains(&needle)
    }

    #[tokio::test]
    async fn a_batch_parses_the_data_array_and_skips_junk_entries() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("thumbnails", "/v1/batch")))
            .and(body_mentions(70_001))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    { "targetId": 70001, "state": "Completed", "imageUrl": "url-1" },
                    { "state": "Completed" },
                    { "targetId": 70002, "errorCode": 7, "state": "Error" }
                ]
            })))
            .mount(server)
            .await;

        let results = batch_thumbnails(vec![request_for(70_001)])
            .await
            .expect("batch");
        // The entry without a targetId cannot deserialize and is dropped.
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].target_id, 70_001);
        assert_eq!(results[0].image_url.as_deref(), Some("url-1"));
        assert_eq!(results[1].error_code, 7);
        assert!(results[1].image_url.is_none());
    }

    /// More than 100 requests are split into several calls.
    #[tokio::test]
    async fn a_large_batch_is_split_into_chunks_of_a_hundred() {
        let server = mock_server().await;
        let calls = Mock::given(method("POST"))
            .and(path(mock_path("thumbnails", "/v1/batch")))
            .and(body_mentions(71_000))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "data": [] })),
            )
            .expect(1)
            .named("first chunk carries target 71000")
            .mount_as_scoped(server)
            .await;

        let second = Mock::given(method("POST"))
            .and(path(mock_path("thumbnails", "/v1/batch")))
            .and(body_mentions(71_100))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "data": [] })),
            )
            .expect(1)
            .named("second chunk carries target 71100")
            .mount_as_scoped(server)
            .await;

        let requests: Vec<ThumbnailRequest> =
            (71_000..71_120).map(request_for).collect();
        assert!(batch_thumbnails(requests).await.expect("batch").is_empty());

        drop(calls);
        drop(second);
    }

    #[tokio::test]
    async fn a_failing_batch_reports_the_status() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("thumbnails", "/v1/batch")))
            .and(body_mentions(72_001))
            .respond_with(ResponseTemplate::new(400))
            .mount(server)
            .await;

        assert_eq!(
            batch_thumbnails(vec![request_for(72_001)])
                .await
                .unwrap_err(),
            "Batch request failed (status 400)"
        );
    }

    #[tokio::test]
    async fn a_malformed_batch_body_is_a_parse_error() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("thumbnails", "/v1/batch")))
            .and(body_mentions(72_002))
            .respond_with(ResponseTemplate::new(200).set_body_raw("nope", "text/plain"))
            .mount(server)
            .await;

        let err = batch_thumbnails(vec![request_for(72_002)])
            .await
            .unwrap_err();
        assert!(err.starts_with("Failed to parse: "), "unexpected: {}", err);
    }

    #[tokio::test]
    async fn headshots_short_circuit_on_an_empty_id_list() {
        assert!(get_avatar_headshots(&[], "48x48")
            .await
            .expect("empty")
            .is_empty());
    }

    #[tokio::test]
    async fn a_malformed_headshot_body_is_a_parse_error() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("thumbnails", "/v1/users/avatar-headshot")))
            .and(query_param("userIds", "998"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("nope", "text/plain"))
            .mount(server)
            .await;

        let err = get_avatar_headshots(&[998], "48x48").await.unwrap_err();
        assert!(err.starts_with("Failed to parse: "), "unexpected: {}", err);
    }

    #[tokio::test]
    async fn asset_thumbnails_short_circuit_on_an_empty_id_list() {
        assert!(get_asset_thumbnails(&[], "150x150", None)
            .await
            .expect("empty")
            .is_empty());
    }

    #[tokio::test]
    async fn asset_thumbnails_send_the_cookie_and_parse_the_data() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("thumbnails", "/v1/assets")))
            .and(query_param("assetIds", "73001,73002"))
            .and(header("cookie", cookie_of("asset-thumbs")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    { "targetId": 73001, "state": "Completed", "imageUrl": "a.png" },
                    { "targetId": 73002, "state": "Blocked", "imageUrl": serde_json::Value::Null }
                ]
            })))
            .mount(server)
            .await;

        let thumbs = get_asset_thumbnails(&[73_001, 73_002], "150x150", Some("asset-thumbs"))
            .await
            .expect("thumbnails");
        assert_eq!(thumbs.len(), 2);
        assert_eq!(thumbs[0].image_url.as_deref(), Some("a.png"));
        assert_eq!(thumbs[1].state, "Blocked");
        assert!(thumbs[1].image_url.is_none());
    }

    #[tokio::test]
    async fn asset_thumbnails_report_the_status() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("thumbnails", "/v1/assets")))
            .and(query_param("assetIds", "73003"))
            .respond_with(ResponseTemplate::new(500))
            .mount(server)
            .await;

        assert_eq!(
            get_asset_thumbnails(&[73_003], "150x150", None)
                .await
                .unwrap_err(),
            "Failed to get asset thumbnails (status 500)"
        );
    }

    #[tokio::test]
    async fn a_payload_without_data_is_an_empty_list() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("thumbnails", "/v1/assets")))
            .and(query_param("assetIds", "73004"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(server)
            .await;

        assert!(get_asset_thumbnails(&[73_004], "150x150", None)
            .await
            .expect("thumbnails")
            .is_empty());
    }

    /// `thumbnailType` is the alias the batch payload uses for `type`.
    #[test]
    fn a_thumbnail_request_accepts_both_type_spellings() {
        let from_type: ThumbnailRequest = serde_json::from_value(serde_json::json!({
            "requestId": "r", "type": "Avatar", "targetId": 1,
            "size": "150x150", "format": "png"
        }))
        .expect("type");
        let from_alias: ThumbnailRequest = serde_json::from_value(serde_json::json!({
            "requestId": "r", "thumbnailType": "Avatar", "targetId": 1,
            "size": "150x150", "format": "png"
        }))
        .expect("thumbnailType");
        assert_eq!(from_type.thumbnail_type, from_alias.thumbnail_type);
    }
}
