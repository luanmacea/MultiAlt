/// Emit one structured line to the launch console (frontend listens on the
/// "launch-log" event). `level` is one of "info" | "success" | "warn" | "error";
/// `step` is a short machine code (start, isolation, auth, target, pid, spawn,
/// wait, done) the UI can use for coloring/icons.
pub(crate) fn emit_launch_log(
    app: &tauri::AppHandle,
    user_id: i64,
    level: &str,
    step: &str,
    message: impl Into<String>,
) {
    let _ = app.emit(
        "launch-log",
        serde_json::json!({
            "userId": user_id,
            "level": level,
            "step": step,
            "message": message.into(),
        }),
    );
}

/// Linha de console que nao pertence a uma conta: inicio/fim de uma sessao de
/// Botting, por exemplo. O frontend ja aceita `userId: null` e desenha "—";
/// passar `0` faria o console imprimir "0" no lugar do nome.
pub(crate) fn emit_session_log(
    app: &tauri::AppHandle,
    level: &str,
    step: &str,
    message: impl Into<String>,
) {
    let _ = app.emit(
        "launch-log",
        serde_json::json!({
            "userId": serde_json::Value::Null,
            "level": level,
            "step": step,
            "message": message.into(),
        }),
    );
}

/// Group name used to bucket accounts that failed to launch because Roblox
/// reports them as moderated/banned.
pub(crate) const MODERATED_GROUP: &str = "moderadas";

/// Returns true if an auth-ticket error string indicates the account is
/// moderated/banned (Roblox returns 403 with `"User is moderated"`).
pub(crate) fn is_moderated_error(err: &str) -> bool {
    let e = err.to_lowercase();
    e.contains("moderated") || e.contains("is banned") || e.contains("account has been")
}

/// Move an account into the "moderadas" group and persist it, then notify the
/// frontend so it can refresh and surface a toast. No-op if already grouped.
pub(crate) fn mark_account_moderated(store: &AccountStore, app: &tauri::AppHandle, user_id: i64) {
    if let Ok(accounts) = store.get_all() {
        if let Some(mut account) = accounts.into_iter().find(|a| a.user_id == user_id) {
            if account.group != MODERATED_GROUP {
                account.group = MODERATED_GROUP.to_string();
                let _ = store.update(account);
                let _ = app.emit(
                    "account-moderated",
                    serde_json::json!({ "userId": user_id, "group": MODERATED_GROUP }),
                );
            }
        }
    }
}

#[derive(Clone, Default)]
struct WindowsClientOverrides {
    max_fps: Option<u32>,
    master_volume: Option<f32>,
    graphics_level: Option<u32>,
    window_size: Option<(u32, u32)>,
    fast_flags: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum LaunchClientProfile {
    Normal,
    BottingPlayer,
    BottingBot,
}

pub(crate) fn profile_key(
    profile: LaunchClientProfile,
    normal: &'static str,
    player: &'static str,
    bot: &'static str,
) -> &'static str {
    match profile {
        LaunchClientProfile::Normal => normal,
        LaunchClientProfile::BottingPlayer => player,
        LaunchClientProfile::BottingBot => bot,
    }
}

pub(crate) fn effective_launch_profile(
    settings: &SettingsStore,
    profile: LaunchClientProfile,
) -> LaunchClientProfile {
    if matches!(profile, LaunchClientProfile::Normal) || !botting_uses_shared_client_profile(settings)
    {
        profile
    } else {
        LaunchClientProfile::Normal
    }
}

fn custom_client_settings_path(settings: &SettingsStore, profile: LaunchClientProfile) -> String {
    let key = profile_key(
        profile,
        "CustomClientSettings",
        "BottingPlayerCustomClientSettings",
        "BottingBotCustomClientSettings",
    );
    settings.get_string("General", key)
}

pub(crate) fn start_minimized_for_profile(
    settings: &SettingsStore,
    profile: LaunchClientProfile,
) -> bool {
    let key = profile_key(
        profile,
        "StartRobloxMinimized",
        "BottingPlayerStartRobloxMinimized",
        "BottingBotStartRobloxMinimized",
    );
    settings.get_bool("General", key)
}

pub(crate) fn botting_uses_shared_client_profile(settings: &SettingsStore) -> bool {
    settings
        .get("General", "BottingUseSharedClientProfile")
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(true)
}

