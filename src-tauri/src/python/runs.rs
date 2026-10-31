// As execuções (docs/features/python-automation.md, "Execuções").
//
// Uma execução = script + contas + modo + gatilho, **um processo Python** por
// vez (o modo "todas juntas" são threads dentro do runner.py). Várias execuções
// convivem; a entrada delas se reveza pela fila de foco. Parar manda o evento
// `stop`, espera 3 s e então mata **o processo Python** (e só a árvore dele,
// por um job object) — nunca o Roblox. O gatilho "a cada N minutos" reabre a
// execução N minutos depois de ela terminar, até o usuário parar.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::future::BoxFuture;
use serde_json::{json, Value};

use super::errors::PyError;
use super::focus::FocusQueue;
use super::primitives::{Desk, RunCtx, RunSink};
use super::types::*;

/// Quantas linhas de log ficam guardadas (a tela mostra as últimas 5 000).
pub const LOG_CAPACITY: usize = 5_000;
/// Execuções terminadas que continuam na lista.
const FINISHED_KEPT: usize = 30;
/// Resposta a quem tenta rodar um script que já está rodando.
pub const ALREADY_RUNNING: &str = "This script is already running. Stop it first.";
/// Espera do Parar antes de matar o processo Python.
pub const STOP_GRACE: Duration = Duration::from_secs(3);

/// Para onde vão os eventos das execuções: a tela (eventos do Tauri) no app,
/// uma lista nos testes.
pub trait RunEvents: Send + Sync {
    fn log(&self, line: PythonLogLine);
    fn run_update(&self, info: &PythonRunInfo);
    fn dashboard(&self, script: &str, op: &str, args: &Value) -> Result<Value, PyError>;
    fn discord(&self, script: &str, user_id: Option<i64>, title: Option<String>, text: String) -> Result<Value, PyError>;
}

/// Como o final de uma rodada aconteceu.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecOutcome {
    pub exit_code: i32,
    /// A última linha do traceback, quando falhou.
    pub error: Option<String>,
}

/// Quem roda uma rodada: o processo Python (app) ou o pipe (terminal).
pub trait Executor: Send + Sync {
    fn execute(&self, ctx: RunCtx, hello: Value) -> BoxFuture<'static, ExecOutcome>;
    /// Execução do terminal: a rodada é única (o laço é do próprio script).
    fn repeatable(&self) -> bool {
        true
    }
}

struct Entry {
    info: PythonRunInfo,
    user_stop: Arc<AtomicBool>,
    exec_stop: Option<Arc<AtomicBool>>,
}

pub struct RunManager {
    runs: Mutex<HashMap<String, Entry>>,
    order: Mutex<Vec<String>>,
    seq: AtomicU64,
    minute_ms: u64,
    events: Arc<dyn RunEvents>,
    desk: Arc<dyn Desk>,
    focus: Arc<FocusQueue>,
}

fn live(state: &str) -> bool {
    matches!(state, "starting" | "running" | "waiting" | "stopping")
}

struct ExecSink {
    script: String,
    events: Arc<dyn RunEvents>,
}

impl RunSink for ExecSink {
    fn log(&self, line: PythonLogLine) {
        self.events.log(line);
    }
    fn status(&self, user_id: Option<i64>, text: &str) {
        // O status vira uma linha `system` do log da conta.
        self.events.log(PythonLogLine {
            run_id: None,
            user_id,
            level: "system".into(),
            message: format!("Status: {text}"),
            code: Some("status".into()),
            hint: None,
            ts: now_ms(),
        });
    }
    fn dashboard(&self, op: &str, args: &Value) -> Result<Value, PyError> {
        self.events.dashboard(&self.script, op, args)
    }
    fn discord(&self, user_id: Option<i64>, title: Option<String>, text: String) -> Result<Value, PyError> {
        self.events.discord(&self.script, user_id, title, text)
    }
}

/// O sink de uma execução marca o id dela em cada linha.
struct RunTagged {
    run_id: String,
    inner: ExecSink,
}

impl RunSink for RunTagged {
    fn log(&self, mut line: PythonLogLine) {
        line.run_id = Some(self.run_id.clone());
        self.inner.log(line);
    }
    fn status(&self, user_id: Option<i64>, text: &str) {
        let mut line = PythonLogLine {
            run_id: Some(self.run_id.clone()),
            user_id,
            level: "system".into(),
            message: format!("Status: {text}"),
            code: Some("status".into()),
            hint: None,
            ts: now_ms(),
        };
        line.run_id = Some(self.run_id.clone());
        self.inner.events.log(line);
    }
    fn dashboard(&self, op: &str, args: &Value) -> Result<Value, PyError> {
        self.inner.dashboard(op, args)
    }
    fn discord(&self, user_id: Option<i64>, title: Option<String>, text: String) -> Result<Value, PyError> {
        self.inner.discord(user_id, title, text)
    }
}

