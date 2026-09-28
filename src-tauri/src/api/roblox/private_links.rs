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
        let request = client
            .get(&url)
            .header(COOKIE, cookie_header(security_token))
            .header("Referer", &referer);
        let response = crate::api::auth::send_with_csrf_retry(request, &csrf).await?;

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

    let request = client
        .post(format!("{}/sharelinks/v1/resolve-link", endpoints::host("apis")))
        .header(COOKIE, cookie_header(security_token))
        .header("Content-Type", "application/json")
        .header("Origin", endpoints::host("www"))
        .header("Referer", format!("{}/share-links", endpoints::host("www")))
        .json(&serde_json::json!({
            "linkId": normalized_link_id,
            "linkType": link_type,
        }));
    let response = crate::api::auth::send_with_csrf_retry(request, &csrf).await?;

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
        .map_err(|e| http_client::describe_error(&e))?;

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
        // Escaped JSON: the value is terminated by \" , so stop at the
        // backslash or it ends up inside the code.
        ("\\\"accessCode\\\":\\\"", '\\'),
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

/// The pure link/HTML scraping helpers. They decide which access code a VIP
/// launch uses, so every accepted shape is pinned here.
#[cfg(test)]
mod private_link_parsing_tests {
    use super::*;

    #[test]
    fn url_components_are_decoded_once_and_never_fail() {
        assert_eq!(decode_url_component("a%20b"), "a b");
        assert_eq!(decode_url_component("plain"), "plain");
        // An invalid escape is returned untouched instead of erroring.
        assert_eq!(decode_url_component("100%"), "100%");
    }

    #[test]
    fn query_params_are_matched_case_insensitively_and_decoded() {
        assert_eq!(
            extract_query_param_value("https://x/y?Code=abc%20d", "code").as_deref(),
            Some("abc d")
        );
        assert_eq!(
            extract_query_param_value("?a=1&code=xyz#fragment", "code").as_deref(),
            Some("xyz")
        );
    }

    #[test]
    fn empty_null_and_undefined_query_values_are_ignored() {
        assert!(extract_query_param_value("?code=", "code").is_none());
        assert!(extract_query_param_value("?code=null", "code").is_none());
        assert!(extract_query_param_value("?code=UNDEFINED", "code").is_none());
        assert!(extract_query_param_value("?other=1", "code").is_none());
        assert!(extract_query_param_value("no-query-at-all", "code").is_none());
    }

    /// Links pasted out of a chat client are often encoded twice.
    #[test]
    fn a_double_encoded_link_is_decoded_before_matching() {
        assert_eq!(
            extract_query_param_value_recursive(
                "https%3A%2F%2Fwww.roblox.com%2Fshare%3Fcode%3Ddeep123%26type%3DServer",
                "code"
            )
            .as_deref(),
            Some("deep123")
        );
        assert!(extract_query_param_value_recursive("nothing here", "code").is_none());
    }

    #[test]
    fn the_private_server_link_code_is_read_from_every_accepted_shape() {
        assert_eq!(
            normalize_private_server_link_code(
                "https://www.roblox.com/games/606849621?privateServerLinkCode=1122"
            ),
            "1122"
        );
        assert_eq!(
            normalize_private_server_link_code("https://www.roblox.com/games/1?linkCode=3344"),
            "3344"
        );
        assert_eq!(
            normalize_private_server_link_code("https://www.roblox.com/share?code=5566&type=Server"),
            "5566"
        );
        assert_eq!(
            normalize_private_server_link_code("https://www.roblox.com/share-links?code=7788"),
            "7788"
        );
        assert_eq!(
            normalize_private_server_link_code("roblox://navigation/share_links?code=9900"),
            "9900"
        );
        assert_eq!(normalize_private_server_link_code("code=1357"), "1357");
        assert_eq!(normalize_private_server_link_code("VIP:2468"), "2468");
        assert_eq!(normalize_private_server_link_code("  plain-code  "), "plain-code");
        assert_eq!(normalize_private_server_link_code("   "), "");
    }

    /// A bare `?code=` on a link that is not a share link is not a private
    /// server code — those go through the share-link resolver instead.
    #[test]
    fn a_code_param_on_an_unrelated_link_is_not_treated_as_a_link_code() {
        assert_eq!(
            normalize_private_server_link_code("https://example.com/page?code=1234"),
            "https://example.com/page?code=1234"
        );
    }

    #[test]
    fn access_codes_are_sanitised() {
        assert_eq!(sanitize_access_code("  \"abc\"  ").as_deref(), Some("abc"));
        assert_eq!(sanitize_access_code("'abc'").as_deref(), Some("abc"));
        assert!(sanitize_access_code("   ").is_none());
        assert!(sanitize_access_code("null").is_none());
        assert!(sanitize_access_code("Undefined").is_none());
    }

