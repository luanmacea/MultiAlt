pub async fn parse_private_server_link_code(
    security_token: &str,
    place_id: i64,
    link_code: &str,
) -> Result<String, String> {
    let csrf = crate::api::auth::get_csrf_token(security_token).await?;
    let client = game_join_client();

    let normalized_link_code = normalize_private_server_link_code(link_code);
    if normalized_link_code.is_empty() {
        return Err("Failed to parse private server access code".to_string());
    }
    let encoded_link_code = urlencoding::encode(&normalized_link_code);
    let referer = format!("{}/games/{}", endpoints::host("www"), place_id);

    let candidates = [
        format!(
            "{}/games/{}?privateServerLinkCode={}",
            endpoints::host("www"),
            place_id, encoded_link_code
        ),
        format!(
            "{}/share-links?code={}&type=Server",
            endpoints::host("www"),
            encoded_link_code
        ),
        format!(
            "{}/share?code={}&type=Server",
            endpoints::host("www"),
            encoded_link_code
        ),
        format!(
            "{}/games/{}?privateServerLinkCode={}",
            endpoints::host("web"),
            place_id, encoded_link_code
        ),
        format!(
            "{}/share-links?code={}&type=Server",
            endpoints::host("web"),
            encoded_link_code
        ),
    ];

    for url in candidates {
        let response = client
            .get(&url)
            .header(COOKIE, cookie_header(security_token))
            .header("X-CSRF-TOKEN", &csrf)
            .header("Referer", &referer)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if response.status().is_success() {
            let body = response
                .text()
                .await
                .map_err(|e| format!("Failed to read response: {}", e))?;
            if let Some(code) = extract_access_code(&body) {
                return Ok(code);
            }
        }
    }

    Err("Failed to parse private server access code".to_string())
}

/// POSTs to the share-link resolver and returns the raw payload. Shared by the
/// private-server path below and by join_links.rs (ExperienceInvite).
pub async fn resolve_share_link_payload(
    security_token: &str,
    link_id: &str,
    link_type: &str,
) -> Result<serde_json::Value, String> {
    let csrf = crate::api::auth::get_csrf_token(security_token).await?;
    let client = game_join_client();

    let normalized_link_id = extract_query_param_value_recursive(link_id, "code")
        .unwrap_or_else(|| decode_url_component(link_id.trim()).trim().to_string());

    if normalized_link_id.is_empty() {
        return Err("Missing share link code".to_string());
    }

    let response = client
        .post(format!("{}/sharelinks/v1/resolve-link", endpoints::host("apis")))
        .header(COOKIE, cookie_header(security_token))
        .header("X-CSRF-TOKEN", &csrf)
        .header("Content-Type", "application/json")
        .header("Origin", endpoints::host("www"))
        .header("Referer", format!("{}/share-links", endpoints::host("www")))
        .json(&serde_json::json!({
            "linkId": normalized_link_id,
            "linkType": link_type,
        }))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Failed to resolve share link (status {}): {}",
            status,
            body.trim()
        ));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

pub async fn resolve_share_server_link(
    security_token: &str,
    link_id: &str,
) -> Result<(Option<i64>, String), String> {
    let body = resolve_share_link_payload(security_token, link_id, "Server").await?;


    let server_data = if body["privateServerInviteData"].is_object() {
        &body["privateServerInviteData"]
    } else {
        &body["serverData"]
    };

    let status = server_data["status"].as_str().unwrap_or_default().to_ascii_lowercase();

    let resolved_link_code = server_data["linkCode"]
        .as_str()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if let Some(code) = resolved_link_code {
        let mut place_id = server_data["placeId"].as_i64();
        if place_id.is_none() {
            if let Some(universe_id) = server_data["universeId"].as_i64() {
                place_id = get_root_place_id_from_universe(security_token, universe_id).await?;
            }
        }

        return Ok((place_id, code));
    }

    if !status.is_empty() && status != "valid" && status != "expired" {
        return Err("Share link is not valid for server launch".to_string());
    }

    Err("Share link did not return a private server link code".to_string())
}

