/// Captcha-safety floor. Roblox issues a "verify you're not a robot" challenge
/// when authentication-ticket redemptions from the same IP arrive too close
/// together, so never let the target spacing drop below this, regardless of
/// AccountJoinDelay / EnableMultiRbx.
const MIN_JOIN_GAP_SECS: u64 = 8;

/// Minimum residual gap between two consecutive account launches, even when the
/// account's own work (auth + PID wait) already ate the whole join delay.
const MIN_RESIDUAL_GAP_MS: u64 = 5000;

/// Human-readable launch target for the console log line.
fn launch_target_description(join_vip: bool, link_code: &str, job_id: &str) -> String {
    if join_vip || !link_code.trim().is_empty() {
        "servidor VIP/privado".to_string()
    } else if !job_id.trim().is_empty() {
        format!("servidor {}", job_id.trim())
    } else {
        "servidor público".to_string()
    }
}

/// Full isolation wipes the default Roblox install, so a launch on the default
/// install has to re-download the client first. A pinned catalog version lives
/// outside that folder and is therefore untouched.
fn isolation_wipes_install(isolation_mode: &str, version_id: Option<&str>) -> bool {
    isolation_mode.eq_ignore_ascii_case("Full") && version_id.is_none()
}

/// Old-join launches the exe directly from a known folder. That folder is about
/// to be wiped when isolation runs in Full mode on the default install, so the
/// protocol handler is used instead; a pinned version always uses old join.
fn resolve_use_old_join(
    isolation_wipes_install: bool,
    configured_old_join: bool,
    version_id: Option<&str>,
) -> bool {
    if isolation_wipes_install {
        false
    } else {
        configured_old_join || version_id.is_some()
    }
}

/// True when a client is already running on a *different* Roblox version than
/// the one about to launch. Concurrent multi-version clients are not supported.
fn has_version_conflict(
    running_keys: &HashSet<Option<String>>,
    target_version_id: &Option<String>,
) -> bool {
    running_keys.iter().any(|k| k != target_version_id)
}

/// How long to wait for the new client's PID. A Full-isolation launch has to
/// download Roblox again first, which is far slower than a normal start.
fn pid_wait_seconds(isolation_wipes_install: bool) -> u64 {
    if isolation_wipes_install {
        180
    } else {
        12
    }
}

/// Seconds to space multi-account launches by, never below the captcha floor.
fn effective_join_delay_seconds(configured: Option<i64>) -> u64 {
    let delay = configured.unwrap_or(8) as u64;
    delay.max(MIN_JOIN_GAP_SECS)
}

/// A small randomized tail on the inter-account gap, so launches are neither
/// back-to-back nor perfectly periodic.
fn launch_jitter_ms(subsec_millis: u32) -> u64 {
    300 + (subsec_millis as u64) % 1200
}

/// Gap before the next account: the configured delay measured from the *start*
/// of this account's launch, minus the time already spent, but never below
/// `MIN_RESIDUAL_GAP_MS`, plus jitter.
fn next_account_wait(
    delay_seconds: u64,
    elapsed: std::time::Duration,
    jitter_ms: u64,
) -> std::time::Duration {
    std::time::Duration::from_secs(delay_seconds)
        .saturating_sub(elapsed)
        .max(std::time::Duration::from_millis(MIN_RESIDUAL_GAP_MS))
        + std::time::Duration::from_millis(jitter_ms)
}

/// Picks a pseudo-random public server from the list (no RNG dependency).
fn shuffle_server_index(nanos: u128, server_count: usize) -> usize {
    (nanos as usize) % server_count
}

/// The saved window rectangle of an account, or `None` when any part is
/// missing or unparsable (a half-applied rectangle would misplace the window).
fn window_rect_from_fields(
    fields: &std::collections::HashMap<String, String>,
) -> Option<(i32, i32, i32, i32)> {
    let x = fields.get("Window_Position_X")?.parse::<i32>().ok()?;
    let y = fields.get("Window_Position_Y")?.parse::<i32>().ok()?;
    let w = fields.get("Window_Width")?.parse::<i32>().ok()?;
    let h = fields.get("Window_Height")?.parse::<i32>().ok()?;
    Some((x, y, w, h))
}

