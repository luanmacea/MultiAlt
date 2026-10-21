// O canal do terminal do Modo desenvolvedor (docs/features/python-automation.md,
// "Modo desenvolvedor"): um named pipe `\\.\pipe\multialt-python-<hash do SID>`
// com DACL **só do usuário atual** e clientes remotos recusados. Nada de porta
// de rede. Aberto só com o modo ligado e o app aberto; o nome vai para
// `multialt/_core/pipe.txt` enquanto ele estiver aberto.
//
// `python -m multialt run <script> --account <nome>` conecta, manda
// `{"op":"cli.start","args":{...}}` e, aceito, recebe o `hello` e vira uma
// execução como outra qualquer — listada em Running como "terminal" — servida
// pelo mesmo `bridge::serve`. O script roda no processo do terminal.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use futures_util::future::BoxFuture;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};

use super::primitives::RunCtx;
use super::runs::{ExecOutcome, Executor, RunManager};
use super::types::{AccountView, PythonRunConfig, PythonRunMode, PythonTrigger};

pub const PIPE_FILE: &str = "multialt/_core/pipe.txt";
const PIPE_PREFIX: &str = r"\\.\pipe\multialt-python-";

fn fnv1a64(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// O nome do pipe do usuário com este SID (o mesmo a cada abertura do app).
pub fn pipe_name_for_sid(sid: &str) -> String {
    format!("{PIPE_PREFIX}{:016x}", fnv1a64(sid))
}

/// O pedido do terminal, já conferido.
#[derive(Debug, Clone, PartialEq)]
pub struct CliStart {
    pub script: String,
    pub accounts: Vec<String>,
    pub mode: PythonRunMode,
    pub trigger: PythonTrigger,
}

fn refuse(message: &str, hint: &str) -> String {
    json!({"error": {"message": message, "hint": hint}}).to_string()
}

pub fn parse_cli_start(line: &str) -> Result<CliStart, String> {
    let bad = |m: &str| refuse(m, "Run: python -m multialt run <script> --account NAME");
    let message: Value = serde_json::from_str(line.trim()).map_err(|_| bad("The first message must be cli.start"))?;
    if message.get("op").and_then(Value::as_str) != Some("cli.start") {
        return Err(bad("The first message must be cli.start"));
    }
    let args = message.get("args").cloned().unwrap_or(Value::Null);
    let script = args.get("script").and_then(Value::as_str).unwrap_or("").trim().to_string();
    if script.is_empty() || script.len() > 128 || script.contains(['/', '\\']) {
        return Err(bad("cli.start needs the script file name"));
    }
    let accounts: Vec<String> = args
        .get("accounts")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    if accounts.is_empty() {
        return Err(bad("Pick at least one account with --account NAME"));
    }
    let mode = match args.get("mode").cloned() {
        None | Some(Value::Null) => PythonRunMode::OneAtATime,
        Some(v) => serde_json::from_value(v).map_err(|_| bad("mode must be one_at_a_time or all_together"))?,
    };
    let trigger = match args.get("trigger").cloned() {
        None | Some(Value::Null) => PythonTrigger::Manual,
        Some(v) => serde_json::from_value(v).map_err(|_| bad("trigger must be manual, loop or interval:<minutes>"))?,
    };
    Ok(CliStart {
        script,
        accounts,
        mode,
        trigger,
    })
}

/// A rodada de uma execução do terminal: o `serve` sobre o próprio pipe. Só
/// uma (o laço é do script, no terminal).
struct PipeExecutor<S> {
    halves: Mutex<Option<(BufReader<ReadHalf<S>>, WriteHalf<S>)>>,
}

impl<S: AsyncRead + AsyncWrite + Send + 'static> Executor for PipeExecutor<S> {
    fn execute(&self, ctx: RunCtx, hello: Value) -> BoxFuture<'static, ExecOutcome> {
        let halves = self.halves.lock().unwrap().take();
        Box::pin(async move {
            let Some((reader, writer)) = halves else {
                return ExecOutcome {
                    exit_code: 0,
                    error: None,
                };
            };
            let result = super::bridge::serve(ctx, hello, reader, writer).await;
            ExecOutcome {
                exit_code: 0,
                error: result.err(),
            }
        })
    }
    fn repeatable(&self) -> bool {
        false
    }
}

