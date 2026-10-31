/// Catalog key for a Roblox build: `"<channel>:<hash>"`. Used as the account
/// override value, the default-version setting and the tracker's version key.
fn version_id_of(channel: &str, version_hash: &str) -> String {
    format!("{}:{}", channel, version_hash)
}

/// Empty channel means the production channel.
fn normalize_install_channel(channel: &str) -> String {
    if channel.trim().is_empty() {
        "LIVE".to_string()
    } else {
        channel.trim().to_string()
    }
}

/// The version hash is what identifies the build; without it there is nothing
/// to download.
fn normalize_install_version_hash(version_hash: &str) -> Result<String, String> {
    let trimmed = version_hash.trim().to_string();
    if trimmed.is_empty() {
        return Err("Version hash is required".into());
    }
    Ok(trimmed)
}

/// Blank install ids get a time-based one so two installs never share a folder.
fn normalize_install_id(install_id: &str, now: i64) -> String {
    if install_id.trim().is_empty() {
        format!("install-{}", now)
    } else {
        install_id.trim().to_string()
    }
}

/// True when a client is currently running on the version being uninstalled.
/// `None` entries are clients on the default install, which is never a catalog
/// version and therefore never blocks an uninstall.
fn version_is_running(running_keys: &HashSet<Option<String>>, version_id: &str) -> bool {
    running_keys.contains(&Some(version_id.to_string()))
}

/// The value stored in `Versions/DefaultVersion` (empty clears the default).
fn normalize_default_version(version_id: Option<String>) -> String {
    version_id.unwrap_or_default().trim().to_string()
}

/// `Some(id)` sets the account's `RobloxVersion` field, `None` removes it.
fn normalize_account_version_override(version_id: Option<String>) -> Option<String> {
    match version_id.map(|v| v.trim().to_string()) {
        Some(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

#[tauri::command]
fn versions_list_installed(
    catalog: tauri::State<'_, data::versions::VersionsCatalogStore>,
) -> Result<Vec<data::versions::VersionEntry>, String> {
    Ok(catalog.list())
}

#[tauri::command]
async fn versions_list_remote() -> Result<serde_json::Value, String> {
    #[cfg(target_os = "windows")]
    {
        let catalog = platform::windows::fetch_remote_catalog().await?;
        Ok(serde_json::json!({
            "current": catalog.current,
            "past": catalog.past,
            "pastError": catalog.past_error,
        }))
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Available on Windows only".into())
    }
}

#[tauri::command]
async fn versions_install(
    app: tauri::AppHandle,
    install_id: String,
    channel: String,
    version_hash: String,
    label: Option<String>,
) -> Result<data::versions::VersionEntry, String> {
    #[cfg(target_os = "windows")]
    {
        let channel_normalized = normalize_install_channel(&channel);
        let version_hash_normalized = normalize_install_version_hash(&version_hash)?;
        let install_id = normalize_install_id(&install_id, now_ms());
        platform::windows::install_version(
            app,
            install_id,
            channel_normalized,
            version_hash_normalized,
            label,
        )
        .await
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, install_id, channel, version_hash, label);
        Err("Available on Windows only".into())
    }
}

#[tauri::command]
fn versions_uninstall(
    catalog: tauri::State<'_, data::versions::VersionsCatalogStore>,
    settings: tauri::State<'_, SettingsStore>,
    accounts: tauri::State<'_, AccountStore>,
    channel: String,
    version_hash: String,
) -> Result<(), String> {
    let version_id = version_id_of(&channel, &version_hash);
    #[cfg(target_os = "windows")]
    {
        let tracker = platform::windows::tracker();
        let _ = tracker.cleanup_dead_processes();
        if version_is_running(&tracker.running_version_keys(), &version_id) {
            return Err(
                "Cannot uninstall: this Roblox version is currently running. Close all accounts using it first.".into(),
            );
        }
        platform::windows::uninstall_version(&channel, &version_hash)?;
    }
    catalog.remove(&channel, &version_hash)?;
    stop_using_version(&settings, &accounts, &version_id)
}

/// Tira `version_id` de onde ela foi escolhida — versão padrão e versão de
/// cada conta —, para os próximos launches abrirem a instalação normal. Usado
/// ao desinstalar e pelo "trocar para a versão atual" do aviso de versão velha.
pub(crate) fn stop_using_version(
    settings: &SettingsStore,
    accounts: &AccountStore,
    version_id: &str,
) -> Result<(), String> {
    if settings.get_string("Versions", "DefaultVersion") == version_id {
        let _ = settings.set("Versions", "DefaultVersion", "");
    }
    for mut account in accounts.get_all()? {
        if account.fields.get("RobloxVersion").is_some_and(|value| value == version_id) {
            account.fields.remove("RobloxVersion");
            accounts.update(account)?;
        }
    }
    Ok(())
}

#[tauri::command]
fn versions_set_default(
    settings: tauri::State<'_, SettingsStore>,
    version_id: Option<String>,
) -> Result<(), String> {
    let value = normalize_default_version(version_id);
    settings.set("Versions", "DefaultVersion", &value)
}

#[tauri::command]
fn versions_set_account_override(
    accounts: tauri::State<'_, AccountStore>,
    user_id: i64,
    version_id: Option<String>,
) -> Result<(), String> {
    let snapshot = accounts.get_all()?;
    let Some(mut account) = snapshot.into_iter().find(|a| a.user_id == user_id) else {
        return Err("Account not found".into());
    };
    match normalize_account_version_override(version_id) {
        Some(v) => {
            account.fields.insert("RobloxVersion".to_string(), v);
        }
        None => {
            account.fields.remove("RobloxVersion");
        }
    }
    accounts.update(account)?;
    Ok(())
}

#[tauri::command]
fn versions_set_label(
    catalog: tauri::State<'_, data::versions::VersionsCatalogStore>,
    channel: String,
    version_hash: String,
    label: Option<String>,
) -> Result<(), String> {
    catalog.set_label(&channel, &version_hash, label)
}

#[tauri::command]
fn versions_open_folder(
    catalog: tauri::State<'_, data::versions::VersionsCatalogStore>,
    channel: String,
    version_hash: String,
) -> Result<(), String> {
    let entry = catalog
        .find(&version_id_of(&channel, &version_hash))
        .ok_or("Version not found")?;
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&entry.install_path)
            .spawn()
            .map_err(|e| format!("Could not open folder: {}", e))?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = entry;
        Err("Available on Windows only".into())
    }
}