    #[test]
    fn extract_between_needs_both_ends() {
        assert_eq!(
            extract_between("prefix[value]suffix", "prefix[", ']').as_deref(),
            Some("value")
        );
        assert!(extract_between("no marker here", "prefix[", ']').is_none());
        assert!(extract_between("prefix[unterminated", "prefix[", ']').is_none());
    }

    #[test]
    fn a_quoted_value_after_a_marker_accepts_either_quote() {
        assert_eq!(
            extract_quoted_value_after("call('code-1')", "call(").as_deref(),
            Some("code-1")
        );
        assert_eq!(
            extract_quoted_value_after("call(\"code-2\")", "call(").as_deref(),
            Some("code-2")
        );
        assert!(extract_quoted_value_after("call(no quotes)", "call(").is_none());
    }

    /// The four HTML shapes Roblox has shipped the access code in.
    #[test]
    fn the_access_code_is_scraped_from_every_known_html_shape() {
        assert_eq!(
            extract_access_code("<script>Roblox.GameLauncher.joinPrivateGame(1, 'code-launcher')</script>")
                .as_deref(),
            Some("code-launcher")
        );
        assert_eq!(
            extract_access_code(r#"{"accessCode":"code-json"}"#).as_deref(),
            Some("code-json")
        );
        // Escaped JSON: the value ends at the backslash of `\"`, so the code
        // must come back clean.
        assert_eq!(
            extract_access_code(r#"data-x="{\"accessCode\":\"code-escaped\"}""#).as_deref(),
            Some("code-escaped")
        );
        assert_eq!(
            extract_access_code(r#"<div accessCode="code-attr"></div>"#).as_deref(),
            Some("code-attr")
        );
        assert_eq!(
            extract_access_code("<div accessCode='code-single'></div>").as_deref(),
            Some("code-single")
        );
        assert!(extract_access_code("<html>nothing useful</html>").is_none());
        // A placeholder code is rejected rather than launched with.
        assert!(extract_access_code(r#"{"accessCode":"null"}"#).is_none());
    }
}

/// The networked half of the private-server helpers that
/// `private_link_http_tests` does not cover.
#[cfg(test)]
mod private_link_extra_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server, mount_csrf};
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, ResponseTemplate};

    /// An empty code never reaches the candidate URLs.
    #[tokio::test]
    async fn an_empty_link_code_is_rejected() {
        mount_csrf("vip-empty", "csrf-vip-empty").await;
        assert_eq!(
            parse_private_server_link_code("vip-empty", 1, "   ")
                .await
                .unwrap_err(),
            "Failed to parse private server access code"
        );
    }

    /// The first candidate URL is the game page carrying the link code.
    #[tokio::test]
    async fn the_access_code_is_scraped_off_the_game_page() {
        let server = mock_server().await;
        mount_csrf("vip-scrape", "csrf-vip-scrape").await;

        Mock::given(method("GET"))
            .and(path(mock_path("www", "/games/8100")))
            .and(query_param("privateServerLinkCode", "link-8100"))
            .and(header("cookie", cookie_of("vip-scrape")))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                "<html>Roblox.GameLauncher.joinPrivateGame(8100, 'access-8100')</html>",
                "text/html",
            ))
            .mount(server)
            .await;

        let code = parse_private_server_link_code("vip-scrape", 8100, "link-8100")
            .await
            .expect("access code");
        assert_eq!(code, "access-8100");
    }

    /// Every candidate answering 404 (nothing mounted for this place) ends in
    /// the same error as an unusable code.
    #[tokio::test]
    async fn all_candidates_failing_is_an_error() {
        mount_csrf("vip-none", "csrf-vip-none").await;

        assert_eq!(
            parse_private_server_link_code("vip-none", 8101, "link-8101")
                .await
                .unwrap_err(),
            "Failed to parse private server access code"
        );
    }

    #[tokio::test]
    async fn a_share_payload_without_a_link_id_is_rejected() {
        mount_csrf("share-empty", "csrf-share-empty").await;
        assert_eq!(
            resolve_share_link_payload("share-empty", "   ", "Server")
                .await
                .unwrap_err(),
            "Missing share link code"
        );
    }

