// O estado vivo da automação Python no app: o gerente de execuções, o
// dashboard e o log, ligados aos eventos do Tauri (`python-log`,
// `python-run-update`, `python-dashboard-update`). Nasce no primeiro comando
// que precisa dele.

use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;
use tauri::Emitter;

use super::dashboard::DashboardStore;
use super::errors::PyError;
use super::focus::FocusQueue;
use super::primitives::RealDesk;
use super::runs::{LogBatch, LogBook, RunEvents, RunManager};
use super::types::{AccountView, PythonLogLine, PythonRunInfo};

pub struct Service {
    pub manager: Arc<RunManager>,
    pub dashboard: Arc<DashboardStore>,
    pub logs: Arc<LogBook>,
    /// O que ainda vai para a tela no próximo tique (`python-log` com array).
    pub log_batch: Arc<LogBatch>,
    pub app: tauri::AppHandle,
}

static SERVICE: OnceLock<Service> = OnceLock::new();

/// Tique da entrega à tela: no máximo 10 eventos `python-log` por segundo.
const LOG_FLUSH_EVERY: Duration = Duration::from_millis(100);

struct AppEvents {
    app: tauri::AppHandle,
    dashboard: Arc<DashboardStore>,
    logs: Arc<LogBook>,
    log_batch: Arc<LogBatch>,
}

impl RunEvents for AppEvents {
    fn log(&self, line: PythonLogLine) {
        self.logs.push(line.clone());
        self.log_batch.push(line);
    }
    fn run_update(&self, info: &PythonRunInfo) {
        let _ = self.app.emit("python-run-update", info);
    }
    fn dashboard(&self, script: &str, op: &str, args: &Value) -> Result<Value, PyError> {
        self.dashboard.apply(script, op, args, super::types::now_ms())
    }
}

pub fn existing() -> Option<&'static Service> {
    SERVICE.get()
}

pub fn service(app: &tauri::AppHandle) -> &'static Service {
    SERVICE.get_or_init(|| {
        let root = crate::data::settings::get_python_workspace_dir();
        let dashboard = Arc::new(DashboardStore::new(Some(root.join("dashboard.json"))));
        let logs = Arc::new(LogBook::new());
        let log_batch = Arc::new(LogBatch::default());
        let events = Arc::new(AppEvents {
            app: app.clone(),
            dashboard: dashboard.clone(),
            logs: logs.clone(),
            log_batch: log_batch.clone(),
        });
        let manager = Arc::new(RunManager::new(events, Arc::new(RealDesk), FocusQueue::global()));
        start_ticker(app.clone(), dashboard.clone(), log_batch.clone());
        Service {
            manager,
            dashboard,
            logs,
            log_batch,
            app: app.clone(),
        }
    })
}

/// A cada 100 ms: manda à tela as linhas novas do log num evento só, entrega
/// o dashboard no máximo 4 vezes por segundo por script e grava o arquivo no
/// máximo uma vez por segundo.
fn start_ticker(app: tauri::AppHandle, dashboard: Arc<DashboardStore>, log_batch: Arc<LogBatch>) {
    tauri::async_runtime::spawn(async move {
        let mut last_save = Instant::now();
        loop {
            tokio::time::sleep(LOG_FLUSH_EVERY).await;
            let lines = log_batch.take();
            if !lines.is_empty() {
                let _ = app.emit("python-log", lines);
            }
            for section in dashboard.take_due_emits(Instant::now()) {
                let _ = app.emit("python-dashboard-update", section);
            }
            if last_save.elapsed() >= Duration::from_secs(1) {
                last_save = Instant::now();
                let _ = dashboard.save_if_dirty();
            }
        }
    });
}

/// Uma linha `system` no log da tela (instalação, modo desenvolvedor...).
pub fn system_log(app: &tauri::AppHandle, run_id: Option<&str>, message: impl Into<String>) {
    let line = PythonLogLine {
        run_id: run_id.map(str::to_string),
        user_id: None,
        level: "system".into(),
        message: super::types::clamp_log_message(message.into()),
        code: None,
        hint: None,
        ts: super::types::now_ms(),
    };
    let svc = service(app);
    svc.logs.push(line.clone());
    svc.log_batch.push(line);
}

/// As contas pedidas, como o script as enxerga (sem cookie nem senha). Conta
/// que não existe mais é recusada com o id.
pub fn account_views(app: &tauri::AppHandle, user_ids: &[i64]) -> Result<Vec<AccountView>, String> {
    use tauri::Manager;
    let all = app.state::<crate::data::accounts::AccountStore>().get_all()?;
    let mut out = Vec::new();
    for id in user_ids {
        let account = all
            .iter()
            .find(|a| a.user_id == *id)
            .ok_or_else(|| format!("Account {id} is not in MultiAlt"))?;
        out.push(view_of(account));
    }
    Ok(out)
}

/// As contas pelo **nome** (ou apelido), para o terminal.
pub fn account_views_by_name(app: &tauri::AppHandle, names: &[String]) -> Result<Vec<AccountView>, String> {
    use tauri::Manager;
    let all = app.state::<crate::data::accounts::AccountStore>().get_all()?;
    let mut out = Vec::new();
    let mut unknown = Vec::new();
    for name in names {
        match all.iter().find(|a| a.username.eq_ignore_ascii_case(name))
            .or_else(|| all.iter().find(|a| !a.alias.is_empty() && a.alias.eq_ignore_ascii_case(name)))
        {
            Some(account) => out.push(view_of(account)),
            None => unknown.push(name.clone()),
        }
    }
    if !unknown.is_empty() {
        return Err(format!("Unknown account(s): {}", unknown.join(", ")));
    }
    Ok(out)
}

fn view_of(account: &crate::data::accounts::Account) -> AccountView {
    let base = AccountView {
        user_id: account.user_id,
        name: account.username.clone(),
        alias: account.alias.clone(),
        group: account.group.clone(),
        in_game: false,
        place_id: 0,
        job_id: String::new(),
    };
    super::primitives::live_view(&RealDesk, &base)
}

/// Garante a pasta de trabalho e o `._pth` antes de uma execução.
pub fn prepare_workspace(app: &tauri::AppHandle, root: &Path) -> Result<(), String> {
    super::workspace::sync_workspace(root)?;
    super::runtime::ensure_workspace_in_pth(&super::runtime::runtime_dir(app), root)
}
