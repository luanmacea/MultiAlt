use std::io::Read;
use futures_util::future::{BoxFuture, FutureExt};
use futures_util::stream::{FuturesUnordered, StreamExt};
use tauri::Manager;

const CDN_HOST: &str = "https://setup-aws.rbxcdn.com";
const WEAO_CURRENT_URL: &str = "https://weao.xyz/api/versions/current";
const WEAO_PAST_URL: &str = "https://weao.xyz/api/versions/past";

const EXTRACT_ROOTS: &[(&str, &str)] = &[
    ("RobloxApp.zip", ""),
    ("redist.zip", ""),
    ("WebView2.zip", ""),
    ("Libraries.zip", ""),
    ("LibrariesQt5.zip", ""),
    ("shaders.zip", "shaders/"),
    ("ssl.zip", "ssl/"),
    ("WebView2RuntimeInstaller.zip", "WebView2RuntimeInstaller/"),
    ("content-avatar.zip", "content/avatar/"),
    ("content-configs.zip", "content/configs/"),
    ("content-fonts.zip", "content/fonts/"),
    ("content-models.zip", "content/models/"),
    ("content-music.zip", "content/music/"),
    ("content-particles.zip", "content/particles/"),
    ("content-sky.zip", "content/sky/"),
    ("content-sounds.zip", "content/sounds/"),
    ("content-textures.zip", "content/textures/"),
    ("content-textures2.zip", "content/textures/"),
    ("content-textures3.zip", "PlatformContent/pc/textures/"),
    ("content-terrain.zip", "PlatformContent/pc/terrain/"),
    ("content-platform-fonts.zip", "PlatformContent/pc/fonts/"),
    (
        "content-platform-dictionaries.zip",
        "PlatformContent/pc/shared_compression_dictionaries/",
    ),
    ("content-platform-shaders.zip", "PlatformContent/pc/shaders/"),
    ("extracontent-luapackages.zip", "ExtraContent/LuaPackages/"),
    ("extracontent-models.zip", "ExtraContent/models/"),
    ("extracontent-places.zip", "ExtraContent/places/"),
    ("extracontent-scripts.zip", "ExtraContent/scripts/"),
    ("extracontent-textures.zip", "ExtraContent/textures/"),
    ("extracontent-translations.zip", "ExtraContent/translations/"),
    ("Plugins.zip", "Plugins/"),
];

const APP_SETTINGS_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<Settings>
\t<ContentFolder>content</ContentFolder>
\t<BaseUrl>http://www.roblox.com</BaseUrl>
</Settings>
";

