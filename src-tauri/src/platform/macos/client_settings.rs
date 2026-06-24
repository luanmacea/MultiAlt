fn get_client_settings_file() -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Some(home) = home_dir() {
        candidates.push(
            home.join("Library")
                .join("Application Support")
                .join("Roblox")
                .join("ClientSettings")
                .join("ClientAppSettings.json"),
        );
        candidates.push(
            home.join("Library")
                .join("Roblox")
                .join("ClientSettings")
                .join("ClientAppSettings.json"),
        );
    }

    if let Ok(app) = get_roblox_path() {
        let app_path = Path::new(&app);
        candidates.push(
            app_path
                .join("Contents")
                .join("MacOS")
                .join("ClientSettings")
                .join("ClientAppSettings.json"),
        );
        candidates.push(
            app_path
                .join("Contents")
                .join("Resources")
                .join("ClientSettings")
                .join("ClientAppSettings.json"),
        );
    }

    if let Some(existing) = candidates.iter().find(|p| p.exists()) {
        return Ok(existing.clone());
    }

    let fallback = candidates
        .into_iter()
        .next()
        .ok_or_else(|| "No macOS ClientSettings path candidates available".to_string())?;
    if let Some(parent) = fallback.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create ClientSettings folder: {}", e))?;
    }
    Ok(fallback)
}

pub fn apply_fps_unlock(max_fps: u32) -> Result<(), String> {
    let settings_file = get_client_settings_file()?;

    let mut settings: serde_json::Value = if settings_file.exists() {
        std::fs::read_to_string(&settings_file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    settings["DFIntTaskSchedulerTargetFps"] = serde_json::json!(max_fps);

    std::fs::write(
        &settings_file,
        serde_json::to_string(&settings).unwrap_or_default(),
    )
    .map_err(|e| format!("Failed to write ClientAppSettings.json: {}", e))
}

pub fn copy_custom_client_settings(custom_settings_path: &str) -> Result<(), String> {
    let custom_path = Path::new(custom_settings_path);
    if !custom_path.exists() {
        return Err("Custom ClientAppSettings.json path does not exist".into());
    }

    let content = std::fs::read_to_string(custom_path)
        .map_err(|e| format!("Failed to read custom settings file: {}", e))?;
    serde_json::from_str::<serde_json::Value>(&content)
        .map_err(|e| format!("Custom settings file is not valid JSON: {}", e))?;

    let settings_file = get_client_settings_file()?;
    std::fs::write(settings_file, content)
        .map_err(|e| format!("Failed to copy custom ClientAppSettings.json: {}", e))
}