async fn get_root_place_id_from_universe(
    security_token: &str,
    universe_id: i64,
) -> Result<Option<i64>, String> {
    let client = game_join_client();
    let response = client
        .get(format!(
            "{}/v1/games?universeIds={}",
            endpoints::host("games"),
            universe_id
        ))
        .header(COOKIE, cookie_header(security_token))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Failed to resolve root place from universe (status {}): {}",
            status,
            body.trim()
        ));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let root_place_id = body["data"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item["rootPlaceId"].as_i64());

    Ok(root_place_id)
}

fn decode_url_component(value: &str) -> String {
    urlencoding::decode(value)
        .map(|v| v.into_owned())
        .unwrap_or_else(|_| value.to_string())
}

fn extract_query_param_value(input: &str, key: &str) -> Option<String> {
    for part in input.split(['?', '&']) {
        let pair = part.split('#').next().unwrap_or(part);
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        if !k.eq_ignore_ascii_case(key) {
            continue;
        }

        let decoded = decode_url_component(v.trim());
        let value = decoded.trim();
        if value.is_empty()
            || value.eq_ignore_ascii_case("null")
            || value.eq_ignore_ascii_case("undefined")
        {
            continue;
        }

        return Some(value.to_string());
    }
    None
}

fn extract_query_param_value_recursive(input: &str, key: &str) -> Option<String> {
    if let Some(value) = extract_query_param_value(input, key) {
        return Some(value);
    }

    let decoded = decode_url_component(input);
    if decoded != input {
        return extract_query_param_value(&decoded, key);
    }

    None
}

fn normalize_private_server_link_code(link_code: &str) -> String {
    let trimmed = link_code.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if let Some(code) = extract_query_param_value_recursive(trimmed, "privateServerLinkCode") {
        return code;
    }

    if let Some(code) = extract_query_param_value_recursive(trimmed, "linkCode") {
        return code;
    }

    let lower = trimmed.to_ascii_lowercase();
    let starts_with_code = lower.starts_with("code=");
    if starts_with_code
        || lower.contains("/share?")
        || lower.contains("/share-links")
        || lower.contains("navigation/share_links")
        || lower.contains("type=server")
        || lower.contains("pid=server")
    {
        if let Some(code) = extract_query_param_value_recursive(trimmed, "code") {
            return code;
        }
    }

    if trimmed.len() >= 4
        && trimmed
            .get(..4)
            .map(|head| head.eq_ignore_ascii_case("vip:"))
            .unwrap_or(false)
    {
        if let Some(rest) = trimmed.get(4..) {
            let decoded = decode_url_component(rest.trim());
            let value = decoded.trim();
            if !value.is_empty() {
                return value.to_string();
            }
        }
    }

    decode_url_component(trimmed).trim().to_string()
}

fn sanitize_access_code(value: &str) -> Option<String> {
    let cleaned = value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim();
    if cleaned.is_empty()
        || cleaned.eq_ignore_ascii_case("null")
        || cleaned.eq_ignore_ascii_case("undefined")
    {
        None
    } else {
        Some(cleaned.to_string())
    }
}

fn extract_between(html: &str, marker: &str, terminator: char) -> Option<String> {
    let start = html.find(marker)? + marker.len();
    let rest = &html[start..];
    let end = rest.find(terminator)?;
    sanitize_access_code(&rest[..end])
}

fn extract_quoted_value_after(html: &str, marker: &str) -> Option<String> {
    let start = html.find(marker)? + marker.len();
    let rest = &html[start..];
    let quote_idx = rest.find(|c| c == '\'' || c == '"')?;
    let quote = rest.as_bytes().get(quote_idx).copied()? as char;
    let payload = rest.get(quote_idx + 1..)?;
    let end = payload.find(quote)?;
    sanitize_access_code(&payload[..end])
}

