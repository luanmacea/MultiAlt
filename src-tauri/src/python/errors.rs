// Os erros que as primitivas devolvem ao Python. Código, mensagem e dica são
// contrato (o plano, "Error codes"): o SDK transforma cada código numa
// subclasse de `MultiAltError`, e a tela destaca as travas com a dica.

use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PyErrorCode {
    NotForeground,
    NoFocusLease,
    UntrackedWindow,
    Minimized,
    Forbidden,
    FocusTimeout,
    UnsupportedKey,
    BadArgs,
    CaptureFailed,
    Stopped,
}

impl PyErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            PyErrorCode::NotForeground => "not_foreground",
            PyErrorCode::NoFocusLease => "no_focus_lease",
            PyErrorCode::UntrackedWindow => "untracked_window",
            PyErrorCode::Minimized => "minimized",
            PyErrorCode::Forbidden => "forbidden",
            PyErrorCode::FocusTimeout => "focus_timeout",
            PyErrorCode::UnsupportedKey => "unsupported_key",
            PyErrorCode::BadArgs => "bad_args",
            PyErrorCode::CaptureFailed => "capture_failed",
            PyErrorCode::Stopped => "stopped",
        }
    }

    /// As travas de segurança (entrada ou captura recusada): vão para o log com
    /// nível `blocked`, em destaque com a dica.
    pub fn is_block(&self) -> bool {
        matches!(
            self,
            PyErrorCode::NotForeground
                | PyErrorCode::NoFocusLease
                | PyErrorCode::UntrackedWindow
                | PyErrorCode::Minimized
                | PyErrorCode::Forbidden
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PyError {
    pub code: PyErrorCode,
    pub message: String,
    pub hint: Option<String>,
}

impl PyError {
    fn new(code: PyErrorCode, message: String, hint: Option<&str>) -> Self {
        Self {
            code,
            message,
            hint: hint.map(str::to_string),
        }
    }

    pub fn not_foreground(name: &str) -> Self {
        Self::new(
            PyErrorCode::NotForeground,
            format!("Blocked: {name}'s window is not in front. Input only goes to the focused window."),
            Some("Wrap input in \"with account.focused():\"."),
        )
    }

    pub fn no_focus_lease() -> Self {
        Self::new(
            PyErrorCode::NoFocusLease,
            "Blocked: take the focus queue first, so other runs, AFK mode and recordings don't type into the same window.".into(),
            Some("Use \"with account.focused():\"."),
        )
    }

    pub fn untracked_window(name: &str) -> Self {
        Self::new(
            PyErrorCode::UntrackedWindow,
            format!("Blocked: {name} has no Roblox client opened by MultiAlt."),
            Some("Launch the account from MultiAlt first."),
        )
    }

    pub fn minimized(name: &str) -> Self {
        Self::new(
            PyErrorCode::Minimized,
            format!("{name}'s window is minimized."),
            Some("\"account.focused()\" restores it before input."),
        )
    }

    pub fn forbidden() -> Self {
        Self::new(
            PyErrorCode::Forbidden,
            "Not available to scripts: cookies and passwords never leave MultiAlt.".into(),
            None,
        )
    }

    pub fn focus_timeout(ms: u64) -> Self {
        Self::new(
            PyErrorCode::FocusTimeout,
            format!("Waited {ms} ms for the focus queue (AFK mode or a recording may be running)."),
            Some("Raise timeout_ms or try again."),
        )
    }

    pub fn unsupported_key(key: &str) -> Self {
        Self::new(
            PyErrorCode::UnsupportedKey,
            format!("Key \"{key}\" is not supported."),
            Some("See docs/API.md, \"Keys\"."),
        )
    }

    pub fn bad_args(op: &str, detail: &str) -> Self {
        Self::new(
            PyErrorCode::BadArgs,
            format!("Invalid arguments for {op}: {detail}"),
            None,
        )
    }

    pub fn capture_failed(name: &str, detail: &str) -> Self {
        Self::new(
            PyErrorCode::CaptureFailed,
            format!("Could not capture {name}'s window ({detail})."),
            Some("Make sure the window is not minimized."),
        )
    }

    pub fn stopped() -> Self {
        Self::new(PyErrorCode::Stopped, "Run stopped.".into(), None)
    }

    /// `{"code","message","hint"}` — `hint` é `null` quando o código não tem.
    pub fn to_json(&self) -> Value {
        json!({
            "code": self.code.as_str(),
            "message": self.message,
            "hint": self.hint,
        })
    }
}

impl std::fmt::Display for PyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

#[cfg(test)]
mod python_errors_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn messages_match_the_contract() {
        let n = "Main_01";
        let cases: Vec<(PyError, &str, &str, Option<&str>)> = vec![
            (
                PyError::not_foreground(n),
                "not_foreground",
                "Blocked: Main_01's window is not in front. Input only goes to the focused window.",
                Some("Wrap input in \"with account.focused():\"."),
            ),
            (
                PyError::no_focus_lease(),
                "no_focus_lease",
                "Blocked: take the focus queue first, so other runs, AFK mode and recordings don't type into the same window.",
                Some("Use \"with account.focused():\"."),
            ),
            (
                PyError::untracked_window(n),
                "untracked_window",
                "Blocked: Main_01 has no Roblox client opened by MultiAlt.",
                Some("Launch the account from MultiAlt first."),
            ),
            (
                PyError::minimized(n),
                "minimized",
                "Main_01's window is minimized.",
                Some("\"account.focused()\" restores it before input."),
            ),
            (
                PyError::forbidden(),
                "forbidden",
                "Not available to scripts: cookies and passwords never leave MultiAlt.",
                None,
            ),
            (
                PyError::focus_timeout(20000),
                "focus_timeout",
                "Waited 20000 ms for the focus queue (AFK mode or a recording may be running).",
                Some("Raise timeout_ms or try again."),
            ),
            (
                PyError::unsupported_key("F4"),
                "unsupported_key",
                "Key \"F4\" is not supported.",
                Some("See docs/API.md, \"Keys\"."),
            ),
            (
                PyError::bad_args("input.click", "x must be between 0 and 1"),
                "bad_args",
                "Invalid arguments for input.click: x must be between 0 and 1",
                None,
            ),
            (
                PyError::capture_failed(n, "PrintWindow failed"),
                "capture_failed",
                "Could not capture Main_01's window (PrintWindow failed).",
                Some("Make sure the window is not minimized."),
            ),
            (PyError::stopped(), "stopped", "Run stopped.", None),
        ];
        for (error, code, message, hint) in cases {
            assert_eq!(
                error.to_json(),
                json!({"code": code, "message": message, "hint": hint}),
                "{code}"
            );
        }
    }

    #[test]
    fn only_the_safety_gates_are_logged_as_blocked() {
        assert!(PyErrorCode::NotForeground.is_block());
        assert!(PyErrorCode::Forbidden.is_block());
        assert!(!PyErrorCode::Stopped.is_block());
        assert!(!PyErrorCode::BadArgs.is_block());
    }
}
