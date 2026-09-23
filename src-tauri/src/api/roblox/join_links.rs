// Resolving any pasted Roblox join link into a launchable target.
//
// Supported shapes (see `parse_join_link` tests):
// - Experience invites: `roblox.com/share?code=<code>&type=ExperienceInvite`
// - Private/VIP servers: `roblox.com/games/<id>?privateServerLinkCode=<code>`,
//   `share?code=<code>&type=Server`, `vip:<code>`
// - Plain games/servers: `roblox.com/games/<id>`, `games/start?placeId=`,
//   `...&gameInstanceId=<jobId>`
// - Deep links: `roblox://experiences/start?placeId=...`,
//   `roblox://navigation/share_links?type=...&code=...`
// - `ro.blox.com` short links (AppsFlyer `af_dp`/`af_web_dp`, or one redirect)

/// A launch target resolved from a link, in the shape the launch commands take.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinTarget {
    /// "invite" | "private" | "job" | "place"
    pub kind: String,
    pub place_id: i64,
    /// Roblox `instanceId`/`gameInstanceId`; empty when the link has none.
    pub job_id: String,
    pub access_code: String,
    pub link_code: String,
    pub launch_data: String,
    pub inviter_id: Option<i64>,
    /// Non-fatal warning, e.g. an invite whose status is not `Valid`.
    pub note: Option<String>,
}

/// Everything that can be read from a link without calling Roblox.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedJoinLink {
    pub share_code: Option<String>,
    /// Share link type as written in the link ("ExperienceInvite", "Server"...).
    pub share_type: Option<String>,
    pub place_id: Option<i64>,
    pub job_id: Option<String>,
    pub link_code: Option<String>,
    pub launch_data: Option<String>,
}

impl ParsedJoinLink {
    fn is_empty(&self) -> bool {
        self.share_code.is_none()
            && self.place_id.is_none()
            && self.job_id.is_none()
            && self.link_code.is_none()
    }
}

fn join_link_clean_value(value: &str) -> Option<String> {
    let decoded = decode_url_component(value.trim());
    let trimmed = decoded.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("null")
        || trimmed.eq_ignore_ascii_case("undefined")
    {
        return None;
    }
    Some(trimmed.to_string())
}

fn join_link_first_param(input: &str, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| extract_query_param_value_recursive(input, key))
        .and_then(|v| join_link_clean_value(&v))
}

/// `roblox://navigation/share_links/<type>/<code>` (path form).
fn join_link_share_path_segments(input: &str) -> Option<(String, String)> {
    let lower = input.to_ascii_lowercase();
    let marker = "share_links/";
    let start = lower.find(marker)? + marker.len();
    let rest = input.get(start..)?;
    let mut parts = rest
        .split(['/', '?', '&', '#'])
        .filter(|p| !p.trim().is_empty());
    let link_type = parts.next()?.trim().to_string();
    let code = parts.next()?.trim().to_string();
    if link_type.is_empty() || code.is_empty() {
        return None;
    }
    Some((link_type, code))
}

fn join_link_looks_like_share_code(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.len() >= 20
        && trimmed.len() <= 80
        && trimmed.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && trimmed.chars().any(|c| c.is_ascii_alphabetic())
}

fn join_link_place_id(input: &str) -> Option<i64> {
    if let Some(value) = join_link_first_param(input, &["placeId", "placeid"]) {
        if let Ok(id) = value.parse::<i64>() {
            if id > 0 {
                return Some(id);
            }
        }
    }
    // .../games/<placeId>/Name
    let lower = input.to_ascii_lowercase();
    for marker in ["/games/", "/experiences/"] {
        if let Some(idx) = lower.find(marker) {
            let rest = input.get(idx + marker.len()..)?;
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(id) = digits.parse::<i64>() {
                if id > 0 {
                    return Some(id);
                }
            }
        }
    }
    None
}

