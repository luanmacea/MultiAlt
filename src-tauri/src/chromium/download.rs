use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

const VERSIONS_URL: &str =
    "https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json";

static ENSURE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Clone, Serialize)]
struct DownloadProgress {
    stage: String,
    downloaded: u64,
    total: u64,
}

fn platform_key() -> &'static str {
    if cfg!(target_os = "windows") {
        if cfg!(target_arch = "x86") {
            "win32"
        } else {
            "win64"
        }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "mac-arm64"
        } else {
            "mac-x64"
        }
    } else {
        "linux64"
    }
}

fn binary_path(version_dir: &Path) -> PathBuf {
    let folder = format!("chrome-{}", platform_key());
    let base = version_dir.join(folder);
    if cfg!(target_os = "windows") {
        base.join("chrome.exe")
    } else if cfg!(target_os = "macos") {
        base.join("Google Chrome for Testing.app")
            .join("Contents")
            .join("MacOS")
            .join("Google Chrome for Testing")
    } else {
        base.join("chrome")
    }
}

pub fn chromium_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("Could not resolve data directory: {}", e))?
        .join("chromium");
    Ok(dir)
}

fn cached_binary(app: &AppHandle) -> Option<PathBuf> {
    let dir = chromium_dir(app).ok()?;
    let manifest = dir.join("version.json");
    let raw = std::fs::read_to_string(manifest).ok()?;
    let json: Value = serde_json::from_str(&raw).ok()?;
    let binary = json.get("binary").and_then(Value::as_str)?;
    let path = PathBuf::from(binary);
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

pub fn is_installed(app: &AppHandle) -> bool {
    cached_binary(app).is_some()
}

/// De onde veio o binário que `resolve_browser_binary` decidiu usar — só para
/// o aviso na tela; a chamada ao CDP é igual para os três.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserSource {
    /// Cópia própria do Chromium (baixada e cacheada por este módulo).
    Downloaded,
    /// Caminho que o usuário digitou em Settings (`Login.ManualBinaryPath`).
    Manual,
    /// Chrome/Edge/Chromium/Brave já instalado, achado sozinho quando o
    /// download falha e não há caminho manual configurado.
    System(String),
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FallbackNotice {
    browser: String,
    error: String,
}

/// `true` só para um arquivo comum, checado **sem seguir link** —
/// `symlink_metadata` (ao contrário de `Path::exists`/`is_file`, que resolvem
/// o link) enxerga o link em si. O Chromium é usado para login e criação de
/// conta, então um candidato de navegador — vindo de detecção automática ou
/// digitado pelo usuário em Settings — nunca pode ser uma pasta nem um link
/// simbólico apontando para outro lugar.
fn is_regular_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|meta| meta.is_file())
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn system_chromium_candidates() -> Vec<(PathBuf, &'static str)> {
    let roots: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
        .iter()
        .filter_map(|var| std::env::var(var).ok())
        .map(PathBuf::from)
        .collect();

    // Ordem = ordem de preferência: todo Chrome instalado vence todo Edge.
    let relative: [(&[&str], &str); 4] = [
        (&["Google", "Chrome", "Application", "chrome.exe"], "Google Chrome"),
        (&["Chromium", "Application", "chrome.exe"], "Chromium"),
        (
            &["BraveSoftware", "Brave-Browser", "Application", "brave.exe"],
            "Brave",
        ),
        (&["Microsoft", "Edge", "Application", "msedge.exe"], "Microsoft Edge"),
    ];

    let mut out = Vec::new();
    for (parts, name) in relative {
        for root in &roots {
            let mut path = root.clone();
            for part in parts {
                path = path.join(part);
            }
            out.push((path, name));
        }
    }
    out
}

