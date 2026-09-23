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

#[tauri::command]
pub fn import_theme_font_asset(path: String) -> Result<ThemeFontAssetImportResult, String> {
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

    let digest = sodiumoxide::crypto::hash::sha256::hash(&bytes);
    let hash_hex = to_hex(digest.as_ref());
    let file_name = format!("{}.{}", hash_hex, ext);

    let fonts_dir = get_theme_fonts_dir();
    fs::create_dir_all(&fonts_dir).map_err(|e| format!("Failed to create font dir: {}", e))?;
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
pub fn resolve_theme_font_asset(file: String) -> Result<String, String> {
    let name = Path::new(file.trim())
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Invalid font file name".to_string())?
        .to_string();
    if name.contains('/') || name.contains('\\') {
        return Err("Invalid font file name".to_string());
    }
    let path = get_theme_fonts_dir().join(&name);
    if !path.exists() {
        return Err(format!("Font asset not found: {}", name));
    }
    Ok(path.to_string_lossy().into_owned())
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
    state.import_preset_file(&path)
}

#[tauri::command]
pub fn export_theme_preset_file(name: String, theme: ThemeData) -> Result<String, String> {
    ThemePresetStore::export_preset_file(&name, theme)
}

#[cfg(test)]
mod theme_font_command_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn nanos() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    }

    /// `ext` is appended after the unique suffix so the probe really carries it.
    fn temp_file(stem: &str, ext: &str, bytes: &[u8]) -> PathBuf {
        let name = if ext.is_empty() {
            format!("ram-font-{stem}-{}", nanos())
        } else {
            format!("ram-font-{stem}-{}.{ext}", nanos())
        };
        let path = std::env::temp_dir().join(name);
        fs::write(&path, bytes).expect("write font probe");
        path
    }

    // ---- import_theme_font_asset: rejection paths (no side effects) ------------

    #[test]
    fn importing_a_font_rejects_missing_paths_directories_and_wrong_extensions() {
        let missing = std::env::temp_dir().join(format!("ram-font-missing-{}.ttf", nanos()));
        assert_eq!(
            import_theme_font_asset(missing.to_string_lossy().into_owned()).unwrap_err(),
            "Font file does not exist"
        );

        let dir = std::env::temp_dir().join(format!("ram-font-dir-{}.ttf", nanos()));
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            import_theme_font_asset(dir.to_string_lossy().into_owned()).unwrap_err(),
            "Font path is not a file"
        );
        let _ = fs::remove_dir_all(&dir);

        let wrong_ext = temp_file("wrong-ext", "exe", b"MZ");
        let err = import_theme_font_asset(wrong_ext.to_string_lossy().into_owned()).unwrap_err();
        assert!(err.starts_with("Unsupported font extension"), "{err}");
        let _ = fs::remove_file(&wrong_ext);

        // No extension at all is rejected the same way.
        let no_ext = temp_file("no-ext", "", b"data");
        assert!(import_theme_font_asset(no_ext.to_string_lossy().into_owned()).is_err());
        let _ = fs::remove_file(&no_ext);

        let empty = temp_file("empty", "woff2", b"");
        assert_eq!(
            import_theme_font_asset(empty.to_string_lossy().into_owned()).unwrap_err(),
            "Font file is empty"
        );
        let _ = fs::remove_file(&empty);
    }

    #[test]
    fn importing_a_font_is_content_addressed_and_idempotent() {
        let source = temp_file("MyFamily", "ttf", b"fake font bytes for hashing");
        let first = import_theme_font_asset(source.to_string_lossy().into_owned())
            .expect("first import");

        assert_eq!(first.suggested_family, {
            // The family is the trimmed file stem of the imported path.
            source.file_stem().and_then(|s| s.to_str()).unwrap().to_string()
        });
        assert!(first.file.ends_with(".ttf"), "{}", first.file);
        let hash_part = first.file.trim_end_matches(".ttf");
        assert_eq!(hash_part.len(), 64, "sha256 hex is 64 chars: {}", first.file);
        assert!(hash_part.chars().all(|c| c.is_ascii_hexdigit()));

        let stored = get_theme_fonts_dir().join(&first.file);
        assert!(stored.exists(), "{}", stored.display());
        assert_eq!(fs::read(&stored).unwrap(), b"fake font bytes for hashing");

        // Re-importing the same bytes reuses the same asset name.
        let again = import_theme_font_asset(source.to_string_lossy().into_owned())
            .expect("second import");
        assert_eq!(again.file, first.file);

        // The stored name is resolvable, with or without a leading directory.
        let resolved = resolve_theme_font_asset(first.file.clone()).expect("resolve");
        assert_eq!(PathBuf::from(&resolved), stored);
        assert_eq!(
            resolve_theme_font_asset(format!("  {}  ", first.file)).unwrap(),
            resolved,
            "the name is trimmed"
        );

        let _ = fs::remove_file(&stored);
        let _ = fs::remove_file(&source);
        let _ = fs::remove_dir(get_theme_fonts_dir());
    }

    // ---- resolve_theme_font_asset ---------------------------------------------

    #[test]
    fn resolving_a_font_asset_rejects_paths_and_unknown_names() {
        let err = resolve_theme_font_asset("no-such-font-asset.ttf".to_string()).unwrap_err();
        assert_eq!(err, "Font asset not found: no-such-font-asset.ttf");

        // Only the file name survives, so traversal attempts become a plain
        // "not found" for the last component.
        let err = resolve_theme_font_asset("../../etc/passwd".to_string()).unwrap_err();
        assert_eq!(err, "Font asset not found: passwd");

        let err = resolve_theme_font_asset("..\\..\\secrets.ttf".to_string()).unwrap_err();
        assert_eq!(err, "Font asset not found: secrets.ttf");

        // A name that has no final component at all is refused outright.
        assert_eq!(
            resolve_theme_font_asset("".to_string()).unwrap_err(),
            "Invalid font file name"
        );
        assert_eq!(
            resolve_theme_font_asset("   ".to_string()).unwrap_err(),
            "Invalid font file name"
        );
        assert_eq!(
            resolve_theme_font_asset("..".to_string()).unwrap_err(),
            "Invalid font file name"
        );
    }
}
