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
