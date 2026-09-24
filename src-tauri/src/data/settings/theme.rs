#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeFontGoogleSpec {
    pub weights: Vec<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeFontLocalSpec {
    pub file: String,
    pub weight: i32,
    pub style: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeFontSpec {
    pub source: String,
    pub family: String,
    pub fallbacks: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google: Option<ThemeFontGoogleSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local: Option<ThemeFontLocalSpec>,
}

fn default_font_sans() -> ThemeFontSpec {
    ThemeFontSpec {
        source: "google".to_string(),
        family: "Outfit".to_string(),
        fallbacks: vec![
            "system-ui".to_string(),
            "-apple-system".to_string(),
            "Segoe UI".to_string(),
            "sans-serif".to_string(),
        ],
        google: Some(ThemeFontGoogleSpec {
            weights: vec![300, 400, 500, 600, 700],
        }),
        local: None,
    }
}

fn default_font_mono() -> ThemeFontSpec {
    ThemeFontSpec {
        source: "google".to_string(),
        family: "JetBrains Mono".to_string(),
        fallbacks: vec![
            "Cascadia Code".to_string(),
            "Consolas".to_string(),
            "monospace".to_string(),
        ],
        google: Some(ThemeFontGoogleSpec {
            weights: vec![400, 500],
        }),
        local: None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeData {
    pub accounts_background: String,
    pub accounts_foreground: String,
    pub buttons_background: String,
    pub buttons_foreground: String,
    pub buttons_border: String,
    #[serde(default = "default_toggle_on_background")]
    pub toggle_on_background: String,
    #[serde(default = "default_toggle_off_background")]
    pub toggle_off_background: String,
    #[serde(default = "default_toggle_knob_background")]
    pub toggle_knob_background: String,
    pub forms_background: String,
    pub forms_foreground: String,
    pub textboxes_background: String,
    pub textboxes_foreground: String,
    pub textboxes_border: String,
    pub label_background: String,
    pub label_foreground: String,
    pub label_transparent: bool,
    pub dark_top_bar: bool,
    pub show_headers: bool,
    pub light_images: bool,
    pub button_style: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_sans: Option<ThemeFontSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_mono: Option<ThemeFontSpec>,
}

fn default_toggle_on_background() -> String {
    "#0EA5E9".to_string()
}

fn default_toggle_off_background() -> String {
    "#3F3F46".to_string()
}

fn default_toggle_knob_background() -> String {
    "#FFFFFF".to_string()
}

impl Default for ThemeData {
    fn default() -> Self {
        Self {
            accounts_background: "#09090B".to_string(),
            accounts_foreground: "#E4E4E7".to_string(),
            buttons_background: "#27272A".to_string(),
            buttons_foreground: "#A1A1AA".to_string(),
            buttons_border: "#3F3F46".to_string(),
            toggle_on_background: default_toggle_on_background(),
            toggle_off_background: default_toggle_off_background(),
            toggle_knob_background: default_toggle_knob_background(),
            forms_background: "#09090B".to_string(),
            forms_foreground: "#E4E4E7".to_string(),
            textboxes_background: "#18181B".to_string(),
            textboxes_foreground: "#D4D4D8".to_string(),
            textboxes_border: "#27272A".to_string(),
            label_background: "#09090B".to_string(),
            label_foreground: "#71717A".to_string(),
            label_transparent: true,
            dark_top_bar: true,
            show_headers: true,
            light_images: false,
            button_style: "Flat".to_string(),
            font_sans: Some(default_font_sans()),
            font_mono: Some(default_font_mono()),
        }
    }
}

pub struct ThemeStore {
    data: Mutex<ThemeData>,
    file_path: PathBuf,
    /// Ligado quando o arquivo de tema existe mas não pôde ser lido (permissão,
    /// bytes inválidos, arquivo travado). `IniFile::load` engole o erro de I/O e
    /// devolve um INI vazio, então sem este latch o tema voltaria ao default e a
    /// primeira gravação apagaria as cores do usuário — mesmo latch do
    /// `ScriptStore`.
    load_failed: std::sync::atomic::AtomicBool,
}

impl ThemeStore {
    pub fn new(file_path: PathBuf) -> Self {
        let (data, failed) = if file_path.exists() {
            match Self::load_from_file(&file_path) {
                Ok(data) => (data, false),
                Err(_) => (ThemeData::default(), true),
            }
        } else {
            (ThemeData::default(), false)
        };

        Self {
            data: Mutex::new(data),
            file_path,
            load_failed: std::sync::atomic::AtomicBool::new(failed),
        }
    }

    fn load_from_file(path: &Path) -> Result<ThemeData, String> {
        // O parser de INI nunca falha (ignora linhas que não entende), mas a
        // leitura sim — e é justamente ela que precisa virar erro em vez de
        // devolver um tema default. `IniFile::load` engole o erro de I/O, então
        // lemos aqui e só depois entregamos o conteúdo ao parser.
        let raw =
            fs::read_to_string(path).map_err(|e| format!("Failed to read theme file: {}", e))?;
        let mut ini = IniFile::new();
        ini.parse(&raw);
        let mut data = ThemeData::default();

        let section = ini
            .get_section("Roblox Account Manager")
            .or_else(|| ini.get_section("RBX Alt Manager"));

        if let Some(s) = section {
            if let Some(v) = s.get("AccountsBG") {
                data.accounts_background = v.to_string();
            }
            if let Some(v) = s.get("AccountsFG") {
                data.accounts_foreground = v.to_string();
            }
            if let Some(v) = s.get("ButtonsBG") {
                data.buttons_background = v.to_string();
            }
            if let Some(v) = s.get("ButtonsFG") {
                data.buttons_foreground = v.to_string();
            }
            if let Some(v) = s.get("ButtonsBC") {
                data.buttons_border = v.to_string();
            }
            if let Some(v) = s.get("ToggleOnBG") {
                data.toggle_on_background = v.to_string();
            }
            if let Some(v) = s.get("ToggleOffBG") {
                data.toggle_off_background = v.to_string();
            }
            if let Some(v) = s.get("ToggleKnobBG") {
                data.toggle_knob_background = v.to_string();
            }
            if let Some(v) = s.get("FormsBG") {
                data.forms_background = v.to_string();
            }
            if let Some(v) = s.get("FormsFG") {
                data.forms_foreground = v.to_string();
            }
            if let Some(v) = s.get("TextBoxesBG") {
                data.textboxes_background = v.to_string();
            }
            if let Some(v) = s.get("TextBoxesFG") {
                data.textboxes_foreground = v.to_string();
            }
            if let Some(v) = s.get("TextBoxesBC") {
                data.textboxes_border = v.to_string();
            }
            if let Some(v) = s.get("LabelsBC") {
                data.label_background = v.to_string();
            }
            if let Some(v) = s.get("LabelsFC") {
                data.label_foreground = v.to_string();
            }
            if let Some(v) = s.get("LabelsTransparent") {
                data.label_transparent = v.eq_ignore_ascii_case("true");
            }
            if let Some(v) = s.get("DarkTopBar") {
                data.dark_top_bar = v.eq_ignore_ascii_case("true");
            }
            if let Some(v) = s.get("ShowHeaders") {
                data.show_headers = v.eq_ignore_ascii_case("true");
            }
            if let Some(v) = s.get("LightImages") {
                data.light_images = v.eq_ignore_ascii_case("true");
            }
            if let Some(v) = s.get("ButtonStyle") {
                data.button_style = v.to_string();
            }
            if let Some(v) = s.get("FontSans") {
                if let Ok(spec) = serde_json::from_str::<ThemeFontSpec>(v) {
                    data.font_sans = Some(spec);
                }
            }
            if let Some(v) = s.get("FontMono") {
                if let Ok(spec) = serde_json::from_str::<ThemeFontSpec>(v) {
                    data.font_mono = Some(spec);
                }
            }
        }

        Ok(data)
    }

    pub fn get(&self) -> Result<ThemeData, String> {
        let data = self.data.lock().map_err(|e| e.to_string())?;
        Ok(data.clone())
    }

    pub fn save(&self) -> Result<(), String> {
        if self.load_failed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(
                "Theme file could not be read; refusing to overwrite it. Fix or restore the theme file and restart.".to_string(),
            );
        }

        let data = self.data.lock().map_err(|e| e.to_string())?;
        let mut ini = IniFile::new();
        let section = ini.section("Roblox Account Manager");

        section.set("AccountsBG", &data.accounts_background, None);
        section.set("AccountsFG", &data.accounts_foreground, None);
        section.set("ButtonsBG", &data.buttons_background, None);
        section.set("ButtonsFG", &data.buttons_foreground, None);
        section.set("ButtonsBC", &data.buttons_border, None);
        section.set("ToggleOnBG", &data.toggle_on_background, None);
        section.set("ToggleOffBG", &data.toggle_off_background, None);
        section.set("ToggleKnobBG", &data.toggle_knob_background, None);
        section.set("FormsBG", &data.forms_background, None);
        section.set("FormsFG", &data.forms_foreground, None);
        section.set("TextBoxesBG", &data.textboxes_background, None);
        section.set("TextBoxesFG", &data.textboxes_foreground, None);
        section.set("TextBoxesBC", &data.textboxes_border, None);
        section.set("LabelsBC", &data.label_background, None);
        section.set("LabelsFC", &data.label_foreground, None);
        section.set(
            "LabelsTransparent",
            if data.label_transparent {
                "true"
            } else {
                "false"
            },
            None,
        );
        section.set(
            "DarkTopBar",
            if data.dark_top_bar { "true" } else { "false" },
            None,
        );
        section.set(
            "ShowHeaders",
            if data.show_headers { "true" } else { "false" },
            None,
        );
        section.set(
            "LightImages",
            if data.light_images { "true" } else { "false" },
            None,
        );
        section.set("ButtonStyle", &data.button_style, None);

        if let Some(ref spec) = data.font_sans {
            if let Ok(json) = serde_json::to_string(spec) {
                section.set("FontSans", &json, None);
            }
        }
        if let Some(ref spec) = data.font_mono {
            if let Ok(json) = serde_json::to_string(spec) {
                section.set("FontMono", &json, None);
            }
        }

        ini.save(&self.file_path)
    }

    pub fn update(&self, theme: ThemeData) -> Result<(), String> {
        let mut data = self.data.lock().map_err(|e| e.to_string())?;
        *data = theme;
        drop(data);
        self.save()
    }
}

#[cfg(test)]
mod theme_store_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("ram-theme-{name}-{nanos}.ini"))
    }

    struct TestStore {
        store: ThemeStore,
    }

    impl Drop for TestStore {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.store.file_path);
        }
    }

    impl std::ops::Deref for TestStore {
        type Target = ThemeStore;
        fn deref(&self) -> &ThemeStore {
            &self.store
        }
    }

    fn store(name: &str) -> TestStore {
        TestStore {
            store: ThemeStore::new(temp_path(name)),
        }
    }

    fn seeded(name: &str, contents: &str) -> TestStore {
        let path = temp_path(name);
        fs::write(&path, contents).expect("seed theme ini");
        TestStore {
            store: ThemeStore::new(path),
        }
    }

    // ---- defaults ---------------------------------------------------------------

    #[test]
    fn theme_defaults_are_the_dark_zinc_palette_with_google_fonts() {
        let d = ThemeData::default();
        assert_eq!(d.accounts_background, "#09090B");
        assert_eq!(d.accounts_foreground, "#E4E4E7");
        assert_eq!(d.buttons_background, "#27272A");
        assert_eq!(d.buttons_foreground, "#A1A1AA");
        assert_eq!(d.buttons_border, "#3F3F46");
        assert_eq!(d.toggle_on_background, "#0EA5E9");
        assert_eq!(d.toggle_off_background, "#3F3F46");
        assert_eq!(d.toggle_knob_background, "#FFFFFF");
        assert_eq!(d.forms_background, "#09090B");
        assert_eq!(d.forms_foreground, "#E4E4E7");
        assert_eq!(d.textboxes_background, "#18181B");
        assert_eq!(d.textboxes_foreground, "#D4D4D8");
        assert_eq!(d.textboxes_border, "#27272A");
        assert_eq!(d.label_background, "#09090B");
        assert_eq!(d.label_foreground, "#71717A");
        assert!(d.label_transparent);
        assert!(d.dark_top_bar);
        assert!(d.show_headers);
        assert!(!d.light_images);
        assert_eq!(d.button_style, "Flat");
        assert_eq!(d.font_sans, Some(default_font_sans()));
        assert_eq!(d.font_mono, Some(default_font_mono()));
    }

    #[test]
    fn the_default_font_specs_describe_google_families_with_fallbacks() {
        let sans = default_font_sans();
        assert_eq!(sans.source, "google");
        assert_eq!(sans.family, "Outfit");
        assert_eq!(sans.local, None);
        assert_eq!(
            sans.google.as_ref().map(|g| g.weights.clone()),
            Some(vec![300, 400, 500, 600, 700])
        );
        assert!(sans.fallbacks.contains(&"sans-serif".to_string()));

        let mono = default_font_mono();
        assert_eq!(mono.family, "JetBrains Mono");
        assert_eq!(
            mono.google.as_ref().map(|g| g.weights.clone()),
            Some(vec![400, 500])
        );
        assert!(mono.fallbacks.contains(&"monospace".to_string()));
    }

    #[test]
    fn a_missing_theme_file_yields_the_defaults_without_creating_it() {
        let s = store("missing");
        assert_eq!(s.get().unwrap().accounts_background, "#09090B");
        assert!(!s.file_path.exists(), "opening must not write the file");
    }

    #[test]
    fn an_unreadable_theme_file_is_never_overwritten() {
        // `IniFile::load` engole erros de I/O, então um arquivo ilegível virava
        // silenciosamente o tema default e a primeira gravação apagava as cores
        // do usuário. Agora a falha fica latcheada e `save` é recusado.
        let s = store("unreadable");
        // Bytes que não são UTF-8 fazem `read_to_string` falhar.
        let broken = b"[Roblox Account Manager]\nAccountsBG=\xff\xfe\xfd\n";
        fs::write(&s.file_path, broken).unwrap();

        let reopened = ThemeStore::new(s.file_path.clone());
        assert_eq!(
            reopened.get().unwrap().accounts_background,
            "#09090B",
            "em memória ficam os defaults"
        );

        let err = reopened
            .update(ThemeData::default())
            .expect_err("gravar sobre um tema ilegível deve falhar");
        assert!(err.contains("refusing to overwrite"), "{err}");
        assert_eq!(
            fs::read(&s.file_path).unwrap(),
            broken.to_vec(),
            "o arquivo do usuário fica intacto"
        );
    }

    #[test]
    fn a_readable_theme_file_keeps_saving() {
        let s = store("readable-latch");
        s.update(ThemeData::default()).unwrap();

        let reopened = ThemeStore::new(s.file_path.clone());
        let mut theme = ThemeData::default();
        theme.accounts_background = "#123456".to_string();
        reopened.update(theme).expect("um tema legível continua gravável");
        assert_eq!(
            ThemeStore::new(s.file_path.clone())
                .get()
                .unwrap()
                .accounts_background,
            "#123456"
        );
    }

    // ---- save / load round trip ---------------------------------------------------

    #[test]
    fn update_persists_every_field_and_reloads_identically() {
        let s = store("roundtrip");
        let mut theme = ThemeData::default();
        theme.accounts_background = "#101010".to_string();
        theme.accounts_foreground = "#FAFAFA".to_string();
        theme.buttons_background = "#202020".to_string();
        theme.buttons_foreground = "#303030".to_string();
        theme.buttons_border = "#404040".to_string();
        theme.toggle_on_background = "#111111".to_string();
        theme.toggle_off_background = "#222222".to_string();
        theme.toggle_knob_background = "#333333".to_string();
        theme.forms_background = "#444444".to_string();
        theme.forms_foreground = "#555555".to_string();
        theme.textboxes_background = "#666666".to_string();
        theme.textboxes_foreground = "#777777".to_string();
        theme.textboxes_border = "#888888".to_string();
        theme.label_background = "#999999".to_string();
        theme.label_foreground = "#AAAAAA".to_string();
        theme.label_transparent = false;
        theme.dark_top_bar = false;
        theme.show_headers = false;
        theme.light_images = true;
        theme.button_style = "Raised".to_string();
        theme.font_sans = Some(ThemeFontSpec {
            source: "local".to_string(),
            family: "My Font".to_string(),
            fallbacks: vec!["serif".to_string()],
            google: None,
            local: Some(ThemeFontLocalSpec {
                file: "abc123.ttf".to_string(),
                weight: 500,
                style: "normal".to_string(),
            }),
        });

        s.update(theme.clone()).unwrap();
        assert!(s.file_path.exists());

        let reloaded = ThemeStore::new(s.file_path.clone()).get().unwrap();
        assert_eq!(reloaded.accounts_background, "#101010");
        assert_eq!(reloaded.buttons_border, "#404040");
        assert_eq!(reloaded.toggle_on_background, "#111111");
        assert_eq!(reloaded.toggle_knob_background, "#333333");
        assert_eq!(reloaded.textboxes_border, "#888888");
        assert_eq!(reloaded.label_foreground, "#AAAAAA");
        assert!(!reloaded.label_transparent);
        assert!(!reloaded.dark_top_bar);
        assert!(!reloaded.show_headers);
        assert!(reloaded.light_images);
        assert_eq!(reloaded.button_style, "Raised");
        assert_eq!(reloaded.font_sans, theme.font_sans);
        assert_eq!(reloaded.font_mono, Some(default_font_mono()));
    }

    #[test]
    fn save_writes_the_legacy_ini_key_names() {
        let s = store("keys");
        s.update(ThemeData::default()).unwrap();
        let raw = fs::read_to_string(&s.file_path).unwrap();

        assert!(raw.contains("[Roblox Account Manager]"), "{raw}");
        for key in [
            "AccountsBG",
            "AccountsFG",
            "ButtonsBG",
            "ButtonsFG",
            "ButtonsBC",
            "ToggleOnBG",
            "ToggleOffBG",
            "ToggleKnobBG",
            "FormsBG",
            "FormsFG",
            "TextBoxesBG",
            "TextBoxesFG",
            "TextBoxesBC",
            "LabelsBC",
            "LabelsFC",
            "LabelsTransparent",
            "DarkTopBar",
            "ShowHeaders",
            "LightImages",
            "ButtonStyle",
            "FontSans",
            "FontMono",
        ] {
            assert!(raw.contains(&format!("{key}=")), "missing {key} in\n{raw}");
        }
    }

    // ---- partial / legacy files -----------------------------------------------------

    #[test]
    fn only_the_keys_present_in_the_file_override_the_defaults() {
        let s = seeded(
            "partial",
            "[Roblox Account Manager]\nAccountsBG=#123456\nButtonStyle=Outline\n",
        );
        let theme = s.get().unwrap();
        assert_eq!(theme.accounts_background, "#123456");
        assert_eq!(theme.button_style, "Outline");
        // Everything else keeps its default.
        assert_eq!(theme.accounts_foreground, "#E4E4E7");
        assert_eq!(theme.toggle_on_background, "#0EA5E9");
        assert_eq!(theme.font_sans, Some(default_font_sans()));
    }

    #[test]
    fn a_legacy_rbx_alt_manager_theme_file_is_still_read() {
        let s = seeded(
            "legacy",
            "[RBX Alt Manager]\nAccountsBG=#ABCDEF\nDarkTopBar=false\n",
        );
        let theme = s.get().unwrap();
        assert_eq!(theme.accounts_background, "#ABCDEF");
        assert!(!theme.dark_top_bar);
    }

    #[test]
    fn theme_booleans_are_parsed_case_insensitively_and_anything_else_is_false() {
        let s = seeded(
            "booleans",
            "[Roblox Account Manager]\nLabelsTransparent=TRUE\nDarkTopBar=True\n\
             ShowHeaders=yes\nLightImages=TrUe\n",
        );
        let theme = s.get().unwrap();
        assert!(theme.label_transparent);
        assert!(theme.dark_top_bar);
        assert!(!theme.show_headers, "only true/false are recognised");
        assert!(theme.light_images);
    }

    #[test]
    fn an_unparseable_font_spec_falls_back_to_the_default_instead_of_failing() {
        let s = seeded(
            "bad-font",
            "[Roblox Account Manager]\nFontSans={not json}\nFontMono={\"source\":\"google\"}\n",
        );
        let theme = s.get().unwrap();
        assert_eq!(theme.font_sans, Some(default_font_sans()));
        assert_eq!(
            theme.font_mono,
            Some(default_font_mono()),
            "an incomplete spec is rejected too"
        );
    }

    #[test]
    fn a_theme_file_without_a_known_section_is_ignored() {
        let s = seeded("wrong-section", "[Other]\nAccountsBG=#000000\n");
        assert_eq!(s.get().unwrap().accounts_background, "#09090B");
    }

    // ---- serde -----------------------------------------------------------------------

    #[test]
    fn theme_data_fills_in_the_toggle_colors_when_the_payload_predates_them() {
        let json = serde_json::to_value(&ThemeData::default()).unwrap();
        let mut object = json.as_object().unwrap().clone();
        object.remove("toggle_on_background");
        object.remove("toggle_off_background");
        object.remove("toggle_knob_background");

        let parsed: ThemeData =
            serde_json::from_value(serde_json::Value::Object(object)).expect("legacy payload");
        assert_eq!(parsed.toggle_on_background, "#0EA5E9");
        assert_eq!(parsed.toggle_off_background, "#3F3F46");
        assert_eq!(parsed.toggle_knob_background, "#FFFFFF");
    }

    #[test]
    fn theme_data_requires_its_non_defaulted_colors() {
        assert!(serde_json::from_str::<ThemeData>("{}").is_err());
        let mut object = serde_json::to_value(&ThemeData::default())
            .unwrap()
            .as_object()
            .unwrap()
            .clone();
        object.remove("accounts_background");
        assert!(serde_json::from_value::<ThemeData>(serde_json::Value::Object(object)).is_err());
    }

    #[test]
    fn font_specs_omit_the_unused_source_variant_when_serialized() {
        let json = serde_json::to_value(&default_font_sans()).unwrap();
        assert!(json.get("google").is_some());
        assert!(json.get("local").is_none(), "None is skipped: {json}");

        let local = ThemeFontSpec {
            source: "local".to_string(),
            family: "Local".to_string(),
            fallbacks: vec![],
            google: None,
            local: Some(ThemeFontLocalSpec {
                file: "f.woff2".to_string(),
                weight: 400,
                style: "italic".to_string(),
            }),
        };
        let json = serde_json::to_value(&local).unwrap();
        assert!(json.get("google").is_none());
        assert_eq!(json["local"]["file"], "f.woff2");

        // Round trip through the string form used inside the INI.
        let text = serde_json::to_string(&local).unwrap();
        assert_eq!(serde_json::from_str::<ThemeFontSpec>(&text).unwrap(), local);
    }
}
