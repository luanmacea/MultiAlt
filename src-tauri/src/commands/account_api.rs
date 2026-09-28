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
    // Resolver o share link e leitura: se o cookie estiver velho, isto falha e a
    // pessoa reloga. Renovar sessao aqui derrubaria os clientes abertos da conta
    // para resolver um link — preco alto demais para uma consulta.
    read_without_refresh(state.inner(), user_id, move |cookie| async move {
        api::roblox::resolve_join_link(&cookie, &link).await
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
    read_without_refresh(state.inner(), user_id, |cookie| async move {
        api::auth::get_csrf_token(&cookie).await
    })
    .await
}

/// Leitura que **não** pode renovar sessão: pega o cookie e chama a API direto.
///
/// O refresh (`run_with_session_retry`) chama
/// `signoutfromallsessionsandreauthenticate`, que desloga a conta de todas as
/// sessões e pode derrubar clientes Roblox abertos. Para leitura, um cookie velho
/// tem de virar erro na tela — quem lê o saldo não aceita esse preço.
async fn read_without_refresh<T, F, Fut>(
    state: &AccountStore,
    user_id: i64,
    operation: F,
) -> Result<T, String>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let cookie = get_cookie(state, user_id)?;
    operation(cookie).await
}

/// Ticket de auth para os links que o menu de contexto copia (`roblox-player://`
/// e o app link). É leitura não crítica, então **não** passa por
/// `run_with_session_retry`: o refresh chama
/// `signoutfromallsessionsandreauthenticate` e derrubaria as sessões abertas da
/// conta só porque o cookie estava velho. O launch tem o seu próprio caminho e
/// continua com retry.
async fn auth_ticket_without_refresh(state: &AccountStore, user_id: i64) -> Result<String, String> {
    read_without_refresh(state, user_id, |cookie| async move {
        api::auth::get_auth_ticket(&cookie).await
    })
    .await
}

#[tauri::command]
async fn get_auth_ticket(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
) -> Result<String, String> {
    auth_ticket_without_refresh(state.inner(), user_id).await
}