#[cfg(target_os = "windows")]
fn windows_client_overrides(
    settings: &SettingsStore,
    allow_fps_override: bool,
    profile: LaunchClientProfile,
) -> WindowsClientOverrides {
    let unlock_fps_key = profile_key(
        profile,
        "UnlockFPS",
        "BottingPlayerUnlockFPS",
        "BottingBotUnlockFPS",
    );
    let max_fps_key = profile_key(
        profile,
        "MaxFPSValue",
        "BottingPlayerMaxFPSValue",
        "BottingBotMaxFPSValue",
    );

    let max_fps = if allow_fps_override && settings.get_bool("General", unlock_fps_key) {
        let fps = settings.get_int("General", max_fps_key).unwrap_or(120);
        if fps > 0 {
            Some(fps as u32)
        } else {
            None
        }
    } else {
        None
    };

    let override_volume_key = profile_key(
        profile,
        "OverrideClientVolume",
        "BottingPlayerOverrideClientVolume",
        "BottingBotOverrideClientVolume",
    );
    let client_volume_key = profile_key(
        profile,
        "ClientVolume",
        "BottingPlayerClientVolume",
        "BottingBotClientVolume",
    );

    let master_volume = if settings.get_bool("General", override_volume_key) {
        Some(
            settings
                .get_float("General", client_volume_key)
                .unwrap_or(0.5)
                .clamp(0.0, 1.0) as f32,
        )
    } else {
        None
    };

    let override_graphics_key = profile_key(
        profile,
        "OverrideClientGraphics",
        "BottingPlayerOverrideClientGraphics",
        "BottingBotOverrideClientGraphics",
    );
    let graphics_level_key = profile_key(
        profile,
        "ClientGraphicsLevel",
        "BottingPlayerClientGraphicsLevel",
        "BottingBotClientGraphicsLevel",
    );

    let graphics_level = if settings.get_bool("General", override_graphics_key) {
        let lvl = settings
            .get_int("General", graphics_level_key)
            .unwrap_or(10);
        if lvl > 0 {
            Some(lvl.clamp(1, 10) as u32)
        } else {
            None
        }
    } else {
        None
    };

    let override_window_key = profile_key(
        profile,
        "OverrideClientWindowSize",
        "BottingPlayerOverrideClientWindowSize",
        "BottingBotOverrideClientWindowSize",
    );
    let window_width_key = profile_key(
        profile,
        "ClientWindowWidth",
        "BottingPlayerClientWindowWidth",
        "BottingBotClientWindowWidth",
    );
    let window_height_key = profile_key(
        profile,
        "ClientWindowHeight",
        "BottingPlayerClientWindowHeight",
        "BottingBotClientWindowHeight",
    );

    let window_size = if settings.get_bool("General", override_window_key) {
        let w = settings
            .get_int("General", window_width_key)
            .unwrap_or(1280);
        let h = settings
            .get_int("General", window_height_key)
            .unwrap_or(720);
        if w > 0 && h > 0 {
            Some((w as u32, h as u32))
        } else {
            None
        }
    } else {
        None
    };

    let optimization_profile = platform::windows::load_optimization_profile(settings, profile);
    let fast_flags = if optimization_profile.experimental.enable_fast_flags {
        match platform::windows::parse_allowlisted_fast_flags_json(
            &optimization_profile.experimental.fast_flags_json,
        ) {
            Ok(flags) => Some(flags),
            Err(err) => {
                eprintln!("Skipped allowlisted fast flags for {:?}: {}", profile, err);
                None
            }
        }
    } else {
        None
    };

    WindowsClientOverrides {
        max_fps,
        master_volume,
        graphics_level,
        window_size,
        fast_flags,
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn patch_client_settings_for_launch(
    settings: &SettingsStore,
    profile: LaunchClientProfile,
) {
    use platform::windows;

    let effective_profile = effective_launch_profile(settings, profile);
    let custom_settings = custom_client_settings_path(settings, effective_profile);
    let custom_settings = custom_settings.trim();
    let mut custom_applied = false;

    // Legacy behavior: custom settings file overrides FPS unlock when valid.
    if !custom_settings.is_empty()
        && std::path::Path::new(custom_settings).exists()
        && windows::copy_custom_client_settings(custom_settings).is_ok()
    {
        custom_applied = true;
    }

    let mut overrides = windows_client_overrides(settings, !custom_applied, effective_profile);
    if custom_applied {
        overrides.fast_flags = None;
    }
    let _ = windows::apply_runtime_client_settings(
        overrides.max_fps,
        overrides.master_volume,
        overrides.graphics_level,
        overrides.window_size,
        overrides.fast_flags.as_ref(),
    );
}

#[cfg(target_os = "macos")]
fn fps_unlock_target(settings: &SettingsStore, profile: LaunchClientProfile) -> Option<u32> {
    let unlock_fps_key = profile_key(
        profile,
        "UnlockFPS",
        "BottingPlayerUnlockFPS",
        "BottingBotUnlockFPS",
    );
    let max_fps_key = profile_key(
        profile,
        "MaxFPSValue",
        "BottingPlayerMaxFPSValue",
        "BottingBotMaxFPSValue",
    );

    if !settings.get_bool("General", unlock_fps_key) {
        return None;
    }
    settings
        .get_int("General", max_fps_key)
        .filter(|fps| *fps > 0)
        .map(|fps| fps as u32)
}

#[cfg(target_os = "macos")]
fn patch_client_settings_for_launch(settings: &SettingsStore, profile: LaunchClientProfile) {
    use platform::macos;

    let custom_settings = custom_client_settings_path(settings, profile);
    let custom_settings = custom_settings.trim();

    // Keep the same override precedence as Windows.
    if !custom_settings.is_empty()
        && std::path::Path::new(custom_settings).exists()
        && macos::copy_custom_client_settings(custom_settings).is_ok()
    {
        return;
    }

    if let Some(fps) = fps_unlock_target(settings, profile) {
        let _ = macos::apply_fps_unlock(fps);
    }
}

fn save_browser_tracker_id(
    state: &AccountStore,
    user_id: i64,
    browser_tracker_id: &str,
) -> Result<(), String> {
    let accounts = state.get_all()?;
    if let Some(mut account) = accounts.into_iter().find(|a| a.user_id == user_id) {
        account.browser_tracker_id = browser_tracker_id.to_string();
        state.update(account)?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub(crate) fn get_or_create_browser_tracker_id(
    state: &AccountStore,
    user_id: i64,
) -> Result<String, String> {
    let accounts = state.get_all()?;
    if let Some(existing) = accounts
        .iter()
        .find(|a| a.user_id == user_id)
        .map(|a| a.browser_tracker_id.trim().to_string())
        .filter(|id| !id.is_empty())
    {
        return Ok(existing);
    }

    let generated = platform::windows::generate_browser_tracker_id();
    save_browser_tracker_id(state, user_id, &generated)?;
    Ok(generated)
}

#[cfg(target_os = "macos")]
fn get_or_create_browser_tracker_id(state: &AccountStore, user_id: i64) -> Result<String, String> {
    let accounts = state.get_all()?;
    if let Some(existing) = accounts
        .iter()
        .find(|a| a.user_id == user_id)
        .map(|a| a.browser_tracker_id.trim().to_string())
        .filter(|id| !id.is_empty())
    {
        return Ok(existing);
    }

    let generated = platform::macos::generate_browser_tracker_id();
    save_browser_tracker_id(state, user_id, &generated)?;
    Ok(generated)
}

#[cfg(target_os = "windows")]
pub(crate) async fn wait_for_new_roblox_pid(
    pids_before: &[u32],
    timeout: std::time::Duration,
) -> Option<u32> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let pids_after = platform::windows::get_roblox_pids();
        if let Some(pid) = pids_after
            .iter()
            .find(|p| !pids_before.contains(p))
            .copied()
        {
            return Some(pid);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }
}

#[cfg(target_os = "macos")]
async fn wait_for_new_roblox_pid(pids_before: &[u32], timeout: std::time::Duration) -> Option<u32> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let pids_after = platform::macos::get_roblox_pids();
        if let Some(pid) = pids_after
            .iter()
            .find(|p| !pids_before.contains(p))
            .copied()
        {
            return Some(pid);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }
}

#[derive(Debug, Clone, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct BottingAccountStatusPayload {
    user_id: i64,
    is_player: bool,
    disconnected: bool,
    phase: String,
    retry_count: u32,
    next_restart_at_ms: Option<i64>,
    player_grace_until_ms: Option<i64>,
    last_error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct BottingStatusPayload {
    active: bool,
    started_at_ms: Option<i64>,
    place_id: i64,
    job_id: String,
    launch_data: String,
    interval_minutes: i64,
    launch_delay_seconds: i64,
    player_grace_minutes: i64,
    player_user_ids: Vec<i64>,
    user_ids: Vec<i64>,
    accounts: Vec<BottingAccountStatusPayload>,
}

#[cfg(target_os = "windows")]
async fn minimize_new_roblox_windows(pids_before: Vec<u32>, timeout: std::time::Duration) {
    let deadline = std::time::Instant::now() + timeout;
    let mut minimized: HashSet<u32> = HashSet::new();
    loop {
        for pid in platform::windows::get_roblox_pids() {
            if pids_before.contains(&pid) || minimized.contains(&pid) {
                continue;
            }
            if let Some(hwnd) = platform::windows::find_main_window(pid) {
                let _ = platform::windows::minimize_window(hwnd);
                minimized.insert(pid);
            }
        }

        if std::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
}

#[cfg(target_os = "windows")]
pub(crate) async fn apply_windows_post_launch_profile(
    app: Option<&tauri::AppHandle>,
    settings: &SettingsStore,
    profile: LaunchClientProfile,
    pid: u32,
) {
    let effective_profile = effective_launch_profile(settings, profile);
    let optimization_profile = platform::windows::load_optimization_profile(settings, effective_profile);
    let has_process_policy = optimization_profile.process.enabled;
    let has_job_limits = optimization_profile.experimental.enable_job_cpu_limit
        || optimization_profile.experimental.enable_job_memory_limit;
    if !has_process_policy && !has_job_limits {
        return;
    }
    if has_process_policy && optimization_profile.process.delay_ms > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(
            optimization_profile.process.delay_ms,
        ))
        .await;
    }

    if let Err(err) = platform::windows::apply_optimization_to_pid(pid, &optimization_profile) {
        eprintln!("Failed to apply Windows optimization to pid {}: {}", pid, err);
        if let Some(app) = app {
            let _ = app.emit(
                "roblox-optimization-warning",
                serde_json::json!({
                    "pid": pid,
                    "message": err,
                }),
            );
        }
    }
}

#[cfg(target_os = "windows")]
async fn ensure_multi_roblox_enabled(auto_close_conflicts: bool) -> Result<(), String> {
    let enabled = platform::windows::enable_multi_roblox()?;
    if enabled {
        return Ok(());
    }

    let roblox_pids = platform::windows::get_roblox_pids();
    let legacy_pids = platform::windows::find_legacy_ram_pids();

    if !roblox_pids.is_empty() {
        if auto_close_conflicts {
            let killed = platform::windows::kill_all_roblox();
            if killed > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(700)).await;
            }
            let _ = platform::windows::tracker().cleanup_dead_processes();
            let enabled_after = platform::windows::enable_multi_roblox()?;
            if enabled_after {
                return Ok(());
            }
        }
        return Err(
            "A Roblox client is already running. Close it or enable Auto-close Roblox for Multi-Roblox in settings.".into(),
        );
    }

    if !legacy_pids.is_empty() {
        return Err(
            "The legacy Roblox Account Manager is running and holds the Roblox singleton mutex. Close it before launching from this app.".into(),
        );
    }

    platform::windows::release_multi_roblox_handle();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let enabled_retry = platform::windows::enable_multi_roblox()?;
    if enabled_retry {
        return Ok(());
    }

    Err(
        "Could not acquire the Roblox singleton mutex. Another program may be holding it. Close any Roblox-related tools and try again.".into(),
    )
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone)]
struct BottingConfig {
    user_ids: Vec<i64>,
    place_id: i64,
    job_id: String,
    launch_data: String,
    player_user_ids: HashSet<i64>,
    interval_minutes: u64,
    launch_delay_seconds: u64,
    retry_max: u32,
    retry_base_seconds: u64,
    player_grace_minutes: u64,
    /// A sessao foi aberta sobre contas que **ja estavam em jogo**: na primeira
    /// passagem elas nao sao fechadas nem relancadas, so entram no ciclo.
    adopt_running: bool,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone)]