/// Reads everything a link carries on its own. Never touches the network, so
/// it is exhaustively unit-tested.
pub fn parse_join_link(raw: &str) -> ParsedJoinLink {
    let trimmed = raw.trim();
    let mut parsed = ParsedJoinLink::default();
    if trimmed.is_empty() {
        return parsed;
    }

    // AppsFlyer short links carry the real deep link inside af_dp/af_web_dp.
    let inner = join_link_first_param(trimmed, &["af_dp", "af_web_dp"]);
    let input: &str = inner.as_deref().unwrap_or(trimmed);

    if let Some((link_type, code)) = join_link_share_path_segments(input) {
        parsed.share_type = Some(link_type);
        parsed.share_code = Some(code);
    } else if let Some(code) = join_link_first_param(input, &["code", "linkId"]) {
        let is_share_link = {
            let lower = input.to_ascii_lowercase();
            lower.contains("/share")
                || lower.contains("share_links")
                || lower.contains("sharelinks")
                || lower.contains("type=")
        };
        if is_share_link {
            parsed.share_code = Some(code);
            parsed.share_type = join_link_first_param(input, &["type", "linkType"]);
        }
    }

    if let Some(code) = join_link_first_param(input, &["privateServerLinkCode", "linkCode"]) {
        parsed.link_code = Some(code);
    } else if let Some(rest) = input.get(..4).and_then(|head| {
        head.eq_ignore_ascii_case("vip:")
            .then(|| input.get(4..))
            .flatten()
    }) {
        parsed.link_code = join_link_clean_value(rest);
    }

    if let Some(job) = join_link_first_param(input, &["gameInstanceId", "gameId", "jobId"]) {
        parsed.job_id = Some(job);
    }
    if let Some(data) = join_link_first_param(input, &["launchData", "launchdata"]) {
        parsed.launch_data = Some(data);
    }
    parsed.place_id = join_link_place_id(input);

    // A bare code pasted without any URL around it.
    if parsed.is_empty() && join_link_looks_like_share_code(input) {
        parsed.share_code = Some(input.trim().to_string());
    }

    parsed
}

fn join_link_is_invite_type(link_type: Option<&str>) -> bool {
    link_type
        .map(|t| t.eq_ignore_ascii_case("ExperienceInvite"))
        .unwrap_or(false)
}

fn join_link_is_server_type(link_type: Option<&str>) -> bool {
    link_type
        .map(|t| t.eq_ignore_ascii_case("Server") || t.eq_ignore_ascii_case("PrivateServer"))
        .unwrap_or(false)
}

/// Follows one redirect for `ro.blox.com`-style short links.
async fn join_link_follow_short_link(url: &str) -> Option<String> {
    let response = no_redirect_client().get(url).send().await.ok()?;
    let location = response.headers().get(reqwest::header::LOCATION)?;
    location.to_str().ok().map(|v| v.to_string())
}

fn join_link_target_from_invite(data: &serde_json::Value) -> Option<JoinTarget> {
    let place_id = data["placeId"].as_i64().unwrap_or_default();
    if place_id <= 0 {
        return None;
    }
    let status = data["status"].as_str().unwrap_or_default().to_string();
    Some(JoinTarget {
        kind: "invite".to_string(),
        place_id,
        job_id: data["instanceId"]
            .as_str()
            .or_else(|| data["gameInstanceId"].as_str())
            .unwrap_or_default()
            .trim()
            .to_string(),
        access_code: String::new(),
        link_code: String::new(),
        launch_data: data["launchData"].as_str().unwrap_or_default().to_string(),
        inviter_id: data["inviterId"].as_i64(),
        note: (!status.is_empty() && !status.eq_ignore_ascii_case("Valid")).then_some(status),
    })
}