fn extract_root_for(filename: &str) -> Option<&'static str> {
    for (name, root) in EXTRACT_ROOTS {
        if name.eq_ignore_ascii_case(filename) {
            return Some(*root);
        }
    }
    None
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInstallProgress {
    pub install_id: String,
    pub channel: String,
    pub version_hash: String,
    pub stage: String,
    pub package: Option<String>,
    pub current: u64,
    pub total: u64,
    pub message: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteVersionEntry {
    pub binary_type: String,
    pub version_hash: String,
    pub display_version: Option<String>,
    pub deploy_date: Option<String>,
    pub channel: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCatalog {
    pub current: Vec<RemoteVersionEntry>,
    pub past: Vec<RemoteVersionEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub past_error: Option<String>,
}

fn channel_base_url(channel: &str) -> String {
    let trimmed = channel.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("LIVE") {
        format!("{}/", CDN_HOST)
    } else {
        format!("{}/channel/{}/", CDN_HOST, trimmed.to_ascii_lowercase())
    }
}

fn fallback_common_base_url() -> String {
    format!("{}/channel/common/", CDN_HOST)
}

/// Cliente do catálogo e do **download de build**, com teto próprio e maior
/// (`DOWNLOAD_REQUEST_TIMEOUT`, os mesmos 180 s de sempre): um zip de build passa
/// de 100 MB e o teto de uma chamada de API cortaria um download que ia bem. O
/// que ganhou teto aqui foi o handshake — sem `connect_timeout`, um socket que
/// aceita e nunca fala TLS prendia o launch os 3 minutos inteiros.
async fn http_client_versioned() -> reqwest::Client {
    // `expect` e não `unwrap_or_else(reqwest::Client::new)`: o fallback antigo
    // devolvia um cliente **sem teto nenhum** justamente no download, que é o
    // caminho mais longo do launch. `build()` só falha se o TLS não inicializar.
    crate::api::http_client::download_builder()
        .user_agent("RobloxAccountManager/4")
        .build()
        .expect("TLS backend for the Roblox build downloader")
}

async fn fetch_text(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| crate::api::http_client::describe_error(&e))?;
    if !response.status().is_success() {
        return Err(format!(
            "HTTP {} for {}",
            response.status().as_u16(),
            url
        ));
    }
    response
        .text()
        .await
        .map_err(|e| format!("Could not read response: {}", e))
}

/// Quantas vezes um download da build é tentado e quanto se espera entre as
/// tentativas (a espera dobra a cada falha).
#[derive(Debug, Clone, Copy)]
struct RetryPolicy {
    attempts: u32,
    base_delay: Duration,
}

impl RetryPolicy {
    /// Espera antes da próxima tentativa, depois de `failures` falhas. `None`
    /// quando as tentativas acabaram.
    fn delay_after(&self, failures: u32) -> Option<Duration> {
        if failures == 0 || failures >= self.attempts {
            return None;
        }
        Some(self.base_delay.saturating_mul(1u32 << (failures - 1).min(16)))
    }
}

/// Pacote da build (e o manifesto): 4 tentativas, 1 s / 2 s / 4 s entre elas.
/// Antes era uma só — um 5xx ou um timeout num dos ~30 pacotes abortava a
/// instalação inteira e o launch caía no instalador do Roblox, que fecha todos
/// os clientes. Só o CDN do Roblox, só HTTPS, hash conferido a cada tentativa.
const PACKAGE_RETRY_POLICY: RetryPolicy = RetryPolicy {
    attempts: 4,
    base_delay: Duration::from_secs(1),
};

/// Status que valem outra tentativa: os passageiros. 404/403 dizem que o
/// arquivo não está lá — repetir só atrasaria o erro (e o fallback do canal
/// `common` no manifesto).
fn status_is_retryable(status: u16) -> bool {
    matches!(status, 408 | 425 | 429) || (500..=599).contains(&status)
}

/// Uma falha de download e se vale tentar de novo.
struct FetchFailure {
    message: String,
    retryable: bool,
}

async fn fetch_bytes_once(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, FetchFailure> {
    let response = client.get(url).send().await.map_err(|e| FetchFailure {
        message: crate::api::http_client::describe_error(&e),
        retryable: true,
    })?;
    let status = response.status().as_u16();
    if !response.status().is_success() {
        return Err(FetchFailure {
            message: format!("HTTP {} for {}", status, url),
            retryable: status_is_retryable(status),
        });
    }
    let bytes = response.bytes().await.map_err(|e| FetchFailure {
        message: crate::api::http_client::describe_error(&e),
        retryable: true,
    })?;
    Ok(bytes.to_vec())
}

/// Baixa `url` com novas tentativas e, havendo hash no manifesto, só aceita
/// bytes que batem com ele — bytes corrompidos no caminho contam como falha
/// passageira e são baixados de novo.
async fn fetch_package_verified(
    client: &reqwest::Client,
    url: &str,
    expected_hash: Option<&str>,
    policy: &RetryPolicy,
) -> Result<Vec<u8>, String> {
    let mut failures = 0u32;
    loop {
        let failure = match fetch_bytes_once(client, url).await {
            Ok(bytes) => match expected_hash {
                Some(expected) if !verify_hash(&bytes, expected) => FetchFailure {
                    message: format!(
                        "Package {} failed integrity check (expected hash {})",
                        url.rsplit('/').next().unwrap_or(url),
                        expected
                    ),
                    retryable: true,
                },
                _ => return Ok(bytes),
            },
            Err(failure) => failure,
        };
        failures += 1;
        match policy.delay_after(failures).filter(|_| failure.retryable) {
            Some(delay) => {
                eprintln!(
                    "Roblox build download: {} (attempt {}/{}), retrying",
                    failure.message, failures, policy.attempts
                );
                tokio::time::sleep(delay).await;
            }
            None => return Err(failure.message),
        }
    }
}

/// Texto (o manifesto) com as mesmas novas tentativas dos pacotes.
async fn fetch_text_with_retry(
    client: &reqwest::Client,
    url: &str,
    policy: &RetryPolicy,
) -> Result<String, String> {
    let bytes = fetch_package_verified(client, url, None, policy).await?;
    String::from_utf8(bytes).map_err(|e| format!("Could not read response: {}", e))
}

pub async fn fetch_remote_catalog() -> Result<RemoteCatalog, String> {
    let client = http_client_versioned().await;
    let current_raw = fetch_text(&client, WEAO_CURRENT_URL).await?;
    let (past_raw, mut past_error): (String, Option<String>) =
        match fetch_text(&client, WEAO_PAST_URL).await {
            Ok(text) => (text, None),
            Err(err) => (String::new(), Some(err)),
        };

    let current: serde_json::Value =
        serde_json::from_str(&current_raw).map_err(|e| format!("Parse error: {}", e))?;
    let past: serde_json::Value = if past_raw.is_empty() {
        serde_json::json!({})
    } else {
        match serde_json::from_str(&past_raw) {
            Ok(value) => value,
            Err(err) => {
                past_error = Some(format!("Parse error: {}", err));
                serde_json::json!({})
            }
        }
    };

    Ok(RemoteCatalog {
        current: parse_weao_entries(&current),
        past: parse_weao_entries(&past),
        past_error,
    })
}

/// Turn one weao.xyz versions document into catalog entries. Split out of
/// [`fetch_remote_catalog`] (which only fetches) so the shape handling can be
/// tested without network access. Missing or non-string fields are skipped.
fn parse_weao_entries(value: &serde_json::Value) -> Vec<RemoteVersionEntry> {
    let mut out = Vec::new();
    for (hash_key, response_key, date_key, binary_type) in [
        ("Windows", "WindowsResponse", "WindowsDate", "WindowsPlayer"),
        ("Mac", "MacResponse", "MacDate", "MacPlayer"),
    ] {
        if let Some(v) = value.get(hash_key).and_then(|v| v.as_str()) {
            out.push(RemoteVersionEntry {
                binary_type: binary_type.into(),
                version_hash: v.to_string(),
                display_version: value
                    .get(response_key)
                    .and_then(|r| r.get("version"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                deploy_date: value
                    .get(date_key)
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                channel: "LIVE".into(),
            });
        }
    }
    out
}

#[derive(Debug, Clone)]
pub struct PkgManifestEntry {
    pub filename: String,
    pub hash: Option<String>,
}

fn parse_pkg_manifest(text: &str) -> Result<Vec<PkgManifestEntry>, String> {
    let mut lines = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty());
    let header = lines.next().ok_or("Empty manifest")?;
    if !header.eq_ignore_ascii_case("v0") {
        return Err(format!("Unexpected manifest header: {}", header));
    }
    let rest: Vec<&str> = lines.collect();
    let mut packages = Vec::new();
    let mut i = 0;
    while i + 3 < rest.len() {
        let filename = rest[i];
        if !filename.to_ascii_lowercase().ends_with(".zip") {
            i += 1;
            continue;
        }
        let hash_candidate = rest[i + 1];
        let hash = if hash_candidate.chars().all(|c| c.is_ascii_hexdigit())
            && (hash_candidate.len() == 32 || hash_candidate.len() == 64)
        {
            Some(hash_candidate.to_string())
        } else {
            None
        };
        packages.push(PkgManifestEntry {
            filename: filename.to_string(),
            hash,
        });
        i += 4;
    }
    Ok(packages)
}

fn verify_hash(bytes: &[u8], expected: &str) -> bool {
    use md5::Digest;
    match expected.len() {
        32 => {
            let mut hasher = md5::Md5::new();
            hasher.update(bytes);
            let computed = hasher.finalize();
            let hex: String = computed.iter().map(|b| format!("{:02x}", b)).collect();
            hex.eq_ignore_ascii_case(expected)
        }
        64 => {
            let mut hasher = sha2::Sha256::new();
            hasher.update(bytes);
            let computed = hasher.finalize();
            let hex: String = computed.iter().map(|b| format!("{:02x}", b)).collect();
            hex.eq_ignore_ascii_case(expected)
        }
        // An unrecognised hash length must not count as "verified": the only
        // safe reading of a hash we cannot check is failure. Entries with no
        // hash at all are `None` and never reach this function.
        _ => false,
    }
}

fn ensure_dir(dir: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("mkdir failed: {}", e))
}

fn is_valid_channel_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        && value != "."
        && value != ".."
}

fn is_valid_version_hash(value: &str) -> bool {
    value.len() >= 9
        && value.len() <= 96
        && value.starts_with("version-")
        && value[8..].chars().all(|c| c.is_ascii_hexdigit())
}

fn install_target_dir(channel: &str, version_hash: &str) -> Result<PathBuf, String> {
    if !is_valid_channel_name(channel) {
        return Err(format!(
            "Invalid channel name: must be alphanumeric, underscore, hyphen, or dot (got {:?})",
            channel
        ));
    }
    if !is_valid_version_hash(version_hash) {
        return Err(format!(
            "Invalid version hash: must look like 'version-<hex>' (got {:?})",
            version_hash
        ));
    }
    let root = versions_root_dir().ok_or_else(|| "Could not resolve LOCALAPPDATA".to_string())?;
    Ok(root.join(channel).join(version_hash))
}

fn versions_root_dir() -> Option<PathBuf> {
    crate::data::versions::ram_managed_versions_root()
}

fn emit_progress(app: &tauri::AppHandle, progress: &VersionInstallProgress) {
    let _ = app.emit("version-install-progress", progress);
}

fn folder_size(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total = total.saturating_add(meta.len());
                } else if meta.is_dir() {
                    total = total.saturating_add(folder_size(&entry.path()));
                }
            }
        }
    }
    total
}

