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

    let digest = sodiumoxide::crypto::hash::sha256::hash(&bytes);
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
    let name = Path::new(file.trim())
        .file_name()
        .and_then(|n| n.to_str())
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
    let name = Path::new(file_name.trim())
        .file_name()
        .and_then(|n| n.to_str())
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

    let digest = sodiumoxide::crypto::hash::sha256::hash(&file_data);
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

#[cfg(test)]
mod theme_font_command_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Nenhum teste deste módulo pode chamar `get_theme_fonts_dir()`, que
    // resolve a pasta real de fontes do usuário (`%LOCALAPPDATA%\Roblox
    // Account Manager\RAMThemeFonts`): todo `import_theme_font_asset_in` /
    // `resolve_theme_font_asset_in` / `import_theme_font_bytes_in` aqui recebe
    // um `TempDir` próprio, único por teste, em vez da pasta real — é também
    // o que tira a disputa pela mesma pasta que deixava este módulo flaky
    // quando os testes rodavam em paralelo.

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

    /// Pasta temporária própria (sem dependência de `tempfile`), usada como
    /// `fonts_dir` de teste — nunca a pasta real do usuário.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("ram-font-cmd-dir-{tag}-{}-{n}", nanos()));
            fs::create_dir_all(&dir).expect("temp dir");
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    // ---- import_theme_font_asset: rejection paths (no side effects) ------------

    #[test]
    fn importing_a_font_rejects_missing_paths_directories_and_wrong_extensions() {
        let fonts = TempDir::new("reject");

        let missing = std::env::temp_dir().join(format!("ram-font-missing-{}.ttf", nanos()));
        assert_eq!(
            import_theme_font_asset_in(fonts.path(), missing.to_string_lossy().into_owned())
                .unwrap_err(),
            "Font file does not exist"
        );

        let dir = std::env::temp_dir().join(format!("ram-font-dir-{}.ttf", nanos()));
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            import_theme_font_asset_in(fonts.path(), dir.to_string_lossy().into_owned())
                .unwrap_err(),
            "Font path is not a file"
        );
        let _ = fs::remove_dir_all(&dir);

        let wrong_ext = temp_file("wrong-ext", "exe", b"MZ");
        let err = import_theme_font_asset_in(fonts.path(), wrong_ext.to_string_lossy().into_owned())
            .unwrap_err();
        assert!(err.starts_with("Unsupported font extension"), "{err}");
        let _ = fs::remove_file(&wrong_ext);

        // No extension at all is rejected the same way.
        let no_ext = temp_file("no-ext", "", b"data");
        assert!(
            import_theme_font_asset_in(fonts.path(), no_ext.to_string_lossy().into_owned())
                .is_err()
        );
        let _ = fs::remove_file(&no_ext);

        let empty = temp_file("empty", "woff2", b"");
        assert_eq!(
            import_theme_font_asset_in(fonts.path(), empty.to_string_lossy().into_owned())
                .unwrap_err(),
            "Font file is empty"
        );
        let _ = fs::remove_file(&empty);
    }

    #[test]
    fn importing_a_font_is_content_addressed_and_idempotent() {
        let fonts = TempDir::new("idempotent");
        let source = temp_file("MyFamily", "ttf", b"fake font bytes for hashing");
        let first = import_theme_font_asset_in(fonts.path(), source.to_string_lossy().into_owned())
            .expect("first import");

        assert_eq!(first.suggested_family, {
            // The family is the trimmed file stem of the imported path.
            source.file_stem().and_then(|s| s.to_str()).unwrap().to_string()
        });
        assert!(first.file.ends_with(".ttf"), "{}", first.file);
        let hash_part = first.file.trim_end_matches(".ttf");
        assert_eq!(hash_part.len(), 64, "sha256 hex is 64 chars: {}", first.file);
        assert!(hash_part.chars().all(|c| c.is_ascii_hexdigit()));

        let stored = fonts.path().join(&first.file);
        assert!(stored.exists(), "{}", stored.display());
        assert_eq!(fs::read(&stored).unwrap(), b"fake font bytes for hashing");

        // Re-importing the same bytes reuses the same asset name.
        let again = import_theme_font_asset_in(fonts.path(), source.to_string_lossy().into_owned())
            .expect("second import");
        assert_eq!(again.file, first.file);

        // The stored name is resolvable, with or without a leading directory.
        let resolved = resolve_theme_font_asset_in(fonts.path(), first.file.clone()).expect("resolve");
        assert_eq!(PathBuf::from(&resolved), stored);
        assert_eq!(
            resolve_theme_font_asset_in(fonts.path(), format!("  {}  ", first.file)).unwrap(),
            resolved,
            "the name is trimmed"
        );

        let _ = fs::remove_file(&source);
    }

    // ---- resolve_theme_font_asset ---------------------------------------------

    #[test]
    fn resolving_a_font_asset_rejects_paths_and_unknown_names() {
        let fonts = TempDir::new("resolve-reject");

        let err = resolve_theme_font_asset_in(fonts.path(), "no-such-font-asset.ttf".to_string())
            .unwrap_err();
        assert_eq!(err, "Font asset not found: no-such-font-asset.ttf");

        // Only the file name survives, so traversal attempts become a plain
        // "not found" for the last component.
        let err = resolve_theme_font_asset_in(fonts.path(), "../../etc/passwd".to_string())
            .unwrap_err();
        assert_eq!(err, "Font asset not found: passwd");

        let err = resolve_theme_font_asset_in(fonts.path(), "..\\..\\secrets.ttf".to_string())
            .unwrap_err();
        assert_eq!(err, "Font asset not found: secrets.ttf");

        // A name that has no final component at all is refused outright.
        assert_eq!(
            resolve_theme_font_asset_in(fonts.path(), "".to_string()).unwrap_err(),
            "Invalid font file name"
        );
        assert_eq!(
            resolve_theme_font_asset_in(fonts.path(), "   ".to_string()).unwrap_err(),
            "Invalid font file name"
        );
        assert_eq!(
            resolve_theme_font_asset_in(fonts.path(), "..".to_string()).unwrap_err(),
            "Invalid font file name"
        );
    }

    // ---- import_theme_font_bytes: rejection paths (no side effects) -----------

    #[test]
    fn importing_font_bytes_rejects_wrong_extension_empty_and_oversized_payloads() {
        let fonts = TempDir::new("bytes-reject");

        let wrong_ext =
            import_theme_font_bytes_in(fonts.path(), "font.exe".to_string(), b"MZ".to_vec())
                .unwrap_err();
        assert!(wrong_ext.starts_with("Unsupported font extension"), "{wrong_ext}");

        // No extension at all is rejected the same way.
        assert!(import_theme_font_bytes_in(
            fonts.path(),
            "no-extension".to_string(),
            b"data".to_vec()
        )
        .is_err());

        let empty =
            import_theme_font_bytes_in(fonts.path(), "empty.woff2".to_string(), Vec::new())
                .unwrap_err();
        assert_eq!(empty, "Font file is empty");

        let oversized = vec![0u8; MAX_THEME_UPLOAD_BYTES + 1];
        let err =
            import_theme_font_bytes_in(fonts.path(), "huge.ttf".to_string(), oversized).unwrap_err();
        assert!(err.starts_with("Font file is too large"), "{err}");

        // Right at the cap is still accepted; content-address it away again.
        let at_cap = vec![7u8; MAX_THEME_UPLOAD_BYTES];
        let ok = import_theme_font_bytes_in(fonts.path(), "at-cap.ttf".to_string(), at_cap)
            .expect("at the cap");
        let _ = fs::remove_file(fonts.path().join(&ok.file));
    }

    #[test]
    fn importing_font_bytes_rejects_a_blank_file_name() {
        let fonts = TempDir::new("bytes-blank-name");

        assert_eq!(
            import_theme_font_bytes_in(fonts.path(), "".to_string(), b"data".to_vec())
                .unwrap_err(),
            "Invalid font file name"
        );
        assert_eq!(
            import_theme_font_bytes_in(fonts.path(), "   ".to_string(), b"data".to_vec())
                .unwrap_err(),
            "Invalid font file name"
        );
    }

    #[test]
    fn importing_font_bytes_is_content_addressed_and_uses_only_the_base_name() {
        let fonts = TempDir::new("bytes-content-addressed");
        let bytes = b"fake font bytes uploaded from the browser".to_vec();
        let result = import_theme_font_bytes_in(
            fonts.path(),
            "My Family.ttf".to_string(),
            bytes.clone(),
        )
        .expect("import");

        assert_eq!(result.suggested_family, "My Family");
        assert!(result.file.ends_with(".ttf"), "{}", result.file);
        let hash_part = result.file.trim_end_matches(".ttf");
        assert_eq!(hash_part.len(), 64, "sha256 hex is 64 chars: {}", result.file);
        assert!(hash_part.chars().all(|c| c.is_ascii_hexdigit()));

        let stored = fonts.path().join(&result.file);
        assert!(stored.exists(), "{}", stored.display());
        assert_eq!(fs::read(&stored).unwrap(), bytes);

        // Even if the browser somehow sent a path, only the base name is used
        // for the family suggestion, and the same bytes hash to the same file.
        let with_path = import_theme_font_bytes_in(
            fonts.path(),
            "C:\\fake\\My Family.ttf".to_string(),
            bytes,
        )
        .expect("import with embedded path");
        assert_eq!(with_path.file, result.file, "same bytes, same content hash");
        assert_eq!(with_path.suggested_family, "My Family");
    }
}

