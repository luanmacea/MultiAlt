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
