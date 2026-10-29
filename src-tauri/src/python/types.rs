// Tipos que a tela recebe dos comandos `python_*` (o contrato do plano em
// docs/superpowers/plans/2026-10-21-python-automation.md). O frontend os
// espelha em `src/components/python/pythonApi.ts`: nome de campo aqui muda lá.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PythonRuntimeStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub python_exe: Option<String>,
    pub packages: Vec<String>,
    pub installing: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PythonInstallProgress {
    /// "python" | "pip" | "packages" | "ready" | "error"
    pub stage: String,
    pub message: String,
    pub downloaded: u64,
    pub total: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PythonWorkspaceInfo {
    pub root: String,
    pub scripts_dir: String,
    pub assets_dir: String,
    pub python_exe: Option<String>,
    pub cli_command: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PythonScriptFile {
    pub name: String,
    pub size: u64,
    pub modified_ms: u64,
}

/// Um nó do Explorer da aba Code (`python_fs_tree`).
#[derive(Serialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct FsNode {
    /// Relativo à pasta de trabalho, com `/`; `""` é a raiz.
    pub path: String,
    pub name: String,
    /// "file" | "dir"
    pub kind: String,
    pub size: u64,
    pub modified_ms: u64,
    pub writable: bool,
    /// Os blocos padrão (e as pastas deles): com cadeado.
    pub pinned: bool,
    /// `scripts/<nome>.py` do topo (o Run só roda esses).
    pub runnable: bool,
    pub children: Option<Vec<FsNode>>,
}

/// Uma função de nível de topo de um módulo da aba Global functions.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GlobalFunction {
    pub name: String,
    /// `walk_forward(account, seconds=1.0)`
    pub signature: String,
    /// A primeira linha da docstring (vazio sem docstring).
    pub doc: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GlobalModule {
    /// `lib/helpers.py`, `multialt/defaults/vision.py`
    pub path: String,
    /// `lib.helpers`, `multialt.defaults.vision`
    pub module: String,
    pub pinned: bool,
    pub modified: bool,
    pub new_default_available: bool,
    pub functions: Vec<GlobalFunction>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DefaultBlockInfo {
    pub name: String,
    pub modified: bool,
    pub new_default_available: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PythonTrigger {
    Manual,
    Interval { minutes: u32 },
    Loop,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PythonRunMode {
    OneAtATime,
    AllTogether,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PythonRunConfig {
    pub script: String,
    pub accounts: Vec<i64>,
    pub mode: PythonRunMode,
    pub trigger: PythonTrigger,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PythonRunInfo {
    pub id: String,
    pub script: String,
    pub accounts: Vec<i64>,
    pub mode: PythonRunMode,
    pub trigger: PythonTrigger,
    /// "app" | "terminal"
    pub source: String,
    /// "starting" | "running" | "waiting" | "stopping" | "finished" | "failed"
    pub state: String,
    pub started_ms: u64,
    pub ended_ms: Option<u64>,
    pub next_run_ms: Option<u64>,
    pub error: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PythonLogLine {
    pub run_id: Option<String>,
    pub user_id: Option<i64>,
    /// "log" | "warn" | "error" | "system" | "blocked"
    pub level: String,
    pub message: String,
    pub code: Option<String>,
    pub hint: Option<String>,
    pub ts: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSection {
    pub script: String,
    pub layout: Vec<serde_json::Value>,
    /// "id" ou "id@userId" -> valor
    pub values: serde_json::Map<String, serde_json::Value>,
    /// id -> [[ts, v]...], no máximo 500
    pub series: serde_json::Map<String, serde_json::Value>,
    pub updated_ms: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InspectorFrame {
    pub width: u32,
    pub height: u32,
    pub png_base64: String,
    /// O quadro que a tela mostra: o `python_save_asset` recorta dele.
    pub frame_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FracRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// O que um script enxerga de uma conta (`Account` do protocolo). **Nunca**
/// cookie nem senha: é só isto que sai do app para o Python.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AccountView {
    pub user_id: i64,
    pub name: String,
    pub alias: String,
    pub group: String,
    pub in_game: bool,
    pub place_id: i64,
    pub job_id: String,
}

impl PythonRunMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            PythonRunMode::OneAtATime => "one_at_a_time",
            PythonRunMode::AllTogether => "all_together",
        }
    }
}

/// Mensagem de log maior que isto (bytes) é cortada: um `print` de 1 MB não
/// vai inteiro para o log nem para a tela.
pub const MAX_LOG_MESSAGE: usize = 8 * 1024;

pub fn clamp_log_message(message: String) -> String {
    if message.len() <= MAX_LOG_MESSAGE {
        return message;
    }
    let mut end = MAX_LOG_MESSAGE;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… (truncated, {} KB in total)", &message[..end], message.len() / 1024)
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod python_types_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn trigger_and_mode_serialize_as_the_frontend_expects() {
        assert_eq!(
            serde_json::to_value(PythonTrigger::Interval { minutes: 5 }).unwrap(),
            json!({"kind": "interval", "minutes": 5})
        );
        assert_eq!(serde_json::to_value(PythonTrigger::Manual).unwrap(), json!({"kind": "manual"}));
        assert_eq!(serde_json::to_value(PythonTrigger::Loop).unwrap(), json!({"kind": "loop"}));
        assert_eq!(
            serde_json::to_value(PythonRunMode::OneAtATime).unwrap(),
            json!("one_at_a_time")
        );
        assert_eq!(
            serde_json::to_value(PythonRunMode::AllTogether).unwrap(),
            json!("all_together")
        );
        let config: PythonRunConfig = serde_json::from_value(json!({
            "script": "farm.py",
            "accounts": [1, 2],
            "mode": "all_together",
            "trigger": {"kind": "interval", "minutes": 3}
        }))
        .unwrap();
        assert_eq!(config.trigger, PythonTrigger::Interval { minutes: 3 });
        assert_eq!(config.mode.as_str(), "all_together");
    }

    #[test]
    fn records_reach_the_frontend_in_camel_case() {
        let line = PythonLogLine {
            run_id: Some("r1".into()),
            user_id: Some(7),
            level: "blocked".into(),
            message: "m".into(),
            code: Some("not_foreground".into()),
            hint: None,
            ts: 1,
        };
        let v = serde_json::to_value(line).unwrap();
        assert!(v.get("runId").is_some() && v.get("userId").is_some());
        let status = serde_json::to_value(PythonRuntimeStatus::default()).unwrap();
        assert!(status.get("pythonExe").is_some());
        let frame = serde_json::to_value(InspectorFrame { width: 1, height: 1, png_base64: String::new(), frame_id: 3 }).unwrap();
        assert!(frame.get("pngBase64").is_some());
        assert_eq!(frame["frameId"], json!(3));
    }

    #[test]
    fn the_account_view_has_no_secret_fields() {
        let v = serde_json::to_value(AccountView {
            user_id: 1,
            name: "Main_01".into(),
            alias: String::new(),
            group: "Default".into(),
            in_game: true,
            place_id: 0,
            job_id: String::new(),
        })
        .unwrap();
        let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
        assert_eq!(keys.len(), 7);
        for k in keys {
            let k = k.to_ascii_lowercase();
            assert!(!k.contains("cookie") && !k.contains("password") && !k.contains("token"), "{k}");
        }
    }
}
