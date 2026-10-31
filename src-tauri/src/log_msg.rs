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
