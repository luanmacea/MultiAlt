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
//!   dos outros arquivos de dados);
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
