use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[serde(rename_all = "camelCase")]
pub struct ScriptPermissions {
    pub allow_invoke: bool,
    pub allow_http: bool,
    #[serde(rename = "allowWebSocket", alias = "allowWebsocket")]
    pub allow_websocket: bool,
    pub allow_window: bool,
    pub allow_modal: bool,
    pub allow_settings: bool,
    pub allow_ui: bool,
}

impl Default for ScriptPermissions {
    fn default() -> Self {
        Self {
            allow_invoke: false,
            allow_http: false,
            allow_websocket: false,
            allow_window: false,
            allow_modal: false,
            allow_settings: false,
            allow_ui: false,
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn default_language() -> String {
    "javascript".to_string()
}

fn default_enabled() -> bool {
    true
}

const MAX_SCRIPTS_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_SCRIPT_COUNT: usize = 256;
const MAX_SCRIPT_ID_CHARS: usize = 96;
const MAX_SCRIPT_NAME_CHARS: usize = 120;
const MAX_SCRIPT_DESCRIPTION_CHARS: usize = 2048;
const MAX_SCRIPT_LANGUAGE_CHARS: usize = 32;
const MAX_SCRIPT_SOURCE_BYTES: usize = 262_144;

fn contains_disallowed_control_chars(value: &str) -> bool {
    value
        .chars()
        .any(|ch| ch.is_control() && ch != '\n' && ch != '\r' && ch != '\t')
}

fn validate_script_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("Script id is required".to_string());
    }
    if id.len() > MAX_SCRIPT_ID_CHARS {
        return Err(format!(
            "Script id exceeds {} characters",
            MAX_SCRIPT_ID_CHARS
        ));
    }
    if !id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.')
    {
        return Err(
            "Script id contains unsupported characters (allowed: a-z, A-Z, 0-9, -, _, .)"
                .to_string(),
        );
    }
    Ok(())
}

fn validate_script_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Script name is required".to_string());
    }
    if name.chars().count() > MAX_SCRIPT_NAME_CHARS {
        return Err(format!(
            "Script name exceeds {} characters",
            MAX_SCRIPT_NAME_CHARS
        ));
    }
    if contains_disallowed_control_chars(name) {
        return Err("Script name contains unsupported control characters".to_string());
    }
    Ok(())
}

fn validate_script_description(description: &str) -> Result<(), String> {
    if description.chars().count() > MAX_SCRIPT_DESCRIPTION_CHARS {
        return Err(format!(
            "Script description exceeds {} characters",
            MAX_SCRIPT_DESCRIPTION_CHARS
        ));
    }
    if contains_disallowed_control_chars(description) {
        return Err("Script description contains unsupported control characters".to_string());
    }
    Ok(())
}

fn normalize_script_language(language: &str) -> String {
    let lowered = language.trim().to_ascii_lowercase();
    if lowered.is_empty() {
        return default_language();
    }
    if lowered == "js" || lowered == "javascript" {
        return "javascript".to_string();
    }
    "javascript".to_string()
}

