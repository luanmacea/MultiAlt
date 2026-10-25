// A pasta de trabalho (`<pasta de dados>/python/`): scripts do usuário,
// `lib/`, `assets/`, o SDK (`multialt/`) e os blocos padrão
// (docs/features/python-automation.md, "Pasta de trabalho").
//
// Os arquivos do SDK vão embutidos no app (`include_str!`). Os do app são
// reescritos a cada sincronização; os **blocos** (`multialt/defaults/*.py`) são
// do usuário quando ele os edita: bloco que ele não mexeu acompanha o app,
// bloco editado fica e ganha o aviso "New default available". O hash de
// referência de cada bloco (o original do qual o usuário partiu) fica em
// `multialt/defaults/.originals.json`. Os hashes são do texto com quebras de
// linha normalizadas, para o CRLF/LF do checkout não contar como edição.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::types::{DefaultBlockInfo, PythonRunConfig, PythonScriptFile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdkKind {
    AppOwned,
    Block,
}

pub struct SdkFile {
    pub path: &'static str,
    pub contents: &'static str,
    pub kind: SdkKind,
}

macro_rules! sdk {
    ($path:literal, $kind:ident) => {
        SdkFile {
            path: $path,
            contents: include_str!(concat!("../../python-sdk/", $path)),
            kind: SdkKind::$kind,
        }
    };
}

/// Todo arquivo do SDK que vai para a pasta de trabalho. `tests/` fica só no
/// repositório.
pub const SDK_FILES: &[SdkFile] = &[
    sdk!("multialt/__init__.py", AppOwned),
    sdk!("multialt/__init__.pyi", AppOwned),
    sdk!("multialt/__main__.py", AppOwned),
    sdk!("multialt/cli.py", AppOwned),
    sdk!("multialt/_core/__init__.py", AppOwned),
    sdk!("multialt/_core/protocol.py", AppOwned),
    sdk!("multialt/_core/host.py", AppOwned),
    sdk!("multialt/_core/primitives.py", AppOwned),
    sdk!("multialt/defaults/__init__.py", AppOwned),
    sdk!("multialt/defaults/runner.py", Block),
    sdk!("multialt/defaults/focus.py", Block),
    sdk!("multialt/defaults/vision.py", Block),
    sdk!("multialt/defaults/actions.py", Block),
    sdk!("multialt/defaults/dashboard.py", Block),
    sdk!("AGENTS.md", AppOwned),
    sdk!("CLAUDE.md", AppOwned),
    sdk!("docs/API.md", AppOwned),
    sdk!("examples/anti_afk_pixel.py", AppOwned),
    sdk!("examples/click_button_image.py", AppOwned),
    sdk!("examples/routine_all_accounts.py", AppOwned),
    sdk!("examples/farm_dashboard.py", AppOwned),
    sdk!("templates/blank.py", AppOwned),
    sdk!("templates/per_account.py", AppOwned),
    sdk!("templates/controller.py", AppOwned),
    sdk!("templates/dashboard.py", AppOwned),
];

/// O exemplo da primeira abertura (`python-sdk/seed/`): criado **uma vez só**,
/// e só o arquivo que ainda não existe. Depois é do usuário: apagado, não
/// volta (o marcador `SEED_MARKER` fica).
pub const SEED_FILES: &[(&str, &str)] = &[
    ("scripts/example_walk_jump.py", include_str!("../../python-sdk/seed/scripts/example_walk_jump.py")),
    ("lib/__init__.py", include_str!("../../python-sdk/seed/lib/__init__.py")),
    ("lib/helpers.py", include_str!("../../python-sdk/seed/lib/helpers.py")),
];
pub const SEED_MARKER: &str = ".seeded-examples";

const BLOCKS_DIR: &str = "multialt/defaults";
const ORIGINALS: &str = "multialt/defaults/.originals.json";
const RUNS_FILE: &str = "runs.json";
pub const SCRIPTS_DIR: &str = "scripts";
pub const ASSETS_DIR: &str = "assets";
pub const LIB_DIR: &str = "lib";

/// Os modelos do "New script".
pub const TEMPLATES: &[(&str, &str)] = &[
    ("blank", "templates/blank.py"),
    ("per_account", "templates/per_account.py"),
    ("controller", "templates/controller.py"),
    ("dashboard", "templates/dashboard.py"),
];

