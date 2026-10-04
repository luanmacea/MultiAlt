//! Favoritos (com os servidores VIP salvos), jogos recentes e servidores
//! recentes — as listas da Choose Game e do Server List.
//!
//! Até a v0.1.x elas moravam **só** no `localStorage` do WebView: ficavam fora
//! do backup, da migração de pasta de dados e se perdiam num reset do WebView ou
//! na troca de PC. Agora o arquivo `RAMGameLists.json`, na pasta de dados, é a
//! cópia durável (está em `DATA_FILES`, então backup, restauração e migração o
//! levam junto). O `localStorage` continua como cache rápido do frontend — ver
//! `src/components/server-list/gameListsSync.ts`.
//!
//! O **formato dos itens é do frontend**: aqui cada item é um
//! `serde_json::Value` opaco. O backend só garante que as três chaves são listas
//! e cuida de não perder dado:
//!
//! - gravação atômica (temporário + troca), com o conteúdo anterior guardado em
//!   `RAMGameLists.json.bak` a cada gravação;
//! - arquivo ilegível **trava** a gravação em vez de ser sobrescrito (mesma regra
//!   do `RAMAvatars.json`);
//! - uma gravação que zera **todos** os favoritos ou **todos** os servidores VIP
//!   de uma vez é recusada, a menos que venha de uma exclusão explícita do
//!   usuário (`allow_destructive`). O dono já perdeu VIPs salvos por um bug de
//!   tela que regravou uma cópia velha da lista; esta é a última barreira.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const GAME_LISTS_FILE_NAME: &str = "RAMGameLists.json";
const MAX_GAME_LISTS_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// As três listas. Itens opacos: quem define o formato é o frontend.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameLists {
    #[serde(default)]
    pub favorites: Vec<serde_json::Value>,
    #[serde(default)]
    pub recent_games: Vec<serde_json::Value>,
    #[serde(default)]
    pub recent_jobs: Vec<serde_json::Value>,
}

impl GameLists {
    /// Quantos servidores VIP os favoritos guardam: `vipServers` de cada um, ou
    /// o campo antigo `privateServer` quando a lista nova ainda não existe.
    pub fn vip_server_count(&self) -> usize {
        self.favorites
            .iter()
            .map(|favorite| match favorite.get("vipServers") {
                Some(serde_json::Value::Array(vips)) => vips.len(),
                _ => match favorite.get("privateServer") {
                    Some(serde_json::Value::String(link)) if !link.trim().is_empty() => 1,
                    _ => 0,
                },
            })
            .sum()
    }
}

pub struct GameListsStore {
    file_path: PathBuf,
    /// Serializa leitura-e-gravação: duas gravações nunca se cruzam no `.tmp`.
    lock: Mutex<()>,
}

fn backup_path(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(".bak");
    PathBuf::from(name)
}