fn extract_package_into(
    archive_bytes: &[u8],
    target_root: &std::path::Path,
    subdir: &str,
) -> Result<(), String> {
    let cursor = std::io::Cursor::new(archive_bytes);
    let mut zip = zip::ZipArchive::new(cursor).map_err(|e| format!("Bad zip: {}", e))?;

    let prefix = if subdir.is_empty() {
        std::path::PathBuf::new()
    } else {
        std::path::PathBuf::from(subdir.replace('/', std::path::MAIN_SEPARATOR_STR))
    };
    let base = target_root.join(&prefix);
    ensure_dir(&base)?;

    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| format!("Could not read zip entry: {}", e))?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out_path = base.join(rel);

        if entry.is_dir() {
            ensure_dir(&out_path)?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            ensure_dir(parent)?;
        }

        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut buf)
            .map_err(|e| format!("Could not read entry: {}", e))?;
        std::fs::write(&out_path, &buf)
            .map_err(|e| format!("Could not write {}: {}", out_path.display(), e))?;
    }

    Ok(())
}

struct StagingGuard {
    staging: PathBuf,
    committed: bool,
}

impl Drop for StagingGuard {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        if self.staging.exists() {
            let _ = std::fs::remove_dir_all(&self.staging);
        }
    }
}

/// Downloads a Roblox build and installs it into `target` (staged, then moved
/// into place atomically). Shared by the Versions UI installer and by the
/// launcher, which uses it to install a missing production build itself
/// instead of falling back to Roblox's own installer — that installer runs in
/// the foreground and closes every running client.
///
/// `progress(stage, package, current, total)` is called as work advances.
pub(crate) async fn install_build_to_dir(
    channel: &str,
    version_hash: &str,
    target: &std::path::Path,
    max_parallel: usize,
    progress: &(dyn Fn(&str, Option<String>, u64, u64) + Send + Sync),
) -> Result<(), String> {
    let staging_name = format!(
        "{}.staging-{}",
        target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("install"),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let staging = target.with_file_name(staging_name);
    if let Some(parent) = staging.parent() {
        ensure_dir(parent)?;
    }
    if staging.exists() {
        std::fs::remove_dir_all(&staging)
            .map_err(|e| format!("Could not clear stale staging directory: {}", e))?;
    }
    ensure_dir(&staging)?;
    let mut guard = StagingGuard {
        staging: staging.clone(),
        committed: false,
    };

    let client = http_client_versioned().await;

    progress("resolving", None, 0, 0);

    let mut base = channel_base_url(channel);
    let manifest_name = format!("{}-rbxPkgManifest.txt", version_hash);

    let manifest_url = format!("{}{}", base, manifest_name);
    let manifest_text = match fetch_text_with_retry(&client, &manifest_url, &PACKAGE_RETRY_POLICY).await {
        Ok(t) => t,
        Err(_) if !channel.eq_ignore_ascii_case("LIVE") => {
            base = fallback_common_base_url();
            fetch_text_with_retry(
                &client,
                &format!("{}{}", base, manifest_name),
                &PACKAGE_RETRY_POLICY,
            )
            .await?
        }
        Err(e) => return Err(e),
    };

    let packages = parse_pkg_manifest(&manifest_text)?;
    if packages.is_empty() {
        return Err("Manifest contained no packages".into());
    }

    let total = packages.len() as u64;
    let max_parallel = max_parallel.clamp(1, 8);

    let mut completed: u64 = 0;
    let mut pipeline: FuturesUnordered<BoxFuture<'static, Result<String, String>>> =
        FuturesUnordered::new();
    let mut iter = packages.iter().cloned();
    let spawn_one = |entry: PkgManifestEntry,
                     client: reqwest::Client,
                     base: String,
                     version_hash: String,
                     staging: PathBuf| {
        async move {
            let url = format!("{}{}-{}", base, version_hash, entry.filename);
            // Cada pacote tenta de novo sozinho (com o hash conferido a cada
            // vez) antes de a instalação inteira desistir.
            let bytes = fetch_package_verified(
                &client,
                &url,
                entry.hash.as_deref(),
                &PACKAGE_RETRY_POLICY,
            )
            .await?;
            let filename = entry.filename.clone();
            tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
                let subdir = extract_root_for(&filename).unwrap_or("");
                extract_package_into(&bytes, &staging, subdir)?;
                Ok(filename)
            })
            .await
            .map_err(|e| format!("Extraction task panicked: {}", e))?
        }
        .boxed()
    };

    for _ in 0..max_parallel.min(packages.len()) {
        if let Some(entry) = iter.next() {
            pipeline.push(spawn_one(
                entry,
                client.clone(),
                base.clone(),
                version_hash.to_string(),
                staging.clone(),
            ));
        }
    }

    while let Some(result) = pipeline.next().await {
        let pkg = result?;
        completed += 1;
        progress("installing", Some(pkg), completed, total);
        if let Some(next_entry) = iter.next() {
            pipeline.push(spawn_one(
                next_entry,
                client.clone(),
                base.clone(),
                version_hash.to_string(),
                staging.clone(),
            ));
        }
    }

    let app_settings_path = staging.join("AppSettings.xml");
    std::fs::write(&app_settings_path, APP_SETTINGS_XML)
        .map_err(|e| format!("Could not write AppSettings.xml: {}", e))?;

    let exe_path = staging.join("RobloxPlayerBeta.exe");
    if !exe_path.exists() {
        return Err("Install completed but RobloxPlayerBeta.exe is missing".into());
    }

    let backup_target = if target.exists() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let candidate = target.with_file_name(format!(
            "{}.old-{}",
            target.file_name().and_then(|n| n.to_str()).unwrap_or("version"),
            stamp
        ));
        std::fs::rename(target, &candidate).map_err(|e| {
            format!("Could not set aside existing version directory: {}", e)
        })?;
        Some(candidate)
    } else {
        None
    };

    if let Err(e) = std::fs::rename(&staging, target) {
        if let Some(backup) = &backup_target {
            let _ = std::fs::rename(backup, target);
        }
        return Err(format!("Could not move staged install into place: {}", e));
    }
    guard.committed = true;
    if let Some(backup) = backup_target {
        let _ = std::fs::remove_dir_all(&backup);
    }

    Ok(())
}

