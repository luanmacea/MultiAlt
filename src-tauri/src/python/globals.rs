// A aba Global functions: os módulos que um script pode importar — os blocos
// padrão (`multialt.defaults.*`) e os do usuário em `lib/` — com as funções de
// nível de topo de cada um. As funções saem do **texto** do arquivo (`def` na
// coluna 0, fora de string), sem rodar Python: abrir a aba nunca executa
// código do usuário.

use std::path::Path;

use super::types::{GlobalFunction, GlobalModule};
use super::workspace::{self, LIB_DIR};

/// Arquivo maior que isto não é lido (não é um módulo de funções).
const MAX_MODULE_BYTES: u64 = 1024 * 1024;
/// Módulos de `lib/` no máximo.
const MAX_LIB_FILES: usize = 500;
const MAX_LIB_DEPTH: usize = 8;
/// A primeira linha da docstring é cortada aqui.
const MAX_DOC_CHARS: usize = 200;

/// As funções públicas (nome sem `_` na frente) de nível de topo, na ordem do
/// arquivo. `def` e `async def`; assinatura de várias linhas vira uma só.
pub fn parse_functions(source: &str) -> Vec<GlobalFunction> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut depth = 0i32;
    let mut line_start = true;
    while i < chars.len() {
        if line_start && depth == 0 {
            line_start = false;
            if let Some((function, next)) = parse_def(&chars, i) {
                if let Some(function) = function {
                    if !function.name.starts_with('_') {
                        out.push(function);
                    }
                }
                i = next;
                continue;
            }
        }
        match chars[i] {
            '\n' => {
                line_start = true;
                i += 1;
            }
            '#' => i = skip_comment(&chars, i),
            '\'' | '"' => i = skip_string(&chars, i).0,
            '(' | '[' | '{' => {
                depth += 1;
                i += 1;
            }
            ')' | ']' | '}' => {
                depth = (depth - 1).max(0);
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

fn skip_comment(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && chars[i] != '\n' {
        i += 1;
    }
    i
}

/// `(fim, conteúdo)` da string que começa na aspa `chars[i]` (simples ou
/// tripla). String simples sem fechar acaba no fim da linha.
fn skip_string(chars: &[char], i: usize) -> (usize, String) {
    let quote = chars[i];
    let triple = chars.get(i + 1) == Some(&quote) && chars.get(i + 2) == Some(&quote);
    let mut j = if triple { i + 3 } else { i + 1 };
    let start = j;
    while j < chars.len() {
        let c = chars[j];
        if c == '\\' {
            j += 2;
            continue;
        }
        if triple {
            if c == quote && chars.get(j + 1) == Some(&quote) && chars.get(j + 2) == Some(&quote) {
                let content: String = chars[start..j].iter().collect();
                return (j + 3, content);
            }
        } else if c == quote {
            let content: String = chars[start..j].iter().collect();
            return (j + 1, content);
        } else if c == '\n' {
            break;
        }
        j += 1;
    }
    let end = j.min(chars.len());
    (end, chars[start.min(end)..end].iter().collect())
}

fn starts_with_word(chars: &[char], i: usize, word: &str) -> Option<usize> {
    let mut j = i;
    for w in word.chars() {
        if chars.get(j) != Some(&w) {
            return None;
        }
        j += 1;
    }
    match chars.get(j) {
        Some(' ') | Some('\t') => Some(j),
        _ => None,
    }
}

fn skip_blanks(chars: &[char], mut i: usize) -> usize {
    while matches!(chars.get(i), Some(' ') | Some('\t')) {
        i += 1;
    }
    i
}

/// Na coluna `i`: `def nome(...)` ou `async def nome(...)`? `None`: não é um
/// `def` (a varredura segue normal). `Some((função, onde continuar))`, com a
/// função `None` quando o `def` está malformado.
fn parse_def(chars: &[char], i: usize) -> Option<(Option<GlobalFunction>, usize)> {
    let after = match starts_with_word(chars, i, "def") {
        Some(j) => j,
        None => {
            let j = starts_with_word(chars, i, "async")?;
            starts_with_word(chars, skip_blanks(chars, j), "def")?
        }
    };
    let mut k = skip_blanks(chars, after);
    let name_start = k;
    while k < chars.len() && (chars[k].is_alphanumeric() || chars[k] == '_') {
        k += 1;
    }
    if k == name_start {
        return Some((None, k));
    }
    let name: String = chars[name_start..k].iter().collect();
    k = skip_blanks(chars, k);
    // Parâmetros de tipo (`def f[T](...)`, Python 3.12).
    if chars.get(k) == Some(&'[') {
        let mut depth = 0;
        while k < chars.len() {
            match chars[k] {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        k += 1;
                        break;
                    }
                }
                '\n' => return Some((None, k)),
                _ => {}
            }
            k += 1;
        }
        k = skip_blanks(chars, k);
    }
    if chars.get(k) != Some(&'(') {
        return Some((None, k));
    }
    k += 1;
    let mut depth = 1;
    let mut params = String::new();
    loop {
        let Some(&c) = chars.get(k) else {
            return Some((None, chars.len()));
        };
        match c {
            '\'' | '"' => {
                let (end, _) = skip_string(chars, k);
                params.extend(&chars[k..end]);
                k = end;
                continue;
            }
            '#' => {
                k = skip_comment(chars, k);
                continue;
            }
            '\\' if chars.get(k + 1) == Some(&'\n') => {
                params.push(' ');
                k += 2;
                continue;
            }
            '(' | '[' | '{' => {
                depth += 1;
                params.push(c);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    k += 1;
                    break;
                }
                params.push(c);
            }
            c if c.is_whitespace() => params.push(' '),
            c => params.push(c),
        }
        k += 1;
    }
    // O `:` do fim do cabeçalho (depois de um `-> anotação` opcional).
    let mut depth = 0;
    let colon = loop {
        let Some(&c) = chars.get(k) else {
            return Some((None, chars.len()));
        };
        match c {
            '\'' | '"' => {
                k = skip_string(chars, k).0;
                continue;
            }
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ':' if depth == 0 => break k,
            '\n' if depth == 0 => return Some((None, k)),
            _ => {}
        }
        k += 1;
    };
    let doc = docstring(chars, colon + 1);
    let signature = format!("{name}({})", normalize_params(&params));
    Some((Some(GlobalFunction { name, signature, doc }), colon + 1))
}

