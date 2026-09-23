#[cfg(target_os = "windows")]
fn isolation_options_from_settings(settings: &SettingsStore) -> platform::windows::IsolationOptions {
    use platform::windows::IsolationMode;

    let mode = IsolationMode::from_str(&settings.get_string("Isolation", "Mode"));
    platform::windows::IsolationOptions {
        mode,
        spoof_machine_guid: settings.get_bool("Isolation", "SpoofMachineGuid"),
        spoof_mac: settings.get_bool("Isolation", "SpoofMacAddress"),
        target_adapter: settings.get_string("Isolation", "TargetAdapter"),
        include_studio: settings.get_bool("Isolation", "IncludeStudio"),
        preserve_fast_flags: settings
            .get("Isolation", "PreserveFastFlags")
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(true),
        preserve_basic_settings: settings
            .get("Isolation", "PreserveBasicSettings")
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(true),
    }
}

#[cfg(target_os = "windows")]
fn persist_isolation_backups(
    settings: &SettingsStore,
    report: &platform::windows::IsolationReport,
) {
    if let Some(guid) = report.captured_machine_guid.as_deref() {
        let existing = settings.get_string("Isolation", "BackupMachineGuid");
        if existing.trim().is_empty() {
            let _ = settings.set("Isolation", "BackupMachineGuid", guid);
        }
    }
    if let Some(subkey) = report.captured_adapter_subkey.as_deref() {
        let existing_subkey = settings.get_string("Isolation", "BackupAdapterId");
        if existing_subkey.trim().is_empty() {
            let _ = settings.set("Isolation", "BackupAdapterId", subkey);
            let value = report.captured_network_address.clone().unwrap_or_default();
            let _ = settings.set("Isolation", "BackupNetworkAddress", &value);
        }
    }
}

#[cfg(target_os = "windows")]
pub(crate) async fn run_pre_launch_isolation(
    app: &tauri::AppHandle,
    settings: &SettingsStore,
) -> Result<Option<platform::windows::IsolationReport>, String> {
    let opts = isolation_options_from_settings(settings);
    if !opts.does_anything() {
        return Ok(None);
    }
    let app_owned = app.clone();
    let opts_owned = opts;
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        platform::windows::apply_pre_launch(&app_owned, &opts_owned)
    })
    .await
    .map_err(|e| format!("Isolation task panicked: {}", e))?;

    match outcome {
        Ok(report) => {
            persist_isolation_backups(settings, &report);
            Ok(Some(report))
        }
        Err(failure) => {
            persist_isolation_backups(settings, &failure.partial);
            Err(failure.message)
        }
    }
}

#[cfg(target_os = "windows")]
pub(crate) async fn apply_pending_fast_flags_when_ready(timeout: std::time::Duration) {
    let Some(local) = std::env::var_os("LOCALAPPDATA") else {
        return;
    };
    let versions_root = std::path::PathBuf::from(local).join("Roblox").join("Versions");
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if !platform::windows::has_pending_fast_flags() {
            return;
        }
        let mut best: Option<(std::path::PathBuf, std::time::SystemTime)> = None;
        if let Ok(entries) = std::fs::read_dir(&versions_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                if !path.join("RobloxPlayerBeta.exe").exists() {
                    continue;
                }
                let mtime = std::fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                if best.as_ref().map(|(_, t)| mtime > *t).unwrap_or(true) {
                    best = Some((path, mtime));
                }
            }
        }
        if let Some((path, _)) = best {
            if platform::windows::apply_pending_fast_flags_to(&path) {
                return;
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) async fn apply_pending_fast_flags_when_ready(_timeout: std::time::Duration) {}

#[tauri::command]
fn isolation_get_status(
    settings: tauri::State<'_, SettingsStore>,
) -> Result<serde_json::Value, String> {
    #[cfg(target_os = "windows")]
    {
        let opts = isolation_options_from_settings(&settings);
        let backup_guid = settings.get_string("Isolation", "BackupMachineGuid");
        let backup_mac = settings.get_string("Isolation", "BackupNetworkAddress");
        let backup_adapter = settings.get_string("Isolation", "BackupAdapterId");
        Ok(serde_json::json!({
            "platformSupported": true,
            "mode": opts.mode.as_str(),
            "spoofMachineGuid": opts.spoof_machine_guid,
            "spoofMac": opts.spoof_mac,
            "targetAdapter": opts.target_adapter,
            "includeStudio": opts.include_studio,
            "preserveFastFlags": opts.preserve_fast_flags,
            "preserveBasicSettings": opts.preserve_basic_settings,
            "backupMachineGuid": backup_guid,
            "backupNetworkAddress": backup_mac,
            "backupAdapterId": backup_adapter,
        }))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = settings;
        Ok(serde_json::json!({
            "platformSupported": false,
        }))
    }
}

/// Canonical `Isolation/Mode` value written to the INI. Anything unrecognised
/// (including an empty string) disables isolation rather than guessing.
fn normalize_isolation_mode(mode: &str) -> &'static str {
    match mode.trim().to_ascii_lowercase().as_str() {
        "light" => "Light",
        "medium" => "Medium",
        "full" => "Full",
        _ => "Off",
    }
}