#[tauri::command]
async fn check_pin(state: tauri::State<'_, AccountStore>, user_id: i64) -> Result<bool, String> {
    read_without_refresh(state.inner(), user_id, |cookie| async move {
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
    read_without_refresh(state.inner(), user_id, |cookie| async move {
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

/// Estado de **uma conta** dentro de uma vinculação de amizades.
///
/// O progresso antigo era só `{phase, done, total}`, e na fase de envio o
/// `done` contava **pares**, não contas: não dava para dizer quais contas já
/// terminaram, qual está sendo processada e qual deu erro. O erro tampouco era
/// atribuível — vinha como a string `"a->b: msg"`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FriendLinkAccount {
    user_id: i64,
    /// `pending` | `processing` | `done` | `failed`
    state: String,
    error: Option<String>,
}

/// Retrato completo da operação, como o `launch-queue` faz: o backend guarda o
/// estado e reemite o payload inteiro a cada mudança, e a UI só substitui. Sem
/// isso, a tela que remonta no meio perde o progresso até o próximo evento.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FriendLinkSnapshot {
    active: bool,
    /// `idle` | `checking` | `linking` | `verifying` | `done`
    phase: String,
    /// Contas que já terminaram (concluídas ou com erro).
    processed: usize,
    total: usize,
    accounts: Vec<FriendLinkAccount>,
    mode: String,
    main_user_id: Option<i64>,
}

/// Núcleo puro do acompanhamento: nada de `AppHandle` aqui, para os testes não
/// precisarem de um app Tauri (mesma divisão do `LaunchQueue`).
#[derive(Debug, Default)]
struct FriendLinkRun {
    active: bool,
    phase: String,
    mode: String,
    main_user_id: Option<i64>,
    /// Ordem da seleção, para a lista não dançar entre um evento e outro.
    order: Vec<i64>,
    states: std::collections::HashMap<i64, (String, Option<String>)>,
    /// Pares que ainda faltam para a conta terminar.
    pending_pairs: std::collections::HashMap<i64, usize>,
}

impl FriendLinkRun {
    fn start(&mut self, ids: &[i64], mode: &str, main_user_id: Option<i64>) {
        self.active = true;
        self.phase = "checking".to_string();
        self.mode = mode.to_string();
        self.main_user_id = main_user_id;
        self.order = ids.to_vec();
        self.states = ids
            .iter()
            .map(|&id| (id, ("pending".to_string(), None)))
            .collect();
        self.pending_pairs.clear();
    }

    fn set_phase(&mut self, phase: &str) {
        self.phase = phase.to_string();
    }

    /// Estado de uma conta, **sem** rebaixar quem já terminou: a fase de
    /// verificação passa por todas as contas de novo, e marcar "processando"
    /// em quem já falhou apagaria o erro.
    fn set_state(&mut self, user_id: i64, state: &str) {
        if let Some(entry) = self.states.get_mut(&user_id) {
            if entry.0 == "done" || entry.0 == "failed" {
                return;
            }
            entry.0 = state.to_string();
        }
    }

    fn fail(&mut self, user_id: i64, error: String) {
        if let Some(entry) = self.states.get_mut(&user_id) {
            entry.0 = "failed".to_string();
            if entry.1.is_none() {
                entry.1 = Some(error);
            }
        }
    }

    /// Quantos pares faltam para cada conta. Conta sem par nenhum (já amiga de
    /// todo mundo) termina aqui mesmo — não teria evento que a concluísse.
    fn plan_pairs(&mut self, needed: &[(i64, i64)]) {
        self.pending_pairs.clear();
        for &(a, b) in needed {
            *self.pending_pairs.entry(a).or_insert(0) += 1;
            *self.pending_pairs.entry(b).or_insert(0) += 1;
        }
        let sem_par: Vec<i64> = self
            .order
            .iter()
            .copied()
            .filter(|id| !self.pending_pairs.contains_key(id))
            .collect();
        for id in sem_par {
            self.set_state(id, "done");
        }
    }

    fn pair_started(&mut self, a: i64, b: i64) {
        self.set_state(a, "processing");
        self.set_state(b, "processing");
    }

    /// Fecha um par. O erro é atribuído a quem **enviou** o pedido que falhou:
    /// é o cookie/CSRF daquela conta que não funcionou.
    fn pair_finished(&mut self, a: i64, b: i64, error_ab: Option<String>, error_ba: Option<String>) {
        if let Some(e) = error_ab {
            self.fail(a, e);
        }
        if let Some(e) = error_ba {
            self.fail(b, e);
        }
        for id in [a, b] {
            let restante = self.pending_pairs.entry(id).or_insert(0);
            *restante = restante.saturating_sub(1);
            if *restante == 0 {
                self.set_state(id, "done");
            }
        }
    }

    /// O par não virou amizade na verificação, mesmo sem erro no envio.
    fn pair_not_verified(&mut self, a: i64, b: i64, motivo: &str) {
        self.fail(a, motivo.to_string());
        self.fail(b, motivo.to_string());
    }

    fn finish(&mut self) {
        self.active = false;
        self.phase = "done".to_string();
        let restantes: Vec<i64> = self.order.clone();
        for id in restantes {
            self.set_state(id, "done");
        }
    }

    fn processed(&self) -> usize {
        self.order
            .iter()
            .filter(|id| {
                self.states
                    .get(id)
                    .map(|(state, _)| state == "done" || state == "failed")
                    .unwrap_or(false)
            })
            .count()
    }

    fn snapshot(&self) -> FriendLinkSnapshot {
        FriendLinkSnapshot {
            active: self.active,
            phase: if self.phase.is_empty() { "idle".to_string() } else { self.phase.clone() },
            processed: self.processed(),
            total: self.order.len(),
            accounts: self
                .order
                .iter()
                .map(|id| {
                    let (state, error) = self
                        .states
                        .get(id)
                        .cloned()
                        .unwrap_or_else(|| ("pending".to_string(), None));
                    FriendLinkAccount { user_id: *id, state, error }
                })
                .collect(),
            mode: self.mode.clone(),
            main_user_id: self.main_user_id,
        }
    }
}

static FRIEND_LINK: std::sync::LazyLock<std::sync::Mutex<FriendLinkRun>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(FriendLinkRun::default()));

/// Muda o estado e publica o retrato inteiro. Mutex envenenado não pode
/// derrubar a vinculação: o acompanhamento é informativo.
fn update_friend_link(app: &tauri::AppHandle, f: impl FnOnce(&mut FriendLinkRun)) {
    let snapshot = {
        let Ok(mut run) = FRIEND_LINK.lock() else { return };
        f(&mut run);
        run.snapshot()
    };
    let _ = app.emit("friend-link-state", snapshot);
}

/// Retrato atual, para a tela que abre (ou remonta) no meio da operação.
#[tauri::command]
fn get_friend_link_state() -> FriendLinkSnapshot {
    FRIEND_LINK
        .lock()
        .map(|run| run.snapshot())
        .unwrap_or_else(|_| FriendLinkRun::default().snapshot())
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
/// actually formed. Progress is emitted via the `friend-link-state` event, with
/// one entry per account (the old `{phase, done, total}` counted **pairs** in the
/// linking phase, so it could not say which accounts were finished).
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
    update_friend_link(&app, |run| run.start(&ids, &mode, main_user_id));
    let mut friend_sets: std::collections::HashMap<i64, std::collections::HashSet<i64>> =
        std::collections::HashMap::new();
    for &uid in ids.iter() {
        update_friend_link(&app, |run| run.set_state(uid, "processing"));
        friend_sets.insert(uid, fetch_friend_set(store, uid).await);
        update_friend_link(&app, |run| run.set_state(uid, "pending"));
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

    update_friend_link(&app, |run| {
        run.set_phase("linking");
        run.plan_pairs(&needed);
    });
    for (idx, &(a, b)) in needed.iter().enumerate() {
        update_friend_link(&app, |run| run.pair_started(a, b));

        requests_sent += 1;
        let mut error_ab: Option<String> = None;
        if let Err(e) = send_directed_friend(store, &mut cookies, &mut csrfs, a, b).await {
            if errors.len() < 20 {
                errors.push(format!("{}->{}: {}", a, b, e));
            }
            error_ab = Some(e);
        }
        if delay > std::time::Duration::ZERO {
            tokio::time::sleep(delay).await;
        }

        requests_sent += 1;
        let mut error_ba: Option<String> = None;
        if let Err(e) = send_directed_friend(store, &mut cookies, &mut csrfs, b, a).await {
            if errors.len() < 20 {
                errors.push(format!("{}->{}: {}", b, a, e));
            }
            error_ba = Some(e);
        }
        update_friend_link(&app, |run| run.pair_finished(a, b, error_ab, error_ba));

        if delay > std::time::Duration::ZERO && idx + 1 < needed.len() {
            tokio::time::sleep(delay).await;
        }
    }

    // ── Phase 3: verify which needed pairs actually became friends. ──
    let mut verified_ok = 0usize;
    if !needed.is_empty() {
        update_friend_link(&app, |run| run.set_phase("verifying"));
        let mut after: std::collections::HashMap<i64, std::collections::HashSet<i64>> =
            std::collections::HashMap::new();
        for &uid in ids.iter() {
            after.insert(uid, fetch_friend_set(store, uid).await);
        }
        verified_ok = needed
            .iter()
            .filter(|&&(a, b)| friend_sets_contain(&after, a, b))
            .count();
        // Par que não virou amizade marca as duas contas, mesmo sem erro de
        // envio: o pedido saiu, a amizade não se formou.
        let nao_formados: Vec<(i64, i64)> = needed
            .iter()
            .copied()
            .filter(|&(a, b)| !friend_sets_contain(&after, a, b))
            .collect();
        update_friend_link(&app, |run| {
            for (a, b) in nao_formados {
                run.pair_not_verified(a, b, "A amizade não se formou");
            }
        });
    }
    let failed = needed.len() - verified_ok;

    update_friend_link(&app, |run| run.finish());

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
    read_without_refresh(state.inner(), user_id, |cookie| async move {
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
    read_without_refresh(state.inner(), user_id, |cookie| async move {
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

            extend_unique(&mut all, batch);
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

/// Junta a página nova **sem repetir servidor**.
///
/// A lista do Roblox se mexe entre uma página e outra (jogador entra, jogador
/// sai), então o mesmo Job ID volta em páginas diferentes — em dados reais, 50
/// repetidos em 400. Isso inflava a contagem de "servidores examinados" e,
/// principalmente, mandava ids repetidos para a UI, onde viravam chaves de
/// lista duplicadas e travavam a reordenação do React.
fn extend_unique(all: &mut Vec<api::roblox::ServerData>, batch: Vec<api::roblox::ServerData>) {
    let seen: std::collections::HashSet<String> = all.iter().map(|s| s.id.clone()).collect();
    let mut seen = seen;
    for server in batch {
        if server.id.trim().is_empty() || !seen.insert(server.id.clone()) {
            continue;
        }
        all.push(server);
    }
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
    // Os que cabem o lote vão na frente **antes** do corte: a varredura pode
    // achar milhares de servidores e o recorte não pode ser o que esconde os
    // poucos que servem.
    let ranked = api::roblox::rank_servers(all, preference, accounts);
    let needed = accounts.max(1) as i32;
    let (fits, rest): (Vec<_>, Vec<_>) = ranked
        .into_iter()
        .partition(|s| s.max_players > 0 && s.playing + needed <= s.max_players);
    let servers: Vec<api::roblox::ServerData> = fits
        .into_iter()
        .chain(rest)
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

#[cfg(test)]
mod server_scan_budget_tests {
    use super::*;

    #[test]
    fn the_default_budget_is_used_when_nothing_is_asked() {
        assert_eq!(scan_page_budget(None), SCAN_DEFAULT_PAGES);
        // Zero página é erro de chamada, não "não procure nada".
        assert_eq!(scan_page_budget(Some(0)), SCAN_DEFAULT_PAGES);
    }

    #[test]
    fn the_user_can_ask_for_more_pages() {
        assert_eq!(scan_page_budget(Some(1)), 1);
        assert_eq!(scan_page_budget(Some(120)), 120);
    }

    /// Mesmo pedindo mais, há um teto: uma varredura sem fim martelaria a API.
    #[test]
    fn the_budget_is_capped() {
        assert_eq!(scan_page_budget(Some(usize::MAX)), SCAN_HARD_MAX_PAGES);
        assert_eq!(
            scan_page_budget(Some(SCAN_HARD_MAX_PAGES + 1)),
            SCAN_HARD_MAX_PAGES
        );
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

/// Escolhe o cookie de "quem está olhando" para uma chamada de presença sem
/// `viewer_user_id` explícito: prefere uma conta válida com cookie não vazio e
/// cai para qualquer conta com cookie não vazio se nenhuma válida sobrar.
///
/// Ressalva: o `gameId` (job id) que a Roblox devolve para as outras contas
/// passa a depender da privacidade/configuração de amigos de quem foi
/// escolhida como "viewer" — é assim que a API funciona, não uma falha daqui.
fn pick_viewer_cookie(accounts: &[data::accounts::Account]) -> Option<String> {
    accounts
        .iter()
        .find(|account| account.valid && !account.security_token.trim().is_empty())
        .or_else(|| accounts.iter().find(|account| !account.security_token.trim().is_empty()))
        .map(|account| account.security_token.clone())
}

/// Presença dos usuários pedidos, com o cookie de uma conta quando possível —
/// sem cookie a Roblox devolve a versão degradada, sem `gameId`, e a lista
/// perde as bolinhas In Game/Online precisas (`src/store.tsx`).
///
/// **Nunca** usa `run_with_session_retry` aqui: é leitura, e o refresh chama
/// `signoutfromallsessionsandreauthenticate`, que derrubaria as sessões
/// abertas da conta escolhida só para colorir bolinhas de presença.
///
/// **Fallback obrigatório:** se a chamada autenticada falhar (cookie morto,
/// rede, o que for), repete sem cookie em vez de propagar o erro — uma única
/// conta com sessão inválida não pode apagar a presença de todo mundo.
async fn presence_with_viewer_cookie(
    state: &AccountStore,
    user_ids: &[i64],
    viewer_user_id: Option<i64>,
) -> Result<Vec<api::roblox::UserPresence>, String> {
    let cookie = viewer_user_id
        .and_then(|id| get_cookie(state, id).ok())
        .filter(|token| !token.trim().is_empty())
        .or_else(|| state.get_all().ok().and_then(|accounts| pick_viewer_cookie(&accounts)));

    if let Some(token) = cookie {
        if let Ok(presences) = api::roblox::get_presence_as(Some(&token), user_ids).await {
            return Ok(presences);
        }
    }

    api::roblox::get_presence(user_ids).await
}

#[tauri::command]
async fn get_presence(
    state: tauri::State<'_, AccountStore>,
    user_ids: Vec<i64>,
    viewer_user_id: Option<i64>,
) -> Result<Vec<api::roblox::UserPresence>, String> {
    presence_with_viewer_cookie(state.inner(), &user_ids, viewer_user_id).await
}

/// Onde uma conta esta jogando agora.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountGameLocation {
    user_id: i64,
    in_game: bool,
    /// Place do jogo (`rootPlaceId` quando existe: e o que a tela usa).
    place_id: Option<i64>,
    /// Job id do servidor. **So vem com o cookie da propria conta** — sem ele a
    /// API do Roblox omite o campo.
    job_id: Option<String>,
}

/// Descobre o place e o servidor de uma conta que ja esta em jogo.
///
/// Serve para ligar o Botting Mode numa conta que o usuario lancou por fora:
/// sem saber onde ela esta, o primeiro ciclo a relancaria no place da sessao e
/// a tiraria do servidor em que estava.
///
/// **Sem `run_with_session_retry`** de proposito: e leitura, e o refresh chama
/// `signoutfromallsessionsandreauthenticate`, que derruba justamente o cliente
/// aberto que se quer preservar.
#[tauri::command]
async fn get_account_game_location(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
) -> Result<AccountGameLocation, String> {
    get_account_game_location_inner(state.inner(), user_id).await
}

/// O miolo, sem o `State` do Tauri, para o teste poder chamar.
async fn get_account_game_location_inner(
    state: &AccountStore,
    user_id: i64,
) -> Result<AccountGameLocation, String> {
    let presences = read_without_refresh(state, user_id, |cookie| async move {
        api::roblox::get_presence_as(Some(&cookie), &[user_id]).await
    })
    .await?;

    let found = presences.into_iter().find(|p| p.user_id == user_id);
    let Some(presence) = found else {
        return Ok(AccountGameLocation {
            user_id,
            in_game: false,
            place_id: None,
            job_id: None,
        });
    };

    // 2 = InGame na API de presenca do Roblox.
    let in_game = presence.user_presence_type == 2;
    Ok(AccountGameLocation {
        user_id,
        in_game,
        place_id: if in_game {
            presence.root_place_id.or(presence.place_id)
        } else {
            None
        },
        job_id: if in_game {
            presence.game_id.filter(|j| !j.is_empty())
        } else {
            None
        },
    })
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

/// Acesso de uma conta a um servidor, para a aba Servidores.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountAccess {
    user_id: i64,
    /// Recusada por falta de permissão (o erro 524 do cliente).
    denied: bool,
    /// Outro motivo de não dar para saber (servidor cheio, rede, cookie).
    error: Option<String>,
}

/// Pergunta, **uma conta por vez** e com a pausa da região, se cada conta
/// pode entrar em `job_id`. Permissão costuma ser do place (regra de acesso
/// do jogo, teleporte, VC) e não do servidor, então um servidor responde pela
/// lista inteira — é o que deixa barato perguntar por todas as contas, em vez
/// de só pela primeira como a verificação das regiões faz.
#[tauri::command]
async fn check_place_access(
    state: tauri::State<'_, AccountStore>,
    user_ids: Vec<i64>,
    place_id: i64,
    job_id: String,
) -> Result<Vec<AccountAccess>, String> {
    let delay = std::time::Duration::from_millis(api::roblox::REGION_LOOKUP_DELAY_MS);
    let mut out = Vec::with_capacity(user_ids.len());
    for (index, user_id) in user_ids.into_iter().enumerate() {
        if index > 0 {
            tokio::time::sleep(delay).await;
        }
        let access = match get_cookie(state.inner(), user_id) {
            Err(error) => AccountAccess { user_id, denied: false, error: Some(error) },
            Ok(cookie) => match api::roblox::join_access(&cookie, place_id, &job_id).await {
                Ok(()) => AccountAccess { user_id, denied: false, error: None },
                Err(refusal) => AccountAccess {
                    user_id,
                    denied: refusal.denied(),
                    error: Some(refusal.message),
                },
            },
        };
        out.push(access);
    }
    Ok(out)
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

#[cfg(test)]
mod account_api_tests {
    use super::*;

    fn temp_store(tag: &str) -> AccountStore {
        crypto::init();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        AccountStore::new(std::env::temp_dir().join(format!("ram-apitests-{tag}-{nanos}.json")))
    }

    fn account(user_id: i64, token: &str) -> crate::data::accounts::Account {
        crate::data::accounts::Account::new(token.to_string(), format!("user{user_id}"), user_id)
    }

    // ---- is_auth_session_error ----------------------------------------------

    #[test]
    fn is_auth_session_error_matches_every_known_signal() {
        for message in [
            "Request failed with status 401",
            "[401 Unauthorized] body",
            "Unauthorized",
            "User is not authenticated",
            "not authenticated",
            "Invalid cookie",
            "Authorization has been denied for this request",
            "{\"errors\":[{\"code\":9002}]}",
            "error code 9002",
        ] {
            assert!(is_auth_session_error(message), "should match: {message}");
        }
    }

    #[test]
    fn is_auth_session_error_is_case_insensitive() {
        assert!(is_auth_session_error("STATUS 401"));
        assert!(is_auth_session_error("UNAUTHORIZED"));
        assert!(is_auth_session_error("Invalid Cookie"));
    }

    #[test]
    fn is_auth_session_error_ignores_unrelated_failures() {
        // Regression guard: a false positive here triggers a session refresh,
        // which signs the account out of every running client.
        for message in [
            "",
            "network timeout",
            "Failed to send friend request (status 429): too many requests",
            "status 403 Token Validation Failed",
            "User is moderated",
            "status 500",
        ] {
            assert!(!is_auth_session_error(message), "should not match: {message}");
        }
    }

    #[test]
    fn session_relogin_error_is_the_message_the_ui_shows() {
        assert_eq!(
            session_relogin_error(),
            "Roblox invalidated this session. Re-login required."
        );
        assert!(!is_auth_session_error(&session_relogin_error()));
    }

    // ---- store helpers -------------------------------------------------------

    #[test]
    fn get_account_finds_the_account_or_names_the_missing_id() {
        let store = temp_store("get-account");
        store.add(account(11, "TOK")).unwrap();

        assert_eq!(get_account(&store, 11).unwrap().security_token, "TOK");
        assert_eq!(
            get_account(&store, 12).unwrap_err(),
            "Account 12 not found"
        );
    }

    #[test]
    fn mark_refresh_attempt_stamps_and_persists_the_attempt_time() {
        let store = temp_store("mark-refresh");
        let mut original = account(21, "TOK");
        original.last_attempted_refresh = chrono::DateTime::parse_from_rfc3339(
            "2000-01-01T00:00:00Z",
        )
        .unwrap()
        .with_timezone(&chrono::Utc);
        store.add(original.clone()).unwrap();
        store.update(original.clone()).unwrap();

        let before = store.get_all().unwrap()[0].last_attempted_refresh;
        let returned = mark_refresh_attempt(&store, 21).unwrap();
        let stored = store.get_all().unwrap()[0].last_attempted_refresh;

        assert!(returned.last_attempted_refresh > before);
        assert_eq!(stored, returned.last_attempted_refresh);
    }

    #[test]
    fn mark_refresh_attempt_fails_for_an_unknown_account() {
        let store = temp_store("mark-refresh-missing");
        assert_eq!(
            mark_refresh_attempt(&store, 5).unwrap_err(),
            "Account 5 not found"
        );
    }

    #[test]
    fn persist_cookie_update_replaces_the_token_and_revalidates() {
        let store = temp_store("persist-cookie");
        let mut acc = account(31, "OLD");
        acc.valid = false;
        store.add(acc.clone()).unwrap();
        store.update(acc).unwrap();

        persist_cookie_update(&store, 31, "NEW").unwrap();

        let stored = store.get_all().unwrap().remove(0);
        assert_eq!(stored.security_token, "NEW");
        assert!(stored.valid);
    }

    #[test]
    fn persist_cookie_update_fails_for_an_unknown_account() {
        let store = temp_store("persist-cookie-missing");
        assert!(persist_cookie_update(&store, 7, "NEW").is_err());
    }

    // ---- pick_viewer_cookie ----------------------------------------------------

    #[test]
    fn pick_viewer_cookie_prefers_a_valid_account_over_an_invalid_one() {
        let mut invalid = account(1, "INVALID-TOKEN");
        invalid.valid = false;
        let mut valid = account(2, "VALID-TOKEN");
        valid.valid = true;

        assert_eq!(
            pick_viewer_cookie(&[invalid, valid]),
            Some("VALID-TOKEN".to_string())
        );
    }

    #[test]
    fn pick_viewer_cookie_falls_back_to_an_invalid_account_when_none_is_valid() {
        let mut only = account(3, "ONLY-TOKEN");
        only.valid = false;

        assert_eq!(pick_viewer_cookie(&[only]), Some("ONLY-TOKEN".to_string()));
    }

    #[test]
    fn pick_viewer_cookie_skips_accounts_with_an_empty_token() {
        let mut valid_no_token = account(4, "");
        valid_no_token.valid = true;
        let mut invalid_with_token = account(5, "HAS-TOKEN");
        invalid_with_token.valid = false;

        assert_eq!(
            pick_viewer_cookie(&[valid_no_token, invalid_with_token]),
            Some("HAS-TOKEN".to_string())
        );
    }

    #[test]
    fn pick_viewer_cookie_on_an_empty_store_is_none() {
        assert_eq!(pick_viewer_cookie(&[]), None);
    }

    // ---- dedupe_friend_ids ---------------------------------------------------

    #[test]
    fn dedupe_friend_ids_keeps_the_first_occurrence_in_order() {
        assert_eq!(dedupe_friend_ids(vec![3, 1, 3, 2, 1]), vec![3, 1, 2]);
    }

    #[test]
    fn dedupe_friend_ids_drops_zero_and_negative_ids() {
        // A zero/negative id is never a real Roblox account.
        assert_eq!(dedupe_friend_ids(vec![0, -1, 5, i64::MIN]), vec![5]);
        assert_eq!(dedupe_friend_ids(vec![]), Vec::<i64>::new());
        assert_eq!(dedupe_friend_ids(vec![0]), Vec::<i64>::new());
    }

    #[test]
    fn dedupe_friend_ids_keeps_large_ids() {
        assert_eq!(dedupe_friend_ids(vec![i64::MAX, i64::MAX]), vec![i64::MAX]);
    }

    // ---- build_friend_pairs --------------------------------------------------

    #[test]
    fn build_friend_pairs_mesh_covers_every_combination_once() {
        let pairs = build_friend_pairs(&[1, 2, 3], "mesh", None).unwrap();
        assert_eq!(pairs, vec![(1, 2), (1, 3), (2, 3)]);
    }

    #[test]
    fn build_friend_pairs_mesh_grows_quadratically() {
        for n in 2..=8_usize {
            let ids: Vec<i64> = (1..=n as i64).collect();
            let pairs = build_friend_pairs(&ids, "mesh", None).unwrap();
            assert_eq!(pairs.len(), n * (n - 1) / 2, "n = {n}");
        }
    }

    #[test]
    fn build_friend_pairs_mesh_of_fewer_than_two_accounts_is_empty() {
        assert!(build_friend_pairs(&[], "mesh", None).unwrap().is_empty());
        assert!(build_friend_pairs(&[1], "mesh", None).unwrap().is_empty());
    }

    #[test]
    fn build_friend_pairs_star_pairs_everyone_with_the_main_account() {
        let pairs = build_friend_pairs(&[1, 2, 3], "star", Some(2)).unwrap();
        assert_eq!(pairs, vec![(2, 1), (2, 3)]);
        assert_eq!(pairs.len(), 2);
    }

    #[test]
    fn build_friend_pairs_star_requires_a_main_inside_the_selection() {
        let expected = "Star mode requires a main account within the selection.";
        assert_eq!(
            build_friend_pairs(&[1, 2, 3], "star", None).unwrap_err(),
            expected
        );
        assert_eq!(
            build_friend_pairs(&[1, 2, 3], "star", Some(99)).unwrap_err(),
            expected
        );
    }

    #[test]
    fn build_friend_pairs_star_never_pairs_the_main_with_itself() {
        let pairs = build_friend_pairs(&[7], "star", Some(7)).unwrap();
        assert!(pairs.is_empty());
    }

    #[test]
    fn build_friend_pairs_accepts_any_casing_and_padding_of_the_mode() {
        assert_eq!(
            build_friend_pairs(&[1, 2], "  MESH  ", None).unwrap(),
            vec![(1, 2)]
        );
        assert_eq!(
            build_friend_pairs(&[1, 2], "Star", Some(1)).unwrap(),
            vec![(1, 2)]
        );
    }

    #[test]
    fn build_friend_pairs_rejects_an_unknown_mode_and_echoes_it() {
        assert_eq!(
            build_friend_pairs(&[1, 2], "ring", None).unwrap_err(),
            "Unknown mode: ring"
        );
        assert_eq!(
            build_friend_pairs(&[1, 2], "", None).unwrap_err(),
            "Unknown mode: "
        );
    }

    // ---- resolve_friend_delay_ms ---------------------------------------------

    #[test]
    fn resolve_friend_delay_ms_defaults_to_2500_ms() {
        assert_eq!(resolve_friend_delay_ms(None, None), 2500);
    }

    #[test]
    fn resolve_friend_delay_ms_prefers_the_explicit_argument() {
        assert_eq!(resolve_friend_delay_ms(Some(1000), Some(9000)), 1000);
    }

    #[test]
    fn resolve_friend_delay_ms_falls_back_to_the_setting() {
        assert_eq!(resolve_friend_delay_ms(None, Some(4000)), 4000);
    }

    #[test]
    fn resolve_friend_delay_ms_clamps_into_the_500_to_60000_window() {
        // Too fast trips Roblox's rate limiter; too slow stalls the run.
        assert_eq!(resolve_friend_delay_ms(Some(0), None), 500);
        assert_eq!(resolve_friend_delay_ms(Some(1), None), 500);
        assert_eq!(resolve_friend_delay_ms(Some(u64::MAX), None), 60_000);
        assert_eq!(resolve_friend_delay_ms(None, Some(1)), 500);
        assert_eq!(resolve_friend_delay_ms(None, Some(i64::MAX)), 60_000);
    }

    #[test]
    fn resolve_friend_delay_ms_ignores_a_negative_setting() {
        // `u64::try_from` fails, so the default applies instead of wrapping.
        assert_eq!(resolve_friend_delay_ms(None, Some(-1)), 2500);
        assert_eq!(resolve_friend_delay_ms(None, Some(i64::MIN)), 2500);
    }

    // ---- friend_sets_contain --------------------------------------------------

    fn sets(entries: &[(i64, &[i64])]) -> std::collections::HashMap<i64, std::collections::HashSet<i64>> {
        entries
            .iter()
            .map(|(id, friends)| (*id, friends.iter().copied().collect()))
            .collect()
    }

    #[test]
    fn friend_sets_contain_matches_in_either_direction() {
        let map = sets(&[(1, &[2]), (2, &[])]);
        assert!(friend_sets_contain(&map, 1, 2));
        assert!(friend_sets_contain(&map, 2, 1));
    }

    #[test]
    fn friend_sets_contain_is_false_when_neither_side_lists_the_other() {
        let map = sets(&[(1, &[3]), (2, &[4])]);
        assert!(!friend_sets_contain(&map, 1, 2));
    }

    #[test]
    fn friend_sets_contain_treats_a_missing_entry_as_not_friends() {
        // A failed friend-list fetch must not make us skip the pair.
        let map = sets(&[]);
        assert!(!friend_sets_contain(&map, 1, 2));
        let map = sets(&[(1, &[])]);
        assert!(!friend_sets_contain(&map, 1, 2));
    }

    // ---- the one-run-at-a-time guard ------------------------------------------

    #[test]
    fn friend_link_guard_allows_one_run_at_a_time_and_releases_on_drop() {
        // Regression guard: a remounted dialog starting a second batch doubles
        // the requests and the rate-limit/captcha risk.
        {
            let _first = try_begin_friend_link().expect("first run should start");
            assert_eq!(
                try_begin_friend_link().err(),
                Some("Já existe uma vinculação de amizades em andamento.".to_string())
            );
        }
        // The guard released the lock, so a later run is allowed again.
        let _again = try_begin_friend_link().expect("a later run should start");
    }

    // ---- payload shapes --------------------------------------------------------

    #[test]
    fn friend_link_result_serializes_with_the_camel_case_keys_the_ui_reads() {
        let json = serde_json::to_value(FriendLinkResult {
            pairs_total: 3,
            already_friends: 1,
            attempted: 2,
            verified_ok: 2,
            failed: 0,
            requests_sent: 4,
            errors: vec!["1->2: boom".to_string()],
        })
        .unwrap();

        assert_eq!(json["pairsTotal"], 3);
        assert_eq!(json["alreadyFriends"], 1);
        assert_eq!(json["attempted"], 2);
        assert_eq!(json["verifiedOk"], 2);
        assert_eq!(json["failed"], 0);
        assert_eq!(json["requestsSent"], 4);
        assert_eq!(json["errors"][0], "1->2: boom");
    }

    /// Contrato de serializacao: campo novo aqui tem que aparecer no teste,
    /// senao a UI le `undefined` sem ninguem perceber.
    #[test]
    fn friend_link_state_serializes_with_the_camel_case_keys_the_ui_reads() {
        let mut run = FriendLinkRun::default();
        run.start(&[1, 2, 3], "star", Some(1));
        run.set_phase("linking");
        run.plan_pairs(&[(1, 2), (1, 3)]);
        run.pair_started(1, 2);
        run.pair_finished(1, 2, Some("boom".to_string()), None);

        let json = serde_json::to_value(run.snapshot()).unwrap();
        assert_eq!(json["active"], true);
        assert_eq!(json["phase"], "linking");
        assert_eq!(json["total"], 3);
        assert_eq!(json["mode"], "star");
        assert_eq!(json["mainUserId"], 1);
        assert_eq!(json["accounts"][0]["userId"], 1);
        assert_eq!(json["accounts"][0]["state"], "failed");
        assert_eq!(json["accounts"][0]["error"], "boom");
        assert_eq!(json["accounts"][1]["state"], "done");
        assert_eq!(json["processed"], 2);
    }

    /// O progresso antigo contava **pares** na fase de envio. Em `star` com 4
    /// contas sao 3 pares: "2 de 3" nao dizia nada sobre quantas contas ja
    /// tinham terminado, que e o que o usuario pediu.
    #[test]
    fn a_conta_so_termina_quando_todos_os_pares_dela_acabam() {
        let mut run = FriendLinkRun::default();
        run.start(&[1, 2, 3], "mesh", None);
        run.plan_pairs(&[(1, 2), (1, 3), (2, 3)]);
        assert_eq!(run.processed(), 0);

        run.pair_finished(1, 2, None, None);
        // 1 e 2 ainda tem um par cada: ninguem terminou.
        assert_eq!(run.processed(), 0);

        run.pair_finished(1, 3, None, None);
        assert_eq!(run.snapshot().accounts[0].state, "done", "a conta 1 acabou");
        assert_eq!(run.processed(), 1);

        run.pair_finished(2, 3, None, None);
        assert_eq!(run.processed(), 3);
    }

    #[test]
    fn conta_ja_amiga_de_todo_mundo_termina_de_saida() {
        let mut run = FriendLinkRun::default();
        run.start(&[1, 2, 3], "mesh", None);
        // A conta 3 nao aparece em par nenhum: nao existiria evento para
        // conclui-la, e ela ficaria "aguardando" para sempre na tela.
        run.plan_pairs(&[(1, 2)]);
        assert_eq!(run.snapshot().accounts[2].state, "done");
        assert_eq!(run.processed(), 1);
    }

    #[test]
    fn o_erro_fica_na_conta_que_enviou_o_pedido_que_falhou() {
        let mut run = FriendLinkRun::default();
        run.start(&[1, 2], "mesh", None);
        run.plan_pairs(&[(1, 2)]);
        run.pair_finished(1, 2, None, Some("cookie invalido".to_string()));

        let snap = run.snapshot();
        assert_eq!(snap.accounts[0].state, "done");
        assert_eq!(snap.accounts[1].state, "failed");
        assert_eq!(snap.accounts[1].error.as_deref(), Some("cookie invalido"));
    }

    #[test]
    fn terminar_a_operacao_nao_apaga_o_erro_de_quem_falhou() {
        let mut run = FriendLinkRun::default();
        run.start(&[1, 2], "mesh", None);
        run.plan_pairs(&[(1, 2)]);
        run.pair_finished(1, 2, Some("boom".to_string()), None);
        run.finish();

        let snap = run.snapshot();
        assert_eq!(snap.active, false);
        assert_eq!(snap.phase, "done");
        assert_eq!(snap.accounts[0].state, "failed");
        assert_eq!(snap.accounts[0].error.as_deref(), Some("boom"));
        assert_eq!(snap.processed, 2);
    }

    #[test]
    fn par_que_nao_virou_amizade_marca_as_duas_contas() {
        let mut run = FriendLinkRun::default();
        run.start(&[1, 2], "mesh", None);
        run.plan_pairs(&[(1, 2)]);
        run.pair_finished(1, 2, None, None);
        run.pair_not_verified(1, 2, "A amizade nao se formou");

        let snap = run.snapshot();
        assert_eq!(snap.accounts[0].state, "failed");
        assert_eq!(snap.accounts[1].state, "failed");
    }

    /// Antes de qualquer execucao a tela pergunta o estado: nao pode explodir
    /// nem fingir que ha operacao em andamento.
    #[test]
    fn o_retrato_inicial_e_vazio_e_inativo() {
        let snap = FriendLinkRun::default().snapshot();
        assert_eq!(snap.active, false);
        assert_eq!(snap.phase, "idle");
        assert_eq!(snap.total, 0);
        assert!(snap.accounts.is_empty());
    }
}

#[cfg(test)]
mod account_api_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server, mount_csrf};
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, Request, ResponseTemplate};

    fn temp_store(tag: &str) -> AccountStore {
        crypto::init();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        AccountStore::new(std::env::temp_dir().join(format!("ram-apihttp-{tag}-{nanos}.json")))
    }

    fn store_with(user_id: i64, token: &str, tag: &str) -> AccountStore {
        let store = temp_store(tag);
        store
            .add(crate::data::accounts::Account::new(
                token.to_string(),
                format!("user{user_id}"),
                user_id,
            ))
            .unwrap();
        store
    }

    /// Mounts the sign-out endpoint `refresh_account_session` calls, optionally
    /// handing back a fresh `.ROBLOSECURITY`.
    async fn mount_signout(token: &str, new_cookie: Option<&str>, status: u16) {
        let mut response = ResponseTemplate::new(status);
        if let Some(cookie) = new_cookie {
            response = response.insert_header(
                "set-cookie",
                format!(".ROBLOSECURITY={}; path=/; HttpOnly", cookie).as_str(),
            );
        }
        Mock::given(method("POST"))
            .and(path(mock_path(
                "www",
                "/authentication/signoutfromallsessionsandreauthenticate",
            )))
            .and(header("cookie", cookie_of(token)))
            .respond_with(response)
            .mount(mock_server().await)
            .await;
    }

    // ---- refresh_account_session ---------------------------------------------

    #[tokio::test]
    async fn refresh_account_session_stores_the_new_cookie() {
        mount_csrf("refresh-ok", "CSRF-REFRESH-OK").await;
        mount_signout("refresh-ok", Some("REFRESHED-COOKIE"), 200).await;
        let store = store_with(9001, "refresh-ok", "refresh-ok");

        let cookie = refresh_account_session(&store, 9001).await.unwrap();

        assert_eq!(cookie, "REFRESHED-COOKIE");
        let stored = store.get_all().unwrap().remove(0);
        assert_eq!(stored.security_token, "REFRESHED-COOKIE");
        assert!(stored.valid);
    }

    #[tokio::test]
    async fn refresh_account_session_reports_relogin_when_no_cookie_comes_back() {
        mount_csrf("refresh-nocookie", "CSRF-REFRESH-NOCOOKIE").await;
        mount_signout("refresh-nocookie", None, 200).await;
        let store = store_with(9002, "refresh-nocookie", "refresh-nocookie");

        assert_eq!(
            refresh_account_session(&store, 9002).await.unwrap_err(),
            session_relogin_error()
        );
        // The old cookie must stay untouched so the user can still re-login.
        assert_eq!(
            store.get_all().unwrap()[0].security_token,
            "refresh-nocookie"
        );
    }

    #[tokio::test]
    async fn refresh_account_session_surfaces_a_failing_signout() {
        mount_csrf("refresh-fail", "CSRF-REFRESH-FAIL").await;
        mount_signout("refresh-fail", None, 500).await;
        let store = store_with(9003, "refresh-fail", "refresh-fail");

        let err = refresh_account_session(&store, 9003).await.unwrap_err();
        assert!(err.contains("sign out other sessions"), "{err}");
    }

    #[tokio::test]
    async fn refresh_account_session_fails_for_an_unknown_account() {
        let store = temp_store("refresh-unknown");
        assert_eq!(
            refresh_account_session(&store, 4242).await.unwrap_err(),
            "Account 4242 not found"
        );
    }

    // ---- run_with_session_retry ------------------------------------------------

    #[tokio::test]
    async fn run_with_session_retry_passes_the_cookie_through_on_success() {
        let store = store_with(9010, "plain-cookie", "retry-ok");
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let recorder = seen.clone();

        let value = run_with_session_retry(&store, 9010, move |cookie| {
            let recorder = recorder.clone();
            async move {
                recorder.lock().unwrap().push(cookie);
                Ok::<_, String>(7)
            }
        })
        .await
        .unwrap();

        assert_eq!(value, 7);
        assert_eq!(*seen.lock().unwrap(), vec!["plain-cookie".to_string()]);
    }

    #[tokio::test]
    async fn auth_ticket_for_the_clipboard_never_refreshes_the_session() {
        // O ticket só alimenta os links de debug que o menu de contexto copia:
        // e leitura não crítica não pode passar por `run_with_session_retry`, que
        // chama `signoutfromallsessionsandreauthenticate` e derruba as sessões
        // abertas da conta. Cookie velho aqui tem de virar erro na tela.
        mount_csrf("ticket-stale", "CSRF-TICKET-STALE").await;
        mount_signout("ticket-stale", Some("ticket-fresh"), 200).await;
        // O mock server é compartilhado entre os testes: filtre pelo cookie desta
        // conta, senão esta rota responde pelos outros testes de ticket também.
        Mock::given(method("POST"))
            .and(path(mock_path("auth", "/v1/authentication-ticket/")))
            .and(header("cookie", cookie_of("ticket-stale")))
            .respond_with(ResponseTemplate::new(401))
            .mount(mock_server().await)
            .await;
        let store = store_with(9030, "ticket-stale", "ticket-norefresh");

        let err = auth_ticket_without_refresh(&store, 9030).await.unwrap_err();

        assert!(err.contains("401"), "erro inesperado: {err}");
        // O cookie continua o mesmo: nenhum refresh aconteceu.
        assert_eq!(store.get_all().unwrap()[0].security_token, "ticket-stale");
    }

    #[tokio::test]
    async fn run_with_session_retry_never_refreshes_on_an_unrelated_error() {
        // Regression guard: refreshing signs the account out of every session,
        // so a 429 or a network blip must not trigger it.
        let store = store_with(9011, "no-refresh", "retry-unrelated");
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = calls.clone();

        let err = run_with_session_retry(&store, 9011, move |_cookie| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err::<(), String>("status 429 too many requests".to_string())
            }
        })
        .await
        .unwrap_err();

        assert_eq!(err, "status 429 too many requests");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(store.get_all().unwrap()[0].security_token, "no-refresh");
    }

    #[tokio::test]
    async fn run_with_session_retry_refreshes_once_and_retries_with_the_new_cookie() {
        mount_csrf("retry-stale", "CSRF-RETRY-STALE").await;
        mount_signout("retry-stale", Some("retry-fresh"), 200).await;
        let store = store_with(9012, "retry-stale", "retry-refresh");

        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let recorder = seen.clone();

        let value = run_with_session_retry(&store, 9012, move |cookie| {
            let recorder = recorder.clone();
            async move {
                recorder.lock().unwrap().push(cookie.clone());
                if cookie == "retry-stale" {
                    Err("Request failed with status 401".to_string())
                } else {
                    Ok(42)
                }
            }
        })
        .await
        .unwrap();

        assert_eq!(value, 42);
        assert_eq!(
            *seen.lock().unwrap(),
            vec!["retry-stale".to_string(), "retry-fresh".to_string()]
        );
        assert_eq!(store.get_all().unwrap()[0].security_token, "retry-fresh");
    }

    #[tokio::test]
    async fn run_with_session_retry_reports_relogin_when_the_retry_also_fails() {
        mount_csrf("retry-doomed", "CSRF-RETRY-DOOMED").await;
        mount_signout("retry-doomed", Some("retry-doomed-2"), 200).await;
        let store = store_with(9013, "retry-doomed", "retry-doomed");
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = calls.clone();

        let err = run_with_session_retry(&store, 9013, move |_cookie| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err::<(), String>("user is not authenticated".to_string())
            }
        })
        .await
        .unwrap_err();

        assert_eq!(err, session_relogin_error());
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "the operation must be attempted exactly twice"
        );
    }

    #[tokio::test]
    async fn run_with_session_retry_reports_relogin_when_the_refresh_itself_is_unauthorized() {
        mount_csrf("retry-refresh-401", "CSRF-RETRY-R401").await;
        mount_signout("retry-refresh-401", None, 401).await;
        let store = store_with(9014, "retry-refresh-401", "retry-refresh-401");

        let err = run_with_session_retry(&store, 9014, |_cookie| async move {
            Err::<(), String>("invalid cookie".to_string())
        })
        .await
        .unwrap_err();

        assert_eq!(err, session_relogin_error());
    }

    #[tokio::test]
    async fn run_with_session_retry_fails_before_running_for_an_unknown_account() {
        let store = temp_store("retry-unknown");
        let err = run_with_session_retry(&store, 777, |_cookie| async move {
            panic!("the operation must not run without a cookie");
            #[allow(unreachable_code)]
            Ok::<(), String>(())
        })
        .await
        .unwrap_err();
        assert_eq!(err, "Account 777 not found");
    }

    // ---- fetch_friend_set --------------------------------------------------------

    #[tokio::test]
    async fn fetch_friend_set_collects_the_friend_ids() {
        Mock::given(method("GET"))
            .and(path(mock_path("friends", "/v1/users/9020/friends")))
            .and(header("cookie", cookie_of("friends-ok")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "id": 1 }, { "id": 2 }, { "name": "no id" }]
            })))
            .mount(mock_server().await)
            .await;
        let store = store_with(9020, "friends-ok", "friends-ok");

        let set = fetch_friend_set(&store, 9020).await;
        assert_eq!(set.len(), 2);
        assert!(set.contains(&1));
        assert!(set.contains(&2));
    }

    #[tokio::test]
    async fn fetch_friend_set_is_empty_when_the_lookup_fails() {
        // Tolerated failure: an empty set only means "link this pair anyway".
        Mock::given(method("GET"))
            .and(path(mock_path("friends", "/v1/users/9021/friends")))
            .and(header("cookie", cookie_of("friends-fail")))
            .respond_with(ResponseTemplate::new(500))
            .mount(mock_server().await)
            .await;
        let store = store_with(9021, "friends-fail", "friends-fail");

        assert!(fetch_friend_set(&store, 9021).await.is_empty());
    }

    #[tokio::test]
    async fn fetch_friend_set_is_empty_for_an_account_without_a_cookie() {
        let store = temp_store("friends-unknown");
        assert!(fetch_friend_set(&store, 9022).await.is_empty());
    }

    // ---- send_directed_friend -----------------------------------------------------

    #[tokio::test]
    async fn send_directed_friend_fetches_the_csrf_once_and_caches_it() {
        mount_csrf("directed-ok", "CSRF-DIRECTED-OK").await;
        Mock::given(method("POST"))
            .and(path(mock_path("friends", "/v1/users/9031/request-friendship")))
            .and(header("cookie", cookie_of("directed-ok")))
            .and(header("x-csrf-token", "CSRF-DIRECTED-OK"))
            .respond_with(ResponseTemplate::new(200))
            .mount(mock_server().await)
            .await;
        Mock::given(method("POST"))
            .and(path(mock_path("friends", "/v1/users/9032/request-friendship")))
            .and(header("cookie", cookie_of("directed-ok")))
            .and(header("x-csrf-token", "CSRF-DIRECTED-OK"))
            .respond_with(ResponseTemplate::new(200))
            .mount(mock_server().await)
            .await;

        let store = store_with(9030, "directed-ok", "directed-ok");
        let mut cookies = std::collections::HashMap::new();
        let mut csrfs = std::collections::HashMap::new();

        send_directed_friend(&store, &mut cookies, &mut csrfs, 9030, 9031)
            .await
            .unwrap();
        assert_eq!(cookies.get(&9030).map(String::as_str), Some("directed-ok"));
        assert_eq!(csrfs.get(&9030).map(String::as_str), Some("CSRF-DIRECTED-OK"));

        // Second call reuses the cached cookie and token.
        send_directed_friend(&store, &mut cookies, &mut csrfs, 9030, 9032)
            .await
            .unwrap();
        assert_eq!(csrfs.len(), 1);
    }

    #[tokio::test]
    async fn send_directed_friend_refetches_a_stale_csrf_and_retries_once() {
        mount_csrf("directed-stale", "CSRF-FRESH").await;
        Mock::given(method("POST"))
            .and(path(mock_path("friends", "/v1/users/9041/request-friendship")))
            .and(header("cookie", cookie_of("directed-stale")))
            .and(header("x-csrf-token", "CSRF-STALE"))
            .respond_with(ResponseTemplate::new(403).set_body_string("Token Validation Failed"))
            .mount(mock_server().await)
            .await;
        Mock::given(method("POST"))
            .and(path(mock_path("friends", "/v1/users/9041/request-friendship")))
            .and(header("cookie", cookie_of("directed-stale")))
            .and(header("x-csrf-token", "CSRF-FRESH"))
            .respond_with(ResponseTemplate::new(200))
            .mount(mock_server().await)
            .await;

        let store = store_with(9040, "directed-stale", "directed-stale");
        let mut cookies = std::collections::HashMap::new();
        let mut csrfs = std::collections::HashMap::new();
        csrfs.insert(9040_i64, "CSRF-STALE".to_string());

        send_directed_friend(&store, &mut cookies, &mut csrfs, 9040, 9041)
            .await
            .unwrap();

        assert_eq!(csrfs.get(&9040).map(String::as_str), Some("CSRF-FRESH"));
    }

    #[tokio::test]
    async fn send_directed_friend_reports_an_invalid_session_without_refreshing_it() {
        // Refreshing here would sign the account out of every running client,
        // so the error is surfaced with a hint instead.
        Mock::given(method("POST"))
            .and(path(mock_path("friends", "/v1/users/9051/request-friendship")))
            .and(header("cookie", cookie_of("directed-401")))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(mock_server().await)
            .await;

        let store = store_with(9050, "directed-401", "directed-401");
        let mut cookies = std::collections::HashMap::new();
        let mut csrfs = std::collections::HashMap::new();
        cookies.insert(9050_i64, "directed-401".to_string());
        csrfs.insert(9050_i64, "CSRF-ANY".to_string());

        let err = send_directed_friend(&store, &mut cookies, &mut csrfs, 9050, 9051)
            .await
            .unwrap_err();

        assert!(err.contains("status 401"), "{err}");
        assert!(err.contains("sessão inválida"), "{err}");
        assert_eq!(store.get_all().unwrap()[0].security_token, "directed-401");
    }

    #[tokio::test]
    async fn send_directed_friend_passes_other_errors_through_unchanged() {
        Mock::given(method("POST"))
            .and(path(mock_path("friends", "/v1/users/9061/request-friendship")))
            .and(header("cookie", cookie_of("directed-429")))
            .respond_with(ResponseTemplate::new(429).set_body_string("Too many requests"))
            .mount(mock_server().await)
            .await;

        let store = store_with(9060, "directed-429", "directed-429");
        let mut cookies = std::collections::HashMap::new();
        let mut csrfs = std::collections::HashMap::new();
        cookies.insert(9060_i64, "directed-429".to_string());
        csrfs.insert(9060_i64, "CSRF-ANY".to_string());

        let err = send_directed_friend(&store, &mut cookies, &mut csrfs, 9060, 9061)
            .await
            .unwrap_err();

        assert!(err.contains("status 429"), "{err}");
        assert!(!err.contains("sessão inválida"), "{err}");
    }

    #[tokio::test]
    async fn send_directed_friend_fails_for_a_source_account_that_is_gone() {
        let store = temp_store("directed-missing");
        let mut cookies = std::collections::HashMap::new();
        let mut csrfs = std::collections::HashMap::new();

        assert_eq!(
            send_directed_friend(&store, &mut cookies, &mut csrfs, 8888, 1)
                .await
                .unwrap_err(),
            "Account 8888 not found"
        );
    }
    /// Adotar uma conta que ja esta em jogo exige saber ONDE ela esta: sem
    /// isso o primeiro ciclo do Botting a relancaria no place da sessao e a
    /// tiraria do servidor. O `gameId` (job) so vem com o cookie da conta.
    #[tokio::test]
    async fn the_game_location_comes_from_the_authenticated_presence() {
        let server = mock_server().await;
        let store = store_with(4242, "loc-in-game", "loc-in-game");

        Mock::given(method("POST"))
            .and(path(mock_path("presence", "/v1/presence/users")))
            .and(header("cookie", cookie_of("loc-in-game")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "userPresences": [{
                    "userPresenceType": 2,
                    "lastLocation": "Jailbreak",
                    "placeId": 606849621,
                    "rootPlaceId": 606849621,
                    "gameId": "job-abc",
                    "universeId": 245662005,
                    "userId": 4242,
                    "lastOnline": ""
                }]
            })))
            .mount(server)
            .await;

        let local = super::get_account_game_location_inner(&store, 4242)
            .await
            .expect("presenca lida");

        assert!(local.in_game);
        assert_eq!(local.place_id, Some(606849621));
        assert_eq!(local.job_id.as_deref(), Some("job-abc"));
    }

    /// Conta online mas fora de jogo nao tem place: devolver um place velho
    /// faria a sessao de Botting nascer apontando para o lugar errado.
    #[tokio::test]
    async fn an_account_that_is_not_in_game_reports_no_place() {
        let server = mock_server().await;
        let store = store_with(4243, "loc-online", "loc-online");

        Mock::given(method("POST"))
            .and(path(mock_path("presence", "/v1/presence/users")))
            .and(header("cookie", cookie_of("loc-online")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "userPresences": [{
                    "userPresenceType": 1,
                    "lastLocation": "Website",
                    "placeId": null,
                    "rootPlaceId": null,
                    "gameId": null,
                    "universeId": null,
                    "userId": 4243,
                    "lastOnline": ""
                }]
            })))
            .mount(server)
            .await;

        let local = super::get_account_game_location_inner(&store, 4243)
            .await
            .expect("presenca lida");

        assert!(!local.in_game);
        assert_eq!(local.place_id, None);
        assert_eq!(local.job_id, None);
    }

    /// A leitura **nao** pode passar pelo refresh de sessao: ele chama
    /// `signoutfromallsessionsandreauthenticate`, que derruba justamente o
    /// cliente aberto que se quer adotar. Sem mock de sign-out montado, um
    /// refresh apareceria como falha aqui.
    #[tokio::test]
    async fn reading_the_location_never_refreshes_the_session() {
        let server = mock_server().await;
        let store = store_with(4244, "loc-no-refresh", "loc-no-refresh");

        Mock::given(method("POST"))
            .and(path(mock_path("presence", "/v1/presence/users")))
            .and(header("cookie", cookie_of("loc-no-refresh")))
            .respond_with(ResponseTemplate::new(401))
            .mount(server)
            .await;

        let erro = super::get_account_game_location_inner(&store, 4244)
            .await
            .expect_err("401 vira erro, nao refresh");

        assert!(!erro.to_lowercase().contains("signout"), "{erro}");
    }


    // ---- presence_with_viewer_cookie ------------------------------------------

    #[tokio::test]
    async fn presence_with_viewer_cookie_uses_a_valid_accounts_cookie_when_none_is_named() {
        let store = store_with(9070, "presence-auto", "presence-auto");

        Mock::given(method("POST"))
            .and(path(mock_path("presence", "/v1/presence/users")))
            .and(body_partial_json(serde_json::json!({ "userIds": [7070] })))
            .and(header("cookie", cookie_of("presence-auto")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "userPresences": [{
                    "userPresenceType": 2,
                    "userId": 7070,
                    "placeId": 1,
                    "rootPlaceId": 1,
                    "gameId": "job-auto"
                }]
            })))
            .mount(mock_server().await)
            .await;

        let presences = presence_with_viewer_cookie(&store, &[7070], None)
            .await
            .expect("presence");
        assert_eq!(presences[0].game_id.as_deref(), Some("job-auto"));
    }

    #[tokio::test]
    async fn presence_with_viewer_cookie_prefers_the_explicit_viewer_over_the_stores_pick() {
        let store = store_with(9071, "presence-other", "presence-explicit-a");
        store
            .add(crate::data::accounts::Account::new(
                "presence-explicit".to_string(),
                "user9072".to_string(),
                9072,
            ))
            .unwrap();

        // Só a rota autenticada com o cookie da conta 9072 responde; se o
        // helper mandasse o cookie da 9071 (a primeira do store) a chamada
        // cairia sem mock e o teste falharia.
        Mock::given(method("POST"))
            .and(path(mock_path("presence", "/v1/presence/users")))
            .and(body_partial_json(serde_json::json!({ "userIds": [7071] })))
            .and(header("cookie", cookie_of("presence-explicit")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "userPresences": [{ "userPresenceType": 2, "userId": 7071, "gameId": "job-explicit" }]
            })))
            .mount(mock_server().await)
            .await;

        let presences = presence_with_viewer_cookie(&store, &[7071], Some(9072))
            .await
            .expect("presence");
        assert_eq!(presences[0].game_id.as_deref(), Some("job-explicit"));
    }

    /// Cookie morto na conta escolhida como viewer: a chamada autenticada
    /// falha e o helper tem que cair para a versão anônima em vez de propagar
    /// o erro e apagar a presença de todo mundo (regra do plano, Task 5).
    #[tokio::test]
    async fn presence_with_viewer_cookie_falls_back_to_anonymous_when_the_authenticated_call_fails(
    ) {
        let store = store_with(9073, "presence-dead", "presence-dead");

        Mock::given(method("POST"))
            .and(path(mock_path("presence", "/v1/presence/users")))
            .and(body_partial_json(serde_json::json!({ "userIds": [7073] })))
            .and(header("cookie", cookie_of("presence-dead")))
            .respond_with(ResponseTemplate::new(500))
            .mount(mock_server().await)
            .await;

        Mock::given(method("POST"))
            .and(path(mock_path("presence", "/v1/presence/users")))
            .and(body_partial_json(serde_json::json!({ "userIds": [7073] })))
            .and(|req: &Request| !req.headers.contains_key("cookie"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "userPresences": [{ "userPresenceType": 1, "userId": 7073, "gameId": null }]
            })))
            .mount(mock_server().await)
            .await;

        let presences = presence_with_viewer_cookie(&store, &[7073], None)
            .await
            .expect("presence");
        assert_eq!(presences[0].user_id, 7073);
        assert!(presences[0].game_id.is_none());
    }

    #[tokio::test]
    async fn presence_with_viewer_cookie_is_anonymous_when_the_store_is_empty() {
        let store = temp_store("presence-empty");

        Mock::given(method("POST"))
            .and(path(mock_path("presence", "/v1/presence/users")))
            .and(body_partial_json(serde_json::json!({ "userIds": [7074] })))
            .and(|req: &Request| !req.headers.contains_key("cookie"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "userPresences": [{ "userPresenceType": 0, "userId": 7074 }]
            })))
            .mount(mock_server().await)
            .await;

        let presences = presence_with_viewer_cookie(&store, &[7074], None)
            .await
            .expect("presence");
        assert_eq!(presences[0].user_id, 7074);
    }
}