fn tmp_path(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

impl GameListsStore {
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            file_path,
            lock: Mutex::new(()),
        }
    }

    #[cfg(test)]
    pub fn backup_file_path(&self) -> PathBuf {
        backup_path(&self.file_path)
    }

    /// `Ok(None)` quando o arquivo não existe (ou está vazio): é o sinal para o
    /// frontend migrar o que tem no `localStorage`. Arquivo ilegível é `Err` —
    /// nunca "lista vazia", senão a próxima gravação apagaria o que está nele.
    fn read_from_disk(&self) -> Result<Option<GameLists>, String> {
        let metadata = match fs::metadata(&self.file_path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("Failed to read game lists file metadata: {e}")),
        };
        if metadata.len() > MAX_GAME_LISTS_FILE_BYTES {
            return Err(format!(
                "Game lists file is too large (max {MAX_GAME_LISTS_FILE_BYTES} bytes)"
            ));
        }
        let data = fs::read(&self.file_path)
            .map_err(|e| format!("Failed to read game lists file: {e}"))?;
        if data.iter().all(|b| b.is_ascii_whitespace()) {
            return Ok(None);
        }
        serde_json::from_slice::<GameLists>(&data)
            .map(Some)
            .map_err(|e| format!("Failed to parse game lists file: {e}"))
    }

    pub fn load(&self) -> Result<Option<GameLists>, String> {
        let _guard = self.lock.lock().map_err(|e| e.to_string())?;
        self.read_from_disk()
    }

    /// Grava as listas. Recusa (sem tocar no arquivo) quando:
    /// - o arquivo atual não pôde ser lido — sobrescrevê-lo seria perder o que
    ///   ele tem;
    /// - a gravação zera todos os favoritos, ou todos os servidores VIP, e não
    ///   veio de uma exclusão explícita (`allow_destructive`).
    ///
    /// Antes de trocar o arquivo, o conteúdo atual vai para `.bak`.
    pub fn save(&self, lists: &GameLists, allow_destructive: bool) -> Result<(), String> {
        let _guard = self.lock.lock().map_err(|e| e.to_string())?;

        let current = self.read_from_disk().map_err(|e| {
            format!(
                "{e}; refusing to overwrite it. Fix or restore {GAME_LISTS_FILE_NAME} and restart."
            )
        })?;

        if let Some(current) = &current {
            if !allow_destructive {
                if !current.favorites.is_empty() && lists.favorites.is_empty() {
                    let msg = format!(
                        "Refusing to save game lists: the write would remove all {} favorites without an explicit delete.",
                        current.favorites.len()
                    );
                    eprintln!("{msg}");
                    return Err(msg);
                }
                let current_vips = current.vip_server_count();
                if current_vips > 0 && lists.vip_server_count() == 0 {
                    let msg = format!(
                        "Refusing to save game lists: the write would remove all {current_vips} saved VIP servers without an explicit delete."
                    );
                    eprintln!("{msg}");
                    return Err(msg);
                }
            }
        }

        let bytes = serde_json::to_vec_pretty(lists)
            .map_err(|e| format!("Failed to serialize game lists: {e}"))?;

        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create the data folder: {e}"))?;
        }

        // A versão anterior fica ao lado: se uma gravação levar algo que não
        // devia, o `.bak` ainda tem.
        if current.is_some() {
            fs::copy(&self.file_path, backup_path(&self.file_path)).map_err(|e| {
                format!("Failed to keep the previous game lists ({e}); nothing was saved.")
            })?;
        }

        let tmp = tmp_path(&self.file_path);
        match crate::data::versions::write_all_synced(&tmp, &bytes) {
            Ok(true) => {}
            Ok(false) => eprintln!("Warning: game lists written but fsync could not be confirmed"),
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                return Err(format!("Failed to write game lists file: {e}"));
            }
        }
        crate::data::versions::atomic_replace(&tmp, &self.file_path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            format!("Failed to write game lists file: {e}")
        })
    }
}

#[tauri::command]
pub fn get_game_lists(
    store: tauri::State<'_, GameListsStore>,
) -> Result<Option<GameLists>, String> {
    store.load()
}

/// `allow_destructive` só vai `true` quando a mudança veio de uma exclusão
/// pedida pelo usuário (remover favorito, remover VIP, limpar recentes).
#[tauri::command]
pub fn save_game_lists(
    store: tauri::State<'_, GameListsStore>,
    lists: GameLists,
    allow_destructive: Option<bool>,
) -> Result<(), String> {
    store.save(&lists, allow_destructive.unwrap_or(false))
}