/// O que uma sincronização fez.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SyncReport {
    pub written: Vec<String>,
    pub blocks_updated: Vec<String>,
    pub blocks_kept: Vec<String>,
    /// Os arquivos do exemplo criados agora (só na primeira sincronização).
    pub seeded: Vec<String>,
}

pub fn content_hash(text: &str) -> String {
    use sha2::Digest;
    let normalized = text.replace("\r\n", "\n");
    let digest = sha2::Sha256::digest(normalized.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn io_err(path: &Path, e: std::io::Error) -> String {
    format!("{}: {e}", path.display())
}

fn write_file(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }
    std::fs::write(path, contents).map_err(|e| io_err(path, e))
}

fn block_files() -> impl Iterator<Item = &'static SdkFile> {
    SDK_FILES.iter().filter(|f| f.kind == SdkKind::Block)
}

/// `runner.py` -> o bloco. Nome que não é bloco: `None`.
fn find_block(name: &str) -> Option<&'static SdkFile> {
    block_files().find(|f| f.path.rsplit('/').next() == Some(name))
}

fn block_name(file: &SdkFile) -> &'static str {
    file.path.rsplit('/').next().unwrap_or(file.path)
}

/// Os nomes dos blocos, na ordem da tela (`runner.py`, `focus.py`, ...).
pub fn block_names() -> impl Iterator<Item = &'static str> {
    block_files().map(block_name)
}

pub fn is_block_name(name: &str) -> bool {
    find_block(name).is_some()
}


fn bad_block(name: &str) -> String {
    super::errors::PyError::bad_args("default block", &format!("\"{name}\" is not a default block"))
        .message
}