#[cfg(target_os = "windows")]
#[tauri::command]
async fn launch_roblox(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    settings: tauri::State<'_, SettingsStore>,
    versions: tauri::State<'_, data::versions::VersionsCatalogStore>,
    user_id: i64,
    place_id: i64,
    job_id: String,
    launch_data: String,
    follow_user: bool,
    join_vip: bool,
    link_code: String,
    shuffle_job: bool,
) -> Result<(), String> {
    use platform::windows;

    let target_desc = launch_target_description(join_vip, &link_code, &job_id);
    emit_launch_log(
        &app,
        user_id,
        "info",
        "start",
        format!("Iniciando launch — place {place_id} ({target_desc})"),
    );

    let is_teleport = settings.get_bool("Developer", "IsTeleport");
    let configured_old_join = settings.get_bool("Developer", "UseOldJoin");
    let auto_close_last_process = settings.get_bool("General", "AutoCloseLastProcess");
    let auto_close_multi_conflicts = settings.get_bool("General", "AutoCloseRobloxForMultiRbx");
    let start_minimized = settings.get_bool("General", "StartRobloxMinimized");

    let account_snapshot_for_version = state.get_all()?;
    let account_version_override = account_snapshot_for_version
        .iter()
        .find(|a| a.user_id == user_id)
        .and_then(|a| a.fields.get("RobloxVersion").cloned())
        .filter(|v| !v.trim().is_empty());

    let (resolved_base_path, resolved_version_id) =
        windows::resolve_roblox_install_path(account_version_override.as_deref(), &settings, &versions)?;
    let isolation_will_wipe_install = isolation_wipes_install(
        &settings.get_string("Isolation", "Mode"),
        resolved_version_id.as_deref(),
    );
    let use_old_join = resolve_use_old_join(
        isolation_will_wipe_install,
        configured_old_join,
        resolved_version_id.as_deref(),
    );

    if let Some(report) = run_pre_launch_isolation(&app, &settings).await? {
        let _ = app.emit("isolation-report", &report);
        emit_launch_log(&app, user_id, "info", "isolation", "Isolamento pré-launch aplicado");
        if windows::has_pending_fast_flags() {
            tokio::spawn(apply_pending_fast_flags_when_ready(
                std::time::Duration::from_secs(240),
            ));
        }
    }

    let tracker_check = windows::tracker();
    let _ = tracker_check.cleanup_dead_processes();
    let running_keys = tracker_check.running_version_keys();
    if has_version_conflict(&running_keys, &resolved_version_id) {
        return Err(
            "A Roblox client is already running on a different version. Close it before launching this account on a different Roblox version. Concurrent multi-version support is planned for a future update.".into(),
        );
    }

    let multi_rbx = settings.get_bool("General", "EnableMultiRbx");
    if multi_rbx {
        ensure_multi_roblox_enabled(auto_close_multi_conflicts).await?;
    } else {
        let _ = windows::disable_multi_roblox();
    }

    windows::refresh_production_version().await;
    patch_client_settings_for_launch(&settings, LaunchClientProfile::Normal);

    let tracker = windows::tracker();
    if auto_close_last_process && tracker.get_pid(user_id).is_some() {
        let closed = tracker.kill_for_user_graceful_async(user_id, 4500).await;
        if !closed {
            return Err("Previous Roblox instance did not close before relaunch".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }

    let mut resolved_launch = resolve_launch_job(&job_id, join_vip, &link_code);
    if follow_user {
        resolved_launch.join_vip = false;
        resolved_launch.link_code.clear();
    }

    let mut actual_job = resolved_launch.job_id.clone();
    if shuffle_job && !follow_user && actual_job.trim().is_empty() {
        if let Ok(response) = run_with_session_retry(state.inner(), user_id, |cookie| async move {
            api::roblox::get_servers(place_id, "Public", None, Some(&cookie)).await
        })
        .await
        {
            if !response.data.is_empty() {
                let idx = shuffle_server_index(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos(),
                    response.data.len(),
                );
                actual_job = response.data[idx].id.clone();
            }
        }
    }

    let browser_tracker_id = get_or_create_browser_tracker_id(&state, user_id)?;
    emit_launch_log(&app, user_id, "info", "auth", "Solicitando authentication ticket...");
    let ticket = match run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::auth::get_auth_ticket(&cookie).await
    })
    .await
    {
        Ok(t) => {
            emit_launch_log(&app, user_id, "success", "auth", "Authentication ticket obtido");
            t
        }
        Err(e) => {
            emit_launch_log(&app, user_id, "error", "auth", format!("Falha no auth ticket: {e}"));
            if is_moderated_error(&e) {
                mark_account_moderated(state.inner(), &app, user_id);
                emit_launch_log(&app, user_id, "warn", "moderated", "Conta movida para o grupo 'moderadas'");
            }
            return Err(e);
        }
    };
    let private_join = run_with_session_retry(state.inner(), user_id, |cookie| {
        let resolved_launch = resolved_launch.clone();
        async move { resolve_private_join(&cookie, place_id, &resolved_launch).await }
    })
    .await?;
    if private_join.use_private_join {
        emit_launch_log(&app, user_id, "info", "target", "Alvo resolvido: servidor privado/VIP");
    } else if !actual_job.trim().is_empty() {
        emit_launch_log(&app, user_id, "info", "target", format!("Alvo resolvido: {}", actual_job.trim()));
    }

    let pids_before = windows::get_roblox_pids();

    let pid_wait_secs = pid_wait_seconds(isolation_will_wipe_install);
    let pending_id = tracker.add_pending_launch(
        user_id,
        resolved_version_id.clone(),
        std::time::Duration::from_secs(pid_wait_secs + 30),
    );

    let spawn_result = if use_old_join {
        let base_path = if resolved_version_id.is_none() {
            windows::default_player_dir(&resolved_base_path).await
        } else {
            resolved_base_path.clone()
        };
        windows::launch_old_join_from(
            &base_path,
            &ticket,
            private_join.place_id,
            &actual_job,
            &launch_data,
            follow_user,
            private_join.use_private_join,
            &private_join.access_code,
            &private_join.link_code,
            is_teleport,
        )
    } else {
        let url = windows::build_launch_url(
            &ticket,
            private_join.place_id,
            &actual_job,
            &browser_tracker_id,
            &launch_data,
            follow_user,
            private_join.use_private_join,
            &private_join.access_code,
            &private_join.link_code,
            is_teleport,
        );
        windows::launch_url(&url).await
    };
    if let Err(err) = spawn_result {
        emit_launch_log(&app, user_id, "error", "spawn", format!("Falha ao abrir o cliente: {err}"));
        tracker.clear_pending_launch(pending_id);
        return Err(err);
    }

    let detected_pid =
        wait_for_new_roblox_pid(&pids_before, std::time::Duration::from_secs(pid_wait_secs)).await;
    if detected_pid.is_none() && !isolation_will_wipe_install {
        emit_launch_log(&app, user_id, "warn", "pid", "PID não detectado no tempo esperado");
        tracker.clear_pending_launch(pending_id);
    }
    if let Some(pid) = detected_pid {
        emit_launch_log(&app, user_id, "success", "pid", format!("Cliente iniciado (PID {pid})"));
        tracker.clear_pending_launch(pending_id);
        tracker.track_with_version(
            user_id,
            pid,
            browser_tracker_id.clone(),
            resolved_version_id.clone(),
        );
        if let Some(version_id) = resolved_version_id.as_deref() {
            if let Some((channel, hash)) = version_id.split_once(':') {
                versions.touch_launched(channel, hash);
            }
        }
        apply_windows_post_launch_profile(Some(&app), &settings, LaunchClientProfile::Normal, pid)
            .await;

        let accounts = state.get_all()?;
        if let Some(account) = accounts.iter().find(|a| a.user_id == user_id) {
            if let Some((x, y, w, h)) = window_rect_from_fields(&account.fields) {
                let target_pid = pid;
                tokio::spawn(async move {
                    for _ in 0..45 {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        if let Some(hwnd) = windows::find_main_window(target_pid) {
                            windows::set_window_position(hwnd, x, y, w, h);
                            break;
                        }
                    }
                });
            }
        }

        if start_minimized {
            let baseline = pids_before.clone();
            tokio::spawn(async move {
                minimize_new_roblox_windows(baseline, std::time::Duration::from_secs(14)).await;
            });
        }
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn launch_roblox(
    _app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    settings: tauri::State<'_, SettingsStore>,
    user_id: i64,
    place_id: i64,
    job_id: String,
    launch_data: String,
    follow_user: bool,
    join_vip: bool,
    link_code: String,
    shuffle_job: bool,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use platform::macos;

        let is_teleport = settings.get_bool("Developer", "IsTeleport");
        let use_old_join = settings.get_bool("Developer", "UseOldJoin");
        let auto_close_last_process = settings.get_bool("General", "AutoCloseLastProcess");

        let multi_rbx = settings.get_bool("General", "EnableMultiRbx");
        if multi_rbx {
            let enabled = macos::enable_multi_roblox()?;
            if !enabled {
                return Err(
                    "Failed to enable Multi Roblox. Close all Roblox processes and try again."
                        .into(),
                );
            }
        } else {
            let _ = macos::disable_multi_roblox();
        }

        patch_client_settings_for_launch(&settings, LaunchClientProfile::Normal);

        let tracker = macos::tracker();
        if auto_close_last_process && tracker.get_pid(user_id).is_some() {
            tracker.kill_for_user(user_id);
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        let mut resolved_launch = resolve_launch_job(&job_id, join_vip, &link_code);
        if follow_user {
            resolved_launch.join_vip = false;
            resolved_launch.link_code.clear();
        }

        let mut actual_job = resolved_launch.job_id.clone();
        if shuffle_job && !follow_user && actual_job.trim().is_empty() {
            if let Ok(response) =
                run_with_session_retry(state.inner(), user_id, |cookie| async move {
                    api::roblox::get_servers(place_id, "Public", None, Some(&cookie)).await
                })
                .await
            {
                if !response.data.is_empty() {
                    let idx = (std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos() as usize)
                        % response.data.len();
                    actual_job = response.data[idx].id.clone();
                }
            }
        }

        let browser_tracker_id = get_or_create_browser_tracker_id(&state, user_id)?;
        let ticket = run_with_session_retry(state.inner(), user_id, |cookie| async move {
            api::auth::get_auth_ticket(&cookie).await
        })
        .await?;
        let private_join = run_with_session_retry(state.inner(), user_id, |cookie| {
            let resolved_launch = resolved_launch.clone();
            async move { resolve_private_join(&cookie, place_id, &resolved_launch).await }
        })
        .await?;

        let pids_before = macos::get_roblox_pids();

        if use_old_join {
            macos::launch_old_join(
                &ticket,
                private_join.place_id,
                &actual_job,
                &launch_data,
                follow_user,
                private_join.use_private_join,
                &private_join.access_code,
                &private_join.link_code,
                is_teleport,
            )?;
        } else {
            let url = macos::build_launch_url(
                &ticket,
                private_join.place_id,
                &actual_job,
                &browser_tracker_id,
                &launch_data,
                follow_user,
                private_join.use_private_join,
                &private_join.access_code,
                &private_join.link_code,
                is_teleport,
            );
            macos::launch_url(&url)?;
        }

        if let Some(pid) =
            wait_for_new_roblox_pid(&pids_before, std::time::Duration::from_secs(12)).await
        {
            tracker.track(user_id, pid, browser_tracker_id);
        }

        return Ok(());
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = (
            state,
            settings,
            user_id,
            place_id,
            job_id,
            launch_data,
            follow_user,
            join_vip,
            link_code,
            shuffle_job,
        );
        Err("Launching is only supported on Windows and macOS".into())
    }
}

#[cfg(target_os = "windows")]
#[tauri::command]
async fn launch_multiple(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    settings: tauri::State<'_, SettingsStore>,
    versions: tauri::State<'_, data::versions::VersionsCatalogStore>,
    user_ids: Vec<i64>,
    place_id: i64,
    job_id: String,
    launch_data: String,
) -> Result<(), String> {
    use platform::windows;

    let delay = effective_join_delay_seconds(settings.get_int("General", "AccountJoinDelay"));
    let multi_rbx = settings.get_bool("General", "EnableMultiRbx");
    let async_join = settings.get_bool("General", "AsyncJoin");
    let is_teleport = settings.get_bool("Developer", "IsTeleport");
    let configured_old_join = settings.get_bool("Developer", "UseOldJoin");
    let auto_close_last_process = settings.get_bool("General", "AutoCloseLastProcess");
    let auto_close_multi_conflicts = settings.get_bool("General", "AutoCloseRobloxForMultiRbx");
    let start_minimized = settings.get_bool("General", "StartRobloxMinimized");
    let tracker = windows::tracker();
    tracker.reset_launch_cancelled();

    if let Some(report) = run_pre_launch_isolation(&app, &settings).await? {
        let _ = app.emit("isolation-report", &report);
        if windows::has_pending_fast_flags() {
            tokio::spawn(apply_pending_fast_flags_when_ready(
                std::time::Duration::from_secs(240),
            ));
        }
    }

    let accounts = state.get_all()?;

    for (i, &uid) in user_ids.iter().enumerate() {
        if tracker.is_launch_cancelled() {
            break;
        }

        let iter_start = std::time::Instant::now();

        let account = accounts.iter().find(|a| a.user_id == uid);
        // Always launch into the selected place/job. Per-account "saved game"
        // overrides were removed so every account joins exactly the game the
        // user picked (previously a saved SavedPlaceId/SavedJobId silently sent
        // some accounts to a different server).
        let acct_place = place_id;
        let acct_job = job_id.clone();
        let acct_version_override = account
            .and_then(|a| a.fields.get("RobloxVersion").cloned())
            .filter(|v| !v.trim().is_empty());

        let acct_target_desc = launch_target_description(false, "", &acct_job);
        emit_launch_log(
            &app,
            uid,
            "info",
            "start",
            format!(
                "Conta {}/{} — place {acct_place} ({acct_target_desc})",
                i + 1,
                user_ids.len()
            ),
        );

        let (acct_base_path, acct_version_id) = match windows::resolve_roblox_install_path(
            acct_version_override.as_deref(),
            &settings,
            &versions,
        ) {
            Ok(value) => value,
            Err(err) => {
                let _ = app.emit(
                    "launch-progress",
                    serde_json::json!({
                        "userId": uid,
                        "index": i,
                        "total": user_ids.len(),
                        "error": "version-resolve-failed",
                        "message": err,
                    }),
                );
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };
        let acct_isolation_wipes_install = isolation_wipes_install(
            &settings.get_string("Isolation", "Mode"),
            acct_version_id.as_deref(),
        );
        let acct_use_old_join = resolve_use_old_join(
            acct_isolation_wipes_install,
            configured_old_join,
            acct_version_id.as_deref(),
        );

        let _ = tracker.cleanup_dead_processes();
        let running_keys = tracker.running_version_keys();
        if has_version_conflict(&running_keys, &acct_version_id) {
            let _ = app.emit(
                "launch-progress",
                serde_json::json!({
                    "userId": uid,
                    "index": i,
                    "total": user_ids.len(),
                    "error": "version-conflict",
                }),
            );
            continue;
        }

        let _ = app.emit(
            "launch-progress",
            serde_json::json!({
                "userId": uid,
                "index": i,
                "total": user_ids.len(),
            }),
        );

        let resolved_launch = resolve_launch_job(&acct_job, false, "");

        if multi_rbx {
            ensure_multi_roblox_enabled(auto_close_multi_conflicts).await?;
        } else {
            let _ = windows::disable_multi_roblox();
        }

        windows::refresh_production_version().await;
        patch_client_settings_for_launch(&settings, LaunchClientProfile::Normal);

        if auto_close_last_process && tracker.get_pid(uid).is_some() {
            let closed = tracker.kill_for_user_graceful_async(uid, 4500).await;
            if !closed {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }

        let browser_tracker_id = get_or_create_browser_tracker_id(&state, uid)?;
        emit_launch_log(&app, uid, "info", "auth", "Solicitando authentication ticket...");
        let ticket = match run_with_session_retry(state.inner(), uid, |cookie| async move {
            api::auth::get_auth_ticket(&cookie).await
        })
        .await
        {
            Ok(t) => {
                emit_launch_log(&app, uid, "success", "auth", "Authentication ticket obtido");
                t
            }
            Err(e) => {
                emit_launch_log(&app, uid, "error", "auth", format!("Falha no auth ticket: {e}"));
                if is_moderated_error(&e) {
                    mark_account_moderated(state.inner(), &app, uid);
                    emit_launch_log(&app, uid, "warn", "moderated", "Conta movida para o grupo 'moderadas'");
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };
        let private_join = match run_with_session_retry(state.inner(), uid, |cookie| {
            let resolved_launch = resolved_launch.clone();
            async move { resolve_private_join(&cookie, acct_place, &resolved_launch).await }
        })
        .await
        {
            Ok(value) => value,
            Err(_) => {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        // "Close All Roblox" may have been clicked while this account was
        // fetching its ticket; don't spawn a client right after the kill.
        if tracker.is_launch_cancelled() {
            break;
        }

        let pids_before = windows::get_roblox_pids();

        let acct_pid_wait_secs = pid_wait_seconds(acct_isolation_wipes_install);
        let acct_pending_id = tracker.add_pending_launch(
            uid,
            acct_version_id.clone(),
            std::time::Duration::from_secs(acct_pid_wait_secs + 30),
        );

        let launch_result = if acct_use_old_join {
            let base_path = if acct_version_id.is_none() {
                windows::default_player_dir(&acct_base_path).await
            } else {
                acct_base_path.clone()
            };
            windows::launch_old_join_from(
                &base_path,
                &ticket,
                private_join.place_id,
                &resolved_launch.job_id,
                &launch_data,
                false,
                private_join.use_private_join,
                &private_join.access_code,
                &private_join.link_code,
                is_teleport,
            )
        } else {
            let url = windows::build_launch_url(
                &ticket,
                private_join.place_id,
                &resolved_launch.job_id,
                &browser_tracker_id,
                &launch_data,
                false,
                private_join.use_private_join,
                &private_join.access_code,
                &private_join.link_code,
                is_teleport,
            );
            windows::launch_url(&url).await
        };

        if let Err(err) = &launch_result {
            emit_launch_log(&app, uid, "error", "spawn", format!("Falha ao abrir o cliente: {err}"));
            tracker.clear_pending_launch(acct_pending_id);
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            continue;
        }

        let acct_detected_pid = wait_for_new_roblox_pid(
            &pids_before,
            std::time::Duration::from_secs(acct_pid_wait_secs),
        )
        .await;
        if acct_detected_pid.is_none() && !acct_isolation_wipes_install {
            emit_launch_log(&app, uid, "warn", "pid", "PID não detectado no tempo esperado");
            tracker.clear_pending_launch(acct_pending_id);
        }
        if let Some(pid) = acct_detected_pid {
            emit_launch_log(&app, uid, "success", "pid", format!("Cliente iniciado (PID {pid})"));
            tracker.clear_pending_launch(acct_pending_id);
            tracker.track_with_version(uid, pid, browser_tracker_id, acct_version_id.clone());
            if let Some(version_id) = acct_version_id.as_deref() {
                if let Some((channel, hash)) = version_id.split_once(':') {
                    versions.touch_launched(channel, hash);
                }
            }
            apply_windows_post_launch_profile(
                Some(&app),
                &settings,
                LaunchClientProfile::Normal,
                pid,
            )
            .await;
            if start_minimized {
                let baseline = pids_before.clone();
                tokio::spawn(async move {
                    minimize_new_roblox_windows(baseline, std::time::Duration::from_secs(14)).await;
                });
            }
        }

        if i < user_ids.len() - 1 {
            if async_join {
                tracker.reset_next_account();
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
                while !tracker.is_next_account() && !tracker.is_launch_cancelled() {
                    if std::time::Instant::now() > deadline {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            } else {
                // Space launches by `delay` measured from the START of this account's
                // launch — subtract the time already spent (auth + waiting for the PID)
                // instead of stacking another full delay on top of it.
                //
                // But always keep a minimum residual gap plus a little jitter
                // between consecutive accounts. If this account's own work
                // (auth + PID wait) already took >= `delay`, the naive
                // `target - elapsed` collapses to zero and the next
                // authentication-ticket request fires back-to-back, which is
                // what trips Roblox's captcha. A non-zero, slightly randomized
                // gap avoids both back-to-back requests and a perfectly
                // periodic cadence.
                let jitter_ms = launch_jitter_ms(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.subsec_millis())
                        .unwrap_or(0),
                );
                let wait = next_account_wait(delay, iter_start.elapsed(), jitter_ms);
                emit_launch_log(
                    &app,
                    uid,
                    "info",
                    "wait",
                    format!("Aguardando {}s antes da próxima conta (anti-captcha)", wait.as_secs()),
                );
                tokio::time::sleep(wait).await;
            }
        }
    }

    let _ = app.emit("launch-complete", serde_json::json!({}));
    Ok(())
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn launch_multiple(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    settings: tauri::State<'_, SettingsStore>,
    user_ids: Vec<i64>,
    place_id: i64,
    job_id: String,
    launch_data: String,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use platform::macos;

        let delay = settings.get_int("General", "AccountJoinDelay").unwrap_or(8) as u64;
        let multi_rbx = settings.get_bool("General", "EnableMultiRbx");
        let delay = if multi_rbx { delay.max(12) } else { delay };
        let async_join = settings.get_bool("General", "AsyncJoin");
        let is_teleport = settings.get_bool("Developer", "IsTeleport");
        let use_old_join = settings.get_bool("Developer", "UseOldJoin");
        let auto_close_last_process = settings.get_bool("General", "AutoCloseLastProcess");
        let tracker = macos::tracker();
        tracker.reset_launch_cancelled();

        for (i, &uid) in user_ids.iter().enumerate() {
            if tracker.is_launch_cancelled() {
                break;
            }

            // Always launch into the selected place/job (per-account saved-game
            // overrides removed — see the Windows path for rationale).
            let acct_place = place_id;
            let acct_job = job_id.clone();

            let _ = app.emit(
                "launch-progress",
                serde_json::json!({
                    "userId": uid,
                    "index": i,
                    "total": user_ids.len(),
                }),
            );

            let resolved_launch = resolve_launch_job(&acct_job, false, "");

            if multi_rbx {
                let enabled = macos::enable_multi_roblox()?;
                if !enabled {
                    return Err(
                        "Failed to enable Multi Roblox. Close all Roblox processes and try again."
                            .into(),
                    );
                }
            } else {
                let _ = macos::disable_multi_roblox();
            }

            patch_client_settings_for_launch(&settings, LaunchClientProfile::Normal);

            if auto_close_last_process && tracker.get_pid(uid).is_some() {
                tracker.kill_for_user(uid);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }

            let browser_tracker_id = get_or_create_browser_tracker_id(&state, uid)?;
            let ticket = match run_with_session_retry(state.inner(), uid, |cookie| async move {
                api::auth::get_auth_ticket(&cookie).await
            })
            .await
            {
                Ok(t) => t,
                Err(_) => {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    continue;
                }
            };
            let private_join =
                match run_with_session_retry(state.inner(), uid, |cookie| {
                    let resolved_launch = resolved_launch.clone();
                    async move { resolve_private_join(&cookie, acct_place, &resolved_launch).await }
                })
                .await
                {
                Ok(value) => value,
                Err(_) => {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    continue;
                }
            };

            let pids_before = macos::get_roblox_pids();

            let launch_result = if use_old_join {
                macos::launch_old_join(
                    &ticket,
                    private_join.place_id,
                    &resolved_launch.job_id,
                    &launch_data,
                    false,
                    private_join.use_private_join,
                    &private_join.access_code,
                    &private_join.link_code,
                    is_teleport,
                )
            } else {
                let url = macos::build_launch_url(
                    &ticket,
                    private_join.place_id,
                    &resolved_launch.job_id,
                    &browser_tracker_id,
                    &launch_data,
                    false,
                    private_join.use_private_join,
                    &private_join.access_code,
                    &private_join.link_code,
                    is_teleport,
                );
                macos::launch_url(&url)
            };

            if launch_result.is_err() {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }

            if let Some(pid) =
                wait_for_new_roblox_pid(&pids_before, std::time::Duration::from_secs(12)).await
            {
                tracker.track(uid, pid, browser_tracker_id);
            }

            if i < user_ids.len() - 1 {
                if async_join {
                    tracker.reset_next_account();
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
                    while !tracker.is_next_account() && !tracker.is_launch_cancelled() {
                        if std::time::Instant::now() > deadline {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                } else {
                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                }
            }
        }

        let _ = app.emit("launch-complete", serde_json::json!({}));
        return Ok(());
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = (app, state, settings, user_ids, place_id, job_id, launch_data);
        Err("Launching is only supported on Windows and macOS".into())
    }
}

#[tauri::command]
fn cancel_launch() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        platform::windows::tracker().cancel_launch();
    }
    #[cfg(target_os = "macos")]
    {
        platform::macos::tracker().cancel_launch();
    }
    Ok(())
}

#[tauri::command]
fn next_account() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        platform::windows::tracker().signal_next_account();
    }
    #[cfg(target_os = "macos")]
    {
        platform::macos::tracker().signal_next_account();
    }
    Ok(())
}

#[tauri::command]
fn cmd_kill_roblox(user_id: i64) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        return Ok(platform::windows::tracker().kill_for_user(user_id));
    }
    #[cfg(target_os = "macos")]
    {
        return Ok(platform::macos::tracker().kill_for_user(user_id));
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = user_id;
        Err("Not supported on this platform".into())
    }
}

#[tauri::command]
fn focus_roblox_window(user_id: i64) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        let tracker = platform::windows::tracker();
        let Some(pid) = tracker.get_pid(user_id) else {
            return Ok(false);
        };
        let Some(hwnd) = platform::windows::find_main_window(pid) else {
            return Ok(false);
        };
        return Ok(platform::windows::focus_window(hwnd));
    }
    #[cfg(target_os = "macos")]
    {
        let _ = user_id;
        return Ok(false);
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = user_id;
        Ok(false)
    }
}

#[derive(serde::Serialize)]
struct GridArrangeResult {
    arranged: usize,
    total: usize,
}

/// List physical monitors so the Console can offer them as grid targets.
#[tauri::command]
fn list_display_monitors() -> Result<serde_json::Value, String> {
    #[cfg(target_os = "windows")]
    {
        return Ok(serde_json::to_value(platform::windows::list_monitors())
            .unwrap_or_else(|_| serde_json::json!([])));
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(serde_json::json!([]))
    }
}

/// Arrange every open Roblox window into a grid across the selected monitors
/// (1-based indices; empty = all monitors).
#[tauri::command]
fn arrange_windows_grid(monitor_indices: Vec<usize>, gap: i32) -> Result<GridArrangeResult, String> {
    #[cfg(target_os = "windows")]
    {
        let (arranged, total) = platform::windows::arrange_roblox_grid(&monitor_indices, gap)?;
        return Ok(GridArrangeResult { arranged, total });
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (monitor_indices, gap);
        Err("Grid de janelas só é suportado no Windows.".to_string())
    }
}

#[tauri::command]
fn cmd_kill_all_roblox() -> Result<u32, String> {
    #[cfg(target_os = "windows")]
    {
        let killed = platform::windows::kill_all_roblox();
        let tracker = platform::windows::tracker();
        tracker.cancel_launch();
        let all = tracker.get_all();
        for p in all {
            tracker.untrack(p.user_id);
        }
        return Ok(killed);
    }
    #[cfg(target_os = "macos")]
    {
        let killed = platform::macos::kill_all_roblox();
        let tracker = platform::macos::tracker();
        tracker.cancel_launch();
        let all = tracker.get_all();
        for p in all {
            tracker.untrack(p.user_id);
        }
        return Ok(killed);
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        Err("Not supported on this platform".into())
    }
}

#[derive(serde::Serialize)]
struct RunningInstance {
    pid: u32,
    user_id: i64,
    browser_tracker_id: String,
}

#[tauri::command]
fn get_running_instances() -> Result<Vec<RunningInstance>, String> {
    #[cfg(target_os = "windows")]
    {
        return Ok(platform::windows::tracker()
            .get_all()
            .into_iter()
            .map(|p| RunningInstance {
                pid: p.pid,
                user_id: p.user_id,
                browser_tracker_id: p.browser_tracker_id,
            })
            .collect());
    }
    #[cfg(target_os = "macos")]
    {
        return Ok(platform::macos::tracker()
            .get_all()
            .into_iter()
            .map(|p| RunningInstance {
                pid: p.pid,
                user_id: p.user_id,
                browser_tracker_id: p.browser_tracker_id,
            })
            .collect());
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        Ok(Vec::new())
    }
}

#[tauri::command]
fn cmd_enable_multi_roblox() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        return platform::windows::enable_multi_roblox();
    }
    #[cfg(target_os = "macos")]
    {
        return platform::macos::enable_multi_roblox();
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        Err("Not supported on this platform".into())
    }
}

#[tauri::command]
fn cmd_disable_multi_roblox() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        return platform::windows::disable_multi_roblox();
    }
    #[cfg(target_os = "macos")]
    {
        return platform::macos::disable_multi_roblox();
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        Ok(())
    }
}

