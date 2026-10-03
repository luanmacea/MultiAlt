//! Avatares salvos pelo usuário (receitas de itens oficiais gratuitos).
//!
//! Persistência em `RAMAvatars.json`, no mesmo molde do `ScriptStore`: arquivo
//! ilegível trava a gravação em vez de ser sobrescrito por uma lista vazia.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::api::roblox::CatalogItemKind;

const MAX_AVATARS_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_AVATAR_COUNT: usize = 100;
const MAX_AVATAR_ID_CHARS: usize = 96;
const MAX_AVATAR_NAME_CHARS: usize = 60;
const MAX_AVATAR_ITEMS: usize = 12;

/// Referência a um item do catálogo dentro de um avatar salvo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarItemRef {
    pub id: i64,
    pub kind: CatalogItemKind,
    pub type_id: i64,
    pub name: String,
    pub collectible_item_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedAvatar {
    pub id: String,
    pub name: String,
    pub items: Vec<AvatarItemRef>,
    /// BrickColor id aplicado às seis partes do corpo.
    pub skin_color: Option<i64>,
}

/// Regras de um avatar salvo. O nome é checado já sem espaços nas pontas.
pub fn validate_avatar(a: &SavedAvatar) -> Result<(), String> {
    if a.id.is_empty() {
        return Err("Avatar id is required".to_string());
    }
    if a.id.chars().count() > MAX_AVATAR_ID_CHARS {
        return Err(format!(
            "Avatar id exceeds {} characters",
            MAX_AVATAR_ID_CHARS
        ));
    }
    if !a
        .id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(
            "Avatar id contains unsupported characters (allowed: a-z, A-Z, 0-9, -, _)".to_string(),
        );
    }

    let name = a.name.trim();
    if name.is_empty() {
        return Err("Avatar name is required".to_string());
    }
    if name.chars().count() > MAX_AVATAR_NAME_CHARS {
        return Err(format!(
            "Avatar name exceeds {} characters",
            MAX_AVATAR_NAME_CHARS
        ));
    }

    if a.items.is_empty() {
        return Err("Avatar needs at least one item".to_string());
    }
    if a.items.len() > MAX_AVATAR_ITEMS {
        return Err(format!(
            "Avatar exceeds {} items",
            MAX_AVATAR_ITEMS
        ));
    }
    for (i, item) in a.items.iter().enumerate() {
        if a.items[..i].iter().any(|other| other.id == item.id) {
            return Err(format!("Avatar has duplicate item {}", item.id));
        }
    }
    Ok(())
}

pub struct AvatarStore {
    avatars: Mutex<Vec<SavedAvatar>>,
    file_path: PathBuf,
    /// Ligado quando o arquivo existe mas não pôde ser lido (JSON corrompido,
    /// grande demais). Enquanto estiver ligado a gravação é recusada, para a
    /// lista vazia em memória nunca sobrescrever os avatares do usuário.
    load_failed: AtomicBool,
}

impl AvatarStore {
    pub fn new(path: PathBuf) -> Self {
        let store = Self {
            avatars: Mutex::new(Vec::new()),
            file_path: path,
            load_failed: AtomicBool::new(false),
        };
        if store.file_path.exists() && store.load_from_disk().is_err() {
            store.load_failed.store(true, Ordering::SeqCst);
        }
        store
    }

    fn load_from_disk(&self) -> Result<(), String> {
        let metadata = fs::metadata(&self.file_path)
            .map_err(|e| format!("Failed to read avatars file metadata: {}", e))?;
        if metadata.len() > MAX_AVATARS_FILE_BYTES {
            return Err(format!(
                "Avatars file is too large (max {} bytes)",
                MAX_AVATARS_FILE_BYTES
            ));
        }
        let data =
            fs::read(&self.file_path).map_err(|e| format!("Failed to read avatars file: {}", e))?;
        if data.is_empty() {
            return Ok(());
        }
        let parsed = serde_json::from_slice::<Vec<SavedAvatar>>(&data)
            .map_err(|e| format!("Failed to parse avatars file: {}", e))?;
        let mut avatars = self.avatars.lock().map_err(|e| e.to_string())?;
        *avatars = parsed;
        Ok(())
    }