    /// Older payloads use `serverData` instead of `privateServerInviteData`.
    #[tokio::test]
    async fn the_legacy_server_data_key_is_accepted() {
        let server = mock_server().await;
        mount_csrf("share-legacy", "csrf-share-legacy").await;

        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/sharelinks/v1/resolve-link")))
            .and(header("cookie", cookie_of("share-legacy")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "serverData": {
                    "status": "Valid",
                    "placeId": 8200,
                    "linkCode": "legacy-code"
                }
            })))
            .mount(server)
            .await;

        let (place_id, link_code) = resolve_share_server_link("share-legacy", "code-legacy")
            .await
            .expect("resolved");
        assert_eq!(place_id, Some(8200));
        assert_eq!(link_code, "legacy-code");
    }

    /// An expired server link still carries a usable code, so it resolves.
    #[tokio::test]
    async fn an_expired_server_link_still_resolves() {
        let server = mock_server().await;
        mount_csrf("share-expired", "csrf-share-expired").await;

        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/sharelinks/v1/resolve-link")))
            .and(header("cookie", cookie_of("share-expired")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "privateServerInviteData": {
                    "status": "Expired",
                    "placeId": 8201,
                    "linkCode": "expired-code"
                }
            })))
            .mount(server)
            .await;

        let (place_id, link_code) = resolve_share_server_link("share-expired", "code-expired")
            .await
            .expect("resolved");
        assert_eq!(place_id, Some(8201));
        assert_eq!(link_code, "expired-code");
    }

    #[tokio::test]
    async fn an_unusable_status_without_a_code_is_rejected() {
        let server = mock_server().await;
        mount_csrf("share-invalid", "csrf-share-invalid").await;

        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/sharelinks/v1/resolve-link")))
            .and(header("cookie", cookie_of("share-invalid")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "privateServerInviteData": { "status": "Inaccessible" }
            })))
            .mount(server)
            .await;

        assert_eq!(
            resolve_share_server_link("share-invalid", "code-invalid")
                .await
                .unwrap_err(),
            "Share link is not valid for server launch"
        );
    }

    #[tokio::test]
    async fn a_valid_payload_without_a_link_code_is_rejected() {
        let server = mock_server().await;
        mount_csrf("share-nocode", "csrf-share-nocode").await;

        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/sharelinks/v1/resolve-link")))
            .and(header("cookie", cookie_of("share-nocode")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "privateServerInviteData": { "status": "Valid", "placeId": 8202, "linkCode": "   " }
            })))
            .mount(server)
            .await;

        assert_eq!(
            resolve_share_server_link("share-nocode", "code-nocode")
                .await
                .unwrap_err(),
            "Share link did not return a private server link code"
        );
    }

    /// A failing universe lookup fails the whole resolution instead of
    /// launching into the wrong place.
    #[tokio::test]
    async fn a_failing_universe_lookup_is_reported() {
        let server = mock_server().await;
        mount_csrf("share-universe-fail", "csrf-share-universe-fail").await;

        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/sharelinks/v1/resolve-link")))
            .and(header("cookie", cookie_of("share-universe-fail")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "privateServerInviteData": {
                    "status": "Valid",
                    "universeId": 8300,
                    "linkCode": "universe-code"
                }
            })))
            .mount(server)
            .await;

        Mock::given(method("GET"))
            .and(path(mock_path("games", "/v1/games")))
            .and(query_param("universeIds", "8300"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(server)
            .await;

        let err = resolve_share_server_link("share-universe-fail", "code-universe-fail")
            .await
            .unwrap_err();
        assert!(
            err.starts_with("Failed to resolve root place from universe (status 500)"),
            "unexpected error: {}",
            err
        );
    }

    /// A universe with no root place resolves to "no place", which the caller
    /// turns into an error of its own.
    #[tokio::test]
    async fn a_universe_without_a_root_place_resolves_to_none() {
        let server = mock_server().await;
        mount_csrf("share-universe-empty", "csrf-share-universe-empty").await;

        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/sharelinks/v1/resolve-link")))
            .and(header("cookie", cookie_of("share-universe-empty")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "privateServerInviteData": {
                    "status": "Valid",
                    "universeId": 8301,
                    "linkCode": "universe-code-2"
                }
            })))
            .mount(server)
            .await;

        Mock::given(method("GET"))
            .and(path(mock_path("games", "/v1/games")))
            .and(query_param("universeIds", "8301"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "data": [] })),
            )
            .mount(server)
            .await;

        let (place_id, link_code) =
            resolve_share_server_link("share-universe-empty", "code-universe-empty")
                .await
                .expect("resolved");
        assert!(place_id.is_none());
        assert_eq!(link_code, "universe-code-2");
    }

    /// The resolver accepts a full share URL and digs the code out of it.
    #[tokio::test]
    async fn a_full_share_url_is_reduced_to_its_code() {
        let server = mock_server().await;
        mount_csrf("share-url", "csrf-share-url").await;

        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/sharelinks/v1/resolve-link")))
            .and(header("cookie", cookie_of("share-url")))
            .and(wiremock::matchers::body_partial_json(serde_json::json!({
                "linkId": "url-code-1",
                "linkType": "Server"
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "ok": true })),
            )
            .mount(server)
            .await;

        let body = resolve_share_link_payload(
            "share-url",
            "https://www.roblox.com/share?code=url-code-1&type=Server",
            "Server",
        )
        .await
        .expect("payload");
        assert_eq!(body["ok"], serde_json::json!(true));
    }
}
