use std::future::Future;

fn session_relogin_error() -> String {
    "Roblox invalidated this session. Re-login required.".to_string()
}

fn is_auth_session_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("status 401")
        || lower.contains("[401")
        || lower.contains("unauthorized")
        || lower.contains("not authenticated")
        || lower.contains("user is not authenticated")
        || lower.contains("invalid cookie")
        || lower.contains("authorization has been denied")
        || lower.contains("\"code\":9002")
        || lower.contains("code 9002")
}

fn get_account(state: &AccountStore, user_id: i64) -> Result<crate::data::accounts::Account, String> {
    state
        .get_all()?
        .into_iter()
        .find(|a| a.user_id == user_id)
        .ok_or_else(|| format!("Account {} not found", user_id))
}

fn mark_refresh_attempt(
    state: &AccountStore,
    user_id: i64,
) -> Result<crate::data::accounts::Account, String> {
    let mut account = get_account(state, user_id)?;
    account.last_attempted_refresh = chrono::Utc::now();
    state.update(account.clone())?;
    Ok(account)
}

fn persist_cookie_update(state: &AccountStore, user_id: i64, new_cookie: &str) -> Result<(), String> {
    let mut account = get_account(state, user_id)?;
    account.security_token = new_cookie.to_string();
    account.valid = true;
    state.update(account)?;
    Ok(())
}

async fn refresh_account_session(state: &AccountStore, user_id: i64) -> Result<String, String> {
    let account = mark_refresh_attempt(state, user_id)?;
    let result = api::auth::log_out_other_sessions(&account.security_token).await?;
    if !result.success {
        return Err(session_relogin_error());
    }
    let new_cookie = result
        .new_cookie
        .filter(|cookie| !cookie.trim().is_empty())
        .ok_or_else(session_relogin_error)?;
    persist_cookie_update(state, user_id, &new_cookie)?;
    Ok(new_cookie)
}

async fn run_with_session_retry<T, F, Fut>(
    state: &AccountStore,
    user_id: i64,
    mut operation: F,
) -> Result<T, String>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let cookie = get_cookie(state, user_id)?;
    match operation(cookie).await {
        Ok(value) => Ok(value),
        Err(error) => {
            if !is_auth_session_error(&error) {
                return Err(error);
            }

            let refreshed_cookie = refresh_account_session(state, user_id)
                .await
                .map_err(|refresh_error| {
                    if is_auth_session_error(&refresh_error) {
                        session_relogin_error()
                    } else {
                        refresh_error
                    }
                })?;

            match operation(refreshed_cookie).await {
                Ok(value) => Ok(value),
                Err(retry_error) => {
                    if is_auth_session_error(&retry_error) {
                        Err(session_relogin_error())
                    } else {
                        Err(retry_error)
                    }
                }
            }
        }
    }
}

/// Turns any pasted Roblox link (experience invite, VIP/private server, plain
/// game or deep link) into a launch target for the launch commands.
/// Uses the account's cookie only for share codes, which need a resolve call.
#[tauri::command]
async fn resolve_join_link(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    link: String,
) -> Result<api::roblox::JoinTarget, String> {
    // Links with everything inline (place/job/link code) need no account.
    let parsed = api::roblox::parse_join_link(&link);
    if parsed.share_code.is_none() {
        return api::roblox::resolve_join_link("", &link).await;
    }
    let link = link.clone();
    run_with_session_retry(state.inner(), user_id, move |cookie| {
        let link = link.clone();
        async move { api::roblox::resolve_join_link(&cookie, &link).await }
    })
    .await
}

#[tauri::command]
async fn validate_cookie(cookie: String) -> Result<api::auth::AccountInfo, String> {
    api::auth::validate_cookie(&cookie).await
}

#[tauri::command]
async fn get_csrf_token(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
) -> Result<String, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::auth::get_csrf_token(&cookie).await
    })
    .await
}

