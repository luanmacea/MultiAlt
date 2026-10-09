fn watcher_clamped_u64(
    settings: &SettingsStore,
    key: &str,
    default: i64,
    min: i64,
    max: i64,
) -> u64 {
    settings
        .get_int("Watcher", key)
        .unwrap_or(default)
        .clamp(min, max) as u64
}

fn watcher_due(last_tick: Option<std::time::Instant>, interval_ms: u64) -> bool {
    match last_tick {
        Some(tick) => tick.elapsed().as_millis() as u64 >= interval_ms,
        None => true,
    }
}

fn watcher_remaining_ms(last_tick: Option<std::time::Instant>, interval_ms: u64) -> u64 {
    match last_tick {
        Some(tick) => interval_ms.saturating_sub(tick.elapsed().as_millis() as u64),
        None => 0,
    }
}

#[cfg(target_os = "windows")]
#[derive(Clone)]
struct WindowsWatcherConfig {
    scan_interval_ms: u64,
    memory_enabled: bool,
    memory_low_mb: u64,
    title_enabled: bool,
    expected_title: String,
    save_window_positions: bool,
    exit_if_no_connection: bool,
    no_connection_timeout_secs: u64,
    exit_on_beta: bool,
    startup_grace_secs: u64,
}

#[cfg(target_os = "windows")]
fn load_windows_watcher_config(settings: &SettingsStore) -> WindowsWatcherConfig {
    WindowsWatcherConfig {
        scan_interval_ms: watcher_clamped_u64(settings, "ScanInterval", 6, 1, 3600) * 1000,
        memory_enabled: settings.get_bool("Watcher", "CloseRbxMemory"),
        memory_low_mb: watcher_clamped_u64(settings, "MemoryLowValue", 200, 1, 16384),
        title_enabled: settings.get_bool("Watcher", "CloseRbxWindowTitle"),
        expected_title: settings.get_string("Watcher", "ExpectedWindowTitle"),
        save_window_positions: settings.get_bool("Watcher", "SaveWindowPositions"),
        exit_if_no_connection: settings.get_bool("Watcher", "ExitIfNoConnection"),
        no_connection_timeout_secs: watcher_clamped_u64(
            settings,
            "NoConnectionTimeout",
            60,
            1,
            3600,
        ),
        exit_on_beta: settings.get_bool("Watcher", "ExitOnBeta"),
        startup_grace_secs: 30,
    }
}

#[cfg(target_os = "windows")]
/// O Watcher só mexe nos clientes que o app abriu: a promessa da tela é que
/// ele ignora os clientes abertos fora do app. Um cliente aberto pelo site e
/// reconhecido pelo log (`external_clients.rs`) entra no rastreamento para a
/// Sessão e o Modo AFK, mas as regras que fecham cliente não valem para ele —
/// senão uma regra de memória ou de desconexão fecharia a conta que o usuário
/// está jogando pelo site.
fn only_launched_by_app<T>(instances: Vec<T>, adopted: impl Fn(&T) -> bool) -> Vec<T> {
    instances.into_iter().filter(|inst| !adopted(inst)).collect()
}

fn windows_title_indicates_disconnect(title_lower: &str) -> bool {
    title_lower.contains("disconnected")
        || title_lower.contains("connection error")
        || title_lower.contains("lost connection")
        || title_lower.contains("no connection")
}