#[tauri::command]
fn cmd_get_roblox_path() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        return platform::windows::get_roblox_path();
    }
    #[cfg(target_os = "macos")]
    {
        return platform::macos::get_roblox_path();
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        Err("Not supported on this platform".into())
    }
}

#[tauri::command]
fn cmd_apply_fps_unlock(max_fps: u32) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        return platform::windows::apply_fps_unlock(max_fps);
    }
    #[cfg(target_os = "macos")]
    {
        return platform::macos::apply_fps_unlock(max_fps);
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = max_fps;
        Err("Not supported on this platform".into())
    }
}

#[cfg(test)]
mod launch_command_tests {
    use super::*;
    use std::time::Duration;

    fn version_keys(keys: &[Option<&str>]) -> HashSet<Option<String>> {
        keys.iter().map(|k| k.map(|v| v.to_string())).collect()
    }

    // ---- launch_target_description -----------------------------------------

    #[test]
    fn launch_target_description_names_a_public_server_when_nothing_is_set() {
        assert_eq!(launch_target_description(false, "", ""), "servidor público");
        assert_eq!(
            launch_target_description(false, "   ", "  \t "),
            "servidor público"
        );
    }

    #[test]
    fn launch_target_description_names_the_job_id_of_a_specific_server() {
        assert_eq!(
            launch_target_description(false, "", "  job-123  "),
            "servidor job-123"
        );
    }