pub fn hello_line(run: &PythonRunInfo, accounts: &[AccountView]) -> Value {
    json!({
        "event": "hello",
        "run": {
            "id": run.id,
            "script": run.script,
            "mode": run.mode,
            "trigger": run.trigger,
            "source": run.source,
        },
        "accounts": accounts,
    })
}

impl RunManager {
    pub fn new(events: Arc<dyn RunEvents>, desk: Arc<dyn Desk>, focus: Arc<FocusQueue>) -> Self {
        Self::with_minute(events, desk, focus, 60_000)
    }

    /// `minute_ms` troca o minuto do gatilho de intervalo (testes).
    pub fn with_minute(events: Arc<dyn RunEvents>, desk: Arc<dyn Desk>, focus: Arc<FocusQueue>, minute_ms: u64) -> Self {
        Self {
            runs: Mutex::new(HashMap::new()),
            order: Mutex::new(Vec::new()),
            seq: AtomicU64::new(0),
            minute_ms,
            events,
            desk,
            focus,
        }
    }

    fn system(&self, run_id: &str, message: impl Into<String>) {
        self.events.log(super::bridge::system_line(run_id, None, "system", message));
    }

    fn update(&self, id: &str, f: impl FnOnce(&mut PythonRunInfo)) -> Option<PythonRunInfo> {
        let info = {
            let mut runs = self.runs.lock().unwrap();
            let entry = runs.get_mut(id)?;
            f(&mut entry.info);
            entry.info.clone()
        };
        self.events.run_update(&info);
        Some(info)
    }

    /// Registra e começa uma execução. Devolve o id. Um script com execução
    /// viva (de qualquer origem) não começa de novo: duas cópias do mesmo
    /// script disputariam as mesmas contas e o mesmo dashboard.
    pub fn start(
        self: &Arc<Self>,
        config: PythonRunConfig,
        accounts: Vec<AccountView>,
        source: &str,
        executor: Arc<dyn Executor>,
    ) -> Result<String, String> {
        let mut runs = self.runs.lock().unwrap();
        if runs.values().any(|e| live(&e.info.state) && e.info.script.eq_ignore_ascii_case(&config.script)) {
            return Err(ALREADY_RUNNING.into());
        }
        let id = format!("r{}", self.seq.fetch_add(1, Ordering::SeqCst) + 1);
        let info = PythonRunInfo {
            id: id.clone(),
            script: config.script.clone(),
            accounts: accounts.iter().map(|a| a.user_id).collect(),
            mode: config.mode.clone(),
            trigger: config.trigger.clone(),
            source: source.to_string(),
            state: "starting".into(),
            started_ms: now_ms(),
            ended_ms: None,
            next_run_ms: None,
            error: None,
        };
        let user_stop = Arc::new(AtomicBool::new(false));
        runs.insert(
            id.clone(),
            Entry {
                info: info.clone(),
                user_stop: user_stop.clone(),
                exec_stop: None,
            },
        );
        drop(runs);
        self.order.lock().unwrap().push(id.clone());
        self.prune();
        self.events.run_update(&info);
        let names: Vec<String> = accounts.iter().map(|a| a.name.clone()).collect();
        self.system(
            &id,
            format!("Run started: {} on {} ({}, from the {})", config.script, names.join(", "), config.mode.as_str(), source),
        );
        let manager = self.clone();
        let run_id = id.clone();
        tokio::spawn(async move {
            manager.run_loop(run_id, config, accounts, user_stop, executor).await;
        });
        Ok(id)
    }

