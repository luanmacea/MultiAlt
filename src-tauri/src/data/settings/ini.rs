#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IniProperty {
    pub name: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IniSection {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    properties: Vec<IniProperty>,
}

impl IniSection {
    fn new(name: String) -> Self {
        Self {
            name,
            comment: None,
            properties: Vec::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|p| p.name == key)
            .map(|p| p.value.as_str())
    }

    #[allow(dead_code)]
    pub fn get_bool(&self, key: &str) -> bool {
        self.get(key).map(|v| v == "true").unwrap_or(false)
    }

    #[allow(dead_code)]
    pub fn get_int(&self, key: &str) -> i64 {
        self.get(key).and_then(|v| v.parse().ok()).unwrap_or(0)
    }

    #[allow(dead_code)]
    pub fn get_float(&self, key: &str) -> f64 {
        self.get(key).and_then(|v| v.parse().ok()).unwrap_or(0.0)
    }

    pub fn exists(&self, key: &str) -> bool {
        self.properties.iter().any(|p| p.name == key)
    }

    pub fn set(&mut self, key: &str, value: &str, comment: Option<&str>) {
        if value.trim().is_empty() {
            self.remove(key);
            return;
        }

        if let Some(prop) = self.properties.iter_mut().find(|p| p.name == key) {
            prop.value = value.to_string();
            if let Some(c) = comment {
                prop.comment = Some(c.to_string());
            }
        } else {
            self.properties.push(IniProperty {
                name: key.to_string(),
                value: value.to_string(),
                comment: comment.map(|c| c.to_string()),
            });
        }
    }

    pub fn remove(&mut self, key: &str) {
        self.properties.retain(|p| p.name != key);
    }

    pub fn to_map(&self) -> HashMap<String, String> {
        self.properties
            .iter()
            .map(|p| (p.name.clone(), p.value.clone()))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct IniFile {
    sections: Vec<IniSection>,
    write_spacing: bool,
    comment_char: char,
}

impl IniFile {
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
            write_spacing: false,
            comment_char: '#',
        }
    }

    pub fn load(path: &Path) -> Self {
        let mut ini = Self::new();
        if let Ok(data) = fs::read_to_string(path) {
            ini.parse(&data);
        }
        ini
    }

    fn parse(&mut self, content: &str) {
        let mut current_section: Option<usize> = None;

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.is_empty() {
                continue;
            }

            if trimmed.starts_with(';') || trimmed.starts_with('#') {
                continue;
            }

            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                let mut section_name = trimmed[1..trimmed.len() - 1].to_string();
                if section_name == "RBX Alt Manager" {
                    section_name = "Roblox Account Manager".to_string();
                }
                if !self.sections.iter().any(|s| s.name == section_name) {
                    self.sections.push(IniSection::new(section_name.clone()));
                }
                current_section = self.sections.iter().position(|s| s.name == section_name);
                continue;
            }

            if let Some(idx) = current_section {
                if let Some(eq_pos) = trimmed.find('=') {
                    let key = trimmed[..eq_pos].trim();
                    let value = trimmed[eq_pos + 1..].trim();
                    if !key.is_empty() && !value.is_empty() {
                        self.sections[idx].set(key, value, None);
                    }
                }
            }
        }
    }

    pub fn section(&mut self, name: &str) -> &mut IniSection {
        if !self.sections.iter().any(|s| s.name == name) {
            self.sections.push(IniSection::new(name.to_string()));
        }
        self.sections.iter_mut().find(|s| s.name == name).unwrap()
    }

    pub fn get_section(&self, name: &str) -> Option<&IniSection> {
        self.sections.iter().find(|s| s.name == name)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let mut output = String::new();

        for section in &self.sections {
            if section.properties.is_empty() {
                continue;
            }

            if let Some(ref comment) = section.comment {
                output.push_str(&format!("{} {}\n", self.comment_char, comment));
            }

            output.push_str(&format!("[{}]\n", section.name));

            for prop in &section.properties {
                if let Some(ref comment) = prop.comment {
                    output.push_str(&format!("{} {}\n", self.comment_char, comment));
                }

                if self.write_spacing {
                    output.push_str(&format!("{} = {}\n", prop.name, prop.value));
                } else {
                    output.push_str(&format!("{}={}\n", prop.name, prop.value));
                }
            }

            output.push('\n');
        }

        fs::write(path, output).map_err(|e| format!("Failed to save INI file: {}", e))
    }

    pub fn to_map(&self) -> HashMap<String, HashMap<String, String>> {
        self.sections
            .iter()
            .map(|s| (s.name.clone(), s.to_map()))
            .collect()
    }
}