/// Resolves a pasted link into a launch target, calling Roblox only when the
/// link cannot be understood on its own (share codes).
pub async fn resolve_join_link(security_token: &str, raw: &str) -> Result<JoinTarget, String> {
    let mut parsed = parse_join_link(raw);

    // Short link with nothing inline: follow it once and parse the destination.
    if parsed.is_empty() {
        let trimmed = raw.trim();
        if trimmed.to_ascii_lowercase().starts_with("http") {
            if let Some(location) = join_link_follow_short_link(trimmed).await {
                parsed = parse_join_link(&location);
            }
        }
    }

    if let Some(code) = parsed.share_code.clone() {
        let link_type = parsed.share_type.as_deref();

        if !join_link_is_server_type(link_type) {
            // Invite first (unknown types included): it is the common share link.
            match resolve_share_link_payload(security_token, &code, "ExperienceInvite").await {
                Ok(body) => {
                    if let Some(mut target) =
                        join_link_target_from_invite(&body["experienceInviteData"])
                    {
                        if target.launch_data.is_empty() {
                            target.launch_data = parsed.launch_data.clone().unwrap_or_default();
                        }
                        return Ok(target);
                    }
                    if join_link_is_invite_type(link_type) {
                        let status = body["experienceInviteData"]["status"]
                            .as_str()
                            .unwrap_or("unknown");
                        return Err(format!("Invite link is not usable (status: {})", status));
                    }
                }
                Err(e) if join_link_is_invite_type(link_type) => return Err(e),
                Err(_) => {}
            }
        }

        // Server/private share link (also the fallback for unknown types).
        let (place_id, link_code) = resolve_share_server_link(security_token, &code).await?;
        let place_id = place_id.or(parsed.place_id).unwrap_or_default();
        if place_id <= 0 {
            return Err("Could not determine the place for this link".to_string());
        }
        return Ok(JoinTarget {
            kind: "private".to_string(),
            place_id,
            link_code,
            launch_data: parsed.launch_data.unwrap_or_default(),
            ..Default::default()
        });
    }

    if let Some(link_code) = parsed.link_code.clone() {
        let place_id = parsed.place_id.unwrap_or_default();
        if place_id <= 0 {
            return Err(
                "Private server link without a place: paste the full link from the browser."
                    .to_string(),
            );
        }
        return Ok(JoinTarget {
            kind: "private".to_string(),
            place_id,
            link_code,
            launch_data: parsed.launch_data.unwrap_or_default(),
            ..Default::default()
        });
    }

    let place_id = parsed.place_id.unwrap_or_default();
    if place_id <= 0 {
        return Err("Link not recognized. Paste a Roblox game, invite or private server link.".to_string());
    }
    let job_id = parsed.job_id.unwrap_or_default();
    Ok(JoinTarget {
        kind: if job_id.is_empty() { "place" } else { "job" }.to_string(),
        place_id,
        job_id,
        launch_data: parsed.launch_data.unwrap_or_default(),
        ..Default::default()
    })
}

#[cfg(test)]
mod join_link_tests {
    use super::*;

    #[test]
    fn parses_experience_invite_share_link() {
        let parsed = parse_join_link(
            "https://www.roblox.com/share?code=_63jb88b8ck3k7p3zf8w2n7i34pdznbcnxvnczrizs6fc00qzm4&type=ExperienceInvite",
        );
        assert_eq!(
            parsed.share_code.as_deref(),
            Some("_63jb88b8ck3k7p3zf8w2n7i34pdznbcnxvnczrizs6fc00qzm4")
        );
        assert_eq!(parsed.share_type.as_deref(), Some("ExperienceInvite"));
        assert!(parsed.link_code.is_none());
    }

    #[test]
    fn parses_server_share_link() {
        let parsed = parse_join_link("https://www.roblox.com/share-links?code=abc123def&type=Server");
        assert_eq!(parsed.share_code.as_deref(), Some("abc123def"));
        assert_eq!(parsed.share_type.as_deref(), Some("Server"));
    }

