//! Linhas do Console (evento `launch-log`) que a tela traduz.
//!
//! O backend **nunca** escreve a frase já num idioma: manda a frase em inglês
//! com `{{parametros}}` (a `key`, que é também a chave do catálogo
//! `src/locales/<idioma>/common.json`) e os valores (`params`). A tela faz
//! `t(key, params)`; `message` leva a mesma frase em inglês já montada, para
//! quem não traduz (relatório de problema, ouvintes antigos do evento).
//!
//! Texto vindo de fora (erro do Roblox, do Windows, nome de preset) entra como
//! **parâmetro**, nunca dentro da frase: a frase em volta é traduzida, o texto
//! de fora fica como veio. Regras e a trava que as vigia: `docs/features/console.md`.

use serde_json::{Map, Value};

/// Chave da linha que não tem frase própria (texto de fora, sem tradução).
/// A tela mostra o `message` como veio.
const RAW_KEY: &str = "{{text}}";

/// Uma linha do Console: frase em inglês com `{{parametros}}` e os valores.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LogMsg {
    key: &'static str,
    params: Map<String, Value>,
}

impl LogMsg {
    /// `key` é um literal: o teste `console_language_tests` lê o fonte, confere
    /// que ele está no catálogo dos quatro idiomas e que não tem português.
    pub(crate) fn new(key: &'static str) -> Self {
        Self { key, params: Map::new() }
    }

    /// Um valor para o `{{name}}` da frase. Número e texto entram como são;
    /// outro `LogMsg` (ou uma lista deles) é traduzido pela tela antes de
    /// entrar na frase de fora.
    pub(crate) fn with(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.params.insert(name.to_string(), value.into());
        self
    }

    /// Texto que vem de fora e não tem frase para traduzir.
    pub(crate) fn raw(text: impl Into<String>) -> Self {
        Self::new(RAW_KEY).with("text", text.into())
    }

    pub(crate) fn is_raw(&self) -> bool {
        self.key == RAW_KEY
    }

    pub(crate) fn key(&self) -> &'static str {
        self.key
    }

    pub(crate) fn params(&self) -> &Map<String, Value> {
        &self.params
    }

    /// A frase em inglês com os valores no lugar.
    pub(crate) fn render(&self) -> String {
        render_template(self.key, Some(&self.params))
    }
}

impl From<&'static str> for LogMsg {
    fn from(key: &'static str) -> Self {
        Self::new(key)
    }
}

/// Texto solto vira linha crua (sem tradução). Existe para o texto que já vem
/// pronto de fora; frase do app tem de ser `LogMsg::new`/`log_msg!`, e o teste
/// `console_language_tests` reprova `format!`/`String::from` direto numa linha.
impl From<String> for LogMsg {
    fn from(text: String) -> Self {
        Self::raw(text)
    }
}

/// Um `LogMsg` dentro de outro (parâmetro que também é frase).
impl From<LogMsg> for Value {
    fn from(msg: LogMsg) -> Value {
        let mut object = Map::new();
        object.insert("key".into(), Value::String(msg.key.to_string()));
        object.insert("params".into(), Value::Object(msg.params));
        Value::Object(object)
    }
}

/// Monta o `LogMsg` de uma frase literal:
/// `log_msg!("Client started (PID {{pid}})", pid = pid)`.
macro_rules! log_msg {
    ($key:literal $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::log_msg::LogMsg::new($key)$(.with(stringify!($name), $value))*
    };
}

fn render_param(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null => String::new(),
        Value::Array(items) => items.iter().map(render_param).collect::<Vec<_>>().join(", "),
        Value::Object(object) => match object.get("key").and_then(Value::as_str) {
            Some(key) => render_template(key, object.get("params").and_then(Value::as_object)),
            None => String::new(),
        },
    }
}