fn load_originals(root: &Path) -> HashMap<String, String> {
    std::fs::read_to_string(root.join(ORIGINALS))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_originals(root: &Path, originals: &HashMap<String, String>) -> Result<(), String> {
    let mut sorted: Vec<(&String, &String)> = originals.iter().collect();
    sorted.sort();
    let map: serde_json::Map<String, serde_json::Value> = sorted
        .into_iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect();
    write_file(
        &root.join(ORIGINALS),
        &serde_json::to_string_pretty(&map).map_err(|e| e.to_string())?,
    )
}

/// Cria as pastas, reescreve os arquivos do app e atualiza os blocos que o
/// usuário não editou. Roda ao abrir a aba e antes de cada execução.
pub fn sync_workspace(root: &Path) -> Result<SyncReport, String> {
    let mut report = SyncReport::default();
    for dir in [SCRIPTS_DIR, LIB_DIR, ASSETS_DIR] {
        let path = root.join(dir);
        std::fs::create_dir_all(&path).map_err(|e| io_err(&path, e))?;
    }
    seed_examples(root, &mut report)?;
    let mut originals = load_originals(root);
    for file in SDK_FILES {
        let path = root.join(file.path);
        match file.kind {
            SdkKind::AppOwned => {
                let current = std::fs::read_to_string(&path).ok();
                if current.as_deref() != Some(file.contents) {
                    write_file(&path, file.contents)?;
                    report.written.push(file.path.to_string());
                }
            }
            SdkKind::Block => {
                let name = block_name(file).to_string();
                let embedded = content_hash(file.contents);
                let on_disk = std::fs::read_to_string(&path).ok();
                match on_disk {
                    None => {
                        write_file(&path, file.contents)?;
                        originals.insert(name.clone(), embedded);
                        report.written.push(file.path.to_string());
                    }
                    Some(text) => {
                        let disk = content_hash(&text);
                        let original = originals.get(&name).cloned();
                        match original {
                            // Sem referência (o .originals.json sumiu): igual ao
                            // embutido conta como não editado; diferente, como
                            // editado a partir do embutido.
                            None => {
                                originals.insert(name.clone(), embedded.clone());
                                if disk != embedded {
                                    report.blocks_kept.push(name);
                                }
                            }
                            Some(original) if disk == original => {
                                if embedded != original {
                                    write_file(&path, file.contents)?;
                                    originals.insert(name.clone(), embedded);
                                    report.blocks_updated.push(name);
                                }
                            }
                            Some(_) => report.blocks_kept.push(name),
                        }
                    }
                }
            }
        }
    }
    save_originals(root, &originals)?;
    Ok(report)
}

/// Cria o exemplo uma vez: sem o marcador, grava cada arquivo de `SEED_FILES`
/// que ainda não existe (o que o usuário já tem nunca é tocado) e deixa o
/// marcador. Com o marcador, nada.
fn seed_examples(root: &Path, report: &mut SyncReport) -> Result<(), String> {
    let marker = root.join(SEED_MARKER);
    if marker.exists() {
        return Ok(());
    }
    for (rel, contents) in SEED_FILES {
        let path = root.join(rel);
        if path.exists() {
            continue;
        }
        write_file(&path, contents)?;
        report.seeded.push((*rel).to_string());
    }
    write_file(&marker, "Examples created once by MultiAlt. Delete this file to get them back.
")
}

pub fn list_blocks(root: &Path) -> Result<Vec<DefaultBlockInfo>, String> {
    let originals = load_originals(root);
    Ok(block_files()
        .map(|file| {
            let name = block_name(file).to_string();
            let embedded = content_hash(file.contents);
            let original = originals.get(&name).cloned().unwrap_or_else(|| embedded.clone());
            let disk = std::fs::read_to_string(root.join(file.path))
                .map(|t| content_hash(&t))
                .unwrap_or_else(|_| original.clone());
            let modified = disk != original;
            DefaultBlockInfo {
                name,
                modified,
                new_default_available: modified && embedded != original,
            }
        })
        .collect())
}

pub fn read_block(root: &Path, name: &str) -> Result<String, String> {
    let file = find_block(name).ok_or_else(|| bad_block(name))?;
    let path = root.join(file.path);
    std::fs::read_to_string(&path).or_else(|_| Ok(file.contents.to_string()))
}

/// Grava a edição do usuário num bloco (quem chama confere o Modo
/// desenvolvedor). O hash de referência não muda: é ele que diz "Modified".
pub fn write_block(root: &Path, name: &str, source: &str) -> Result<(), String> {
    let file = find_block(name).ok_or_else(|| bad_block(name))?;
    write_file(&root.join(file.path), source)
}

/// Devolve **um** bloco ao padrão desta versão. Os outros blocos e os scripts
/// não são tocados.
pub fn restore_block(root: &Path, name: &str) -> Result<(), String> {
    let file = find_block(name).ok_or_else(|| bad_block(name))?;
    write_file(&root.join(file.path), file.contents)?;
    let mut originals = load_originals(root);
    originals.insert(name.to_string(), content_hash(file.contents));
    save_originals(root, &originals)
}

pub const DEVELOPER_MODE_OFF: &str = "Turn on developer mode to edit default blocks.";

// ── scripts ────────────────────────────────────────────────────────────────

/// `^[A-Za-z0-9_\-]{1,64}\.py$`: só nome, nunca caminho.
pub fn validate_script_name(name: &str) -> Result<(), String> {
    let Some(stem) = name.strip_suffix(".py") else {
        return Err(format!("Invalid script name \"{name}\": it must end with .py"));
    };
    let ok = !stem.is_empty()
        && stem.len() <= 64
        && stem.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "Invalid script name \"{name}\": use 1-64 letters, digits, '_' or '-', then .py"
        ))
    }
}

pub fn script_path(root: &Path, name: &str) -> Result<PathBuf, String> {
    validate_script_name(name)?;
    Ok(root.join(SCRIPTS_DIR).join(name))
}

pub fn list_scripts(root: &Path) -> Result<Vec<PythonScriptFile>, String> {
    let dir = root.join(SCRIPTS_DIR);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let mut out: Vec<PythonScriptFile> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            validate_script_name(&name).ok()?;
            let meta = entry.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            let modified_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            Some(PythonScriptFile {
                name,
                size: meta.len(),
                modified_ms,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));
    Ok(out)
}

pub fn read_script(root: &Path, name: &str) -> Result<String, String> {
    let path = script_path(root, name)?;
    std::fs::read_to_string(&path).map_err(|e| io_err(&path, e))
}

pub fn write_script(root: &Path, name: &str, source: &str) -> Result<(), String> {
    write_file(&script_path(root, name)?, source)
}