#[tauri::command]
async fn get_auth_ticket(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
) -> Result<String, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::auth::get_auth_ticket(&cookie).await
    })
    .await
}

#[tauri::command]
async fn check_pin(state: tauri::State<'_, AccountStore>, user_id: i64) -> Result<bool, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::auth::check_pin(&cookie).await
    })
    .await
}

#[tauri::command]
async fn unlock_pin(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    pin: String,
) -> Result<bool, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| {
        let pin = pin.clone();
        async move { api::auth::unlock_pin(&cookie, &pin).await }
    })
    .await
}

#[tauri::command]
async fn refresh_cookie(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
) -> Result<bool, String> {
    refresh_account_session(state.inner(), user_id).await?;
    Ok(true)
}

#[tauri::command]
async fn get_robux(state: tauri::State<'_, AccountStore>, user_id: i64) -> Result<i64, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::roblox::get_robux(&cookie).await
    })
    .await
}

#[tauri::command]
async fn get_user_info(user_id: i64) -> Result<api::roblox::UserInfo, String> {
    api::roblox::get_user_info(None, user_id).await
}

#[tauri::command]
async fn lookup_user(username: String) -> Result<api::roblox::UserLookupResult, String> {
    api::roblox::get_user_id(None, &username).await
}

#[tauri::command]
async fn test_auth(cookie: String) -> Result<String, String> {
    let mut results = Vec::new();

    results.push(format!("Cookie length: {}", cookie.len()));

    match api::auth::validate_cookie(&cookie).await {
        Ok(info) => results.push(format!(
            "Validate: OK - {} (ID: {})",
            info.name, info.user_id
        )),
        Err(e) => results.push(format!("Validate: FAILED - {}", e)),
    }

    match api::auth::get_csrf_token(&cookie).await {
        Ok(token) => results.push(format!("CSRF: OK - {}...", &token[..token.len().min(16)])),
        Err(e) => results.push(format!("CSRF: FAILED - {}", e)),
    }

    match api::auth::get_auth_ticket(&cookie).await {
        Ok(ticket) => results.push(format!(
            "Ticket: OK - {}...",
            &ticket[..ticket.len().min(20)]
        )),
        Err(e) => results.push(format!("Ticket: FAILED - {}", e)),
    }

    Ok(results.join("\n"))
}

