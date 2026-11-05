#[tauri::command]
pub fn get_all_settings(
    state: tauri::State<'_, SettingsStore>,
) -> Result<HashMap<String, HashMap<String, String>>, String> {
    state.get_all()
}

#[tauri::command]
pub fn get_setting(
    state: tauri::State<'_, SettingsStore>,
    section: String,
    key: String,
) -> Result<Option<String>, String> {
    state.get(&section, &key)
}

#[tauri::command]
pub fn update_setting(
    state: tauri::State<'_, SettingsStore>,
    section: String,
    key: String,
    value: String,
) -> Result<(), String> {
    // `Webhook.Url` só grava vazio ou um webhook do Discord (a tela confere
    // antes; isto é a segunda trava — ver docs/features/webhook.md).
    crate::api::discord_webhook::validate_setting(&section, &key, &value)?;
    state.set(&section, &key, &value)
}

#[tauri::command]
pub fn get_theme(state: tauri::State<'_, ThemeStore>) -> Result<ThemeData, String> {
    state.get()
}

#[tauri::command]
pub fn update_theme(state: tauri::State<'_, ThemeStore>, theme: ThemeData) -> Result<(), String> {
    state.update(theme)
}

/// Miolo de [`import_theme_font_asset`]: recebe a pasta de fontes já
/// resolvida em vez de chamar `get_theme_fonts_dir()` diretamente, para que
/// os testes gravem num `TempDir` em vez da pasta real do usuário.
fn import_theme_font_asset_in(
    fonts_dir: &Path,
    path: String,
) -> Result<ThemeFontAssetImportResult, String> {
    let source = PathBuf::from(path.trim());
    if !source.exists() {
        return Err("Font file does not exist".to_string());
    }
    if !source.is_file() {
        return Err("Font path is not a file".to_string());
    }

    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !is_allowed_font_ext(&ext) {
        return Err(
            "Unsupported font extension (supported: .ttf, .otf, .woff, .woff2)".to_string(),
        );
    }

    let bytes = fs::read(&source).map_err(|e| format!("Failed to read font file: {}", e))?;
    if bytes.is_empty() {
        return Err("Font file is empty".to_string());
    }

    let digest = <sha2::Sha256 as sha2::Digest>::digest(&bytes);
    let hash_hex = to_hex(digest.as_ref());
    let file_name = format!("{}.{}", hash_hex, ext);

    fs::create_dir_all(fonts_dir).map_err(|e| format!("Failed to create font dir: {}", e))?;
    let dest = fonts_dir.join(&file_name);
    if !dest.exists() {
        fs::write(&dest, &bytes).map_err(|e| format!("Failed to write font asset: {}", e))?;
    }

    Ok(ThemeFontAssetImportResult {
        file: file_name,
        suggested_family: sanitize_font_family_from_path(&source),
    })
}

#[tauri::command]
pub fn import_theme_font_asset(path: String) -> Result<ThemeFontAssetImportResult, String> {
    import_theme_font_asset_in(&get_theme_fonts_dir(), path)
}