pub async fn install_version(
    app: tauri::AppHandle,
    install_id: String,
    channel: String,
    version_hash: String,
    label: Option<String>,
) -> Result<crate::data::versions::VersionEntry, String> {
    use crate::data::versions::VersionEntry;

    let target = install_target_dir(&channel, &version_hash)?;

    let max_parallel = app
        .state::<crate::data::settings::SettingsStore>()
        .get_int("Versions", "MaxParallelDownloads")
        .unwrap_or(4)
        .clamp(1, 8) as usize;

    // Package count, captured from progress, so the final "ready" event can
    // report the same total the UI has been counting up to.
    let total_packages = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    {
        let app = app.clone();
        let install_id = install_id.clone();
        let ev_channel = channel.clone();
        let ev_version_hash = version_hash.clone();
        let total_packages = total_packages.clone();
        let progress = move |stage: &str, package: Option<String>, current: u64, total: u64| {
            total_packages.store(total, std::sync::atomic::Ordering::SeqCst);
            emit_progress(
                &app,
                &VersionInstallProgress {
                    install_id: install_id.clone(),
                    channel: ev_channel.clone(),
                    version_hash: ev_version_hash.clone(),
                    stage: stage.into(),
                    package,
                    current,
                    total,
                    message: None,
                },
            );
        };
        install_build_to_dir(&channel, &version_hash, &target, max_parallel, &progress).await?;
    }

    let install_size = folder_size(&target);

    let entry = VersionEntry {
        channel: channel.clone(),
        version_hash: version_hash.clone(),
        binary_type: "WindowsPlayer".into(),
        display_version: None,
        install_path: target.to_string_lossy().into_owned(),
        install_size_bytes: install_size,
        installed_at: Some(chrono::Utc::now()),
        last_launched_at: None,
        user_label: label.filter(|l| !l.trim().is_empty()),
    };

    let store = app.state::<crate::data::versions::VersionsCatalogStore>();
    store.upsert(entry.clone()).map_err(|e| {
        format!(
            "Files installed at {} but catalog write failed: {}. Re-run install to register.",
            target.display(),
            e
        )
    })?;

    let total = total_packages.load(std::sync::atomic::Ordering::SeqCst);
    emit_progress(
        &app,
        &VersionInstallProgress {
            install_id,
            channel,
            version_hash,
            stage: "ready".into(),
            package: None,
            current: total,
            total,
            message: None,
        },
    );

    Ok(entry)
}

pub fn uninstall_version(channel: &str, version_hash: &str) -> Result<bool, String> {
    if let Ok(target) = install_target_dir(channel, version_hash) {
        if target.exists() {
            std::fs::remove_dir_all(&target)
                .map_err(|e| format!("Could not delete version folder: {}", e))?;
        }
    }
    Ok(true)
}

/// A pasta do Roblox para o launch, **sem nunca recusar**: a versão da conta,
/// senão a padrão, senão a instalação normal — e, sem nenhuma, caminho vazio
/// (o launch pelo protocolo baixa a build sozinho). Antes, uma versão
/// configurada que sumiu do catálogo (desinstalada, pasta apagada) recusava o
/// launch com erro; agora a próxima regra vale e o terceiro valor diz por quê,
/// em linhas do Console (pedido do dono, 23/10/2026: nada de erro que impede
/// jogar quando dá para jogar).
pub fn resolve_roblox_install_path_with_notice(
    account_version: Option<&str>,
    settings: &crate::data::settings::SettingsStore,
    catalog: &crate::data::versions::VersionsCatalogStore,
) -> (String, Option<String>, Vec<crate::log_msg::LogMsg>) {
    use crate::log_msg::LogMsg;
    let mut notices: Vec<LogMsg> = Vec::new();
    // `Err(true)`: não está mais no catálogo; `Err(false)`: faltam os arquivos.
    let usable = |version_id: &str| -> Result<crate::data::versions::VersionEntry, bool> {
        let entry = catalog.find(version_id).ok_or(true)?;
        if !std::path::Path::new(&entry.install_path).join("RobloxPlayerBeta.exe").exists() {
            return Err(false);
        }
        Ok(entry)
    };

    if let Some(version_id) = account_version.filter(|v| !v.trim().is_empty()) {
        match usable(version_id) {
            Ok(entry) => return (entry.install_path.clone(), Some(entry.version_id()), notices),
            Err(true) => notices.push(log_msg!(
                "The Roblox version {{version}} set for this account is not installed anymore, so the next available Roblox was opened.",
                version = version_id
            )),
            Err(false) => notices.push(log_msg!(
                "The Roblox version {{version}} set for this account is missing its files, so the next available Roblox was opened.",
                version = version_id
            )),
        }
    }

    let default = settings.get_string("Versions", "DefaultVersion");
    if !default.trim().is_empty() {
        match usable(&default) {
            Ok(entry) => return (entry.install_path.clone(), Some(entry.version_id()), notices),
            Err(true) => notices.push(log_msg!(
                "The default Roblox version {{version}} is not installed anymore, so the next available Roblox was opened.",
                version = default.as_str()
            )),
            Err(false) => notices.push(log_msg!(
                "The default Roblox version {{version}} is missing its files, so the next available Roblox was opened.",
                version = default.as_str()
            )),
        }
    }

    (get_roblox_path().unwrap_or_default(), None, notices)
}

/// `resolve_roblox_install_path_with_notice` sem o aviso. Nunca falha; o
/// `Result` fica pelos chamadores antigos.
pub fn resolve_roblox_install_path(
    account_version: Option<&str>,
    settings: &crate::data::settings::SettingsStore,
    catalog: &crate::data::versions::VersionsCatalogStore,
) -> Result<(String, Option<String>), String> {
    let (path, id, _) = resolve_roblox_install_path_with_notice(account_version, settings, catalog);
    Ok((path, id))
}

/// Versão do catálogo que ficou para trás da build atual do canal dela.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutdatedVersion {
    pub version_id: String,
    pub label: Option<String>,
    pub current: String,
}

