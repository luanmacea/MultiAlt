use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Grava o arquivo e **espera o conteúdo chegar ao disco** antes de devolver.
///
/// Existe porque `fs::write` + rename não é durável: o rename pode publicar um
/// arquivo cujo conteúdo ainda está em cache, e uma queda de energia entre os
/// dois deixa um arquivo que existe, tem o nome certo e está **vazio**. Para o
/// `AccountData.json` isso é a lista de contas; para o `AccountData.key` é a
/// chave que abre a lista. Os dois passam por aqui.
/// Devolve `false` quando os bytes foram escritos mas o `sync_all` **não** pôde
/// ser confirmado.
///
/// Isso é deliberado, e a alternativa era pior: `sync_all` falha em volumes onde
/// `fs::write` funciona normalmente (share de rede, sistema de arquivos virtual
/// de nuvem, pen drive exótico). Abortar ali deixaria **toda** gravação de conta
/// falhando — o app viraria somente-leitura — para proteger contra uma janela de
/// queda de energia. Como `write_all` + `flush` já passaram, os dados estão
/// entregues ao SO: o que se perde é a *garantia* de ordem, ou seja, volta-se ao
/// comportamento de antes desta tarefa. Degradar a garantia é aceitável; tirar o
/// app do ar não é. Quem chama decide se registra a degradação.
pub(crate) fn write_all_synced(path: &Path, data: &[u8]) -> std::io::Result<bool> {
    use std::io::Write;

    let mut file = std::fs::File::create(path)?;
    file.write_all(data)?;
    file.flush()?;
    Ok(file.sync_all().is_ok())
}