#[cfg(target_os = "macos")]
fn system_chromium_candidates() -> Vec<(PathBuf, &'static str)> {
    [
        (
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "Google Chrome",
        ),
        ("/Applications/Chromium.app/Contents/MacOS/Chromium", "Chromium"),
        (
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
            "Brave",
        ),
        (
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "Microsoft Edge",
        ),
    ]
    .iter()
    .map(|(path, name)| (PathBuf::from(path), *name))
    .collect()
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn system_chromium_candidates() -> Vec<(PathBuf, &'static str)> {
    [
        ("/usr/bin/google-chrome", "Google Chrome"),
        ("/usr/bin/google-chrome-stable", "Google Chrome"),
        ("/usr/bin/chromium", "Chromium"),
        ("/usr/bin/chromium-browser", "Chromium"),
        ("/usr/bin/brave-browser", "Brave"),
        ("/usr/bin/microsoft-edge", "Microsoft Edge"),
    ]
    .iter()
    .map(|(path, name)| (PathBuf::from(path), *name))
    .collect()
}

/// Primeiro candidato que é um arquivo comum, na ordem da lista. Separado de
/// `find_system_chromium` para poder ser testado com uma lista fabricada em
/// vez de depender do que está de fato instalado na máquina do teste.
fn first_regular_file(candidates: &[(PathBuf, &'static str)]) -> Option<(PathBuf, String)> {
    candidates
        .iter()
        .find(|(path, _)| is_regular_file(path))
        .map(|(path, name)| (path.clone(), name.to_string()))
}

pub fn find_system_chromium() -> Option<(PathBuf, String)> {
    first_regular_file(&system_chromium_candidates())
}

/// Valida `Login.ManualBinaryPath`. `None`/vazio significa "nada configurado"
/// (segue para o download); um caminho preenchido é a escolha explícita do
/// usuário e **tem** que ser um arquivo comum — senão é erro na hora, sem
/// cair silenciosamente para o download ou para a detecção automática, que é
/// exatamente o que o usuário estava tentando evitar ao configurar isto.
fn resolve_manual_path(raw: Option<&str>) -> Result<Option<PathBuf>, String> {
    let Some(raw) = raw else { return Ok(None) };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let path = PathBuf::from(trimmed);
    if is_regular_file(&path) {
        Ok(Some(path))
    } else {
        Err(format!(
            "The custom browser path is not a valid file: {}",
            path.display()
        ))
    }
}

pub fn with_download_hint(err: String) -> String {
    format!(
        "{}. Check your internet connection or antivirus, then retry from Settings > General > Login browser.",
        err
    )
}

/// Escolhe o binário a usar, na ordem: caminho manual (se configurado) >
/// Chromium baixado > navegador do sistema.
///
/// O manual vence porque é a escolha explícita do usuário — ele configurou
/// isso para não depender do download nem da detecção automática. O
/// navegador do sistema só entra como último recurso, quando o download
/// falha e não há nada configurado à mão: é um binário fora do nosso
/// controle (versão e flags variam), então não é o que a maioria dos
/// usuários quer no dia a dia.
pub async fn resolve_browser_binary(
    app: &AppHandle,
    manual_path: Option<&str>,
) -> Result<(PathBuf, BrowserSource), String> {
    if let Some(path) = resolve_manual_path(manual_path)? {
        return Ok((path, BrowserSource::Manual));
    }

    match ensure_chromium(app).await {
        Ok(binary) => Ok((binary, BrowserSource::Downloaded)),
        Err(err) => {
            let _ = app.emit(
                "chromium-download-progress",
                DownloadProgress {
                    stage: "error".into(),
                    downloaded: 0,
                    total: 0,
                },
            );
            if let Some((binary, browser)) = find_system_chromium() {
                let _ = app.emit(
                    "chromium-fallback",
                    FallbackNotice {
                        browser: browser.clone(),
                        error: err,
                    },
                );
                Ok((binary, BrowserSource::System(browser)))
            } else {
                Err(with_download_hint(err))
            }
        }
    }
}

/// Apaga a instalação inteira e baixa de novo, em vez de deixar a versão
/// antiga empilhada ao lado da nova (`chromium_dir` guarda uma pasta por
/// versão; sem isso, "Reinstall" nunca soltava espaço nenhum).
pub async fn reinstall_chromium(app: &AppHandle) -> Result<PathBuf, String> {
    {
        let _guard = ENSURE_LOCK.lock().await;
        let dir = chromium_dir(app)?;
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .map_err(|e| format!("Could not remove the existing browser: {}", e))?;
        }
    }
    ensure_chromium(app).await
}

pub async fn ensure_chromium(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(binary) = cached_binary(app) {
        return Ok(binary);
    }

    let _guard = ENSURE_LOCK.lock().await;
    if let Some(binary) = cached_binary(app) {
        return Ok(binary);
    }

    let dir = chromium_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create data directory: {}", e))?;

    let _ = app.emit(
        "chromium-download-progress",
        DownloadProgress {
            stage: "resolving".into(),
            downloaded: 0,
            total: 0,
        },
    );

    let (version, url) = resolve_download().await?;
    let version_dir = dir.join(&version);
    let binary = binary_path(&version_dir);

    if !binary.exists() {
        let archive = dir.join("download.zip");
        let client = archive_client(ARCHIVE_IDLE_TIMEOUT)?;
        download_archive(&client, &url, &archive, |downloaded, total| {
            let _ = app.emit(
                "chromium-download-progress",
                DownloadProgress {
                    stage: "downloading".into(),
                    downloaded,
                    total,
                },
            );
        })
        .await?;

        let _ = app.emit(
            "chromium-download-progress",
            DownloadProgress {
                stage: "extracting".into(),
                downloaded: 0,
                total: 0,
            },
        );

        // Extrai para uma pasta `.tmp` ao lado, nunca direto em `version_dir`:
        // se o processo morrer no meio (energia, antivírus, disco cheio), o
        // que sobra é só o `.tmp` — `version_dir` (o caminho que `cached_binary`
        // lê do manifesto) nunca chega a existir pela metade. Sem isso, um
        // zip que grava `chrome.exe` antes dos outros arquivos faria
        // `binary.exists()` virar `true` com o browser ainda incompleto, e a
        // próxima abertura do app aceitaria essa instalação quebrada como boa.
        let extract_target = dir.join(format!("{}.tmp", version));
        if extract_target.exists() {
            let _ = std::fs::remove_dir_all(&extract_target);
        }
        let archive_for_extract = archive.clone();
        let extract_dir = extract_target.clone();
        tauri::async_runtime::spawn_blocking(move || {
            extract_archive(&archive_for_extract, &extract_dir)
        })
        .await
        .map_err(|e| format!("Extraction task failed: {}", e))??;

        let _ = std::fs::remove_file(&archive);

        finalize_extraction(&extract_target, &version_dir)?;
    }

    if !binary.exists() {
        return Err("Browser archive did not contain the expected executable".into());
    }

    make_executable(&binary)?;

    let manifest = serde_json::json!({
        "version": version,
        "binary": binary.to_string_lossy(),
    });
    std::fs::write(
        dir.join("version.json"),
        serde_json::to_string_pretty(&manifest).unwrap_or_default(),
    )
    .map_err(|e| format!("Could not write version manifest: {}", e))?;

    let _ = app.emit(
        "chromium-download-progress",
        DownloadProgress {
            stage: "ready".into(),
            downloaded: 0,
            total: 0,
        },
    );

    Ok(binary)
}

/// Troca `extract_target` (a pasta `.tmp` que acabou de ser extraída por
/// inteiro) pelo nome final `version_dir`, num `rename` só — é isso que torna
/// a instalação atômica: quem olhar de fora só vê `version_dir` inexistente
/// (ainda extraindo) ou completo (já trocado), nunca pela metade.
///
/// Se já existir algo em `version_dir` (reinstalação da mesma versão, ou uma
/// pasta corrompida de uma tentativa anterior a este fix), ele é apagado
/// primeiro: a nova extração **substitui**, nunca fica empilhada ao lado da
/// antiga.
fn finalize_extraction(extract_target: &Path, version_dir: &Path) -> Result<(), String> {
    if version_dir.exists() {
        std::fs::remove_dir_all(version_dir)
            .map_err(|e| format!("Could not replace the existing browser: {}", e))?;
    }
    std::fs::rename(extract_target, version_dir)
        .map_err(|e| format!("Could not finalize the browser installation: {}", e))?;
    Ok(())
}

/// Tenta resolver a versão/URL do catálogo até 3 vezes, com espera crescente
/// entre tentativas. O catálogo do chrome-for-testing devolve 5xx de vez em
/// quando; sem retry, isso virava "instale de novo" para o usuário por um
/// erro que se resolvia sozinho num segundo pedido.
async fn resolve_download() -> Result<(String, String), String> {
    resolve_download_from(VERSIONS_URL).await
}

/// `resolve_download` split by URL so tests can point it at a wiremock server
/// instead of the real chrome-for-testing catalog.
async fn resolve_download_from(url: &str) -> Result<(String, String), String> {
    let mut last_error = String::new();
    for attempt in 0..3u32 {
        match fetch_version_list(url).await {
            Ok(json) => return parse_stable_download(&json, platform_key()),
            Err(err) => {
                last_error = err;
                if attempt < 2 {
                    tokio::time::sleep(std::time::Duration::from_millis(400 * 2_u64.pow(attempt)))
                        .await;
                }
            }
        }
    }
    Err(last_error)
}

/// Busca o catálogo com timeout — sem isso um DNS ou proxy travado prendia o
/// launch inteiro em vez de cair no navegador do sistema em tempo razoável.
/// `error_for_status` é o que faz uma resposta não-2xx (ex.: 503 do CDN)
/// contar como falha e disparar o retry acima; sem ele o corpo de erro ainda
/// tentava ser lido como o JSON do catálogo e o retry nunca acontecia.
async fn fetch_version_list(url: &str) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Could not reach browser download service: {}", e))?;

    client
        .get(url)
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|e| format!("Could not reach browser download service: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Could not read browser version list: {}", e))
}