pub fn template_contents(template: &str) -> Option<&'static str> {
    let path = TEMPLATES.iter().find(|(id, _)| *id == template)?.1;
    SDK_FILES.iter().find(|f| f.path == path).map(|f| f.contents)
}

/// Cria um script novo a partir de um modelo. Não sobrescreve um que existe.
pub fn create_script(root: &Path, name: &str, template: &str) -> Result<(), String> {
    let path = script_path(root, name)?;
    let contents = template_contents(template)
        .ok_or_else(|| format!("Unknown template \"{template}\""))?;
    if path.exists() {
        return Err(format!("A script named {name} already exists"));
    }
    write_file(&path, contents)
}

pub fn delete_script(root: &Path, name: &str) -> Result<(), String> {
    let path = script_path(root, name)?;
    std::fs::remove_file(&path).map_err(|e| io_err(&path, e))
}

// ── configurações das execuções (`runs.json`) ────────────────────────────

pub fn load_run_configs(root: &Path) -> HashMap<String, PythonRunConfig> {
    std::fs::read_to_string(root.join(RUNS_FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_run_config(root: &Path, config: &PythonRunConfig) -> Result<(), String> {
    validate_script_name(&config.script)?;
    let mut all = load_run_configs(root);
    all.insert(config.script.clone(), config.clone());
    let mut sorted: Vec<(&String, &PythonRunConfig)> = all.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let map: serde_json::Map<String, serde_json::Value> = sorted
        .into_iter()
        .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or_default()))
        .collect();
    write_file(
        &root.join(RUNS_FILE),
        &serde_json::to_string_pretty(&map).map_err(|e| e.to_string())?,
    )
}

// ── abrir no Explorer / no editor ─────────────────────────────────────────

/// Abre a pasta no Explorer (ShellExecute: nenhum console).
pub fn open_folder(path: &Path) -> Result<(), String> {
    shell_open(path, "open")
}

/// Abre o script no editor. **Nunca** pelo verbo "open" quando o `.py` está
/// associado ao próprio Python: aí "abrir" seria rodar o script. Nesse caso vai
/// para o Bloco de Notas.
pub fn open_in_editor(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        if py_open_runs_python() {
            return shell_open_with("notepad.exe", path);
        }
    }
    shell_open(path, "open")
}

/// O executável que o Windows usa no "open" de `.py` é um Python?
#[cfg(windows)]
fn py_open_runs_python() -> bool {
    use windows_sys::Win32::UI::Shell::{AssocQueryStringW, ASSOCF_NONE, ASSOCSTR_EXECUTABLE};
    let ext = wide(".py");
    let verb = wide("open");
    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    let hr = unsafe {
        AssocQueryStringW(
            ASSOCF_NONE,
            ASSOCSTR_EXECUTABLE,
            ext.as_ptr(),
            verb.as_ptr(),
            buf.as_mut_ptr(),
            &mut len,
        )
    };
    if hr != 0 {
        // Sem associação: o "open" cai no "abrir com" do Windows, que não roda nada.
        return false;
    }
    let exe = String::from_utf16_lossy(&buf[..(len as usize).saturating_sub(1)]).to_ascii_lowercase();
    exe_runs_python(&exe)
}

fn exe_runs_python(exe: &str) -> bool {
    let file = exe.rsplit(['\\', '/']).next().unwrap_or(exe);
    file.starts_with("python") || file.starts_with("py.exe") || file == "pyw.exe"
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn shell_open(path: &Path, verb: &str) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let file: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let verb = wide(verb);
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if (result as isize) <= 32 {
        return Err(format!("Windows could not open {}", path.display()));
    }
    Ok(())
}

#[cfg(windows)]
fn shell_open_with(program: &str, path: &Path) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let verb = wide("open");
    let program_w = wide(program);
    let params = wide(&format!("\"{}\"", path.display()));
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            program_w.as_ptr(),
            params.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if (result as isize) <= 32 {
        return Err(format!("Windows could not open {}", path.display()));
    }
    Ok(())
}

#[cfg(not(windows))]
fn shell_open(path: &Path, _verb: &str) -> Result<(), String> {
    Err(format!("Opening {} is only available on Windows", path.display()))
}