#[cfg(test)]
mod theme_preset_bytes_command_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Nenhum teste deste módulo pode chamar `get_theme_fonts_dir()`: o import
    // de um bundle `.zip` extrai fontes locais, e isso teria que ir para um
    // `TempDir` próprio, nunca para a pasta real do usuário.

    fn nanos() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ram-preset-bytes-{name}-{}.json", nanos()))
    }

    /// Pasta temporária própria (sem dependência de `tempfile`), usada como
    /// `fonts_dir` de teste — nunca a pasta real do usuário.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("ram-preset-bytes-dir-{tag}-{}-{n}", nanos()));
            fs::create_dir_all(&dir).expect("temp dir");
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    struct TestStore {
        store: ThemePresetStore,
    }

    impl Drop for TestStore {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.store.file_path);
        }
    }

    impl std::ops::Deref for TestStore {
        type Target = ThemePresetStore;
        fn deref(&self) -> &ThemePresetStore {
            &self.store
        }
    }

    fn store(name: &str) -> TestStore {
        TestStore {
            store: ThemePresetStore::new(temp_path(name)),
        }
    }

    fn valid_json_bytes(name: &str, color: &str) -> Vec<u8> {
        let payload = serde_json::json!({
            "format": "ram-theme-preset-v1",
            "name": name,
            "theme": serde_json::to_value({
                let mut theme = ThemeData::default();
                theme.accounts_background = color.to_string();
                theme
            })
            .unwrap(),
        });
        serde_json::to_vec_pretty(&payload).unwrap()
    }

    fn zip_bundle_bytes(name: &str) -> Vec<u8> {
        let manifest = serde_json::json!({
            "format": "ram-theme-bundle-v1",
            "name": name,
            "theme": serde_json::to_value(ThemeData::default()).unwrap(),
        });
        let mut buf: Vec<u8> = Vec::new();
        {
            let cursor = std::io::Cursor::new(&mut buf);
            let mut writer = ZipWriter::new(cursor);
            let opts = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            writer.start_file("theme.json", opts).unwrap();
            writer
                .write_all(serde_json::to_vec(&manifest).unwrap().as_slice())
                .unwrap();
            writer.finish().unwrap();
        }
        buf
    }

    // ---- rejection paths (no side effects) -------------------------------------

    #[test]
    fn importing_preset_bytes_rejects_an_unsupported_extension() {
        let s = store("wrong-ext");
        let fonts = TempDir::new("wrong-ext");
        let err = import_theme_preset_bytes_impl(
            &s,
            "theme.exe",
            valid_json_bytes("Neon", "#FF00FF"),
            fonts.path(),
        )
        .unwrap_err();
        assert!(err.starts_with("Unsupported preset extension"), "{err}");
        assert!(s.get_all().unwrap().is_empty());
    }

    #[test]
    fn importing_preset_bytes_rejects_a_blank_or_pathy_file_name() {
        let s = store("blank-name");
        let fonts = TempDir::new("blank-name");
        assert_eq!(
            import_theme_preset_bytes_impl(&s, "", vec![1], fonts.path()).unwrap_err(),
            "Invalid preset file name"
        );
        assert_eq!(
            import_theme_preset_bytes_impl(&s, "   ", vec![1], fonts.path()).unwrap_err(),
            "Invalid preset file name"
        );
    }

    #[test]
    fn importing_preset_bytes_rejects_empty_and_oversized_payloads() {
        let s = store("empty-and-huge");
        let fonts = TempDir::new("empty-and-huge");

        let empty =
            import_theme_preset_bytes_impl(&s, "theme.json", Vec::new(), fonts.path()).unwrap_err();
        assert_eq!(empty, "Preset file is empty");

        let oversized = vec![0u8; MAX_THEME_UPLOAD_BYTES + 1];
        let err =
            import_theme_preset_bytes_impl(&s, "theme.json", oversized, fonts.path()).unwrap_err();
        assert!(err.starts_with("Preset file is too large"), "{err}");

        assert!(s.get_all().unwrap().is_empty());
    }

    #[test]
    fn importing_preset_bytes_surfaces_the_underlying_parse_error_for_bad_json() {
        let s = store("bad-json");
        let fonts = TempDir::new("bad-json");
        let err =
            import_theme_preset_bytes_impl(&s, "theme.json", b"{not json".to_vec(), fonts.path())
                .unwrap_err();
        assert!(err.starts_with("Invalid preset JSON:"), "{err}");
        assert!(s.get_all().unwrap().is_empty());
    }

    // ---- success paths, delegating to ThemePresetStore::import_preset_file ----

    #[test]
    fn importing_preset_bytes_accepts_a_wrapped_json_export_and_saves_it() {
        let s = store("json-ok");
        let fonts = TempDir::new("json-ok");
        let bytes = valid_json_bytes("Uploaded Neon", "#ABCDEF");

        let imported = import_theme_preset_bytes_impl(
            &s,
            "Uploaded Neon.ram-theme.json",
            bytes,
            fonts.path(),
        )
        .expect("import");
        assert_eq!(imported.name, "Uploaded Neon");
        assert_eq!(imported.theme.accounts_background, "#ABCDEF");
        assert_eq!(s.get_all().unwrap().len(), 1);
    }

    #[test]
    fn importing_preset_bytes_falls_back_to_the_original_file_stem_when_unnamed() {
        let s = store("fallback-name");
        let fonts = TempDir::new("fallback-name");
        let payload = serde_json::json!({
            "theme": serde_json::to_value(ThemeData::default()).unwrap(),
        });
        let bytes = serde_json::to_vec(&payload).unwrap();

        // The uploaded name (not any scratch/temp name) is what the fallback
        // is derived from, exactly like the by-path command's own fallback.
        let imported = import_theme_preset_bytes_impl(
            &s,
            "MyBareTheme.ram-theme.json",
            bytes,
            fonts.path(),
        )
        .expect("import");
        assert_eq!(imported.name, "MyBareTheme.ram-theme");
    }

    #[test]
    fn importing_preset_bytes_accepts_a_zip_bundle() {
        let s = store("zip-ok");
        let fonts = TempDir::new("zip-ok");
        let bytes = zip_bundle_bytes("Uploaded Bundle");

        let imported =
            import_theme_preset_bytes_impl(&s, "bundle.ram-theme.zip", bytes, fonts.path())
                .expect("import");
        assert_eq!(imported.name, "Uploaded Bundle");
        assert_eq!(s.get_all().unwrap().len(), 1);
    }

    #[test]
    fn importing_preset_bytes_only_uses_the_base_name_from_a_pathy_file_name() {
        let s = store("pathy-name");
        let fonts = TempDir::new("pathy-name");
        let bytes = valid_json_bytes("Traversal", "#111111");

        let imported = import_theme_preset_bytes_impl(
            &s,
            "..\\..\\Traversal.ram-theme.json",
            bytes,
            fonts.path(),
        )
        .expect("import");
        assert_eq!(imported.name, "Traversal");
    }
}