    #[test]
    fn launch_target_description_prefers_vip_over_the_job_id() {
        assert_eq!(
            launch_target_description(true, "", "job-123"),
            "servidor VIP/privado"
        );
        assert_eq!(
            launch_target_description(false, "CODE", "job-123"),
            "servidor VIP/privado"
        );
    }

    #[test]
    fn launch_target_description_keeps_unicode_job_ids_intact() {
        assert_eq!(
            launch_target_description(false, "", "サーバー"),
            "servidor サーバー"
        );
    }

    // ---- isolation_wipes_install / resolve_use_old_join --------------------

    #[test]
    fn isolation_wipes_install_only_for_full_mode_on_the_default_install() {
        assert!(isolation_wipes_install("Full", None));
        assert!(isolation_wipes_install("full", None));
        assert!(isolation_wipes_install("FULL", None));
    }

    #[test]
    fn isolation_wipes_install_is_false_for_a_pinned_version() {
        // A catalog version lives outside the wiped folder.
        assert!(!isolation_wipes_install("Full", Some("LIVE:version-aaa")));
    }

    #[test]
    fn isolation_wipes_install_is_false_for_every_lighter_mode() {
        for mode in ["Off", "Light", "Medium", "", "   ", "fullish"] {
            assert!(!isolation_wipes_install(mode, None), "mode {mode}");
        }
    }

