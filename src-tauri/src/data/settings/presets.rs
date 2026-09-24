#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemePresetData {
    pub id: String,
    pub name: String,
    pub theme: ThemeData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ThemePresetExportFile {
    format: String,
    name: String,
    theme: ThemeData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ThemeBundleExportFile {
    format: String,
    name: String,
    theme: ThemeData,
}

pub struct ThemePresetStore {
    presets: Mutex<Vec<ThemePresetData>>,
    file_path: PathBuf,
    /// Ligado quando o arquivo existe mas não pôde ser lido/parseado. Enquanto
    /// ligado, gravar é recusado para que uma lista vazia em memória nunca
    /// sobrescreva os presets do usuário — mesmo latch do `ScriptStore`.
    load_failed: std::sync::atomic::AtomicBool,
}

impl ThemePresetStore {
    pub fn new(file_path: PathBuf) -> Self {
        let (presets, failed) = match Self::load_from_file(&file_path) {
            Ok(presets) => (presets, false),
            Err(_) => (Vec::new(), true),
        };
        Self {
            presets: Mutex::new(presets),
            file_path,
            load_failed: std::sync::atomic::AtomicBool::new(failed),
        }
    }

    fn load_from_file(path: &Path) -> Result<Vec<ThemePresetData>, String> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        let raw =
            fs::read_to_string(path).map_err(|e| format!("Failed to read preset file: {}", e))?;
        // Arquivo de 0 byte é uma lista vazia legítima, não corrupção.
        if raw.trim().is_empty() {
            return Ok(Vec::new());
        }

        serde_json::from_str::<Vec<ThemePresetData>>(&raw)
            .map_err(|e| format!("Failed to parse preset file: {}", e))
    }

    fn save_all(&self, presets: &[ThemePresetData]) -> Result<(), String> {
        if self.load_failed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(
                "Preset file could not be read; refusing to overwrite it. Fix or restore the preset file and restart.".to_string(),
            );
        }

        let payload = serde_json::to_string_pretty(presets)
            .map_err(|e| format!("Failed to serialize presets: {}", e))?;
        fs::write(&self.file_path, payload).map_err(|e| {
            format!(
                "Failed to write preset file {}: {}",
                self.file_path.display(),
                e
            )
        })
    }

    fn sanitize_preset_name(name: &str) -> String {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return "Custom Preset".to_string();
        }
        trimmed.to_string()
    }

    fn make_preset_id(name: &str) -> String {
        let mut slug = String::new();
        for ch in name.chars() {
            if ch.is_ascii_alphanumeric() {
                slug.push(ch.to_ascii_lowercase());
            } else if ch == ' ' || ch == '-' || ch == '_' {
                if !slug.ends_with('-') {
                    slug.push('-');
                }
            }
        }
        let base = slug.trim_matches('-');
        let safe = if base.is_empty() { "preset" } else { base };
        format!("{}-{}", safe, chrono::Utc::now().timestamp_millis())
    }

    fn sanitize_file_stem(name: &str) -> String {
        let mut out = String::new();
        for ch in name.chars() {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                out.push(ch);
            } else if ch == ' ' {
                out.push('-');
            }
        }
        let trimmed = out.trim_matches('-');
        if trimmed.is_empty() {
            "theme-preset".to_string()
        } else {
            trimmed.to_string()
        }
    }

    fn normalize_font_spec(spec: &Option<ThemeFontSpec>, fallback: ThemeFontSpec) -> ThemeFontSpec {
        spec.clone().unwrap_or(fallback)
    }

    #[allow(dead_code)]
    fn uses_default_fonts(theme: &ThemeData) -> bool {
        let ds = default_font_sans();
        let dm = default_font_mono();
        let sans = Self::normalize_font_spec(&theme.font_sans, ds.clone());
        let mono = Self::normalize_font_spec(&theme.font_mono, dm.clone());
        sans == ds && mono == dm
    }

    #[allow(dead_code)]
    fn strip_default_fonts(mut theme: ThemeData) -> ThemeData {
        if Self::uses_default_fonts(&theme) {
            theme.font_sans = None;
            theme.font_mono = None;
        }
        theme
    }

    fn local_font_files(theme: &ThemeData) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let ds = default_font_sans();
        let dm = default_font_mono();
        let sans = Self::normalize_font_spec(&theme.font_sans, ds);
        let mono = Self::normalize_font_spec(&theme.font_mono, dm);
        for spec in [sans, mono] {
            if spec.source == "local" {
                if let Some(local) = spec.local {
                    let name = local.file.trim().to_string();
                    if !name.is_empty() {
                        out.push(name);
                    }
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    pub fn get_all(&self) -> Result<Vec<ThemePresetData>, String> {
        let presets = self.presets.lock().map_err(|e| e.to_string())?;
        Ok(presets.clone())
    }

    pub fn save_preset(&self, name: &str, theme: ThemeData) -> Result<ThemePresetData, String> {
        let normalized_name = Self::sanitize_preset_name(name);
        let mut presets = self.presets.lock().map_err(|e| e.to_string())?;

        if let Some(existing) = presets
            .iter_mut()
            .find(|p| p.name.eq_ignore_ascii_case(&normalized_name))
        {
            existing.name = normalized_name.clone();
            existing.theme = theme;
            let result = existing.clone();
            let snapshot = presets.clone();
            drop(presets);
            self.save_all(&snapshot)?;
            return Ok(result);
        }

        let preset = ThemePresetData {
            id: Self::make_preset_id(&normalized_name),
            name: normalized_name,
            theme,
        };
        presets.push(preset.clone());
        let snapshot = presets.clone();
        drop(presets);
        self.save_all(&snapshot)?;
        Ok(preset)
    }

    pub fn delete_preset(&self, preset_id: &str) -> Result<(), String> {
        let mut presets = self.presets.lock().map_err(|e| e.to_string())?;
        let before = presets.len();
        presets.retain(|p| p.id != preset_id);
        if before == presets.len() {
            return Err(format!("Preset '{}' not found", preset_id));
        }
        let snapshot = presets.clone();
        drop(presets);
        self.save_all(&snapshot)
    }

    pub fn import_preset_file(&self, path: &str) -> Result<ThemePresetData, String> {
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".zip") || lower.ends_with(".ram-theme.zip") {
            return self.import_bundle_file(path);
        }

        let raw =
            fs::read_to_string(path).map_err(|e| format!("Failed to read preset file: {}", e))?;
        let value: serde_json::Value =
            serde_json::from_str(&raw).map_err(|e| format!("Invalid preset JSON: {}", e))?;

        let (name, theme) = if let Some(theme_value) = value.get("theme") {
            let parsed_theme: ThemeData = serde_json::from_value(theme_value.clone())
                .map_err(|e| format!("Invalid preset theme payload: {}", e))?;
            let parsed_name = value
                .get("name")
                .and_then(|n| n.as_str())
                .map(|n| n.to_string())
                .unwrap_or_default();
            (parsed_name, parsed_theme)
        } else {
            let parsed_theme: ThemeData =
                serde_json::from_value(value).map_err(|e| format!("Invalid theme data: {}", e))?;
            (String::new(), parsed_theme)
        };

        let fallback_name = Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Imported Preset")
            .to_string();
        let chosen_name = if name.trim().is_empty() {
            fallback_name
        } else {
            name
        };

        self.save_preset(&chosen_name, theme)
    }

    fn import_bundle_file(&self, path: &str) -> Result<ThemePresetData, String> {
        let file =
            fs::File::open(path).map_err(|e| format!("Failed to open theme bundle: {}", e))?;
        let mut archive =
            ZipArchive::new(file).map_err(|e| format!("Invalid theme bundle zip: {}", e))?;

        let mut manifest_raw = String::new();
        {
            let mut mf = archive
                .by_name("theme.json")
                .map_err(|_| "Missing theme.json in bundle".to_string())?;
            use std::io::Read;
            mf.read_to_string(&mut manifest_raw)
                .map_err(|e| format!("Failed to read theme.json: {}", e))?;
        }

        let parsed: ThemeBundleExportFile = serde_json::from_str(&manifest_raw)
            .map_err(|e| format!("Invalid theme bundle manifest: {}", e))?;
        if parsed.format != "ram-theme-bundle-v1" {
            return Err("Unsupported theme bundle format".to_string());
        }

        let fonts_dir = get_theme_fonts_dir();
        fs::create_dir_all(&fonts_dir).map_err(|e| format!("Failed to create font dir: {}", e))?;

        for i in 0..archive.len() {
            let mut f = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = f.name().to_string();
            if !name.starts_with("fonts/") {
                continue;
            }
            if name.ends_with('/') {
                continue;
            }

            // Prevent zip-slip by only honoring enclosed names.
            let enclosed = match f.enclosed_name() {
                Some(p) => p.to_path_buf(),
                None => continue,
            };
            let file_name = enclosed
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if file_name.is_empty() {
                continue;
            }

            let ext = Path::new(&file_name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !is_allowed_font_ext(&ext) {
                continue;
            }

            let dest = fonts_dir.join(&file_name);
            if dest.exists() {
                continue;
            }

            let mut bytes: Vec<u8> = Vec::new();
            use std::io::Read;
            f.read_to_end(&mut bytes)
                .map_err(|e| format!("Failed to extract font {}: {}", file_name, e))?;
            if !bytes.is_empty() {
                fs::write(&dest, &bytes)
                    .map_err(|e| format!("Failed to write extracted font {}: {}", file_name, e))?;
            }
        }

        let preset_name = Self::sanitize_preset_name(&parsed.name);
        self.save_preset(&preset_name, parsed.theme)
    }

    pub fn export_preset_file(name: &str, theme: ThemeData) -> Result<String, String> {
        let normalized_name = Self::sanitize_preset_name(name);
        let stem = Self::sanitize_file_stem(&normalized_name);
        let base_dir = get_runtime_data_dir();

        let local_fonts = Self::local_font_files(&theme);
        let should_export_json = local_fonts.is_empty();
        let mut out_path = if should_export_json {
            base_dir.join(format!("{}.ram-theme.json", stem))
        } else {
            base_dir.join(format!("{}.ram-theme.zip", stem))
        };

        loop {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&out_path)
            {
                Ok(mut file) => {
                    if should_export_json {
                        let payload = ThemePresetExportFile {
                            format: "ram-theme-preset-v1".to_string(),
                            name: normalized_name.clone(),
                            theme: theme.clone(),
                        };
                        let json = serde_json::to_string_pretty(&payload)
                            .map_err(|e| format!("Failed to serialize exported preset: {}", e))?;
                        file.write_all(json.as_bytes()).map_err(|e| {
                            format!(
                                "Failed to write preset export {}: {}",
                                out_path.display(),
                                e
                            )
                        })?;
                        return Ok(out_path.to_string_lossy().into_owned());
                    }

                    let mut writer = ZipWriter::new(file);
                    let opts =
                        FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

                    let payload = ThemeBundleExportFile {
                        format: "ram-theme-bundle-v1".to_string(),
                        name: normalized_name.clone(),
                        theme: theme.clone(),
                    };
                    let manifest_json = serde_json::to_string_pretty(&payload)
                        .map_err(|e| format!("Failed to serialize theme bundle: {}", e))?;

                    writer
                        .start_file("theme.json", opts)
                        .map_err(|e| format!("Failed to write theme.json: {}", e))?;
                    writer
                        .write_all(manifest_json.as_bytes())
                        .map_err(|e| format!("Failed to write theme.json: {}", e))?;

                    if !local_fonts.is_empty() {
                        let fonts_dir = get_theme_fonts_dir();
                        for font_file in local_fonts {
                            let safe = Path::new(&font_file)
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("")
                                .to_string();
                            if safe.is_empty() {
                                continue;
                            }
                            let src = fonts_dir.join(&safe);
                            if !src.exists() {
                                continue;
                            }
                            let bytes = fs::read(&src).map_err(|e| {
                                format!("Failed to read font asset {}: {}", safe, e)
                            })?;
                            if bytes.is_empty() {
                                continue;
                            }
                            writer
                                .start_file(format!("fonts/{}", safe), opts)
                                .map_err(|e| format!("Failed to add font to bundle: {}", e))?;
                            writer
                                .write_all(&bytes)
                                .map_err(|e| format!("Failed to add font to bundle: {}", e))?;
                        }
                    }

                    writer
                        .finish()
                        .map_err(|e| format!("Failed to finalize zip: {}", e))?;
                    return Ok(out_path.to_string_lossy().into_owned());
                }
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                    let suffix = chrono::Utc::now().format("%Y%m%d-%H%M%S-%f");
                    out_path = if should_export_json {
                        base_dir.join(format!("{}-{}.ram-theme.json", stem, suffix))
                    } else {
                        base_dir.join(format!("{}-{}.ram-theme.zip", stem, suffix))
                    };
                }
                Err(e) => {
                    return Err(format!(
                        "Failed to write preset export {}: {}",
                        out_path.display(),
                        e
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod theme_preset_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn nanos() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ram-presets-{name}-{}.json", nanos()))
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

    fn themed(color: &str) -> ThemeData {
        let mut theme = ThemeData::default();
        theme.accounts_background = color.to_string();
        theme
    }

    fn local_font_theme(file: &str) -> ThemeData {
        let mut theme = ThemeData::default();
        theme.font_sans = Some(ThemeFontSpec {
            source: "local".to_string(),
            family: "Local Sans".to_string(),
            fallbacks: vec![],
            google: None,
            local: Some(ThemeFontLocalSpec {
                file: file.to_string(),
                weight: 400,
                style: "normal".to_string(),
            }),
        });
        theme
    }

    // ---- name / id / file stem helpers -------------------------------------------

    #[test]
    fn sanitize_preset_name_trims_and_falls_back_to_custom_preset() {
        assert_eq!(ThemePresetStore::sanitize_preset_name("  Neon  "), "Neon");
        assert_eq!(ThemePresetStore::sanitize_preset_name("Neon"), "Neon");
        assert_eq!(ThemePresetStore::sanitize_preset_name(""), "Custom Preset");
        assert_eq!(ThemePresetStore::sanitize_preset_name("   "), "Custom Preset");
        assert_eq!(
            ThemePresetStore::sanitize_preset_name("\t\n"),
            "Custom Preset"
        );
    }

    #[test]
    fn make_preset_id_slugifies_the_name_and_appends_a_timestamp() {
        let id = ThemePresetStore::make_preset_id("My Neon Theme!");
        let (slug, stamp) = id.rsplit_once('-').expect("id has a timestamp suffix");
        assert_eq!(slug, "my-neon-theme");
        assert!(stamp.parse::<i64>().is_ok(), "{id}");

        // Separators collapse and are trimmed from both ends.
        assert!(ThemePresetStore::make_preset_id("  --a__b  ").starts_with("a-b-"));
        assert!(ThemePresetStore::make_preset_id("A B  C").starts_with("a-b-c-"));
        // Nothing sluggable falls back to "preset".
        assert!(ThemePresetStore::make_preset_id("!!!").starts_with("preset-"));
        assert!(ThemePresetStore::make_preset_id("\u{00e7}\u{00e3}\u{00f5}").starts_with("preset-"));
        // Non-ASCII letters are dropped; ASCII ones beside them survive.
        assert!(ThemePresetStore::make_preset_id("A\u{00e7}\u{00e3}o").starts_with("ao-"));
        // Digits survive.
        assert!(ThemePresetStore::make_preset_id("Theme 2").starts_with("theme-2-"));
    }

    #[test]
    fn sanitize_file_stem_keeps_only_filename_safe_characters() {
        assert_eq!(ThemePresetStore::sanitize_file_stem("My Theme"), "My-Theme");
        assert_eq!(ThemePresetStore::sanitize_file_stem("keep_me-2"), "keep_me-2");
        assert_eq!(ThemePresetStore::sanitize_file_stem("a/b\\c:d*e?"), "abcde");
        assert_eq!(ThemePresetStore::sanitize_file_stem("..\\escape"), "escape");
        assert_eq!(ThemePresetStore::sanitize_file_stem("---"), "theme-preset");
        assert_eq!(ThemePresetStore::sanitize_file_stem(""), "theme-preset");
        assert_eq!(
            ThemePresetStore::sanitize_file_stem("\u{00e7}\u{00e3}\u{00f5}"),
            "theme-preset"
        );
        assert_eq!(ThemePresetStore::sanitize_file_stem("A\u{00e7}\u{00e3}o"), "Ao");
    }

    // ---- font helpers --------------------------------------------------------------

    #[test]
    fn normalize_font_spec_uses_the_fallback_only_when_the_slot_is_empty() {
        let fallback = default_font_mono();
        assert_eq!(
            ThemePresetStore::normalize_font_spec(&None, fallback.clone()),
            fallback
        );
        let custom = default_font_sans();
        assert_eq!(
            ThemePresetStore::normalize_font_spec(&Some(custom.clone()), fallback),
            custom
        );
    }

    #[test]
    fn uses_default_fonts_treats_none_as_the_default_and_strip_clears_them() {
        let mut default_theme = ThemeData::default();
        assert!(ThemePresetStore::uses_default_fonts(&default_theme));

        default_theme.font_sans = None;
        default_theme.font_mono = None;
        assert!(
            ThemePresetStore::uses_default_fonts(&default_theme),
            "an absent spec is normalized to the default"
        );

        let custom = local_font_theme("abc.ttf");
        assert!(!ThemePresetStore::uses_default_fonts(&custom));

        let stripped = ThemePresetStore::strip_default_fonts(ThemeData::default());
        assert_eq!(stripped.font_sans, None);
        assert_eq!(stripped.font_mono, None);

        let kept = ThemePresetStore::strip_default_fonts(local_font_theme("abc.ttf"));
        assert!(kept.font_sans.is_some());
    }

    #[test]
    fn local_font_files_lists_only_non_empty_local_sources_sorted_and_deduped() {
        assert!(ThemePresetStore::local_font_files(&ThemeData::default()).is_empty());

        let mut theme = local_font_theme("bbb.ttf");
        assert_eq!(
            ThemePresetStore::local_font_files(&theme),
            vec!["bbb.ttf".to_string()]
        );

        // Both slots pointing at the same file collapse to one entry.
        theme.font_mono = theme.font_sans.clone();
        assert_eq!(
            ThemePresetStore::local_font_files(&theme),
            vec!["bbb.ttf".to_string()]
        );

        // Two different files come back sorted.
        theme.font_mono = Some(ThemeFontSpec {
            source: "local".to_string(),
            family: "Mono".to_string(),
            fallbacks: vec![],
            google: None,
            local: Some(ThemeFontLocalSpec {
                file: "  aaa.otf  ".to_string(),
                weight: 400,
                style: "normal".to_string(),
            }),
        });
        assert_eq!(
            ThemePresetStore::local_font_files(&theme),
            vec!["aaa.otf".to_string(), "bbb.ttf".to_string()]
        );

        // A local source without a usable file name is skipped.
        theme.font_mono = Some(ThemeFontSpec {
            source: "local".to_string(),
            family: "Mono".to_string(),
            fallbacks: vec![],
            google: None,
            local: Some(ThemeFontLocalSpec {
                file: "   ".to_string(),
                weight: 400,
                style: "normal".to_string(),
            }),
        });
        assert_eq!(
            ThemePresetStore::local_font_files(&theme),
            vec!["bbb.ttf".to_string()]
        );
    }

    // ---- save / delete ----------------------------------------------------------------

    #[test]
    fn save_preset_adds_normalizes_and_persists() {
        let s = store("save");
        assert!(s.get_all().unwrap().is_empty());

        let saved = s.save_preset("  Neon  ", themed("#FF00FF")).unwrap();
        assert_eq!(saved.name, "Neon");
        assert!(saved.id.starts_with("neon-"));
        assert_eq!(saved.theme.accounts_background, "#FF00FF");

        let reloaded = ThemePresetStore::new(s.file_path.clone());
        let all = reloaded.get_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, saved.id);
        assert_eq!(all[0].theme.accounts_background, "#FF00FF");
    }

    #[test]
    fn save_preset_replaces_a_same_name_preset_case_insensitively_and_keeps_its_id() {
        let s = store("save-replace");
        let first = s.save_preset("Neon", themed("#111111")).unwrap();
        let second = s.save_preset("  nEoN  ", themed("#222222")).unwrap();

        assert_eq!(second.id, first.id, "the id is stable across an overwrite");
        assert_eq!(second.name, "nEoN", "the new spelling wins");
        assert_eq!(second.theme.accounts_background, "#222222");
        assert_eq!(s.get_all().unwrap().len(), 1, "no duplicate preset");

        // A different name really does add a second preset.
        s.save_preset("Other", themed("#333333")).unwrap();
        assert_eq!(s.get_all().unwrap().len(), 2);
    }

    #[test]
    fn save_preset_falls_back_to_custom_preset_for_a_blank_name() {
        let s = store("save-blank");
        let saved = s.save_preset("   ", ThemeData::default()).unwrap();
        assert_eq!(saved.name, "Custom Preset");
        assert!(saved.id.starts_with("custom-preset-"));
    }

    #[test]
    fn delete_preset_removes_it_and_errors_for_an_unknown_id() {
        let s = store("delete");
        let saved = s.save_preset("Neon", ThemeData::default()).unwrap();

        assert_eq!(
            s.delete_preset("nope").unwrap_err(),
            "Preset 'nope' not found"
        );
        assert_eq!(s.get_all().unwrap().len(), 1);

        s.delete_preset(&saved.id).unwrap();
        assert!(s.get_all().unwrap().is_empty());
        assert!(ThemePresetStore::new(s.file_path.clone())
            .get_all()
            .unwrap()
            .is_empty());

        assert!(s.delete_preset(&saved.id).is_err(), "deleting twice errors");
    }

    // ---- loading --------------------------------------------------------------------

    #[test]
    fn a_corrupt_preset_file_is_never_overwritten() {
        // Comportamento antigo (corrigido): `load_from_file` caía em
        // `unwrap_or_default()` e o próximo `save_preset` apagava os presets do
        // usuário. Agora a falha de leitura fica latcheada, como no ScriptStore.
        let s = store("load-corrupt");
        assert!(s.get_all().unwrap().is_empty(), "missing file");

        let corrupt = b"[{\"id\": \"x\", \"name\"";
        fs::write(&s.file_path, corrupt).unwrap();
        let reopened = ThemePresetStore::new(s.file_path.clone());
        assert!(reopened.get_all().unwrap().is_empty());

        let err = reopened
            .save_preset("New", ThemeData::default())
            .expect_err("gravar sobre um arquivo ilegível deve falhar");
        assert!(err.contains("refusing to overwrite"), "{err}");
        assert_eq!(
            fs::read(&s.file_path).unwrap(),
            corrupt.to_vec(),
            "o arquivo do usuário fica intacto"
        );
    }

    #[test]
    fn a_preset_file_of_the_wrong_shape_also_latches() {
        let s = store("load-wrong-shape");
        let wrong = br#"{"presets": []}"#;
        fs::write(&s.file_path, wrong).unwrap();

        let reopened = ThemePresetStore::new(s.file_path.clone());
        assert!(reopened.get_all().unwrap().is_empty());
        assert!(reopened.save_preset("New", ThemeData::default()).is_err());
        assert_eq!(fs::read(&s.file_path).unwrap(), wrong.to_vec());
    }

    #[test]
    fn an_empty_preset_file_is_not_treated_as_corrupt() {
        // Uma gravação interrompida pode deixar 0 byte; isso é uma lista vazia
        // legítima e precisa continuar gravável.
        let s = store("load-empty-file");
        fs::write(&s.file_path, b"").unwrap();

        let reopened = ThemePresetStore::new(s.file_path.clone());
        assert!(reopened.get_all().unwrap().is_empty());
        reopened
            .save_preset("New", ThemeData::default())
            .expect("deve gravar");
        assert_eq!(
            ThemePresetStore::new(s.file_path.clone())
                .get_all()
                .unwrap()
                .len(),
            1
        );
    }

    // ---- import ----------------------------------------------------------------------

    #[test]
    fn import_preset_file_accepts_the_wrapped_export_format() {
        let s = store("import-wrapped");
        let payload = serde_json::json!({
            "format": "ram-theme-preset-v1",
            "name": "Exported Neon",
            "theme": serde_json::to_value(themed("#ABCDEF")).unwrap(),
        });
        let file = temp_path("import-wrapped-src");
        fs::write(&file, serde_json::to_vec_pretty(&payload).unwrap()).unwrap();

        let imported = s.import_preset_file(file.to_str().unwrap()).unwrap();
        assert_eq!(imported.name, "Exported Neon");
        assert_eq!(imported.theme.accounts_background, "#ABCDEF");
        assert_eq!(s.get_all().unwrap().len(), 1);

        let _ = fs::remove_file(&file);
    }

    #[test]
    fn import_preset_file_accepts_a_bare_theme_and_names_it_after_the_file() {
        let s = store("import-bare");
        let file = temp_path("MyBareTheme");
        fs::write(&file, serde_json::to_vec(&themed("#010203")).unwrap()).unwrap();

        let imported = s.import_preset_file(file.to_str().unwrap()).unwrap();
        let expected_name = file.file_stem().unwrap().to_str().unwrap();
        assert_eq!(imported.name, expected_name);
        assert_eq!(imported.theme.accounts_background, "#010203");

        let _ = fs::remove_file(&file);
    }

    #[test]
    fn import_preset_file_falls_back_to_the_file_stem_when_the_name_is_blank() {
        let s = store("import-blank-name");
        let payload = serde_json::json!({
            "name": "   ",
            "theme": serde_json::to_value(ThemeData::default()).unwrap(),
        });
        let file = temp_path("FallbackName");
        fs::write(&file, serde_json::to_vec(&payload).unwrap()).unwrap();

        let imported = s.import_preset_file(file.to_str().unwrap()).unwrap();
        assert_eq!(imported.name, file.file_stem().unwrap().to_str().unwrap());

        let _ = fs::remove_file(&file);
    }

    #[test]
    fn import_preset_file_reports_missing_files_and_bad_payloads() {
        let s = store("import-errors");

        let missing = temp_path("does-not-exist");
        let err = s.import_preset_file(missing.to_str().unwrap()).unwrap_err();
        assert!(err.starts_with("Failed to read preset file:"), "{err}");

        let file = temp_path("import-bad");
        fs::write(&file, b"{not json").unwrap();
        let err = s.import_preset_file(file.to_str().unwrap()).unwrap_err();
        assert!(err.starts_with("Invalid preset JSON:"), "{err}");

        fs::write(&file, br#"{"theme": {"nope": 1}}"#).unwrap();
        let err = s.import_preset_file(file.to_str().unwrap()).unwrap_err();
        assert!(err.starts_with("Invalid preset theme payload:"), "{err}");

        fs::write(&file, br#"{"unrelated": 1}"#).unwrap();
        let err = s.import_preset_file(file.to_str().unwrap()).unwrap_err();
        assert!(err.starts_with("Invalid theme data:"), "{err}");

        assert!(s.get_all().unwrap().is_empty());
        let _ = fs::remove_file(&file);
    }

    #[test]
    fn importing_a_bundle_validates_the_zip_and_its_manifest() {
        let s = store("import-bundle");

        // Not a zip at all.
        let not_zip = std::env::temp_dir().join(format!("ram-presets-fake-{}.zip", nanos()));
        fs::write(&not_zip, b"definitely not a zip").unwrap();
        let err = s.import_preset_file(not_zip.to_str().unwrap()).unwrap_err();
        assert!(err.starts_with("Invalid theme bundle zip:"), "{err}");
        let _ = fs::remove_file(&not_zip);

        // A zip without the manifest.
        let no_manifest = std::env::temp_dir().join(format!("ram-presets-nomf-{}.zip", nanos()));
        {
            let file = fs::File::create(&no_manifest).unwrap();
            let mut writer = ZipWriter::new(file);
            let opts = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            writer.start_file("readme.txt", opts).unwrap();
            writer.write_all(b"hello").unwrap();
            writer.finish().unwrap();
        }
        let err = s.import_preset_file(no_manifest.to_str().unwrap()).unwrap_err();
        assert_eq!(err, "Missing theme.json in bundle");
        let _ = fs::remove_file(&no_manifest);

        // A manifest declaring an unsupported format.
        let wrong_format = std::env::temp_dir().join(format!("ram-presets-fmt-{}.zip", nanos()));
        {
            let manifest = serde_json::json!({
                "format": "ram-theme-bundle-v999",
                "name": "Future",
                "theme": serde_json::to_value(ThemeData::default()).unwrap(),
            });
            let file = fs::File::create(&wrong_format).unwrap();
            let mut writer = ZipWriter::new(file);
            let opts = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            writer.start_file("theme.json", opts).unwrap();
            writer
                .write_all(serde_json::to_vec(&manifest).unwrap().as_slice())
                .unwrap();
            writer.finish().unwrap();
        }
        let err = s.import_preset_file(wrong_format.to_str().unwrap()).unwrap_err();
        assert_eq!(err, "Unsupported theme bundle format");
        let _ = fs::remove_file(&wrong_format);

        // A manifest that is not JSON at all.
        let bad_manifest = std::env::temp_dir().join(format!("ram-presets-badmf-{}.zip", nanos()));
        {
            let file = fs::File::create(&bad_manifest).unwrap();
            let mut writer = ZipWriter::new(file);
            let opts = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            writer.start_file("theme.json", opts).unwrap();
            writer.write_all(b"{not json").unwrap();
            writer.finish().unwrap();
        }
        let err = s.import_preset_file(bad_manifest.to_str().unwrap()).unwrap_err();
        assert!(err.starts_with("Invalid theme bundle manifest:"), "{err}");
        let _ = fs::remove_file(&bad_manifest);

        assert!(s.get_all().unwrap().is_empty(), "no preset may be created");
    }

    // ---- export -----------------------------------------------------------------------

    #[test]
    fn export_writes_json_for_a_font_free_theme_and_never_overwrites() {
        let name = format!("RamExportProbe{}", nanos());

        let first = ThemePresetStore::export_preset_file(&name, themed("#0A0B0C"))
            .expect("first export");
        assert!(first.ends_with(".ram-theme.json"), "{first}");

        let raw = fs::read_to_string(&first).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed["format"], "ram-theme-preset-v1");
        assert_eq!(parsed["name"], name);
        assert_eq!(parsed["theme"]["accounts_background"], "#0A0B0C");

        // A second export of the same name gets a timestamped file instead of
        // clobbering the first one.
        let second = ThemePresetStore::export_preset_file(&name, ThemeData::default())
            .expect("second export");
        assert_ne!(second, first);
        assert!(fs::metadata(&first).is_ok(), "the first export must survive");

        let _ = fs::remove_file(&first);
        let _ = fs::remove_file(&second);
    }

    #[test]
    fn export_writes_a_zip_bundle_when_the_theme_references_local_fonts() {
        let name = format!("RamExportBundle{}", nanos());
        let path = ThemePresetStore::export_preset_file(&name, local_font_theme("missing.ttf"))
            .expect("export");
        assert!(path.ends_with(".ram-theme.zip"), "{path}");

        let mut archive = ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
        let mut manifest = String::new();
        {
            use std::io::Read;
            archive
                .by_name("theme.json")
                .unwrap()
                .read_to_string(&mut manifest)
                .unwrap();
        }
        let parsed: serde_json::Value = serde_json::from_str(&manifest).unwrap();
        assert_eq!(parsed["format"], "ram-theme-bundle-v1");
        assert_eq!(parsed["name"], name);
        // The referenced font does not exist on disk, so it is simply skipped.
        assert_eq!(archive.len(), 1, "only the manifest is in the bundle");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn exported_names_are_sanitized_into_the_file_name() {
        let unique = nanos();
        let name = format!("Ram/Export:Probe {unique}");
        let path = ThemePresetStore::export_preset_file(&name, ThemeData::default())
            .expect("export");
        let file_name = Path::new(&path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap()
            .to_string();
        assert_eq!(file_name, format!("RamExportProbe-{unique}.ram-theme.json"));

        // The unsanitized name is still what gets stored inside the file.
        let parsed: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed["name"], name);

        let _ = fs::remove_file(&path);
    }

    // ---- serde -------------------------------------------------------------------------

    #[test]
    fn preset_data_round_trips_through_json() {
        let preset = ThemePresetData {
            id: "neon-1".to_string(),
            name: "Neon".to_string(),
            theme: themed("#FEFEFE"),
        };
        let json = serde_json::to_string(&preset).unwrap();
        let parsed: ThemePresetData = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "neon-1");
        assert_eq!(parsed.name, "Neon");
        assert_eq!(parsed.theme.accounts_background, "#FEFEFE");
    }
}