/// A conta está sem conexão? O log do cliente manda quando foi achado (diz o
/// motivo e não confunde teleporte com queda); o título da janela fica só de
/// reserva para o cliente cujo log não apareceu. `None`: não dá para saber
/// (sem log e sem título) — o contador não mexe.
fn client_connection_lost(health: Option<&ClientHealthView>, title_lower: &str) -> Option<bool> {
    match health {
        Some(view) if view.log_found => Some(view.drop.is_some()),
        _ if !title_lower.is_empty() => Some(windows_title_indicates_disconnect(title_lower)),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn windows_title_indicates_beta(title: &str) -> bool {
    title.to_lowercase().contains("roblox beta")
}

#[cfg(target_os = "windows")]
#[tauri::command]
async fn start_watcher(
    app: tauri::AppHandle,
    _settings: tauri::State<'_, SettingsStore>,
) -> Result<(), String> {
    use platform::windows;

    let tracker = windows::tracker();
    let Some(session) = tracker.try_start_watcher() else {
        return Ok(());
    };

    let app_handle = app.clone();

    tokio::spawn(async move {
        let mut disconnected_since: HashMap<i64, std::time::Instant> = HashMap::new();
        let mut startup_seen: HashMap<i64, (u32, std::time::Instant)> = HashMap::new();
        let mut last_saved_positions: HashMap<i64, (i32, i32, i32, i32)> = HashMap::new();
        let mut last_scan_at: Option<std::time::Instant> = None;

        while tracker.is_watcher_session_active(session) {
            let cfg = {
                let settings_state = app_handle.state::<SettingsStore>();
                load_windows_watcher_config(settings_state.inner())
            };

            if watcher_due(last_scan_at, cfg.scan_interval_ms) {
                last_scan_at = Some(std::time::Instant::now());

                let dead_users = tracker.cleanup_dead_processes();
                for uid in &dead_users {
                    disconnected_since.remove(uid);
                    startup_seen.remove(uid);
                    last_saved_positions.remove(uid);
                    let _ = app_handle.emit(
                        "roblox-process-died",
                        serde_json::json!({
                            "userId": uid,
                        }),
                    );
                    // O console e o historico geral: evento do Watcher tambem vira linha la.
                    emit_launch_log(&app_handle, *uid, "warn", "watcher", String::from("Cliente do Roblox fechou (o processo morreu)"));
                }

                let instances = only_launched_by_app(tracker.get_all(), |inst| inst.adopted);
                let active_user_ids: HashSet<i64> = instances.iter().map(|inst| inst.user_id).collect();
                disconnected_since.retain(|uid, _| active_user_ids.contains(uid));
                startup_seen.retain(|uid, _| active_user_ids.contains(uid));
                last_saved_positions.retain(|uid, _| active_user_ids.contains(uid));

                let fg_hwnd = windows::get_foreground_hwnd();

                for inst in instances {
                    let Some(hwnd) = windows::find_main_window(inst.pid) else {
                        continue;
                    };

                    if hwnd == fg_hwnd {
                        continue;
                    }

                    let startup = startup_seen
                        .entry(inst.user_id)
                        .or_insert_with(|| (inst.pid, std::time::Instant::now()));
                    if startup.0 != inst.pid {
                        *startup = (inst.pid, std::time::Instant::now());
                    }
                    let startup_grace_elapsed = startup.1.elapsed().as_secs() >= cfg.startup_grace_secs;

                    if cfg.memory_enabled && startup_grace_elapsed {
                        if let Some(mem) = windows::get_process_memory_mb(inst.pid) {
                            if mem < cfg.memory_low_mb && tracker.kill_for_user(inst.user_id) {
                                let _ = app_handle.emit(
                                    "roblox-low-memory",
                                    serde_json::json!({
                                        "userId": inst.user_id,
                                        "memoryMb": mem,
                                    }),
                                );
                                // O console e o historico geral: evento do Watcher tambem vira linha la.
                                emit_launch_log(&app_handle, inst.user_id, "warn", "watcher", format!("Cliente fechado pelo Watcher: memoria em {} MB, abaixo do minimo", mem));
                                disconnected_since.remove(&inst.user_id);
                                startup_seen.remove(&inst.user_id);
                                last_saved_positions.remove(&inst.user_id);
                                continue;
                            }
                        }
                    }

                    let should_read_title = (cfg.title_enabled
                        && startup_grace_elapsed
                        && !cfg.expected_title.is_empty())
                        || cfg.exit_on_beta
                        || cfg.exit_if_no_connection;

                    let title = if should_read_title {
                        windows::get_window_title(hwnd)
                    } else {
                        String::new()
                    };

                    if cfg.title_enabled
                        && startup_grace_elapsed
                        && !cfg.expected_title.is_empty()
                        && !title.is_empty()
                        && title != cfg.expected_title
                    {
                        if tracker.kill_for_user(inst.user_id) {
                            let _ = app_handle.emit(
                                "roblox-title-mismatch",
                                serde_json::json!({
                                    "userId": inst.user_id,
                                    "title": title,
                                    "expected": cfg.expected_title.clone(),
                                }),
                            );
                            // O console e o historico geral: evento do Watcher tambem vira linha la.
                            emit_launch_log(&app_handle, inst.user_id, "warn", "watcher", format!("Cliente fechado pelo Watcher: titulo \"{}\" nao e o esperado", title));
                            disconnected_since.remove(&inst.user_id);
                            startup_seen.remove(&inst.user_id);
                            last_saved_positions.remove(&inst.user_id);
                            continue;
                        }
                    }

                    if cfg.exit_on_beta && windows_title_indicates_beta(&title) {
                        if tracker.kill_for_user(inst.user_id) {
                            let _ = app_handle.emit(
                                "roblox-beta-detected",
                                serde_json::json!({
                                    "userId": inst.user_id,
                                    "title": title,
                                }),
                            );
                            // O console e o historico geral: evento do Watcher tambem vira linha la.
                            emit_launch_log(&app_handle, inst.user_id, "warn", "watcher", String::from("Cliente fechado pelo Watcher: Roblox Beta detectado"));
                            disconnected_since.remove(&inst.user_id);
                            startup_seen.remove(&inst.user_id);
                            last_saved_positions.remove(&inst.user_id);
                            continue;
                        }
                    }

                    if cfg.exit_if_no_connection {
                        let lower_title = title.to_lowercase();
                        let health = client_health_of(inst.user_id, inst.pid);
                        if let Some(lost) = client_connection_lost(health.as_ref(), &lower_title) {
                            if lost {
                                let since = disconnected_since
                                    .entry(inst.user_id)
                                    .or_insert_with(std::time::Instant::now);
                                if since.elapsed().as_secs() >= cfg.no_connection_timeout_secs
                                    && tracker.kill_for_user(inst.user_id)
                                {
                                    let _ = app_handle.emit(
                                        "roblox-no-connection",
                                        serde_json::json!({
                                            "userId": inst.user_id,
                                            "title": lower_title,
                                            "timeout": cfg.no_connection_timeout_secs,
                                        }),
                                    );
                                    // O console e o historico geral: evento do Watcher tambem vira linha la.
                                    emit_launch_log(&app_handle, inst.user_id, "warn", "watcher", format!("Cliente fechado pelo Watcher: sem conexao por {}s", cfg.no_connection_timeout_secs));
                                    disconnected_since.remove(&inst.user_id);
                                    startup_seen.remove(&inst.user_id);
                                    last_saved_positions.remove(&inst.user_id);
                                    continue;
                                }
                            } else {
                                disconnected_since.remove(&inst.user_id);
                            }
                        }
                    }

                    if cfg.save_window_positions && startup_grace_elapsed {
                        if let Some(position) = windows::get_window_position(hwnd) {
                            let changed_since_last_tick = last_saved_positions
                                .get(&inst.user_id)
                                .map(|saved| *saved != position)
                                .unwrap_or(true);

                            if changed_since_last_tick {
                                let mut persisted = false;
                                let store = app_handle.state::<AccountStore>();
                                if let Ok(accounts) = store.get_all() {
                                    if let Some(mut account) =
                                        accounts.into_iter().find(|a| a.user_id == inst.user_id)
                                    {
                                        let x = position.0.to_string();
                                        let y = position.1.to_string();
                                        let w = position.2.to_string();
                                        let h = position.3.to_string();

                                        let unchanged = account
                                            .fields
                                            .get("Window_Position_X")
                                            .map(String::as_str)
                                            == Some(x.as_str())
                                            && account
                                                .fields
                                                .get("Window_Position_Y")
                                                .map(String::as_str)
                                                == Some(y.as_str())
                                            && account
                                                .fields
                                                .get("Window_Width")
                                                .map(String::as_str)
                                                == Some(w.as_str())
                                            && account
                                                .fields
                                                .get("Window_Height")
                                                .map(String::as_str)
                                                == Some(h.as_str());

                                        if !unchanged {
                                            account.fields.insert("Window_Position_X".into(), x);
                                            account.fields.insert("Window_Position_Y".into(), y);
                                            account.fields.insert("Window_Width".into(), w);
                                            account.fields.insert("Window_Height".into(), h);
                                            if store.update(account).is_ok() {
                                                persisted = true;
                                            }
                                        } else {
                                            persisted = true;
                                        }
                                    }
                                }

                                if persisted {
                                    last_saved_positions.insert(inst.user_id, position);
                                }
                            }
                        }
                    }
                }
            }

            if !tracker.is_watcher_session_active(session) {
                break;
            }

            let sleep_ms = watcher_remaining_ms(last_scan_at, cfg.scan_interval_ms).clamp(50, 1000);
            tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)).await;
        }
    });

    Ok(())
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn start_watcher(
    app: tauri::AppHandle,
    _settings: tauri::State<'_, SettingsStore>,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        #[derive(Clone)]
        struct MacWatcherConfig {
            scan_interval_ms: u64,
            read_interval_ms: u64,
            exit_if_no_connection: bool,
            no_connection_timeout_secs: u64,
            exit_on_beta: bool,
        }

        fn load_macos_watcher_config(settings: &SettingsStore) -> MacWatcherConfig {
            MacWatcherConfig {
                scan_interval_ms: watcher_clamped_u64(settings, "ScanInterval", 6, 1, 3600) * 1000,
                read_interval_ms: watcher_clamped_u64(settings, "ReadInterval", 250, 50, 60000),
                exit_if_no_connection: settings.get_bool("Watcher", "ExitIfNoConnection"),
                no_connection_timeout_secs: watcher_clamped_u64(
                    settings,
                    "NoConnectionTimeout",
                    60,
                    1,
                    3600,
                ),
                exit_on_beta: settings.get_bool("Watcher", "ExitOnBeta"),
            }
        }

        fn macos_line_indicates_beta_home(line_lower: &str) -> bool {
            line_lower.contains("[flog::singlesurfaceapp] returntoluaapp:")
                && line_lower.contains("returning from game")
        }

        use platform::macos;

        let tracker = macos::tracker();
        let Some(session) = tracker.try_start_watcher() else {
            return Ok(());
        };

        let app_handle = app.clone();
        tokio::spawn(async move {
            let mut disconnected_since: HashMap<i64, std::time::Instant> = HashMap::new();
            let mut log_paths: HashMap<u32, std::path::PathBuf> = HashMap::new();
            let mut log_offsets: HashMap<u32, u64> = HashMap::new();
            let mut last_scan_at: Option<std::time::Instant> = None;
            let mut last_read_at: Option<std::time::Instant> = None;

            while tracker.is_watcher_session_active(session) {
                let cfg = {
                    let settings_state = app_handle.state::<SettingsStore>();
                    load_macos_watcher_config(settings_state.inner())
                };

                if watcher_due(last_scan_at, cfg.scan_interval_ms) {
                    last_scan_at = Some(std::time::Instant::now());

                    let dead_users = tracker.cleanup_dead_processes();
                    for uid in &dead_users {
                        disconnected_since.remove(uid);
                        let _ = app_handle.emit(
                            "roblox-process-died",
                            serde_json::json!({
                                "userId": uid,
                            }),
                        );
                        // O console e o historico geral: evento do Watcher tambem vira linha la.
                        emit_launch_log(&app_handle, *uid, "warn", "watcher", String::from("Cliente do Roblox fechou (o processo morreu)"));
                    }

                    let instances = tracker.get_all();
                    let active_user_ids: HashSet<i64> = instances.iter().map(|inst| inst.user_id).collect();
                    let active_pids: HashSet<u32> = instances.iter().map(|inst| inst.pid).collect();

                    disconnected_since.retain(|uid, _| active_user_ids.contains(uid));
                    log_paths.retain(|pid, _| active_pids.contains(pid));
                    log_offsets.retain(|pid, _| active_pids.contains(pid));
                }

                let read_checks_enabled = cfg.exit_if_no_connection || cfg.exit_on_beta;

                if read_checks_enabled && watcher_due(last_read_at, cfg.read_interval_ms) {
                    last_read_at = Some(std::time::Instant::now());

                    let instances = tracker.get_all();
                    let active_user_ids: HashSet<i64> =
                        instances.iter().map(|inst| inst.user_id).collect();
                    disconnected_since.retain(|uid, _| active_user_ids.contains(uid));

                    for inst in instances {
                        let log_path = match log_paths.get(&inst.pid).cloned() {
                            Some(path) if path.exists() => path,
                            _ => {
                                let Some(path) = macos::latest_log_file_for_pid(inst.pid) else {
                                    continue;
                                };
                                log_paths.insert(inst.pid, path.clone());
                                path
                            }
                        };

                        let cursor = log_offsets.entry(inst.pid).or_insert(0);
                        let chunk = match macos::read_log_delta(&log_path, cursor) {
                            Ok(s) => s,
                            Err(_) => {
                                log_paths.remove(&inst.pid);
                                log_offsets.remove(&inst.pid);
                                continue;
                            }
                        };

                        let mut beta_detected = false;
                        if !chunk.is_empty() {
                            for line in chunk.lines() {
                                let lower = line.to_lowercase();
                                if cfg.exit_on_beta && macos_line_indicates_beta_home(&lower) {
                                    beta_detected = true;
                                }
                                if cfg.exit_if_no_connection {
                                    // Mesmo classificador do Windows (client_health.rs):
                                    // o 285 de toda saída e de todo teleporte não é queda.
                                    match classify_log_line(line) {
                                        Some(ClientLogEvent::JoinedGame { .. }) => {
                                            disconnected_since.remove(&inst.user_id);
                                        }
                                        Some(ClientLogEvent::Disconnected { .. })
                                        | Some(ClientLogEvent::Kicked { .. })
                                        | Some(ClientLogEvent::ServerShutdown { .. }) => {
                                            disconnected_since
                                                .entry(inst.user_id)
                                                .or_insert_with(std::time::Instant::now);
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }

                        if cfg.exit_on_beta && beta_detected {
                            if tracker.kill_for_user(inst.user_id) {
                                let _ = app_handle.emit(
                                    "roblox-beta-detected",
                                    serde_json::json!({
                                        "userId": inst.user_id,
                                        "logPath": log_path.to_string_lossy(),
                                    }),
                                );
                                // O console e o historico geral: evento do Watcher tambem vira linha la.
                                emit_launch_log(&app_handle, inst.user_id, "warn", "watcher", String::from("Cliente fechado pelo Watcher: Roblox Beta detectado"));
                                disconnected_since.remove(&inst.user_id);
                                log_paths.remove(&inst.pid);
                                log_offsets.remove(&inst.pid);
                                continue;
                            }
                        }

                        if cfg.exit_if_no_connection {
                            if let Some(since) = disconnected_since.get(&inst.user_id) {
                                if since.elapsed().as_secs() >= cfg.no_connection_timeout_secs
                                    && tracker.kill_for_user(inst.user_id)
                                {
                                    let _ = app_handle.emit(
                                        "roblox-no-connection",
                                        serde_json::json!({
                                            "userId": inst.user_id,
                                            "timeout": cfg.no_connection_timeout_secs,
                                            "logPath": log_path.to_string_lossy(),
                                        }),
                                    );
                                    // O console e o historico geral: evento do Watcher tambem vira linha la.
                                    emit_launch_log(&app_handle, inst.user_id, "warn", "watcher", format!("Cliente fechado pelo Watcher: sem conexao por {}s", cfg.no_connection_timeout_secs));
                                    disconnected_since.remove(&inst.user_id);
                                    log_paths.remove(&inst.pid);
                                    log_offsets.remove(&inst.pid);
                                }
                            }
                        }
                    }
                }

                if !tracker.is_watcher_session_active(session) {
                    break;
                }

                let scan_remaining = watcher_remaining_ms(last_scan_at, cfg.scan_interval_ms);
                let sleep_ms = if read_checks_enabled {
                    let read_remaining = watcher_remaining_ms(last_read_at, cfg.read_interval_ms);
                    scan_remaining.min(read_remaining)
                } else {
                    scan_remaining
                }
                .clamp(50, 1000);
                tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)).await;
            }
        });

        return Ok(());
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = (app, _settings);
        Err("Watcher is only supported on Windows and macOS".into())
    }
}

#[tauri::command]
fn stop_watcher() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        platform::windows::tracker().stop_watcher();
    }
    #[cfg(target_os = "macos")]
    {
        platform::macos::tracker().stop_watcher();
    }
    Ok(())
}