    #[test]
    fn resolve_use_old_join_never_uses_old_join_when_the_install_is_wiped() {
        // Regression guard: old join points at an exe path that Full isolation
        // is about to delete, so the protocol handler has to take over.
        assert!(!resolve_use_old_join(true, true, None));
        assert!(!resolve_use_old_join(true, false, None));
    }

    #[test]
    fn resolve_use_old_join_follows_the_setting_on_the_default_install() {
        assert!(resolve_use_old_join(false, true, None));
        assert!(!resolve_use_old_join(false, false, None));
    }

    #[test]
    fn resolve_use_old_join_is_forced_on_for_a_pinned_version() {
        // A pinned build is never reachable through the protocol handler.
        assert!(resolve_use_old_join(false, false, Some("LIVE:version-aaa")));
        assert!(resolve_use_old_join(false, true, Some("LIVE:version-aaa")));
    }

    // ---- has_version_conflict ----------------------------------------------

    #[test]
    fn has_version_conflict_is_false_when_nothing_is_running() {
        assert!(!has_version_conflict(&version_keys(&[]), &None));
        assert!(!has_version_conflict(
            &version_keys(&[]),
            &Some("LIVE:version-aaa".to_string())
        ));
    }

    #[test]
    fn has_version_conflict_is_false_when_every_client_matches_the_target() {
        assert!(!has_version_conflict(&version_keys(&[None]), &None));
        assert!(!has_version_conflict(
            &version_keys(&[Some("LIVE:version-aaa")]),
            &Some("LIVE:version-aaa".to_string())
        ));
    }