    async fn run_loop(
        self: Arc<Self>,
        id: String,
        config: PythonRunConfig,
        accounts: Vec<AccountView>,
        user_stop: Arc<AtomicBool>,
        executor: Arc<dyn Executor>,
    ) {
        loop {
            let exec_stop = Arc::new(AtomicBool::new(user_stop.load(Ordering::SeqCst)));
            if let Some(entry) = self.runs.lock().unwrap().get_mut(&id) {
                entry.exec_stop = Some(exec_stop.clone());
            }
            if user_stop.load(Ordering::SeqCst) {
                break;
            }
            let Some(info) = self.update(&id, |i| {
                i.state = "running".into();
                i.next_run_ms = None;
                i.error = None;
            }) else {
                return;
            };
            let sink: Arc<dyn RunSink> = Arc::new(RunTagged {
                run_id: id.clone(),
                inner: ExecSink {
                    script: config.script.clone(),
                    events: self.events.clone(),
                },
            });
            let ctx = RunCtx::new(id.clone(), accounts.clone(), exec_stop, sink, self.desk.clone(), self.focus.clone());
            let outcome = executor.execute(ctx, hello_line(&info, &accounts)).await;
            let stopped = user_stop.load(Ordering::SeqCst);
            let failed = outcome.exit_code != 0 && !stopped;
            self.system(
                &id,
                if stopped {
                    "Run stopped.".to_string()
                } else if failed {
                    format!("Python exited with code {}.", outcome.exit_code)
                } else {
                    "Run finished.".to_string()
                },
            );
            let interval = match (&config.trigger, executor.repeatable()) {
                (PythonTrigger::Interval { minutes }, true) if !stopped => Some((*minutes).max(1) as u64),
                _ => None,
            };
            let Some(minutes) = interval else {
                self.update(&id, |i| {
                    i.state = if failed { "failed" } else { "finished" }.into();
                    i.ended_ms = Some(now_ms());
                    i.error = if failed { outcome.error.clone() } else { None };
                });
                return;
            };
            let wait_ms = minutes * self.minute_ms;
            let next = now_ms() + wait_ms;
            self.update(&id, |i| {
                i.state = "waiting".into();
                i.next_run_ms = Some(next);
                i.error = if failed { outcome.error.clone() } else { None };
            });
            let deadline = tokio::time::Instant::now() + Duration::from_millis(wait_ms);
            while tokio::time::Instant::now() < deadline && !user_stop.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(25.min(wait_ms.max(1)))).await;
            }
        }
        self.update(&id, |i| {
            i.state = "finished".into();
            i.ended_ms = Some(now_ms());
            i.next_run_ms = None;
        });
    }

    /// Manda parar (o evento `stop` sai pelo bridge; o processo morre em 3 s se
    /// não sair sozinho).
    pub fn stop(&self, id: &str) -> Result<(), String> {
        let flags = {
            let runs = self.runs.lock().unwrap();
            let entry = runs.get(id).ok_or_else(|| format!("No run {id}"))?;
            if !live(&entry.info.state) {
                return Ok(());
            }
            (entry.user_stop.clone(), entry.exec_stop.clone())
        };
        flags.0.store(true, Ordering::SeqCst);
        if let Some(exec) = flags.1 {
            exec.store(true, Ordering::SeqCst);
        }
        self.update(id, |i| {
            if i.state != "waiting" {
                i.state = "stopping".into();
            }
        });
        Ok(())
    }

    /// Para todas. Devolve quantas estavam vivas.
    pub fn stop_all(&self) -> usize {
        let ids: Vec<String> = self
            .runs
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, e)| live(&e.info.state))
            .map(|(id, _)| id.clone())
            .collect();
        for id in &ids {
            let _ = self.stop(id);
        }
        ids.len()
    }

    pub fn list(&self) -> Vec<PythonRunInfo> {
        let runs = self.runs.lock().unwrap();
        self.order
            .lock()
            .unwrap()
            .iter()
            .filter_map(|id| runs.get(id).map(|e| e.info.clone()))
            .collect()
    }

    pub fn active(&self) -> usize {
        self.runs.lock().unwrap().values().filter(|e| live(&e.info.state)).count()
    }

    fn prune(&self) {
        let mut runs = self.runs.lock().unwrap();
        let mut order = self.order.lock().unwrap();
        let finished: Vec<String> = order
            .iter()
            .filter(|id| runs.get(*id).map(|e| !live(&e.info.state)).unwrap_or(true))
            .cloned()
            .collect();
        let extra = finished.len().saturating_sub(FINISHED_KEPT);
        for id in finished.into_iter().take(extra) {
            runs.remove(&id);
            order.retain(|o| o != &id);
        }
    }
}

/// O app está fechando: para todas as execuções, espera no máximo `wait` elas
/// saírem (o `serve` solta lease e teclas de quem sai) e então solta o lease e
/// as teclas que sobraram — tecla presa faz o personagem andar sozinho depois
/// que o app fecha. Bloqueia (roda no fechamento). Não toca em cliente do
/// Roblox: o processo Python morre com o app pelo job object. Devolve quantas
/// execuções estavam vivas e as teclas soltas aqui.
pub fn shutdown(manager: &RunManager, focus: &FocusQueue, desk: &dyn Desk, wait: Duration) -> (usize, Vec<String>) {
    let stopped = manager.stop_all();
    let deadline = std::time::Instant::now() + wait;
    while manager.active() > 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    (stopped, focus.release_current(desk))
}