/// Pick `(version, url)` for `platform` out of the chrome-for-testing
/// "last known good versions" document. Split out from [`resolve_download`]
/// so the parsing can be exercised without hitting the network.
fn parse_stable_download(json: &Value, platform: &str) -> Result<(String, String), String> {
    let stable = json
        .get("channels")
        .and_then(|c| c.get("Stable"))
        .ok_or("Browser version list is missing the Stable channel")?;

    let version = stable
        .get("version")
        .and_then(Value::as_str)
        .ok_or("Browser version list is missing a version")?
        .to_string();

    let url = stable
        .get("downloads")
        .and_then(|d| d.get("chrome"))
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.get("platform").and_then(Value::as_str) == Some(platform))
        })
        .and_then(|entry| entry.get("url").and_then(Value::as_str))
        .ok_or("No browser build is available for this platform")?
        .to_string();

    Ok((version, url))
}

/// Sem receber nenhum byte por este tempo, o download do navegador desiste.
///
/// Teto de **inatividade**, não do download inteiro: o zip tem ~150 MB e numa
/// conexão lenta leva minutos de verdade. O que não pode é ficar parado para
/// sempre num servidor que aceitou e parou de mandar — era o `reqwest::get`,
/// sem teto nenhum.
const ARCHIVE_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Cliente do download do zip do navegador: handshake com o teto de sempre,
/// e `idle` sem receber nada.
fn archive_client(idle: std::time::Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(crate::api::http_client::CONNECT_TIMEOUT)
        .read_timeout(idle)
        .build()
        .map_err(|e| format!("Browser download failed: {}", e))
}