#[cfg(test)]
mod python_workspace_tests {
    use super::*;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!("ram-pyws-{tag}-{nanos}"));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn block_path(root: &Path, name: &str) -> PathBuf {
        root.join(BLOCKS_DIR).join(name)
    }

    /// Simula "a versão anterior do app embutia outro runner.py": o arquivo e o
    /// hash de referência são de um texto antigo.
    fn pretend_previous_version(root: &Path, name: &str, old_text: &str) {
        std::fs::write(block_path(root, name), old_text).unwrap();
        let mut originals = load_originals(root);
        originals.insert(name.into(), content_hash(old_text));
        save_originals(root, &originals).unwrap();
    }

    #[test]
    fn sync_creates_folders_and_app_files() {
        let ws = TempDir::new("sync");
        let report = sync_workspace(&ws.0).unwrap();
        for dir in [SCRIPTS_DIR, LIB_DIR, ASSETS_DIR] {
            assert!(ws.0.join(dir).is_dir(), "{dir}");
        }
        for file in SDK_FILES {
            let text = std::fs::read_to_string(ws.0.join(file.path)).unwrap();
            assert_eq!(text, file.contents, "{}", file.path);
        }
        assert_eq!(report.written.len(), SDK_FILES.len());
        // Arquivo do app editado por fora volta ao do app.
        std::fs::write(ws.0.join("multialt/_core/protocol.py"), "# hacked").unwrap();
        let again = sync_workspace(&ws.0).unwrap();
        assert_eq!(again.written, vec!["multialt/_core/protocol.py".to_string()]);
        assert!(list_blocks(&ws.0).unwrap().iter().all(|b| !b.modified && !b.new_default_available));
    }

    #[test]
    fn unmodified_block_is_updated_on_new_version() {
        let ws = TempDir::new("update");
        sync_workspace(&ws.0).unwrap();
        pretend_previous_version(&ws.0, "runner.py", "# runner from the previous version\n");
        let report = sync_workspace(&ws.0).unwrap();
        assert_eq!(report.blocks_updated, vec!["runner.py".to_string()]);
        let embedded = find_block("runner.py").unwrap().contents;
        assert_eq!(std::fs::read_to_string(block_path(&ws.0, "runner.py")).unwrap(), embedded);
        let info = list_blocks(&ws.0).unwrap();
        let runner = info.iter().find(|b| b.name == "runner.py").unwrap();
        assert!(!runner.modified && !runner.new_default_available);
    }

    #[test]
    fn modified_block_is_kept_and_flagged() {
        let ws = TempDir::new("modified");
        sync_workspace(&ws.0).unwrap();
        // O usuário partiu do runner antigo e o editou.
        pretend_previous_version(&ws.0, "runner.py", "# runner from the previous version\n");
        std::fs::write(block_path(&ws.0, "runner.py"), "# my own runner\n").unwrap();
        // E editou o focus.py desta versão.
        std::fs::write(block_path(&ws.0, "focus.py"), "# my own focus\n").unwrap();

        let report = sync_workspace(&ws.0).unwrap();
        assert!(report.blocks_updated.is_empty());
        assert_eq!(std::fs::read_to_string(block_path(&ws.0, "runner.py")).unwrap(), "# my own runner\n");
        assert_eq!(std::fs::read_to_string(block_path(&ws.0, "focus.py")).unwrap(), "# my own focus\n");

        let info = list_blocks(&ws.0).unwrap();
        let get = |n: &str| info.iter().find(|b| b.name == n).unwrap().clone();
        assert!(get("runner.py").modified && get("runner.py").new_default_available);
        assert!(get("focus.py").modified && !get("focus.py").new_default_available);
        assert!(!get("vision.py").modified);
        // CRLF no disco não conta como edição.
        let vision = find_block("vision.py").unwrap().contents.replace("\r\n", "\n").replace('\n', "\r\n");
        std::fs::write(block_path(&ws.0, "vision.py"), vision).unwrap();
        let info = list_blocks(&ws.0).unwrap();
        assert!(!info.iter().find(|b| b.name == "vision.py").unwrap().modified);
    }

    #[test]
    fn restore_one_block_leaves_the_others() {
        let ws = TempDir::new("restore");
        sync_workspace(&ws.0).unwrap();
        std::fs::create_dir_all(ws.0.join(SCRIPTS_DIR)).unwrap();
        write_script(&ws.0, "farm.py", "print('farm')").unwrap();
        write_block(&ws.0, "runner.py", "# my runner\n").unwrap();
        write_block(&ws.0, "focus.py", "# my focus\n").unwrap();

        restore_block(&ws.0, "runner.py").unwrap();
        assert_eq!(
            std::fs::read_to_string(block_path(&ws.0, "runner.py")).unwrap(),
            find_block("runner.py").unwrap().contents
        );
        assert_eq!(std::fs::read_to_string(block_path(&ws.0, "focus.py")).unwrap(), "# my focus\n");
        assert_eq!(read_script(&ws.0, "farm.py").unwrap(), "print('farm')");
        let info = list_blocks(&ws.0).unwrap();
        assert!(!info.iter().find(|b| b.name == "runner.py").unwrap().modified);
        assert!(info.iter().find(|b| b.name == "focus.py").unwrap().modified);

        // Só nomes de bloco: nada de arquivo do app nem caminho.
        for bad in ["protocol.py", "../runner.py", "__init__.py", "farm.py", ""] {
            let error = restore_block(&ws.0, bad).expect_err(bad);
            assert!(error.starts_with("Invalid arguments for default block"), "{error}");
        }
    }

    #[test]
    fn script_names_reject_paths_and_bad_extensions() {
        for good in ["farm.py", "a.py", "Farm_2-x.py", &format!("{}.py", "a".repeat(64))] {
            assert!(validate_script_name(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            ".py",
            "farm",
            "farm.txt",
            "farm.py.txt",
            "../farm.py",
            "sub/farm.py",
            "sub\\farm.py",
            "C:farm.py",
            "fa rm.py",
            "farm.PY",
            "ção.py",
            &format!("{}.py", "a".repeat(65)),
        ] {
            assert!(validate_script_name(bad).is_err(), "{bad:?}");
        }
        let ws = TempDir::new("names");
        assert!(write_script(&ws.0, "../evil.py", "x").is_err());
        assert!(!ws.0.join("evil.py").exists());
    }

    #[test]
    fn create_from_template_writes_the_template() {
        let ws = TempDir::new("template");
        sync_workspace(&ws.0).unwrap();
        for (id, path) in TEMPLATES {
            let name = format!("new_{id}.py");
            create_script(&ws.0, &name, id).unwrap();
            let expected = SDK_FILES.iter().find(|f| f.path == *path).unwrap().contents;
            assert_eq!(read_script(&ws.0, &name).unwrap(), expected);
        }
        assert!(create_script(&ws.0, "new_blank.py", "blank").is_err(), "never overwrites");
        assert!(create_script(&ws.0, "x.py", "nope").is_err());
        // Os modelos mais o exemplo semeado.
        let listed: Vec<String> = list_scripts(&ws.0).unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(listed.len(), TEMPLATES.len() + 1);
        delete_script(&ws.0, "new_blank.py").unwrap();
        assert_eq!(list_scripts(&ws.0).unwrap().len(), TEMPLATES.len());
    }

    #[test]
    fn run_configs_round_trip_per_script() {
        use super::super::types::{PythonRunMode, PythonTrigger};
        let ws = TempDir::new("runs");
        let config = PythonRunConfig {
            script: "farm.py".into(),
            accounts: vec![1, 2],
            mode: PythonRunMode::AllTogether,
            trigger: PythonTrigger::Interval { minutes: 5 },
        };
        save_run_config(&ws.0, &config).unwrap();
        assert_eq!(load_run_configs(&ws.0).get("farm.py"), Some(&config));
        let bad = PythonRunConfig { script: "../x.py".into(), ..config };
        assert!(save_run_config(&ws.0, &bad).is_err());
    }

    #[test]
    fn opening_a_script_never_runs_it_with_python() {
        assert!(exe_runs_python(r"c:\python312\python.exe"));
        assert!(exe_runs_python(r"c:\windows\py.exe"));
        assert!(exe_runs_python(r"c:\windows\pyw.exe"));
        assert!(exe_runs_python(r"c:\python312\pythonw.exe"));
        assert!(!exe_runs_python(r"c:\program files\microsoft vs code\code.exe"));
        assert!(!exe_runs_python(r"c:\windows\notepad.exe"));
    }

    #[test]
    fn the_sdk_ships_every_contract_file_and_no_tests() {
        assert!(SDK_FILES.iter().all(|f| !f.path.starts_with("tests/")));
        let blocks: Vec<&str> = block_files().map(block_name).collect();
        assert_eq!(blocks, vec!["runner.py", "focus.py", "vision.py", "actions.py", "dashboard.py"]);
        assert!(SDK_FILES.iter().all(|f| !f.contents.trim().is_empty()), "every SDK file has content");
    }
}

#[cfg(test)]
mod python_seed_tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ram-pyseed-{tag}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_example_is_seeded_once_and_never_comes_back() {
        let ws = temp("once");
        let first = sync_workspace(&ws).unwrap();
        let rels: Vec<&str> = SEED_FILES.iter().map(|(rel, _)| *rel).collect();
        assert_eq!(first.seeded, rels);
        for (rel, contents) in SEED_FILES {
            assert_eq!(std::fs::read_to_string(ws.join(rel)).unwrap(), *contents, "{rel}");
        }
        assert!(ws.join(SEED_MARKER).is_file());
        assert!(list_scripts(&ws).unwrap().iter().any(|s| s.name == "example_walk_jump.py"));

        // Editado, fica como está; apagado, não volta.
        std::fs::write(ws.join("lib/helpers.py"), "# mine
").unwrap();
        std::fs::remove_file(ws.join("scripts/example_walk_jump.py")).unwrap();
        let again = sync_workspace(&ws).unwrap();
        assert!(again.seeded.is_empty());
        assert_eq!(std::fs::read_to_string(ws.join("lib/helpers.py")).unwrap(), "# mine
");
        assert!(!ws.join("scripts/example_walk_jump.py").exists());
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn seeding_never_overwrites_a_file_the_user_already_has() {
        let ws = temp("keep");
        std::fs::create_dir_all(ws.join("lib")).unwrap();
        std::fs::write(ws.join("lib/helpers.py"), "def mine():
    pass
").unwrap();
        let report = sync_workspace(&ws).unwrap();
        assert!(!report.seeded.contains(&"lib/helpers.py".to_string()));
        assert_eq!(std::fs::read_to_string(ws.join("lib/helpers.py")).unwrap(), "def mine():
    pass
");
        assert!(ws.join("scripts/example_walk_jump.py").is_file());
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn the_seeded_example_uses_the_real_sdk_api() {
        let example = SEED_FILES.iter().find(|(rel, _)| *rel == "scripts/example_walk_jump.py").unwrap().1;
        assert!(validate_script_name("example_walk_jump.py").is_ok(), "runnable from the Run button");
        for needle in [
            "from lib.helpers import walk_forward",
            "from multialt.defaults.actions import jitter_sleep",
            "REPORT = True",
            "dashboard.report_loop(account)",
            "account.press(\"Escape\")",
            "wait_pixel",
            "Inspector",
        ] {
            assert!(example.contains(needle), "{needle}");
        }
        let actions = SDK_FILES.iter().find(|f| f.path == "multialt/defaults/actions.py").unwrap().contents;
        assert!(actions.contains("def jitter_sleep("));
        let helpers = SEED_FILES.iter().find(|(rel, _)| *rel == "lib/helpers.py").unwrap().1;
        assert!(helpers.contains("def walk_forward(account"));
    }
}

/// Os testes do SDK (Python `unittest`, `src-tauri/python-sdk/tests`) rodam na
/// suíte `python` do Rust. Sem `python` no PATH, pula com aviso.
#[cfg(test)]
mod python_sdk_unittest_tests {
    #[test]
    fn the_python_sdk_unit_tests_pass() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut command = std::process::Command::new("python");
        command
            .args([
                "-m",
                "unittest",
                "discover",
                "-s",
                "src-tauri/python-sdk/tests",
                "-t",
                "src-tauri/python-sdk",
            ])
            .current_dir(&repo)
            .env("PYTHONDONTWRITEBYTECODE", "1");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let output = match command.output() {
            Ok(output) => output,
            Err(e) => {
                eprintln!("python not on PATH ({e}): skipping the SDK unit tests");
                return;
            }
        };
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "python -m unittest failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            stderr
        );
        assert!(stderr.contains("OK"), "{stderr}");
    }
}