    #[test]
    fn has_version_conflict_catches_a_client_on_another_version() {
        assert!(has_version_conflict(
            &version_keys(&[Some("LIVE:version-bbb")]),
            &Some("LIVE:version-aaa".to_string())
        ));
        // Default install running, pinned version requested.
        assert!(has_version_conflict(
            &version_keys(&[None]),
            &Some("LIVE:version-aaa".to_string())
        ));
        // Pinned version running, default install requested.
        assert!(has_version_conflict(
            &version_keys(&[Some("LIVE:version-aaa")]),
            &None
        ));
    }

    #[test]
    fn has_version_conflict_catches_a_mixed_set_of_clients() {
        assert!(has_version_conflict(
            &version_keys(&[Some("LIVE:version-aaa"), None]),
            &Some("LIVE:version-aaa".to_string())
        ));
    }

    // ---- pid_wait_seconds ---------------------------------------------------

    #[test]
    fn pid_wait_seconds_allows_a_reinstall_when_isolation_wipes_the_client() {
        assert_eq!(pid_wait_seconds(true), 180);
        assert_eq!(pid_wait_seconds(false), 12);
    }

    // ---- effective_join_delay_seconds ---------------------------------------

    #[test]
    fn effective_join_delay_seconds_defaults_to_eight_seconds() {
        assert_eq!(effective_join_delay_seconds(None), 8);
        assert_eq!(effective_join_delay_seconds(Some(8)), 8);
    }