/// A primeira linha da docstring do corpo que começa em `i` (logo depois do
/// `:`), ou vazio.
fn docstring(chars: &[char], mut i: usize) -> String {
    loop {
        while matches!(chars.get(i), Some(' ') | Some('\t') | Some('\r') | Some('\n')) {
            i += 1;
        }
        if chars.get(i) == Some(&'#') {
            i = skip_comment(chars, i);
            continue;
        }
        break;
    }
    if matches!(chars.get(i), Some('r') | Some('R') | Some('u') | Some('U'))
        && matches!(chars.get(i + 1), Some('\'') | Some('"'))
    {
        i += 1;
    }
    if !matches!(chars.get(i), Some('\'') | Some('"')) {
        return String::new();
    }
    let (_, content) = skip_string(chars, i);
    let line = content.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    if line.chars().count() > MAX_DOC_CHARS {
        let cut: String = line.chars().take(MAX_DOC_CHARS).collect();
        format!("{cut}…")
    } else {
        line.to_string()
    }
}

/// Espaços em sequência viram um, sem espaço colado nos parênteses e sem a
/// vírgula final.
fn normalize_params(raw: &str) -> String {
    let mut out = String::new();
    let mut last_space = false;
    for c in raw.chars() {
        if c == ' ' {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(c);
            last_space = false;
        }
    }
    let mut out = out.trim().to_string();
    for (from, to) in [("( ", "("), (" )", ")"), ("[ ", "["), (" ]", "]"), (" ,", ",")] {
        while out.contains(from) {
            out = out.replace(from, to);
        }
    }
    let out = out.trim_end_matches(|c: char| c == ',' || c == ' ');
    out.to_string()
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `(caminho relativo, módulo)` dos `.py` importáveis de `lib/`.
fn collect_lib(dir: &Path, rel: &str, module: &str, out: &mut Vec<(String, String)>, depth: usize) {
    if depth > MAX_LIB_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= MAX_LIB_FILES {
            return;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if super::explorer::skipped_entry(&name, &meta) {
            continue;
        }
        let path = format!("{rel}/{name}");
        if meta.is_dir() {
            if is_identifier(&name) {
                collect_lib(&entry.path(), &path, &format!("{module}.{name}"), out, depth + 1);
            }
        } else if let Some(stem) = name.strip_suffix(".py") {
            if stem == "__init__" {
                out.push((path, module.to_string()));
            } else if is_identifier(stem) {
                out.push((path, format!("{module}.{stem}")));
            }
        }
    }
}

fn read_module(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_MODULE_BYTES {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// Os blocos padrão (na ordem da tela) e depois os módulos de `lib/` em ordem
/// alfabética. `__init__.py` sem função não aparece.
pub fn list_global_functions(root: &Path) -> Result<Vec<GlobalModule>, String> {
    let mut out = Vec::new();
    for info in workspace::list_blocks(root)? {
        let source = workspace::read_block(root, &info.name).unwrap_or_default();
        let stem = info.name.trim_end_matches(".py");
        out.push(GlobalModule {
            path: format!("multialt/defaults/{}", info.name),
            module: format!("multialt.defaults.{stem}"),
            pinned: true,
            modified: info.modified,
            new_default_available: info.new_default_available,
            functions: parse_functions(&source),
        });
    }
    let mut lib = Vec::new();
    collect_lib(&root.join(LIB_DIR), LIB_DIR, LIB_DIR, &mut lib, 0);
    lib.sort_by(|a, b| a.1.to_ascii_lowercase().cmp(&b.1.to_ascii_lowercase()));
    for (rel, module) in lib {
        let Some(source) = read_module(&root.join(&rel)) else {
            continue;
        };
        let functions = parse_functions(&source);
        if rel.ends_with("/__init__.py") && functions.is_empty() {
            continue;
        }
        out.push(GlobalModule {
            path: rel,
            module,
            pinned: false,
            modified: false,
            new_default_available: false,
            functions,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod python_globals_tests {
    use super::*;

    fn names(source: &str) -> Vec<String> {
        parse_functions(source).into_iter().map(|f| f.name).collect()
    }

    #[test]
    fn top_level_defs_with_signature_and_first_doc_line() {
        let source = r#""""Module doc.

def not_a_function(x):
    this is inside the module docstring
"""

import os


def walk_forward(account, seconds=1.0):
    """Hold W for ``seconds``.

    More text.
    """
    def inner():
        pass
    return None


async def fetch(url: str) -> dict[str, int]:
    'Single quoted doc.'
    pass


def _private(x):
    """Hidden."""


class Thing:
    def method(self):
        """Not top level."""


def no_doc(a, *args, **kwargs): return a
"#;
        let functions = parse_functions(source);
        let got: Vec<(&str, &str, &str)> =
            functions.iter().map(|f| (f.name.as_str(), f.signature.as_str(), f.doc.as_str())).collect();
        assert_eq!(
            got,
            vec![
                ("walk_forward", "walk_forward(account, seconds=1.0)", "Hold W for ``seconds``."),
                ("fetch", "fetch(url: str)", "Single quoted doc."),
                ("no_doc", "no_doc(a, *args, **kwargs)", ""),
            ]
        );
    }

    #[test]
    fn multi_line_signatures_become_one_line() {
        let source = "def click_when(\n    account,          # the account\n    x: float = 0.5,\n    y: float = 0.5,\n    colour: tuple[int, int, int] = (255, 200, 0),\n    label: str = \"a, (b)\",\n) -> bool:\n    r\"\"\"\n    Wait for a colour, then click.\n    \"\"\"\n    return True\n";
        let f = &parse_functions(source)[0];
        assert_eq!(
            f.signature,
            "click_when(account, x: float = 0.5, y: float = 0.5, colour: tuple[int, int, int] = (255, 200, 0), label: str = \"a, (b)\")"
        );
        assert_eq!(f.doc, "Wait for a colour, then click.");
        // Comentário antes da docstring e CRLF.
        let crlf = "def a(x):\r\n    # comment\r\n    \"\"\"Doc a.\"\"\"\r\n\r\ndef b():\r\n    pass\r\n";
        let functions = parse_functions(crlf);
        assert_eq!(names(crlf), vec!["a", "b"]);
        assert_eq!(functions[0].doc, "Doc a.");
        assert_eq!(functions[1].doc, "");
    }

    #[test]
    fn defs_inside_strings_brackets_or_broken_code_are_ignored() {
        let source = "X = '''\ndef fake():\n    pass\n'''\nY = (\ndef\n)\nZ = \"def nope(): pass\"\ndef ok():\n    pass\ndef broken(\n";
        assert_eq!(names(source), vec!["ok"]);
        assert!(parse_functions("").is_empty());
        assert!(parse_functions("def").is_empty());
        assert!(parse_functions("define = 1\ndefault(x)\n").is_empty());
        assert_eq!(names("def generic[T](x: T) -> T:\n    return x\n"), vec!["generic"]);
    }

    #[test]
    fn the_real_blocks_and_the_seeded_helper_parse() {
        let ws = std::env::temp_dir().join(format!(
            "ram-pyglobals-{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        workspace::sync_workspace(&ws).unwrap();
        std::fs::create_dir_all(ws.join("lib/menus")).unwrap();
        std::fs::write(ws.join("lib/menus/shop.py"), "def open_shop(account):\n    \"\"\"Open it.\"\"\"\n").unwrap();
        std::fs::write(ws.join("lib/not-importable.py"), "def x():\n    pass\n").unwrap();
        std::fs::create_dir_all(ws.join("lib/__pycache__")).unwrap();
        std::fs::write(ws.join("lib/__pycache__/junk.py"), "def junk():\n    pass\n").unwrap();
        workspace::write_block(&ws, "vision.py", "def my_pixel(account):\n    \"\"\"Mine.\"\"\"\n").unwrap();

        let modules = list_global_functions(&ws).unwrap();
        let module_names: Vec<&str> = modules.iter().map(|m| m.module.as_str()).collect();
        assert_eq!(
            module_names,
            vec![
                "multialt.defaults.runner",
                "multialt.defaults.focus",
                "multialt.defaults.vision",
                "multialt.defaults.actions",
                "multialt.defaults.dashboard",
                "lib.helpers",
                "lib.menus.shop",
            ]
        );
        let get = |m: &str| modules.iter().find(|x| x.module == m).unwrap();
        let actions = get("multialt.defaults.actions");
        assert!(actions.pinned && !actions.modified);
        assert_eq!(actions.path, "multialt/defaults/actions.py");
        let jitter = actions.functions.iter().find(|f| f.name == "jitter_sleep").unwrap();
        assert_eq!(jitter.signature, "jitter_sleep(base_ms: int, spread_ms: int = 0)");
        assert!(actions.functions.iter().any(|f| f.name == "press"));
        let dashboard = get("multialt.defaults.dashboard");
        assert!(dashboard.functions.iter().any(|f| f.name == "farm_report"));
        assert!(dashboard.functions.iter().all(|f| !f.name.starts_with('_')));
        let vision = get("multialt.defaults.vision");
        assert!(vision.modified);
        assert_eq!(vision.functions[0].name, "my_pixel");
        let helpers = get("lib.helpers");
        assert!(!helpers.pinned && !helpers.modified && !helpers.new_default_available);
        assert_eq!(helpers.path, "lib/helpers.py");
        assert_eq!(helpers.functions[0].signature, "walk_forward(account: Account, seconds: float = 1.0)");
        assert_eq!(helpers.functions[0].doc, "Hold W for ``seconds`` so the character walks forward.");
        assert_eq!(get("lib.menus.shop").functions[0].doc, "Open it.");

        // `lib/__init__.py` com função aparece como `lib`.
        std::fs::write(ws.join("lib/__init__.py"), "def top():\n    pass\n").unwrap();
        let modules = list_global_functions(&ws).unwrap();
        assert!(modules.iter().any(|m| m.module == "lib" && m.path == "lib/__init__.py"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn modules_reach_the_frontend_in_camel_case() {
        let v = serde_json::to_value(GlobalModule {
            path: "lib/a.py".into(),
            module: "lib.a".into(),
            pinned: false,
            modified: false,
            new_default_available: false,
            functions: vec![GlobalFunction { name: "f".into(), signature: "f()".into(), doc: String::new() }],
        })
        .unwrap();
        assert!(v.get("newDefaultAvailable").is_some());
        assert_eq!(v["functions"][0]["signature"], serde_json::json!("f()"));
    }
}