    /// Grava num arquivo ao lado e renomeia por cima: queda no meio da escrita
    /// não deixa o `RAMAvatars.json` pela metade.
    fn save_to_disk(&self, avatars: &[SavedAvatar]) -> Result<(), String> {
        if self.load_failed.load(Ordering::SeqCst) {
            return Err(
                "Avatars file could not be read; refusing to overwrite it. Fix or restore RAMAvatars.json and restart.".to_string(),
            );
        }
        let bytes = serde_json::to_vec_pretty(avatars)
            .map_err(|e| format!("Failed to serialize avatars: {}", e))?;
        let mut tmp = self.file_path.clone().into_os_string();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        fs::write(&tmp, bytes).map_err(|e| format!("Failed to write avatars file: {}", e))?;
        fs::rename(&tmp, &self.file_path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            format!("Failed to write avatars file: {}", e)
        })
    }

    pub fn list(&self) -> Vec<SavedAvatar> {
        self.avatars
            .lock()
            .map(|a| a.clone())
            .unwrap_or_default()
    }

    /// Valida e grava. O mesmo id substitui o avatar existente (na mesma
    /// posição); um id novo vai para o fim da lista.
    pub fn upsert(&self, mut avatar: SavedAvatar) -> Result<SavedAvatar, String> {
        avatar.name = avatar.name.trim().to_string();
        validate_avatar(&avatar)?;

        let mut avatars = self.avatars.lock().map_err(|e| e.to_string())?;
        let mut next = avatars.clone();
        match next.iter_mut().find(|a| a.id == avatar.id) {
            Some(existing) => *existing = avatar.clone(),
            None => {
                if next.len() >= MAX_AVATAR_COUNT {
                    return Err(format!(
                        "Avatar limit reached (max {})",
                        MAX_AVATAR_COUNT
                    ));
                }
                next.push(avatar.clone());
            }
        }
        // Só troca a memória depois de gravar: falha de disco não deixa a lista
        // em memória adiante do arquivo.
        self.save_to_disk(&next)?;
        *avatars = next;
        Ok(avatar)
    }

    /// `true` se o avatar existia e foi apagado.
    pub fn delete(&self, id: &str) -> Result<bool, String> {
        let mut avatars = self.avatars.lock().map_err(|e| e.to_string())?;
        if !avatars.iter().any(|a| a.id == id) {
            return Ok(false);
        }
        let next: Vec<SavedAvatar> = avatars.iter().filter(|a| a.id != id).cloned().collect();
        self.save_to_disk(&next)?;
        *avatars = next;
        Ok(true)
    }
}