/// Miolo de [`resolve_theme_font_asset`], veja [`import_theme_font_asset_in`].
fn resolve_theme_font_asset_in(fonts_dir: &Path, file: String) -> Result<String, String> {
    let name = font_file_base_name(&file)
        .ok_or_else(|| "Invalid font file name".to_string())?
        .to_string();
    if name.contains('/') || name.contains('\\') {
        return Err("Invalid font file name".to_string());
    }
    let path = fonts_dir.join(&name);
    if !path.exists() {
        return Err(format!("Font asset not found: {}", name));
    }
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn resolve_theme_font_asset(file: String) -> Result<String, String> {
    resolve_theme_font_asset_in(&get_theme_fonts_dir(), file)
}

#[tauri::command]
pub fn get_theme_presets(
    state: tauri::State<'_, ThemePresetStore>,
) -> Result<Vec<ThemePresetData>, String> {
    state.get_all()
}

#[tauri::command]
pub fn save_theme_preset(
    state: tauri::State<'_, ThemePresetStore>,
    name: String,
    theme: ThemeData,
) -> Result<ThemePresetData, String> {
    state.save_preset(&name, theme)
}

#[tauri::command]
pub fn delete_theme_preset(
    state: tauri::State<'_, ThemePresetStore>,
    preset_id: String,
) -> Result<(), String> {
    state.delete_preset(&preset_id)
}

#[tauri::command]
pub fn import_theme_preset_file(
    state: tauri::State<'_, ThemePresetStore>,
    path: String,
) -> Result<ThemePresetData, String> {
    state.import_preset_file(&path, &get_theme_fonts_dir())
}

#[tauri::command]
pub fn export_theme_preset_file(name: String, theme: ThemeData) -> Result<String, String> {
    ThemePresetStore::export_preset_file(&name, theme, &get_runtime_data_dir(), &get_theme_fonts_dir())
}

/// Cap for bytes accepted straight from the frontend (font upload or theme
/// preset upload). The by-path commands above trust a path the user already
/// has sitting on disk and impose no size limit; bytes arriving over IPC from
/// an `<input type="file">` picker have no such trust boundary, so both
/// bytes-based commands below enforce this cap themselves.
const MAX_THEME_UPLOAD_BYTES: usize = 20 * 1024 * 1024;

/// Miolo de [`import_theme_font_bytes`], veja [`import_theme_font_asset_in`].
/// Same validation and storage as [`import_theme_font_asset`], starting from
/// bytes handed over by the frontend (`<input type="file">`) instead of a
/// path on disk: allowed extension, non-empty, size-capped, content-addressed
/// storage under the given `fonts_dir`.
fn import_theme_font_bytes_in(
    fonts_dir: &Path,
    file_name: String,
    file_data: Vec<u8>,
) -> Result<ThemeFontAssetImportResult, String> {
    let name = font_file_base_name(&file_name)
        .ok_or_else(|| "Invalid font file name".to_string())?
        .to_string();
    let name_path = Path::new(&name);

    let ext = name_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !is_allowed_font_ext(&ext) {
        return Err(
            "Unsupported font extension (supported: .ttf, .otf, .woff, .woff2)".to_string(),
        );
    }

    if file_data.is_empty() {
        return Err("Font file is empty".to_string());
    }
    if file_data.len() > MAX_THEME_UPLOAD_BYTES {
        return Err(format!(
            "Font file is too large (max {} bytes)",
            MAX_THEME_UPLOAD_BYTES
        ));
    }

    let digest = <sha2::Sha256 as sha2::Digest>::digest(&file_data);
    let hash_hex = to_hex(digest.as_ref());
    let stored_name = format!("{}.{}", hash_hex, ext);

    fs::create_dir_all(fonts_dir).map_err(|e| format!("Failed to create font dir: {}", e))?;
    let dest = fonts_dir.join(&stored_name);
    if !dest.exists() {
        fs::write(&dest, &file_data).map_err(|e| format!("Failed to write font asset: {}", e))?;
    }

    Ok(ThemeFontAssetImportResult {
        file: stored_name,
        suggested_family: sanitize_font_family_from_path(name_path),
    })
}

#[tauri::command]
pub fn import_theme_font_bytes(
    file_name: String,
    file_data: Vec<u8>,
) -> Result<ThemeFontAssetImportResult, String> {
    import_theme_font_bytes_in(&get_theme_fonts_dir(), file_name, file_data)
}

/// Shared by [`import_theme_preset_bytes`] and its tests. Kept free of
/// `tauri::State` (which has no public constructor) so it can be exercised
/// directly against a plain `ThemePresetStore`.
///
/// `ThemePresetStore::import_preset_file` already does all the real work
/// (JSON vs. `.zip` bundle dispatch, zip-slip-safe font extraction, the
/// file-stem fallback for an unnamed preset) keyed off the path's extension
/// and file stem. Rather than reimplement any of that against raw bytes, the
/// uploaded bytes are staged under their original file name in a scratch
/// directory and handed to that exact function. `fonts_dir` is threaded
/// through (instead of `import_preset_file` resolving `get_theme_fonts_dir()`
/// itself) so tests can point a `.ram-theme.zip` bundle's font extraction at a
/// `TempDir` instead of the user's real font folder.
fn import_theme_preset_bytes_impl(
    store: &ThemePresetStore,
    file_name: &str,
    file_data: Vec<u8>,
    fonts_dir: &Path,
) -> Result<ThemePresetData, String> {
    let name = Path::new(file_name.trim())
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Invalid preset file name".to_string())?
        .to_string();

    let lower = name.to_ascii_lowercase();
    if !(lower.ends_with(".json") || lower.ends_with(".zip")) {
        return Err(
            "Unsupported preset extension (supported: .json, .ram-theme.json, or .ram-theme.zip)"
                .to_string(),
        );
    }

    if file_data.is_empty() {
        return Err("Preset file is empty".to_string());
    }
    if file_data.len() > MAX_THEME_UPLOAD_BYTES {
        return Err(format!(
            "Preset file is too large (max {} bytes)",
            MAX_THEME_UPLOAD_BYTES
        ));
    }

    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let scratch_dir = std::env::temp_dir().join(format!(
        "ram-theme-preset-upload-{}-{}",
        std::process::id(),
        unique
    ));
    fs::create_dir_all(&scratch_dir)
        .map_err(|e| format!("Failed to create scratch dir: {}", e))?;
    let staged_path = scratch_dir.join(&name);

    if let Err(e) = fs::write(&staged_path, &file_data) {
        let _ = fs::remove_dir(&scratch_dir);
        return Err(format!("Failed to stage preset file: {}", e));
    }

    let result = store.import_preset_file(&staged_path.to_string_lossy(), fonts_dir);

    let _ = fs::remove_file(&staged_path);
    let _ = fs::remove_dir(&scratch_dir);

    result
}

#[tauri::command]
pub fn import_theme_preset_bytes(
    state: tauri::State<'_, ThemePresetStore>,
    file_name: String,
    file_data: Vec<u8>,
) -> Result<ThemePresetData, String> {
    import_theme_preset_bytes_impl(&state, &file_name, file_data, &get_theme_fonts_dir())
}