/// O lote de amigos online: sequencial, com progresso, e com o erro de uma
/// conta contido na entrada dela.
#[cfg(test)]
mod online_friends_batch_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server};
    use std::sync::{Arc, Mutex};
    use wiremock::matchers::{body_string_contains, header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    /// Rota atual dos amigos online, por conta. A antiga
    /// (`my/friends/online`) foi removida pelo Roblox.
    fn route(user_id: i64) -> String {
        format!("/v1/users/{}/friends/online", user_id)
    }

    fn temp_store(tag: &str) -> AccountStore {
        crypto::init();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        AccountStore::new(std::env::temp_dir().join(format!("ram-friends-{tag}-{nanos}.json")))
    }

    fn add_account(store: &AccountStore, user_id: i64, token: &str) {
        store
            .add(crate::data::accounts::Account::new(
                token.to_string(),
                format!("user{user_id}"),
                user_id,
            ))
            .unwrap();
    }

    /// Uma conta responde, a outra devolve 500 e a terceira nem existe na
    /// store: as três aparecem no resultado, na ordem pedida.
    #[tokio::test]
    async fn a_failing_account_does_not_take_down_the_others() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("friends", &route(8001))))
            .and(header("cookie", cookie_of("batch-ok")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "userId": 7901, "userPresenceType": 2, "gameInstanceId": "job-7901" }]
            })))
            .mount(server)
            .await;

        Mock::given(method("POST"))
            .and(path(mock_path("users", "/v1/users")))
            .and(body_string_contains("7901"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "id": 7901, "name": "friendly", "displayName": "Friendly" }]
            })))
            .mount(server)
            .await;

        Mock::given(method("GET"))
            .and(path(mock_path("friends", &route(8002))))
            .and(header("cookie", cookie_of("batch-broken")))
            .respond_with(ResponseTemplate::new(500))
            .mount(server)
            .await;

        let store = temp_store("batch-mixed");
        add_account(&store, 8001, "batch-ok");
        add_account(&store, 8002, "batch-broken");

        let results =
            collect_online_friends(&store, vec![8001, 8002, 8003], Some(0), |_, _| {}).await;

        assert_eq!(results.len(), 3);

        assert_eq!(results[0].user_id, 8001);
        assert!(results[0].error.is_none());
        assert_eq!(results[0].friends.len(), 1);
        assert_eq!(results[0].friends[0].name, "friendly");

        assert_eq!(results[1].user_id, 8002);
        assert!(results[1].friends.is_empty());
        assert_eq!(
            results[1].error.as_deref(),
            Some("Failed to get online friends (status 500)")
        );

        // Conta fora da store: erro contido na entrada, não no comando.
        assert_eq!(results[2].user_id, 8003);
        assert_eq!(results[2].error.as_deref(), Some("Account 8003 not found"));
    }

    /// O progresso sai uma vez no início e uma por conta concluída, sempre com
    /// o mesmo total (ids repetidos são descartados antes).
    #[tokio::test]
    async fn progress_is_emitted_once_per_account_plus_the_initial_zero() {
        let store = temp_store("batch-progress");
        let seen = Arc::new(Mutex::new(Vec::<(usize, usize)>::new()));
        let sink = Arc::clone(&seen);

        let results = collect_online_friends(&store, vec![8101, 8102, 8101], Some(0), move |done, total| {
            sink.lock().unwrap().push((done, total));
        })
        .await;

        assert_eq!(results.len(), 2);
        assert_eq!(*seen.lock().unwrap(), vec![(0, 2), (1, 2), (2, 2)]);
    }

    #[tokio::test]
    async fn an_empty_selection_only_reports_a_zero_total() {
        let store = temp_store("batch-empty");
        let seen = Arc::new(Mutex::new(Vec::<(usize, usize)>::new()));
        let sink = Arc::clone(&seen);

        let results = collect_online_friends(&store, vec![], Some(0), move |done, total| {
            sink.lock().unwrap().push((done, total));
        })
        .await;

        assert!(results.is_empty());
        assert_eq!(*seen.lock().unwrap(), vec![(0, 0)]);
    }
}

