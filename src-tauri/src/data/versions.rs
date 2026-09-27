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
/// app do ar não é.
///
/// `#[must_use]` porque descartar esse `bool` torna a degradação **invisível**, que
/// era o estado anterior: os dois chamadores jogavam fora o `Ok(false)` enquanto
/// esta doc afirmava que eles registravam a degradação.
#[must_use = "o `false` diz que o fsync não foi confirmado; registre isso em vez de descartar"]
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

#[cfg(test)]
mod versions_atomic_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ram-versions-{name}-{nanos}"));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn atomic_replace_overwrites_an_existing_destination() {
        let dir = unique_dir("replace");
        let src = dir.join("RAMVersions.json.tmp");
        let dst = dir.join("RAMVersions.json");

        std::fs::write(&dst, "old contents").expect("write dst");
        std::fs::write(&src, "new contents").expect("write src");

        atomic_replace(&src, &dst).expect("atomic_replace");

        assert_eq!(std::fs::read_to_string(&dst).unwrap(), "new contents");
        assert!(!src.exists(), "the temp file must not survive the replace");

        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftovers, vec!["RAMVersions.json".to_string()]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_replace_creates_the_destination_when_it_does_not_exist() {
        let dir = unique_dir("create");
        let src = dir.join("RAMVersions.json.tmp");
        let dst = dir.join("RAMVersions.json");

        std::fs::write(&src, "fresh").expect("write src");
        assert!(!dst.exists());

        atomic_replace(&src, &dst).expect("atomic_replace");

        assert_eq!(std::fs::read_to_string(&dst).unwrap(), "fresh");
        assert!(!src.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_replace_fails_when_the_source_is_missing() {
        let dir = unique_dir("missing");
        let src = dir.join("does-not-exist.tmp");
        let dst = dir.join("RAMVersions.json");

        assert!(atomic_replace(&src, &dst).is_err());
        assert!(!dst.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn version_id_joins_channel_and_hash() {
        let entry = VersionEntry {
            channel: "LIVE".into(),
            version_hash: "version-abc123".into(),
            binary_type: "WindowsPlayer".into(),
            display_version: None,
            install_path: String::new(),
            install_size_bytes: 0,
            installed_at: None,
            last_launched_at: None,
            user_label: None,
        };
        assert_eq!(entry.version_id(), "LIVE:version-abc123");
    }
}

#[cfg(test)]
mod versions_catalog_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("ram-catalog-{name}-{nanos}.json"))
    }

    struct TestStore {
        store: VersionsCatalogStore,
    }

    impl Drop for TestStore {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.store.file_path);
            let _ = std::fs::remove_file(self.store.file_path.with_extension("json.tmp"));
        }
    }

    impl std::ops::Deref for TestStore {
        type Target = VersionsCatalogStore;
        fn deref(&self) -> &VersionsCatalogStore {
            &self.store
        }
    }

    fn store(name: &str) -> TestStore {
        TestStore {
            store: VersionsCatalogStore::new(temp_path(name)),
        }
    }

    fn entry(channel: &str, hash: &str) -> VersionEntry {
        VersionEntry {
            channel: channel.to_string(),
            version_hash: hash.to_string(),
            binary_type: "WindowsPlayer".to_string(),
            display_version: Some("1.2.3.4".to_string()),
            install_path: format!("C:\\versions\\{hash}"),
            install_size_bytes: 1234,
            installed_at: Some(Utc::now()),
            last_launched_at: None,
            user_label: None,
        }
    }

    fn listed(store: &VersionsCatalogStore) -> Vec<String> {
        store.list().iter().map(|e| e.version_id()).collect()
    }

    // ---- version_id ------------------------------------------------------------

    #[test]
    fn version_id_is_channel_colon_hash_and_round_trips_through_find() {
        let s = store("version-id");
        s.upsert(entry("LIVE", "version-abc123")).unwrap();
        s.upsert(entry("zintegration", "version-def456")).unwrap();

        for id in ["LIVE:version-abc123", "zintegration:version-def456"] {
            let found = s.find(id).unwrap_or_else(|| panic!("{id} should be found"));
            assert_eq!(found.version_id(), id);
        }
    }

    #[test]
    fn find_parses_only_the_first_colon_so_the_hash_may_contain_one() {
        let s = store("find-colon");
        s.upsert(entry("LIVE", "weird:hash")).unwrap();

        // find() compara o version_id() inteiro, então um hash com ':' continua
        // encontrável e um prefixo qualquer continua não casando.
        assert!(s.find("LIVE:weird:hash").is_some());
        assert!(s.find("LIVE:weird").is_none());
    }

    #[test]
    fn upsert_rejects_a_channel_that_would_break_the_version_id_round_trip() {
        // Antes: o canal com ':' entrava no catálogo e nunca mais era achado
        // (version_id() monta "canal:hash" e find() dividia no primeiro ':').
        // Agora a entrada é recusada, e o formato "canal:hash" — que já está
        // gravado em settings e é remontado no frontend — fica intacto.
        let s = store("upsert-bad-channel");

        for bad in ["weird:channel", "", "a/b", "..", ".", "a b", "canal\u{00e7}"] {
            let err = match s.upsert(entry(bad, "hash1")) {
                Ok(()) => panic!("{bad:?} deve ser recusado"),
                Err(e) => e,
            };
            assert!(err.starts_with("Invalid channel name:"), "{err}");
        }
        assert!(s.list().is_empty(), "nada inválido entra no catálogo");
        assert!(!s.file_path.exists(), "um upsert recusado não grava");

        // Um canal longo demais também é recusado; no limite ainda passa.
        assert!(s.upsert(entry(&"a".repeat(65), "hash1")).is_err());
        s.upsert(entry(&"a".repeat(64), "hash1")).unwrap();
        for good in ["LIVE", "zcanary", "z-integration_2.1"] {
            s.upsert(entry(good, "hash1")).unwrap();
            assert!(s.find(&format!("{good}:hash1")).is_some());
        }
    }

    #[test]
    fn find_still_matches_a_legacy_entry_whose_channel_contains_a_colon() {
        // Catálogos gravados antes da validação podem ter um canal com ':'.
        // A busca por version_id() inteiro recupera essas entradas em vez de
        // escondê-las para sempre.
        let s = store("find-legacy-colon");
        std::fs::write(
            &s.file_path,
            br#"{"installed":[{"channel":"weird:channel","versionHash":"hash1","binaryType":"WindowsPlayer","installPath":"C:\\x"}]}"#,
        )
        .unwrap();

        let reopened = VersionsCatalogStore::new(s.file_path.clone());
        let found = reopened
            .find("weird:channel:hash1")
            .expect("a entrada legada precisa ser encontrada");
        assert_eq!(found.channel, "weird:channel");
        assert_eq!(found.version_hash, "hash1");
    }

    #[test]
    fn find_returns_none_for_malformed_or_unknown_ids() {
        let s = store("find-none");
        s.upsert(entry("LIVE", "hash1")).unwrap();

        for id in ["", "LIVE", "nocolon", "OTHER:hash1", "LIVE:other", "live:hash1"] {
            assert!(s.find(id).is_none(), "{id:?} must not match");
        }
        // An empty channel or hash is matched literally, not treated as a wildcard.
        assert!(s.find(":hash1").is_none());
        assert!(s.find("LIVE:").is_none());
    }

    // ---- upsert / list ---------------------------------------------------------

    #[test]
    fn upsert_appends_new_entries_and_replaces_matching_channel_and_hash() {
        let s = store("upsert");
        s.upsert(entry("LIVE", "hash1")).unwrap();
        s.upsert(entry("LIVE", "hash2")).unwrap();
        s.upsert(entry("zcanary", "hash1")).unwrap();

        assert_eq!(
            listed(&s),
            vec!["LIVE:hash1", "LIVE:hash2", "zcanary:hash1"],
            "channel and hash together are the key"
        );

        let mut replacement = entry("LIVE", "hash1");
        replacement.install_size_bytes = 999;
        replacement.user_label = Some("Replaced".to_string());
        s.upsert(replacement).unwrap();

        assert_eq!(s.list().len(), 3, "a replacement must not append");
        let found = s.find("LIVE:hash1").unwrap();
        assert_eq!(found.install_size_bytes, 999);
        assert_eq!(found.user_label.as_deref(), Some("Replaced"));
        assert_eq!(listed(&s)[0], "LIVE:hash1", "the slot is kept");
    }

    #[test]
    fn upsert_persists_to_disk_without_leaving_a_temp_file() {
        let s = store("upsert-persist");
        s.upsert(entry("LIVE", "hash1")).unwrap();
        s.upsert(entry("LIVE", "hash2")).unwrap();

        assert!(!s.file_path.with_extension("json.tmp").exists());

        let reloaded = VersionsCatalogStore::new(s.file_path.clone());
        assert_eq!(listed(&reloaded), vec!["LIVE:hash1", "LIVE:hash2"]);
        assert_eq!(
            reloaded.find("LIVE:hash1").unwrap().display_version.as_deref(),
            Some("1.2.3.4")
        );
    }

    #[test]
    fn upsert_creates_the_parent_directory_when_it_is_missing() {
        let dir = temp_path("nested-dir").with_extension("");
        let path = dir.join("nested").join("RAMVersions.json");
        let s = VersionsCatalogStore::new(path.clone());

        s.upsert(entry("LIVE", "hash1")).unwrap();
        assert!(path.exists(), "{}", path.display());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- remove ----------------------------------------------------------------

    #[test]
    fn remove_reports_whether_it_hit_and_only_writes_when_it_did() {
        let s = store("remove");
        assert!(!s.remove("LIVE", "hash1").unwrap());
        assert!(!s.file_path.exists(), "a missed remove must not write");

        s.upsert(entry("LIVE", "hash1")).unwrap();
        s.upsert(entry("LIVE", "hash2")).unwrap();

        assert!(!s.remove("LIVE", "nope").unwrap());
        assert!(!s.remove("OTHER", "hash1").unwrap());
        assert_eq!(s.list().len(), 2);

        assert!(s.remove("LIVE", "hash1").unwrap());
        assert_eq!(listed(&s), vec!["LIVE:hash2"]);
        assert!(!s.remove("LIVE", "hash1").unwrap(), "removing twice misses");

        let reloaded = VersionsCatalogStore::new(s.file_path.clone());
        assert_eq!(listed(&reloaded), vec!["LIVE:hash2"]);
    }

    // ---- set_label -------------------------------------------------------------

    #[test]
    fn set_label_stores_trims_and_clears_labels() {
        let s = store("set-label");
        s.upsert(entry("LIVE", "hash1")).unwrap();

        s.set_label("LIVE", "hash1", Some("My Build".to_string())).unwrap();
        assert_eq!(s.find("LIVE:hash1").unwrap().user_label.as_deref(), Some("My Build"));

        // A blank label clears it instead of storing whitespace.
        s.set_label("LIVE", "hash1", Some("   ".to_string())).unwrap();
        assert_eq!(s.find("LIVE:hash1").unwrap().user_label, None);

        s.set_label("LIVE", "hash1", Some("Back".to_string())).unwrap();
        s.set_label("LIVE", "hash1", None).unwrap();
        assert_eq!(s.find("LIVE:hash1").unwrap().user_label, None);

        let reloaded = VersionsCatalogStore::new(s.file_path.clone());
        assert_eq!(reloaded.find("LIVE:hash1").unwrap().user_label, None);
    }

    #[test]
    fn set_label_errors_for_an_unknown_version() {
        let s = store("set-label-missing");
        s.upsert(entry("LIVE", "hash1")).unwrap();
        assert_eq!(
            s.set_label("LIVE", "nope", Some("x".to_string())).unwrap_err(),
            "Version not found in catalog"
        );
        assert_eq!(
            s.set_label("OTHER", "hash1", None).unwrap_err(),
            "Version not found in catalog"
        );
    }

    // ---- touch_launched --------------------------------------------------------

    #[test]
    fn touch_launched_stamps_the_entry_and_orders_most_recently_used_last() {
        let s = store("touch");
        s.upsert(entry("LIVE", "old")).unwrap();
        s.upsert(entry("LIVE", "new")).unwrap();
        assert!(s.find("LIVE:old").unwrap().last_launched_at.is_none());

        s.touch_launched("LIVE", "old");
        std::thread::sleep(std::time::Duration::from_millis(5));
        s.touch_launched("LIVE", "new");

        let old = s.find("LIVE:old").unwrap().last_launched_at.expect("old stamped");
        let new = s.find("LIVE:new").unwrap().last_launched_at.expect("new stamped");
        assert!(new > old, "the later launch must have the later timestamp");

        // Most-recently-used selection is a plain sort over that field.
        let mut all = s.list();
        all.sort_by_key(|e| e.last_launched_at);
        assert_eq!(all.last().unwrap().version_hash, "new");

        let reloaded = VersionsCatalogStore::new(s.file_path.clone());
        assert!(reloaded.find("LIVE:new").unwrap().last_launched_at.is_some());
    }

    #[test]
    fn touch_launched_is_a_silent_no_op_for_an_unknown_version() {
        let s = store("touch-missing");
        s.touch_launched("LIVE", "never-installed");
        assert!(
            !s.file_path.exists(),
            "a no-op touch must not create the catalog"
        );

        s.upsert(entry("LIVE", "hash1")).unwrap();
        s.touch_launched("OTHER", "hash1");
        assert!(s.find("LIVE:hash1").unwrap().last_launched_at.is_none());
    }

    // ---- loading ---------------------------------------------------------------

    #[test]
    fn new_starts_empty_when_the_catalog_file_is_missing() {
        let s = store("load-missing");
        assert!(s.list().is_empty());
        assert!(!s.file_path.exists(), "opening must not create the file");
    }

    #[test]
    fn new_accepts_a_catalog_without_the_installed_key_and_entry_defaults() {
        let s = store("load-minimal");
        std::fs::write(&s.file_path, b"{}").unwrap();
        let reopened = VersionsCatalogStore::new(s.file_path.clone());
        assert!(reopened.list().is_empty());

        std::fs::write(
            &s.file_path,
            br#"{"installed":[{"channel":"LIVE","versionHash":"h","binaryType":"WindowsPlayer","installPath":"C:\\x"}]}"#,
        )
        .unwrap();
        let reopened = VersionsCatalogStore::new(s.file_path.clone());
        let e = reopened.find("LIVE:h").expect("entry");
        assert_eq!(e.install_size_bytes, 0);
        assert_eq!(e.display_version, None);
        assert_eq!(e.installed_at, None);
        assert_eq!(e.last_launched_at, None);
        assert_eq!(e.user_label, None);
    }

    #[test]
    fn a_corrupt_catalog_file_is_never_overwritten() {
        // Comportamento antigo (corrigido): `new` caía em `unwrap_or_default()`
        // e a primeira gravação apagava as versões do usuário. Agora a falha de
        // leitura fica latcheada e toda gravação é recusada até o arquivo ser
        // consertado.
        let s = store("load-corrupt");
        let corrupt = br#"{"installed": [ broken"#;
        std::fs::write(&s.file_path, corrupt).unwrap();

        let reopened = VersionsCatalogStore::new(s.file_path.clone());
        assert!(reopened.list().is_empty(), "corrupt catalog is ignored");

        let err = reopened
            .upsert(entry("LIVE", "hash1"))
            .expect_err("gravar sobre um catálogo ilegível deve falhar");
        assert!(err.contains("refusing to overwrite"), "{err}");
        assert_eq!(
            std::fs::read(&s.file_path).unwrap(),
            corrupt.to_vec(),
            "o arquivo do usuário fica intacto"
        );

        // set_label e remove passam pelo mesmo latch.
        std::fs::write(&s.file_path, br#"{"installed":[{"channel":"LIVE","versionHash":"hash1","binaryType":"WindowsPlayer","installPath":"C:\\x"}]}"#).unwrap();
        let healthy = VersionsCatalogStore::new(s.file_path.clone());
        healthy.set_label("LIVE", "hash1", Some("ok".into())).unwrap();
        assert_eq!(healthy.list().len(), 1);
    }

    #[test]
    fn a_catalog_with_the_wrong_shape_latches_instead_of_starting_empty() {
        let s = store("load-wrong-shape");
        let wrong = br#"{"installed": "not-an-array"}"#;
        std::fs::write(&s.file_path, wrong).unwrap();

        let reopened = VersionsCatalogStore::new(s.file_path.clone());
        assert!(reopened.list().is_empty());
        assert!(
            reopened.remove("LIVE", "hash1").is_ok(),
            "um remove que não acha nada não grava, então não falha"
        );
        assert!(reopened.upsert(entry("LIVE", "hash1")).is_err());
        assert_eq!(std::fs::read(&s.file_path).unwrap(), wrong.to_vec());

        // `[]` não é corrupção: o serde aceita uma sequência para uma struct e
        // `installed` cai no default, então isso é um catálogo vazio gravável.
        std::fs::write(&s.file_path, b"[]").unwrap();
        let as_array = VersionsCatalogStore::new(s.file_path.clone());
        assert!(as_array.list().is_empty());
        as_array.upsert(entry("LIVE", "hash1")).expect("deve gravar");
    }

    #[test]
    fn an_empty_catalog_file_is_not_treated_as_corrupt() {
        // Uma gravação interrompida pode deixar 0 byte; isso é um catálogo
        // vazio legítimo e precisa continuar gravável.
        let s = store("load-empty-file");
        std::fs::write(&s.file_path, b"").unwrap();

        let reopened = VersionsCatalogStore::new(s.file_path.clone());
        assert!(reopened.list().is_empty());
        reopened.upsert(entry("LIVE", "hash1")).expect("deve gravar");
        assert_eq!(
            VersionsCatalogStore::new(s.file_path.clone()).list().len(),
            1
        );
    }

    // ---- serde -----------------------------------------------------------------

    #[test]
    fn version_entry_serializes_in_camel_case() {
        let mut e = entry("LIVE", "hash1");
        e.installed_at = None;
        let json = serde_json::to_value(&e).unwrap();
        for key in [
            "channel",
            "versionHash",
            "binaryType",
            "displayVersion",
            "installPath",
            "installSizeBytes",
            "installedAt",
            "lastLaunchedAt",
            "userLabel",
        ] {
            assert!(json.get(key).is_some(), "missing key {key} in {json}");
        }
        assert_eq!(json["versionHash"], "hash1");
        assert_eq!(json["installSizeBytes"], 1234);
    }

    #[test]
    fn versions_catalog_file_defaults_to_an_empty_install_list() {
        let parsed: VersionsCatalogFile = serde_json::from_str("{}").unwrap();
        assert!(parsed.installed.is_empty());
        assert!(VersionsCatalogFile::default().installed.is_empty());
        let json = serde_json::to_string(&VersionsCatalogFile::default()).unwrap();
        assert_eq!(json, r#"{"installed":[]}"#);
    }

    // ---- path helpers ----------------------------------------------------------

    #[test]
    fn legacy_versions_catalog_path_sits_next_to_the_executable() {
        let path = legacy_versions_catalog_path();
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("RAMVersions.json")
        );
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap();
        assert_eq!(path.parent(), Some(exe_dir.as_path()));
    }

    #[test]
    fn ram_managed_versions_root_follows_localappdata() {
        match std::env::var_os("LOCALAPPDATA") {
            Some(local) => {
                let root = ram_managed_versions_root().expect("root");
                assert_eq!(
                    root,
                    PathBuf::from(local)
                        .join("Roblox Account Manager")
                        .join("RobloxVersions")
                );
            }
            None => assert!(ram_managed_versions_root().is_none()),
        }
    }
}