/// Atende um cliente do terminal: confere o `cli.start`, resolve as contas pelo
/// nome e registra a execução (`source: "terminal"`). Devolve o id dela.
pub async fn handle_client<S>(
    stream: S,
    resolve: &(dyn Fn(&[String]) -> Result<Vec<AccountView>, String> + Send + Sync),
    manager: &Arc<RunManager>,
) -> Result<String, String>
where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (read, mut write) = tokio::io::split(stream);
    let mut reader = BufReader::new(read);
    let mut first = String::new();
    let read = tokio::time::timeout(std::time::Duration::from_secs(10), reader.read_line(&mut first)).await;
    if !matches!(read, Ok(Ok(n)) if n > 0) {
        return Err("The terminal client sent nothing".into());
    }
    let start = match parse_cli_start(&first) {
        Ok(start) => start,
        Err(reply) => {
            let _ = write.write_all(format!("{reply}\n").as_bytes()).await;
            let _ = write.flush().await;
            return Err(reply);
        }
    };
    let accounts = match resolve(&start.accounts) {
        Ok(accounts) => accounts,
        Err(message) => {
            let reply = refuse(&message, "Use the account names shown in MultiAlt (or their alias).");
            let _ = write.write_all(format!("{reply}\n").as_bytes()).await;
            let _ = write.flush().await;
            return Err(message);
        }
    };
    let config = PythonRunConfig {
        script: start.script,
        accounts: accounts.iter().map(|a| a.user_id).collect(),
        mode: start.mode,
        trigger: start.trigger,
    };
    let executor = Arc::new(PipeExecutor {
        halves: Mutex::new(Some((reader, write))),
    });
    Ok(manager.start(config, accounts, "terminal", executor))
}

// ── o servidor (Windows) ──────────────────────────────────────────────────

#[cfg(target_os = "windows")]
pub use server::*;