fn validate_script_source(source: &str) -> Result<(), String> {
    if source.len() > MAX_SCRIPT_SOURCE_BYTES {
        return Err(format!(
            "Script source exceeds {} bytes",
            MAX_SCRIPT_SOURCE_BYTES
        ));
    }
    if source.contains('\0') {
        return Err("Script source contains null bytes".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedScript {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub source: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub trusted: bool,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub permissions: ScriptPermissions,
    #[serde(default = "now_ms")]
    pub created_at_ms: i64,
    #[serde(default = "now_ms")]
    pub updated_at_ms: i64,
}

pub struct ScriptStore {
    scripts: Mutex<Vec<ManagedScript>>,
    file_path: PathBuf,
    /// Set when the file exists but could not be read (corrupt JSON, over the
    /// size limit). While set, saving is refused so an empty in-memory list
    /// never overwrites the user's scripts — same guard as `AccountStore`.
    load_failed: std::sync::atomic::AtomicBool,
}

impl ScriptStore {
    pub fn new(file_path: PathBuf) -> Self {
        let store = Self {
            scripts: Mutex::new(Vec::new()),
            file_path,
            load_failed: std::sync::atomic::AtomicBool::new(false),
        };
        if store.file_path.exists() && store.load_from_disk().is_err() {
            store
                .load_failed
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
        store
    }

    fn load_from_disk(&self) -> Result<(), String> {
        if !self.file_path.exists() {
            return Ok(());
        }

        let metadata = fs::metadata(&self.file_path)
            .map_err(|e| format!("Failed to read scripts file metadata: {}", e))?;
        if metadata.len() > MAX_SCRIPTS_FILE_BYTES {
            return Err(format!(
                "Scripts file is too large (max {} bytes)",
                MAX_SCRIPTS_FILE_BYTES
            ));
        }

        let data =
            fs::read(&self.file_path).map_err(|e| format!("Failed to read scripts file: {}", e))?;
        if data.is_empty() {
            return Ok(());
        }

        let parsed = serde_json::from_slice::<Vec<ManagedScript>>(&data)
            .map_err(|e| format!("Failed to parse scripts file: {}", e))?;

        let mut scripts = self.scripts.lock().map_err(|e| e.to_string())?;
        *scripts = parsed;
        Ok(())
    }

    fn save_to_disk(&self) -> Result<(), String> {
        if self.load_failed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(
                "Scripts file could not be read; refusing to overwrite it. Fix or restore RAMScripts.json and restart.".to_string(),
            );
        }

        let scripts = self.scripts.lock().map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec_pretty(&*scripts)
            .map_err(|e| format!("Failed to serialize scripts: {}", e))?;
        fs::write(&self.file_path, bytes)
            .map_err(|e| format!("Failed to write scripts file: {}", e))?;
        Ok(())
    }

    pub fn get_all(&self) -> Result<Vec<ManagedScript>, String> {
        let mut scripts = self.scripts.lock().map_err(|e| e.to_string())?.clone();
        scripts.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(scripts)
    }

    pub fn upsert(&self, mut script: ManagedScript) -> Result<ManagedScript, String> {
        let id = script.id.trim();
        validate_script_id(id)?;
        script.id = id.to_string();

        let name = script.name.trim();
        validate_script_name(name)?;
        script.name = name.to_string();

        script.description = script.description.trim().to_string();
        validate_script_description(&script.description)?;

        if script.language.chars().count() > MAX_SCRIPT_LANGUAGE_CHARS {
            return Err(format!(
                "Script language exceeds {} characters",
                MAX_SCRIPT_LANGUAGE_CHARS
            ));
        }
        script.language = normalize_script_language(&script.language);

        validate_script_source(&script.source)?;

        let mut scripts = self.scripts.lock().map_err(|e| e.to_string())?;
        let now = now_ms();

        if let Some(existing) = scripts.iter_mut().find(|s| s.id == script.id) {
            let created = existing.created_at_ms;
            *existing = script;
            existing.created_at_ms = created;
            existing.updated_at_ms = now;
            let out = existing.clone();
            drop(scripts);
            self.save_to_disk()?;
            return Ok(out);
        }

        if scripts.len() >= MAX_SCRIPT_COUNT {
            return Err(format!("Script limit reached (max {})", MAX_SCRIPT_COUNT));
        }

        script.created_at_ms = now;
        script.updated_at_ms = now;
        scripts.push(script.clone());
        drop(scripts);
        self.save_to_disk()?;
        Ok(script)
    }

    pub fn remove(&self, script_id: &str) -> Result<bool, String> {
        let normalized_id = script_id.trim();
        if normalized_id.is_empty() {
            return Err("Script id is required".to_string());
        }

        let mut scripts = self.scripts.lock().map_err(|e| e.to_string())?;
        let before = scripts.len();
        scripts.retain(|s| s.id != normalized_id);
        let removed = scripts.len() < before;
        drop(scripts);
        if removed {
            self.save_to_disk()?;
        }
        Ok(removed)
    }
}

#[tauri::command]
pub fn get_scripts(state: tauri::State<'_, ScriptStore>) -> Result<Vec<ManagedScript>, String> {
    state.get_all()
}

#[tauri::command]
pub fn save_script(
    state: tauri::State<'_, ScriptStore>,
    script: ManagedScript,
) -> Result<ManagedScript, String> {
    state.upsert(script)
}

#[tauri::command]
pub fn delete_script(
    state: tauri::State<'_, ScriptStore>,
    script_id: String,
) -> Result<bool, String> {
    state.remove(&script_id)
}

#[cfg(test)]
mod scripts_store_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("ram-scripts-{name}-{nanos}.json"))
    }

    struct TestStore {
        store: ScriptStore,
    }

    impl Drop for TestStore {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.store.file_path);
        }
    }

    impl std::ops::Deref for TestStore {
        type Target = ScriptStore;
        fn deref(&self) -> &ScriptStore {
            &self.store
        }
    }

    fn store(name: &str) -> TestStore {
        TestStore {
            store: ScriptStore::new(temp_path(name)),
        }
    }

    fn script(id: &str, name: &str) -> ManagedScript {
        ManagedScript {
            id: id.to_string(),
            name: name.to_string(),
            description: String::new(),
            language: "javascript".to_string(),
            source: "ram.info('ok');".to_string(),
            enabled: true,
            trusted: false,
            auto_start: false,
            permissions: ScriptPermissions::default(),
            created_at_ms: 0,
            updated_at_ms: 0,
        }
    }

    // ---- id validation --------------------------------------------------------

    #[test]
    fn validate_script_id_accepts_the_documented_character_set() {
        for id in [
            "a",
            "A",
            "0",
            "my-script",
            "my_script",
            "my.script.v2",
            "-_.",
            &"a".repeat(MAX_SCRIPT_ID_CHARS),
        ] {
            validate_script_id(id).unwrap_or_else(|e| panic!("{id} should be valid: {e}"));
        }
    }

    #[test]
    fn validate_script_id_rejects_empty_oversized_and_unsupported_characters() {
        assert_eq!(
            validate_script_id("").unwrap_err(),
            "Script id is required"
        );
        assert_eq!(
            validate_script_id(&"a".repeat(MAX_SCRIPT_ID_CHARS + 1)).unwrap_err(),
            format!("Script id exceeds {MAX_SCRIPT_ID_CHARS} characters")
        );

        for id in [
            "has space",
            "has/slash",
            "has\\backslash",
            "../escape",
            "has:colon",
            "acentuado\u{00e7}",
            "emoji\u{1F600}",
            "null\0byte",
            "tab\tchar",
        ] {
            assert!(
                validate_script_id(id).is_err(),
                "{id:?} must be rejected"
            );
        }
    }

    // ---- name / description validation ----------------------------------------

    #[test]
    fn validate_script_name_enforces_length_in_characters_and_bans_control_chars() {
        validate_script_name("A Name").unwrap();
        // Counted in characters, so a multi-byte name at the limit is fine.
        validate_script_name(&"\u{00e7}".repeat(MAX_SCRIPT_NAME_CHARS)).unwrap();
        // Whitespace control characters are explicitly allowed.
        validate_script_name("line\nbreak\ttab\rcr").unwrap();

        assert_eq!(
            validate_script_name("").unwrap_err(),
            "Script name is required"
        );
        assert_eq!(
            validate_script_name(&"a".repeat(MAX_SCRIPT_NAME_CHARS + 1)).unwrap_err(),
            format!("Script name exceeds {MAX_SCRIPT_NAME_CHARS} characters")
        );
        assert_eq!(
            validate_script_name("bell\u{0007}").unwrap_err(),
            "Script name contains unsupported control characters"
        );
        assert!(validate_script_name("null\0byte").is_err());
    }

    #[test]
    fn validate_script_description_allows_empty_and_enforces_the_same_rules() {
        validate_script_description("").unwrap();
        validate_script_description(&"a".repeat(MAX_SCRIPT_DESCRIPTION_CHARS)).unwrap();
        validate_script_description("multi\nline\tdescription").unwrap();

        assert_eq!(
            validate_script_description(&"a".repeat(MAX_SCRIPT_DESCRIPTION_CHARS + 1)).unwrap_err(),
            format!("Script description exceeds {MAX_SCRIPT_DESCRIPTION_CHARS} characters")
        );
        assert_eq!(
            validate_script_description("esc\u{001b}[0m").unwrap_err(),
            "Script description contains unsupported control characters"
        );
    }

    #[test]
    fn contains_disallowed_control_chars_permits_only_newline_cr_and_tab() {
        assert!(!contains_disallowed_control_chars("plain text"));
        assert!(!contains_disallowed_control_chars("\n\r\t"));
        assert!(!contains_disallowed_control_chars("acentua\u{00e7}\u{00e3}o"));
        assert!(contains_disallowed_control_chars("\0"));
        assert!(contains_disallowed_control_chars("\u{0007}"));
        assert!(contains_disallowed_control_chars("\u{001b}"));
        assert!(contains_disallowed_control_chars("\u{007f}"));
    }

    // ---- language / source validation -----------------------------------------

    #[test]
    fn normalize_script_language_always_collapses_to_javascript() {
        for input in ["js", "JS", "javascript", "JavaScript", "  js  ", "lua", "ts", "\u{00e7}"] {
            assert_eq!(normalize_script_language(input), "javascript", "{input:?}");
        }
        assert_eq!(normalize_script_language(""), default_language());
        assert_eq!(normalize_script_language("   "), default_language());
    }

    #[test]
    fn validate_script_source_limits_bytes_and_bans_null_bytes() {
        validate_script_source("").unwrap();
        validate_script_source(&"a".repeat(MAX_SCRIPT_SOURCE_BYTES)).unwrap();

        assert_eq!(
            validate_script_source(&"a".repeat(MAX_SCRIPT_SOURCE_BYTES + 1)).unwrap_err(),
            format!("Script source exceeds {MAX_SCRIPT_SOURCE_BYTES} bytes")
        );
        // The limit is in bytes, so multi-byte characters count more than once.
        let multibyte = "\u{00e7}".repeat(MAX_SCRIPT_SOURCE_BYTES / 2 + 1);
        assert!(multibyte.chars().count() <= MAX_SCRIPT_SOURCE_BYTES);
        assert!(validate_script_source(&multibyte).is_err());

        assert_eq!(
            validate_script_source("a\0b").unwrap_err(),
            "Script source contains null bytes"
        );
    }

    // ---- upsert ---------------------------------------------------------------

    #[test]
    fn upsert_trims_and_normalizes_the_incoming_script() {
        let s = store("upsert-normalize");
        let mut incoming = script("  my.script  ", "  My Script  ");
        incoming.description = "  described  ".to_string();
        incoming.language = "JS".to_string();

        let saved = s.upsert(incoming).unwrap();

        assert_eq!(saved.id, "my.script");
        assert_eq!(saved.name, "My Script");
        assert_eq!(saved.description, "described");
        assert_eq!(saved.language, "javascript");
        assert!(saved.created_at_ms > 0);
        assert_eq!(saved.created_at_ms, saved.updated_at_ms);
    }

    #[test]
    fn upsert_rejects_invalid_input_without_writing_the_file() {
        let s = store("upsert-invalid");

        assert!(s.upsert(script("   ", "Name")).is_err(), "blank id");
        assert!(s.upsert(script("bad id", "Name")).is_err(), "spaces in id");
        assert!(s.upsert(script("ok", "   ")).is_err(), "blank name");

        let mut long_source = script("ok", "Name");
        long_source.source = "a".repeat(MAX_SCRIPT_SOURCE_BYTES + 1);
        assert!(s.upsert(long_source).is_err());

        let mut null_source = script("ok", "Name");
        null_source.source = "a\0b".to_string();
        assert!(s.upsert(null_source).is_err());

        // The language length is checked on the raw value, before normalization.
        let mut long_language = script("ok", "Name");
        long_language.language = "j".repeat(MAX_SCRIPT_LANGUAGE_CHARS + 1);
        assert_eq!(
            s.upsert(long_language).unwrap_err(),
            format!("Script language exceeds {MAX_SCRIPT_LANGUAGE_CHARS} characters")
        );

        assert!(s.get_all().unwrap().is_empty());
        assert!(!s.file_path.exists(), "a rejected upsert must not write");
    }

    #[test]
    fn upsert_of_an_existing_id_keeps_created_at_and_bumps_updated_at() {
        let s = store("upsert-existing");
        let first = s.upsert(script("dup", "First")).unwrap();

        let mut second = script("dup", "Second");
        second.created_at_ms = 1;
        second.updated_at_ms = 1;
        second.trusted = true;
        second.auto_start = true;
        second.enabled = false;
        second.permissions.allow_http = true;
        std::thread::sleep(std::time::Duration::from_millis(2));
        let updated = s.upsert(second).unwrap();

        assert_eq!(s.get_all().unwrap().len(), 1, "no duplicate id");
        assert_eq!(updated.name, "Second");
        assert!(updated.trusted && updated.auto_start && !updated.enabled);
        assert!(updated.permissions.allow_http);
        assert_eq!(
            updated.created_at_ms, first.created_at_ms,
            "created_at is preserved and the caller cannot forge it"
        );
        assert!(updated.updated_at_ms >= first.updated_at_ms);
    }

    #[test]
    fn upsert_enforces_the_script_count_limit_but_still_allows_edits() {
        let s = store("upsert-limit");
        {
            let mut scripts = s.scripts.lock().unwrap();
            for i in 0..MAX_SCRIPT_COUNT {
                scripts.push(script(&format!("s{i}"), &format!("Script {i}")));
            }
        }

        assert_eq!(
            s.upsert(script("one-too-many", "Too Many")).unwrap_err(),
            format!("Script limit reached (max {MAX_SCRIPT_COUNT})")
        );
        // Editing an existing script at the limit is still allowed.
        s.upsert(script("s0", "Renamed")).expect("edit at the limit");
        assert_eq!(s.get_all().unwrap().len(), MAX_SCRIPT_COUNT);
    }

    // ---- get_all --------------------------------------------------------------

    #[test]
    fn get_all_sorts_by_name_case_insensitively() {
        let s = store("sorting");
        for (id, name) in [
            ("c", "zebra"),
            ("a", "Alpha"),
            ("b", "beta"),
            ("d", "ALPHA2"),
        ] {
            s.upsert(script(id, name)).unwrap();
        }

        let names: Vec<String> = s.get_all().unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(names, vec!["Alpha", "ALPHA2", "beta", "zebra"]);
    }

    // ---- remove ---------------------------------------------------------------

    #[test]
    fn remove_requires_an_id_trims_it_and_reports_whether_it_hit() {
        let s = store("remove");
        assert_eq!(s.remove("").unwrap_err(), "Script id is required");
        assert_eq!(s.remove("   ").unwrap_err(), "Script id is required");
        assert!(!s.remove("nope").unwrap());
        assert!(!s.file_path.exists(), "a missed remove must not write");

        s.upsert(script("keep", "Keep")).unwrap();
        s.upsert(script("drop", "Drop")).unwrap();

        assert!(s.remove("  drop  ").unwrap(), "the id is trimmed");
        assert!(!s.remove("drop").unwrap());
        assert_eq!(s.get_all().unwrap().len(), 1);

        let reloaded = ScriptStore::new(s.file_path.clone());
        assert_eq!(reloaded.get_all().unwrap().len(), 1);
    }

    // ---- persistence ----------------------------------------------------------

    #[test]
    fn scripts_round_trip_through_the_file_including_permissions() {
        let s = store("persist");
        let mut full = script("persisted", "Persisted");
        full.description = "with permissions".to_string();
        full.source = "ram.info('hi');".to_string();
        full.trusted = true;
        full.auto_start = true;
        full.permissions = ScriptPermissions {
            allow_invoke: true,
            allow_http: true,
            allow_websocket: true,
            allow_window: false,
            allow_modal: true,
            allow_settings: false,
            allow_ui: true,
        };
        let saved = s.upsert(full).unwrap();

        let raw = fs::read_to_string(&s.file_path).unwrap();
        assert!(raw.contains("\"allowWebSocket\": true"), "{raw}");

        let reloaded = ScriptStore::new(s.file_path.clone());
        let loaded = reloaded.get_all().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "persisted");
        assert_eq!(loaded[0].description, "with permissions");
        assert!(loaded[0].trusted && loaded[0].auto_start);
        assert!(loaded[0].permissions.allow_invoke);
        assert!(loaded[0].permissions.allow_websocket);
        assert!(!loaded[0].permissions.allow_window);
        assert_eq!(loaded[0].created_at_ms, saved.created_at_ms);
    }

    #[test]
    fn load_from_disk_accepts_a_missing_or_empty_file() {
        let s = store("load-empty");
        s.load_from_disk().expect("missing file is fine");
        assert!(s.get_all().unwrap().is_empty());

        fs::write(&s.file_path, b"").unwrap();
        s.load_from_disk().expect("empty file is fine");
        assert!(s.get_all().unwrap().is_empty());
    }

    #[test]
    fn load_from_disk_refuses_a_file_over_the_size_limit() {
        let s = store("load-oversized");
        let oversized = vec![b'a'; (MAX_SCRIPTS_FILE_BYTES + 1) as usize];
        fs::write(&s.file_path, &oversized).unwrap();

        let err = s.load_from_disk().expect_err("oversized file must fail");
        assert_eq!(
            err,
            format!("Scripts file is too large (max {MAX_SCRIPTS_FILE_BYTES} bytes)")
        );
        assert!(s.get_all().unwrap().is_empty());
    }

    #[test]
    fn load_from_disk_reports_a_parse_error_for_a_corrupt_file() {
        let s = store("load-corrupt");
        fs::write(&s.file_path, b"{not json").unwrap();
        let err = s.load_from_disk().expect_err("corrupt file must fail");
        assert!(err.starts_with("Failed to parse scripts file:"), "{err}");

        // A JSON document of the wrong shape is a parse error too.
        fs::write(&s.file_path, br#"{"scripts": []}"#).unwrap();
        assert!(s.load_from_disk().is_err());
    }

    #[test]
    fn a_corrupt_scripts_file_is_never_overwritten() {
        // Regression guard: the store used to start empty and let the first
        // upsert replace the user's file. Now the failed load latches and
        // every save is refused until the file is fixed.
        let s = store("corrupt-overwrite");
        let corrupt = br#"[{"id": "important", "name": "Important"#;
        fs::write(&s.file_path, corrupt).unwrap();

        let reopened = ScriptStore::new(s.file_path.clone());
        assert!(reopened.get_all().unwrap().is_empty());

        let err = reopened
            .upsert(script("new", "New"))
            .expect_err("saving over an unreadable file must fail");
        assert!(err.contains("refusing to overwrite"), "{err}");
        assert_eq!(
            fs::read(&s.file_path).unwrap(),
            corrupt.to_vec(),
            "the user's file is untouched"
        );
    }

    #[test]
    fn an_oversized_scripts_file_is_never_overwritten() {
        let s = store("oversized-overwrite");
        let oversized = vec![b'a'; (MAX_SCRIPTS_FILE_BYTES + 1) as usize];
        fs::write(&s.file_path, &oversized).unwrap();

        let reopened = ScriptStore::new(s.file_path.clone());
        assert!(reopened.upsert(script("new", "New")).is_err());
        assert_eq!(fs::read(&s.file_path).unwrap().len(), oversized.len());
    }

    #[test]
    fn a_healthy_store_still_saves() {
        let s = store("healthy-save");
        s.upsert(script("ok", "Ok")).expect("first save");

        let reopened = ScriptStore::new(s.file_path.clone());
        assert_eq!(reopened.get_all().unwrap().len(), 1);
        reopened.upsert(script("two", "Two")).expect("second save");
        assert_eq!(reopened.get_all().unwrap().len(), 2);
    }

    // ---- serde defaults -------------------------------------------------------

    #[test]
    fn managed_script_fills_in_defaults_for_a_minimal_json_document() {
        let parsed: ManagedScript =
            serde_json::from_str(r#"{"id": "min", "name": "Minimal"}"#).unwrap();
        assert_eq!(parsed.description, "");
        assert_eq!(parsed.language, "javascript");
        assert_eq!(parsed.source, "");
        assert!(parsed.enabled, "scripts default to enabled");
        assert!(!parsed.trusted);
        assert!(!parsed.auto_start);
        assert!(parsed.created_at_ms > 0, "timestamps default to now");
        assert!(parsed.updated_at_ms > 0);

        // Every permission defaults to denied.
        let p = parsed.permissions;
        assert!(
            !p.allow_invoke
                && !p.allow_http
                && !p.allow_websocket
                && !p.allow_window
                && !p.allow_modal
                && !p.allow_settings
                && !p.allow_ui
        );
    }

    #[test]
    fn script_permissions_accept_the_legacy_lowercase_websocket_key() {
        let legacy: ScriptPermissions =
            serde_json::from_str(r#"{"allowWebsocket": true}"#).unwrap();
        assert!(legacy.allow_websocket);

        let current: ScriptPermissions =
            serde_json::from_str(r#"{"allowWebSocket": true}"#).unwrap();
        assert!(current.allow_websocket);

        // Serialization always uses the current spelling.
        let json = serde_json::to_string(&current).unwrap();
        assert!(json.contains("\"allowWebSocket\":true"), "{json}");
        assert!(!json.contains("allowWebsocket\""), "{json}");
    }

    #[test]
    fn managed_script_id_and_name_are_required_by_serde() {
        assert!(serde_json::from_str::<ManagedScript>(r#"{"name": "No Id"}"#).is_err());
        assert!(serde_json::from_str::<ManagedScript>(r#"{"id": "no-name"}"#).is_err());
    }

    // ---- path helper ----------------------------------------------------------

}