    #[test]
    fn effective_join_delay_seconds_enforces_the_captcha_floor() {
        // Regression guard: a short AccountJoinDelay must never push auth-ticket
        // redemptions closer together than the captcha floor.
        assert_eq!(effective_join_delay_seconds(Some(0)), MIN_JOIN_GAP_SECS);
        assert_eq!(effective_join_delay_seconds(Some(1)), MIN_JOIN_GAP_SECS);
        assert_eq!(effective_join_delay_seconds(Some(7)), MIN_JOIN_GAP_SECS);
        assert_eq!(MIN_JOIN_GAP_SECS, 8);
    }

    #[test]
    fn effective_join_delay_seconds_keeps_a_longer_configured_delay() {
        assert_eq!(effective_join_delay_seconds(Some(30)), 30);
        assert_eq!(effective_join_delay_seconds(Some(3600)), 3600);
    }

    #[test]
    fn effective_join_delay_seconds_turns_a_negative_setting_into_a_huge_delay() {
        // Documented quirk: the i64 -> u64 cast wraps, so a negative
        // AccountJoinDelay produces an effectively infinite gap rather than a
        // short one. It errs on the safe side for the captcha, but the UI
        // should never write a negative value here.
        assert_eq!(effective_join_delay_seconds(Some(-1)), u64::MAX);
    }

    // ---- launch_jitter_ms / next_account_wait -------------------------------

    #[test]
    fn launch_jitter_ms_stays_inside_300_to_1499_milliseconds() {
        for ms in [0_u32, 1, 499, 500, 999, 1199, 1200, 1400] {
            let jitter = launch_jitter_ms(ms);
            assert!(
                (300..=1499).contains(&jitter),
                "jitter {jitter} out of range for {ms}"
            );
        }
        assert_eq!(launch_jitter_ms(0), 300);
        assert_eq!(launch_jitter_ms(1199), 1499);
        assert_eq!(launch_jitter_ms(1200), 300);
    }

    #[test]
    fn next_account_wait_subtracts_the_time_already_spent() {
        let wait = next_account_wait(20, Duration::from_secs(8), 0);
        assert_eq!(wait, Duration::from_secs(12));
    }

    #[test]
    fn next_account_wait_keeps_a_minimum_residual_gap() {
        // Regression guard: when the account's own work ate the whole delay the
        // naive `target - elapsed` collapses to zero and the next auth-ticket
        // request fires back-to-back, which is what trips Roblox's captcha.
        let wait = next_account_wait(20, Duration::from_secs(20), 0);
        assert_eq!(wait, Duration::from_millis(MIN_RESIDUAL_GAP_MS));

        let wait = next_account_wait(20, Duration::from_secs(600), 0);
        assert_eq!(wait, Duration::from_millis(MIN_RESIDUAL_GAP_MS));
    }