    #[test]
    fn parses_private_server_link_with_place() {
        let parsed = parse_join_link(
            "https://www.roblox.com/games/606849621/Jailbreak?privateServerLinkCode=8811223344",
        );
        assert_eq!(parsed.place_id, Some(606849621));
        assert_eq!(parsed.link_code.as_deref(), Some("8811223344"));
        assert!(parsed.share_code.is_none());
    }

    #[test]
    fn parses_vip_prefix() {
        let parsed = parse_join_link("vip:9988776655");
        assert_eq!(parsed.link_code.as_deref(), Some("9988776655"));
    }

    #[test]
    fn parses_game_link_with_instance() {
        let parsed = parse_join_link(
            "https://www.roblox.com/games/start?placeId=606849621&gameInstanceId=1a2b3c4d-1111-2222-3333-444455556666",
        );
        assert_eq!(parsed.place_id, Some(606849621));
        assert_eq!(
            parsed.job_id.as_deref(),
            Some("1a2b3c4d-1111-2222-3333-444455556666")
        );
    }

    #[test]
    fn parses_plain_game_link() {
        let parsed = parse_join_link("https://www.roblox.com/games/920587237/Adopt-Me");
        assert_eq!(parsed.place_id, Some(920587237));
        assert!(parsed.job_id.is_none());
        assert!(parsed.link_code.is_none());
    }

    #[test]
    fn parses_deep_link_share_path_form() {
        let parsed = parse_join_link("roblox://navigation/share_links/ExperienceInvite/zzz111code222");
        assert_eq!(parsed.share_type.as_deref(), Some("ExperienceInvite"));
        assert_eq!(parsed.share_code.as_deref(), Some("zzz111code222"));
    }

    #[test]
    fn parses_appsflyer_short_link_payload() {
        let parsed = parse_join_link(
            "https://ro.blox.com/Ebh5?af_dp=roblox%3A%2F%2Fnavigation%2Fshare_links%3Fcode%3Dshortcode123456%26type%3DExperienceInvite",
        );
        assert_eq!(parsed.share_code.as_deref(), Some("shortcode123456"));
        assert_eq!(parsed.share_type.as_deref(), Some("ExperienceInvite"));
    }

    #[test]
    fn parses_launch_data() {
        let parsed = parse_join_link(
            "https://www.roblox.com/games/start?placeId=1234&launchData=hello%20world",
        );
        assert_eq!(parsed.launch_data.as_deref(), Some("hello world"));
    }

    #[test]
    fn parses_bare_share_code() {
        let parsed =
            parse_join_link("_63jb88b8ck3k7p3zf8w2n7i34pdznbcnxvnczrizs6fc00qzm4");
        assert_eq!(
            parsed.share_code.as_deref(),
            Some("_63jb88b8ck3k7p3zf8w2n7i34pdznbcnxvnczrizs6fc00qzm4")
        );
    }

    #[test]
    fn ignores_empty_and_junk_input() {
        assert!(parse_join_link("   ").is_empty());
        assert!(parse_join_link("hello").is_empty());
        assert!(parse_join_link("https://example.com/whatever").is_empty());
    }

    #[test]
    fn ignores_null_query_values() {
        let parsed = parse_join_link("https://www.roblox.com/games/start?placeId=1234&gameInstanceId=null");
        assert_eq!(parsed.place_id, Some(1234));
        assert!(parsed.job_id.is_none());
    }

    #[test]
    fn invite_payload_maps_instance_id_to_job() {
        let body = serde_json::json!({
            "status": "Valid",
            "inviterId": 42,
            "placeId": 606849621,
            "instanceId": "job-abc",
            "launchData": "data"
        });
        let target = join_link_target_from_invite(&body).expect("target");
        assert_eq!(target.kind, "invite");
        assert_eq!(target.place_id, 606849621);
        assert_eq!(target.job_id, "job-abc");
        assert_eq!(target.launch_data, "data");
        assert_eq!(target.inviter_id, Some(42));
        assert!(target.note.is_none());
    }