// ── o log (as últimas 5 000 linhas) ───────────────────────────────────────

pub struct LogBook {
    lines: Mutex<VecDeque<PythonLogLine>>,
}

impl LogBook {
    pub fn new() -> Self {
        Self {
            lines: Mutex::new(VecDeque::new()),
        }
    }
    pub fn push(&self, line: PythonLogLine) {
        let mut lines = self.lines.lock().unwrap();
        lines.push_back(line);
        while lines.len() > LOG_CAPACITY {
            lines.pop_front();
        }
    }
    pub fn all(&self) -> Vec<PythonLogLine> {
        self.lines.lock().unwrap().iter().cloned().collect()
    }
}

impl Default for LogBook {
    fn default() -> Self {
        Self::new()
    }
}

/// As linhas que ainda não foram para a tela: o app as manda num evento
/// `python-log` só (um array) a cada tique de 100 ms, não um evento por linha
/// — um script que imprime sem parar não inunda a tela de eventos.
#[derive(Default)]
pub struct LogBatch {
    pending: Mutex<VecDeque<PythonLogLine>>,
}

impl LogBatch {
    pub fn push(&self, line: PythonLogLine) {
        let mut pending = self.pending.lock().unwrap();
        pending.push_back(line);
        while pending.len() > LOG_CAPACITY {
            pending.pop_front();
        }
    }

    pub fn take(&self) -> Vec<PythonLogLine> {
        self.pending.lock().unwrap().drain(..).collect()
    }
}

// ── o processo Python ─────────────────────────────────────────────────────

/// Roda `"<python>" -s -E -B -X utf8 -u -m multialt._core.host scripts/<nome>` na pasta
/// de trabalho, sem janela de console, com stdin/stdout no protocolo e o
/// stderr no log (nível `error`).
pub struct ProcessExecutor {
    pub python_exe: PathBuf,
    pub workspace: PathBuf,
}

impl Executor for ProcessExecutor {
    fn execute(&self, ctx: RunCtx, hello: Value) -> BoxFuture<'static, ExecOutcome> {
        let exe = self.python_exe.clone();
        let workspace = self.workspace.clone();
        let script = hello["run"]["script"].as_str().unwrap_or_default().to_string();
        Box::pin(async move { run_process(exe, workspace, script, ctx, hello).await })
    }
}

async fn run_process(exe: PathBuf, workspace: PathBuf, script: String, ctx: RunCtx, hello: Value) -> ExecOutcome {
    use std::process::Stdio;
    use tokio::io::AsyncBufReadExt;

    let mut command = tokio::process::Command::new(&exe);
    command
        .args(super::runtime::ISOLATION_FLAGS)
        .args(["-u", "-m", "multialt._core.host"])
        .arg(format!("scripts/{script}"))
        .current_dir(&workspace)
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONNOUSERSITE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        // Sem isto o python.exe acende um console preto.
        command.creation_flags(0x0800_0000);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            let message = format!("Could not start Python: {e}");
            ctx.sink.log(super::bridge::system_line(&ctx.run_id, None, "error", message.clone()));
            return ExecOutcome {
                exit_code: -1,
                error: Some(message),
            };
        }
    };
    let job = ProcessJob::attach(&child);
    let (Some(stdin), Some(stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take()) else {
        return ExecOutcome {
            exit_code: -1,
            error: Some("Python pipes are not available".into()),
        };
    };

    let sink = ctx.sink.clone();
    let run_id = ctx.run_id.clone();
    let stop = ctx.stop.clone();
    let last_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let last = last_error.clone();
    let stderr_task = tokio::spawn(async move {
        let mut lines = tokio::io::BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if !line.trim().is_empty() {
                *last.lock().unwrap() = Some(line.trim().to_string());
            }
            sink.log(super::bridge::system_line(&run_id, None, "error", clamp_log_message(line)));
        }
    });

    let serve = super::bridge::serve(ctx, hello, tokio::io::BufReader::new(stdout), stdin);
    tokio::pin!(serve);
    // Parar: o evento `stop` sai pelo bridge; 3 s depois, se o Python não
    // saiu, morre (só ele e os filhos dele, pelo job object).
    let killer = async {
        while !stop.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        tokio::time::sleep(STOP_GRACE).await;
    };
    let mut killed = false;
    tokio::select! {
        _ = &mut serve => {}
        _ = killer => {
            killed = true;
            job.terminate();
            let _ = child.start_kill();
            let _ = serve.await;
        }
    }
    let status = child.wait().await;
    let _ = tokio::time::timeout(Duration::from_secs(2), stderr_task).await;
    let exit_code = status.ok().and_then(|s| s.code()).unwrap_or(if killed { 0 } else { -1 });
    let error = if exit_code != 0 { last_error.lock().unwrap().clone() } else { None };
    ExecOutcome { exit_code, error }
}

