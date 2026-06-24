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
}

impl ThemePresetStore {
    pub fn new(file_path: PathBuf) -> Self {
        let presets = Self::load_from_file(&file_path);
        Self {
            presets: Mutex::new(presets),
            file_path,
        }
    }

    fn load_from_file(path: &Path) -> Vec<ThemePresetData> {
        if !path.exists() {
            return Vec::new();
        }

        let raw = match fs::read_to_string(path) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

        serde_json::from_str::<Vec<ThemePresetData>>(&raw).unwrap_or_default()
    }

    fn save_all(&self, presets: &[ThemePresetData]) -> Result<(), String> {
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
