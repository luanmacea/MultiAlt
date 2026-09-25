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
        Ok(token) => results.push(format!("CSRF: OK - {}...", token.chars().take(16).collect::<String>())),
        Err(e) => results.push(format!("CSRF: FAILED - {}", e)),
    }

    match api::auth::get_auth_ticket(&cookie).await {
        Ok(ticket) => results.push(format!(
            "Ticket: OK - {}...",
            ticket.chars().take(20).collect::<String>()
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

/// One linking run at a time: the UI can remount and start a second batch,
/// doubling requests and the rate-limit/captcha risk.
static FRIEND_LINK_RUNNING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Releases the friend-linking lock when the run ends, however it ends.
struct FriendLinkRunGuard;

impl Drop for FriendLinkRunGuard {
    fn drop(&mut self) {
        FRIEND_LINK_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Takes the friend-linking lock, or reports that a run is already in progress.
fn try_begin_friend_link() -> Result<FriendLinkRunGuard, String> {
    if FRIEND_LINK_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("Já existe uma vinculação de amizades em andamento.".to_string());
    }
    Ok(FriendLinkRunGuard)
}

/// Drops non-positive and repeated ids while preserving the selection order.
fn dedupe_friend_ids(user_ids: Vec<i64>) -> Vec<i64> {
    let mut ids: Vec<i64> = Vec::new();
    for id in user_ids {
        if id > 0 && !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

/// The unordered pairs to link.
///
/// - `mesh`: every combination of the selected accounts.
/// - `star`: every account paired only with `main_user_id`.
fn build_friend_pairs(
    ids: &[i64],
    mode: &str,
    main_user_id: Option<i64>,
) -> Result<Vec<(i64, i64)>, String> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "mesh" => {
            let mut out = Vec::new();
            for i in 0..ids.len() {
                for j in (i + 1)..ids.len() {
                    out.push((ids[i], ids[j]));
                }
            }
            Ok(out)
        }
        "star" => {
            let main = main_user_id
                .filter(|m| ids.contains(m))
                .ok_or_else(|| "Star mode requires a main account within the selection.".to_string())?;
            Ok(ids
                .iter()
                .copied()
                .filter(|&id| id != main)
                .map(|id| (main, id))
                .collect())
        }
        other => Err(format!("Unknown mode: {}", other)),
    }
}

/// Delay between friend requests: the explicit argument wins, else the
/// `Friends/RequestDelayMs` setting, else 2.5s. Clamped to a sane range so a
/// misconfigured value cannot hammer (or stall) the endpoint.
fn resolve_friend_delay_ms(explicit: Option<u64>, setting: Option<i64>) -> u64 {
    explicit
        .or_else(|| setting.and_then(|v| u64::try_from(v).ok()))
        .unwrap_or(2500)
        .clamp(500, 60_000)
}

/// True when either side already lists the other as a friend. Checking both
/// directions keeps a one-sided fetch failure from re-sending requests.
fn friend_sets_contain(
    sets: &std::collections::HashMap<i64, std::collections::HashSet<i64>>,
    a: i64,
    b: i64,
) -> bool {
    sets.get(&a).map(|s| s.contains(&b)).unwrap_or(false)
        || sets.get(&b).map(|s| s.contains(&a)).unwrap_or(false)
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
    let _running = try_begin_friend_link()?;

    let ids = dedupe_friend_ids(user_ids);
    if ids.len() < 2 {
        return Err("Select at least 2 accounts.".to_string());
    }

    let pairs = build_friend_pairs(&ids, &mode, main_user_id)?;

    let resolved_delay =
        resolve_friend_delay_ms(delay_ms, settings.get_int("Friends", "RequestDelayMs"));
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

    let needed: Vec<(i64, i64)> = pairs
        .iter()
        .copied()
        .filter(|&(a, b)| !friend_sets_contain(&friend_sets, a, b))
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
            .filter(|&&(a, b)| friend_sets_contain(&after, a, b))
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
    sort_order: Option<String>,
    exclude_full: Option<bool>,
) -> Result<api::roblox::ServersResponse, String> {
    let cookie = user_id.and_then(|id| get_cookie(&state, id).ok());
    api::roblox::get_servers_page(
        place_id,
        &server_type,
        cursor.as_deref(),
        cookie.as_deref(),
        sort_order.as_deref().unwrap_or("Asc"),
        exclude_full.unwrap_or(false),
    )
    .await
}

/// A lista de servidores **na ordem da preferência**, já sem os que não cabem
/// o lote.
///
/// Existe para a aba Servers mostrar exatamente o que o launch escolheria: a
/// ordenação e a paginação do "melhor encaixe" moram no backend, e refazer isso
/// no frontend com uma página de 100 servidores dava resultado diferente.
#[tauri::command]
async fn list_servers_ranked(
    state: tauri::State<'_, AccountStore>,
    place_id: i64,
    user_id: Option<i64>,
    preference: String,
    accounts: Option<usize>,
) -> Result<Vec<api::roblox::ServerData>, String> {
    let cookie = user_id.and_then(|id| get_cookie(&state, id).ok());
    let preference = api::roblox::parse_server_preference(&preference);
    let accounts = accounts.unwrap_or(1).max(1);

    let servers = api::roblox::collect_servers_for(
        place_id,
        cookie.as_deref(),
        preference,
        accounts,
        api::roblox::BEST_FIT_MAX_PAGES,
    )
    .await?;

    Ok(api::roblox::rank_servers(&servers, preference, accounts)
        .into_iter()
        .cloned()
        .collect())
}

// ---------------------------------------------------------------------------
// Varredura de servidores
// ---------------------------------------------------------------------------
//
// Um jogo grande tem milhares de servidores e a API devolve 100 por página. Com
// um lote de 6 contas, as primeiras páginas podem não ter **nenhum** servidor
// que caiba todo mundo — e esperar a varredura inteira antes de mostrar
// qualquer coisa deixaria a tela vazia por vários segundos.
//
// A varredura roda em background e publica um `server-scan` a cada página, já
// com a lista reordenada: o usuário vê o que existe agora e a lista vai
// melhorando enquanto mais páginas chegam.

/// Páginas por varredura quando o usuário não escolheu nada (100 servidores
/// cada). Cobre a maioria dos jogos sem virar uma enxurrada de requisições.
pub const SCAN_DEFAULT_PAGES: usize = 30;

/// Teto absoluto, mesmo que o usuário peça mais: 50 mil servidores já é mais do
/// que qualquer jogo tem, e uma varredura infinita martelaria a API do Roblox.
pub const SCAN_HARD_MAX_PAGES: usize = 500;

/// Quantas páginas esta varredura pode percorrer.
///
/// `None` (a UI não mandou nada) usa o padrão; zero é erro de chamada e vira o
/// padrão também; acima do teto, o teto.
pub fn scan_page_budget(requested: Option<usize>) -> usize {
    match requested {
        Some(0) | None => SCAN_DEFAULT_PAGES,
        Some(pages) => pages.min(SCAN_HARD_MAX_PAGES),
    }
}
/// Pausa entre páginas, para não tomar 429 da API de servidores.
const SCAN_PAGE_DELAY_MS: u64 = 250;
/// Quantos servidores que cabem o lote bastam para parar de procurar.
const SCAN_ENOUGH_FITTING: usize = 12;
/// Quantos servidores a UI recebe por atualização.
const SCAN_VISIBLE_LIMIT: usize = 150;

/// Geração da varredura em curso. Começar outra (ou parar) invalida a anterior,
/// que percebe a mudança na próxima página e se encerra.
static SERVER_SCAN_GENERATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Uma atualização da varredura (evento `server-scan`).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerScanUpdate {
    pub scan_id: u64,
    pub place_id: i64,
    /// Os melhores servidores encontrados até agora, já ordenados.
    pub servers: Vec<api::roblox::ServerData>,
    /// Quantos servidores foram examinados no total.
    pub scanned: usize,
    /// Quantos deles cabem o lote inteiro.
    pub fitting: usize,
    pub done: bool,
    /// Parou por ter batido o limite de páginas, não por falta de servidores.
    /// A UI oferece aumentar o limite em vez de dizer que acabou.
    pub stopped_at_limit: bool,
    pub error: Option<String>,
}

fn scan_cancelled(scan_id: u64) -> bool {
    SERVER_SCAN_GENERATION.load(std::sync::atomic::Ordering::Relaxed) != scan_id
}

/// Encerra a varredura em curso (troca de jogo, de preferência, ou a aba saiu
/// da tela).
#[tauri::command]
fn stop_server_scan() {
    SERVER_SCAN_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Começa a varrer os servidores de um place em background.
///
/// Devolve o `scanId` da varredura; a UI ignora eventos de outro id (o de uma
/// varredura antiga que ainda estava no ar).
#[tauri::command]
async fn start_server_scan(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    place_id: i64,
    user_id: Option<i64>,
    preference: String,
    accounts: Option<usize>,
    max_pages: Option<usize>,
) -> Result<u64, String> {
    if place_id <= 0 {
        return Err("Place ID inválido".to_string());
    }
    let budget = scan_page_budget(max_pages);
    let cookie = user_id.and_then(|id| get_cookie(&state, id).ok());
    let preference = api::roblox::parse_server_preference(&preference);
    let accounts = accounts.unwrap_or(1).max(1);

    let scan_id = SERVER_SCAN_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;

    tauri::async_runtime::spawn(async move {
        let mut all: Vec<api::roblox::ServerData> = Vec::new();
        let mut cursor: Option<String> = None;

        for page in 0..budget {
            if scan_cancelled(scan_id) {
                return;
            }

            let fetched = api::roblox::get_servers_page(
                place_id,
                "Public",
                cursor.as_deref(),
                cookie.as_deref(),
                preference.sort_order(),
                true,
            )
            .await;

            let (batch, next) = match fetched {
                Ok(page) => (page.data, page.next_page_cursor),
                Err(error) => {
                    // Erro na primeira página é falha; no meio, a varredura
                    // para com o que já achou — melhor uma lista parcial que
                    // uma tela de erro.
                    let _ = app.emit(
                        "server-scan",
                        scan_update(
                            scan_id, place_id, &all, preference, accounts, true, false,
                            Some(error),
                        ),
                    );
                    return;
                }
            };

            all.extend(batch);
            let fitting = count_fitting(&all, accounts);
            let exhausted = next.as_deref().map(|c| c.trim().is_empty()).unwrap_or(true);
            let enough = fitting >= SCAN_ENOUGH_FITTING;
            let hit_limit = page + 1 == budget && !exhausted && !enough;
            let done = exhausted || enough || hit_limit;

            if scan_cancelled(scan_id) {
                return;
            }
            let _ = app.emit(
                "server-scan",
                scan_update(
                    scan_id, place_id, &all, preference, accounts, done, hit_limit, None,
                ),
            );

            if done {
                return;
            }
            cursor = next;
            tokio::time::sleep(std::time::Duration::from_millis(SCAN_PAGE_DELAY_MS)).await;
        }
    });

    Ok(scan_id)
}

/// Quantos servidores da lista cabem o lote inteiro.
fn count_fitting(servers: &[api::roblox::ServerData], accounts: usize) -> usize {
    let needed = accounts.max(1) as i32;
    servers
        .iter()
        .filter(|s| s.max_players > 0 && s.playing + needed <= s.max_players)
        .count()
}

/// Monta a atualização com a lista já ordenada e cortada no que a UI mostra.
fn scan_update(
    scan_id: u64,
    place_id: i64,
    all: &[api::roblox::ServerData],
    preference: api::roblox::ServerPreference,
    accounts: usize,
    done: bool,
    stopped_at_limit: bool,
    error: Option<String>,
) -> ServerScanUpdate {
    let servers: Vec<api::roblox::ServerData> =
        api::roblox::rank_servers(all, preference, accounts)
            .into_iter()
            .take(SCAN_VISIBLE_LIMIT)
            .cloned()
            .collect();

    ServerScanUpdate {
        scan_id,
        place_id,
        servers,
        scanned: all.len(),
        fitting: count_fitting(all, accounts),
        done,
        stopped_at_limit,
        error,
    }
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

// ── Amigos online ─────────────────────────────────────────────────────────────

/// Uma conta e os amigos online dela. `error` preenchido substitui a lista
/// daquela conta **sem** derrubar as outras entradas do lote.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountFriends {
    user_id: i64,
    friends: Vec<api::roblox::OnlineFriend>,
    error: Option<String>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FriendsOnlineProgress {
    done: usize,
    total: usize,
}

/// Espaçamento default entre contas no lote. O rate limit da API de amigos é
/// por conta **e** por IP, e todas as contas saem do mesmo IP.
const FRIENDS_ONLINE_DELAY_MS: u64 = 350;

/// Amigos online de uma conta, ordenados (quem dá para entrar primeiro).
///
/// Usa `get_cookie` direto e **nunca** `run_with_session_retry`: o refresh dele
/// chama `signoutfromallsessionsandreauthenticate`, que derruba as sessões
/// abertas da conta — caro demais para uma leitura.
#[tauri::command]
async fn get_online_friends(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
) -> Result<Vec<api::roblox::OnlineFriend>, String> {
    let cookie = get_cookie(state.inner(), user_id)?;
    api::roblox::get_online_friends(&cookie, user_id).await
}

/// Percorre as contas **em sequência**, com pausa entre elas, e isola o erro de
/// cada uma na sua própria entrada. `on_progress` recebe `(feitas, total)`.
///
/// Separado do comando Tauri para poder ser testado sem `AppHandle`.
async fn collect_online_friends<F>(
    store: &AccountStore,
    user_ids: Vec<i64>,
    delay_ms: Option<u64>,
    mut on_progress: F,
) -> Vec<AccountFriends>
where
    F: FnMut(usize, usize),
{
    let ids = dedupe_friend_ids(user_ids);
    let total = ids.len();
    let delay =
        std::time::Duration::from_millis(delay_ms.unwrap_or(FRIENDS_ONLINE_DELAY_MS));
    let mut results: Vec<AccountFriends> = Vec::with_capacity(total);

    on_progress(0, total);
    for (index, &user_id) in ids.iter().enumerate() {
        let entry = match get_cookie(store, user_id) {
            Ok(cookie) => match api::roblox::get_online_friends(&cookie, user_id).await {
                Ok(friends) => AccountFriends { user_id, friends, error: None },
                Err(error) => {
                    AccountFriends { user_id, friends: Vec::new(), error: Some(error) }
                }
            },
            Err(error) => AccountFriends { user_id, friends: Vec::new(), error: Some(error) },
        };
        results.push(entry);
        on_progress(index + 1, total);

        if index + 1 < total && !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
    }

    results
}

/// Amigos online de várias contas, uma chamada por conta. Emite
/// `friends-online-progress` com `{ done, total }` a cada conta concluída.
#[tauri::command]
async fn get_online_friends_for_accounts(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    user_ids: Vec<i64>,
    delay_ms: Option<u64>,
) -> Result<Vec<AccountFriends>, String> {
    let results = collect_online_friends(state.inner(), user_ids, delay_ms, |done, total| {
        let _ = app.emit("friends-online-progress", FriendsOnlineProgress { done, total });
    })
    .await;

    Ok(results)
}

// ---------------------------------------------------------------------------
// Escolha de servidor e região
// ---------------------------------------------------------------------------

/// Template de exibição da região. Configurável porque o usuário pode querer
/// só o país, ou o IP cru.
fn server_region_template(settings: &SettingsStore) -> String {
    settings
        .get("General", "ServerRegionFormat")
        .ok()
        .flatten()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "<city>, <countryCode>".to_string())
}

/// Região de vários servidores de um place, uma chamada de join por servidor.
///
/// Emite `server-region-progress` com `{ done, total }`. Sequencial e com pausa
/// de propósito: o `join-game-instance` é o mesmo endpoint que o cliente usa
/// para entrar no jogo, e disparar em rajada toma 429.
///
/// Usa `get_cookie` direto e **nunca** `run_with_session_retry`: o refresh dele
/// derruba as sessões abertas da conta.
#[tauri::command]
async fn get_server_regions(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    settings: tauri::State<'_, SettingsStore>,
    user_id: i64,
    place_id: i64,
    job_ids: Vec<String>,
) -> Result<Vec<api::roblox::ServerRegion>, String> {
    let cookie = get_cookie(state.inner(), user_id)?;
    let template = server_region_template(settings.inner());

    let results = api::roblox::resolve_server_regions(
        &cookie,
        place_id,
        &job_ids,
        &template,
        api::roblox::REGION_LOOKUP_DELAY_MS,
        |done, total| {
            let _ = app.emit(
                "server-region-progress",
                api::roblox::ServerRegionProgress { done, total },
            );
        },
    )
    .await;

    Ok(results)
}

/// Escolhe o servidor do lote conforme a preferência (`random` | `emptiest` |
/// `fullest`) e, opcionalmente, um país (`BR`).
///
/// Resolvido **uma vez** para o lote inteiro: a UI passa o Job ID devolvido
/// para o launch, e todas as contas entram no mesmo servidor.
#[tauri::command]
async fn pick_server(
    state: tauri::State<'_, AccountStore>,
    settings: tauri::State<'_, SettingsStore>,
    user_id: i64,
    place_id: i64,
    preference: String,
    accounts: Option<usize>,
    country_code: Option<String>,
    max_lookups: Option<usize>,
) -> Result<api::roblox::PickedServer, String> {
    let cookie = get_cookie(state.inner(), user_id)?;
    let template = server_region_template(settings.inner());

    api::roblox::pick_server(
        &cookie,
        place_id,
        api::roblox::parse_server_preference(&preference),
        accounts.unwrap_or(1),
        country_code.as_deref().unwrap_or(""),
        &template,
        max_lookups.unwrap_or(api::roblox::REGION_LOOKUP_MAX),
    )
    .await
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
