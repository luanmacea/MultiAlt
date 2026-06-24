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
fn windows_title_indicates_disconnect(title_lower: &str) -> bool {
    title_lower.contains("disconnected")
        || title_lower.contains("connection error")
        || title_lower.contains("lost connection")
        || title_lower.contains("no connection")
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
                }

                let instances = tracker.get_all();
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
                            disconnected_since.remove(&inst.user_id);
                            startup_seen.remove(&inst.user_id);
                            last_saved_positions.remove(&inst.user_id);
                            continue;
                        }
                    }

                    if cfg.exit_if_no_connection {
                        let lower_title = title.to_lowercase();
                        if !lower_title.is_empty() {
                            if windows_title_indicates_disconnect(&lower_title) {
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

        fn macos_line_indicates_disconnect(line_lower: &str) -> bool {
            line_lower.contains("sending disconnect with reason")
                || line_lower.contains("disconnected")
                || line_lower.contains("connection error")
                || line_lower.contains("lost connection")
                || line_lower.contains("no connection")
                || line_lower.contains("error code: 277")
        }

        fn macos_line_indicates_reconnect(line_lower: &str) -> bool {
            line_lower.contains("joining game")
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
                                    if macos_line_indicates_reconnect(&lower) {
                                        disconnected_since.remove(&inst.user_id);
                                    }
                                    if macos_line_indicates_disconnect(&lower) {
                                        disconnected_since
                                            .entry(inst.user_id)
                                            .or_insert_with(std::time::Instant::now);
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