#[cfg(test)]
mod watcher_tests {
    use super::*;

    #[test]
    fn the_watcher_leaves_clients_opened_outside_the_app_alone() {
        let all = vec![(1, false), (2, true), (3, false)];
        let watched = only_launched_by_app(all, |(_, adopted)| *adopted);
        assert_eq!(watched, vec![(1, false), (3, false)]);
    }
    use std::path::PathBuf;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    fn unique_settings_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("ram-watcher-{name}-{nanos}.ini"))
    }

    struct TempSettings {
        store: SettingsStore,
        path: PathBuf,
    }

    impl TempSettings {
        fn new(name: &str) -> Self {
            let path = unique_settings_path(name);
            let store = SettingsStore::new(path.clone());
            Self { store, path }
        }
    }

    impl Drop for TempSettings {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    // ---- watcher_clamped_u64 ------------------------------------------------

    #[test]
    fn watcher_clamped_u64_uses_the_stored_value_when_in_range() {
        let s = TempSettings::new("in-range");
        s.store.set("Watcher", "ScanInterval", "12").unwrap();
        assert_eq!(watcher_clamped_u64(&s.store, "ScanInterval", 6, 1, 3600), 12);
    }

    #[test]
    fn watcher_clamped_u64_clamps_above_max_and_below_min() {
        let s = TempSettings::new("clamp");
        s.store.set("Watcher", "ScanInterval", "999999").unwrap();
        assert_eq!(
            watcher_clamped_u64(&s.store, "ScanInterval", 6, 1, 3600),
            3600
        );

        s.store.set("Watcher", "ScanInterval", "-50").unwrap();
        assert_eq!(watcher_clamped_u64(&s.store, "ScanInterval", 6, 1, 3600), 1);

        // A negative value would underflow an unsigned cast if it were not
        // clamped first.
        s.store.set("Watcher", "MemoryLowValue", "-1").unwrap();
        assert_eq!(
            watcher_clamped_u64(&s.store, "MemoryLowValue", 200, 1, 16384),
            1
        );
    }

    #[test]
    fn watcher_clamped_u64_falls_back_to_the_default_and_clamps_it() {
        let s = TempSettings::new("default");
        assert_eq!(
            watcher_clamped_u64(&s.store, "NoSuchWatcherKey", 42, 1, 3600),
            42
        );
        // The default itself is clamped too.
        assert_eq!(
            watcher_clamped_u64(&s.store, "NoSuchWatcherKey", 99_999, 1, 3600),
            3600
        );
    }

    #[test]
    fn watcher_clamped_u64_falls_back_when_the_value_is_not_a_number() {
        let s = TempSettings::new("garbage");
        s.store.set("Watcher", "ScanInterval", "abc").unwrap();
        assert_eq!(watcher_clamped_u64(&s.store, "ScanInterval", 6, 1, 3600), 6);
    }

    // ---- watcher_due / watcher_remaining_ms ---------------------------------

    #[test]
    fn watcher_due_is_true_before_the_first_tick() {
        assert!(watcher_due(None, 60_000));
    }

    #[test]
    fn watcher_due_is_false_while_the_interval_has_not_elapsed() {
        assert!(!watcher_due(Some(Instant::now()), 60_000));
    }

    #[test]
    fn watcher_due_is_true_once_the_interval_elapsed() {
        let tick = Instant::now() - Duration::from_millis(500);
        assert!(watcher_due(Some(tick), 100));
        // A zero interval is always due.
        assert!(watcher_due(Some(Instant::now()), 0));
    }

    #[test]
    fn watcher_remaining_ms_is_zero_before_the_first_tick() {
        assert_eq!(watcher_remaining_ms(None, 60_000), 0);
    }

    #[test]
    fn watcher_remaining_ms_saturates_instead_of_underflowing() {
        let tick = Instant::now() - Duration::from_millis(500);
        assert_eq!(watcher_remaining_ms(Some(tick), 100), 0);
        assert_eq!(watcher_remaining_ms(Some(Instant::now()), 0), 0);
    }

    #[test]
    fn watcher_remaining_ms_counts_down_within_the_interval() {
        let remaining = watcher_remaining_ms(Some(Instant::now()), 60_000);
        assert!(remaining > 0 && remaining <= 60_000, "remaining={remaining}");
    }

    // ---- title heuristics ---------------------------------------------------

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_title_indicates_disconnect_matches_known_phrases() {
        // The caller lowercases the title before calling in.
        assert!(windows_title_indicates_disconnect("roblox - disconnected"));
        assert!(windows_title_indicates_disconnect("connection error"));
        assert!(windows_title_indicates_disconnect("lost connection"));
        assert!(windows_title_indicates_disconnect("no connection"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_title_indicates_disconnect_ignores_healthy_titles() {
        assert!(!windows_title_indicates_disconnect("roblox"));
        assert!(!windows_title_indicates_disconnect(""));
        assert!(!windows_title_indicates_disconnect("jailbreak"));
    }

    fn health(log_found: bool, dropped: bool) -> ClientHealthView {
        ClientHealthView {
            pid: 1,
            log_found,
            drop: dropped.then(|| ClientDrop {
                kind: DropKind::Disconnected,
                reason: Some(DropReason::ConnectionLost),
                code: Some(277),
                message: None,
                since_ms: 0,
            }),
        }
    }

    #[test]
    fn the_log_decides_the_connection_when_it_was_found() {
        // Título normal, log com queda: caiu.
        assert_eq!(client_connection_lost(Some(&health(true, true)), "roblox"), Some(true));
        // Título com "disconnected" no nome da conta, log sem queda: não caiu.
        assert_eq!(
            client_connection_lost(Some(&health(true, false)), "roblox - no connection bob"),
            Some(false)
        );
    }

    #[test]
    fn without_a_log_the_title_is_the_fallback() {
        assert_eq!(client_connection_lost(None, "roblox - disconnected"), Some(true));
        assert_eq!(client_connection_lost(Some(&health(false, false)), "roblox"), Some(false));
        assert_eq!(client_connection_lost(None, ""), None);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_title_indicates_beta_is_case_insensitive() {
        assert!(windows_title_indicates_beta("Roblox Beta"));
        assert!(windows_title_indicates_beta("ROBLOX BETA"));
        assert!(!windows_title_indicates_beta("Roblox"));
        assert!(!windows_title_indicates_beta(""));
    }
}

/// O Watcher fecha clientes sozinho (memoria baixa, sem conexao, beta, titulo
/// errado). Isso so virava toast, que some em 2,5 s: quem voltasse depois nao
/// tinha como saber por que a conta caiu. Cada evento tem que deixar linha no
/// console.
///
/// O teste le o proprio arquivo porque assim cobre tambem o ramo de macOS, que
/// nem compila nesta plataforma.
#[cfg(test)]
mod watcher_console_tests {
    const FONTE: &str = include_str!("watcher.rs");

    #[test]
    fn todo_evento_do_watcher_deixa_linha_no_console() {
        let linhas: Vec<&str> = FONTE.lines().collect();
        let mut vistos = 0;
        for (i, linha) in linhas.iter().enumerate() {
            if !linha.contains("app_handle.emit(") {
                continue;
            }
            let evento = linhas.get(i + 1).copied().unwrap_or("");
            if !evento.contains("\"roblox-") {
                continue;
            }
            vistos += 1;
            let janela = linhas[i..(i + 14).min(linhas.len())].join("\n");
            assert!(
                janela.contains("emit_launch_log("),
                "evento sem linha de console: {}",
                evento.trim()
            );
        }
        assert!(
            vistos >= 8,
            "esperava os eventos das duas plataformas, achei {vistos}"
        );
    }
}