#[tauri::command]
async fn send_friend_request(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    target_user_id: i64,
) -> Result<(), String> {
    run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::roblox::send_friend_request(&cookie, target_user_id).await
    })
    .await
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FriendLinkResult {
    pairs_total: usize,
    already_friends: usize,
    attempted: usize,
    verified_ok: usize,
    failed: usize,
    requests_sent: usize,
    errors: Vec<String>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FriendLinkProgress {
    phase: String,
    done: usize,
    total: usize,
}

fn emit_friend_progress(app: &tauri::AppHandle, phase: &str, done: usize, total: usize) {
    let _ = app.emit(
        "friend-link-progress",
        FriendLinkProgress { phase: phase.to_string(), done, total },
    );
}

/// Best-effort fetch of an account's current friend IDs (empty on failure, so a
/// lookup error never causes us to skip a pair we should have linked).
///
/// Deliberately not wrapped in `run_with_session_retry`: its recovery signs the
/// account out of every session (killing running clients) — far too costly
/// for a read whose failure is already tolerated.
async fn fetch_friend_set(state: &AccountStore, user_id: i64) -> std::collections::HashSet<i64> {
    let Ok(cookie) = get_cookie(state, user_id) else {
        return Default::default();
    };
    api::roblox::get_friend_ids(&cookie, user_id)
        .await
        .map(|ids| ids.into_iter().collect())
        .unwrap_or_default()
}

/// Sends one directed friend request (source -> target) reusing a cached cookie
/// and CSRF token for the source account. On a stale CSRF (403) it refetches
/// the token and retries once. An invalidated session (401) is reported, not
/// "repaired": the refresh signs out every session, closing running clients.
async fn send_directed_friend(
    state: &AccountStore,
    cookies: &mut std::collections::HashMap<i64, String>,
    csrfs: &mut std::collections::HashMap<i64, String>,
    source: i64,
    target: i64,
) -> Result<(), String> {
    if !cookies.contains_key(&source) {
        cookies.insert(source, get_cookie(state, source)?);
    }
    if !csrfs.contains_key(&source) {
        let cookie = cookies[&source].clone();
        let token = api::auth::get_csrf_token(&cookie).await?;
        csrfs.insert(source, token);
    }

    let cookie = cookies[&source].clone();
    let csrf = csrfs[&source].clone();
    let err = match api::roblox::send_friend_request_with_csrf(&cookie, &csrf, target).await {
        Ok(()) => return Ok(()),
        Err(e) => e,
    };

    let lower = err.to_ascii_lowercase();
    // Stale CSRF token → refetch and retry once.
    if lower.contains("token validation") || lower.contains("status 403") {
        let token = api::auth::get_csrf_token(&cookie).await?;
        csrfs.insert(source, token.clone());
        return api::roblox::send_friend_request_with_csrf(&cookie, &token, target).await;
    }
    if is_auth_session_error(&err) {
        return Err(format!("{} (sessão inválida — refaça o login da conta)", err));
    }
    Err(err)
}

/// Makes the selected accounts friends with each other.
///
/// - `mode = "mesh"`: every unordered pair of selected accounts.
/// - `mode = "star"`: every account paired only with `main_user_id`.
///
/// Pipeline: (1) fetch each account's friend list once and skip pairs already
/// friends; (2) for each remaining pair send A->B and B->A (mutual pending
/// requests auto-become a friendship — no accept endpoint needed), reusing one
/// CSRF token per source; (3) re-fetch friend lists and verify which pairs
/// actually formed. Progress is emitted via the `friend-link-progress` event.
#[tauri::command]
async fn make_selected_friends(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    settings: tauri::State<'_, SettingsStore>,
    user_ids: Vec<i64>,
    mode: String,
    main_user_id: Option<i64>,
    delay_ms: Option<u64>,
) -> Result<FriendLinkResult, String> {
    // One linking run at a time: the UI can remount and start a second batch,
    // doubling requests and the rate-limit/captcha risk.
    static FRIEND_LINK_RUNNING: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);
    struct RunningGuard;
    impl Drop for RunningGuard {
        fn drop(&mut self) {
            FRIEND_LINK_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
    if FRIEND_LINK_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("Já existe uma vinculação de amizades em andamento.".to_string());
    }
    let _running = RunningGuard;

    // Dedupe while preserving order.
    let mut ids: Vec<i64> = Vec::new();
    for id in user_ids {
        if id > 0 && !ids.contains(&id) {
            ids.push(id);
        }
    }
    if ids.len() < 2 {
        return Err("Select at least 2 accounts.".to_string());
    }

    // Build the list of unordered pairs based on the mode.
    let pairs: Vec<(i64, i64)> = match mode.trim().to_ascii_lowercase().as_str() {
        "mesh" => {
            let mut out = Vec::new();
            for i in 0..ids.len() {
                for j in (i + 1)..ids.len() {
                    out.push((ids[i], ids[j]));
                }
            }
            out
        }
        "star" => {
            let main = main_user_id
                .filter(|m| ids.contains(m))
                .ok_or_else(|| "Star mode requires a main account within the selection.".to_string())?;
            ids.iter().copied().filter(|&id| id != main).map(|id| (main, id)).collect()
        }
        other => return Err(format!("Unknown mode: {}", other)),
    };

    // Delay per request: explicit arg wins, else the "Friends/RequestDelayMs"
    // setting, else 2.5s. Clamped to a sane range.
    let resolved_delay = delay_ms
        .or_else(|| {
            settings
                .get_int("Friends", "RequestDelayMs")
                .and_then(|v| u64::try_from(v).ok())
        })
        .unwrap_or(2500)
        .clamp(500, 60_000);
    let delay = std::time::Duration::from_millis(resolved_delay);
    let store = state.inner();

    // ── Phase 1: fetch current friends of each involved account (dedupe). ──
    emit_friend_progress(&app, "checking", 0, ids.len());
    let mut friend_sets: std::collections::HashMap<i64, std::collections::HashSet<i64>> =
        std::collections::HashMap::new();
    for (i, &uid) in ids.iter().enumerate() {
        friend_sets.insert(uid, fetch_friend_set(store, uid).await);
        emit_friend_progress(&app, "checking", i + 1, ids.len());
    }

    let is_friends = |sets: &std::collections::HashMap<i64, std::collections::HashSet<i64>>,
                      a: i64,
                      b: i64| {
        sets.get(&a).map(|s| s.contains(&b)).unwrap_or(false)
            || sets.get(&b).map(|s| s.contains(&a)).unwrap_or(false)
    };

    let needed: Vec<(i64, i64)> = pairs
        .iter()
        .copied()
        .filter(|&(a, b)| !is_friends(&friend_sets, a, b))
        .collect();
    let already_friends = pairs.len() - needed.len();

    // ── Phase 2: send the requests (both directions), reusing CSRF per source. ──
    let mut cookies: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    let mut csrfs: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    let mut requests_sent = 0usize;
    let mut errors: Vec<String> = Vec::new();

    emit_friend_progress(&app, "linking", 0, needed.len());
    for (idx, &(a, b)) in needed.iter().enumerate() {
        requests_sent += 1;
        if let Err(e) = send_directed_friend(store, &mut cookies, &mut csrfs, a, b).await {
            if errors.len() < 20 {
                errors.push(format!("{}->{}: {}", a, b, e));
            }
        }
        if delay > std::time::Duration::ZERO {
            tokio::time::sleep(delay).await;
        }

        requests_sent += 1;
        if let Err(e) = send_directed_friend(store, &mut cookies, &mut csrfs, b, a).await {
            if errors.len() < 20 {
                errors.push(format!("{}->{}: {}", b, a, e));
            }
        }
        emit_friend_progress(&app, "linking", idx + 1, needed.len());

        if delay > std::time::Duration::ZERO && idx + 1 < needed.len() {
            tokio::time::sleep(delay).await;
        }
    }

    // ── Phase 3: verify which needed pairs actually became friends. ──
    let mut verified_ok = 0usize;
    if !needed.is_empty() {
        emit_friend_progress(&app, "verifying", 0, ids.len());
        let mut after: std::collections::HashMap<i64, std::collections::HashSet<i64>> =
            std::collections::HashMap::new();
        for (i, &uid) in ids.iter().enumerate() {
            after.insert(uid, fetch_friend_set(store, uid).await);
            emit_friend_progress(&app, "verifying", i + 1, ids.len());
        }
        verified_ok = needed
            .iter()
            .filter(|&&(a, b)| is_friends(&after, a, b))
            .count();
    }
    let failed = needed.len() - verified_ok;

    emit_friend_progress(&app, "done", needed.len(), needed.len());

    Ok(FriendLinkResult {
        pairs_total: pairs.len(),
        already_friends,
        attempted: needed.len(),
        verified_ok,
        failed,
        requests_sent,
        errors,
    })
}

#[tauri::command]
async fn block_user(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    target_user_id: i64,
) -> Result<(), String> {
    run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::roblox::block_user(&cookie, target_user_id).await
    })
    .await
}