/// Um job object com `KILL_ON_JOB_CLOSE` em volta do processo Python: matar o
/// job mata o Python e o que ele abriu, e nada mais.
struct ProcessJob {
    #[cfg(windows)]
    handle: Option<isize>,
}

impl ProcessJob {
    #[cfg(windows)]
    fn attach(child: &tokio::process::Child) -> Self {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        let Some(raw) = child.raw_handle() else {
            return Self { handle: None };
        };
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Self { handle: None };
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if AssignProcessToJobObject(job, raw as _) == 0 {
                CloseHandle(job);
                return Self { handle: None };
            }
            Self { handle: Some(job as isize) }
        }
    }

    #[cfg(not(windows))]
    fn attach(_child: &tokio::process::Child) -> Self {
        Self {}
    }

    fn terminate(&self) {
        #[cfg(windows)]
        if let Some(job) = self.handle {
            unsafe {
                windows_sys::Win32::System::JobObjects::TerminateJobObject(job as _, 1);
            }
        }
    }
}

impl Drop for ProcessJob {
    fn drop(&mut self) {
        #[cfg(windows)]
        if let Some(job) = self.handle.take() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(job as _);
            }
        }
    }
}

#[cfg(test)]
pub mod python_runs_tests {
    use super::*;
    use crate::python::primitives::python_primitives_tests::{account, own_queue, FakeDesk};

    #[derive(Default)]
    pub struct Recorder {
        pub lines: Mutex<Vec<PythonLogLine>>,
        pub updates: Mutex<Vec<PythonRunInfo>>,
    }
    impl RunEvents for Recorder {
        fn log(&self, line: PythonLogLine) {
            self.lines.lock().unwrap().push(line);
        }
        fn run_update(&self, info: &PythonRunInfo) {
            self.updates.lock().unwrap().push(info.clone());
        }
        fn dashboard(&self, _script: &str, _op: &str, _args: &Value) -> Result<Value, PyError> {
            Ok(json!(true))
        }
        fn discord(&self, _script: &str, _user_id: Option<i64>, _title: Option<String>, _text: String) -> Result<Value, PyError> {
            Ok(json!(true))
        }
    }