struct BottingAccountRuntime {
    user_id: i64,
    is_player: bool,
    disconnected: bool,
    manual_restart_pending: bool,
    manual_restart_keep_schedule: bool,
    manual_restart_saved_next_restart_at_ms: Option<i64>,
    phase: &'static str,
    retry_count: u32,
    next_restart_at_ms: Option<i64>,
    player_grace_until_ms: Option<i64>,
    last_error: Option<String>,
}

#[cfg(target_os = "windows")]
#[derive(Clone)]
struct BottingSession {
    id: u64,
    stop_flag: Arc<AtomicBool>,
    stopped_notify: Arc<tokio::sync::Notify>,
    started_at_ms: i64,
    config: Arc<Mutex<BottingConfig>>,
    accounts: Arc<Mutex<HashMap<i64, BottingAccountRuntime>>>,
}

#[cfg(target_os = "windows")]
struct BottingManager {
    session: Mutex<Option<BottingSession>>,
    next_id: AtomicU64,
}

#[cfg(target_os = "windows")]
impl BottingManager {
    fn new() -> Self {
        Self {
            session: Mutex::new(None),
            next_id: AtomicU64::new(1),
        }
    }

    fn next_session_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    fn get_session(&self) -> Option<BottingSession> {
        self.session.lock().ok().and_then(|s| s.as_ref().cloned())
    }

    fn replace_session(&self, session: Option<BottingSession>) {
        if let Ok(mut guard) = self.session.lock() {
            *guard = session;
        }
    }
}

#[cfg(target_os = "windows")]
static BOTTING_MANAGER: LazyLock<BottingManager> = LazyLock::new(BottingManager::new);

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[derive(Debug, Clone, Default)]
struct ResolvedLaunchJob {
    job_id: String,
    join_vip: bool,
    link_code: String,
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

fn strip_ascii_prefix<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    let head = value.get(..prefix.len())?;
    if !head.eq_ignore_ascii_case(prefix) {
        return None;
    }
    value.get(prefix.len()..)
}

fn looks_like_share_link(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("/share?")
        || lower.contains("/share-links")
        || lower.contains("navigation/share_links")
        || lower.contains("type=server")
        || lower.contains("pid=server")
}

fn extract_private_server_link_code(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(rest) = strip_ascii_prefix(trimmed, "vip:") {
        let decoded = decode_url_component(rest.trim());
        let code = decoded.trim();
        if !code.is_empty() {
            return Some(code.to_string());
        }
    }

    if let Some(code) = extract_query_param_value_recursive(trimmed, "privateServerLinkCode") {
        return Some(code);
    }
    if let Some(code) = extract_query_param_value_recursive(trimmed, "linkCode") {
        return Some(code);
    }

    let starts_with_code = trimmed
        .get(..5)
        .map(|head| head.eq_ignore_ascii_case("code="))
        .unwrap_or(false);
    if looks_like_share_link(trimmed) || starts_with_code {
        if let Some(code) = extract_query_param_value_recursive(trimmed, "code") {
            return Some(code);
        }
    }

    None
}

fn resolve_launch_job(
    raw_job_id: &str,
    explicit_join_vip: bool,
    explicit_link_code: &str,
) -> ResolvedLaunchJob {
    let trimmed_job = raw_job_id.trim();
    let mut job_id = trimmed_job.to_string();

    let mut link_code = extract_private_server_link_code(explicit_link_code).unwrap_or_else(|| {
        decode_url_component(explicit_link_code.trim())
            .trim()
            .to_string()
    });

    let mut join_vip = explicit_join_vip;
    if let Some(rest) = strip_ascii_prefix(trimmed_job, "vip:") {
        join_vip = true;
        job_id = rest.trim().to_string();
    }

    if link_code.is_empty() {
        if let Some(code) = extract_private_server_link_code(trimmed_job) {
            link_code = code;
        }
    }

    if join_vip && link_code.is_empty() {
        if job_id.trim().is_empty() {
            join_vip = false;
        } else {
            link_code = decode_url_component(job_id.trim()).trim().to_string();
        }
    }

    ResolvedLaunchJob {
        job_id,
        join_vip,
        link_code,
    }
}

fn looks_like_access_code(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }

    let parts: Vec<&str> = trimmed.split('-').collect();
    if parts.len() != 5 {
        return false;
    }

    parts
        .iter()
        .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

fn looks_like_share_link_code(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.len() == 32
        && trimmed.chars().any(|c| c.is_ascii_alphabetic())
        && trimmed.chars().all(|c| c.is_ascii_hexdigit())
}

fn extract_place_id_from_url(value: &str) -> Option<i64> {
    let lower = value.to_ascii_lowercase();
    let marker = "/games/";
    let start = lower.find(marker)? + marker.len();
    let rest = value.get(start..)?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<i64>().ok().filter(|id| *id > 0)
}

/// Picks a random public server for **this** account.
///
/// Cada conta chama este helper por conta própria: num launch múltiplo com
/// `shuffleJob` ligado, as contas acabam espalhadas por servidores diferentes
/// (a lista é buscada de novo e o índice é sorteado por conta), em vez de todas
/// caírem no mesmo servidor. Devolve `None` — e o chamador mantém o Job ID
/// vazio, entrando num servidor público qualquer — quando a listagem falha ou
/// vem vazia.
async fn pick_shuffled_public_job(
    accounts: &AccountStore,
    user_id: i64,
    place_id: i64,
) -> Option<String> {
    // Sem `run_with_session_retry` de propósito: o refresh dele chama
    // `signoutfromallsessionsandreauthenticate`, que derruba as sessões abertas
    // da conta. Sortear servidor é leitura opcional — se falhar, o launch segue
    // com o Job vazio (servidor público qualquer).
    let cookie = get_cookie(accounts, user_id).ok()?;
    let response = api::roblox::get_servers(place_id, "Public", None, Some(&cookie))
        .await
        .ok()?;

    if response.data.is_empty() {
        return None;
    }

    let index = shuffle_server_index(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        response.data.len(),
    );
    Some(response.data[index].id.clone())
}

#[derive(Debug, Clone)]
struct ResolvedPrivateJoin {
    place_id: i64,
    link_code: String,
    access_code: String,
    use_private_join: bool,
}