#[tauri::command]
async fn unblock_user(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    target_user_id: i64,
) -> Result<(), String> {
    run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::roblox::unblock_user(&cookie, target_user_id).await
    })
    .await
}

#[tauri::command]
async fn get_blocked_users(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
) -> Result<Vec<api::roblox::BlockedUser>, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::roblox::get_blocked_users(&cookie).await
    })
    .await
}

#[tauri::command]
async fn unblock_all_users(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
) -> Result<i32, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::roblox::unblock_all_users(&cookie).await
    })
    .await
}

#[tauri::command]
async fn set_follow_privacy(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    privacy: String,
) -> Result<(), String> {
    run_with_session_retry(state.inner(), user_id, |cookie| {
        let privacy = privacy.clone();
        async move { api::roblox::set_follow_privacy(&cookie, &privacy).await }
    })
    .await
}

#[tauri::command]
async fn get_private_server_invite_privacy(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
) -> Result<String, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::roblox::get_private_server_invite_privacy(&cookie).await
    })
    .await
}

#[tauri::command]
async fn set_private_server_invite_privacy(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    privacy: String,
) -> Result<(), String> {
    run_with_session_retry(state.inner(), user_id, |cookie| {
        let privacy = privacy.clone();
        async move { api::roblox::set_private_server_invite_privacy(&cookie, &privacy).await }
    })
    .await
}