#[tauri::command]
fn isolation_save(
    settings: tauri::State<'_, SettingsStore>,
    mode: String,
    spoof_machine_guid: bool,
    spoof_mac: bool,
    target_adapter: String,
    include_studio: bool,
    preserve_fast_flags: bool,
    preserve_basic_settings: bool,
) -> Result<(), String> {
    let mode_normalized = normalize_isolation_mode(&mode);
    settings.set("Isolation", "Mode", mode_normalized)?;
    settings.set(
        "Isolation",
        "SpoofMachineGuid",
        if spoof_machine_guid { "true" } else { "false" },
    )?;
    settings.set(
        "Isolation",
        "SpoofMacAddress",
        if spoof_mac { "true" } else { "false" },
    )?;
    settings.set("Isolation", "TargetAdapter", target_adapter.trim())?;
    settings.set(
        "Isolation",
        "IncludeStudio",
        if include_studio { "true" } else { "false" },
    )?;
    settings.set(
        "Isolation",
        "PreserveFastFlags",
        if preserve_fast_flags { "true" } else { "false" },
    )?;
    settings.set(
        "Isolation",
        "PreserveBasicSettings",
        if preserve_basic_settings { "true" } else { "false" },
    )?;
    Ok(())
}

#[tauri::command]
fn isolation_list_adapters() -> Result<Vec<serde_json::Value>, String> {
    #[cfg(target_os = "windows")]
    {
        let adapters = platform::windows::list_network_adapters()?;
        Ok(adapters
            .into_iter()
            .map(|a| {
                serde_json::json!({
                    "subkey": a.subkey,
                    "description": a.description,
                    "currentMac": a.current_mac,
                    "netCfgInstanceId": a.net_cfg_instance_id,
                })
            })
            .collect())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(Vec::new())
    }
}

#[tauri::command]
async fn isolation_restore_network_identifiers(
    settings: tauri::State<'_, SettingsStore>,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let backup_guid = settings.get_string("Isolation", "BackupMachineGuid");
        let backup_adapter = settings.get_string("Isolation", "BackupAdapterId");
        let backup_mac = settings.get_string("Isolation", "BackupNetworkAddress");

        let guid_opt = if backup_guid.trim().is_empty() {
            None
        } else {
            Some(backup_guid.clone())
        };
        let adapter_opt = if backup_adapter.trim().is_empty() {
            None
        } else {
            Some(backup_adapter.clone())
        };
        let mac_opt = if backup_mac.trim().is_empty() {
            None
        } else {
            Some(backup_mac.clone())
        };

        if guid_opt.is_none() && adapter_opt.is_none() {
            return Err("No backup values stored; nothing to restore".into());
        }

        tauri::async_runtime::spawn_blocking(move || {
            platform::windows::restore_network_identifiers(
                guid_opt.as_deref(),
                adapter_opt.as_deref(),
                mac_opt.as_deref(),
            )
        })
        .await
        .map_err(|e| format!("Restore task panicked: {}", e))??;

        let _ = settings.set("Isolation", "BackupMachineGuid", "");
        let _ = settings.set("Isolation", "BackupNetworkAddress", "");
        let _ = settings.set("Isolation", "BackupAdapterId", "");
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = settings;
        Err("Available on Windows only".into())
    }
}

#[tauri::command]
fn isolation_dry_run(
    mode: String,
    spoof_machine_guid: bool,
    spoof_mac: bool,
    target_adapter: String,
    include_studio: bool,
    preserve_fast_flags: bool,
    preserve_basic_settings: bool,
) -> Result<serde_json::Value, String> {
    #[cfg(target_os = "windows")]
    {
        let opts = platform::windows::IsolationOptions {
            mode: platform::windows::IsolationMode::from_str(&mode),
            spoof_machine_guid,
            spoof_mac,
            target_adapter,
            include_studio,
            preserve_fast_flags,
            preserve_basic_settings,
        };
        let report = platform::windows::dry_run(&opts)?;
        Ok(serde_json::json!({
            "paths": report.paths,
            "registryKeys": report.registry_keys,
        }))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (
            mode,
            spoof_machine_guid,
            spoof_mac,
            target_adapter,
            include_studio,
            preserve_fast_flags,
            preserve_basic_settings,
        );
        Err("Available on Windows only".into())
    }
}

#[cfg(test)]
mod isolation_command_tests {
    use super::*;