async fn resolve_private_join(
    cookie: &str,
    place_id: i64,
    launch: &ResolvedLaunchJob,
) -> Result<ResolvedPrivateJoin, String> {
    let mut resolved_place_id = place_id;
    let mut resolved_link_code = launch.link_code.trim().to_string();

    if !resolved_link_code.is_empty() {
        if let Some(url_place_id) = extract_place_id_from_url(&launch.job_id) {
            resolved_place_id = url_place_id;
        }
    }

    if !resolved_link_code.is_empty()
        && (looks_like_share_link(&launch.job_id)
            || looks_like_share_link_code(&resolved_link_code))
    {
        let (maybe_place_id, resolved_code) =
            api::roblox::resolve_share_server_link(cookie, &resolved_link_code).await?;
        if let Some(pid) = maybe_place_id {
            resolved_place_id = pid;
        }
        if !resolved_code.trim().is_empty() {
            resolved_link_code = resolved_code.trim().to_string();
        }
    }

    let mut access_code = String::new();
    if looks_like_access_code(&resolved_link_code) {
        access_code = resolved_link_code.clone();
        resolved_link_code.clear();
    }

    let use_private_join =
        launch.join_vip || !resolved_link_code.is_empty() || !access_code.is_empty();

    Ok(ResolvedPrivateJoin {
        place_id: resolved_place_id,
        link_code: resolved_link_code,
        access_code,
        use_private_join,
    })
}

#[cfg(target_os = "windows")]
fn backoff_delay_seconds(base: u64, retry_count: u32, retry_max: u32) -> u64 {
    let exp = retry_count.saturating_sub(1).min(retry_max.max(1));
    let scaled = base.saturating_mul(1_u64 << exp.min(12));
    scaled.clamp(5, 300)
}

#[cfg(target_os = "windows")]
async fn wait_for_launch_slot(
    last_launch_at: &mut Option<std::time::Instant>,
    launch_delay_seconds: u64,
) {
    if let Some(last) = *last_launch_at {
        let required_gap = std::time::Duration::from_secs(launch_delay_seconds);
        let elapsed = last.elapsed();
        if elapsed < required_gap {
            tokio::time::sleep(required_gap - elapsed).await;
        }
    }
    *last_launch_at = Some(std::time::Instant::now());
}

#[cfg(target_os = "windows")]
fn is_429_related_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("429")
        || lower.contains("too many requests")
        || lower.contains("authentifizierung fehlgeschlagen")
        || lower.contains("authentication failed")
}

#[cfg(target_os = "windows")]
fn title_looks_auth_failure(title: &str) -> bool {
    let t = title.to_lowercase();
    t.contains("authentifizierung fehlgeschlagen")
        || t.contains("authentication failed")
        || t.contains("fehlercode: 429")
        || t.contains("error code: 429")
}