/// Baixa o zip para `target`, avisando `on_progress(baixado, total)` a cada
/// ~2 MB.
async fn download_archive(
    client: &reqwest::Client,
    url: &str,
    target: &Path,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<(), String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Browser download failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Browser download failed (status {})",
            response.status().as_u16()
        ));
    }

    let total = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut file =
        std::fs::File::create(target).map_err(|e| format!("Could not write download: {}", e))?;
    use std::io::Write;

    let mut last_emit = 0u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Browser download interrupted: {}", e))?
    {
        file.write_all(&chunk)
            .map_err(|e| format!("Could not write download: {}", e))?;
        downloaded += chunk.len() as u64;
        if downloaded - last_emit >= 2_000_000 {
            last_emit = downloaded;
            on_progress(downloaded, total);
        }
    }

    file.flush().map_err(|e| format!("Could not finalize download: {}", e))?;
    Ok(())
}

fn extract_archive(archive: &Path, target: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| format!("Could not open download: {}", e))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| format!("Could not read download: {}", e))?;
    std::fs::create_dir_all(target).map_err(|e| format!("Could not create browser directory: {}", e))?;

    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| format!("Could not read archive entry: {}", e))?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out_path = target.join(rel);

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let mut out =
            std::fs::File::create(&out_path).map_err(|e| format!("Could not write file: {}", e))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| format!("Could not write file: {}", e))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = entry.unix_mode() {
                let _ = std::fs::set_permissions(&out_path, std::fs::Permissions::from_mode(mode));
            }
        }
    }

    Ok(())
}

fn make_executable(binary: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(binary, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Could not set browser executable permissions: {}", e))?;
    }
    #[cfg(not(unix))]
    {
        let _ = binary;
    }
    Ok(())
}