/// A versão do catálogo `version_id` quando ela **não é mais a build atual**
/// do canal dela (31/10/2026, relato do Discord: contas banidas por "using a
/// disallowed version of Roblox"). Uma versão instalada pela tela Versions
/// envelhece no primeiro update do Roblox. `None` quando é a atual, quando a
/// consulta de versão não respondeu (não dá para afirmar que é velha) ou com
/// `Versions.AllowOutdated` ligado (a pessoa liberou sem perguntar).
pub async fn outdated_catalog_version(
    version_id: Option<&str>,
    settings: &crate::data::settings::SettingsStore,
    catalog: &crate::data::versions::VersionsCatalogStore,
) -> Option<OutdatedVersion> {
    let entry = version_id.and_then(|v| catalog.find(v))?;
    let allow_outdated = settings.get_bool("Versions", "AllowOutdated");
    if allow_outdated {
        return None;
    }
    let channel = if channel_is_production(&entry.channel) {
        PRODUCTION_CHANNEL.to_string()
    } else {
        entry.channel.clone()
    };
    let current = build_version_for_channel(&channel).await;
    let current = outdated_catalog_build(&entry.version_hash, current.as_deref(), allow_outdated)?;
    Some(OutdatedVersion {
        version_id: entry.version_id(),
        label: entry.user_label.clone().filter(|l| !l.trim().is_empty()),
        current,
    })
}

/// A build atual quando `installed` ficou para trás dela; `None` quando a
/// instalada é a atual, quando não se sabe a atual ou quando a pessoa liberou
/// versões velhas.
fn outdated_catalog_build(installed: &str, current: Option<&str>, allow_outdated: bool) -> Option<String> {
    let current = current?.trim();
    (!allow_outdated && !current.is_empty() && !current.eq_ignore_ascii_case(installed.trim()))
        .then(|| current.to_string())
}


#[cfg(test)]
mod win_versions_tests {
    use super::*;

    // Relato do Discord (31/10/2026): contas banidas por "disallowed version".
    // Versão do catálogo que não é a build atual não abre, a não ser que a
    // pessoa libere versões velhas.
    #[test]
    fn an_outdated_catalog_version_gives_way_to_the_current_build() {
        assert_eq!(
            outdated_catalog_build("version-old", Some("version-new"), false).as_deref(),
            Some("version-new")
        );
        assert_eq!(outdated_catalog_build("version-new", Some("VERSION-NEW"), false), None);
        assert_eq!(outdated_catalog_build("version-old", Some("version-new"), true), None, "allowed by the user");
        assert_eq!(outdated_catalog_build("version-old", None, false), None, "unknown current build");
        assert_eq!(outdated_catalog_build("version-old", Some("  "), false), None);
    }

    // Pedido do dono (23/10/2026): uma versão configurada que sumiu do
    // catálogo recusava o launch. Agora vale a próxima regra, com aviso.
    #[test]
    fn a_missing_configured_version_never_blocks_the_launch() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ram-resolve-{nanos}"));
        std::fs::create_dir_all(&dir).expect("dir");
        let settings = crate::data::settings::SettingsStore::new(dir.join("s.ini"));
        let _ = settings.set("Versions", "DefaultVersion", "ghost-default");
        let catalog = crate::data::versions::VersionsCatalogStore::new(dir.join("v.json"));

        let (_, id, notices) = resolve_roblox_install_path_with_notice(Some("ghost-account"), &settings, &catalog);
        assert_eq!(id, None);
        let notice = notices.iter().map(|n| n.render()).collect::<Vec<_>>().join(" ");
        assert!(notice.contains("ghost-account"), "{notice}");
        assert!(notice.contains("ghost-default"), "{notice}");
        assert!(resolve_roblox_install_path(Some("ghost-account"), &settings, &catalog).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
    use std::io::Write as _;

    // ── temp-dir helpers ───────────────────────────────────────────────────
    // There is no `tempfile` dev-dependency in this crate, so mirror the
    // pattern used by `versions_atomic_tests` in data/versions.rs: a uniquely
    // named folder under the OS temp dir, removed by a RAII guard.

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("ram4-winver-{}-{}-{}", tag, nanos, n));
            std::fs::create_dir_all(&dir).expect("temp dir");
            Self(dir)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn md5_hex(bytes: &[u8]) -> String {
        use md5::Digest;
        let mut hasher = md5::Md5::new();
        hasher.update(bytes);
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(bytes);
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }

    /// Build a zip in memory from `(name, contents)` pairs. Names are written
    /// verbatim so traversal payloads survive into the archive.
    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, contents) in entries {
            writer.start_file(*name, options).expect("start_file");
            writer.write_all(contents).expect("write entry");
        }
        writer.finish().expect("finish zip").into_inner()
    }

    // ── parse_pkg_manifest ─────────────────────────────────────────────────

    #[test]
    fn parse_pkg_manifest_reads_a_valid_v0_manifest() {
        let manifest = concat!(
            "v0\n",
            "RobloxApp.zip\n",
            "0123456789abcdef0123456789abcdef\n",
            "1000\n",
            "2000\n",
            "shaders.zip\n",
            "fedcba9876543210fedcba9876543210\n",
            "30\n",
            "40\n"
        );
        let packages = parse_pkg_manifest(manifest).expect("valid manifest");
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].filename, "RobloxApp.zip");
        assert_eq!(
            packages[0].hash.as_deref(),
            Some("0123456789abcdef0123456789abcdef")
        );
        assert_eq!(packages[1].filename, "shaders.zip");
        assert_eq!(
            packages[1].hash.as_deref(),
            Some("fedcba9876543210fedcba9876543210")
        );
    }

    #[test]
    fn parse_pkg_manifest_accepts_crlf_and_surrounding_whitespace() {
        let manifest = concat!(
            "  v0  \r\n",
            "   RobloxApp.zip   \r\n",
            "  0123456789abcdef0123456789abcdef  \r\n",
            " 1000 \r\n",
            " 2000 \r\n"
        );
        let packages = parse_pkg_manifest(manifest).expect("valid manifest");
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].filename, "RobloxApp.zip");
        assert_eq!(
            packages[0].hash.as_deref(),
            Some("0123456789abcdef0123456789abcdef")
        );
    }

    #[test]
    fn parse_pkg_manifest_ignores_blank_lines_between_groups() {
        let manifest = "v0\n\n\nRobloxApp.zip\n\n0123456789abcdef0123456789abcdef\n1\n2\n\n";
        let packages = parse_pkg_manifest(manifest).expect("valid manifest");
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].filename, "RobloxApp.zip");
    }

    #[test]
    fn parse_pkg_manifest_accepts_a_sha256_hash() {
        let hash = sha256_hex(b"whatever");
        let manifest = format!("v0\nRobloxApp.zip\n{}\n1\n2\n", hash);
        let packages = parse_pkg_manifest(&manifest).expect("valid manifest");
        assert_eq!(packages[0].hash.as_deref(), Some(hash.as_str()));
    }

    #[test]
    fn parse_pkg_manifest_leaves_the_hash_empty_when_it_is_not_hex_of_the_right_length() {
        let manifest = "v0\nRobloxApp.zip\nnot-a-hash\n1\n2\n";
        let packages = parse_pkg_manifest(manifest).expect("valid manifest");
        assert_eq!(packages.len(), 1);
        assert!(
            packages[0].hash.is_none(),
            "a non-hex second line must not be taken as a hash"
        );
    }

    #[test]
    fn parse_pkg_manifest_rejects_a_wrong_header() {
        let err = parse_pkg_manifest("v1\nRobloxApp.zip\nx\n1\n2\n").unwrap_err();
        assert!(err.contains("Unexpected manifest header"), "got {}", err);
    }

    #[test]
    fn parse_pkg_manifest_accepts_the_header_in_any_case() {
        assert!(parse_pkg_manifest("V0\nRobloxApp.zip\nx\n1\n2\n").is_ok());
    }

    #[test]
    fn parse_pkg_manifest_rejects_an_empty_manifest() {
        assert_eq!(parse_pkg_manifest("").unwrap_err(), "Empty manifest");
        assert_eq!(
            parse_pkg_manifest("   \n\n\t\n").unwrap_err(),
            "Empty manifest"
        );
    }

    #[test]
    fn parse_pkg_manifest_drops_a_truncated_trailing_group() {
        // The second group is missing its last line, so there are not enough
        // lines left for a complete record.
        let manifest = concat!(
            "v0\n",
            "RobloxApp.zip\n",
            "0123456789abcdef0123456789abcdef\n",
            "1\n",
            "2\n",
            "shaders.zip\n",
            "fedcba9876543210fedcba9876543210\n",
            "3\n"
        );
        let packages = parse_pkg_manifest(manifest).expect("valid manifest");
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].filename, "RobloxApp.zip");
    }

    #[test]
    fn parse_pkg_manifest_skips_lines_that_are_not_zip_names() {
        let manifest = concat!(
            "v0\n",
            "junk-line\n",
            "RobloxApp.ZIP\n",
            "0123456789abcdef0123456789abcdef\n",
            "1\n",
            "2\n"
        );
        let packages = parse_pkg_manifest(manifest).expect("valid manifest");
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].filename, "RobloxApp.ZIP");
    }

    #[test]
    fn parse_pkg_manifest_returns_no_packages_for_a_header_only_manifest() {
        assert!(parse_pkg_manifest("v0\n").unwrap().is_empty());
    }

    // ── verify_hash ────────────────────────────────────────────────────────

    #[test]
    fn verify_hash_accepts_a_matching_md5() {
        let data = b"roblox package bytes";
        assert!(verify_hash(data, &md5_hex(data)));
    }

    #[test]
    fn verify_hash_accepts_a_matching_md5_in_uppercase() {
        let data = b"roblox package bytes";
        assert!(verify_hash(data, &md5_hex(data).to_ascii_uppercase()));
    }

    #[test]
    fn verify_hash_rejects_a_mismatched_md5() {
        let data = b"roblox package bytes";
        let other = md5_hex(b"different bytes");
        assert_eq!(other.len(), 32);
        assert!(!verify_hash(data, &other));

        // A single flipped character is still rejected.
        let mut flipped = md5_hex(data);
        let replacement = if flipped.starts_with('a') { "b" } else { "a" };
        flipped.replace_range(0..1, replacement);
        assert!(!verify_hash(data, &flipped));
    }

    #[test]
    fn verify_hash_uses_sha256_for_a_64_character_hash() {
        let data = b"roblox package bytes";
        assert!(verify_hash(data, &sha256_hex(data)));
        assert!(!verify_hash(data, &sha256_hex(b"other")));
    }

    #[test]
    fn verify_hash_rejects_a_hash_it_cannot_interpret() {
        // A hash we cannot check is not a verified package: treating it as one
        // would silently disable integrity checking for a malformed manifest.
        // Entries with no hash at all are `None` and never reach this function.
        assert!(!verify_hash(b"data", ""));
        assert!(!verify_hash(b"data", "garbage"));
        assert!(!verify_hash(b"data", &"a".repeat(40)));
    }

    // ── channel base URLs ──────────────────────────────────────────────────

    #[test]
    fn channel_base_url_uses_the_root_cdn_for_live_and_empty_channels() {
        let root = format!("{}/", CDN_HOST);
        assert_eq!(channel_base_url(""), root);
        assert_eq!(channel_base_url("   "), root);
        assert_eq!(channel_base_url("LIVE"), root);
        assert_eq!(channel_base_url("live"), root);
        assert_eq!(channel_base_url("  LiVe  "), root);
    }

    #[test]
    fn channel_base_url_lowercases_a_custom_channel_and_trims_it() {
        assert_eq!(
            channel_base_url("ZCanary"),
            format!("{}/channel/zcanary/", CDN_HOST)
        );
        assert_eq!(
            channel_base_url("  ZIntegration  "),
            format!("{}/channel/zintegration/", CDN_HOST)
        );
    }

    #[test]
    fn fallback_common_base_url_points_at_the_common_channel() {
        assert_eq!(
            fallback_common_base_url(),
            format!("{}/channel/common/", CDN_HOST)
        );
    }

    // ── extract_root_for ───────────────────────────────────────────────────

    #[test]
    fn extract_root_for_maps_every_known_package() {
        for (name, root) in EXTRACT_ROOTS {
            assert_eq!(
                extract_root_for(name),
                Some(*root),
                "package {} should extract into {:?}",
                name,
                root
            );
        }
    }

    #[test]
    fn extract_root_for_matches_case_insensitively() {
        assert_eq!(extract_root_for("robloxapp.zip"), Some(""));
        assert_eq!(
            extract_root_for("CONTENT-TEXTURES3.ZIP"),
            Some("PlatformContent/pc/textures/")
        );
    }

    #[test]
    fn extract_root_for_spot_checks_the_tricky_mappings() {
        // textures2 shares content/textures/ with textures; textures3 does not.
        assert_eq!(
            extract_root_for("content-textures.zip"),
            Some("content/textures/")
        );
        assert_eq!(
            extract_root_for("content-textures2.zip"),
            Some("content/textures/")
        );
        assert_eq!(
            extract_root_for("content-textures3.zip"),
            Some("PlatformContent/pc/textures/")
        );
        assert_eq!(
            extract_root_for("content-platform-dictionaries.zip"),
            Some("PlatformContent/pc/shared_compression_dictionaries/")
        );
    }

    #[test]
    fn extract_root_for_returns_none_for_an_unknown_package() {
        assert_eq!(extract_root_for("mystery.zip"), None);
        assert_eq!(extract_root_for(""), None);
        assert_eq!(extract_root_for("RobloxApp.zip.bak"), None);
    }

    // ── name / hash validation and path composition ────────────────────────

    #[test]
    fn is_valid_channel_name_accepts_ordinary_channel_names() {
        assert!(is_valid_channel_name("LIVE"));
        assert!(is_valid_channel_name("zcanary"));
        assert!(is_valid_channel_name("z-integration_2.1"));
        assert!(is_valid_channel_name(&"a".repeat(64)));
    }

    #[test]
    fn is_valid_channel_name_rejects_path_traversal_and_separators() {
        assert!(!is_valid_channel_name(""));
        assert!(!is_valid_channel_name("."));
        assert!(!is_valid_channel_name(".."));
        assert!(!is_valid_channel_name("../evil"));
        assert!(!is_valid_channel_name("..\\evil"));
        assert!(!is_valid_channel_name("a/b"));
        assert!(!is_valid_channel_name("a\\b"));
        assert!(!is_valid_channel_name("C:"));
        assert!(!is_valid_channel_name("with space"));
        assert!(!is_valid_channel_name(&"a".repeat(65)));
    }

    #[test]
    fn is_valid_version_hash_requires_the_version_prefix_and_hex_body() {
        assert!(is_valid_version_hash("version-abc123"));
        assert!(is_valid_version_hash("version-0"));
        assert!(!is_valid_version_hash("version-"));
        assert!(!is_valid_version_hash("version"));
        assert!(!is_valid_version_hash("Version-abc123"));
        assert!(!is_valid_version_hash("version-xyz"));
        assert!(!is_valid_version_hash("version-../escape"));
        assert!(!is_valid_version_hash(&format!("version-{}", "a".repeat(89))));
    }

    #[test]
    fn install_target_dir_composes_root_channel_version() {
        let Some(root) = versions_root_dir() else {
            return; // no LOCALAPPDATA in this environment
        };
        let target = install_target_dir("LIVE", "version-abcdef01").expect("valid target");
        assert_eq!(target, root.join("LIVE").join("version-abcdef01"));
        assert!(target.starts_with(&root));
    }

    #[test]
    fn install_target_dir_refuses_to_escape_the_versions_root() {
        for channel in ["..", "../..", "..\\..", "a/b", ""] {
            let err = install_target_dir(channel, "version-abcdef01").unwrap_err();
            assert!(
                err.contains("Invalid channel name"),
                "channel {:?}: {}",
                channel,
                err
            );
        }
        for hash in ["..", "version-../x", "notaversion", ""] {
            let err = install_target_dir("LIVE", hash).unwrap_err();
            assert!(
                err.contains("Invalid version hash"),
                "hash {:?}: {}",
                hash,
                err
            );
        }
    }

    // ── filesystem helpers ─────────────────────────────────────────────────

    #[test]
    fn ensure_dir_creates_nested_directories_and_is_idempotent() {
        let temp = TempDir::new("ensuredir");
        let nested = temp.path().join("a").join("b").join("c");
        ensure_dir(&nested).expect("first create");
        assert!(nested.is_dir());
        ensure_dir(&nested).expect("second create is a no-op");
        assert!(nested.is_dir());
    }

    #[test]
    fn folder_size_sums_files_recursively_and_ignores_missing_paths() {
        let temp = TempDir::new("foldersize");
        assert_eq!(folder_size(&temp.path().join("missing")), 0);
        assert_eq!(folder_size(temp.path()), 0);

        std::fs::write(temp.path().join("a.bin"), vec![0u8; 10]).unwrap();
        let sub = temp.path().join("sub").join("deeper");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("b.bin"), vec![0u8; 25]).unwrap();

        assert_eq!(folder_size(temp.path()), 35);
    }

    // ── extract_package_into ───────────────────────────────────────────────

    #[test]
    fn extract_package_into_writes_files_under_the_target_root() {
        let temp = TempDir::new("extract");
        let target = temp.path().join("install");
        let archive = build_zip(&[
            ("RobloxPlayerBeta.exe", b"MZ fake exe".as_slice()),
            ("nested/dir/file.txt", b"hello".as_slice()),
        ]);

        extract_package_into(&archive, &target, "").expect("extract");

        assert_eq!(
            std::fs::read(target.join("RobloxPlayerBeta.exe")).unwrap(),
            b"MZ fake exe"
        );
        assert_eq!(
            std::fs::read(target.join("nested").join("dir").join("file.txt")).unwrap(),
            b"hello"
        );
    }

    #[test]
    fn extract_package_into_honours_the_subdir_prefix() {
        let temp = TempDir::new("extractsub");
        let target = temp.path().join("install");
        let archive = build_zip(&[("pc/terrain.mesh", b"mesh".as_slice())]);

        extract_package_into(&archive, &target, "PlatformContent/pc/terrain/").expect("extract");

        let expected = target
            .join("PlatformContent")
            .join("pc")
            .join("terrain")
            .join("pc")
            .join("terrain.mesh");
        assert_eq!(std::fs::read(expected).unwrap(), b"mesh");
    }

    #[test]
    fn extract_package_into_creates_the_subdir_even_for_an_empty_archive() {
        let temp = TempDir::new("extractempty");
        let target = temp.path().join("install");
        extract_package_into(&build_zip(&[]), &target, "shaders/").expect("extract");
        assert!(target.join("shaders").is_dir());
    }

    #[test]
    fn extract_package_into_does_not_write_outside_the_target_root() {
        let temp = TempDir::new("traversal");
        let target = temp.path().join("install");
        let archive = build_zip(&[
            ("..\\evil.txt", b"pwned".as_slice()),
            ("../evil2.txt", b"pwned".as_slice()),
            ("../../evil3.txt", b"pwned".as_slice()),
            ("good.txt", b"ok".as_slice()),
        ]);

        extract_package_into(&archive, &target, "").expect("extract");

        // The only file written is the legitimate one.
        assert_eq!(std::fs::read(target.join("good.txt")).unwrap(), b"ok");
        for escaped in ["evil.txt", "evil2.txt", "evil3.txt"] {
            assert!(
                !temp.path().join(escaped).exists(),
                "{} escaped the target root",
                escaped
            );
            assert!(
                !temp.path().parent().unwrap().join(escaped).exists(),
                "{} escaped two levels up",
                escaped
            );
            assert!(
                !target.join(escaped).exists(),
                "{} was written at all",
                escaped
            );
        }
    }

    #[test]
    fn extract_package_into_rejects_bytes_that_are_not_a_zip() {
        let temp = TempDir::new("badzip");
        let err =
            extract_package_into(b"definitely not a zip", &temp.path().join("t"), "").unwrap_err();
        assert!(err.starts_with("Bad zip:"), "got {}", err);
    }

    // ── remote catalog parsing ─────────────────────────────────────────────

    #[test]
    fn parse_weao_entries_reads_windows_and_mac_builds() {
        let value = serde_json::json!({
            "Windows": "version-aabbccdd",
            "WindowsDate": "2024-01-02",
            "WindowsResponse": { "version": "0.600.1" },
            "Mac": "version-11223344",
            "MacDate": "2024-01-03",
            "MacResponse": { "version": "0.600.2" },
        });
        let entries = parse_weao_entries(&value);
        assert_eq!(entries.len(), 2);

        assert_eq!(entries[0].binary_type, "WindowsPlayer");
        assert_eq!(entries[0].version_hash, "version-aabbccdd");
        assert_eq!(entries[0].display_version.as_deref(), Some("0.600.1"));
        assert_eq!(entries[0].deploy_date.as_deref(), Some("2024-01-02"));
        assert_eq!(entries[0].channel, "LIVE");

        assert_eq!(entries[1].binary_type, "MacPlayer");
        assert_eq!(entries[1].version_hash, "version-11223344");
        assert_eq!(entries[1].display_version.as_deref(), Some("0.600.2"));
        assert_eq!(entries[1].deploy_date.as_deref(), Some("2024-01-03"));
    }

    #[test]
    fn parse_weao_entries_tolerates_missing_and_wrongly_typed_fields() {
        let value = serde_json::json!({
            "Windows": "version-aabbccdd",
            "WindowsDate": 20240102,            // not a string
            "WindowsResponse": "not-an-object", // no nested "version"
            "Mac": 12345,                       // not a string -> entry skipped
        });
        let entries = parse_weao_entries(&value);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].binary_type, "WindowsPlayer");
        assert!(entries[0].display_version.is_none());
        assert!(entries[0].deploy_date.is_none());
    }

    #[test]
    fn parse_weao_entries_returns_nothing_for_an_empty_document() {
        assert!(parse_weao_entries(&serde_json::json!({})).is_empty());
        assert!(parse_weao_entries(&serde_json::Value::Null).is_empty());
        assert!(parse_weao_entries(&serde_json::json!([1, 2, 3])).is_empty());
    }
}