#[cfg(test)]
mod avatar_store_tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// Pasta temporária única por teste, apagada ao sair de escopo.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "ram-avatars-{}-{nanos}-{n}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn file(&self) -> PathBuf {
            self.0.join("RAMAvatars.json")
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn item(id: i64) -> AvatarItemRef {
        AvatarItemRef {
            id,
            kind: CatalogItemKind::Asset,
            type_id: 8,
            name: format!("Item {id}"),
            collectible_item_id: String::new(),
        }
    }

    fn avatar(id: &str, name: &str, n_items: i64) -> SavedAvatar {
        SavedAvatar {
            id: id.to_string(),
            name: name.to_string(),
            items: (1..=n_items).map(item).collect(),
            skin_color: Some(1030),
        }
    }

    #[test]
    fn missing_file_gives_an_empty_list() {
        let dir = TempDir::new();
        let store = AvatarStore::new(dir.file());
        assert!(store.list().is_empty());
    }

    #[test]
    fn upsert_is_read_back_by_a_new_store() {
        let dir = TempDir::new();
        let store = AvatarStore::new(dir.file());
        let saved = store.upsert(avatar("a-1", "Pirate", 3)).unwrap();
        assert_eq!(saved.name, "Pirate");

        let reopened = AvatarStore::new(dir.file());
        assert_eq!(reopened.list(), vec![avatar("a-1", "Pirate", 3)]);
    }

    #[test]
    fn upsert_trims_the_name() {
        let dir = TempDir::new();
        let store = AvatarStore::new(dir.file());
        let saved = store.upsert(avatar("a", "  Pirate  ", 1)).unwrap();
        assert_eq!(saved.name, "Pirate");
        assert_eq!(store.list()[0].name, "Pirate");
    }

    #[test]
    fn upsert_with_the_same_id_replaces_in_place() {
        let dir = TempDir::new();
        let store = AvatarStore::new(dir.file());
        store.upsert(avatar("a", "First", 1)).unwrap();
        store.upsert(avatar("b", "Other", 1)).unwrap();
        store.upsert(avatar("a", "Renamed", 2)).unwrap();

        let list = AvatarStore::new(dir.file()).list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "a");
        assert_eq!(list[0].name, "Renamed");
        assert_eq!(list[0].items.len(), 2);
        assert_eq!(list[1].id, "b");
    }

    #[test]
    fn delete_reports_whether_it_hit_and_persists() {
        let dir = TempDir::new();
        let store = AvatarStore::new(dir.file());
        store.upsert(avatar("a", "A", 1)).unwrap();
        store.upsert(avatar("b", "B", 1)).unwrap();

        assert!(store.delete("a").unwrap());
        assert!(!store.delete("a").unwrap());
        assert!(!store.delete("never-existed").unwrap());

        let list = AvatarStore::new(dir.file()).list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "b");
    }

    #[test]
    fn a_missed_delete_does_not_write_the_file() {
        let dir = TempDir::new();
        let store = AvatarStore::new(dir.file());
        assert!(!store.delete("nope").unwrap());
        assert!(!dir.file().exists());
    }

    #[test]
    fn validation_rejects_an_empty_or_blank_name() {
        assert!(validate_avatar(&avatar("a", "", 1)).is_err());
        assert!(validate_avatar(&avatar("a", "   ", 1)).is_err());
        validate_avatar(&avatar("a", "x", 1)).unwrap();
    }

    #[test]
    fn validation_limits_the_name_to_60_chars_after_trim() {
        validate_avatar(&avatar("a", &"n".repeat(60), 1)).unwrap();
        validate_avatar(&avatar("a", &format!("  {}  ", "n".repeat(60)), 1)).unwrap();
        assert!(validate_avatar(&avatar("a", &"n".repeat(61), 1)).is_err());
        // Conta caracteres, não bytes.
        validate_avatar(&avatar("a", &"\u{00e7}".repeat(60), 1)).unwrap();
    }

    #[test]
    fn validation_requires_between_1_and_12_items() {
        assert!(validate_avatar(&avatar("a", "A", 0)).is_err());
        validate_avatar(&avatar("a", "A", 1)).unwrap();
        validate_avatar(&avatar("a", "A", 12)).unwrap();
        assert!(validate_avatar(&avatar("a", "A", 13)).is_err());
    }

    #[test]
    fn validation_rejects_duplicate_item_ids() {
        let mut a = avatar("a", "A", 3);
        a.items[2].id = a.items[0].id;
        let err = validate_avatar(&a).unwrap_err();
        assert!(err.contains("duplicate"), "{err}");
    }

    #[test]
    fn validation_restricts_the_id_charset_and_length() {
        for ok in ["a", "A-b_9", &"a".repeat(96)] {
            validate_avatar(&avatar(ok, "A", 1)).unwrap_or_else(|e| panic!("{ok}: {e}"));
        }
        for bad in [
            "",
            "has space",
            "dot.dot",
            "../x",
            "a/b",
            "a\\b",
            "\u{00e7}",
            &"a".repeat(97),
        ] {
            assert!(validate_avatar(&avatar(bad, "A", 1)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn upsert_rejects_invalid_avatars_without_writing() {
        let dir = TempDir::new();
        let store = AvatarStore::new(dir.file());
        assert!(store.upsert(avatar("bad id", "A", 1)).is_err());
        assert!(store.upsert(avatar("a", "", 1)).is_err());
        assert!(store.upsert(avatar("a", "A", 0)).is_err());
        assert!(store.list().is_empty());
        assert!(!dir.file().exists());
    }

    #[test]
    fn the_store_holds_at_most_100_avatars_but_still_allows_edits() {
        let dir = TempDir::new();
        let store = AvatarStore::new(dir.file());
        {
            let mut avatars = store.avatars.lock().unwrap();
            for i in 0..MAX_AVATAR_COUNT {
                avatars.push(avatar(&format!("a{i}"), &format!("Avatar {i}"), 1));
            }
        }
        let err = store.upsert(avatar("one-too-many", "X", 1)).unwrap_err();
        assert!(err.contains("limit"), "{err}");
        store.upsert(avatar("a0", "Renamed", 1)).unwrap();
        assert_eq!(store.list().len(), MAX_AVATAR_COUNT);
    }

    #[test]
    fn a_corrupt_file_reads_as_empty_and_is_never_overwritten() {
        let dir = TempDir::new();
        let corrupt = br#"[{"id": "important", "name": "Impor"#;
        fs::write(dir.file(), corrupt).unwrap();

        let store = AvatarStore::new(dir.file());
        assert!(store.list().is_empty());
        let err = store.upsert(avatar("new", "New", 1)).unwrap_err();
        assert!(err.contains("refusing to overwrite"), "{err}");
        assert!(store.delete("important").is_ok());
        assert_eq!(fs::read(dir.file()).unwrap(), corrupt.to_vec());
    }

    #[test]
    fn an_empty_file_is_an_empty_list_and_still_saves() {
        let dir = TempDir::new();
        fs::write(dir.file(), b"").unwrap();
        let store = AvatarStore::new(dir.file());
        assert!(store.list().is_empty());
        store.upsert(avatar("a", "A", 1)).unwrap();
        assert_eq!(AvatarStore::new(dir.file()).list().len(), 1);
    }

    #[test]
    fn a_save_leaves_no_temp_file_behind() {
        let dir = TempDir::new();
        let store = AvatarStore::new(dir.file());
        store.upsert(avatar("a", "A", 1)).unwrap();
        let names: Vec<String> = fs::read_dir(&dir.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["RAMAvatars.json".to_string()]);
    }

    #[test]
    fn serialized_shape_is_camel_case_and_stable() {
        let a = SavedAvatar {
            id: "x".to_string(),
            name: "N".to_string(),
            items: vec![AvatarItemRef {
                id: 7,
                kind: CatalogItemKind::Bundle,
                type_id: 1,
                name: "Body".to_string(),
                collectible_item_id: "abc".to_string(),
            }],
            skin_color: None,
        };
        let json: serde_json::Value = serde_json::to_value(&a).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "id": "x",
                "name": "N",
                "items": [{
                    "id": 7,
                    "kind": "Bundle",
                    "typeId": 1,
                    "name": "Body",
                    "collectibleItemId": "abc"
                }],
                "skinColor": null
            })
        );
        let back: SavedAvatar = serde_json::from_value(json).unwrap();
        assert_eq!(back, a);
    }
}