#[cfg(test)]
mod server_scan_dedupe_tests {
    use super::*;

    fn server(id: &str, playing: i32) -> api::roblox::ServerData {
        api::roblox::ServerData {
            id: id.to_string(),
            max_players: 13,
            playing,
            player_tokens: Vec::new(),
            fps: 60.0,
            ping: None,
            name: None,
            vip_server_id: None,
            access_code: None,
        }
    }

    /// Dados reais do place do relato tinham 50 Job IDs repetidos em 400
    /// servidores: a lista do Roblox se mexe entre uma página e outra.
    #[test]
    fn a_repeated_job_id_is_not_added_twice() {
        let mut all = vec![server("a", 10), server("b", 9)];
        extend_unique(&mut all, vec![server("b", 8), server("c", 7)]);
        let ids: Vec<&str> = all.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
        // A primeira leitura vence: não adianta trocar por um número que já
        // estará velho no próximo instante.
        assert_eq!(all[1].playing, 9);
    }

    #[test]
    fn a_server_without_an_id_is_dropped() {
        let mut all = Vec::new();
        extend_unique(&mut all, vec![server("", 1), server("   ", 2), server("ok", 3)]);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, "ok");
    }

    /// Sem a deduplicação, a contagem de examinados mentia para o usuário.
    #[test]
    fn the_scanned_count_only_counts_distinct_servers() {
        let mut all = Vec::new();
        extend_unique(&mut all, vec![server("a", 1), server("b", 2)]);
        extend_unique(&mut all, vec![server("a", 1), server("b", 2)]);
        assert_eq!(all.len(), 2);
    }
}