#[cfg(target_os = "windows")]
mod server {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn wide(s: &str) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    /// O SID do usuário que roda o app, em texto (`S-1-5-21-...`).
    pub fn current_user_sid() -> Result<String, String> {
        use windows_sys::Win32::Foundation::{CloseHandle, LocalFree};
        use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
        use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
        unsafe {
            let mut token = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err("Could not read the current user".into());
            }
            let mut needed = 0u32;
            GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed);
            let mut buf = vec![0u8; needed.max(64) as usize];
            let ok = GetTokenInformation(token, TokenUser, buf.as_mut_ptr() as *mut _, buf.len() as u32, &mut needed);
            CloseHandle(token);
            if ok == 0 {
                return Err("Could not read the current user".into());
            }
            let user = &*(buf.as_ptr() as *const TOKEN_USER);
            let mut text: windows_sys::core::PWSTR = std::ptr::null_mut();
            if ConvertSidToStringSidW(user.User.Sid, &mut text) == 0 {
                return Err("Could not read the current user".into());
            }
            let mut len = 0;
            while *text.add(len) != 0 {
                len += 1;
            }
            let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
            LocalFree(text as _);
            Ok(sid)
        }
    }

    /// O descritor de segurança "só este usuário, acesso total".
    struct OwnerOnly {
        descriptor: windows_sys::Win32::Security::PSECURITY_DESCRIPTOR,
    }
    unsafe impl Send for OwnerOnly {}
    unsafe impl Sync for OwnerOnly {}

    impl OwnerOnly {
        fn new(sid: &str) -> Result<Self, String> {
            use windows_sys::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
            let sddl = wide(&format!("D:P(A;;GA;;;{sid})"));
            let mut descriptor = std::ptr::null_mut();
            let ok = unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), 1, &mut descriptor, std::ptr::null_mut())
            };
            if ok == 0 {
                return Err("Could not build the pipe permissions".into());
            }
            Ok(Self { descriptor })
        }

        fn create(&self, name: &str, first: bool) -> std::io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
            use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
            let mut attrs = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: self.descriptor,
                bInheritHandle: 0,
            };
            let mut options = tokio::net::windows::named_pipe::ServerOptions::new();
            options.reject_remote_clients(true).first_pipe_instance(first);
            unsafe { options.create_with_security_attributes_raw(name, &mut attrs as *mut _ as *mut core::ffi::c_void) }
        }
    }

    impl Drop for OwnerOnly {
        fn drop(&mut self) {
            unsafe {
                windows_sys::Win32::Foundation::LocalFree(self.descriptor as _);
            }
        }
    }

    pub type Resolver = Arc<dyn Fn(&[String]) -> Result<Vec<AccountView>, String> + Send + Sync>;

    /// O pipe aberto. Fechar (`stop` ou soltar) apaga o `pipe.txt`.
    pub struct PipeServer {
        pub name: String,
        pipe_file: PathBuf,
        closed: Arc<AtomicBool>,
        task: tokio::task::JoinHandle<()>,
    }

    impl PipeServer {
        /// Abre o pipe `name` e grava o nome em `<workspace>/multialt/_core/pipe.txt`.
        pub fn start(
            name: String,
            sid: &str,
            workspace: &Path,
            resolve: Resolver,
            manager: Arc<RunManager>,
            on_error: Arc<dyn Fn(String) + Send + Sync>,
        ) -> Result<Self, String> {
            let security = Arc::new(OwnerOnly::new(sid)?);
            let first = security
                .create(&name, true)
                .map_err(|e| format!("Could not open the terminal channel: {e}"))?;
            let pipe_file = workspace.join(PIPE_FILE);
            if let Some(parent) = pipe_file.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&pipe_file, &name).map_err(|e| e.to_string())?;
            let closed = Arc::new(AtomicBool::new(false));
            let pipe_name = name.clone();
            let flag = closed.clone();
            let task = tokio::spawn(async move {
                let mut server = first;
                loop {
                    if server.connect().await.is_err() {
                        if flag.load(Ordering::SeqCst) {
                            return;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        match security.create(&pipe_name, false) {
                            Ok(next) => server = next,
                            Err(e) => {
                                on_error(format!("Terminal channel closed: {e}"));
                                return;
                            }
                        }
                        continue;
                    }
                    let connected = server;
                    server = match security.create(&pipe_name, false) {
                        Ok(next) => next,
                        Err(e) => {
                            on_error(format!("Terminal channel closed: {e}"));
                            return;
                        }
                    };
                    let resolve = resolve.clone();
                    let manager = manager.clone();
                    let on_error = on_error.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_client(connected, resolve.as_ref(), &manager).await {
                            on_error(format!("Terminal run refused: {e}"));
                        }
                    });
                }
            });
            Ok(Self {
                name,
                pipe_file,
                closed,
                task,
            })
        }

        pub fn stop(self) {}
    }

    impl Drop for PipeServer {
        fn drop(&mut self) {
            self.closed.store(true, Ordering::SeqCst);
            self.task.abort();
            let _ = std::fs::remove_file(&self.pipe_file);
        }
    }
}

#[cfg(test)]
mod python_devpipe_tests {
    use super::*;
    use crate::python::primitives::python_primitives_tests::{account, own_queue, FakeDesk};
    use crate::python::runs::python_runs_tests::Recorder;
    use std::time::Duration;

    fn manager() -> Arc<RunManager> {
        Arc::new(RunManager::new(Arc::new(Recorder::default()), FakeDesk::with_window(1, 11), own_queue()))
    }

    fn resolver(names: &[String]) -> Result<Vec<AccountView>, String> {
        let known = [account(1, "Main_01"), account(2, "Alt_02")];
        let mut out = Vec::new();
        let mut unknown = Vec::new();
        for n in names {
            match known.iter().find(|a| a.name.eq_ignore_ascii_case(n)) {
                Some(a) => out.push(a.clone()),
                None => unknown.push(n.clone()),
            }
        }
        if unknown.is_empty() {
            Ok(out)
        } else {
            Err(format!("Unknown account(s): {}", unknown.join(", ")))
        }
    }

