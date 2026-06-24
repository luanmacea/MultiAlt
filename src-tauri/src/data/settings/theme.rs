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
}

impl ThemeStore {
    pub fn new(file_path: PathBuf) -> Self {
        let data = if file_path.exists() {
            Self::load_from_file(&file_path)
        } else {
            ThemeData::default()
        };

        Self {
            data: Mutex::new(data),
            file_path,
        }
    }

    fn load_from_file(path: &Path) -> ThemeData {
        let ini = IniFile::load(path);
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

        data
    }

    pub fn get(&self) -> Result<ThemeData, String> {
        let data = self.data.lock().map_err(|e| e.to_string())?;
        Ok(data.clone())
    }

    pub fn save(&self) -> Result<(), String> {
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