    #[test]
    fn invite_payload_reports_non_valid_status_as_note() {
        let body = serde_json::json!({
            "status": "InviterNotInExperience",
            "placeId": 55,
            "instanceId": ""
        });
        let target = join_link_target_from_invite(&body).expect("target");
        assert_eq!(target.note.as_deref(), Some("InviterNotInExperience"));
        assert_eq!(target.place_id, 55);
        assert!(target.job_id.is_empty());
    }

    #[test]
    fn invite_payload_without_place_is_rejected() {
        let body = serde_json::json!({ "status": "Expired" });
        assert!(join_link_target_from_invite(&body).is_none());
    }
}

/// Networked half of `resolve_join_link`. The pure parser above is covered by
/// `join_link_tests`; these drive the share-link resolver against the shared
/// mock server.
#[cfg(test)]
mod join_link_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server, mount_csrf};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    fn invite_link(code: &str) -> String {
        format!("https://www.roblox.com/share?code={}&type=ExperienceInvite", code)
    }

    /// Answers the ExperienceInvite resolve-link call for one account token.
    async fn mount_invite(token: &str, invite_data: serde_json::Value) {
        mount_csrf(token, &format!("csrf-{}", token)).await;
        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/sharelinks/v1/resolve-link")))
            .and(header("cookie", cookie_of(token)))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({ "experienceInviteData": invite_data }),
            ))
            .mount(mock_server().await)
            .await;
    }

    #[tokio::test]
    async fn resolves_a_valid_experience_invite() {
        mount_invite(
            "join-invite-valid",
            serde_json::json!({
                "status": "Valid",
                "inviterId": 42,
                "placeId": 606849621,
                "instanceId": "job-abc",
                "launchData": "payload"
            }),
        )
        .await;

        let target = resolve_join_link("join-invite-valid", &invite_link("code-valid"))
            .await
            .expect("join target");
        assert_eq!(target.kind, "invite");
        assert_eq!(target.place_id, 606849621);
        assert_eq!(target.job_id, "job-abc");
        assert_eq!(target.launch_data, "payload");
        assert_eq!(target.inviter_id, Some(42));
        assert!(target.note.is_none());
    }

    #[tokio::test]
    async fn expired_invite_still_resolves_but_carries_a_note() {
        mount_invite(
            "join-invite-expired",
            serde_json::json!({
                "status": "Expired",
                "placeId": 606849621,
                "instanceId": "job-expired"
            }),
        )
        .await;

        let target = resolve_join_link("join-invite-expired", &invite_link("code-expired"))
            .await
            .expect("join target");
        assert_eq!(target.kind, "invite");
        assert_eq!(target.note.as_deref(), Some("Expired"));
    }

    #[tokio::test]
    async fn inviter_not_in_experience_is_reported_as_a_note() {
        mount_invite(
            "join-invite-absent",
            serde_json::json!({
                "status": "InviterNotInExperience",
                "placeId": 55,
                "instanceId": ""
            }),
        )
        .await;

        let target = resolve_join_link("join-invite-absent", &invite_link("code-absent"))
            .await
            .expect("join target");
        assert_eq!(target.place_id, 55);
        assert!(target.job_id.is_empty());
        assert_eq!(target.note.as_deref(), Some("InviterNotInExperience"));
    }

    #[tokio::test]
    async fn invite_without_a_place_is_an_error() {
        mount_invite(
            "join-invite-no-place",
            serde_json::json!({ "status": "Expired" }),
        )
        .await;

        let err = resolve_join_link("join-invite-no-place", &invite_link("code-no-place"))
            .await
            .unwrap_err();
        assert_eq!(err, "Invite link is not usable (status: Expired)");
    }
}

/// The small parsing helpers behind `parse_join_link`, and the link shapes
/// `join_link_tests` does not already pin.
#[cfg(test)]
mod join_link_helper_tests {
    use super::*;

