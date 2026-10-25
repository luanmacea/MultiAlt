// O Explorer da aba Code (docs/features/python-automation.md, "Explorer"): a
// árvore da pasta de trabalho e as operações de arquivo da tela.
//
// O que aparece: `scripts/`, `lib/`, `assets/` (graváveis), `examples/` (só
// leitura) e `multialt/defaults/` (os blocos padrão, com cadeado; graváveis só
// com o Modo desenvolvedor, e aí pelo mesmo caminho do
// `python_write_default_block`). O resto da pasta (o SDK, `runs.json`,
// `dashboard.json`, `.trash/`, os marcadores) não aparece e não é aceito.
//
// Caminho é sempre relativo à pasta de trabalho, com `/`, e cada pedaço passa
// pela regra de nome antes de tocar o disco; depois o caminho resolvido é
// conferido (canonicalize) dentro da pasta, o que pega junção/link que aponte
// para fora. Apagar não apaga: move para `.trash/<ms>-<nome>`.

use std::path::{Path, PathBuf};

use super::types::FsNode;
use super::workspace::{self, ASSETS_DIR, LIB_DIR, SCRIPTS_DIR};

pub const WRITABLE_DIRS: [&str; 3] = [SCRIPTS_DIR, LIB_DIR, ASSETS_DIR];
pub const EXAMPLES_DIR: &str = "examples";
pub const TRASH_DIR: &str = ".trash";
const BLOCKS_PARENT: &str = "multialt";
const BLOCKS_DIR_NAME: &str = "defaults";

/// O maior arquivo que o editor abre (texto) e grava.
pub const MAX_TEXT_BYTES: u64 = 1024 * 1024;
/// A maior imagem que a prévia de `assets/` mostra.
pub const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
/// Profundidade máxima de um caminho (e da árvore).
const MAX_DEPTH: usize = 12;
/// Nós no máximo na árvore: pasta com milhares de arquivos não trava a tela.
const MAX_NODES: usize = 5_000;

pub const ONLY_WRITABLE: &str = "Only scripts/, lib/ and assets/ can be changed.";

fn bad_name(segment: &str) -> String {
    format!(
        "That name is not allowed: \"{segment}\". Use 1-64 letters, digits, spaces, '_', '-' or '.', not starting with '.'."
    )
}

/// Onde um caminho cai.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Area {
    /// A raiz de uma pasta gravável (`scripts`, `lib`, `assets`): dá para criar
    /// dentro, não para renomear nem apagar.
    WritableRoot(&'static str),
    /// Algo dentro de uma pasta gravável.
    Writable(&'static str),
    /// `examples/` e o que tem dentro.
    Example,
    /// `multialt` ou `multialt/defaults` (as pastas dos blocos).
    BlocksDir,
    /// Um bloco padrão (`multialt/defaults/runner.py`): o nome do arquivo.
    Block(String),
}

/// Confere cada pedaço do caminho. `""` é a raiz.
pub fn split_path(rel: &str) -> Result<Vec<&str>, String> {
    if rel.is_empty() {
        return Ok(Vec::new());
    }
    if rel.contains('\\') || rel.contains(':') {
        return Err(bad_name(rel));
    }
    let segments: Vec<&str> = rel.split('/').collect();
    if segments.len() > MAX_DEPTH {
        return Err("That path is too deep.".into());
    }
    for segment in &segments {
        validate_segment(segment)?;
    }
    Ok(segments)
}

/// `^[A-Za-z0-9 _\-.]{1,64}$`, sem começar com `.`. Também recusa o que o
/// Windows trata de outro jeito: terminar em espaço ou ponto (ele corta) e os
/// nomes de dispositivo (`CON`, `NUL`, `COM1`...).
pub fn validate_segment(segment: &str) -> Result<(), String> {
    let charset = segment
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '-' | '.'));
    let ok = !segment.is_empty()
        && segment.len() <= 64
        && charset
        && !segment.starts_with('.')
        && !segment.ends_with('.')
        && !segment.ends_with(' ')
        && !segment.starts_with(' ')
        && !is_device_name(segment);
    if ok {
        Ok(())
    } else {
        Err(bad_name(segment))
    }
}