/// Igual a [`atomic_replace`], mas preserva o `io::Error`.
///
/// Existe porque o `ErrorKind` é o que separa "o antivírus segurou o arquivo por
/// um instante" de "isto não vai funcionar" — e é essa diferença que decide se o
/// app alarma o usuário ou só tenta de novo na gravação seguinte. É o primitivo;
/// [`atomic_replace`] é só a versão que descarta o tipo do erro.
#[cfg(target_os = "windows")]
pub(crate) fn atomic_replace_io(src: &Path, dst: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let src_wide: Vec<u16> = src
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let dst_wide: Vec<u16> = dst
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let ok = unsafe {
        MoveFileExW(
            src_wide.as_ptr(),
            dst_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn atomic_replace_io(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::rename(src, dst)
}

/// Troca atômica, com o erro já virado texto. Os ~10 chamadores que só querem
/// reportar continuam usando esta; quem precisa do `ErrorKind` usa
/// [`atomic_replace_io`].
pub(crate) fn atomic_replace(src: &Path, dst: &Path) -> Result<(), String> {
    atomic_replace_io(src, dst).map_err(|e| {
        if cfg!(target_os = "windows") {
            format!("MoveFileExW failed: {}", e)
        } else {
            e.to_string()
        }
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionEntry {
    pub channel: String,
    pub version_hash: String,
    pub binary_type: String,
    #[serde(default)]
    pub display_version: Option<String>,
    pub install_path: String,
    #[serde(default)]
    pub install_size_bytes: u64,
    #[serde(default)]
    pub installed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_launched_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub user_label: Option<String>,
}

impl VersionEntry {
    pub fn version_id(&self) -> String {
        format!("{}:{}", self.channel, self.version_hash)
    }
}

/// Mesmo critério de `is_valid_channel_name` em `platform/windows/versions.rs`
/// (que já valida o canal antes de criar a pasta de instalação). Repetido aqui
/// porque aquele arquivo é Windows-only e o catálogo é multiplataforma.
///
/// Por que validar: `version_id()` junta canal e hash com `:`. Se o canal
/// contiver `:`, o id deixa de ser decomponível de forma única e a entrada some
/// das buscas. Trocar o separador não é opção — `channel:hash` já está gravado
/// em `RAMSettings.ini` (versão padrão / override por conta) e é remontado no
/// frontend (`VersionsDialog.tsx`) e no launch (`split_once(':')`), então mudar
/// o formato invalidaria catálogos e settings existentes. A saída é recusar o
/// canal inválido na entrada do catálogo e deixar `find` casar pelo próprio
/// `version_id()` (veja `VersionsCatalogStore::find`), o que ainda encontra
/// entradas legadas gravadas antes desta validação.
fn is_valid_catalog_channel(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        && value != "."
        && value != ".."
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VersionsCatalogFile {
    #[serde(default)]
    pub installed: Vec<VersionEntry>,
}

pub struct VersionsCatalogStore {
    catalog: Mutex<VersionsCatalogFile>,
    file_path: PathBuf,
    /// Ligado quando o arquivo existe mas não pôde ser lido/parseado. Enquanto
    /// ligado, gravar é recusado para que um catálogo vazio em memória nunca
    /// sobrescreva as versões do usuário — mesmo latch do `ScriptStore` e do
    /// `AccountStore`.
    load_failed: std::sync::atomic::AtomicBool,
}

impl VersionsCatalogStore {
    pub fn new(file_path: PathBuf) -> Self {
        let mut loaded = VersionsCatalogFile::default();
        let mut failed = false;

        if file_path.exists() {
            match Self::read_catalog_file(&file_path) {
                Ok(catalog) => loaded = catalog,
                Err(_) => failed = true,
            }
        }

        Self {
            catalog: Mutex::new(loaded),
            file_path,
            load_failed: std::sync::atomic::AtomicBool::new(failed),
        }
    }

    fn read_catalog_file(path: &Path) -> Result<VersionsCatalogFile, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read versions catalog: {}", e))?;
        // Arquivo de 0 byte é um catálogo vazio legítimo (primeira gravação
        // interrompida), não corrupção.
        if raw.trim().is_empty() {
            return Ok(VersionsCatalogFile::default());
        }
        serde_json::from_str::<VersionsCatalogFile>(&raw)
            .map_err(|e| format!("Failed to parse versions catalog: {}", e))
    }

    fn save_locked(&self, catalog: &VersionsCatalogFile) -> Result<(), String> {
        if self.load_failed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(
                "Versions catalog could not be read; refusing to overwrite it. Fix or restore RAMVersions.json and restart.".to_string(),
            );
        }

        let json = serde_json::to_string_pretty(catalog).map_err(|e| e.to_string())?;
        if let Some(parent) = self.file_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp_path = self.file_path.with_extension("json.tmp");
        std::fs::write(&tmp_path, json).map_err(|e| e.to_string())?;
        atomic_replace(&tmp_path, &self.file_path)
    }

    pub fn list(&self) -> Vec<VersionEntry> {
        self.catalog
            .lock()
            .map(|c| c.installed.clone())
            .unwrap_or_default()
    }

    /// Busca pelo próprio `version_id()` de cada entrada em vez de dividir a
    /// string no primeiro `:`. Assim a comparação é exatamente o inverso da
    /// montagem e entradas legadas com `:` no canal — gravadas antes de
    /// `is_valid_catalog_channel` existir — voltam a ser encontradas, sem mudar
    /// o formato do id nem do arquivo.
    pub fn find(&self, version_id: &str) -> Option<VersionEntry> {
        self.catalog.lock().ok().and_then(|c| {
            c.installed
                .iter()
                .find(|e| e.version_id() == version_id)
                .cloned()
        })
    }

    pub fn upsert(&self, entry: VersionEntry) -> Result<(), String> {
        if !is_valid_catalog_channel(&entry.channel) {
            return Err(format!(
                "Invalid channel name: must be alphanumeric, underscore, hyphen, or dot (got {:?})",
                entry.channel
            ));
        }

        let mut catalog = self.catalog.lock().map_err(|e| e.to_string())?;
        if let Some(existing) = catalog
            .installed
            .iter_mut()
            .find(|e| e.channel == entry.channel && e.version_hash == entry.version_hash)
        {
            *existing = entry;
        } else {
            catalog.installed.push(entry);
        }
        self.save_locked(&catalog)
    }

    pub fn remove(&self, channel: &str, version_hash: &str) -> Result<bool, String> {
        let mut catalog = self.catalog.lock().map_err(|e| e.to_string())?;
        let before = catalog.installed.len();
        catalog
            .installed
            .retain(|e| !(e.channel == channel && e.version_hash == version_hash));
        let removed = catalog.installed.len() != before;
        if removed {
            self.save_locked(&catalog)?;
        }
        Ok(removed)
    }

    pub fn set_label(
        &self,
        channel: &str,
        version_hash: &str,
        label: Option<String>,
    ) -> Result<(), String> {
        let mut catalog = self.catalog.lock().map_err(|e| e.to_string())?;
        if let Some(entry) = catalog
            .installed
            .iter_mut()
            .find(|e| e.channel == channel && e.version_hash == version_hash)
        {
            entry.user_label = label.filter(|l| !l.trim().is_empty());
            self.save_locked(&catalog)
        } else {
            Err("Version not found in catalog".into())
        }
    }

    pub fn touch_launched(&self, channel: &str, version_hash: &str) {
        if let Ok(mut catalog) = self.catalog.lock() {
            if let Some(entry) = catalog
                .installed
                .iter_mut()
                .find(|e| e.channel == channel && e.version_hash == version_hash)
            {
                entry.last_launched_at = Some(Utc::now());
                let _ = self.save_locked(&catalog);
            }
        }
    }
}

fn legacy_versions_catalog_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        .join("RAMVersions.json")
}

pub fn get_versions_catalog_path() -> PathBuf {
    let modern = std::env::var_os("LOCALAPPDATA").map(|local| {
        PathBuf::from(local)
            .join("Roblox Account Manager")
            .join("RAMVersions.json")
    });
    let Some(modern) = modern else {
        return legacy_versions_catalog_path();
    };

    if !modern.exists() {
        let legacy = legacy_versions_catalog_path();
        if legacy.exists() {
            if let Some(parent) = modern.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::copy(&legacy, &modern);
        }
    }

    modern
}

pub fn ram_managed_versions_root() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(
        PathBuf::from(local)
            .join("Roblox Account Manager")
            .join("RobloxVersions"),
    )
}