    /// Uma "rodada" que dura `ms` ou até o Parar.
    struct FakeExec {
        ms: u64,
        calls: AtomicU64,
        exit_code: i32,
    }
    impl Executor for FakeExec {
        fn execute(&self, ctx: RunCtx, _hello: Value) -> BoxFuture<'static, ExecOutcome> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let ms = self.ms;
            let code = self.exit_code;
            Box::pin(async move {
                let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
                while tokio::time::Instant::now() < deadline && !ctx.stop.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                ExecOutcome {
                    exit_code: code,
                    error: (code != 0).then(|| "ValueError: boom".to_string()),
                }
            })
        }
    }

    fn fake(ms: u64, exit_code: i32) -> Arc<FakeExec> {
        Arc::new(FakeExec {
            ms,
            calls: AtomicU64::new(0),
            exit_code,
        })
    }

    fn manager(minute_ms: u64) -> (Arc<RunManager>, Arc<Recorder>) {
        let rec = Arc::new(Recorder::default());
        let m = Arc::new(RunManager::with_minute(rec.clone(), FakeDesk::with_window(1, 11), own_queue(), minute_ms));
        (m, rec)
    }

    pub fn config(script: &str, trigger: PythonTrigger) -> PythonRunConfig {
        PythonRunConfig {
            script: script.into(),
            accounts: vec![1],
            mode: PythonRunMode::OneAtATime,
            trigger,
        }
    }

    async fn wait_for(m: &RunManager, id: &str, state: &str) -> PythonRunInfo {
        for _ in 0..400 {
            if let Some(info) = m.list().into_iter().find(|i| i.id == id) {
                if info.state == state {
                    return info;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("run {id} never reached {state}: {:?}", m.list());
    }

    #[tokio::test]
    async fn two_runs_coexist() {
        let (m, rec) = manager(60_000);
        let a = m.start(config("a.py", PythonTrigger::Loop), vec![account(1, "Main_01")], "app", fake(10_000, 0)).unwrap();
        let b = m.start(config("b.py", PythonTrigger::Manual), vec![account(1, "Main_01")], "terminal", fake(10_000, 0)).unwrap();
        assert_ne!(a, b);
        wait_for(&m, &a, "running").await;
        let info_b = wait_for(&m, &b, "running").await;
        assert_eq!(info_b.source, "terminal");
        assert_eq!(m.active(), 2);
        m.stop(&a).unwrap();
        wait_for(&m, &a, "finished").await;
        assert_eq!(m.list().iter().find(|i| i.id == b).unwrap().state, "running", "stopping one leaves the other");
        m.stop(&b).unwrap();
        wait_for(&m, &b, "finished").await;
        assert!(rec.lines.lock().unwrap().iter().any(|l| l.message.starts_with("Run started: a.py")));
    }

    #[tokio::test]
    async fn the_same_script_cannot_run_twice() {
        let (m, _) = manager(60_000);
        let first = m.start(config("farm.py", PythonTrigger::Loop), vec![account(1, "Main_01")], "app", fake(60_000, 0)).unwrap();
        wait_for(&m, &first, "running").await;
        // Nem pela tela nem pelo terminal, nem com outras contas.
        for source in ["app", "terminal"] {
            let error = m
                .start(config("farm.py", PythonTrigger::Manual), vec![account(2, "Alt_02")], source, fake(1, 0))
                .unwrap_err();
            assert_eq!(error, ALREADY_RUNNING);
        }
        assert_eq!(m.active(), 1);
        assert_eq!(m.list().len(), 1, "the refused start is not listed");
        // Outro script pode.
        let other = m.start(config("other.py", PythonTrigger::Loop), vec![account(1, "Main_01")], "app", fake(60_000, 0)).unwrap();
        // Parado, o mesmo script volta a poder.
        m.stop(&first).unwrap();
        wait_for(&m, &first, "finished").await;
        let again = m.start(config("farm.py", PythonTrigger::Manual), vec![account(1, "Main_01")], "app", fake(1, 0)).unwrap();
        assert_ne!(again, first);
        m.stop(&other).unwrap();
    }

    #[tokio::test]
    async fn interval_reschedules_after_finish() {
        let (m, _) = manager(40); // "minuto" de 40 ms
        let exec = fake(5, 0);
        let id = m.start(config("farm.py", PythonTrigger::Interval { minutes: 2 }), vec![account(1, "Main_01")], "app", exec.clone()).unwrap();
        let waiting = wait_for(&m, &id, "waiting").await;
        assert!(waiting.next_run_ms.is_some());
        for _ in 0..200 {
            if exec.calls.load(Ordering::SeqCst) >= 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(exec.calls.load(Ordering::SeqCst) >= 2, "the interval run did not start again");
        m.stop(&id).unwrap();
        let done = wait_for(&m, &id, "finished").await;
        assert!(done.ended_ms.is_some());
        let calls = exec.calls.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(exec.calls.load(Ordering::SeqCst), calls, "no more runs after Stop");
    }

    #[tokio::test]
    async fn stop_all_stops_every_run() {
        let (m, _) = manager(60_000);
        let ids: Vec<String> = (0..3)
            .map(|i| m.start(config(&format!("s{i}.py"), PythonTrigger::Loop), vec![account(1, "Main_01")], "app", fake(60_000, 0)).unwrap())
            .collect();
        for id in &ids {
            wait_for(&m, id, "running").await;
        }
        assert_eq!(m.stop_all(), 3);
        for id in &ids {
            wait_for(&m, id, "finished").await;
        }
        assert_eq!(m.active(), 0);
    }

    #[tokio::test]
    async fn a_failing_script_is_reported_with_its_last_traceback_line() {
        let (m, _) = manager(60_000);
        let id = m.start(config("bad.py", PythonTrigger::Manual), vec![account(1, "Main_01")], "app", fake(1, 1)).unwrap();
        let failed = wait_for(&m, &id, "failed").await;
        assert_eq!(failed.error.as_deref(), Some("ValueError: boom"));
    }

    /// Uma rodada que pega a fila, aperta W e então ignora o Parar (um script
    /// travado, ou o app fechando antes de o Python soltar).
    struct StuckExec;
    impl Executor for StuckExec {
        fn execute(&self, ctx: RunCtx, _hello: Value) -> BoxFuture<'static, ExecOutcome> {
            Box::pin(async move {
                let lease = crate::python::primitives::dispatch(&ctx, "focus.acquire", json!({"user_id": 1})).await;
                assert!(lease.is_ok(), "{lease:?}");
                crate::python::primitives::dispatch(&ctx, "input.key_down", json!({"user_id": 1, "key": "W"})).await.unwrap();
                std::future::pending::<()>().await;
                unreachable!()
            })
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn closing_the_app_stops_runs_and_releases_held_keys() {
        let desk = FakeDesk::with_window(1, 11);
        *desk.front.lock().unwrap() = 11;
        let queue = own_queue();
        let rec = Arc::new(Recorder::default());
        let m = Arc::new(RunManager::with_minute(rec, desk.clone(), queue.clone(), 60_000));
        let stuck = m.start(config("stuck.py", PythonTrigger::Loop), vec![account(1, "Main_01")], "app", Arc::new(StuckExec)).unwrap();
        let quick = m.start(config("quick.py", PythonTrigger::Loop), vec![account(1, "Main_01")], "app", fake(60_000, 0)).unwrap();
        for _ in 0..200 {
            if desk.sent().contains(&"down:W".to_string()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(queue.holds(&stuck, 1));

        let started = std::time::Instant::now();
        let (stopped, released) = {
            let (m, queue, desk) = (m.clone(), queue.clone(), desk.clone());
            tokio::task::spawn_blocking(move || shutdown(&m, &queue, desk.as_ref(), Duration::from_millis(300)))
                .await
                .unwrap()
        };
        assert_eq!(stopped, 2);
        assert!(started.elapsed() < Duration::from_secs(2), "the app does not wait long to close");
        assert_eq!(released, vec!["W".to_string()]);
        assert_eq!(desk.sent().last().map(String::as_str), Some("up:W"), "no key stays held when the app closes");
        assert!(!queue.holds(&stuck, 1));
        // A que obedece ao Parar terminou dentro da espera.
        wait_for(&m, &quick, "finished").await;
        // Nada de Roblox: só teclas e o lease.
        assert!(desk.sent().iter().all(|s| s.starts_with("down:") || s.starts_with("up:")));
    }

    /// As linhas vão para a tela em lotes (um evento por tique, não um por
    /// linha), na ordem, sem perder nenhuma — e sem guardar mais que o log.
    #[test]
    fn log_lines_are_sent_in_batches() {
        let batch = LogBatch::default();
        assert!(batch.take().is_empty());
        for i in 0..1_000 {
            batch.push(super::super::bridge::system_line("r1", None, "log", format!("{i}")));
        }
        let sent = batch.take();
        assert_eq!(sent.len(), 1_000);
        assert_eq!(sent[0].message, "0");
        assert_eq!(sent[999].message, "999");
        assert!(batch.take().is_empty(), "each line goes once");
        for i in 0..(LOG_CAPACITY + 10) {
            batch.push(super::super::bridge::system_line("r1", None, "log", format!("{i}")));
        }
        let sent = batch.take();
        assert_eq!(sent.len(), LOG_CAPACITY);
        assert_eq!(sent[0].message, "10");
    }

    #[test]
    fn the_log_keeps_the_last_5000_lines() {
        let book = LogBook::new();
        for i in 0..5_100 {
            book.push(super::super::bridge::system_line("r1", None, "log", format!("{i}")));
        }
        let all = book.all();
        assert_eq!(all.len(), LOG_CAPACITY);
        assert_eq!(all[0].message, "100");
    }

    #[test]
    fn the_hello_line_matches_the_protocol() {
        let info = PythonRunInfo {
            id: "r1".into(),
            script: "farm.py".into(),
            accounts: vec![1],
            mode: PythonRunMode::OneAtATime,
            trigger: PythonTrigger::Manual,
            source: "app".into(),
            state: "starting".into(),
            started_ms: 0,
            ended_ms: None,
            next_run_ms: None,
            error: None,
        };
        let hello = hello_line(&info, &[account(1, "Main_01")]);
        assert_eq!(
            hello,
            json!({
                "event": "hello",
                "run": {"id": "r1", "script": "farm.py", "mode": "one_at_a_time", "trigger": {"kind": "manual"}, "source": "app"},
                "accounts": [{"user_id": 1, "name": "Main_01", "alias": "", "group": "Default", "in_game": true, "place_id": 0, "job_id": ""}]
            })
        );
    }
}

/// Ida e volta com um Python de verdade (o do PATH) e o SDK embutido: o host
/// lê o `hello`, o runner chama o script, e log, `print`, dashboard e uma trava
/// chegam ao app. Pula (passa com aviso) sem `python` no PATH.
#[cfg(test)]
mod python_e2e_tests {
    use super::*;
    use crate::python::primitives::python_primitives_tests::{account, own_queue, FakeDesk};

    #[derive(Default)]
    struct Capture {
        lines: Mutex<Vec<PythonLogLine>>,
        dashboard: Mutex<Vec<(String, Value)>>,
    }
    impl RunSink for Capture {
        fn log(&self, line: PythonLogLine) {
            self.lines.lock().unwrap().push(line);
        }
        fn status(&self, _user_id: Option<i64>, text: &str) {
            self.lines.lock().unwrap().push(super::super::bridge::system_line("r1", None, "system", format!("Status: {text}")));
        }
        fn dashboard(&self, op: &str, args: &Value) -> Result<Value, PyError> {
            self.dashboard.lock().unwrap().push((op.to_string(), args.clone()));
            Ok(json!(true))
        }
        fn discord(&self, _user_id: Option<i64>, _title: Option<String>, _text: String) -> Result<Value, PyError> {
            Ok(json!(true))
        }
    }

    fn python_on_path() -> Option<PathBuf> {
        let mut cmd = std::process::Command::new("python");
        cmd.args(["-I", "-c", "import sys;print(sys.executable)"]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let out = cmd.output().ok()?;
        out.status
            .success()
            .then(|| PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
    }

    const SCRIPT: &str = r#"
from multialt import log, dashboard, status

def main_all(accs):
    log("accounts:", ",".join(a.name for a in accs))
    print("printed line")
    status("Farming")
    dashboard.set("loops", 3)
    try:
        accs[0].press("Space")
    except Exception as e:
        log("caught:", type(e).__name__)
"#;

    #[tokio::test]
    async fn real_python_round_trip() {
        let Some(python) = python_on_path() else {
            eprintln!("python not on PATH: skipping the end-to-end check");
            return;
        };
        let ws = std::env::temp_dir().join(format!(
            "ram-pye2e-{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        super::super::workspace::sync_workspace(&ws).unwrap();
        std::fs::write(ws.join("scripts").join("e2e.py"), SCRIPT).unwrap();

        let sink = Arc::new(Capture::default());
        let ctx = RunCtx::new(
            "r1",
            vec![account(1, "Main_01")],
            Arc::new(AtomicBool::new(false)),
            sink.clone(),
            FakeDesk::with_window(1, 11),
            own_queue(),
        );
        let info = PythonRunInfo {
            id: "r1".into(),
            script: "e2e.py".into(),
            accounts: vec![1],
            mode: PythonRunMode::OneAtATime,
            trigger: PythonTrigger::Manual,
            source: "app".into(),
            state: "running".into(),
            started_ms: 0,
            ended_ms: None,
            next_run_ms: None,
            error: None,
        };
        let exec = ProcessExecutor {
            python_exe: python,
            workspace: ws.clone(),
        };
        let outcome = tokio::time::timeout(
            Duration::from_secs(60),
            exec.execute(ctx, hello_line(&info, &[account(1, "Main_01")])),
        )
        .await
        .expect("python finished in time");
        let lines = sink.lines.lock().unwrap().clone();
        let text: Vec<String> = lines.iter().map(|l| format!("[{}] {}", l.level, l.message)).collect();
        assert_eq!(outcome.exit_code, 0, "{outcome:?}\n{}", text.join("\n"));
        assert!(lines.iter().any(|l| l.level == "log" && l.message.contains("accounts: Main_01")), "{text:?}");
        assert!(lines.iter().any(|l| l.message.contains("printed line")), "{text:?}");
        assert!(lines.iter().any(|l| l.message == "Status: Farming"), "{text:?}");
        assert!(
            lines.iter().any(|l| l.level == "blocked" && l.code.as_deref() == Some("no_focus_lease")),
            "{text:?}"
        );
        assert!(lines.iter().any(|l| l.message.contains("caught: NoFocusLease")), "{text:?}");
        let dash = sink.dashboard.lock().unwrap().clone();
        assert!(dash.iter().any(|(op, args)| op == "dashboard.set" && args["id"] == json!("loops") && args["value"] == json!(3)), "{dash:?}");
        let _ = std::fs::remove_dir_all(&ws);
    }
}