#[cfg(test)]
mod ini_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn parsed(content: &str) -> IniFile {
        let mut ini = IniFile::new();
        ini.parse(content);
        ini
    }

    fn unique_ini_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("ram-ini-{name}-{nanos}.ini"))
    }

    // ---- parse ---------------------------------------------------------------

    #[test]
    fn parse_keeps_sections_and_trims_keys_and_values() {
        let ini = parsed("[General]\n   Language   =   pt-BR   \n[WebServer]\nPort=7963\n");
        let general = ini.get_section("General").expect("General section");
        assert_eq!(general.get("Language"), Some("pt-BR"));
        assert_eq!(
            ini.get_section("WebServer").and_then(|s| s.get("Port")),
            Some("7963")
        );
        assert!(ini.get_section("Missing").is_none());
    }

    #[test]
    fn parse_ignores_comments_and_blank_lines() {
        let ini = parsed(
            "; a semicolon comment\n\
             # a hash comment\n\
             \n\
             [General]\n\
             \n\
             ; Language=ignored\n\
             # Language=also-ignored\n\
             Language=en\n\
             \n",
        );
        let general = ini.get_section("General").expect("General section");
        assert_eq!(general.get("Language"), Some("en"));
        assert_eq!(general.to_map().len(), 1);
    }

    #[test]
    fn parse_ignores_keys_before_any_section_and_empty_values() {
        let ini = parsed("Orphan=1\n[General]\nEmpty=\nReal=2\n");
        let general = ini.get_section("General").expect("General section");
        assert!(!general.exists("Orphan"));
        assert!(!general.exists("Empty"));
        assert_eq!(general.get("Real"), Some("2"));
    }

    #[test]
    fn parse_keeps_everything_after_the_first_equals_sign() {
        let ini = parsed("[General]\nUrl=https://x/y?a=1&b=2\n");
        assert_eq!(
            ini.get_section("General").and_then(|s| s.get("Url")),
            Some("https://x/y?a=1&b=2")
        );
    }

    #[test]
    fn parse_duplicate_keys_keep_the_last_value() {
        let ini = parsed("[General]\nLanguage=en\nLanguage=pt\n");
        let general = ini.get_section("General").expect("General section");
        assert_eq!(general.get("Language"), Some("pt"));
        assert_eq!(general.to_map().len(), 1);
    }

    #[test]
    fn parse_merges_a_section_declared_twice() {
        let ini = parsed("[General]\na=1\n[Other]\nb=2\n[General]\nc=3\n");
        let general = ini.get_section("General").expect("General section");
        assert_eq!(general.get("a"), Some("1"));
        assert_eq!(general.get("c"), Some("3"));
    }

    #[test]
    fn parse_renames_the_legacy_rbx_alt_manager_section() {
        let ini = parsed("[RBX Alt Manager]\nLanguage=en\n");
        assert!(ini.get_section("RBX Alt Manager").is_none());
        assert_eq!(
            ini.get_section("Roblox Account Manager")
                .and_then(|s| s.get("Language")),
            Some("en")
        );
    }

    // ---- typed getters -------------------------------------------------------

    #[test]
    fn get_bool_is_true_only_for_the_literal_true() {
        let ini = parsed("[General]\nYes=true\nNo=false\nOdd=True\nNum=1\n");
        let general = ini.get_section("General").expect("General section");
        assert!(general.get_bool("Yes"));
        assert!(!general.get_bool("No"));
        assert!(!general.get_bool("Odd"));
        assert!(!general.get_bool("Num"));
        assert!(!general.get_bool("Missing"));
    }

    #[test]
    fn get_int_falls_back_to_zero() {
        let ini = parsed("[General]\nGood=42\nNegative=-7\nBad=abc\nFloat=1.5\n");
        let general = ini.get_section("General").expect("General section");
        assert_eq!(general.get_int("Good"), 42);
        assert_eq!(general.get_int("Negative"), -7);
        assert_eq!(general.get_int("Bad"), 0);
        assert_eq!(general.get_int("Float"), 0);
        assert_eq!(general.get_int("Missing"), 0);
    }

    #[test]
    fn get_float_falls_back_to_zero() {
        let ini = parsed("[General]\nGood=0.75\nInt=2\nBad=abc\n");
        let general = ini.get_section("General").expect("General section");
        assert!((general.get_float("Good") - 0.75).abs() < f64::EPSILON);
        assert!((general.get_float("Int") - 2.0).abs() < f64::EPSILON);
        assert_eq!(general.get_float("Bad"), 0.0);
        assert_eq!(general.get_float("Missing"), 0.0);
    }

    // ---- set / remove --------------------------------------------------------

    #[test]
    fn set_overwrites_an_existing_key_without_duplicating_it() {
        let mut ini = IniFile::new();
        let general = ini.section("General");
        general.set("Language", "en", None);
        general.set("Language", "pt", None);
        assert_eq!(general.get("Language"), Some("pt"));
        assert_eq!(general.to_map().len(), 1);
    }

    #[test]
    fn set_with_a_blank_value_removes_the_key() {
        let mut ini = IniFile::new();
        let general = ini.section("General");
        general.set("Language", "en", None);
        general.set("Language", "   ", None);
        assert!(!general.exists("Language"));
        general.remove("Language");
        assert!(!general.exists("Language"));
    }

    // ---- save / load round trip ---------------------------------------------

    #[test]
    fn set_save_load_round_trips_through_a_real_file() {
        let path = unique_ini_path("roundtrip");

        {
            let mut ini = IniFile::new();
            ini.section("General").set("Language", "pt-BR", None);
            ini.section("General")
                .set("MaxRecentGames", "8", Some("how many recents to keep"));
            ini.section("WebServer").set("Password", "sup3rsecret", None);
            // An empty section must not reach the file.
            ini.section("Empty");
            ini.save(&path).expect("save");
        }

        let raw = fs::read_to_string(&path).expect("read back");
        assert!(raw.contains("[General]"));
        assert!(raw.contains("Language=pt-BR"));
        assert!(raw.contains("# how many recents to keep"));
        assert!(!raw.contains("[Empty]"));

        let reloaded = IniFile::load(&path);
        assert_eq!(
            reloaded.get_section("General").and_then(|s| s.get("Language")),
            Some("pt-BR")
        );
        assert_eq!(
            reloaded
                .get_section("WebServer")
                .and_then(|s| s.get("Password")),
            Some("sup3rsecret")
        );
        assert!(reloaded.get_section("Empty").is_none());

        let map = reloaded.to_map();
        assert_eq!(map["General"]["MaxRecentGames"], "8");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn load_of_a_missing_file_yields_an_empty_ini() {
        let path = unique_ini_path("missing");
        assert!(!path.exists());
        let ini = IniFile::load(&path);
        assert!(ini.to_map().is_empty());
    }
}
