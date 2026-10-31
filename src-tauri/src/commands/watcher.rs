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
    close_if_not_responding: bool,
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
        close_if_not_responding: settings.get_bool("Watcher", "CloseIfNotResponding"),
        startup_grace_secs: 30,
    }
}

/// O Watcher só mexe nos clientes que o app abriu: a promessa da tela é que
/// ele ignora os clientes abertos fora do app. Um cliente aberto pelo site e
/// reconhecido pelo log (`external_clients.rs`) entra no rastreamento para a
/// Sessão e o Modo AFK, mas as regras que fecham cliente não valem para ele —
/// senão uma regra de memória ou de desconexão fecharia a conta que o usuário
/// está jogando pelo site.
fn only_launched_by_app<T>(instances: Vec<T>, adopted: impl Fn(&T) -> bool) -> Vec<T> {
    instances.into_iter().filter(|inst| !adopted(inst)).collect()
}

/// "Close If Memory Low" fecha este cliente? `memory_mb` é a memória privada
/// (a do Gerenciador de Tarefas, `get_process_memory_mb`). Um cliente que o
/// teto de memória liberou (`trimmed_by_ceiling`) fica de fora: depois de
/// liberar, a memória fica baixa até o jogo precisar de novo, e a regra
/// fecharia um cliente saudável.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn low_memory_should_close(memory_mb: u64, low_mb: u64, trimmed_by_ceiling: bool) -> bool {
    !trimmed_by_ceiling && memory_mb < low_mb
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
                    emit_launch_log(&app_handle, *uid, "warn", "watcher", "The Roblox client closed (the process died)");
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
                    // Queda, "Não respondendo" e o título que o app pôs (client_health.rs).
                    let health = client_health_of(inst.user_id, inst.pid);

                    // "Não respondendo" há 30 s (o monitor só conta clientes do app).
                    if cfg.close_if_not_responding
                        && health.as_ref().is_some_and(|h| h.not_responding)
                        && tracker.kill_for_user(inst.user_id)
                    {
                        let _ = app_handle.emit(
                            "roblox-not-responding",
                            serde_json::json!({
                                "userId": inst.user_id,
                                "seconds": HUNG_THRESHOLD_MS / 1000,
                            }),
                        );
                        // O console e o historico geral: evento do Watcher tambem vira linha la.
                        emit_launch_log(&app_handle, inst.user_id, "warn", "watcher", log_msg!("Client closed by the Watcher: it had not responded for {{seconds}}s", seconds = HUNG_THRESHOLD_MS / 1000));
                        disconnected_since.remove(&inst.user_id);
                        startup_seen.remove(&inst.user_id);
                        last_saved_positions.remove(&inst.user_id);
                        continue;
                    }

                    if cfg.memory_enabled && startup_grace_elapsed {
                        if let Some(mem) = windows::get_process_memory_mb(inst.pid) {
                            if low_memory_should_close(
                                mem,
                                cfg.memory_low_mb,
                                client_memory_was_trimmed(inst.user_id),
                            ) && tracker.kill_for_user(inst.user_id)
                            {
                                let _ = app_handle.emit(
                                    "roblox-low-memory",
                                    serde_json::json!({
                                        "userId": inst.user_id,
                                        "memoryMb": mem,
                                    }),
                                );
                                // O console e o historico geral: evento do Watcher tambem vira linha la.
                                emit_launch_log(&app_handle, inst.user_id, "warn", "watcher", log_msg!("Client closed by the Watcher: memory at {{mb}} MB, below the minimum", mb = mem));
                                notify_webhook(
                                    &app_handle,
                                    WebhookEventKind::MemoryClosed,
                                    inst.user_id,
                                    Some(WebhookReason::MemoryBelowMinimum { memory_mb: mem, min_mb: cfg.memory_low_mb }),
                                    webhook_place_of(inst.user_id),
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
                        // O nome da conta que o app pôs no título (client_health.rs)
                        // não conta para as regras: elas veem "Roblox".
                        let raw = windows::get_window_title(hwnd);
                        let applied = health.as_ref().and_then(|h| h.window_title.as_deref());
                        effective_client_title(&raw, applied, None).to_string()
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
                            emit_launch_log(&app_handle, inst.user_id, "warn", "watcher", log_msg!("Client closed by the Watcher: title \"{{title}}\" is not the expected one", title = title.clone()));
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
                            emit_launch_log(&app_handle, inst.user_id, "warn", "watcher", "Client closed by the Watcher: Roblox Beta detected");
                            disconnected_since.remove(&inst.user_id);
                            startup_seen.remove(&inst.user_id);
                            last_saved_positions.remove(&inst.user_id);
                            continue;
                        }
                    }

                    if cfg.exit_if_no_connection {
                        let lower_title = title.to_lowercase();
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
                                    emit_launch_log(&app_handle, inst.user_id, "warn", "watcher", log_msg!("Client closed by the Watcher: no connection for {{seconds}}s", seconds = cfg.no_connection_timeout_secs));
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
                        // Janela minimizada reporta (-32000, -32000, ~160x28): não
                        // é a posição da conta, e gravá-la estragava o launch
                        // seguinte (`window_rect_is_usable`).
                        if let Some(position) = windows::get_window_position(hwnd)
                            .filter(|p| window_rect_is_usable(p.0, p.1, p.2, p.3))
                        {
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
                        emit_launch_log(&app_handle, *uid, "warn", "watcher", "The Roblox client closed (the process died)");
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
                                emit_launch_log(&app_handle, inst.user_id, "warn", "watcher", "Client closed by the Watcher: Roblox Beta detected");
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
                                    emit_launch_log(&app_handle, inst.user_id, "warn", "watcher", log_msg!("Client closed by the Watcher: no connection for {{seconds}}s", seconds = cfg.no_connection_timeout_secs));
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