/// O refresh de sessão chama `signoutfromallsessionsandreauthenticate`: ele
/// desloga a conta de **todas** as sessões e pode derrubar clientes Roblox
/// abertos. Por isso o CLAUDE.md proíbe `run_with_session_retry` em leitura não
/// crítica — perder o saldo de Robux na tela não justifica derrubar o jogo de
/// ninguém. Este teste é estrutural de propósito: ele lê o próprio arquivo, para
/// que um comando de leitura novo não entre com retry sem alguém notar.
#[cfg(test)]
mod read_only_retry_tests {
    const FONTE: &str = include_str!("account_api.rs");

    /// Comandos que são leitura: pegam o cookie, leem algo e devolvem. Nenhum
    /// deles pode renovar sessão por causa de um 401.
    const LEITURAS: &[&str] = &[
        "get_auth_ticket",
        "get_csrf_token",
        "check_pin",
        "get_robux",
        "get_blocked_users",
        "get_private_server_invite_privacy",
        "resolve_join_link",
    ];

    /// Corpo da função: da assinatura até o fecha-chaves de coluna zero.
    ///
    /// Cortar na próxima `async fn` engolia o comentário da função seguinte — e
    /// um comentário que **explica** por que ali não se usa `run_with_session_retry`
    /// fazia o teste acusar quem estava certo.
    fn corpo_da_funcao(nome: &str) -> &'static str {
        corpo_em(FONTE, nome)
    }

    fn corpo_em<'a>(fonte: &'a str, nome: &str) -> &'a str {
        let assinatura = format!("async fn {nome}(");
        let inicio = fonte
            .find(&assinatura)
            .unwrap_or_else(|| panic!("função {nome} não existe mais neste arquivo"));
        let resto = &fonte[inicio + assinatura.len()..];
        match fim_da_funcao(resto) {
            Some(fim) => &resto[..fim],
            None => resto,
        }
    }

    /// Posição do `}` de coluna zero que fecha a função — seguido de `\n` ou de
    /// `\r\n`: a CI (runner Windows) faz checkout com CRLF, e `include_str!`
    /// entrega o arquivo como está no disco.
    fn fim_da_funcao(resto: &str) -> Option<usize> {
        resto
            .match_indices("\n}")
            .map(|(i, _)| i)
            .find(|&i| matches!(resto.as_bytes().get(i + 2), Some(b'\n' | b'\r')))
    }

    /// A CI roda num runner Windows, que faz checkout com CRLF: o corte do corpo
    /// tem que achar o `}` de coluna zero com os dois fins de linha. Sem isso o
    /// "corpo" virava o resto do arquivo e todo comando parecia usar o retry
    /// (o PR #1 falhou assim, passando aqui).
    #[test]
    fn the_body_cut_does_not_depend_on_line_endings() {
        let lf = "async fn a(x: u8) {\n    ler(x);\n}\n\nasync fn b() {\n    run_with_session_retry();\n}\n";
        let crlf = lf.replace('\n', "\r\n");
        for fonte in [lf, crlf.as_str()] {
            let corpo = corpo_em(fonte, "a");
            assert!(corpo.contains("ler(x)"), "{corpo:?}");
            assert!(
                !corpo.contains("run_with_session_retry"),
                "o corte passou do fim da função: {corpo:?}"
            );
        }
    }

    #[test]
    fn read_only_commands_never_refresh_the_session() {
        let culpados: Vec<&str> = LEITURAS
            .iter()
            .copied()
            .filter(|nome| corpo_da_funcao(nome).contains("run_with_session_retry"))
            .collect();
        assert!(
            culpados.is_empty(),
            "estes comandos são leitura e não podem renovar sessão: {culpados:?}"
        );
    }

    /// Guarda contra o teste se tornar vazio por engano (nome mudou, arquivo
    /// dividido): se a varredura não enxerga mais o retry em lugar nenhum, é
    /// porque ela parou de ler o arquivo de verdade.
    #[test]
    fn the_scan_still_sees_the_retry_helper_in_the_file() {
        assert!(FONTE.contains("fn run_with_session_retry"));
        assert!(FONTE.matches("run_with_session_retry(").count() > 5);
    }
}