/// Troca cada `{{name}}` pelo valor. `{{name}}` sem valor fica como está (a
/// tela mostra o buraco e o teste de cada frase o pega).
fn render_template(template: &str, params: Option<&Map<String, Value>>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = after[..end].trim();
        match params.and_then(|p| p.get(name)) {
            Some(value) => out.push_str(&render_param(value)),
            None => out.push_str(&rest[start..start + 2 + end + 2]),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// O payload do evento `launch-log`. `message` é sempre a frase em inglês;
/// `key`/`params` só vão quando a linha tem frase para traduzir.
pub(crate) fn launch_log_payload(user_id: Option<i64>, level: &str, step: &str, msg: &LogMsg) -> Value {
    let mut payload = serde_json::json!({
        "userId": user_id,
        "level": level,
        "step": step,
        "message": msg.render(),
    });
    if !msg.is_raw() {
        payload["key"] = Value::String(msg.key().to_string());
        payload["params"] = Value::Object(msg.params().clone());
    }
    payload
}

#[cfg(test)]
mod log_msg_tests {
    use super::*;

    #[test]
    fn the_english_message_has_the_values_in_place() {
        let msg = log_msg!("Client started (PID {{pid}})", pid = 4242);
        assert_eq!(msg.render(), "Client started (PID 4242)");
        let msg = log_msg!("Waiting {{seconds}}s", seconds = 3u64);
        assert_eq!(msg.render(), "Waiting 3s");
    }

    #[test]
    fn a_nested_sentence_and_a_list_are_rendered_in_place() {
        let inner = log_msg!("AFK mode");
        let other = log_msg!("Auto Rejoin");
        let msg = log_msg!("Keeping the PC awake: {{reasons}}", reasons = vec![inner, other]);
        assert_eq!(msg.render(), "Keeping the PC awake: AFK mode, Auto Rejoin");
        let msg = log_msg!("Failed: {{reason}}", reason = log_msg!("Stopped in the middle"));
        assert_eq!(msg.render(), "Failed: Stopped in the middle");
    }

    #[test]
    fn a_missing_value_leaves_the_hole_visible() {
        assert_eq!(LogMsg::new("Hello {{name}}").render(), "Hello {{name}}");
    }

    #[test]
    fn the_payload_carries_key_and_params_next_to_the_english_message() {
        let msg = log_msg!("Client started (PID {{pid}})", pid = 7);
        let payload = launch_log_payload(Some(42), "success", "pid", &msg);
        assert_eq!(payload["userId"], 42);
        assert_eq!(payload["level"], "success");
        assert_eq!(payload["step"], "pid");
        assert_eq!(payload["message"], "Client started (PID 7)");
        assert_eq!(payload["key"], "Client started (PID {{pid}})");
        assert_eq!(payload["params"]["pid"], 7);
    }

    #[test]
    fn a_line_without_account_has_a_null_user() {
        let payload = launch_log_payload(None, "info", "rejoin", &LogMsg::new("Auto Rejoin stopped"));
        assert!(payload["userId"].is_null());
        assert_eq!(payload["key"], "Auto Rejoin stopped");
    }

    #[test]
    fn raw_text_goes_as_the_message_with_no_key() {
        let payload = launch_log_payload(Some(1), "warn", "version", &LogMsg::from(String::from("Roblox said no")));
        assert_eq!(payload["message"], "Roblox said no");
        assert!(payload.get("key").is_none());
        assert!(payload.get("params").is_none());
    }
}

/// A trava da língua do Console: nenhuma linha que o backend manda ao Console
/// (`emit_launch_log` / `emit_session_log`) pode ter português, e toda frase
/// precisa estar traduzida nos quatro idiomas do catálogo
/// (`src/locales/<idioma>/common.json`). Lê o próprio fonte (como
/// `watcher_console_tests`), então cobre também o ramo de macOS, que nem
/// compila nesta plataforma. Ver `docs/features/console.md`.
#[cfg(test)]
mod console_language_tests {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    // ── leitor mínimo de Rust: sem comentário, sem item de teste ─────────────────

    fn is_ident(c: char) -> bool {
        c.is_alphanumeric() || c == '_'
    }

    /// Se há um literal (string, string crua ou caractere) começando em `i`,
    /// o índice logo depois dele.
    fn skip_literal(cs: &[char], i: usize) -> Option<usize> {
        match cs[i] {
            '"' => {
                let mut j = i + 1;
                while j < cs.len() {
                    match cs[j] {
                        '\\' => j += 2,
                        '"' => return Some(j + 1),
                        _ => j += 1,
                    }
                }
                Some(cs.len())
            }
            '\'' => {
                if cs.get(i + 1) == Some(&'\\') {
                    let mut j = i + 3;
                    while j < cs.len() && cs[j] != '\'' {
                        j += 1;
                    }
                    Some((j + 1).min(cs.len()))
                } else if cs.get(i + 2) == Some(&'\'') {
                    Some(i + 3)
                } else {
                    None
                }
            }
            'r' if i == 0 || !is_ident(cs[i - 1]) => {
                let mut j = i + 1;
                let mut hashes = 0;
                while cs.get(j) == Some(&'#') {
                    hashes += 1;
                    j += 1;
                }
                if cs.get(j) != Some(&'"') {
                    return None;
                }
                j += 1;
                while j < cs.len() {
                    if cs[j] == '"' && (0..hashes).all(|k| cs.get(j + 1 + k) == Some(&'#')) {
                        return Some(j + 1 + hashes);
                    }
                    j += 1;
                }
                Some(cs.len())
            }
            _ => None,
        }
    }

    /// O fonte sem comentários (as quebras de linha ficam).
    fn without_comments(src: &str) -> Vec<char> {
        let cs: Vec<char> = src.chars().collect();
        let mut out = Vec::with_capacity(cs.len());
        let mut i = 0;
        while i < cs.len() {
            if cs[i] == '/' && cs.get(i + 1) == Some(&'/') {
                while i < cs.len() && cs[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            if cs[i] == '/' && cs.get(i + 1) == Some(&'*') {
                i += 2;
                while i + 1 < cs.len() && !(cs[i] == '*' && cs[i + 1] == '/') {
                    if cs[i] == '\n' {
                        out.push('\n');
                    }
                    i += 1;
                }
                i = (i + 2).min(cs.len());
                continue;
            }
            if let Some(end) = skip_literal(&cs, i) {
                out.extend_from_slice(&cs[i..end]);
                i = end;
                continue;
            }
            out.push(cs[i]);
            i += 1;
        }
        out
    }

    /// O índice do `close` que fecha o `open` em `i` (strings de fora).
    fn matching(cs: &[char], i: usize, open: char, close: char) -> usize {
        let mut depth = 0usize;
        let mut j = i;
        while j < cs.len() {
            if let Some(end) = skip_literal(cs, j) {
                j = end;
                continue;
            }
            if cs[j] == open {
                depth += 1;
            } else if cs[j] == close {
                depth -= 1;
                if depth == 0 {
                    return j;
                }
            }
            j += 1;
        }
        cs.len()
    }

    fn starts_with_at(cs: &[char], i: usize, needle: &str) -> bool {
        needle.chars().enumerate().all(|(k, c)| cs.get(i + k) == Some(&c))
    }

    /// O código de produção: tira cada item marcado `#[cfg(test)]` (ou
    /// `#[cfg(all(test, ...))]`) — módulo, função ou `use`.
    fn production_code(src: &str) -> Vec<char> {
        let cs = without_comments(src);
        let mut out: Vec<char> = Vec::with_capacity(cs.len());
        let mut i = 0;
        while i < cs.len() {
            if starts_with_at(&cs, i, "#[cfg(test)]") || starts_with_at(&cs, i, "#[cfg(all(test") {
                let mut j = i;
                while j < cs.len() && cs[j] != ']' {
                    j += 1;
                }
                j += 1;
                while j < cs.len() && cs[j] != '{' && cs[j] != ';' {
                    j = skip_literal(&cs, j).unwrap_or(j + 1);
                }
                if j < cs.len() && cs[j] == '{' {
                    j = matching(&cs, j, '{', '}');
                }
                i = j + 1;
                continue;
            }
            out.push(cs[i]);
            i += 1;
        }
        out
    }

    /// Divide `cs[from..to]` nas vírgulas de fora de qualquer par de parênteses.
    fn split_args(cs: &[char], from: usize, to: usize) -> Vec<String> {
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut depth = 0i32;
        let mut j = from;
        while j < to {
            if let Some(end) = skip_literal(cs, j) {
                current.extend(cs[j..end.min(to)].iter());
                j = end;
                continue;
            }
            match cs[j] {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                ',' if depth == 0 => {
                    parts.push(std::mem::take(&mut current));
                    j += 1;
                    continue;
                }
                _ => {}
            }
            current.push(cs[j]);
            j += 1;
        }
        if !current.trim().is_empty() {
            parts.push(current);
        }
        parts
    }

    /// O conteúdo de um literal de string simples, ou `None`.
    fn string_literal(text: &str) -> Option<String> {
        let t = text.trim();
        let inner = t.strip_prefix('"')?.strip_suffix('"')?;
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => match chars.next()? {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    other => out.push(other),
                },
                '"' => return None, // aspas no meio: não é um literal só
                other => out.push(other),
            }
        }
        Some(out)
    }

    #[derive(Debug)]
    struct Phrase {
        file: String,
        key: String,
        /// Os nomes dos argumentos do `log_msg!`; `None` = literal solto.
        args: Option<BTreeSet<String>>,
    }

    /// Chamada de `emit_*` cuja frase foi montada à mão (`format!`, `String::from`):
    /// vai crua, sem passar pela tradução.
    #[derive(Debug)]
    struct Offender {
        file: String,
        call: String,
    }

    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read src") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    /// Frase em inglês montada à mão que o app ainda manda crua porque o bloco que
    /// a escreve é de outro trabalho (o aviso da versão velha do Roblox, em
    /// `resolve_launch_install`). É inglês, nunca português; sai desta lista
    /// quando virar `log_msg!`.
    const RAW_ENGLISH_ALLOWED: &[&str] = &[];

    /// Acha `needle` a partir de `from` (índice de caractere).
    fn find_from(cs: &[char], from: usize, needle: &str) -> Option<usize> {
        (from..cs.len()).find(|&i| starts_with_at(cs, i, needle))
    }

    fn scan() -> (Vec<Phrase>, Vec<Offender>) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files(&root, &mut files);
        files.sort();
        let mut phrases = Vec::new();
        let mut offenders = Vec::new();
        for path in files {
            let name = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
            let src = std::fs::read_to_string(&path).expect("read file");
            let cs = production_code(&src);

            // log_msg!("frase", nome = valor, ...)
            let mut at = 0;
            while let Some(start) = find_from(&cs, at, "log_msg!(") {
                at = start + 9;
                if start > 0 && is_ident(cs[start - 1]) {
                    continue;
                }
                let open = start + 8;
                let close = matching(&cs, open, '(', ')');
                let parts = split_args(&cs, open + 1, close);
                let Some(key) = parts.first().and_then(|p| string_literal(p)) else {
                    offenders.push(Offender { file: name.clone(), call: format!("log_msg!({})", parts.join(",")) });
                    continue;
                };
                let mut args = BTreeSet::new();
                for part in parts.iter().skip(1) {
                    if let Some((arg, _)) = part.split_once('=') {
                        args.insert(arg.trim().to_string());
                    }
                }
                phrases.push(Phrase { file: name.clone(), key, args: Some(args) });
            }

            // emit_launch_log(app, uid, "nível", "etapa", frase) — a frase é o último argumento
            for call in ["emit_launch_log(", "emit_session_log("] {
                let mut at = 0;
                while let Some(start) = find_from(&cs, at, call) {
                    at = start + call.len();
                    let before: String = cs[start.saturating_sub(3)..start].iter().collect();
                    if before.ends_with("fn ") {
                        continue;
                    }
                    if start > 0 && is_ident(cs[start - 1]) {
                        continue;
                    }
                    let open = start + call.len() - 1;
                    let close = matching(&cs, open, '(', ')');
                    let parts = split_args(&cs, open + 1, close);
                    let Some(last) = parts.last().map(|p| p.trim().to_string()) else { continue };
                    if let Some(key) = string_literal(&last) {
                        phrases.push(Phrase { file: name.clone(), key, args: None });
                    } else if !last.starts_with("log_msg!(")
                    && (last.contains("format!(")
                        || last.starts_with("String::from(")
                        || last.ends_with(".to_string()")
                        || last.ends_with(".into()"))
                        && !RAW_ENGLISH_ALLOWED.iter().any(|ok| last.contains(ok))
                    {
                        offenders.push(Offender { file: name.clone(), call: last });
                    }
                }
            }
        }
        (phrases, offenders)
    }

    /// Palavras do português sem acento (os acentos são barrados à parte). Só as
    /// que não existem em inglês: "com", "do", "no" ficam de fora.
    const PORTUGUESE_WORDS: &[&str] = &[
        "nao", "conta", "contas", "cliente", "clientes", "fechou", "fechado", "fechada", "aberto", "abriu", "tentativa",
        "tentativas", "sessao", "servidor", "aguardando", "esperando", "iniciando", "iniciado", "falha", "entrou",
        "voltou", "reconexao", "gravacao", "tocando", "caiu", "expulso", "jogo", "pelo", "pela", "para", "uma", "sem",
        "foi", "nova", "novo", "antes", "depois", "quando", "fila", "tela", "memoria", "minimo", "limite", "acima",
        "abaixo", "liberar", "pedi", "recusou", "verificacao", "dispensada", "desistiu", "parada", "parado", "conexao",
        "usuario", "reinicio", "reiniciando",
    ];

    fn portuguese_in(key: &str) -> Option<String> {
        if let Some(c) = key.chars().find(|c| !c.is_ascii() && *c != '—' && *c != '…') {
            return Some(format!("caractere {c:?}"));
        }
        key.split(|c: char| !c.is_alphabetic())
            .map(|w| w.to_lowercase())
            .find(|w| PORTUGUESE_WORDS.contains(&w.as_str()))
            .map(|w| format!("palavra {w:?}"))
    }

    fn placeholders(key: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut rest = key;
        while let Some(start) = rest.find("{{") {
            let after = &rest[start + 2..];
            let Some(end) = after.find("}}") else { break };
            out.insert(after[..end].trim().to_string());
            rest = &after[end + 2..];
        }
        out
    }

    fn catalog(lang: &str) -> serde_json::Map<String, serde_json::Value> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("src")
            .join("locales")
            .join(lang)
            .join("common.json");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        serde_json::from_str::<serde_json::Value>(&text)
            .expect("json")
            .as_object()
            .expect("object")
            .clone()
    }

    #[test]
    fn the_scan_actually_sees_the_console_lines() {
        // Protege contra o leitor morto: se ele não vê nada, nenhum teste abaixo vale.
        let (phrases, _) = scan();
        assert!(phrases.len() > 60, "só vi {} frases", phrases.len());
        assert!(phrases.iter().any(|p| p.args.is_none()), "nenhuma frase literal solta");
        assert!(
            phrases.iter().any(|p| p.args.as_ref().is_some_and(|a| !a.is_empty())),
            "nenhum log_msg! com parâmetro"
        );
    }

    #[test]
    fn no_console_line_is_in_portuguese() {
        let (phrases, _) = scan();
        let bad: Vec<String> = phrases
            .iter()
            .filter_map(|p| portuguese_in(&p.key).map(|why| format!("{}: {:?} ({why})", p.file, p.key)))
            .collect();
        assert!(
            bad.is_empty(),
            "português no Console (escreva em inglês e traduza no catálogo):\n{}",
            bad.join("\n")
        );
    }

    #[test]
    fn no_console_line_is_assembled_by_hand() {
        let (_, offenders) = scan();
        let bad: Vec<String> = offenders.iter().map(|o| format!("{}: {}", o.file, o.call)).collect();
        assert!(
            bad.is_empty(),
            "frase montada à mão vai crua, sem tradução; use log_msg!(\"... {{{{valor}}}} ...\", valor = x):\n{}",
            bad.join("\n")
        );
    }

    #[test]
    fn every_placeholder_of_a_phrase_gets_a_value_and_every_value_has_a_place() {
        let (phrases, _) = scan();
        let mut bad = Vec::new();
        for p in &phrases {
            let holes = placeholders(&p.key);
            let given = p.args.clone().unwrap_or_default();
            if holes != given {
                bad.push(format!("{}: {:?} tem {holes:?} mas recebe {given:?}", p.file, p.key));
            }
        }
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }

    #[test]
    fn every_console_phrase_is_translated_in_all_four_languages() {
        let (phrases, _) = scan();
        let en = catalog("en");
        let others = [("pt", catalog("pt")), ("es", catalog("es")), ("de", catalog("de"))];
        let mut missing = Vec::new();
        for p in &phrases {
            if en.get(&p.key).and_then(|v| v.as_str()) != Some(p.key.as_str()) {
                missing.push(format!("en: {:?}", p.key));
            }
            for (lang, dict) in &others {
                if !dict.get(&p.key).and_then(|v| v.as_str()).is_some_and(|v| !v.trim().is_empty()) {
                    missing.push(format!("{lang}: {:?}", p.key));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "frase do Console fora do catálogo (src/locales/<idioma>/common.json):\n{}",
            missing.join("\n")
        );
    }

    /// `cargo test --all-features print_console_phrases -- --ignored --nocapture`
    /// lista as frases (uma por linha, com `PHRASE` na frente) para montar o catálogo.
    #[test]
    #[ignore]
    fn print_console_phrases() {
        let (phrases, _) = scan();
        let keys: BTreeSet<String> = phrases.into_iter().map(|p| p.key).collect();
        for key in keys {
            println!("PHRASE\t{key}");
        }
    }
}