    #[test]
    fn pipe_name_is_stable_per_user() {
        let a = pipe_name_for_sid("S-1-5-21-1-2-3-1001");
        assert_eq!(a, pipe_name_for_sid("S-1-5-21-1-2-3-1001"));
        assert_ne!(a, pipe_name_for_sid("S-1-5-21-1-2-3-1002"));
        assert!(a.starts_with(r"\\.\pipe\multialt-python-"));
        assert_eq!(a.len(), PIPE_PREFIX.len() + 16);
        assert!(!a.contains("S-1-5"), "the SID itself is not exposed in the name");
        assert_eq!(fnv1a64(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64("a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn cli_start_is_parsed_and_checked() {
        let ok = parse_cli_start(r#"{"op":"cli.start","args":{"script":"farm.py","accounts":["Main_01"],"mode":"all_together","trigger":{"kind":"interval","minutes":5}}}"#).unwrap();
        assert_eq!(ok.mode, PythonRunMode::AllTogether);
        assert_eq!(ok.trigger, PythonTrigger::Interval { minutes: 5 });
        for bad in [
            "nope",
            r#"{"op":"run.sleep"}"#,
            r#"{"op":"cli.start","args":{"script":"","accounts":["x"]}}"#,
            r#"{"op":"cli.start","args":{"script":"../x.py","accounts":["x"]}}"#,
            r#"{"op":"cli.start","args":{"script":"x.py","accounts":[]}}"#,
            r#"{"op":"cli.start","args":{"script":"x.py","accounts":["a"],"mode":"fast"}}"#,
        ] {
            let reply: Value = serde_json::from_str(&parse_cli_start(bad).unwrap_err()).unwrap();
            assert!(reply["error"]["message"].is_string(), "{bad}");
        }
    }

    #[tokio::test]
    async fn cli_start_with_unknown_account_is_refused() {
        let m = manager();
        let (client, server) = tokio::io::duplex(8192);
        let (cr, mut cw) = tokio::io::split(client);
        cw.write_all(b"{\"op\":\"cli.start\",\"args\":{\"script\":\"farm.py\",\"accounts\":[\"Main_01\",\"Nobody\"]}}\n").await.unwrap();
        let result = handle_client(server, &resolver, &m).await;
        assert!(result.unwrap_err().contains("Nobody"));
        let mut lines = BufReader::new(cr).lines();
        let reply: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(reply["error"]["message"], json!("Unknown account(s): Nobody"));
        assert!(reply["error"]["hint"].is_string());
        assert!(m.list().is_empty(), "no run is registered");
    }

    #[tokio::test]
    async fn cli_run_is_listed_as_terminal() {
        let m = manager();
        let (client, server) = tokio::io::duplex(64 * 1024);
        let (cr, mut cw) = tokio::io::split(client);
        cw.write_all(b"{\"op\":\"cli.start\",\"args\":{\"script\":\"farm.py\",\"accounts\":[\"main_01\"],\"mode\":\"one_at_a_time\",\"trigger\":{\"kind\":\"manual\"}}}\n").await.unwrap();
        let id = handle_client(server, &resolver, &m).await.unwrap();
        let mut lines = BufReader::new(cr).lines();
        let hello: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(hello["event"], json!("hello"));
        assert_eq!(hello["run"]["source"], json!("terminal"));
        assert_eq!(hello["accounts"][0]["name"], json!("Main_01"));
        let run = m.list().into_iter().find(|r| r.id == id).unwrap();
        assert_eq!(run.source, "terminal");
        assert_eq!(run.accounts, vec![1]);
        // Uma op de verdade pelo canal.
        cw.write_all(b"{\"id\":1,\"op\":\"accounts\",\"args\":{}}\n").await.unwrap();
        let reply: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(reply["result"][0]["user_id"], json!(1));
        // O terminal fechou (as duas metades do cliente).
        drop(cw);
        drop(lines);
        for _ in 0..200 {
            if m.list().iter().any(|r| r.id == id && r.state == "finished") {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the terminal run did not finish when the client left: {:?}", m.list());
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn pipe_file_written_only_while_enabled() {
        let ws = std::env::temp_dir().join(format!(
            "ram-pypipe-{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let sid = current_user_sid().unwrap();
        assert!(sid.starts_with("S-1-"));
        // Nome próprio do teste: não briga com um app aberto no mesmo PC.
        let name = format!(r"\\.\pipe\multialt-python-test-{}", std::process::id());
        let m = manager();
        let server = PipeServer::start(name.clone(), &sid, &ws, Arc::new(resolver), m.clone(), Arc::new(|_| {})).unwrap();
        assert_eq!(std::fs::read_to_string(ws.join(PIPE_FILE)).unwrap(), name);

        // O usuário atual conecta (a DACL o deixa) e conversa.
        let client = tokio::net::windows::named_pipe::ClientOptions::new().open(&name).unwrap();
        let (cr, mut cw) = tokio::io::split(client);
        cw.write_all(b"{\"op\":\"cli.start\",\"args\":{\"script\":\"farm.py\",\"accounts\":[\"Ghost\"]}}\n").await.unwrap();
        let mut lines = BufReader::new(cr).lines();
        let reply: Value = serde_json::from_str(&tokio::time::timeout(Duration::from_secs(5), lines.next_line()).await.unwrap().unwrap().unwrap()).unwrap();
        assert!(reply["error"]["message"].as_str().unwrap().contains("Ghost"));

        drop(server);
        assert!(!ws.join(PIPE_FILE).exists(), "pipe.txt is removed when the channel closes");
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// O CLI de verdade (`python -m multialt run`) pelo pipe de verdade: o
    /// script roda no processo do terminal, o log volta ao app, e a execução
    /// aparece como "terminal". Pula sem `python` no PATH.
    #[cfg(target_os = "windows")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_terminal_cli_runs_a_script_over_the_pipe() {
        use std::os::windows::process::CommandExt;
        let probe = std::process::Command::new("python")
            .args(["-I", "-c", "print(1)"])
            .creation_flags(0x0800_0000)
            .output();
        if !matches!(&probe, Ok(o) if o.status.success()) {
            eprintln!("python not on PATH: skipping the terminal CLI check");
            return;
        }
        let ws = std::env::temp_dir().join(format!(
            "ram-pycli-{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        crate::python::workspace::sync_workspace(&ws).unwrap();
        std::fs::write(
            ws.join("scripts").join("cli_e2e.py"),
            "from multialt import log

def main(account):
    log('hello from', account.name)
",
        )
        .unwrap();
        let sid = current_user_sid().unwrap();
        let name = format!(r"\\.\pipe\multialt-python-clitest-{}", std::process::id());
        let rec = Arc::new(Recorder::default());
        let m = Arc::new(RunManager::new(rec.clone(), FakeDesk::with_window(1, 11), own_queue()));
        let server = PipeServer::start(name, &sid, &ws, Arc::new(resolver), m.clone(), Arc::new(|_| {})).unwrap();
        let ws_run = ws.clone();
        let output = tokio::task::spawn_blocking(move || {
            std::process::Command::new("python")
                .args(["-X", "utf8", "-m", "multialt", "run", "cli_e2e.py", "--account", "Main_01"])
                .current_dir(&ws_run)
                .env("PYTHONDONTWRITEBYTECODE", "1")
                .creation_flags(0x0800_0000)
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        assert!(output.status.success(), "exit {:?}
stdout: {stdout}
stderr: {stderr}", output.status.code());
        let runs = m.list();
        assert_eq!(runs.len(), 1, "{runs:?}");
        assert_eq!(runs[0].source, "terminal");
        assert_eq!(runs[0].script, "cli_e2e.py");
        let mut logged = false;
        for _ in 0..100 {
            logged = rec.lines.lock().unwrap().iter().any(|l| l.message.contains("hello from Main_01"));
            if logged {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(logged, "the log reached the app: {:?}", rec.lines.lock().unwrap());
        drop(server);
        let _ = std::fs::remove_dir_all(&ws);
    }
}