    #[test]
    fn values_are_decoded_and_placeholders_dropped() {
        assert_eq!(join_link_clean_value(" a%20b ").as_deref(), Some("a b"));
        assert!(join_link_clean_value("   ").is_none());
        assert!(join_link_clean_value("null").is_none());
        assert!(join_link_clean_value("UNDEFINED").is_none());
    }

    #[test]
    fn the_first_matching_param_wins() {
        assert_eq!(
            join_link_first_param("?b=second&a=first", &["a", "b"]).as_deref(),
            Some("first")
        );
        assert_eq!(
            join_link_first_param("?b=second", &["a", "b"]).as_deref(),
            Some("second")
        );
        assert!(join_link_first_param("?c=third", &["a", "b"]).is_none());
    }

    #[test]
    fn the_share_link_path_form_needs_both_segments() {
        assert_eq!(
            join_link_share_path_segments("roblox://navigation/share_links/Server/code-1"),
            Some(("Server".to_string(), "code-1".to_string()))
        );
        // Query form, not path form: no segments to read.
        assert!(join_link_share_path_segments("roblox://navigation/share_links?code=x").is_none());
        assert!(join_link_share_path_segments("roblox://navigation/share_links/Server").is_none());
        assert!(join_link_share_path_segments("https://www.roblox.com/games/1").is_none());
    }

    /// The bare-code heuristic must not swallow ordinary words or ids.
    #[test]
    fn a_bare_share_code_is_recognised_by_shape() {
        assert!(join_link_looks_like_share_code(
            "_63jb88b8ck3k7p3zf8w2n7i34pdznbcnxvnczrizs6fc00qzm4"
        ));
        assert!(!join_link_looks_like_share_code("short"));
        assert!(!join_link_looks_like_share_code(&"1".repeat(30)));
        assert!(!join_link_looks_like_share_code(&"a".repeat(81)));
        assert!(!join_link_looks_like_share_code(
            "has spaces in it and is long enough"
        ));
    }

    #[test]
    fn the_place_id_is_read_from_the_param_or_the_path() {
        assert_eq!(join_link_place_id("?placeId=1234"), Some(1234));
        assert_eq!(join_link_place_id("?placeid=1234"), Some(1234));
        assert_eq!(
            join_link_place_id("https://www.roblox.com/games/606849621/Jailbreak"),
            Some(606_849_621)
        );
        assert_eq!(
            join_link_place_id("roblox://experiences/start?placeId=920587237"),
            Some(920_587_237)
        );
        assert_eq!(join_link_place_id("?placeId=0"), None);
        assert_eq!(join_link_place_id("?placeId=abc"), None);
        assert_eq!(join_link_place_id("https://example.com/"), None);
    }

    #[test]
    fn share_types_are_matched_case_insensitively() {
        assert!(join_link_is_invite_type(Some("experienceinvite")));
        assert!(!join_link_is_invite_type(Some("Server")));
        assert!(!join_link_is_invite_type(None));

        assert!(join_link_is_server_type(Some("server")));
        assert!(join_link_is_server_type(Some("PrivateServer")));
        assert!(!join_link_is_server_type(Some("ExperienceInvite")));
        assert!(!join_link_is_server_type(None));
    }

    #[test]
    fn a_vip_prefix_is_url_decoded() {
        let parsed = parse_join_link("vip:code%20with%20spaces");
        assert_eq!(parsed.link_code.as_deref(), Some("code with spaces"));
    }

    #[test]
    fn the_lowercase_launch_data_spelling_is_accepted() {
        let parsed = parse_join_link("https://www.roblox.com/games/start?placeId=12&launchdata=abc");
        assert_eq!(parsed.launch_data.as_deref(), Some("abc"));
    }

