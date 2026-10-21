// O Python portátil (docs/features/python-automation.md, "Python portátil").
//
// Só baixa quando o usuário aperta "Install Python": o *embeddable zip* oficial
// do CPython, o wheel do pip e os wheels do numpy e do opencv-python-headless,
// cada um com versão e SHA-256 fixos aqui. Tudo é montado em
// `python-runtime/.tmp` e só vira `python-runtime/current` no fim (renomear),
// então uma instalação que falha no meio não deixa runtime pela metade — e um
// reinstall que falha deixa o anterior no lugar. A pasta de trabalho (scripts
// do usuário) mora em outro lugar e nunca é tocada daqui.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use super::types::{PythonInstallProgress, PythonRuntimeStatus};

/// Uma peça baixada: o nome do arquivo, de onde vem e o SHA-256 que tem de bater.
#[derive(Debug, Clone, PartialEq)]
pub struct Artifact {
    pub file: String,
    pub url: String,
    pub sha256: String,
}

impl Artifact {
    fn new(file: &str, url: &str, sha256: &str) -> Self {
        Self {
            file: file.into(),
            url: url.into(),
            sha256: sha256.into(),
        }
    }
}

/// Um pacote instalado pelo pip: nome, versão e o wheel (com hash) dele.
#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub wheel: Artifact,
}

/// O que compõe o runtime. `official()` é o da build; os testes trocam as URLs
/// por um servidor local.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeSpec {
    pub python_version: String,
    pub python: Artifact,
    pub pip: Artifact,
    pub packages: Vec<Package>,
}

/// CPython 3.12.10: a última 3.12 com instaladores binários (as seguintes são
/// só de segurança, sem *embeddable zip*). O SHA-256 vem do bundle sigstore
/// assinado pelo python.org (`.zip.sigstore`, `messageDigest`), conferido em
/// 21/10/2026; o tamanho é 11 133 606 bytes.
pub const PYTHON_VERSION: &str = "3.12.10";
pub const PYTHON_ZIP_URL: &str =
    "https://www.python.org/ftp/python/3.12.10/python-3.12.10-embed-amd64.zip";
pub const PYTHON_ZIP_SHA256: &str =
    "4acbed6dd1c744b0376e3b1cf57ce906f9dc9e95e68824584c8099a63025a3c3";

/// pip 25.1.1 em wheel (PyPI). No lugar do `get-pip.py` do plano: o wheel
/// (hash publicado pelo PyPI) é aberto no `Lib\site-packages` do runtime, e
/// daí em diante é `python -m pip`.
pub const PIP_VERSION: &str = "25.1.1";
pub const PIP_WHEEL_URL: &str = "https://files.pythonhosted.org/packages/29/a2/d40fb2460e883eca5199c62cfc2463fd261f760556ae6290f88488c362c0/pip-25.1.1-py3-none-any.whl";
pub const PIP_WHEEL_SHA256: &str =
    "2913a38a2abf4ea6b64ab507bd9e967f3b53dc1ede74b01b0931e1ce548751af";

/// numpy 2.2.6: o opencv-python-headless 4.12.0.88 pede `numpy>=2,<2.3`.
pub const NUMPY_VERSION: &str = "2.2.6";
pub const NUMPY_WHEEL_URL: &str = "https://files.pythonhosted.org/packages/36/fa/8c9210162ca1b88529ab76b41ba02d433fd54fecaf6feb70ef9f124683f1/numpy-2.2.6-cp312-cp312-win_amd64.whl";
pub const NUMPY_WHEEL_SHA256: &str =
    "c1f9540be57940698ed329904db803cf7a402f3fc200bfe599334c9bd84a40b2";

pub const OPENCV_VERSION: &str = "4.12.0.88";
pub const OPENCV_WHEEL_URL: &str = "https://files.pythonhosted.org/packages/f2/35/0858e9e71b36948eafbc5e835874b63e515179dc3b742cbe3d76bc683439/opencv_python_headless-4.12.0.88-cp37-abi3-win_amd64.whl";
pub const OPENCV_WHEEL_SHA256: &str =
    "86b413bdd6c6bf497832e346cd5371995de148e579b9774f8eba686dee3f5528";