#[tauri::command]
async fn set_avatar(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    avatar_json: serde_json::Value,
) -> Result<Vec<i64>, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| {
        let avatar_json = avatar_json.clone();
        async move { api::roblox::set_avatar(&cookie, avatar_json).await }
    })
    .await
}

#[tauri::command]
async fn get_outfits(target_user_id: i64) -> Result<Vec<api::roblox::OutfitInfo>, String> {
    api::roblox::get_outfits(target_user_id).await
}

#[tauri::command]
async fn get_outfit_details(outfit_id: i64) -> Result<serde_json::Value, String> {
    api::roblox::get_outfit_details(outfit_id).await
}

#[tauri::command]
async fn get_place_details(
    state: tauri::State<'_, AccountStore>,
    place_ids: Vec<i64>,
    user_id: Option<i64>,
) -> Result<Vec<api::roblox::PlaceDetails>, String> {
    let cookie = user_id.and_then(|id| get_cookie(&state, id).ok());
    api::roblox::get_place_details(&place_ids, cookie.as_deref()).await
}

#[tauri::command]
async fn get_servers(
    state: tauri::State<'_, AccountStore>,
    place_id: i64,
    server_type: String,
    cursor: Option<String>,
    user_id: Option<i64>,
) -> Result<api::roblox::ServersResponse, String> {
    let cookie = user_id.and_then(|id| get_cookie(&state, id).ok());
    api::roblox::get_servers(place_id, &server_type, cursor.as_deref(), cookie.as_deref()).await
}

#[tauri::command]
async fn join_game_instance(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    place_id: i64,
    game_id: String,
    is_teleport: bool,
) -> Result<serde_json::Value, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| {
        let game_id = game_id.clone();
        async move { api::roblox::join_game_instance(&cookie, place_id, &game_id, is_teleport).await }
    })
    .await
}

#[tauri::command]
async fn join_game(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    place_id: i64,
) -> Result<serde_json::Value, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::roblox::join_game(&cookie, place_id).await
    })
    .await
}

#[tauri::command]
async fn search_games(
    security_token: Option<String>,
    keyword: String,
    start: i32,
) -> Result<serde_json::Value, String> {
    api::roblox::search_games(security_token.as_deref(), &keyword, start).await
}

#[tauri::command]
async fn get_universe_places(
    state: tauri::State<'_, AccountStore>,
    universe_id: i64,
    user_id: Option<i64>,
) -> Result<Vec<api::roblox::UniversePlace>, String> {
    let cookie = user_id.and_then(|id| get_cookie(&state, id).ok());
    api::roblox::get_universe_places(universe_id, cookie.as_deref()).await
}