    #[test]
    fn a_job_id_param_is_accepted_under_all_three_names() {
        for key in ["gameInstanceId", "gameId", "jobId"] {
            let parsed =
                parse_join_link(&format!("https://www.roblox.com/games/start?placeId=12&{}=job-x", key));
            assert_eq!(parsed.job_id.as_deref(), Some("job-x"), "key: {}", key);
        }
    }

    /// `is_empty` ignores `launch_data`: a link that only carries launch data
    /// is still "nothing to join".
    #[test]
    fn a_link_with_only_launch_data_counts_as_empty() {
        let parsed = parse_join_link("https://example.com/?launchData=abc");
        assert!(parsed.is_empty());
        assert_eq!(parsed.launch_data.as_deref(), Some("abc"));
    }
}

/// `resolve_join_link` end to end: the branches that need no network, plus the
/// share-link fallbacks and the short-link redirect.
#[cfg(test)]
mod join_link_resolve_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server, mount_csrf};
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    /// A private-server link that carries its own place never calls Roblox.
    #[tokio::test]
    async fn a_private_server_link_resolves_without_a_request() {
        let target = resolve_join_link(
            "unused-token",
            "https://www.roblox.com/games/606849621/Jailbreak?privateServerLinkCode=8811",
        )
        .await
        .expect("target");
        assert_eq!(target.kind, "private");
        assert_eq!(target.place_id, 606_849_621);
        assert_eq!(target.link_code, "8811");
    }

    #[tokio::test]
    async fn a_link_code_without_a_place_asks_for_the_full_link() {
        let err = resolve_join_link("unused-token", "vip:8811")
            .await
            .unwrap_err();
        assert_eq!(
            err,
            "Private server link without a place: paste the full link from the browser."
        );
    }

    #[tokio::test]
    async fn a_plain_game_link_becomes_a_place_target() {
        let target = resolve_join_link("unused-token", "https://www.roblox.com/games/920587237")
            .await
            .expect("target");
        assert_eq!(target.kind, "place");
        assert_eq!(target.place_id, 920_587_237);
        assert!(target.job_id.is_empty());
    }

    #[tokio::test]
    async fn a_link_with_an_instance_becomes_a_job_target() {
        let target = resolve_join_link(
            "unused-token",
            "https://www.roblox.com/games/start?placeId=1234&gameInstanceId=job-77&launchData=ld",
        )
        .await
        .expect("target");
        assert_eq!(target.kind, "job");
        assert_eq!(target.place_id, 1234);
        assert_eq!(target.job_id, "job-77");
        assert_eq!(target.launch_data, "ld");
    }

    #[tokio::test]
    async fn an_unrecognised_link_is_rejected() {
        let err = resolve_join_link("unused-token", "hello there")
            .await
            .unwrap_err();
        assert_eq!(
            err,
            "Link not recognized. Paste a Roblox game, invite or private server link."
        );
    }

    /// An invite payload without launch data inherits the launch data written
    /// on the link itself.
    #[tokio::test]
    async fn the_links_launch_data_is_kept_when_the_invite_has_none() {
        let server = mock_server().await;
        mount_csrf("join-ld", "csrf-join-ld").await;

        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/sharelinks/v1/resolve-link")))
            .and(header("cookie", cookie_of("join-ld")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "experienceInviteData": { "status": "Valid", "placeId": 606849621 }
            })))
            .mount(server)
            .await;

        let target = resolve_join_link(
            "join-ld",
            "https://www.roblox.com/share?code=code-ld&type=ExperienceInvite&launchData=from-link",
        )
        .await
        .expect("target");
        assert_eq!(target.launch_data, "from-link");
    }

    /// A `type=Server` share link skips the invite resolver entirely.
    #[tokio::test]
    async fn a_server_share_link_goes_straight_to_the_server_resolver() {
        let server = mock_server().await;
        mount_csrf("join-server", "csrf-join-server").await;

        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/sharelinks/v1/resolve-link")))
            .and(header("cookie", cookie_of("join-server")))
            .and(body_partial_json(
                serde_json::json!({ "linkType": "Server" }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "privateServerInviteData": {
                    "status": "Valid",
                    "placeId": 606849621,
                    "linkCode": "server-code"
                }
            })))
            .mount(server)
            .await;

        let target = resolve_join_link(
            "join-server",
            "https://www.roblox.com/share-links?code=code-server&type=Server",
        )
        .await
        .expect("target");
        assert_eq!(target.kind, "private");
        assert_eq!(target.place_id, 606_849_621);
        assert_eq!(target.link_code, "server-code");
    }

    /// An unknown share type tries the invite resolver first and falls back to
    /// the server one when the payload holds no invite.
    #[tokio::test]
    async fn an_unknown_share_type_falls_back_to_the_server_resolver() {
        let server = mock_server().await;
        mount_csrf("join-unknown", "csrf-join-unknown").await;

        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/sharelinks/v1/resolve-link")))
            .and(header("cookie", cookie_of("join-unknown")))
            .and(body_partial_json(
                serde_json::json!({ "linkType": "ExperienceInvite" }),
            ))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "other": true })),
            )
            .mount(server)
            .await;

        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/sharelinks/v1/resolve-link")))
            .and(header("cookie", cookie_of("join-unknown")))
            .and(body_partial_json(
                serde_json::json!({ "linkType": "Server" }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "privateServerInviteData": {
                    "status": "Valid",
                    "placeId": 4242,
                    "linkCode": "fallback-code"
                }
            })))
            .mount(server)
            .await;

        let target = resolve_join_link(
            "join-unknown",
            "https://www.roblox.com/share?code=code-unknown&type=Mystery",
        )
        .await
        .expect("target");
        assert_eq!(target.kind, "private");
        assert_eq!(target.place_id, 4242);
        assert_eq!(target.link_code, "fallback-code");
    }

    /// An invite-typed link reports the resolver's own error instead of
    /// retrying as a server link.
    #[tokio::test]
    async fn an_invite_link_reports_a_failing_resolver() {
        let server = mock_server().await;
        mount_csrf("join-invite-500", "csrf-join-invite-500").await;

        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/sharelinks/v1/resolve-link")))
            .and(header("cookie", cookie_of("join-invite-500")))
            .respond_with(ResponseTemplate::new(500).set_body_string("resolver down"))
            .mount(server)
            .await;

        let err = resolve_join_link(
            "join-invite-500",
            "https://www.roblox.com/share?code=code-500&type=ExperienceInvite",
        )
        .await
        .unwrap_err();
        assert!(err.contains("status 500"), "unexpected error: {}", err);
    }

    /// A `ro.blox.com` style short link is followed once and the destination is
    /// parsed in its place.
    #[tokio::test]
    async fn a_short_link_is_followed_once() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("www", "/short/join-target")))
            .respond_with(ResponseTemplate::new(302).insert_header(
                "location",
                "https://www.roblox.com/games/start?placeId=555&gameInstanceId=job-short",
            ))
            .mount(server)
            .await;

        let target = resolve_join_link(
            "unused-token",
            &format!("{}/short/join-target", endpoints::host("www")),
        )
        .await
        .expect("target");
        assert_eq!(target.kind, "job");
        assert_eq!(target.place_id, 555);
        assert_eq!(target.job_id, "job-short");
    }

    /// A short link that does not redirect stays unrecognised.
    #[tokio::test]
    async fn a_short_link_without_a_location_is_rejected() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("www", "/short/dead-end")))
            .respond_with(ResponseTemplate::new(200).set_body_string("no redirect"))
            .mount(server)
            .await;

        let err = resolve_join_link(
            "unused-token",
            &format!("{}/short/dead-end", endpoints::host("www")),
        )
        .await
        .unwrap_err();
        assert_eq!(
            err,
            "Link not recognized. Paste a Roblox game, invite or private server link."
        );
    }
}