fn extract_access_code(html: &str) -> Option<String> {
    let marker = "Roblox.GameLauncher.joinPrivateGame(";
    if let Some(code) = extract_quoted_value_after(html, marker) {
        return Some(code);
    }

    for (marker, terminator) in [
        ("\"accessCode\":\"", '"'),
        ("\\\"accessCode\\\":\\\"", '"'),
        ("accessCode=\"", '"'),
        ("accessCode='", '\''),
    ] {
        if let Some(code) = extract_between(html, marker, terminator) {
            return Some(code);
        }
    }

    None
}


#[cfg(test)]
mod private_link_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server, mount_csrf};
    use wiremock::matchers::{body_partial_json, header, method, path, query_param};
    use wiremock::{Mock, ResponseTemplate};

    /// Mounts the share-link resolver for one account token.
    async fn mount_resolve_link(token: &str, body: serde_json::Value) {
        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/sharelinks/v1/resolve-link")))
            .and(header("cookie", cookie_of(token)))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(mock_server().await)
            .await;
    }

    #[tokio::test]
    async fn resolves_a_server_payload_with_invite_data() {
        mount_csrf("share-server-ok", "csrf-share-ok").await;
        mount_resolve_link(
            "share-server-ok",
            serde_json::json!({
                "privateServerInviteData": {
                    "status": "Valid",
                    "placeId": 606849621,
                    "linkCode": "link-code-abc"
                }
            }),
        )
        .await;

        let (place_id, link_code) = resolve_share_server_link("share-server-ok", "code-a")
            .await
            .expect("resolved server link");
        assert_eq!(place_id, Some(606849621));
        assert_eq!(link_code, "link-code-abc");
    }

    /// Some payloads only carry the universe; the resolver must follow up with
    /// the universe -> root place call.
    #[tokio::test]
    async fn falls_back_to_the_universe_root_place() {
        let server = mock_server().await;
        mount_csrf("share-server-universe", "csrf-share-universe").await;
        mount_resolve_link(
            "share-server-universe",
            serde_json::json!({
                "privateServerInviteData": {
                    "status": "Valid",
                    "universeId": 99,
                    "linkCode": "link-code-universe"
                }
            }),
        )
        .await;

        Mock::given(method("GET"))
            .and(path(mock_path("games", "/v1/games")))
            .and(query_param("universeIds", "99"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "id": 99, "rootPlaceId": 424242 }]
            })))
            .mount(server)
            .await;

        let (place_id, link_code) =
            resolve_share_server_link("share-server-universe", "code-universe")
                .await
                .expect("resolved server link");
        assert_eq!(place_id, Some(424242));
        assert_eq!(link_code, "link-code-universe");
    }

    #[tokio::test]
    async fn reports_the_status_when_the_resolver_fails() {
        let server = mock_server().await;
        mount_csrf("share-server-fail", "csrf-share-fail").await;

        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/sharelinks/v1/resolve-link")))
            .and(header("cookie", cookie_of("share-server-fail")))
            .respond_with(ResponseTemplate::new(500).set_body_string("upstream exploded"))
            .mount(server)
            .await;

        let err = resolve_share_server_link("share-server-fail", "code-fail")
            .await
            .unwrap_err();
        assert!(err.contains("status 500"), "unexpected error: {}", err);
        assert!(err.contains("upstream exploded"), "unexpected error: {}", err);
    }

    /// `resolve_share_link_payload` is the raw seam both the server and the
    /// invite path go through: it must forward the link id and type verbatim.
    #[tokio::test]
    async fn payload_forwards_the_link_id_and_type() {
        let server = mock_server().await;
        mount_csrf("share-payload", "csrf-share-payload").await;

        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/sharelinks/v1/resolve-link")))
            .and(header("cookie", cookie_of("share-payload")))
            .and(body_partial_json(serde_json::json!({
                "linkId": "payload-code",
                "linkType": "ExperienceInvite"
            })))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "echoed": true })),
            )
            .mount(server)
            .await;

        let body =
            resolve_share_link_payload("share-payload", "payload-code", "ExperienceInvite")
                .await
                .expect("payload");
        assert_eq!(body["echoed"], serde_json::json!(true));
    }
}