    #[test]
    fn next_account_wait_always_adds_the_jitter_on_top() {
        let wait = next_account_wait(20, Duration::from_secs(8), 700);
        assert_eq!(wait, Duration::from_millis(12_700));

        let wait = next_account_wait(20, Duration::from_secs(60), 1499);
        assert_eq!(wait, Duration::from_millis(MIN_RESIDUAL_GAP_MS + 1499));
    }

    #[test]
    fn next_account_wait_is_never_shorter_than_the_residual_gap() {
        for delay in [0_u64, 1, 8, 20, 120] {
            for elapsed_s in [0_u64, 5, 20, 1000] {
                let wait = next_account_wait(delay, Duration::from_secs(elapsed_s), 300);
                assert!(
                    wait >= Duration::from_millis(MIN_RESIDUAL_GAP_MS),
                    "delay={delay} elapsed={elapsed_s} wait={wait:?}"
                );
            }
        }
    }

    // ---- shuffle_server_index ------------------------------------------------

    #[test]
    fn shuffle_server_index_stays_inside_the_server_list() {
        for nanos in [0_u128, 1, 12_345, u128::MAX] {
            for len in [1_usize, 2, 7, 100] {
                assert!(shuffle_server_index(nanos, len) < len);
            }
        }
    }

    #[test]
    fn shuffle_server_index_is_deterministic_for_a_given_timestamp() {
        assert_eq!(shuffle_server_index(10, 4), shuffle_server_index(10, 4));
        assert_eq!(shuffle_server_index(10, 4), 2);
        assert_eq!(shuffle_server_index(0, 5), 0);
    }

    #[test]
    fn shuffle_server_index_of_a_single_server_is_always_zero() {
        assert_eq!(shuffle_server_index(u128::MAX, 1), 0);
    }

    // ---- window_rect_from_fields --------------------------------------------

    fn rect_fields(x: &str, y: &str, w: &str, h: &str) -> std::collections::HashMap<String, String> {
        let mut fields = std::collections::HashMap::new();
        fields.insert("Window_Position_X".to_string(), x.to_string());
        fields.insert("Window_Position_Y".to_string(), y.to_string());
        fields.insert("Window_Width".to_string(), w.to_string());
        fields.insert("Window_Height".to_string(), h.to_string());
        fields
    }

    #[test]
    fn window_rect_from_fields_reads_a_complete_rectangle() {
        assert_eq!(
            window_rect_from_fields(&rect_fields("100", "200", "1280", "720")),
            Some((100, 200, 1280, 720))
        );
    }

    #[test]
    fn window_rect_from_fields_accepts_negative_positions_for_secondary_monitors() {
        assert_eq!(
            window_rect_from_fields(&rect_fields("-1920", "-50", "800", "600")),
            Some((-1920, -50, 800, 600))
        );
    }

    #[test]
    fn window_rect_from_fields_returns_none_when_a_field_is_missing() {
        let mut fields = rect_fields("1", "2", "3", "4");
        fields.remove("Window_Height");
        assert_eq!(window_rect_from_fields(&fields), None);

        assert_eq!(
            window_rect_from_fields(&std::collections::HashMap::new()),
            None
        );
    }

    #[test]
    fn window_rect_from_fields_returns_none_when_a_field_is_unparsable() {
        // A half-applied rectangle would move the window somewhere random.
        assert_eq!(window_rect_from_fields(&rect_fields("1", "2", "", "4")), None);
        assert_eq!(
            window_rect_from_fields(&rect_fields("1", "2", "1280.5", "4")),
            None
        );
        assert_eq!(
            window_rect_from_fields(&rect_fields("1", "2", "99999999999999999999", "4")),
            None
        );
        assert_eq!(
            window_rect_from_fields(&rect_fields("１", "2", "3", "4")),
            None
        );
    }

    #[test]
    fn window_rect_from_fields_ignores_unrelated_fields() {
        let mut fields = rect_fields("1", "2", "3", "4");
        fields.insert("Note".to_string(), "hello".to_string());
        assert_eq!(window_rect_from_fields(&fields), Some((1, 2, 3, 4)));
    }

    // ---- platform-independent commands --------------------------------------

    #[test]
    fn list_display_monitors_returns_a_json_array() {
        let monitors = list_display_monitors().expect("listing monitors should not fail");
        assert!(monitors.is_array(), "expected an array, got {monitors}");
    }

    #[test]
    fn get_running_instances_never_fails() {
        let instances = get_running_instances().expect("listing instances should not fail");
        // Whatever the machine state, the tracker must answer with a list.
        assert!(instances.len() < 10_000);
    }

    #[test]
    fn next_account_and_cancel_launch_are_idempotent_signals() {
        // Both only flip tracker flags; calling them with nothing running is a
        // no-op that must still succeed.
        assert!(next_account().is_ok());
        assert!(cancel_launch().is_ok());
        assert!(cancel_launch().is_ok());
    }

    #[test]
    fn cmd_kill_roblox_reports_false_for_an_untracked_account() {
        assert_eq!(cmd_kill_roblox(-987_654_321).unwrap(), false);
    }

    #[test]
    fn focus_roblox_window_reports_false_for_an_untracked_account() {
        assert_eq!(focus_roblox_window(-987_654_321).unwrap(), false);
    }

    #[test]
    fn grid_arrange_result_serializes_its_two_counters() {
        let json = serde_json::to_value(GridArrangeResult {
            arranged: 3,
            total: 5,
        })
        .unwrap();
        assert_eq!(json["arranged"], 3);
        assert_eq!(json["total"], 5);
    }

    #[test]
    fn running_instance_serializes_the_fields_the_ui_reads() {
        let json = serde_json::to_value(RunningInstance {
            pid: 42,
            user_id: 7,
            browser_tracker_id: "12345".to_string(),
        })
        .unwrap();
        assert_eq!(json["pid"], 42);
        assert_eq!(json["user_id"], 7);
        assert_eq!(json["browser_tracker_id"], "12345");
    }
}