fn is_device_name(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or(segment).trim_end().to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit())
}

/// A área de um caminho já validado. `None`: fora do que a tela vê.
pub fn area_of(segments: &[&str]) -> Option<Area> {
    let first = *segments.first()?;
    if let Some(dir) = WRITABLE_DIRS.iter().find(|d| **d == first) {
        return Some(if segments.len() == 1 { Area::WritableRoot(dir) } else { Area::Writable(dir) });
    }
    if first == EXAMPLES_DIR {
        return Some(Area::Example);
    }
    if first == BLOCKS_PARENT {
        return match segments {
            [_] => Some(Area::BlocksDir),
            [_, d] if *d == BLOCKS_DIR_NAME => Some(Area::BlocksDir),
            [_, d, name] if *d == BLOCKS_DIR_NAME && workspace::is_block_name(name) => {
                Some(Area::Block((*name).to_string()))
            }
            _ => None,
        };
    }
    None
}

/// O caminho no disco, conferido dentro da pasta de trabalho: o ancestral mais
/// fundo que existe é resolvido (links e junções inclusive) e tem de cair
/// dentro da pasta.
fn resolve(root: &Path, segments: &[&str]) -> Result<PathBuf, String> {
    let mut path = root.to_path_buf();
    for segment in segments {
        path.push(segment);
    }
    let canonical_root = std::fs::canonicalize(root).map_err(|e| format!("{}: {e}", root.display()))?;
    let mut existing = path.as_path();
    loop {
        if existing.exists() {
            break;
        }
        match existing.parent() {
            Some(parent) => existing = parent,
            None => return Err("That path is outside the Python folder.".into()),
        }
    }
    let canonical = std::fs::canonicalize(existing).map_err(|e| format!("{}: {e}", existing.display()))?;
    if !canonical.starts_with(&canonical_root) {
        return Err("That path is outside the Python folder.".into());
    }
    Ok(path)
}

/// Arquivo ou pasta que a árvore pula: oculto, cache do Python, link/junção.
pub(super) fn skipped_entry(name: &str, meta: &std::fs::Metadata) -> bool {
    if name.starts_with('.') || name == "__pycache__" || validate_segment(name).is_err() {
        return true;
    }
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

fn modified_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn sort_nodes(nodes: &mut [FsNode]) {
    nodes.sort_by(|a, b| {
        (a.kind != "dir")
            .cmp(&(b.kind != "dir"))
            .then_with(|| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()))
    });
}

fn is_runnable(segments: &[&str]) -> bool {
    matches!(segments, [dir, name] if *dir == SCRIPTS_DIR && workspace::validate_script_name(name).is_ok())
}

/// Lê uma pasta da área `writable` (ou só leitura) recursivamente.
fn walk(dir: &Path, rel: &str, writable: bool, budget: &mut usize, depth: usize) -> Vec<FsNode> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        if *budget == 0 || depth > MAX_DEPTH {
            break;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if skipped_entry(&name, &meta) {
            continue;
        }
        *budget -= 1;
        let path = format!("{rel}/{name}");
        let segments: Vec<&str> = path.split('/').collect();
        if meta.is_dir() {
            let children = walk(&entry.path(), &path, writable, budget, depth + 1);
            out.push(FsNode {
                path,
                name,
                kind: "dir".into(),
                size: 0,
                modified_ms: modified_ms(&meta),
                writable,
                pinned: false,
                runnable: false,
                children: Some(children),
            });
        } else if meta.is_file() {
            out.push(FsNode {
                runnable: is_runnable(&segments),
                path,
                name,
                kind: "file".into(),
                size: meta.len(),
                modified_ms: modified_ms(&meta),
                writable,
                pinned: false,
                children: None,
            });
        }
    }
    sort_nodes(&mut out);
    out
}