impl RuntimeSpec {
    pub fn official() -> Self {
        Self {
            python_version: PYTHON_VERSION.into(),
            python: Artifact::new("python-3.12.10-embed-amd64.zip", PYTHON_ZIP_URL, PYTHON_ZIP_SHA256),
            pip: Artifact::new("pip-25.1.1-py3-none-any.whl", PIP_WHEEL_URL, PIP_WHEEL_SHA256),
            packages: vec![
                Package {
                    name: "numpy".into(),
                    version: NUMPY_VERSION.into(),
                    wheel: Artifact::new(
                        "numpy-2.2.6-cp312-cp312-win_amd64.whl",
                        NUMPY_WHEEL_URL,
                        NUMPY_WHEEL_SHA256,
                    ),
                },
                Package {
                    name: "opencv-python-headless".into(),
                    version: OPENCV_VERSION.into(),
                    wheel: Artifact::new(
                        "opencv_python_headless-4.12.0.88-cp37-abi3-win_amd64.whl",
                        OPENCV_WHEEL_URL,
                        OPENCV_WHEEL_SHA256,
                    ),
                },
            ],
        }
    }
}

/// O que fica gravado ao lado do runtime pronto.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct VersionFile {
    python: String,
    packages: Vec<String>,
}

const CURRENT: &str = "current";
const TMP: &str = ".tmp";
const VERSION_FILE: &str = "version.json";

static INSTALLING: AtomicBool = AtomicBool::new(false);

/// `<app_local_data_dir>/python-runtime` (fora da pasta de dados: não entra em
/// backup).
pub fn runtime_dir(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    app.path()
        .app_local_data_dir()
        .unwrap_or_else(|_| crate::data::settings::get_runtime_data_dir())
        .join("python-runtime")
}

pub fn python_exe(app: &tauri::AppHandle) -> Option<PathBuf> {
    python_exe_in(&runtime_dir(app))
}

pub fn status(app: &tauri::AppHandle) -> PythonRuntimeStatus {
    status_in(&runtime_dir(app))
}

pub fn python_exe_in(base: &Path) -> Option<PathBuf> {
    let exe = base.join(CURRENT).join("python.exe");
    exe.is_file().then_some(exe)
}