#[cfg(test)]
mod build_download_retry_tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // Regressão (ideia 29): um pacote que falhava uma vez (timeout, 5xx, bytes
    // corrompidos no caminho) abortava a instalação inteira da build, e o
    // launch caía no instalador do Roblox, que fecha todos os clientes.

    fn fast() -> RetryPolicy {
        RetryPolicy {
            attempts: 4,
            base_delay: Duration::from_millis(1),
        }
    }

    fn md5_hex(bytes: &[u8]) -> String {
        use md5::Digest;
        let mut hasher = md5::Md5::new();
        hasher.update(bytes);
        hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect()
    }

    #[test]
    fn the_package_policy_retries_with_a_growing_backoff() {
        let policy = PACKAGE_RETRY_POLICY;
        assert!((3..=5).contains(&policy.attempts));
        let delays: Vec<Duration> = (1..policy.attempts)
            .filter_map(|n| policy.delay_after(n))
            .collect();
        assert_eq!(delays.len() as u32, policy.attempts - 1);
        assert!(delays.windows(2).all(|w| w[1] > w[0]), "{delays:?}");
        assert_eq!(policy.delay_after(policy.attempts), None);
    }

    #[test]
    fn only_transient_http_statuses_are_retried() {
        for status in [408, 425, 429, 500, 502, 503, 504] {
            assert!(status_is_retryable(status), "{status}");
        }
        for status in [400, 401, 403, 404, 410] {
            assert!(!status_is_retryable(status), "{status}");
        }
    }

    #[tokio::test]
    async fn a_package_that_fails_twice_still_installs() {
        let server = MockServer::start().await;
        let body = b"zip bytes".to_vec();
        Mock::given(method("GET"))
            .and(path("/pkg.zip"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(2)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/pkg.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
            .mount(&server)
            .await;
        let client = http_client_versioned().await;
        let got = fetch_package_verified(
            &client,
            &format!("{}/pkg.zip", server.uri()),
            Some(&md5_hex(&body)),
            &fast(),
        )
        .await
        .expect("a terceira tentativa devia passar");
        assert_eq!(got, body);
        assert_eq!(server.received_requests().await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn corrupted_bytes_are_downloaded_again() {
        let server = MockServer::start().await;
        let body = b"good bytes".to_vec();
        Mock::given(method("GET"))
            .and(path("/pkg.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"bad bytes".to_vec()))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/pkg.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
            .mount(&server)
            .await;
        let client = http_client_versioned().await;
        let got = fetch_package_verified(
            &client,
            &format!("{}/pkg.zip", server.uri()),
            Some(&md5_hex(&body)),
            &fast(),
        )
        .await
        .expect("o hash tem que ser conferido de novo e passar");
        assert_eq!(got, body);
    }

    #[tokio::test]
    async fn a_package_that_never_matches_its_hash_fails_after_the_last_attempt() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pkg.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"bad".to_vec()))
            .mount(&server)
            .await;
        let client = http_client_versioned().await;
        let err = fetch_package_verified(
            &client,
            &format!("{}/pkg.zip", server.uri()),
            Some(&md5_hex(b"good")),
            &fast(),
        )
        .await
        .unwrap_err();
        assert!(err.contains("integrity"), "{err}");
        assert_eq!(server.received_requests().await.unwrap().len(), 4);
    }

    #[tokio::test]
    async fn a_missing_package_is_not_retried() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pkg.zip"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let client = http_client_versioned().await;
        let err = fetch_package_verified(&client, &format!("{}/pkg.zip", server.uri()), None, &fast())
            .await
            .unwrap_err();
        assert!(err.contains("404"), "{err}");
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[test]
    fn downloads_stay_on_the_roblox_cdn_over_https() {
        assert_eq!(CDN_HOST, "https://setup-aws.rbxcdn.com");
    }
}
