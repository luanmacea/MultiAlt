// Abrir o cliente de UMA conta: perfil -> cookie -> processo em rede própria ->
// `pasta`. Toda a decisão (argv, ambiente, deep link) vem de `platform/sober.rs`;
// aqui só se executa. É síncrono e bloqueia por alguns segundos (espera do
// netns e do `pasta`): chame de `spawn_blocking`.

use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::platform::sober::{
    self, CommandSpec, DeepLink, EnvPlan, Runner, RunnerSettings,
};

use super::diagnostics::host_env;
use super::process::{kill_account_tree, netns_id, own_netns_id};
use super::tracker::tracker;

/// Quanto esperar o cliente aparecer no netns próprio antes de desistir.
const NETNS_WAIT: Duration = Duration::from_secs(5);

/// O que o chamador sabe de um launch (já com o job/servidor privado resolvidos
/// por `resolve_launch_job`).
pub struct LaunchRequest<'a> {
    /// Pasta de dados do app (`<dados>`); os perfis ficam em `<dados>/linux-profiles`.
    pub data_dir: &'a Path,
    pub user_id: i64,
    /// Valor do `.ROBLOSECURITY` da conta.
    pub cookie: &'a str,
    pub place_id: i64,
    pub job_id: &'a str,
    pub join_vip: bool,
    pub link_code: &'a str,
    pub launch_data: &'a str,
    /// As chaves `[Linux]` do INI.
    pub runner: &'a RunnerSettings,
    pub browser_tracker_id: &'a str,
}

#[derive(Debug, Clone)]
pub struct LaunchedClient {
    pub pid: u32,
    pub profile_home: PathBuf,
    /// Avisos para o log do launch (nunca impedem o jogo).
    pub warnings: Vec<String>,
}

/// Abre a conta e a registra no `tracker()`. Em caso de erro nada fica rodando.
pub fn launch_account(req: &LaunchRequest<'_>) -> Result<LaunchedClient, String> {
    let plan = sober::plan_runner(req.runner);
    let mut warnings = plan.warnings.clone();

    let link = DeepLink::from_launch(
        req.place_id,
        req.job_id,
        req.join_vip,
        req.link_code,
        req.launch_data,
    )?;
    warnings.extend(link.warnings.iter().cloned());

    let host = host_env();
    let profile_home = sober::profile_dir(req.data_dir, req.user_id)?;

    let pid = match &plan.runner {
        Runner::Sober => {
            let report = sober::ensure_profile(req.data_dir, req.user_id, &host.home)?;
            if report.skipped_symlinks > 0 {
                warnings.push(format!(
                    "{} symbolic link(s) in the main Sober folder were not copied",
                    report.skipped_symlinks
                ));
            }
            sober::write_cookie(req.data_dir, req.user_id, req.cookie)?;

            let command = sober::sober_launch_command(&link);
            let env = sober::sober_env_plan(&host, &profile_home, req.user_id);
            let log = open_launch_log(req.data_dir, &profile_home)?;
            let mut child = spawn(&command, &env, &log)?;
            let pid = child.id();

            if let Err(e) = connect_network(&mut child, &log) {
                let _ = kill_account_tree(pid, netns_id(pid));
                let _ = child.wait();
                return Err(e);
            }
            reap_in_background(child);
            tracker().track(req.user_id, pid, req.browser_tracker_id.to_string());
            pid
        }
        Runner::Custom(template) => {
            let command =
                sober::custom_launch_command(template, &link.uri(), &profile_home, req.user_id);
            let env = sober::custom_env_plan(&profile_home, req.user_id);
            let log = open_launch_log(req.data_dir, &profile_home)?;
            let child = spawn(&command, &env, &log)?;
            let pid = child.id();
            reap_in_background(child);
            // Sem rede isolada: o fechamento segue só a árvore do processo.
            tracker().track_in_netns(req.user_id, pid, req.browser_tracker_id.to_string(), None);
            pid
        }
    };

    Ok(LaunchedClient { pid, profile_home, warnings })
}

/// `<perfil>/launch.log`, sempre dentro da pasta de perfis e sem symlink.
fn open_launch_log(data_dir: &Path, profile_home: &Path) -> Result<PathBuf, String> {
    let root = sober::profiles_root(data_dir);
    sober::ensure_safe_path(&root, profile_home)?;
    fs::create_dir_all(profile_home)
        .map_err(|e| format!("Cannot create {}: {e}", profile_home.display()))?;
    let log = profile_home.join("launch.log");
    sober::ensure_safe_path(&root, &log)?;
    Ok(log)
}

fn spawn(command: &CommandSpec, env: &EnvPlan, log: &Path) -> Result<Child, String> {
    let log_file = fs::File::create(log)
        .map_err(|e| format!("Cannot write {}: {e}", log.display()))?;
    let log_err = log_file
        .try_clone()
        .map_err(|e| format!("Cannot write {}: {e}", log.display()))?;

    let mut process = Command::new(&command.program);
    process
        .args(&command.args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_err))
        // Grupo próprio: Ctrl+C no terminal do app não derruba os clientes.
        .process_group(0);
    for key in &env.remove {
        process.env_remove(key);
    }
    for (key, value) in &env.set {
        process.env(key, value);
    }

    process.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!(
                "\"{}\" was not found. Install it and try again.",
                command.program
            )
        } else {
            format!("Failed to start \"{}\": {e}", command.program)
        }
    })
}

/// Espera o cliente entrar no netns próprio e liga esse netns à internet com o
/// `pasta`. Sem `pasta` não há como abrir mais de uma conta, e o erro diz qual
/// pacote instalar.
fn connect_network(child: &mut Child, log: &Path) -> Result<(), String> {
    let pid = child.id();
    let deadline = Instant::now() + NETNS_WAIT;
    let host_ns = own_netns_id();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!(
                "The client closed right away ({status}). {}",
                log_tail(log)
            ));
        }
        match netns_id(pid) {
            Some(ns) if Some(ns) != host_ns => break,
            _ if Instant::now() >= deadline => {
                return Err(format!(
                    "The client did not get its own network in time. Is \"unshare\" allowed to create user namespaces? {}",
                    log_tail(log)
                ));
            }
            _ => std::thread::sleep(Duration::from_millis(50)),
        }
    }

    let pasta = sober::pasta_command(pid);
    let output = Command::new(&pasta.program)
        .args(&pasta.args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "\"pasta\" was not found. Install the \"passt\" package; it gives each account its own network so several can run at once.".to_string()
            } else {
                format!("Failed to run pasta: {e}")
            }
        })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "pasta could not connect the client to the network ({}). Check that the \"passt\" package is installed. {}",
            output.status,
            detail.trim()
        ));
    }
    Ok(())
}

/// O fim do log do cliente, para o erro dizer o que ele reclamou.
fn log_tail(log: &Path) -> String {
    let Ok(text) = fs::read_to_string(log) else {
        return String::new();
    };
    let tail: String = text.chars().rev().take(400).collect::<Vec<_>>().into_iter().rev().collect();
    tail.trim().to_string()
}

/// Recolhe o processo quando ele fecha, para não deixar zumbi.
fn reap_in_background(mut child: Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}