fn dir_node(root: &Path, path: &str, writable: bool, pinned: bool, children: Vec<FsNode>) -> FsNode {
    let meta = std::fs::metadata(root.join(path)).ok();
    FsNode {
        path: path.to_string(),
        name: path.rsplit('/').next().unwrap_or(path).to_string(),
        kind: "dir".into(),
        size: 0,
        modified_ms: meta.as_ref().map(modified_ms).unwrap_or(0),
        writable,
        pinned,
        runnable: false,
        children: Some(children),
    }
}

/// A árvore da tela. `developer_mode` decide se os blocos aparecem graváveis.
pub fn tree(root: &Path, developer_mode: bool) -> Result<FsNode, String> {
    let mut budget = MAX_NODES;
    let mut top = Vec::new();
    for dir in [SCRIPTS_DIR, LIB_DIR, ASSETS_DIR] {
        let children = walk(&root.join(dir), dir, true, &mut budget, 2);
        top.push(dir_node(root, dir, true, false, children));
    }
    let examples = walk(&root.join(EXAMPLES_DIR), EXAMPLES_DIR, false, &mut budget, 2);
    top.push(dir_node(root, EXAMPLES_DIR, false, false, examples));

    let blocks_rel = format!("{BLOCKS_PARENT}/{BLOCKS_DIR_NAME}");
    let blocks: Vec<FsNode> = workspace::block_names()
        .map(|name| {
            let path = format!("{blocks_rel}/{name}");
            let meta = std::fs::metadata(root.join(&path)).ok();
            FsNode {
                path,
                name: name.to_string(),
                kind: "file".into(),
                size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                modified_ms: meta.as_ref().map(modified_ms).unwrap_or(0),
                writable: developer_mode,
                pinned: true,
                runnable: false,
                children: None,
            }
        })
        .collect();
    let defaults = dir_node(root, &blocks_rel, false, true, blocks);
    top.push(dir_node(root, BLOCKS_PARENT, false, true, vec![defaults]));

    let mut node = dir_node(root, "", false, false, top);
    node.name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "python".into());
    Ok(node)
}

/// Um arquivo visível na árvore: o caminho e a área.
fn visible_file(root: &Path, rel: &str) -> Result<(PathBuf, Area), String> {
    let segments = split_path(rel)?;
    let area = area_of(&segments).ok_or("That file is not in the Python folder's explorer.")?;
    if matches!(area, Area::WritableRoot(_) | Area::BlocksDir) {
        return Err(format!("{rel} is a folder."));
    }
    let path = resolve(root, &segments)?;
    Ok((path, area))
}

/// O texto de um arquivo (até 1 MB). Binário: erro claro.
pub fn read_text(root: &Path, rel: &str) -> Result<String, String> {
    let (path, area) = visible_file(root, rel)?;
    if let Area::Block(name) = &area {
        return workspace::read_block(root, name);
    }
    let meta = std::fs::metadata(&path).map_err(|_| format!("{rel} does not exist."))?;
    if meta.is_dir() {
        return Err(format!("{rel} is a folder."));
    }
    if meta.len() > MAX_TEXT_BYTES {
        return Err(format!("{rel} is larger than 1 MB, too big for the editor."));
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("{rel}: {e}"))?;
    if bytes.contains(&0) {
        return Err(format!("{rel} is not a text file, so it can't be opened in the editor."));
    }
    String::from_utf8(bytes).map_err(|_| format!("{rel} is not a text file, so it can't be opened in the editor."))
}

pub fn image_mime(rel: &str) -> Option<&'static str> {
    let ext = rel.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "bmp" => "image/bmp",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return None,
    })
}

