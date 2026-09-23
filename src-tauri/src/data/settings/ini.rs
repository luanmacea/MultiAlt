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

#[cfg(test)]
mod ini_structure_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn parsed(content: &str) -> IniFile {
        let mut ini = IniFile::new();
        ini.parse(content);
        ini
    }

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("ram-inistruct-{name}-{nanos}.ini"))
    }

    // ---- IniFile::new / section ------------------------------------------------

    #[test]
    fn a_new_ini_file_is_empty_and_writes_without_spacing() {
        let ini = IniFile::new();
        assert!(ini.to_map().is_empty());
        assert!(!ini.write_spacing);
        assert_eq!(ini.comment_char, '#');
    }

    #[test]
    fn section_creates_once_and_then_returns_the_same_section() {
        let mut ini = IniFile::new();
        ini.section("General").set("A", "1", None);
        ini.section("General").set("B", "2", None);
        ini.section("Other").set("A", "3", None);

        let map = ini.to_map();
        assert_eq!(map.len(), 2);
        assert_eq!(map["General"].len(), 2);
        assert_eq!(map["General"]["A"], "1");
        assert_eq!(map["Other"]["A"], "3");
    }

    #[test]
    fn get_section_sees_a_section_created_through_section_even_while_empty() {
        let mut ini = IniFile::new();
        ini.section("Prompts");
        let section = ini.get_section("Prompts").expect("section exists in memory");
        assert!(section.to_map().is_empty());
        assert!(!section.exists("anything"));
        assert_eq!(section.get("anything"), None);
        assert!(ini.to_map().contains_key("Prompts"));
    }

    #[test]
    fn section_names_are_case_sensitive() {
        let mut ini = IniFile::new();
        ini.section("General").set("A", "1", None);
        ini.section("general").set("A", "2", None);
        let map = ini.to_map();
        assert_eq!(map.len(), 2);
        assert_eq!(map["General"]["A"], "1");
        assert_eq!(map["general"]["A"], "2");
    }

    // ---- parse edge cases --------------------------------------------------------

    #[test]
    fn parse_ignores_lines_without_an_equals_sign() {
        let ini = parsed("[General]\njust-a-word\nReal=1\n");
        let general = ini.get_section("General").expect("General");
        assert_eq!(general.to_map().len(), 1);
        assert_eq!(general.get("Real"), Some("1"));
    }

    #[test]
    fn parse_drops_a_line_whose_key_is_empty() {
        let ini = parsed("[General]\n=orphan value\n   =also orphan\nReal=1\n");
        let general = ini.get_section("General").expect("General");
        assert_eq!(general.to_map().len(), 1);
        assert!(!general.exists(""));
    }

    #[test]
    fn parse_accepts_an_empty_section_header_as_a_section_named_empty_string() {
        let ini = parsed("[]\nKey=value\n");
        let map = ini.to_map();
        assert_eq!(map[""]["Key"], "value");
    }

    #[test]
    fn parse_only_treats_a_line_as_a_header_when_it_starts_and_ends_with_brackets() {
        // A malformed header is not a header; without '=' the line is dropped
        // and its keys stay in whatever section came before.
        let ini = parsed("[General]\nA=1\n[Broken\nB=2\nAlso]\nC=3\n");
        let general = ini.get_section("General").expect("General");
        assert_eq!(general.get("A"), Some("1"));
        assert_eq!(general.get("B"), Some("2"), "B lands in [General]");
        assert_eq!(general.get("C"), Some("3"));
        assert!(ini.get_section("Broken").is_none());
    }

    #[test]
    fn parse_trims_whitespace_around_the_section_header() {
        let ini = parsed("   [General]   \nA=1\n");
        assert_eq!(ini.get_section("General").and_then(|s| s.get("A")), Some("1"));
    }

    #[test]
    fn parse_only_skips_comments_that_start_the_trimmed_line() {
        let ini = parsed("[General]\nUrl=http://x#anchor\nOther=1 ; trailing\n");
        let general = ini.get_section("General").expect("General");
        assert_eq!(
            general.get("Url"),
            Some("http://x#anchor"),
            "an inline # is part of the value"
        );
        assert_eq!(general.get("Other"), Some("1 ; trailing"));
    }

    #[test]
    fn parse_handles_crlf_line_endings() {
        let ini = parsed("[General]\r\nLanguage=en\r\n[WebServer]\r\nWebServerPort=7963\r\n");
        assert_eq!(
            ini.get_section("General").and_then(|s| s.get("Language")),
            Some("en")
        );
        assert_eq!(
            ini.get_section("WebServer").and_then(|s| s.get("WebServerPort")),
            Some("7963")
        );
    }

    #[test]
    fn the_legacy_section_rename_merges_into_an_existing_modern_section() {
        let ini = parsed("[Roblox Account Manager]\nA=1\n[RBX Alt Manager]\nB=2\n");
        let map = ini.to_map();
        assert_eq!(map.len(), 1);
        assert_eq!(map["Roblox Account Manager"]["A"], "1");
        assert_eq!(map["Roblox Account Manager"]["B"], "2");
    }

    // ---- IniSection::set / remove --------------------------------------------------

    #[test]
    fn set_keeps_insertion_order_and_remove_is_tolerant_of_unknown_keys() {
        let mut ini = IniFile::new();
        let general = ini.section("General");
        general.set("First", "1", None);
        general.set("Second", "2", None);
        general.set("Third", "3", None);
        general.remove("NotThere");
        general.remove("Second");
        general.set("Fourth", "4", None);

        let path = temp_path("order");
        ini.save(&path).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        let body = raw.trim().lines().collect::<Vec<_>>();
        assert_eq!(body, vec!["[General]", "First=1", "Third=3", "Fourth=4"]);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn set_only_replaces_a_comment_when_a_new_one_is_supplied() {
        let mut ini = IniFile::new();
        let general = ini.section("General");
        general.set("Key", "1", Some("first comment"));
        general.set("Key", "2", None);

        let path = temp_path("comments");
        ini.save(&path).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("# first comment"), "{raw}");
        assert!(raw.contains("Key=2"), "{raw}");

        ini.section("General").set("Key", "3", Some("second comment"));
        ini.save(&path).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("# second comment"), "{raw}");
        assert!(!raw.contains("# first comment"), "{raw}");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn a_blank_value_removes_the_key_even_when_it_never_existed() {
        let mut ini = IniFile::new();
        let general = ini.section("General");
        general.set("Never", "", None);
        general.set("Also", "\t \n", None);
        assert!(general.to_map().is_empty());
    }

    #[test]
    fn a_value_that_is_only_visually_blank_is_stored_verbatim() {
        let mut ini = IniFile::new();
        let general = ini.section("General");
        general.set("Padded", "  x  ", None);
        assert_eq!(
            general.get("Padded"),
            Some("  x  "),
            "set does not trim a non-blank value"
        );
    }

    #[test]
    fn section_comments_are_written_above_the_header() {
        let mut ini = IniFile::new();
        ini.section("General").set("A", "1", None);
        ini.section("General").comment = Some("a section comment".to_string());

        let path = temp_path("section-comment");
        ini.save(&path).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.starts_with("# a section comment\n[General]\n"), "{raw}");

        // Comments are not read back: parse() skips comment lines entirely.
        let reloaded = IniFile::load(&path);
        assert!(reloaded.get_section("General").unwrap().comment.is_none());

        let _ = fs::remove_file(&path);
    }

    // ---- save ---------------------------------------------------------------------

    #[test]
    fn save_replaces_the_whole_file_rather_than_appending() {
        let path = temp_path("replace");
        fs::write(&path, "[Stale]\nOld=1\n").unwrap();

        let mut ini = IniFile::new();
        ini.section("Fresh").set("New", "1", None);
        ini.save(&path).unwrap();

        let raw = fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("[Stale]"), "{raw}");
        assert!(raw.contains("[Fresh]"), "{raw}");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn save_of_an_ini_with_no_content_produces_an_empty_file() {
        let path = temp_path("empty-save");
        let mut ini = IniFile::new();
        ini.section("OnlyEmptySections");
        ini.save(&path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
        assert!(IniFile::load(&path).to_map().is_empty());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn save_reports_the_io_error_instead_of_panicking() {
        let dir = temp_path("as-dir");
        fs::create_dir_all(&dir).unwrap();
        let mut ini = IniFile::new();
        ini.section("General").set("A", "1", None);
        let err = ini.save(&dir).expect_err("writing over a directory must fail");
        assert!(err.starts_with("Failed to save INI file:"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_saved_file_reloads_into_an_equal_map() {
        let path = temp_path("equal-map");
        let mut ini = IniFile::new();
        ini.section("General").set("Language", "pt-BR", None);
        ini.section("General").set("Url", "https://x/y?a=1&b=2", None);
        ini.section("WebServer").set("WebServerPort", "7963", None);
        ini.save(&path).unwrap();

        assert_eq!(IniFile::load(&path).to_map(), ini.to_map());
        let _ = fs::remove_file(&path);
    }

    // ---- IniSection typed getters on odd input ---------------------------------------

    #[test]
    fn typed_getters_cope_with_out_of_range_and_signed_values() {
        let ini = parsed(
            "[General]\nHuge=99999999999999999999\nMax=9223372036854775807\n\
             Neg=-1\nSci=1e3\nInf=inf\nNan=nan\n",
        );
        let general = ini.get_section("General").expect("General");
        assert_eq!(general.get_int("Huge"), 0, "an i64 overflow falls back to 0");
        assert_eq!(general.get_int("Max"), i64::MAX);
        assert_eq!(general.get_int("Neg"), -1);
        assert_eq!(general.get_int("Sci"), 0);
        assert_eq!(general.get_float("Sci"), 1000.0);
        assert!(general.get_float("Inf").is_infinite());
        assert!(general.get_float("Nan").is_nan());
    }

    #[test]
    fn to_map_snapshots_the_current_values() {
        let mut ini = IniFile::new();
        ini.section("General").set("A", "1", None);
        let first = ini.to_map();
        ini.section("General").set("A", "2", None);
        assert_eq!(first["General"]["A"], "1", "the snapshot does not alias");
        assert_eq!(ini.to_map()["General"]["A"], "2");
    }
}
