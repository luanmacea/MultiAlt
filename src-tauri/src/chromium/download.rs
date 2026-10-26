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

/// Arquivos sem os quais o Chromium abre uma janela em branco e não responde,
/// com um tamanho mínimo bem abaixo do real (`chrome.dll` ~300 MB, `chrome.exe`
/// ~4,5 MB, medidos na 154): pegam arquivo ausente ou cortado, não versão.
#[cfg(target_os = "windows")]
const REQUIRED_FILES: &[(&str, u64)] = &[
    ("chrome.exe", 1024 * 1024),
    ("chrome.dll", 20 * 1024 * 1024),
    ("resources.pak", 1024 * 1024),
    ("icudtl.dat", 1024 * 1024),
    ("v8_context_snapshot.bin", 100 * 1024),
];

/// A instalação baixada está completa? `None` = sim (ou não sabemos conferir
/// nesta plataforma). Só confere o que existe no disco: antivírus que apaga um
/// arquivo depois da extração, ou uma extração cortada, deixava o `chrome.exe`
/// no lugar e a instalação passava por boa — o navegador abria em branco.
fn installation_problem(binary: &Path) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let Some(dir) = binary.parent() else {
            return Some("chrome.exe".to_string());
        };
        for (name, min_bytes) in REQUIRED_FILES {
            match std::fs::metadata(dir.join(name)) {
                Ok(meta) if meta.is_file() && meta.len() >= *min_bytes => {}
                Ok(_) => return Some(format!("{name} is incomplete")),
                Err(_) => return Some(format!("{name} is missing")),
            }
        }
        None
    }
    #[cfg(not(target_os = "windows"))]
    {
        if binary.exists() {
            None
        } else {
            Some("the browser executable is missing".to_string())
        }
    }
}

pub(crate) fn cached_binary(app: &AppHandle) -> Option<PathBuf> {
    let dir = chromium_dir(app).ok()?;
    let manifest = dir.join("version.json");
    let raw = std::fs::read_to_string(manifest).ok()?;
    let json: Value = serde_json::from_str(&raw).ok()?;
    let binary = json.get("binary").and_then(Value::as_str)?;
    let path = PathBuf::from(binary);
    // Instalação incompleta = como se não houvesse: `ensure_chromium` baixa de
    // novo em vez de entregar um navegador que abre em branco.
    if installation_problem(&path).is_none() {
        Some(path)
    } else {
        None
    }
}

/// Há uma cópia baixada registrada, mas com arquivo faltando ou cortado
/// (`installation_problem`): é o "Reinstall" que resolve.
pub(crate) fn has_damaged_download(app: &AppHandle) -> bool {
    let Ok(dir) = chromium_dir(app) else {
        return false;
    };
    dir.join("version.json").exists() && cached_binary(app).is_none()
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
pub(crate) fn resolve_manual_path(raw: Option<&str>) -> Result<Option<PathBuf>, String> {
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

    if installation_problem(&binary).is_some() {
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