#[cfg(target_os = "windows")]
async fn detect_auth_failure_window(pid: u32) -> bool {
    for _ in 0..20 {
        if let Some(hwnd) = platform::windows::find_main_window(pid) {
            let title = platform::windows::get_window_title(hwnd);
            if !title.is_empty() && title_looks_auth_failure(&title) {
                return true;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }
    false
}

#[cfg(target_os = "windows")]
fn botting_status_from_session(session: &BottingSession) -> BottingStatusPayload {
    let config = match session.config.lock() {
        Ok(c) => c.clone(),
        Err(_) => {
            return BottingStatusPayload {
                active: false,
                ..BottingStatusPayload::default()
            }
        }
    };
    let mut accounts: Vec<BottingAccountStatusPayload> = match session.accounts.lock() {
        Ok(map) => map
            .values()
            .map(|a| BottingAccountStatusPayload {
                user_id: a.user_id,
                is_player: a.is_player,
                disconnected: a.disconnected,
                phase: a.phase.to_string(),
                retry_count: a.retry_count,
                next_restart_at_ms: a.next_restart_at_ms,
                player_grace_until_ms: a.player_grace_until_ms,
                last_error: a.last_error.clone(),
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    accounts.sort_by_key(|a| a.user_id);
    let mut player_user_ids: Vec<i64> = config.player_user_ids.iter().copied().collect();
    player_user_ids.sort();
    BottingStatusPayload {
        active: !session.stop_flag.load(Ordering::Relaxed),
        started_at_ms: Some(session.started_at_ms),
        place_id: config.place_id,
        job_id: config.job_id,
        launch_data: config.launch_data,
        interval_minutes: config.interval_minutes as i64,
        launch_delay_seconds: config.launch_delay_seconds as i64,
        player_grace_minutes: config.player_grace_minutes as i64,
        player_user_ids,
        user_ids: config.user_ids,
        accounts,
    }
}

#[cfg(target_os = "windows")]
fn current_botting_status() -> BottingStatusPayload {
    if let Some(session) = BOTTING_MANAGER.get_session() {
        botting_status_from_session(&session)
    } else {
        BottingStatusPayload::default()
    }
}

#[cfg(target_os = "windows")]
fn emit_botting_status(app: &tauri::AppHandle) {
    let _ = app.emit("botting-status", current_botting_status());
}

#[cfg(test)]
mod launch_resolve_tests {
    use super::*;

    // ---- resolve_launch_job -------------------------------------------------

    #[test]
    fn vip_prefix_sets_join_vip_and_strips_prefix() {
        let resolved = resolve_launch_job("vip:ABC123", false, "");
        assert!(resolved.join_vip);
        assert_eq!(resolved.job_id, "ABC123");
        assert_eq!(resolved.link_code, "ABC123");
    }

    #[test]
    fn vip_prefix_is_case_insensitive_and_trims() {
        let resolved = resolve_launch_job("  VIP: ABC123  ", false, "");
        assert!(resolved.join_vip);
        assert_eq!(resolved.job_id, "ABC123");
    }

    #[test]
    fn share_url_private_server_link_code_is_extracted() {
        let resolved = resolve_launch_job(
            "https://www.roblox.com/games/606849621/Jailbreak?privateServerLinkCode=1122334455",
            false,
            "",
        );
        assert_eq!(resolved.link_code, "1122334455");
        assert!(!resolved.join_vip);
    }

    #[test]
    fn link_code_query_param_is_extracted_from_job_id() {
        let resolved = resolve_launch_job(
            "https://www.roblox.com/games/start?placeId=1&linkCode=abcdef",
            false,
            "",
        );
        assert_eq!(resolved.link_code, "abcdef");
    }

    #[test]
    fn join_vip_without_code_falls_back_to_job_id_as_link_code() {
        let resolved = resolve_launch_job("SOMECODE", true, "");
        assert!(resolved.join_vip);
        assert_eq!(resolved.job_id, "SOMECODE");
        assert_eq!(resolved.link_code, "SOMECODE");
    }

    #[test]
    fn vip_prefix_with_empty_job_clears_join_vip() {
        let resolved = resolve_launch_job("vip:", false, "");
        assert!(!resolved.join_vip);
        assert_eq!(resolved.job_id, "");
        assert_eq!(resolved.link_code, "");
    }

    #[test]
    fn explicit_link_code_wins_over_job_id_extraction() {
        let resolved = resolve_launch_job(
            "https://www.roblox.com/games/1/x?privateServerLinkCode=fromjob",
            true,
            "vip:explicit",
        );
        assert!(resolved.join_vip);
        assert_eq!(resolved.link_code, "explicit");
    }

    #[test]
    fn plain_job_id_without_vip_stays_untouched() {
        let resolved = resolve_launch_job(
            "  11111111-2222-3333-4444-555555555555  ",
            false,
            "",
        );
        assert!(!resolved.join_vip);
        assert_eq!(resolved.job_id, "11111111-2222-3333-4444-555555555555");
        assert_eq!(resolved.link_code, "");
    }

    // ---- extract_query_param_value -----------------------------------------

    #[test]
    fn extract_query_param_value_reads_simple_pair() {
        assert_eq!(
            extract_query_param_value("https://x/y?code=abc&other=1", "code").as_deref(),
            Some("abc")
        );
    }

    #[test]
    fn extract_query_param_value_is_case_insensitive_on_the_key() {
        assert_eq!(
            extract_query_param_value("?LINKCODE=abc", "linkCode").as_deref(),
            Some("abc")
        );
    }

    #[test]
    fn extract_query_param_value_ignores_null_and_undefined_and_empty() {
        assert_eq!(extract_query_param_value("?code=null", "code"), None);
        assert_eq!(extract_query_param_value("?code=UNDEFINED", "code"), None);
        assert_eq!(extract_query_param_value("?code=", "code"), None);
        // A later, valid occurrence still wins over the null one.
        assert_eq!(
            extract_query_param_value("?code=null&code=real", "code").as_deref(),
            Some("real")
        );
    }

    #[test]
    fn extract_query_param_value_strips_fragment() {
        assert_eq!(
            extract_query_param_value("?code=abc#frag", "code").as_deref(),
            Some("abc")
        );
    }

    #[test]
    fn extract_query_param_value_recursive_handles_double_encoded_urls() {
        let raw = "https://ro.blox.com/Ebh5?af_dp=roblox%3A%2F%2Fnavigation%2Fshare_links%3Fcode%3DDEADBEEF%26type%3DServer";
        // The non-recursive variant cannot see through the encoded inner query.
        assert_eq!(extract_query_param_value(raw, "code"), None);
        assert_eq!(
            extract_query_param_value_recursive(raw, "code").as_deref(),
            Some("DEADBEEF")
        );
    }

    // ---- looks_like_access_code --------------------------------------------

    #[test]
    fn looks_like_access_code_requires_five_non_empty_segments() {
        assert!(looks_like_access_code(
            "11111111-2222-3333-4444-555555555555"
        ));
        assert!(looks_like_access_code("a-b-c-d-e"));
        assert!(!looks_like_access_code("a-b-c-d"));
        assert!(!looks_like_access_code("a-b-c-d-e-f"));
        assert!(!looks_like_access_code("a--c-d-e"));
        assert!(!looks_like_access_code(""));
        assert!(!looks_like_access_code("   "));
        assert!(!looks_like_access_code("a-b-c-d-e!"));
    }

    // ---- looks_like_share_link_code ----------------------------------------

    #[test]
    fn looks_like_share_link_code_requires_32_hex_with_a_letter() {
        assert!(looks_like_share_link_code(
            "0123456789abcdef0123456789abcdef"
        ));
        // 32 hex digits but no letter -> not a share link code.
        assert!(!looks_like_share_link_code(
            "01234567890123456789012345678901"
        ));
        // Wrong length.
        assert!(!looks_like_share_link_code("0123456789abcdef0123456789abcde"));
        // Non hex character.
        assert!(!looks_like_share_link_code(
            "0123456789abcdeg0123456789abcdef"
        ));
        assert!(!looks_like_share_link_code(""));
    }

    // ---- extract_place_id_from_url -----------------------------------------

    #[test]
    fn extract_place_id_from_url_reads_the_games_segment() {
        assert_eq!(
            extract_place_id_from_url("https://www.roblox.com/games/606849621/Jailbreak"),
            Some(606849621)
        );
        assert_eq!(
            extract_place_id_from_url("https://www.roblox.com/GAMES/42?x=1"),
            Some(42)
        );
    }

    #[test]
    fn extract_place_id_from_url_rejects_zero_and_non_digits() {
        assert_eq!(
            extract_place_id_from_url("https://www.roblox.com/games/0/Zero"),
            None
        );
        assert_eq!(
            extract_place_id_from_url("https://www.roblox.com/games/abc"),
            None
        );
        assert_eq!(extract_place_id_from_url("https://www.roblox.com/home"), None);
    }

    // ---- is_moderated_error -------------------------------------------------

    #[test]
    fn is_moderated_error_matches_known_phrases_case_insensitively() {
        assert!(is_moderated_error("User is moderated"));
        assert!(is_moderated_error("USER IS MODERATED"));
        assert!(is_moderated_error("The account is banned"));
        assert!(is_moderated_error("This account has been terminated"));
    }

    #[test]
    fn is_moderated_error_returns_false_for_unrelated_failures() {
        // Regression guard: transient failures must not move accounts into the
        // "moderadas" group.
        assert!(!is_moderated_error("network timeout"));
        assert!(!is_moderated_error("429 Too Many Requests"));
        assert!(!is_moderated_error(""));
    }

    #[test]
    fn moderated_group_name_is_stable() {
        assert_eq!(MODERATED_GROUP, "moderadas");
    }
}

#[cfg(test)]
mod launch_shared_helper_tests {
    use super::*;

    fn temp_settings(tag: &str) -> SettingsStore {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        SettingsStore::new(std::env::temp_dir().join(format!("ram-lshared-{tag}-{nanos}.ini")))
    }

    #[allow(dead_code)]
    fn temp_accounts(tag: &str) -> AccountStore {
        crypto::init();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        AccountStore::new(std::env::temp_dir().join(format!("ram-lshared-{tag}-{nanos}.json")))
    }

    // ---- profile_key --------------------------------------------------------

    #[test]
    fn profile_key_picks_the_setting_key_of_each_profile() {
        assert_eq!(
            profile_key(LaunchClientProfile::Normal, "N", "P", "B"),
            "N"
        );
        assert_eq!(
            profile_key(LaunchClientProfile::BottingPlayer, "N", "P", "B"),
            "P"
        );
        assert_eq!(
            profile_key(LaunchClientProfile::BottingBot, "N", "P", "B"),
            "B"
        );
    }

    // ---- botting_uses_shared_client_profile / effective_launch_profile ------

    #[test]
    fn botting_uses_shared_client_profile_defaults_to_true() {
        let settings = temp_settings("shared-default");
        assert!(botting_uses_shared_client_profile(&settings));
    }

    #[test]
    fn botting_uses_shared_client_profile_is_false_only_for_the_exact_false_value() {
        let settings = temp_settings("shared-off");
        settings
            .set("General", "BottingUseSharedClientProfile", "false")
            .unwrap();
        assert!(!botting_uses_shared_client_profile(&settings));

        settings
            .set("General", "BottingUseSharedClientProfile", "TRUE")
            .unwrap();
        assert!(!botting_uses_shared_client_profile(&settings));

        settings
            .set("General", "BottingUseSharedClientProfile", "true")
            .unwrap();
        assert!(botting_uses_shared_client_profile(&settings));
    }

    #[test]
    fn effective_launch_profile_collapses_botting_profiles_when_sharing() {
        let settings = temp_settings("effective-shared");
        // Default is "share the Normal profile".
        assert!(matches!(
            effective_launch_profile(&settings, LaunchClientProfile::BottingBot),
            LaunchClientProfile::Normal
        ));
        assert!(matches!(
            effective_launch_profile(&settings, LaunchClientProfile::BottingPlayer),
            LaunchClientProfile::Normal
        ));
        assert!(matches!(
            effective_launch_profile(&settings, LaunchClientProfile::Normal),
            LaunchClientProfile::Normal
        ));
    }

    #[test]
    fn effective_launch_profile_keeps_botting_profiles_when_not_sharing() {
        let settings = temp_settings("effective-split");
        settings
            .set("General", "BottingUseSharedClientProfile", "false")
            .unwrap();
        assert!(matches!(
            effective_launch_profile(&settings, LaunchClientProfile::BottingBot),
            LaunchClientProfile::BottingBot
        ));
        assert!(matches!(
            effective_launch_profile(&settings, LaunchClientProfile::BottingPlayer),
            LaunchClientProfile::BottingPlayer
        ));
        assert!(matches!(
            effective_launch_profile(&settings, LaunchClientProfile::Normal),
            LaunchClientProfile::Normal
        ));
    }

    // ---- start_minimized_for_profile / custom_client_settings_path ---------

    #[test]
    fn start_minimized_for_profile_reads_the_per_profile_key() {
        let settings = temp_settings("minimized");
        settings
            .set("General", "BottingBotStartRobloxMinimized", "true")
            .unwrap();

        assert!(!start_minimized_for_profile(
            &settings,
            LaunchClientProfile::Normal
        ));
        assert!(!start_minimized_for_profile(
            &settings,
            LaunchClientProfile::BottingPlayer
        ));
        assert!(start_minimized_for_profile(
            &settings,
            LaunchClientProfile::BottingBot
        ));
    }

    #[test]
    fn custom_client_settings_path_reads_the_per_profile_key() {
        let settings = temp_settings("custom-path");
        settings
            .set("General", "CustomClientSettings", "C:/normal.json")
            .unwrap();
        settings
            .set(
                "General",
                "BottingPlayerCustomClientSettings",
                "C:/player.json",
            )
            .unwrap();

        assert_eq!(
            custom_client_settings_path(&settings, LaunchClientProfile::Normal),
            "C:/normal.json"
        );
        assert_eq!(
            custom_client_settings_path(&settings, LaunchClientProfile::BottingPlayer),
            "C:/player.json"
        );
        assert_eq!(
            custom_client_settings_path(&settings, LaunchClientProfile::BottingBot),
            ""
        );
    }

    // ---- windows_client_overrides ------------------------------------------

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_client_overrides_are_all_off_by_default() {
        let settings = temp_settings("overrides-default");
        let overrides = windows_client_overrides(&settings, true, LaunchClientProfile::Normal);

        assert_eq!(overrides.max_fps, None);
        assert_eq!(overrides.master_volume, None);
        assert_eq!(overrides.graphics_level, None);
        assert_eq!(overrides.window_size, None);
        assert!(overrides.fast_flags.is_none());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_client_overrides_reads_every_enabled_override() {
        let settings = temp_settings("overrides-on");
        settings.set("General", "UnlockFPS", "true").unwrap();
        settings.set("General", "MaxFPSValue", "240").unwrap();
        settings.set("General", "OverrideClientVolume", "true").unwrap();
        settings.set("General", "ClientVolume", "0.25").unwrap();
        settings.set("General", "OverrideClientGraphics", "true").unwrap();
        settings.set("General", "ClientGraphicsLevel", "7").unwrap();
        settings.set("General", "OverrideClientWindowSize", "true").unwrap();
        settings.set("General", "ClientWindowWidth", "800").unwrap();
        settings.set("General", "ClientWindowHeight", "600").unwrap();

        let overrides = windows_client_overrides(&settings, true, LaunchClientProfile::Normal);
        assert_eq!(overrides.max_fps, Some(240));
        assert_eq!(overrides.master_volume, Some(0.25));
        assert_eq!(overrides.graphics_level, Some(7));
        assert_eq!(overrides.window_size, Some((800, 600)));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_client_overrides_skips_fps_when_the_caller_disallows_it() {
        // A valid custom ClientAppSettings file wins over the FPS unlock.
        let settings = temp_settings("overrides-no-fps");
        settings.set("General", "UnlockFPS", "true").unwrap();
        settings.set("General", "MaxFPSValue", "240").unwrap();

        let overrides = windows_client_overrides(&settings, false, LaunchClientProfile::Normal);
        assert_eq!(overrides.max_fps, None);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_client_overrides_rejects_non_positive_numbers() {
        let settings = temp_settings("overrides-zero");
        settings.set("General", "UnlockFPS", "true").unwrap();
        settings.set("General", "MaxFPSValue", "0").unwrap();
        settings.set("General", "OverrideClientGraphics", "true").unwrap();
        settings.set("General", "ClientGraphicsLevel", "0").unwrap();
        settings.set("General", "OverrideClientWindowSize", "true").unwrap();
        settings.set("General", "ClientWindowWidth", "0").unwrap();
        settings.set("General", "ClientWindowHeight", "600").unwrap();

        let overrides = windows_client_overrides(&settings, true, LaunchClientProfile::Normal);
        assert_eq!(overrides.max_fps, None);
        assert_eq!(overrides.graphics_level, None);
        assert_eq!(overrides.window_size, None);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_client_overrides_clamps_volume_and_graphics_into_range() {
        let settings = temp_settings("overrides-clamp");
        settings.set("General", "OverrideClientVolume", "true").unwrap();
        settings.set("General", "ClientVolume", "9.5").unwrap();
        settings.set("General", "OverrideClientGraphics", "true").unwrap();
        settings.set("General", "ClientGraphicsLevel", "99").unwrap();

        let overrides = windows_client_overrides(&settings, true, LaunchClientProfile::Normal);
        assert_eq!(overrides.master_volume, Some(1.0));
        assert_eq!(overrides.graphics_level, Some(10));

        settings.set("General", "ClientVolume", "-3").unwrap();
        let overrides = windows_client_overrides(&settings, true, LaunchClientProfile::Normal);
        assert_eq!(overrides.master_volume, Some(0.0));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_client_overrides_falls_back_to_defaults_for_unparsable_numbers() {
        let settings = temp_settings("overrides-garbage");
        settings.set("General", "UnlockFPS", "true").unwrap();
        settings.set("General", "MaxFPSValue", "not-a-number").unwrap();
        settings.set("General", "OverrideClientVolume", "true").unwrap();
        settings.set("General", "ClientVolume", "loud").unwrap();

        let overrides = windows_client_overrides(&settings, true, LaunchClientProfile::Normal);
        assert_eq!(overrides.max_fps, Some(120));
        assert_eq!(overrides.master_volume, Some(0.5));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_client_overrides_uses_the_bot_profile_keys() {
        let settings = temp_settings("overrides-bot");
        settings.set("General", "BottingBotUnlockFPS", "true").unwrap();
        settings.set("General", "BottingBotMaxFPSValue", "30").unwrap();
        // The Normal keys must not leak into the bot profile.
        settings.set("General", "MaxFPSValue", "240").unwrap();

        let overrides = windows_client_overrides(&settings, true, LaunchClientProfile::BottingBot);
        assert_eq!(overrides.max_fps, Some(30));

        let normal = windows_client_overrides(&settings, true, LaunchClientProfile::Normal);
        assert_eq!(normal.max_fps, None, "Normal has UnlockFPS off");
    }

    // ---- browser tracker id -------------------------------------------------

    #[cfg(target_os = "windows")]
    #[test]
    fn get_or_create_browser_tracker_id_keeps_an_existing_id() {
        let store = temp_accounts("btid-existing");
        let mut account = data::accounts::Account::new("TOK".into(), "u".into(), 1);
        account.browser_tracker_id = "  1234567  ".to_string();
        store.add(account).unwrap();
        // `add` only merges a few fields for an existing id, so write the
        // tracker id through `update`.
        let mut stored = store.get_all().unwrap().remove(0);
        stored.browser_tracker_id = "  1234567  ".to_string();
        store.update(stored).unwrap();

        assert_eq!(get_or_create_browser_tracker_id(&store, 1).unwrap(), "1234567");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn get_or_create_browser_tracker_id_generates_and_persists_when_blank() {
        let store = temp_accounts("btid-generate");
        store
            .add(data::accounts::Account::new("TOK".into(), "u".into(), 2))
            .unwrap();

        let generated = get_or_create_browser_tracker_id(&store, 2).unwrap();
        assert!(!generated.trim().is_empty());
        assert!(generated.chars().all(|c| c.is_ascii_digit()));

        let persisted = store.get_all().unwrap()[0].browser_tracker_id.clone();
        assert_eq!(persisted, generated);
        // A second call must reuse the persisted id, not roll a new one.
        assert_eq!(get_or_create_browser_tracker_id(&store, 2).unwrap(), generated);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn get_or_create_browser_tracker_id_for_an_unknown_account_does_not_persist() {
        let store = temp_accounts("btid-unknown");
        let generated = get_or_create_browser_tracker_id(&store, 404).unwrap();
        assert!(!generated.is_empty());
        assert!(store.get_all().unwrap().is_empty());
    }

    #[test]
    fn save_browser_tracker_id_is_a_no_op_for_a_missing_account() {
        let store = temp_accounts("btid-save-missing");
        assert!(save_browser_tracker_id(&store, 999, "1").is_ok());
        assert!(store.get_all().unwrap().is_empty());
    }

    // ---- now_ms -------------------------------------------------------------

    #[test]
    fn now_ms_returns_a_plausible_unix_millisecond_timestamp() {
        let now = now_ms();
        // 2023-01-01 .. 2100-01-01 in milliseconds.
        assert!(now > 1_672_531_200_000, "now_ms looks too small: {now}");
        assert!(now < 4_102_444_800_000, "now_ms looks too large: {now}");
        assert!(now_ms() >= now, "now_ms must be monotonic in wall-clock terms");
    }

    // ---- decode_url_component / strip_ascii_prefix -------------------------

    #[test]
    fn decode_url_component_decodes_percent_escapes() {
        assert_eq!(decode_url_component("a%20b"), "a b");
        assert_eq!(decode_url_component("roblox%3A%2F%2F"), "roblox://");
        assert_eq!(decode_url_component("%C3%A9"), "é");
    }

    #[test]
    fn decode_url_component_returns_invalid_input_unchanged() {
        assert_eq!(decode_url_component("100%"), "100%");
        assert_eq!(decode_url_component("%ZZ"), "%ZZ");
        assert_eq!(decode_url_component(""), "");
        assert_eq!(decode_url_component("já-decodificado"), "já-decodificado");
    }

    #[test]
    fn strip_ascii_prefix_is_case_insensitive_and_bounds_safe() {
        assert_eq!(strip_ascii_prefix("vip:ABC", "vip:"), Some("ABC"));
        assert_eq!(strip_ascii_prefix("VIP:ABC", "vip:"), Some("ABC"));
        assert_eq!(strip_ascii_prefix("vi", "vip:"), None);
        assert_eq!(strip_ascii_prefix("", "vip:"), None);
        assert_eq!(strip_ascii_prefix("nope:ABC", "vip:"), None);
        // Must not panic when the prefix length lands inside a multi-byte char.
        assert_eq!(strip_ascii_prefix("éé", "vip:"), None);
        assert_eq!(strip_ascii_prefix("ép:x", "vip:"), None);
    }

    // ---- looks_like_share_link ---------------------------------------------

    #[test]
    fn looks_like_share_link_matches_every_known_share_shape() {
        assert!(looks_like_share_link("https://www.roblox.com/share?code=x"));
        assert!(looks_like_share_link("https://ro.blox.com/share-links/abc"));
        assert!(looks_like_share_link("roblox://navigation/share_links?code=x"));
        assert!(looks_like_share_link("https://x/y?type=Server"));
        assert!(looks_like_share_link("https://x/y?pid=Server"));
        // Case-insensitive.
        assert!(looks_like_share_link("HTTPS://WWW.ROBLOX.COM/SHARE?CODE=X"));
    }

    #[test]
    fn looks_like_share_link_rejects_plain_game_and_vip_links() {
        assert!(!looks_like_share_link("https://www.roblox.com/games/123/Name"));
        assert!(!looks_like_share_link("vip:ABC"));
        assert!(!looks_like_share_link(""));
    }

    // ---- extract_private_server_link_code ----------------------------------

    #[test]
    fn extract_private_server_link_code_reads_the_vip_prefix() {
        assert_eq!(
            extract_private_server_link_code("vip:ABC123").as_deref(),
            Some("ABC123")
        );
        assert_eq!(
            extract_private_server_link_code("VIP:  ABC%20123 ").as_deref(),
            Some("ABC 123")
        );
        assert_eq!(extract_private_server_link_code("vip:"), None);
        assert_eq!(extract_private_server_link_code("vip:   "), None);
    }

    #[test]
    fn extract_private_server_link_code_prefers_private_server_link_code() {
        assert_eq!(
            extract_private_server_link_code(
                "https://www.roblox.com/games/1/x?privateServerLinkCode=AAA&linkCode=BBB"
            )
            .as_deref(),
            Some("AAA")
        );
    }

    #[test]
    fn extract_private_server_link_code_falls_back_to_link_code() {
        assert_eq!(
            extract_private_server_link_code("https://www.roblox.com/games/1/x?linkCode=BBB")
                .as_deref(),
            Some("BBB")
        );
    }

    #[test]
    fn extract_private_server_link_code_reads_code_only_from_share_shaped_input() {
        // A bare `code=` query on a non-share URL is not a private server code.
        assert_eq!(
            extract_private_server_link_code("https://www.roblox.com/games/1/x?code=CCC"),
            None
        );
        assert_eq!(
            extract_private_server_link_code("code=CCC").as_deref(),
            Some("CCC")
        );
        assert_eq!(
            extract_private_server_link_code("https://www.roblox.com/share?code=CCC").as_deref(),
            Some("CCC")
        );
    }

    #[test]
    fn extract_private_server_link_code_returns_none_for_plain_input() {
        assert_eq!(extract_private_server_link_code(""), None);
        assert_eq!(extract_private_server_link_code("   "), None);
        assert_eq!(
            extract_private_server_link_code("11111111-2222-3333-4444-555555555555"),
            None
        );
    }

    // ---- resolve_private_join (no network on these paths) ------------------

    #[tokio::test]
    async fn resolve_private_join_on_a_public_target_uses_the_requested_place() {
        let launch = resolve_launch_job("", false, "");
        let resolved = resolve_private_join("", 606849621, &launch).await.unwrap();

        assert_eq!(resolved.place_id, 606849621);
        assert!(resolved.link_code.is_empty());
        assert!(resolved.access_code.is_empty());
        assert!(!resolved.use_private_join);
    }

    #[tokio::test]
    async fn resolve_private_join_moves_an_access_code_shaped_value_into_access_code() {
        let launch = resolve_launch_job("vip:11111111-2222-3333-4444-555555555555", false, "");
        let resolved = resolve_private_join("", 1, &launch).await.unwrap();

        assert_eq!(resolved.access_code, "11111111-2222-3333-4444-555555555555");
        assert!(resolved.link_code.is_empty());
        assert!(resolved.use_private_join);
    }

    #[tokio::test]
    async fn resolve_private_join_keeps_a_plain_link_code() {
        let launch = resolve_launch_job("vip:SHORTCODE", false, "");
        let resolved = resolve_private_join("", 42, &launch).await.unwrap();

        assert_eq!(resolved.link_code, "SHORTCODE");
        assert!(resolved.access_code.is_empty());
        assert!(resolved.use_private_join);
        assert_eq!(resolved.place_id, 42);
    }

    #[tokio::test]
    async fn resolve_private_join_takes_the_place_id_from_a_games_url() {
        let launch = resolve_launch_job(
            "https://www.roblox.com/games/606849621/Jailbreak?privateServerLinkCode=SHORT",
            false,
            "",
        );
        let resolved = resolve_private_join("", 1, &launch).await.unwrap();

        assert_eq!(
            resolved.place_id, 606849621,
            "the URL's place id must win over the one passed in"
        );
        assert_eq!(resolved.link_code, "SHORT");
        assert!(resolved.use_private_join);
    }

    #[tokio::test]
    async fn resolve_private_join_marks_join_vip_even_without_a_code() {
        let launch = ResolvedLaunchJob {
            job_id: String::new(),
            join_vip: true,
            link_code: String::new(),
        };
        let resolved = resolve_private_join("", 5, &launch).await.unwrap();
        assert!(resolved.use_private_join);
        assert_eq!(resolved.place_id, 5);
    }

    // ---- backoff / 429 detection -------------------------------------------

    #[cfg(target_os = "windows")]
    #[test]
    fn backoff_delay_seconds_doubles_and_clamps_to_the_5_to_300_window() {
        assert_eq!(backoff_delay_seconds(8, 1, 6), 8);
        assert_eq!(backoff_delay_seconds(8, 2, 6), 16);
        assert_eq!(backoff_delay_seconds(8, 3, 6), 32);
        assert_eq!(backoff_delay_seconds(8, 4, 6), 64);
        assert_eq!(backoff_delay_seconds(8, 5, 6), 128);
        assert_eq!(backoff_delay_seconds(8, 6, 6), 256);
        // Capped at 300 seconds however many retries pile up.
        assert_eq!(backoff_delay_seconds(8, 7, 6), 300);
        assert_eq!(backoff_delay_seconds(8, u32::MAX, 6), 300);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn backoff_delay_seconds_never_returns_less_than_five_seconds() {
        assert_eq!(backoff_delay_seconds(0, 1, 6), 5);
        assert_eq!(backoff_delay_seconds(1, 1, 6), 5);
        assert_eq!(backoff_delay_seconds(1, 3, 6), 5);
        assert_eq!(backoff_delay_seconds(1, 4, 6), 8);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn backoff_delay_seconds_treats_retry_count_zero_as_the_first_attempt() {
        assert_eq!(backoff_delay_seconds(10, 0, 6), 10);
        assert_eq!(backoff_delay_seconds(10, 1, 6), 10);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn backoff_delay_seconds_handles_degenerate_retry_max_without_overflowing() {
        assert_eq!(backoff_delay_seconds(10, 5, 0), 20);
        assert_eq!(backoff_delay_seconds(u64::MAX, 3, 6), 300);
        assert_eq!(backoff_delay_seconds(10, 40, u32::MAX), 300);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn is_429_related_error_matches_every_rate_limit_phrasing() {
        assert!(is_429_related_error("HTTP 429 Too Many Requests"));
        assert!(is_429_related_error("TOO MANY REQUESTS"));
        assert!(is_429_related_error("Authentifizierung fehlgeschlagen"));
        assert!(is_429_related_error("Authentication Failed"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn is_429_related_error_ignores_unrelated_failures() {
        assert!(!is_429_related_error(""));
        assert!(!is_429_related_error("network timeout"));
        assert!(!is_429_related_error("User is moderated"));
        assert!(!is_429_related_error("status 403"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn title_looks_auth_failure_matches_the_german_and_english_dialogs() {
        assert!(title_looks_auth_failure("Authentifizierung fehlgeschlagen"));
        assert!(title_looks_auth_failure("Authentication failed"));
        assert!(title_looks_auth_failure("Fehlercode: 429"));
        assert!(title_looks_auth_failure("Roblox — Error Code: 429"));
        assert!(!title_looks_auth_failure("Roblox"));
        assert!(!title_looks_auth_failure(""));
        // A plain 429 in a window title is not enough on its own.
        assert!(!title_looks_auth_failure("429"));
    }

    // ---- wait_for_launch_slot ----------------------------------------------

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn wait_for_launch_slot_does_not_wait_on_the_first_launch() {
        // No previous launch means no spacing to honour, however large the
        // configured delay is.
        let start = std::time::Instant::now();
        let mut last: Option<std::time::Instant> = None;
        wait_for_launch_slot(&mut last, 3600).await;

        assert!(last.is_some(), "the slot must be stamped for the next launch");
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn wait_for_launch_slot_spaces_consecutive_launches_by_the_delay() {
        let mut last: Option<std::time::Instant> = None;
        wait_for_launch_slot(&mut last, 1).await;
        let first_stamp = last.expect("first launch stamps the slot");

        let start = std::time::Instant::now();
        wait_for_launch_slot(&mut last, 1).await;
        let waited = start.elapsed();

        assert!(
            waited >= std::time::Duration::from_millis(900),
            "expected roughly a 1s gap, waited {waited:?}"
        );
        assert!(
            last.expect("second launch re-stamps the slot") > first_stamp,
            "the slot timestamp must move forward"
        );
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn wait_for_launch_slot_with_a_zero_delay_never_blocks() {
        let mut last: Option<std::time::Instant> = None;
        wait_for_launch_slot(&mut last, 0).await;
        let start = std::time::Instant::now();
        wait_for_launch_slot(&mut last, 0).await;
        assert!(start.elapsed() < std::time::Duration::from_millis(500));
    }

    // ---- wait_for_new_roblox_pid -------------------------------------------

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn wait_for_new_roblox_pid_times_out_when_no_new_client_appears() {
        // Baseline = every client already running, so nothing can look "new".
        let baseline = platform::windows::get_roblox_pids();
        let found =
            wait_for_new_roblox_pid(&baseline, std::time::Duration::from_millis(1)).await;
        assert_eq!(found, None);
    }

    // ---- apply_windows_post_launch_profile ---------------------------------

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn apply_windows_post_launch_profile_returns_early_with_no_policy_configured() {
        // With the shipped defaults there is nothing to apply, so the function
        // must not touch the (here: nonexistent) process at all.
        let settings = temp_settings("post-launch-noop");
        let started = std::time::Instant::now();
        apply_windows_post_launch_profile(None, &settings, LaunchClientProfile::Normal, 0).await;
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    // ---- botting status payload --------------------------------------------

    #[cfg(target_os = "windows")]
    fn test_session(stopped: bool) -> BottingSession {
        let mut players = HashSet::new();
        players.insert(20);
        let cfg = BottingConfig {
            user_ids: vec![30, 10, 20],
            place_id: 606849621,
            job_id: "job-1".to_string(),
            launch_data: "data".to_string(),
            player_user_ids: players,
            interval_minutes: 19,
            launch_delay_seconds: 20,
            retry_max: 6,
            retry_base_seconds: 8,
            player_grace_minutes: 15,
            adopt_running: false,
        };
        let mut runtime = HashMap::new();
        for uid in [30_i64, 10, 20] {
            runtime.insert(
                uid,
                BottingAccountRuntime {
                    user_id: uid,
                    is_player: uid == 20,
                    disconnected: false,
                    manual_restart_pending: false,
                    manual_restart_keep_schedule: false,
                    manual_restart_saved_next_restart_at_ms: None,
                    phase: "queued",
                    retry_count: 0,
                    next_restart_at_ms: None,
                    player_grace_until_ms: None,
                    last_error: None,
                },
            );
        }
        BottingSession {
            id: 7,
            stop_flag: Arc::new(AtomicBool::new(stopped)),
            stopped_notify: Arc::new(tokio::sync::Notify::new()),
            started_at_ms: 1_700_000_000_000,
            config: Arc::new(Mutex::new(cfg)),
            accounts: Arc::new(Mutex::new(runtime)),
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn botting_status_from_session_sorts_accounts_and_player_ids() {
        let payload = botting_status_from_session(&test_session(false));

        assert!(payload.active);
        assert_eq!(payload.started_at_ms, Some(1_700_000_000_000));
        assert_eq!(payload.place_id, 606849621);
        assert_eq!(payload.job_id, "job-1");
        assert_eq!(payload.launch_data, "data");
        assert_eq!(payload.interval_minutes, 19);
        assert_eq!(payload.launch_delay_seconds, 20);
        assert_eq!(payload.player_grace_minutes, 15);
        assert_eq!(payload.player_user_ids, vec![20]);
        // user_ids keeps the configured order; accounts are sorted for the UI.
        assert_eq!(payload.user_ids, vec![30, 10, 20]);
        let ids: Vec<i64> = payload.accounts.iter().map(|a| a.user_id).collect();
        assert_eq!(ids, vec![10, 20, 30]);
        assert!(payload.accounts.iter().any(|a| a.user_id == 20 && a.is_player));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn botting_status_from_session_reports_a_stopping_session_as_inactive() {
        let payload = botting_status_from_session(&test_session(true));
        assert!(!payload.active);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn botting_status_payload_serializes_with_camel_case_keys() {
        let payload = botting_status_from_session(&test_session(false));
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["startedAtMs"], 1_700_000_000_000_i64);
        assert_eq!(json["intervalMinutes"], 19);
        assert_eq!(json["launchDelaySeconds"], 20);
        assert_eq!(json["playerGraceMinutes"], 15);
        assert_eq!(json["playerUserIds"], serde_json::json!([20]));
        assert_eq!(json["accounts"][0]["userId"], 10);
        assert_eq!(json["accounts"][0]["retryCount"], 0);
    }

    #[test]
    fn botting_status_payload_default_is_an_inactive_session() {
        let payload = BottingStatusPayload::default();
        assert!(!payload.active);
        assert_eq!(payload.started_at_ms, None);
        assert!(payload.accounts.is_empty());
        assert!(payload.user_ids.is_empty());
    }
}