pub fn status_in(base: &Path) -> PythonRuntimeStatus {
    let installing = INSTALLING.load(Ordering::SeqCst);
    let version: Option<VersionFile> = std::fs::read_to_string(base.join(CURRENT).join(VERSION_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    match (python_exe_in(base), version) {
        (Some(exe), Some(version)) => PythonRuntimeStatus {
            installed: true,
            version: Some(version.python),
            python_exe: Some(exe.to_string_lossy().to_string()),
            packages: version.packages,
            installing,
        },
        _ => PythonRuntimeStatus {
            installing,
            ..Default::default()
        },
    }
}

/// Marca "instalando" e devolve um guarda que desmarca ao sair. `None` se já há
/// uma instalação em andamento — a segunda é recusada.
pub fn begin_install() -> Option<InstallGuard> {
    if INSTALLING.swap(true, Ordering::SeqCst) {
        None
    } else {
        Some(InstallGuard)
    }
}

pub struct InstallGuard;

impl Drop for InstallGuard {
    fn drop(&mut self) {
        INSTALLING.store(false, Ordering::SeqCst);
    }
}

/// O `pythonXY._pth` do embeddable desliga o `site` e fixa o `sys.path`. Aqui
/// ele liga o `site` (o pip e os pacotes precisam), põe `Lib\site-packages` e
/// a pasta de trabalho no caminho (é dela que vêm `multialt` e `lib`).
pub fn patch_pth(original: &str, workspace: &Path) -> String {
    let workspace = workspace.to_string_lossy().to_string();
    let mut lines: Vec<String> = Vec::new();
    let mut has_site = false;
    for line in original.lines() {
        let trimmed = line.trim();
        if trimmed == "#import site" || trimmed == "# import site" || trimmed == "import site" {
            has_site = true;
            continue;
        }
        if trimmed.eq_ignore_ascii_case("Lib\\site-packages") || trimmed == workspace {
            continue;
        }
        lines.push(line.trim_end().to_string());
    }
    let _ = has_site;
    lines.push("Lib\\site-packages".into());
    lines.push(workspace);
    lines.push("import site".into());
    let mut out = lines.join("\r\n");
    out.push_str("\r\n");
    out
}

/// Garante a pasta de trabalho no `._pth` do runtime instalado (a pasta de
/// dados pode ter mudado depois da instalação). Chamado antes de cada execução.
pub fn ensure_workspace_in_pth(base: &Path, workspace: &Path) -> Result<(), String> {
    let dir = base.join(CURRENT);
    if let Some(pth) = find_pth(&dir) {
        let text = std::fs::read_to_string(&pth).map_err(|e| e.to_string())?;
        let patched = patch_pth(&text, workspace);
        if patched != text {
            std::fs::write(&pth, patched).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn find_pth(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
        let name = p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        name.starts_with("python") && name.ends_with("._pth")
    })
}

/// Quem roda os passos do pip. O real abre `python.exe` sem janela de console;
/// o dos testes só anota.
pub trait StepRunner: Send + Sync {
    /// Roda e devolve o código de saída; cada linha de saída vai para `on_line`.
    fn run(&self, exe: &Path, args: &[String], cwd: &Path, on_line: &mut dyn FnMut(String)) -> Result<i32, String>;
}

/// Flags de todo `python.exe` que o app abre: `-s` tira o *site-packages do
/// usuário* (`%APPDATA%\Python\Python312`) do caminho, `-E` ignora as
/// variáveis `PYTHON*` do PC, `-B` não grava `.pyc`. Sem o `-s`, o pip do
/// runtime enxergava os pacotes do Python do próprio usuário e **desinstalava**
/// o numpy dele para pôr o fixado (visto na instalação real de 21/10/2026).
pub const ISOLATION_FLAGS: &[&str] = &["-s", "-E", "-B", "-X", "utf8"];

pub struct RealStepRunner;

impl StepRunner for RealStepRunner {
    fn run(&self, exe: &Path, args: &[String], cwd: &Path, on_line: &mut dyn FnMut(String)) -> Result<i32, String> {
        use std::io::BufRead;
        use std::process::{Command, Stdio};
        let mut command = Command::new(exe);
        command
            .args(ISOLATION_FLAGS)
            .args(args)
            .env("PYTHONNOUSERSITE", "1")
            .current_dir(cwd)
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("PIP_DISABLE_PIP_VERSION_CHECK", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // Sem isto o python.exe acende um console preto.
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().map_err(|e| format!("Could not start Python: {e}"))?;
        let stderr = child.stderr.take();
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let err_thread = stderr.map(|stderr| {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for line in std::io::BufReader::new(stderr).lines().map_while(Result::ok) {
                    let _ = tx.send(line);
                }
            })
        });
        if let Some(stdout) = child.stdout.take() {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
                    let _ = tx.send(line);
                }
            });
        }
        drop(tx);
        for line in rx {
            on_line(line);
        }
        if let Some(t) = err_thread {
            let _ = t.join();
        }
        let status = child.wait().map_err(|e| e.to_string())?;
        Ok(status.code().unwrap_or(-1))
    }
}