#[tauri::command]
async fn parse_private_server_link_code(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    place_id: i64,
    link_code: String,
) -> Result<String, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| {
        let link_code = link_code.clone();
        async move { api::roblox::parse_private_server_link_code(&cookie, place_id, &link_code).await }
    })
    .await
}

#[tauri::command]
async fn join_group(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    group_id: i64,
) -> Result<(), String> {
    run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::roblox::join_group(&cookie, group_id).await
    })
    .await
}

#[tauri::command]
async fn get_presence(user_ids: Vec<i64>) -> Result<Vec<api::roblox::UserPresence>, String> {
    api::roblox::get_presence(&user_ids).await
}

#[tauri::command]
async fn batch_thumbnails(
    requests: Vec<api::roblox::ThumbnailRequest>,
) -> Result<Vec<api::roblox::ThumbnailResponse>, String> {
    api::roblox::batch_thumbnails(requests).await
}

#[tauri::command]
async fn get_avatar_headshots(
    user_ids: Vec<i64>,
    size: String,
) -> Result<Vec<api::roblox::ThumbnailResponse>, String> {
    api::roblox::get_avatar_headshots(&user_ids, &size).await
}

#[tauri::command]
async fn get_asset_thumbnails(
    state: tauri::State<'_, AccountStore>,
    asset_ids: Vec<i64>,
    size: String,
    user_id: Option<i64>,
) -> Result<Vec<api::roblox::ThumbnailResponse>, String> {
    let cookie = user_id.and_then(|id| get_cookie(&state, id).ok());
    api::roblox::get_asset_thumbnails(&asset_ids, &size, cookie.as_deref()).await
}

#[tauri::command]
async fn get_asset_details(
    state: tauri::State<'_, AccountStore>,
    asset_id: i64,
    user_id: Option<i64>,
) -> Result<api::roblox::AssetDetails, String> {
    let cookie = user_id.and_then(|id| get_cookie(&state, id).ok());
    api::roblox::get_asset_details(asset_id, cookie.as_deref()).await
}

#[tauri::command]
async fn purchase_product(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    product_id: i64,
    expected_price: i64,
    expected_seller_id: i64,
) -> Result<api::roblox::PurchaseResult, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::roblox::purchase_product(&cookie, product_id, expected_price, expected_seller_id)
            .await
    })
    .await
}

#[tauri::command]
async fn change_password(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    current_password: String,
    new_password: String,
) -> Result<(), String> {
    let new_cookie = run_with_session_retry(state.inner(), user_id, |cookie| {
        let current_password = current_password.clone();
        let new_password = new_password.clone();
        async move { api::auth::change_password(&cookie, &current_password, &new_password).await }
    })
    .await?;

    if let Some(new_cookie) = new_cookie {
        persist_cookie_update(state.inner(), user_id, &new_cookie)?;
    }

    Ok(())
}

#[tauri::command]
async fn change_email(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    password: String,
    new_email: String,
) -> Result<(), String> {
    run_with_session_retry(state.inner(), user_id, |cookie| {
        let password = password.clone();
        let new_email = new_email.clone();
        async move { api::auth::change_email(&cookie, &password, &new_email).await }
    })
    .await
}

#[tauri::command]
async fn set_display_name(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    display_name: String,
) -> Result<(), String> {
    run_with_session_retry(state.inner(), user_id, |cookie| {
        let display_name = display_name.clone();
        async move { api::auth::set_display_name(&cookie, user_id, &display_name).await }
    })
    .await
}

#[tauri::command]
async fn quick_login_enter_code(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    code: String,
) -> Result<serde_json::Value, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| {
        let code = code.clone();
        async move { api::auth::quick_login_enter_code(&cookie, &code).await }
    })
    .await
}

#[tauri::command]
async fn quick_login_validate_code(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    code: String,
) -> Result<(), String> {
    run_with_session_retry(state.inner(), user_id, |cookie| {
        let code = code.clone();
        async move { api::auth::quick_login_validate_code(&cookie, &code).await }
    })
    .await
}