/// Uma imagem de `assets/` como data URL (`data:image/png;base64,...`).
pub fn read_image(root: &Path, rel: &str) -> Result<String, String> {
    let (path, area) = visible_file(root, rel)?;
    if area != Area::Writable(ASSETS_DIR) {
        return Err("Only images in assets/ can be previewed.".into());
    }
    let mime = image_mime(rel).ok_or_else(|| format!("{rel} is not an image (png, jpg, bmp, gif, webp)."))?;
    let meta = std::fs::metadata(&path).map_err(|_| format!("{rel} does not exist."))?;
    if !meta.is_file() {
        return Err(format!("{rel} is a folder."));
    }
    if meta.len() > MAX_IMAGE_BYTES {
        return Err(format!("{rel} is larger than 10 MB, too big to preview."));
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("{rel}: {e}"))?;
    use base64::Engine;
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// O que a gravação de um arquivo vai fazer: gravar no disco, ou passar pelo
/// caminho dos blocos (quem chama confere o Modo desenvolvedor).
#[derive(Debug, PartialEq, Eq)]
pub enum WriteTarget {
    File(PathBuf),
    Block(String),
}

pub fn write_target(root: &Path, rel: &str) -> Result<WriteTarget, String> {
    let (path, area) = visible_file(root, rel).map_err(|e| {
        if e.starts_with("That name") || e.contains("is a folder") {
            e
        } else {
            ONLY_WRITABLE.to_string()
        }
    })?;
    match area {
        Area::Writable(_) => {
            if path.is_dir() {
                return Err(format!("{rel} is a folder."));
            }
            if !path.parent().map(|p| p.is_dir()).unwrap_or(false) {
                return Err(format!("The folder of {rel} does not exist."));
            }
            Ok(WriteTarget::File(path))
        }
        Area::Block(name) => Ok(WriteTarget::Block(name)),
        _ => Err(ONLY_WRITABLE.into()),
    }
}

/// Grava um arquivo de `scripts/`, `lib/` ou `assets/` (até 1 MB).
pub fn write_file(path: &Path, content: &str) -> Result<(), String> {
    if content.len() as u64 > MAX_TEXT_BYTES {
        return Err("The file is larger than 1 MB.".into());
    }
    std::fs::write(path, content).map_err(|e| format!("{}: {e}", path.display()))
}

/// Algo dentro de uma pasta gravável (nunca a raiz dela). Devolve o caminho e
/// a pasta de topo.
fn writable_entry(root: &Path, rel: &str) -> Result<(PathBuf, &'static str), String> {
    let segments = split_path(rel)?;
    match area_of(&segments) {
        Some(Area::Writable(dir)) => Ok((resolve(root, &segments)?, dir)),
        Some(Area::WritableRoot(_)) => Err(format!("{rel}/ itself can't be renamed or deleted.")),
        _ => Err(ONLY_WRITABLE.into()),
    }
}

/// `kind` = `"file"` (vazio) ou `"dir"`. Nunca sobrescreve.
pub fn create(root: &Path, rel: &str, kind: &str) -> Result<(), String> {
    if kind != "file" && kind != "dir" {
        return Err(format!("Unknown kind \"{kind}\": use \"file\" or \"dir\"."));
    }
    let (path, _) = writable_entry(root, rel).map_err(|e| {
        if e.ends_with("can't be renamed or deleted.") {
            format!("{rel} already exists.")
        } else {
            e
        }
    })?;
    if path.exists() {
        return Err(format!("{rel} already exists."));
    }
    if !path.parent().map(|p| p.is_dir()).unwrap_or(false) {
        return Err(format!("The folder of {rel} does not exist."));
    }
    if kind == "dir" {
        std::fs::create_dir(&path).map_err(|e| format!("{rel}: {e}"))
    } else {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map(|_| ())
            .map_err(|e| format!("{rel}: {e}"))
    }
}

/// Renomeia (ou move) dentro da mesma pasta gravável de topo.
pub fn rename(root: &Path, from: &str, to: &str) -> Result<(), String> {
    let (src, src_dir) = writable_entry(root, from)?;
    let (dst, dst_dir) = writable_entry(root, to)?;
    if src_dir != dst_dir {
        return Err(format!("Files can only be moved inside {src_dir}/."));
    }
    std::fs::symlink_metadata(&src).map_err(|_| format!("{from} does not exist."))?;
    if to.starts_with(&format!("{from}/")) {
        return Err("A folder can't be moved into itself.".into());
    }
    if dst.exists() {
        // Mudar só maiúscula/minúscula no Windows: o destino "existe" porque é
        // o mesmo arquivo.
        let same = std::fs::canonicalize(&src).ok() == std::fs::canonicalize(&dst).ok();
        if !same {
            return Err(format!("{to} already exists."));
        }
    }
    if !dst.parent().map(|p| p.is_dir()).unwrap_or(false) {
        return Err(format!("The folder of {to} does not exist."));
    }
    std::fs::rename(&src, &dst).map_err(|e| format!("Could not rename {from}: {e}"))
}

/// Move para `.trash/<ms>-<nome>` (nada é apagado de verdade). Devolve o
/// caminho relativo na lixeira.
pub fn delete(root: &Path, rel: &str) -> Result<String, String> {
    let (src, _) = writable_entry(root, rel)?;
    std::fs::symlink_metadata(&src).map_err(|_| format!("{rel} does not exist."))?;
    let trash = root.join(TRASH_DIR);
    std::fs::create_dir_all(&trash).map_err(|e| format!("{}: {e}", trash.display()))?;
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let ms = super::types::now_ms();
    let mut target = trash.join(format!("{ms}-{name}"));
    let mut n = 1;
    while target.exists() {
        target = trash.join(format!("{ms}-{n}-{name}"));
        n += 1;
    }
    std::fs::rename(&src, &target).map_err(|e| format!("Could not delete {rel}: {e}"))?;
    Ok(format!(
        "{TRASH_DIR}/{}",
        target.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
    ))
}

#[cfg(test)]
mod python_fs_tests {
    use super::*;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!("ram-pyfs-{tag}-{nanos}"));
            std::fs::create_dir_all(&dir).unwrap();
            workspace::sync_workspace(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn find<'a>(node: &'a FsNode, path: &str) -> Option<&'a FsNode> {
        if node.path == path {
            return Some(node);
        }
        node.children.as_ref()?.iter().find_map(|c| find(c, path))
    }

    fn all_paths(node: &FsNode, out: &mut Vec<String>) {
        out.push(node.path.clone());
        for child in node.children.iter().flatten() {
            all_paths(child, out);
        }
    }

    #[test]
    fn path_traversal_and_bad_names_are_refused() {
        let ws = TempDir::new("traversal");
        let outside = ws.0.parent().unwrap().join("ram-pyfs-outside.txt");
        for bad in [
            "../x.py",
            "scripts/../../x.py",
            "scripts/..",
            "scripts/./a.py",
            "scripts\\a.py",
            "C:/Windows/x.py",
            "scripts/C:x.py",
            "/scripts/a.py",
            "scripts//a.py",
            "scripts/a.py/",
            "scripts/.hidden.py",
            "scripts/a\u{e7}.py",
            "scripts/con.py",
            "scripts/NUL",
            "scripts/lpt1.txt",
            "scripts/trailing.",
            "scripts/trailing ",
            "scripts/a*b.py",
            &format!("scripts/{}.py", "a".repeat(64)),
        ] {
            assert!(split_path(bad).is_err() || write_target(&ws.0, bad).is_err(), "{bad:?}");
            assert!(write_target(&ws.0, bad).is_err(), "{bad:?}");
            assert!(create(&ws.0, bad, "file").is_err(), "{bad:?}");
            assert!(read_text(&ws.0, bad).is_err(), "{bad:?}");
        }
        assert!(!outside.exists());
        let error = create(&ws.0, "scripts/a*b.py", "file").unwrap_err();
        assert!(error.starts_with("That name is not allowed"), "{error}");
        for good in ["scripts/my farm.py", "lib/sub-dir/x_1.py", "assets/a.b.png"] {
            assert!(split_path(good).is_ok(), "{good}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_junction_pointing_outside_is_refused() {
        let ws = TempDir::new("junction");
        let outside = TempDir::new("junction-target");
        let link = ws.0.join("lib").join("out");
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "mklink", "/J"]).arg(&link).arg(&outside.0);
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let made = command.output().map(|o| o.status.success()).unwrap_or(false);
        if !made {
            eprintln!("mklink /J failed: skipping");
            return;
        }
        let error = write_target(&ws.0, "lib/out/evil.py").unwrap_err();
        assert!(error.contains("outside") || error == ONLY_WRITABLE, "{error}");
        assert!(create(&ws.0, "lib/out/evil.py", "file").is_err());
        assert!(!outside.0.join("evil.py").exists());
        // A árvore não entra na junção.
        let tree = tree(&ws.0, false).unwrap();
        assert!(find(&tree, "lib/out").is_none());
        let _ = std::fs::remove_dir(&link);
    }

    #[test]
    fn writes_outside_the_writable_folders_are_refused() {
        let ws = TempDir::new("writes");
        for rel in [
            "examples/farm_dashboard.py",
            "multialt/_core/protocol.py",
            "multialt/__init__.py",
            "multialt/defaults/__init__.py",
            "AGENTS.md",
            "docs/API.md",
            "runs.json",
            "templates/blank.py",
            "new.py",
        ] {
            let before = std::fs::read_to_string(ws.0.join(rel)).ok();
            assert_eq!(write_target(&ws.0, rel).unwrap_err(), ONLY_WRITABLE, "{rel}");
            assert!(create(&ws.0, rel, "file").is_err(), "{rel}");
            assert!(delete(&ws.0, rel).is_err(), "{rel}");
            assert_eq!(std::fs::read_to_string(ws.0.join(rel)).ok(), before, "{rel}");
        }
        // As raízes graváveis não se apagam nem se renomeiam.
        for rel in ["scripts", "lib", "assets"] {
            assert!(delete(&ws.0, rel).is_err(), "{rel}");
            assert!(rename(&ws.0, rel, "lib/x").is_err(), "{rel}");
            assert!(ws.0.join(rel).is_dir());
        }
        // Bloco: vai para o caminho dos blocos (quem chama confere o Modo
        // desenvolvedor), nunca direto para o disco.
        assert_eq!(
            write_target(&ws.0, "multialt/defaults/runner.py").unwrap(),
            WriteTarget::Block("runner.py".into())
        );
        assert!(delete(&ws.0, "multialt/defaults/runner.py").is_err());
        // Dentro das pastas graváveis vale.
        let WriteTarget::File(path) = write_target(&ws.0, "lib/tools.py").unwrap() else { panic!() };
        write_file(&path, "def f():\n    pass\n").unwrap();
        assert_eq!(read_text(&ws.0, "lib/tools.py").unwrap(), "def f():\n    pass\n");
        assert!(write_file(&path, &"x".repeat(MAX_TEXT_BYTES as usize + 1)).is_err());
        assert!(write_target(&ws.0, "lib/missing/tools.py").is_err(), "the folder must exist");
    }

    #[test]
    fn create_rename_and_delete_stay_inside_one_writable_folder() {
        let ws = TempDir::new("ops");
        create(&ws.0, "lib/menus", "dir").unwrap();
        create(&ws.0, "lib/menus/shop.py", "file").unwrap();
        assert!(create(&ws.0, "lib/menus/shop.py", "file").is_err(), "never overwrites");
        assert!(create(&ws.0, "lib/x.py", "link").is_err());
        assert!(create(&ws.0, "scripts", "dir").is_err());
        rename(&ws.0, "lib/menus/shop.py", "lib/menus/store.py").unwrap();
        assert!(ws.0.join("lib/menus/store.py").is_file());
        rename(&ws.0, "lib/menus/store.py", "lib/store.py").unwrap();
        assert!(ws.0.join("lib/store.py").is_file());
        // Para outra pasta de topo, não.
        assert!(rename(&ws.0, "lib/store.py", "scripts/store.py").is_err());
        assert!(rename(&ws.0, "lib/menus", "lib/menus/inner").is_err());
        create(&ws.0, "lib/other.py", "file").unwrap();
        assert!(rename(&ws.0, "lib/store.py", "lib/other.py").unwrap_err().contains("already exists"));
        // Só a caixa muda: vale.
        rename(&ws.0, "lib/store.py", "lib/Store.py").unwrap();
        assert!(rename(&ws.0, "lib/nope.py", "lib/nope2.py").is_err());
    }

    #[test]
    fn delete_moves_to_the_trash() {
        let ws = TempDir::new("trash");
        std::fs::write(ws.0.join("scripts/farm.py"), "print(1)").unwrap();
        let moved = delete(&ws.0, "scripts/farm.py").unwrap();
        assert!(!ws.0.join("scripts/farm.py").exists());
        assert!(moved.starts_with(".trash/") && moved.ends_with("-farm.py"), "{moved}");
        assert_eq!(std::fs::read_to_string(ws.0.join(&moved)).unwrap(), "print(1)");
        // Duas vezes no mesmo milissegundo não se sobrescrevem.
        std::fs::write(ws.0.join("scripts/farm.py"), "print(2)").unwrap();
        let again = delete(&ws.0, "scripts/farm.py").unwrap();
        assert_ne!(again, moved);
        create(&ws.0, "assets/pics", "dir").unwrap();
        std::fs::write(ws.0.join("assets/pics/a.png"), [1u8, 2]).unwrap();
        let dir = delete(&ws.0, "assets/pics").unwrap();
        assert!(ws.0.join(&dir).join("a.png").is_file());
        assert!(delete(&ws.0, "scripts/farm.py").is_err(), "already gone");
        // A lixeira não aparece nem se abre.
        let tree = tree(&ws.0, false).unwrap();
        let mut paths = Vec::new();
        all_paths(&tree, &mut paths);
        assert!(paths.iter().all(|p| !p.contains(".trash")), "{paths:?}");
        assert!(read_text(&ws.0, &moved).is_err());
    }

    #[test]
    fn the_tree_hides_the_internals() {
        let ws = TempDir::new("tree");
        std::fs::write(ws.0.join("scripts/.secret.py"), "").unwrap();
        std::fs::create_dir_all(ws.0.join("lib/__pycache__")).unwrap();
        std::fs::write(ws.0.join("lib/__pycache__/x.pyc"), "").unwrap();
        std::fs::write(ws.0.join("dashboard.json"), "{}").unwrap();
        std::fs::write(ws.0.join("runs.json"), "{}").unwrap();
        delete(&ws.0, "lib/helpers.py").unwrap();
        let tree = tree(&ws.0, false).unwrap();
        assert_eq!(tree.path, "");
        assert_eq!(tree.kind, "dir");
        let top: Vec<&str> = tree.children.as_ref().unwrap().iter().map(|n| n.path.as_str()).collect();
        assert_eq!(top, vec!["scripts", "lib", "assets", "examples", "multialt"]);
        let mut paths = Vec::new();
        all_paths(&tree, &mut paths);
        for hidden in [
            "multialt/_core",
            "multialt/__init__.py",
            "multialt/__init__.pyi",
            "multialt/cli.py",
            "multialt/defaults/__init__.py",
            "multialt/defaults/.originals.json",
            ".trash",
            "runs.json",
            "dashboard.json",
            ".seeded-examples",
            "scripts/.secret.py",
            "lib/__pycache__",
            "AGENTS.md",
            "templates",
        ] {
            assert!(!paths.iter().any(|p| p == hidden || p.starts_with(&format!("{hidden}/"))), "{hidden}");
        }
        let blocks = find(&tree, "multialt/defaults").unwrap();
        assert!(blocks.pinned && !blocks.writable);
        let names: Vec<&str> = blocks.children.as_ref().unwrap().iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["runner.py", "focus.py", "vision.py", "actions.py", "dashboard.py"]);
        let runner = find(&tree, "multialt/defaults/runner.py").unwrap();
        assert!(runner.pinned && !runner.writable && runner.size > 0);
        let dev = super::tree(&ws.0, true).unwrap();
        assert!(find(&dev, "multialt/defaults/runner.py").unwrap().writable);
        let example = find(&tree, "examples/farm_dashboard.py").unwrap();
        assert!(!example.writable && !example.runnable);
        assert!(find(&tree, "scripts").unwrap().writable);
        assert!(find(&tree, "lib/__init__.py").unwrap().writable);
        // A tela abre o que a árvore mostra.
        assert!(read_text(&ws.0, "examples/farm_dashboard.py").unwrap().contains("REPORT"));
        assert!(read_text(&ws.0, "multialt/defaults/runner.py").unwrap().contains("def run"));
        assert!(read_text(&ws.0, "multialt/_core/protocol.py").is_err());
    }

    #[test]
    fn only_top_level_scripts_are_runnable() {
        let ws = TempDir::new("runnable");
        std::fs::write(ws.0.join("scripts/farm.py"), "").unwrap();
        std::fs::write(ws.0.join("scripts/my farm.py"), "").unwrap();
        std::fs::write(ws.0.join("scripts/notes.txt"), "").unwrap();
        create(&ws.0, "scripts/sub", "dir").unwrap();
        std::fs::write(ws.0.join("scripts/sub/deep.py"), "").unwrap();
        std::fs::write(ws.0.join("lib/tool.py"), "").unwrap();
        let tree = tree(&ws.0, true).unwrap();
        assert!(find(&tree, "scripts/farm.py").unwrap().runnable);
        assert!(find(&tree, "scripts/example_walk_jump.py").unwrap().runnable);
        for not in ["scripts/my farm.py", "scripts/notes.txt", "scripts/sub/deep.py", "scripts/sub", "lib/tool.py"] {
            assert!(!find(&tree, not).unwrap().runnable, "{not}");
        }
        // Pastas antes dos arquivos.
        let names: Vec<&str> =
            find(&tree, "scripts").unwrap().children.as_ref().unwrap().iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names[0], "sub");
    }

    #[test]
    fn binary_and_large_files_are_not_opened_as_text_and_images_preview_only_in_assets() {
        let ws = TempDir::new("binary");
        std::fs::write(ws.0.join("assets/button.png"), [0x89u8, b'P', b'N', b'G', 0, 1]).unwrap();
        let error = read_text(&ws.0, "assets/button.png").unwrap_err();
        assert!(error.contains("not a text file"), "{error}");
        assert_eq!(read_image(&ws.0, "assets/button.png").unwrap(), "data:image/png;base64,iVBORwAB");
        std::fs::write(ws.0.join("lib/pic.png"), [1u8]).unwrap();
        assert!(read_image(&ws.0, "lib/pic.png").is_err());
        std::fs::write(ws.0.join("assets/notes.txt"), "hi").unwrap();
        assert!(read_image(&ws.0, "assets/notes.txt").is_err());
        std::fs::write(ws.0.join("lib/big.py"), "x".repeat(MAX_TEXT_BYTES as usize + 1)).unwrap();
        assert!(read_text(&ws.0, "lib/big.py").unwrap_err().contains("1 MB"));
        assert!(read_text(&ws.0, "lib/missing.py").unwrap_err().contains("does not exist"));
    }

    #[test]
    fn fs_node_reaches_the_frontend_in_camel_case() {
        let node = FsNode {
            path: "scripts/a.py".into(),
            name: "a.py".into(),
            kind: "file".into(),
            size: 1,
            modified_ms: 2,
            writable: true,
            pinned: false,
            runnable: true,
            children: None,
        };
        let v = serde_json::to_value(node).unwrap();
        for key in ["path", "name", "kind", "size", "modifiedMs", "writable", "pinned", "runnable", "children"] {
            assert!(v.get(key).is_some(), "{key}");
        }
    }
}