#[cfg(test)]
mod game_lists_store_tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let dir = std::env::temp_dir()
                .join(format!("ram-game-lists-{}-{nanos}-{n}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn file(&self) -> PathBuf {
            self.0.join(GAME_LISTS_FILE_NAME)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn favorite(place_id: u64, vips: &[&str]) -> serde_json::Value {
        json!({
            "placeId": place_id,
            "name": format!("Game {place_id}"),
            "iconUrl": null,
            "addedAt": 1,
            "vipServers": vips
                .iter()
                .enumerate()
                .map(|(i, link)| json!({ "id": format!("vip-{i}"), "name": "VIP", "link": link }))
                .collect::<Vec<_>>(),
        })
    }

    fn lists(favorites: Vec<serde_json::Value>) -> GameLists {
        GameLists {
            favorites,
            recent_games: vec![
                json!({ "placeId": 9, "name": "Recent", "iconUrl": null, "lastPlayed": 5 }),
            ],
            recent_jobs: vec![
                json!({ "kind": "job", "raw": "abc", "placeId": 9, "lastUsed": 5, "userIds": [] }),
            ],
        }
    }

    #[test]
    fn a_missing_file_loads_as_none_so_the_frontend_can_migrate() {
        let dir = TempDir::new();
        let store = GameListsStore::new(dir.file());
        assert_eq!(store.load().unwrap(), None);
    }

    #[test]
    fn the_lists_round_trip_with_their_opaque_items_untouched() {
        let dir = TempDir::new();
        let store = GameListsStore::new(dir.file());
        let mut data = lists(vec![favorite(1, &["vip:111"])]);
        // Campo que o Rust não conhece: tem que voltar igual.
        data.favorites[0]["somethingNew"] = json!({ "nested": [1, 2] });
        store.save(&data, false).unwrap();

        assert_eq!(store.load().unwrap(), Some(data.clone()));
        let on_disk: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.file()).unwrap()).unwrap();
        assert!(
            on_disk.get("recentGames").is_some(),
            "camelCase on disk: {on_disk}"
        );
        assert!(on_disk.get("recentJobs").is_some());
    }

    #[test]
    fn an_unreadable_file_is_an_error_and_is_never_overwritten() {
        let dir = TempDir::new();
        fs::write(dir.file(), b"{ not json").unwrap();
        let store = GameListsStore::new(dir.file());

        assert!(store.load().is_err(), "corrupt must not read as empty");
        assert!(store.save(&lists(vec![favorite(1, &[])]), true).is_err());
        assert_eq!(fs::read(dir.file()).unwrap(), b"{ not json");
    }

    #[test]
    fn every_write_keeps_the_previous_version_in_a_bak_file() {
        let dir = TempDir::new();
        let store = GameListsStore::new(dir.file());
        let first = lists(vec![favorite(1, &["vip:111"])]);
        let second = lists(vec![favorite(1, &["vip:111"]), favorite(2, &[])]);

        store.save(&first, false).unwrap();
        assert!(
            !store.backup_file_path().exists(),
            "nothing to keep on the first write"
        );
        store.save(&second, false).unwrap();

        let bak: GameLists =
            serde_json::from_slice(&fs::read(store.backup_file_path()).unwrap()).unwrap();
        assert_eq!(bak, first);
        assert_eq!(store.load().unwrap(), Some(second));
        assert!(!tmp_path(&dir.file()).exists(), "no temp file left behind");
    }

    #[test]
    fn a_write_that_drops_every_favorite_is_refused_without_an_explicit_delete() {
        let dir = TempDir::new();
        let store = GameListsStore::new(dir.file());
        let good = lists(vec![favorite(1, &[]), favorite(2, &[])]);
        store.save(&good, false).unwrap();

        let err = store.save(&lists(vec![]), false).unwrap_err();
        assert!(err.contains("all 2 favorites"), "{err}");
        assert_eq!(store.load().unwrap(), Some(good));

        // A exclusão explícita do usuário continua funcionando.
        store.save(&lists(vec![]), true).unwrap();
        assert_eq!(store.load().unwrap().unwrap().favorites.len(), 0);
    }

    #[test]
    fn a_write_that_drops_every_vip_server_is_refused_without_an_explicit_delete() {
        let dir = TempDir::new();
        let store = GameListsStore::new(dir.file());
        let good = lists(vec![favorite(1, &["vip:111", "vip:222"]), favorite(2, &[])]);
        store.save(&good, false).unwrap();

        // A cópia velha de uma tela: os mesmos favoritos, sem os VIPs.
        let stale = lists(vec![favorite(1, &[]), favorite(2, &[])]);
        let err = store.save(&stale, false).unwrap_err();
        assert!(err.contains("VIP"), "{err}");
        assert_eq!(store.load().unwrap(), Some(good));

        store.save(&stale, true).unwrap();
        assert_eq!(store.load().unwrap().unwrap().vip_server_count(), 0);
    }

    #[test]
    fn removing_one_vip_out_of_several_is_an_ordinary_write() {
        let dir = TempDir::new();
        let store = GameListsStore::new(dir.file());
        store
            .save(&lists(vec![favorite(1, &["vip:111", "vip:222"])]), false)
            .unwrap();
        store
            .save(&lists(vec![favorite(1, &["vip:111"])]), false)
            .unwrap();
        assert_eq!(store.load().unwrap().unwrap().vip_server_count(), 1);
    }

    #[test]
    fn the_legacy_private_server_field_counts_as_a_vip_server() {
        let legacy = GameLists {
            favorites: vec![json!({ "placeId": 1, "name": "Old", "privateServer": "vip:9" })],
            ..Default::default()
        };
        assert_eq!(legacy.vip_server_count(), 1);
        let blank = GameLists {
            favorites: vec![json!({ "placeId": 1, "name": "Old", "privateServer": "  " })],
            ..Default::default()
        };
        assert_eq!(blank.vip_server_count(), 0);
    }

    #[test]
    fn a_payload_with_non_list_fields_is_rejected_by_deserialization() {
        let parsed = serde_json::from_value::<GameLists>(json!({ "favorites": { "placeId": 1 } }));
        assert!(parsed.is_err());
        let partial = serde_json::from_value::<GameLists>(json!({ "favorites": [] })).unwrap();
        assert!(partial.recent_games.is_empty() && partial.recent_jobs.is_empty());
    }
}