fn progress(stage: &str, message: impl Into<String>, downloaded: u64, total: u64) -> PythonInstallProgress {
    PythonInstallProgress {
        stage: stage.into(),
        message: message.into(),
        downloaded,
        total,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

async fn download_verified(
    client: &reqwest::Client,
    artifact: &Artifact,
    dir: &Path,
    stage: &str,
    on_progress: &mut (dyn FnMut(PythonInstallProgress) + Send),
) -> Result<PathBuf, String> {
    let mut response = client
        .get(&artifact.url)
        .send()
        .await
        .map_err(|e| format!("Download of {} failed: {}", artifact.file, e))?;
    if !response.status().is_success() {
        return Err(format!(
            "Download of {} failed (status {})",
            artifact.file,
            response.status().as_u16()
        ));
    }
    let total = response.content_length().unwrap_or(0);
    let mut bytes: Vec<u8> = Vec::with_capacity(total as usize);
    let mut last = 0u64;
    on_progress(progress(stage, format!("Downloading {}", artifact.file), 0, total));
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Download of {} was interrupted: {}", artifact.file, e))?
    {
        bytes.extend_from_slice(&chunk);
        let done = bytes.len() as u64;
        if done - last >= 1_000_000 {
            last = done;
            on_progress(progress(stage, format!("Downloading {}", artifact.file), done, total));
        }
    }
    let computed = sha256_hex(&bytes);
    if !computed.eq_ignore_ascii_case(&artifact.sha256) {
        return Err(format!(
            "{} failed the integrity check (SHA-256 mismatch). Nothing was installed.",
            artifact.file
        ));
    }
    let path = dir.join(&artifact.file);
    std::fs::write(&path, &bytes).map_err(|e| format!("Could not save {} (disk full?): {}", artifact.file, e))?;
    on_progress(progress(stage, format!("Verified {}", artifact.file), bytes.len() as u64, total.max(bytes.len() as u64)));
    Ok(path)
}

/// Extrai o zip em `target`, recusando entrada que sairia da pasta (zip-slip),
/// como o `extract_archive` do Chromium.
fn extract_zip(archive: &Path, target: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| format!("Could not open the download: {e}"))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("Could not read the download: {e}"))?;
    std::fs::create_dir_all(target).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
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
        let mut out = std::fs::File::create(&out_path).map_err(|e| format!("Could not write {}: {e}", out_path.display()))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn remove_dir_if_exists(dir: &Path) -> Result<(), String> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| format!("Could not remove {}: {e}", dir.display()))?;
    }
    Ok(())
}

/// Instala (ou reinstala) o runtime em `base`. Só no fim o `.tmp` vira
/// `current`; qualquer erro apaga o `.tmp` e deixa o `current` de antes.
pub async fn install_into(
    base: &Path,
    workspace: &Path,
    spec: &RuntimeSpec,
    runner: &dyn StepRunner,
    on_progress: &mut (dyn FnMut(PythonInstallProgress) + Send),
) -> Result<(), String> {
    let tmp = base.join(TMP);
    remove_dir_if_exists(&tmp)?;
    let result = install_steps(base, &tmp, workspace, spec, runner, on_progress).await;
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    result
}