#[cfg(test)]
mod chromium_download_tests {
    use super::*;
    use std::io::Write as _;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Catalog parsing, platform/path resolution, the archive download (against
    // a local wiremock) and archive extraction. `ensure_chromium` (which needs
    // an AppHandle) is not covered here.

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("ram4-chromium-{}-{}-{}", tag, nanos, n));
            std::fs::create_dir_all(&dir).expect("temp dir");
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).expect("create zip");
        let mut writer = zip::ZipWriter::new(file);
        let options =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, contents) in entries {
            writer.start_file(*name, options).expect("start_file");
            writer.write_all(contents).expect("write entry");
        }
        writer.finish().expect("finish zip");
    }

    fn catalog(platform: &str, url: &str, version: &str) -> Value {
        serde_json::json!({
            "channels": {
                "Stable": {
                    "version": version,
                    "downloads": {
                        "chrome": [
                            { "platform": "some-other-platform", "url": "https://wrong" },
                            { "platform": platform, "url": url },
                        ]
                    }
                }
            }
        })
    }

    // ── platform_key / binary_path ─────────────────────────────────────────

    #[test]
    fn platform_key_matches_the_chrome_for_testing_naming() {
        let key = platform_key();
        assert!(
            ["win32", "win64", "mac-arm64", "mac-x64", "linux64"].contains(&key),
            "unexpected platform key {}",
            key
        );
        if cfg!(target_os = "windows") {
            assert!(key.starts_with("win"));
        }
    }

    #[test]
    fn binary_path_lives_under_the_platform_folder_of_the_version_dir() {
        let version_dir = Path::new("C:\\data\\chromium\\131.0.0.0");
        let binary = binary_path(version_dir);

        assert!(binary.starts_with(version_dir));
        assert!(
            binary.to_string_lossy().contains(&format!("chrome-{}", platform_key())),
            "{} should contain the platform folder",
            binary.display()
        );
        if cfg!(target_os = "windows") {
            assert_eq!(binary.file_name().unwrap(), "chrome.exe");
        }
    }

    #[test]
    fn binary_path_is_deterministic() {
        let dir = Path::new("some/dir");
        assert_eq!(binary_path(dir), binary_path(dir));
    }

    // ── parse_stable_download ──────────────────────────────────────────────

    #[test]
    fn parse_stable_download_returns_the_version_and_the_matching_platform_url() {
        let json = catalog("win64", "https://cdn/chrome-win64.zip", "131.0.6778.85");
        let (version, url) = parse_stable_download(&json, "win64").unwrap();
        assert_eq!(version, "131.0.6778.85");
        assert_eq!(url, "https://cdn/chrome-win64.zip");
    }

    #[test]
    fn parse_stable_download_works_for_every_supported_platform_key() {
        for platform in ["win32", "win64", "mac-arm64", "mac-x64", "linux64"] {
            let json = catalog(platform, "https://cdn/build.zip", "1.2.3");
            let (_, url) = parse_stable_download(&json, platform).unwrap();
            assert_eq!(url, "https://cdn/build.zip", "platform {}", platform);
        }
    }

    #[test]
    fn parse_stable_download_reports_a_missing_stable_channel() {
        let err = parse_stable_download(&serde_json::json!({}), "win64").unwrap_err();
        assert_eq!(err, "Browser version list is missing the Stable channel");

        let err = parse_stable_download(
            &serde_json::json!({ "channels": { "Beta": {} } }),
            "win64",
        )
        .unwrap_err();
        assert_eq!(err, "Browser version list is missing the Stable channel");
    }

    #[test]
    fn parse_stable_download_reports_a_missing_version() {
        let json = serde_json::json!({ "channels": { "Stable": { "downloads": {} } } });
        let err = parse_stable_download(&json, "win64").unwrap_err();
        assert_eq!(err, "Browser version list is missing a version");

        // A non-string version is treated as missing.
        let json = serde_json::json!({ "channels": { "Stable": { "version": 131 } } });
        assert_eq!(
            parse_stable_download(&json, "win64").unwrap_err(),
            "Browser version list is missing a version"
        );
    }

    #[test]
    fn parse_stable_download_reports_when_this_platform_has_no_build() {
        let json = catalog("linux64", "https://cdn/linux.zip", "1.2.3");
        let err = parse_stable_download(&json, "win64").unwrap_err();
        assert_eq!(err, "No browser build is available for this platform");
    }

    #[test]
    fn parse_stable_download_reports_a_platform_entry_without_a_url() {
        let json = serde_json::json!({
            "channels": { "Stable": {
                "version": "1.2.3",
                "downloads": { "chrome": [{ "platform": "win64" }] }
            }}
        });
        assert_eq!(
            parse_stable_download(&json, "win64").unwrap_err(),
            "No browser build is available for this platform"
        );
    }

    #[test]
    fn parse_stable_download_reports_a_malformed_downloads_section() {
        for downloads in [
            serde_json::json!({}),
            serde_json::json!({ "chrome": "not-an-array" }),
            serde_json::json!({ "chromedriver": [] }),
        ] {
            let json = serde_json::json!({
                "channels": { "Stable": { "version": "1.2.3", "downloads": downloads } }
            });
            assert_eq!(
                parse_stable_download(&json, "win64").unwrap_err(),
                "No browser build is available for this platform"
            );
        }
    }

    // ── cached_binary manifest shape ───────────────────────────────────────
    //
    // `cached_binary` itself needs an AppHandle, but the manifest it writes and
    // reads is plain JSON; this pins its shape so a rename would be caught.

    #[test]
    fn the_version_manifest_holds_a_version_and_a_binary_path() {
        let manifest = serde_json::json!({
            "version": "131.0.6778.85",
            "binary": "C:\\data\\chromium\\131.0.6778.85\\chrome-win64\\chrome.exe",
        });
        let raw = serde_json::to_string_pretty(&manifest).unwrap();
        let parsed: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            parsed.get("binary").and_then(Value::as_str),
            Some("C:\\data\\chromium\\131.0.6778.85\\chrome-win64\\chrome.exe")
        );
        assert!(parsed.get("version").and_then(Value::as_str).is_some());
    }

    // ── extract_archive ────────────────────────────────────────────────────

    #[test]
    fn extract_archive_writes_every_entry_under_the_target() {
        let temp = TempDir::new("extract");
        let archive = temp.path().join("download.zip");
        write_zip(
            &archive,
            &[
                ("chrome-win64/chrome.exe", b"MZ".as_slice()),
                ("chrome-win64/locales/en-US.pak", b"pak".as_slice()),
            ],
        );

        let target = temp.path().join("out");
        extract_archive(&archive, &target).expect("extract");

        assert_eq!(
            std::fs::read(target.join("chrome-win64").join("chrome.exe")).unwrap(),
            b"MZ"
        );
        assert_eq!(
            std::fs::read(
                target
                    .join("chrome-win64")
                    .join("locales")
                    .join("en-US.pak")
            )
            .unwrap(),
            b"pak"
        );
    }

    #[test]
    fn extract_archive_does_not_write_outside_the_target() {
        let temp = TempDir::new("traversal");
        let archive = temp.path().join("download.zip");
        write_zip(
            &archive,
            &[
                ("..\\escaped.exe", b"pwned".as_slice()),
                ("../escaped2.exe", b"pwned".as_slice()),
                ("chrome.exe", b"MZ".as_slice()),
            ],
        );

        let target = temp.path().join("out");
        extract_archive(&archive, &target).expect("extract");

        assert!(target.join("chrome.exe").exists());
        for escaped in ["escaped.exe", "escaped2.exe"] {
            assert!(
                !temp.path().join(escaped).exists(),
                "{} escaped the target",
                escaped
            );
            assert!(!target.join(escaped).exists(), "{} was written", escaped);
        }
    }

    #[test]
    fn extract_archive_creates_the_target_for_an_empty_archive() {
        let temp = TempDir::new("empty");
        let archive = temp.path().join("download.zip");
        write_zip(&archive, &[]);
        let target = temp.path().join("out");
        extract_archive(&archive, &target).expect("extract");
        assert!(target.is_dir());
    }

    #[test]
    fn extract_archive_reports_a_missing_or_corrupt_download() {
        let temp = TempDir::new("bad");
        let missing = temp.path().join("missing.zip");
        let err = extract_archive(&missing, &temp.path().join("out")).unwrap_err();
        assert!(err.starts_with("Could not open download"), "got {}", err);

        let corrupt = temp.path().join("corrupt.zip");
        std::fs::write(&corrupt, b"not a zip file").unwrap();
        let err = extract_archive(&corrupt, &temp.path().join("out2")).unwrap_err();
        assert!(err.starts_with("Could not read download"), "got {}", err);
    }

    #[test]
    fn make_executable_is_a_no_op_on_a_missing_path() {
        let temp = TempDir::new("chmod");
        make_executable(&temp.path().join("nope")).expect("missing path is not an error");
    }

    // ── is_regular_file ─────────────────────────────────────────────────────
    //
    // O Chromium é usado para login e criação de conta, então um candidato de
    // navegador — vindo de detecção automática ou de `Login.ManualBinaryPath`
    // — tem que ser exatamente um arquivo comum: nunca uma pasta, nunca um
    // link simbólico apontando para outro lugar.

    #[test]
    fn is_regular_file_accepts_a_plain_file() {
        let temp = TempDir::new("regular");
        let file = temp.path().join("chrome.exe");
        std::fs::write(&file, b"MZ").unwrap();
        assert!(is_regular_file(&file));
    }

    #[test]
    fn is_regular_file_rejects_a_directory() {
        let temp = TempDir::new("regular-dir");
        assert!(!is_regular_file(temp.path()));
    }

    #[test]
    fn is_regular_file_rejects_a_missing_path() {
        let temp = TempDir::new("regular-missing");
        assert!(!is_regular_file(&temp.path().join("nope.exe")));
    }

    #[test]
    fn is_regular_file_rejects_a_symlink_even_when_it_points_at_a_real_file() {
        // `Path::is_file`/`exists` seguem o link e aceitariam isto — é
        // exatamente o que este helper existe para recusar. Criar um link
        // simbólico no Windows fora de modo Administrador/Desenvolvedor falha
        // com "acesso negado"; nesse caso o teste não teria como provar nada
        // e é pulado em vez de dar falso positivo.
        let temp = TempDir::new("regular-symlink");
        let real = temp.path().join("real.exe");
        std::fs::write(&real, b"MZ").unwrap();
        let link = temp.path().join("link.exe");

        #[cfg(windows)]
        let created = std::os::windows::fs::symlink_file(&real, &link).is_ok();
        #[cfg(unix)]
        let created = std::os::unix::fs::symlink(&real, &link).is_ok();
        #[cfg(not(any(windows, unix)))]
        let created = false;

        if !created {
            eprintln!("skipping: this machine would not let the test create a symlink");
            return;
        }

        assert!(is_regular_file(&real));
        assert!(
            !is_regular_file(&link),
            "a symlink must never be accepted as a regular-file browser candidate"
        );
    }

    // ── first_regular_file / find_system_chromium ──────────────────────────
    //
    // `find_system_chromium` depende do que está de fato instalado na máquina
    // (variáveis de ambiente, `Program Files`), então a ordem de preferência e
    // as recusas são testadas contra uma lista fabricada em vez da real.

    #[test]
    fn first_regular_file_picks_the_first_candidate_that_is_a_regular_file() {
        let temp = TempDir::new("candidates-order");
        let first = temp.path().join("first.exe");
        let second = temp.path().join("second.exe");
        std::fs::write(&first, b"MZ").unwrap();
        std::fs::write(&second, b"MZ").unwrap();

        // As duas existem; a ordem da lista é quem decide, não a ordem de
        // criação no disco.
        let candidates = vec![(first.clone(), "First"), (second.clone(), "Second")];
        assert_eq!(
            first_regular_file(&candidates),
            Some((first.clone(), "First".to_string()))
        );

        let reordered = vec![(second.clone(), "Second"), (first, "First")];
        assert_eq!(
            first_regular_file(&reordered),
            Some((second, "Second".to_string()))
        );
    }

    #[test]
    fn first_regular_file_skips_a_missing_candidate_and_falls_through_to_the_next() {
        let temp = TempDir::new("candidates-missing");
        let missing = temp.path().join("missing.exe");
        let present = temp.path().join("present.exe");
        std::fs::write(&present, b"MZ").unwrap();

        let candidates = vec![(missing, "Missing"), (present.clone(), "Present")];
        assert_eq!(
            first_regular_file(&candidates),
            Some((present, "Present".to_string()))
        );
    }

    #[test]
    fn first_regular_file_skips_a_directory_candidate() {
        let temp = TempDir::new("candidates-dir");
        let as_dir = temp.path().join("chrome.exe");
        std::fs::create_dir_all(&as_dir).unwrap();
        let real = temp.path().join("real.exe");
        std::fs::write(&real, b"MZ").unwrap();

        // Um instalador poderia, em teoria, deixar uma pasta com o mesmo nome
        // do executável; isso não pode contar como candidato.
        let candidates = vec![(as_dir, "AsDir"), (real.clone(), "Real")];
        assert_eq!(first_regular_file(&candidates), Some((real, "Real".to_string())));
    }

    #[test]
    fn first_regular_file_returns_none_when_no_candidate_exists() {
        let temp = TempDir::new("candidates-none");
        let candidates = vec![
            (temp.path().join("a.exe"), "A"),
            (temp.path().join("b.exe"), "B"),
        ];
        assert_eq!(first_regular_file(&candidates), None);
    }

    #[test]
    fn find_system_chromium_only_ever_returns_a_regular_file() {
        // Não há como controlar o que está instalado na máquina do teste, mas
        // dá para garantir que, se algo for encontrado, respeita a mesma regra
        // que `is_regular_file` teria aplicado.
        if let Some((path, name)) = find_system_chromium() {
            assert!(is_regular_file(&path), "{} ({})", path.display(), name);
        }
    }

    // ── resolve_manual_path ─────────────────────────────────────────────────

    #[test]
    fn resolve_manual_path_treats_none_and_blank_as_not_configured() {
        assert_eq!(resolve_manual_path(None).unwrap(), None);
        assert_eq!(resolve_manual_path(Some("")).unwrap(), None);
        assert_eq!(resolve_manual_path(Some("   ")).unwrap(), None);
    }

    #[test]
    fn resolve_manual_path_accepts_a_regular_file() {
        let temp = TempDir::new("manual-ok");
        let file = temp.path().join("chrome.exe");
        std::fs::write(&file, b"MZ").unwrap();

        let raw = file.to_string_lossy().to_string();
        assert_eq!(resolve_manual_path(Some(&raw)).unwrap(), Some(file));
    }

    #[test]
    fn resolve_manual_path_trims_surrounding_whitespace() {
        let temp = TempDir::new("manual-trim");
        let file = temp.path().join("chrome.exe");
        std::fs::write(&file, b"MZ").unwrap();

        let raw = format!("  {}  ", file.to_string_lossy());
        assert_eq!(resolve_manual_path(Some(&raw)).unwrap(), Some(file));
    }

    #[test]
    fn resolve_manual_path_rejects_a_directory() {
        let temp = TempDir::new("manual-dir");
        let raw = temp.path().to_string_lossy().to_string();
        let err = resolve_manual_path(Some(&raw)).unwrap_err();
        assert!(err.contains("not a valid file"), "got {}", err);
    }

    #[test]
    fn resolve_manual_path_rejects_a_missing_file() {
        let temp = TempDir::new("manual-missing");
        let raw = temp.path().join("nope.exe").to_string_lossy().to_string();
        let err = resolve_manual_path(Some(&raw)).unwrap_err();
        assert!(err.contains("not a valid file"), "got {}", err);
    }

    // ── finalize_extraction (instalação atômica) ────────────────────────────

    #[test]
    fn finalize_extraction_moves_a_fresh_extraction_into_place() {
        let temp = TempDir::new("finalize-fresh");
        let extract_target = temp.path().join("131.0.0.0.tmp");
        std::fs::create_dir_all(&extract_target).unwrap();
        std::fs::write(extract_target.join("chrome.exe"), b"MZ").unwrap();

        let version_dir = temp.path().join("131.0.0.0");
        finalize_extraction(&extract_target, &version_dir).expect("finalize");

        assert!(!extract_target.exists(), "the .tmp folder must not survive finalize");
        assert_eq!(std::fs::read(version_dir.join("chrome.exe")).unwrap(), b"MZ");
    }

    #[test]
    fn finalize_extraction_replaces_an_existing_version_dir_instead_of_stacking() {
        let temp = TempDir::new("finalize-replace");
        let version_dir = temp.path().join("131.0.0.0");
        std::fs::create_dir_all(&version_dir).unwrap();
        std::fs::write(version_dir.join("stale.txt"), b"old install").unwrap();

        let extract_target = temp.path().join("131.0.0.0.tmp");
        std::fs::create_dir_all(&extract_target).unwrap();
        std::fs::write(extract_target.join("chrome.exe"), b"MZ").unwrap();

        finalize_extraction(&extract_target, &version_dir).expect("finalize");

        // Reinstalar substitui: nada da instalação antiga sobrevive ao lado
        // da nova dentro do mesmo `version_dir`.
        assert!(
            !version_dir.join("stale.txt").exists(),
            "the old install must not be stacked alongside the new one"
        );
        assert!(version_dir.join("chrome.exe").exists());
    }

    #[test]
    fn an_interrupted_extraction_leaves_no_binary_at_the_final_path() {
        // Reproduz o processo morrendo logo depois de `extract_archive`
        // terminar de escrever no `.tmp`, mas antes de `finalize_extraction`
        // trocar os nomes. Usa as mesmas funções de produção (`extract_archive`,
        // `binary_path`) em vez de escrever os arquivos à mão, para provar algo
        // sobre o código real e não só sobre a montagem do teste.
        //
        // Sem a extração em duas etapas, um zip que grava `chrome.exe` antes
        // dos outros arquivos faria `binary_path(&version_dir).exists()` virar
        // `true` assim que aquele arquivo saísse, com o resto do browser ainda
        // faltando — e a próxima abertura do app aceitaria essa instalação pela
        // metade como se estivesse pronta.
        let temp = TempDir::new("finalize-interrupted");
        let archive = temp.path().join("download.zip");
        write_zip(
            &archive,
            &[(
                &format!("chrome-{}/chrome.exe", platform_key()),
                b"MZ".as_slice(),
            )],
        );

        let version = "131.0.0.0";
        let extract_target = temp.path().join(format!("{}.tmp", version));
        extract_archive(&archive, &extract_target).expect("extract");

        let version_dir = temp.path().join(version);

        // A extração em si terminou com sucesso (o "processo" só morreu depois
        // dela, antes do rename)...
        assert!(binary_path(&extract_target).exists());
        // ...mas nada sob o nome final existe ainda: nenhum código de produção
        // olha para `version_dir` até `finalize_extraction` rodar.
        assert!(
            !binary_path(&version_dir).exists(),
            "the final path must never look installed before finalize_extraction runs"
        );
        assert!(!version_dir.exists());
    }

    #[test]
    fn a_stale_tmp_folder_from_a_previous_interrupted_attempt_is_cleared_before_reextracting() {
        // `ensure_chromium` apaga um `.tmp` remanescente antes de extrair de
        // novo; aqui fixamos que `finalize_extraction` não depende de o `.tmp`
        // estar "limpo" — ele decide pelo conteúdo que está lá na hora, então
        // quem limpa o `.tmp` velho é o chamador antes de popular o novo.
        let temp = TempDir::new("finalize-stale-tmp");
        let extract_target = temp.path().join("131.0.0.0.tmp");
        std::fs::create_dir_all(&extract_target).unwrap();
        std::fs::write(extract_target.join("leftover.txt"), b"from a previous attempt").unwrap();
        // Simula o chamador limpando antes de popular a nova extração.
        std::fs::remove_dir_all(&extract_target).unwrap();
        std::fs::create_dir_all(&extract_target).unwrap();
        std::fs::write(extract_target.join("chrome.exe"), b"MZ").unwrap();

        let version_dir = temp.path().join("131.0.0.0");
        finalize_extraction(&extract_target, &version_dir).expect("finalize");

        assert!(!version_dir.join("leftover.txt").exists());
        assert!(version_dir.join("chrome.exe").exists());
    }

    // ── resolve_download_from / fetch_version_list (retry por status HTTP) ──

    #[tokio::test]
    async fn fetch_version_list_succeeds_on_a_200_response() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let body = catalog("win64", "https://cdn/win64.zip", "131.0.6778.85");
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;

        let json = fetch_version_list(&server.uri()).await.expect("200 must succeed");
        assert_eq!(json, body);
    }

    #[tokio::test]
    async fn fetch_version_list_errors_on_a_non_2xx_response() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let err = fetch_version_list(&server.uri()).await.unwrap_err();
        assert!(
            err.starts_with("Could not reach browser download service"),
            "got {}",
            err
        );
    }

    #[tokio::test]
    async fn resolve_download_from_retries_a_non_2xx_response_up_to_three_times_then_gives_up() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let result = resolve_download_from(&server.uri()).await;
        assert!(result.is_err());

        let requests = server.received_requests().await.expect("requests");
        assert_eq!(
            requests.len(),
            3,
            "a non-2xx response must be retried until 3 attempts total, not accepted or retried forever"
        );
    }

    #[tokio::test]
    async fn resolve_download_from_does_not_retry_after_a_successful_response() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let body = catalog("win64", "https://cdn/win64.zip", "131.0.6778.85");
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;

        let (version, url) = resolve_download_from(&server.uri()).await.expect("200 must succeed");
        assert_eq!(version, "131.0.6778.85");
        assert_eq!(url, "https://cdn/win64.zip");

        let requests = server.received_requests().await.expect("requests");
        assert_eq!(requests.len(), 1, "a successful response must not be retried");
    }

    // O download do zip do navegador era `reqwest::get`, sem teto nenhum: um
    // servidor que aceita e para de mandar prendia a tela de download para
    // sempre. Ele desiste depois de um tempo sem receber nada — sem teto para o
    // download inteiro, que numa conexão lenta leva minutos de verdade.

    #[tokio::test]
    async fn an_archive_download_that_stops_sending_gives_up() {
        use std::time::{Duration, Instant};
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/chrome-win64.zip"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(vec![7u8; 64])
                    .set_delay(Duration::from_secs(5)),
            )
            .mount(&server)
            .await;
        let dir = TempDir::new("stalled");
        let target = dir.path().join("download.zip");

        let client = archive_client(Duration::from_millis(300)).expect("client");
        let started = Instant::now();
        let result = download_archive(
            &client,
            &format!("{}/chrome-win64.zip", server.uri()),
            &target,
            |_, _| {},
        )
        .await;

        assert!(
            result.is_err(),
            "um download parado terminou como se tivesse dado certo"
        );
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "o download parado só desistiu depois de {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn an_archive_download_that_keeps_sending_is_written_whole() {
        use std::time::Duration;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        // 3 MB: passa do limiar de 2 MB do aviso de progresso.
        let body: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/chrome-win64.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
            .mount(&server)
            .await;
        let dir = TempDir::new("whole");
        let target = dir.path().join("download.zip");

        let client = archive_client(Duration::from_secs(5)).expect("client");
        let mut progress = Vec::new();
        download_archive(
            &client,
            &format!("{}/chrome-win64.zip", server.uri()),
            &target,
            |downloaded, total| progress.push((downloaded, total)),
        )
        .await
        .expect("o download completo falhou");

        assert_eq!(std::fs::read(&target).expect("arquivo baixado"), body);
        assert!(
            progress
                .iter()
                .any(|&(d, t)| d >= 2_000_000 && t == 3_000_000),
            "o progresso não foi avisado: {progress:?}"
        );
    }
}