    #[allow(dead_code)]
    fn temp_settings(tag: &str) -> SettingsStore {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        SettingsStore::new(std::env::temp_dir().join(format!("ram-isolation-{tag}-{nanos}.ini")))
    }

    #[test]
    fn normalize_isolation_mode_canonicalizes_the_four_known_modes() {
        assert_eq!(normalize_isolation_mode("light"), "Light");
        assert_eq!(normalize_isolation_mode("medium"), "Medium");
        assert_eq!(normalize_isolation_mode("full"), "Full");
        assert_eq!(normalize_isolation_mode("off"), "Off");
    }

    #[test]
    fn normalize_isolation_mode_ignores_case_and_surrounding_space() {
        assert_eq!(normalize_isolation_mode("  FULL  "), "Full");
        assert_eq!(normalize_isolation_mode("MeDiUm"), "Medium");
        assert_eq!(normalize_isolation_mode("\tLight\n"), "Light");
    }

    #[test]
    fn normalize_isolation_mode_falls_back_to_off_for_anything_unknown() {
        // Fail closed: an unknown mode must never wipe more than the user asked.
        assert_eq!(normalize_isolation_mode(""), "Off");
        assert_eq!(normalize_isolation_mode("   "), "Off");
        assert_eq!(normalize_isolation_mode("nuclear"), "Off");
        assert_eq!(normalize_isolation_mode("ＦＵＬＬ"), "Off");
        assert_eq!(normalize_isolation_mode("full "), "Full");
        assert_eq!(normalize_isolation_mode("fullish"), "Off");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn normalize_isolation_mode_round_trips_through_isolation_mode_from_str() {
        use platform::windows::IsolationMode;
        for (raw, expected) in [
            ("light", "light"),
            ("medium", "medium"),
            ("full", "full"),
            ("off", "off"),
            ("garbage", "off"),
        ] {
            let stored = normalize_isolation_mode(raw);
            assert_eq!(
                IsolationMode::from_str(stored).as_str(),
                expected,
                "mode {raw} did not survive the INI round-trip"
            );
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn isolation_options_from_settings_uses_the_shipped_defaults() {
        let settings = temp_settings("defaults");
        let opts = isolation_options_from_settings(&settings);

        assert_eq!(opts.mode.as_str(), "off");
        assert!(!opts.spoof_machine_guid);
        assert!(!opts.spoof_mac);
        assert_eq!(opts.target_adapter, "");
        assert!(!opts.include_studio);
        // Both preserve flags default to true so a first run never drops the
        // user's fast flags / basic settings.
        assert!(opts.preserve_fast_flags);
        assert!(opts.preserve_basic_settings);
        assert!(!opts.does_anything());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn isolation_options_from_settings_maps_every_field() {
        let settings = temp_settings("mapped");
        settings.set("Isolation", "Mode", "Medium").unwrap();
        settings.set("Isolation", "SpoofMachineGuid", "true").unwrap();
        settings.set("Isolation", "SpoofMacAddress", "true").unwrap();
        settings.set("Isolation", "TargetAdapter", "0007").unwrap();
        settings.set("Isolation", "IncludeStudio", "true").unwrap();
        settings.set("Isolation", "PreserveFastFlags", "false").unwrap();
        settings.set("Isolation", "PreserveBasicSettings", "false").unwrap();

        let opts = isolation_options_from_settings(&settings);
        assert_eq!(opts.mode.as_str(), "medium");
        assert!(opts.spoof_machine_guid);
        assert!(opts.spoof_mac);
        assert_eq!(opts.target_adapter, "0007");
        assert!(opts.include_studio);
        assert!(!opts.preserve_fast_flags);
        assert!(!opts.preserve_basic_settings);
        assert!(opts.does_anything());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn isolation_options_from_settings_treats_non_true_values_as_false() {
        let settings = temp_settings("loose-bools");
        settings.set("Isolation", "SpoofMachineGuid", "TRUE").unwrap();
        settings.set("Isolation", "SpoofMacAddress", "1").unwrap();
        settings.set("Isolation", "IncludeStudio", "yes").unwrap();

        let opts = isolation_options_from_settings(&settings);
        assert!(
            !opts.spoof_machine_guid,
            "only the exact string 'true' enables a flag"
        );
        assert!(!opts.spoof_mac);
        assert!(!opts.include_studio);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn isolation_options_from_settings_defaults_preserve_flags_to_true_when_absent() {
        let untouched = temp_settings("untouched-preserve");
        let opts = isolation_options_from_settings(&untouched);
        assert!(opts.preserve_fast_flags);
        assert!(opts.preserve_basic_settings);

        // Writing an empty value deletes the INI key, so the flags fall back to
        // "preserve" rather than silently starting to wipe.
        let settings = temp_settings("empty-preserve");
        settings.set("Isolation", "PreserveFastFlags", "").unwrap();
        settings.set("Isolation", "PreserveBasicSettings", "").unwrap();
        assert_eq!(settings.get("Isolation", "PreserveFastFlags").unwrap(), None);
        let opts = isolation_options_from_settings(&settings);
        assert!(opts.preserve_fast_flags);
        assert!(opts.preserve_basic_settings);

        // Any stored value other than "true" means false.
        settings.set("Isolation", "PreserveFastFlags", "0").unwrap();
        settings.set("Isolation", "PreserveBasicSettings", "no").unwrap();
        let opts = isolation_options_from_settings(&settings);
        assert!(!opts.preserve_fast_flags);
        assert!(!opts.preserve_basic_settings);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn isolation_options_does_anything_only_when_something_is_enabled() {
        let settings = temp_settings("does-anything");
        assert!(!isolation_options_from_settings(&settings).does_anything());

        settings.set("Isolation", "SpoofMachineGuid", "true").unwrap();
        assert!(isolation_options_from_settings(&settings).does_anything());

        settings.set("Isolation", "SpoofMachineGuid", "false").unwrap();
        settings.set("Isolation", "SpoofMacAddress", "true").unwrap();
        assert!(isolation_options_from_settings(&settings).does_anything());

        settings.set("Isolation", "SpoofMacAddress", "false").unwrap();
        settings.set("Isolation", "Mode", "Light").unwrap();
        assert!(isolation_options_from_settings(&settings).does_anything());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn persist_isolation_backups_stores_the_first_captured_values() {
        let settings = temp_settings("backup-first");
        let report = platform::windows::IsolationReport {
            captured_machine_guid: Some("GUID-ORIGINAL".to_string()),
            captured_adapter_subkey: Some("0007".to_string()),
            captured_network_address: Some("AABBCCDDEEFF".to_string()),
            ..Default::default()
        };

        persist_isolation_backups(&settings, &report);

        assert_eq!(
            settings.get_string("Isolation", "BackupMachineGuid"),
            "GUID-ORIGINAL"
        );
        assert_eq!(settings.get_string("Isolation", "BackupAdapterId"), "0007");
        assert_eq!(
            settings.get_string("Isolation", "BackupNetworkAddress"),
            "AABBCCDDEEFF"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn persist_isolation_backups_never_overwrites_an_existing_backup() {
        // Regression guard: the backup is the only way back to the machine's
        // real identifiers, so a second launch must not overwrite it with the
        // spoofed values captured in between.
        let settings = temp_settings("backup-keep");
        let first = platform::windows::IsolationReport {
            captured_machine_guid: Some("GUID-ORIGINAL".to_string()),
            captured_adapter_subkey: Some("0007".to_string()),
            captured_network_address: Some("AABBCCDDEEFF".to_string()),
            ..Default::default()
        };
        persist_isolation_backups(&settings, &first);

        let second = platform::windows::IsolationReport {
            captured_machine_guid: Some("GUID-SPOOFED".to_string()),
            captured_adapter_subkey: Some("0009".to_string()),
            captured_network_address: Some("112233445566".to_string()),
            ..Default::default()
        };
        persist_isolation_backups(&settings, &second);

        assert_eq!(
            settings.get_string("Isolation", "BackupMachineGuid"),
            "GUID-ORIGINAL"
        );
        assert_eq!(settings.get_string("Isolation", "BackupAdapterId"), "0007");
        assert_eq!(
            settings.get_string("Isolation", "BackupNetworkAddress"),
            "AABBCCDDEEFF"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn persist_isolation_backups_ignores_a_report_that_captured_nothing() {
        let settings = temp_settings("backup-empty");
        persist_isolation_backups(&settings, &platform::windows::IsolationReport::default());

        assert_eq!(settings.get_string("Isolation", "BackupMachineGuid"), "");
        assert_eq!(settings.get_string("Isolation", "BackupAdapterId"), "");
        assert_eq!(settings.get_string("Isolation", "BackupNetworkAddress"), "");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn persist_isolation_backups_stores_an_empty_mac_when_the_adapter_had_none() {
        let settings = temp_settings("backup-no-mac");
        let report = platform::windows::IsolationReport {
            captured_adapter_subkey: Some("0011".to_string()),
            captured_network_address: None,
            ..Default::default()
        };
        persist_isolation_backups(&settings, &report);

        assert_eq!(settings.get_string("Isolation", "BackupAdapterId"), "0011");
        assert_eq!(settings.get_string("Isolation", "BackupNetworkAddress"), "");
    }

    #[tokio::test]
    async fn apply_pending_fast_flags_when_ready_returns_immediately_with_nothing_pending() {
        // Nothing was queued by this test process, so the loop must exit on its
        // first check instead of burning the whole timeout.
        let started = std::time::Instant::now();
        apply_pending_fast_flags_when_ready(std::time::Duration::from_secs(30)).await;
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }
}