async fn install_steps(
    base: &Path,
    tmp: &Path,
    workspace: &Path,
    spec: &RuntimeSpec,
    runner: &dyn StepRunner,
    on_progress: &mut (dyn FnMut(PythonInstallProgress) + Send),
) -> Result<(), String> {
    let downloads = tmp.join("downloads");
    let home = tmp.join("python");
    std::fs::create_dir_all(&downloads).map_err(|e| format!("Could not create {}: {e}", downloads.display()))?;

    let client = crate::api::http_client::download_builder()
        .build()
        .map_err(|e| format!("Download failed: {e}"))?;

    // 1. Python.
    let zip = download_verified(&client, &spec.python, &downloads, "python", on_progress).await?;
    extract_zip(&zip, &home)?;
    let pth = find_pth(&home).ok_or("The Python package has no ._pth file")?;
    let text = std::fs::read_to_string(&pth).map_err(|e| e.to_string())?;
    std::fs::write(&pth, patch_pth(&text, workspace)).map_err(|e| e.to_string())?;
    let exe = home.join("python.exe");

    // 2. pip: o wheel (conferido) é aberto direto no `Lib\site-packages` — um
    //    wheel é um zip com o pacote e o `.dist-info`. Rodar o pip de dentro
    //    do wheel para ele se instalar é recusado no Windows ("To modify pip,
    //    please run python -m pip", visto na instalação real de 21/10/2026).
    let pip = download_verified(&client, &spec.pip, &downloads, "pip", on_progress).await?;
    extract_zip(&pip, &home.join("Lib").join("site-packages"))?;
    let args = vec!["-m".to_string(), "pip".into(), "--version".into()];
    run_step(runner, &exe, &args, &home, "pip", on_progress)?;

    // 3. Pacotes: os wheels já conferidos aqui, e o pip confere de novo
    //    (`--require-hashes`), sem ir à rede (`--no-index`).
    let mut requirements = String::new();
    for package in &spec.packages {
        download_verified(&client, &package.wheel, &downloads, "packages", on_progress).await?;
        requirements.push_str(&format!(
            "{}=={} --hash=sha256:{}\n",
            package.name, package.version, package.wheel.sha256
        ));
    }
    if !spec.packages.is_empty() {
        let req = tmp.join("requirements.txt");
        std::fs::write(&req, requirements).map_err(|e| e.to_string())?;
        let args = vec![
            "-m".into(),
            "pip".into(),
            "install".into(),
            "--no-index".into(),
            "--find-links".into(),
            downloads.to_string_lossy().to_string(),
            "--require-hashes".into(),
            // Nunca desinstala nada: o runtime nasce vazio.
            "--ignore-installed".into(),
            "--only-binary=:all:".into(),
            "--no-warn-script-location".into(),
            "-r".into(),
            req.to_string_lossy().to_string(),
        ];
        run_step(runner, &exe, &args, &home, "packages", on_progress)?;
    }

    // 4. Pronto: grava a versão e troca o `current` de uma vez.
    let version = VersionFile {
        python: spec.python_version.clone(),
        packages: spec.packages.iter().map(|p| format!("{}=={}", p.name, p.version)).collect(),
    };
    std::fs::write(
        home.join(VERSION_FILE),
        serde_json::to_string_pretty(&version).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let current = base.join(CURRENT);
    remove_dir_if_exists(&current)?;
    std::fs::rename(&home, &current).map_err(|e| format!("Could not finish the install: {e}"))?;
    let _ = std::fs::remove_dir_all(tmp);
    on_progress(progress("ready", format!("Python {} is ready", spec.python_version), 0, 0));
    Ok(())
}

fn run_step(
    runner: &dyn StepRunner,
    exe: &Path,
    args: &[String],
    cwd: &Path,
    stage: &str,
    on_progress: &mut (dyn FnMut(PythonInstallProgress) + Send),
) -> Result<(), String> {
    on_progress(progress(stage, format!("Running {}", args.join(" ")), 0, 0));
    let code = runner.run(exe, args, cwd, &mut |line| on_progress(progress(stage, line, 0, 0)))?;
    if code != 0 {
        return Err(format!("The {stage} step failed (exit code {code}). See the lines above."));
    }
    Ok(())
}

#[cfg(test)]
mod python_runtime_tests {
    use super::*;
    use std::sync::Mutex;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!("ram-pyrt-{tag}-{nanos}"));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[derive(Default)]
    struct FakeRunner {
        calls: Mutex<Vec<Vec<String>>>,
    }
    impl StepRunner for FakeRunner {
        fn run(&self, _exe: &Path, args: &[String], _cwd: &Path, on_line: &mut dyn FnMut(String)) -> Result<i32, String> {
            on_line("Successfully installed".into());
            self.calls.lock().unwrap().push(args.to_vec());
            Ok(0)
        }
    }

    fn fake_embeddable_zip() -> Vec<u8> {
        use std::io::Write;
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::FileOptions::default();
            zip.start_file("python.exe", options).unwrap();
            zip.write_all(b"MZ fake").unwrap();
            zip.start_file("python312._pth", options).unwrap();
            zip.write_all(b"python312.zip\r\n.\r\n\r\n# Uncomment to run site.main() automatically\r\n#import site\r\n")
                .unwrap();
            zip.start_file("../evil.txt", options).unwrap();
            zip.write_all(b"outside").unwrap();
            zip.finish().unwrap();
        }
        cursor.into_inner()
    }

    async fn serve(server: &MockServer, route: &str, body: Vec<u8>) {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(server)
            .await;
    }

    async fn spec_on(server: &MockServer, python_hash: Option<&str>) -> RuntimeSpec {
        let zip = fake_embeddable_zip();
        let zip_hash = python_hash.map(str::to_string).unwrap_or_else(|| sha256_hex(&zip));
        serve(server, "/python.zip", zip).await;
        serve(server, "/pip.whl", b"pip wheel".to_vec()).await;
        serve(server, "/numpy.whl", b"numpy wheel".to_vec()).await;
        RuntimeSpec {
            python_version: "3.12.10".into(),
            python: Artifact::new("python.zip", &format!("{}/python.zip", server.uri()), &zip_hash),
            pip: Artifact::new("pip.whl", &format!("{}/pip.whl", server.uri()), &sha256_hex(b"pip wheel")),
            packages: vec![Package {
                name: "numpy".into(),
                version: "2.2.6".into(),
                wheel: Artifact::new("numpy.whl", &format!("{}/numpy.whl", server.uri()), &sha256_hex(b"numpy wheel")),
            }],
        }
    }

    #[test]
    fn the_official_spec_pins_every_download_with_a_sha256() {
        let spec = RuntimeSpec::official();
        let all = std::iter::once(&spec.python)
            .chain(std::iter::once(&spec.pip))
            .chain(spec.packages.iter().map(|p| &p.wheel));
        for artifact in all {
            assert!(artifact.url.starts_with("https://www.python.org/") || artifact.url.starts_with("https://files.pythonhosted.org/"), "{}", artifact.url);
            assert!(artifact.url.ends_with(&artifact.file), "{}", artifact.url);
            assert_eq!(artifact.sha256.len(), 64);
            assert!(artifact.sha256.chars().all(|c| c.is_ascii_hexdigit()));
        }
        assert_eq!(spec.packages.len(), 2);
    }

    #[tokio::test]
    async fn hash_mismatch_aborts_and_leaves_no_runtime() {
        let server = MockServer::start().await;
        let spec = spec_on(&server, Some(&"0".repeat(64))).await;
        let base = TempDir::new("mismatch");
        let ws = TempDir::new("mismatch-ws");
        let runner = FakeRunner::default();
        let mut stages = Vec::new();
        let result = install_into(&base.0, &ws.0, &spec, &runner, &mut |p| stages.push(p.stage)).await;
        let error = result.expect_err("a wrong hash must fail");
        assert!(error.contains("SHA-256"), "{error}");
        assert!(!base.0.join(CURRENT).exists());
        assert!(!base.0.join(TMP).exists());
        assert!(runner.calls.lock().unwrap().is_empty(), "nothing may run after a bad download");
        assert!(!status_in(&base.0).installed);
    }

    #[test]
    fn pth_file_enables_site_and_workspace() {
        let original = "python312.zip\r\n.\r\n\r\n# Uncomment to run site.main() automatically\r\n#import site\r\n";
        let ws = PathBuf::from(r"C:\Users\x\AppData\Local\Roblox Account Manager\python");
        let patched = patch_pth(original, &ws);
        let lines: Vec<&str> = patched.lines().map(str::trim).collect();
        assert!(lines.contains(&"import site"), "{patched}");
        assert!(!lines.contains(&"#import site"));
        assert!(lines.contains(&"Lib\\site-packages"));
        assert!(lines.contains(&ws.to_string_lossy().as_ref()));
        assert!(lines.contains(&"python312.zip") && lines.contains(&"."));
        // Idempotente: rodar de novo não duplica.
        assert_eq!(patch_pth(&patched, &ws), patched);
    }

    #[test]
    fn status_reports_not_installed_on_a_fresh_dir() {
        let base = TempDir::new("fresh");
        let status = status_in(&base.0);
        assert!(!status.installed);
        assert_eq!(status.version, None);
        assert_eq!(status.python_exe, None);
        assert!(status.packages.is_empty());
        assert!(python_exe_in(&base.0).is_none());
    }

    #[tokio::test]
    async fn reinstall_keeps_the_workspace() {
        let server = MockServer::start().await;
        let spec = spec_on(&server, None).await;
        let base = TempDir::new("reinstall");
        let ws = TempDir::new("reinstall-ws");
        std::fs::create_dir_all(ws.0.join("scripts")).unwrap();
        std::fs::write(ws.0.join("scripts").join("farm.py"), "print('mine')").unwrap();
        let runner = FakeRunner::default();

        install_into(&base.0, &ws.0, &spec, &runner, &mut |_| {}).await.expect("install");
        let status = status_in(&base.0);
        assert!(status.installed, "{status:?}");
        assert_eq!(status.version.as_deref(), Some("3.12.10"));
        assert_eq!(status.packages, vec!["numpy==2.2.6".to_string()]);
        assert!(!base.0.join(TMP).exists());
        assert!(!base.0.join("evil.txt").exists() && !base.0.join(CURRENT).join("..").join("evil.txt").exists());
        let pth = std::fs::read_to_string(base.0.join(CURRENT).join("python312._pth")).unwrap();
        assert!(pth.contains(&ws.0.to_string_lossy().to_string()));

        let calls = runner.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], vec!["-m".to_string(), "pip".into(), "--version".into()]);
        assert!(calls[1].contains(&"--require-hashes".to_string()));

        // Reinstall: o runtime é trocado, a pasta de trabalho fica.
        std::fs::write(base.0.join(CURRENT).join("marker"), "old").unwrap();
        install_into(&base.0, &ws.0, &spec, &runner, &mut |_| {}).await.expect("reinstall");
        assert!(!base.0.join(CURRENT).join("marker").exists(), "the old runtime was replaced");
        assert_eq!(std::fs::read_to_string(ws.0.join("scripts").join("farm.py")).unwrap(), "print('mine')");
    }

    #[test]
    fn python_never_sees_the_users_own_site_packages() {
        assert!(ISOLATION_FLAGS.contains(&"-s"));
        assert!(ISOLATION_FLAGS.contains(&"-E"));
    }

    #[test]
    fn a_second_concurrent_install_is_refused() {
        let first = begin_install().expect("first");
        assert!(begin_install().is_none());
        drop(first);
        assert!(begin_install().is_some());
    }

    /// Instalação de verdade (python.org + PyPI, ~65 MB) numa pasta temporária,
    /// conferindo que `numpy` e `cv2` importam no Python portátil. Manual:
    /// `cargo test --all-features --lib real_install -- --ignored --nocapture`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore]
    async fn real_install_from_the_official_sources() {
        let base = TempDir::new("real");
        let ws = TempDir::new("real-ws");
        let spec = RuntimeSpec::official();
        let base_path = base.0.clone();
        let ws_path = ws.0.clone();
        let result = tokio::task::spawn_blocking(move || {
            tauri::async_runtime::block_on(install_into(&base_path, &ws_path, &spec, &RealStepRunner, &mut |p| {
                if p.downloaded == 0 {
                    eprintln!("[{}] {}", p.stage, p.message);
                }
            }))
        })
        .await
        .unwrap();
        result.expect("install");
        let exe = python_exe_in(&base.0).expect("python.exe");
        let mut lines = Vec::new();
        let code = RealStepRunner
            .run(
                &exe,
                &["-c".into(), "import sys, numpy, cv2; print(sys.version.split()[0], numpy.__version__, cv2.__version__); print(sys.path)".into()],
                &base.0,
                &mut |l| lines.push(l),
            )
            .unwrap();
        eprintln!("{}", lines.join("
"));
        assert_eq!(code, 0);
        assert!(lines[0].starts_with("3.12.10 2.2.6 4.12.0"), "{lines:?}");
        // A pasta de trabalho está no sys.path (pelo ._pth); o site-packages do
        // usuário, não.
        assert!(lines[1].contains("ram-pyrt-real-ws"), "{lines:?}");
        assert!(!lines[1].contains("Roaming"), "{lines:?}");
    }
}
