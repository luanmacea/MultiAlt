// /proc e sinais: tudo o que olha ou mexe em processo no Linux.

use std::collections::HashSet;
use std::fs;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::platform::sober::{self, ProcStat};

fn read_stat(pid: u32) -> Option<ProcStat> {
    let content = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    sober::parse_proc_stat(&content)
}

/// O processo existe e não é zumbi.
pub fn is_pid_alive(pid: u32) -> bool {
    read_stat(pid).is_some_and(|s| sober::is_running_state(s.state))
}

/// (pid, ppid) de todo processo visível.
fn proc_table() -> Vec<(u32, u32)> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let pid: u32 = entry.file_name().to_str()?.parse().ok()?;
            let stat = read_stat(pid)?;
            Some((pid, stat.ppid))
        })
        .collect()
}

/// Id do netns de um processo (`None` se sumiu ou não temos permissão).
pub fn netns_id(pid: u32) -> Option<u64> {
    let target = fs::read_link(format!("/proc/{pid}/ns/net")).ok()?;
    sober::parse_netns_id(&target.to_string_lossy())
}

/// O netns do próprio app (o do host).
pub fn own_netns_id() -> Option<u64> {
    fs::read_link("/proc/self/ns/net")
        .ok()
        .and_then(|t| sober::parse_netns_id(&t.to_string_lossy()))
}

/// Tudo o que é desta conta: a árvore do processo raiz mais quem está no netns
/// próprio dela (pega o que foi reparentado depois de o pai morrer). O netns
/// só conta quando NÃO é o do próprio app, para nunca varrer o sistema inteiro.
pub fn account_pids(root: u32, netns: Option<u64>) -> Vec<u32> {
    let table = proc_table();
    let mut pids: HashSet<u32> = sober::process_tree(&table, root).into_iter().collect();
    if let Some(ns) = netns.filter(|ns| Some(*ns) != own_netns_id()) {
        for (pid, _) in &table {
            if netns_id(*pid) == Some(ns) {
                pids.insert(*pid);
            }
        }
    }
    let mut out: Vec<u32> = pids.into_iter().filter(|p| is_pid_alive(*p)).collect();
    out.sort_unstable();
    out
}

fn signal(name: &str, pids: &[u32]) {
    if pids.is_empty() {
        return;
    }
    let _ = Command::new("kill")
        .arg(format!("-{name}"))
        .args(pids.iter().map(|p| p.to_string()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Fecha o cliente de UMA conta: TERM na árvore dele, espera, KILL no que
/// sobrar. `netns` é o da conta (guardado quando ela abriu).
pub fn kill_account_tree(root: u32, netns: Option<u64>) -> Result<(), String> {
    let first = account_pids(root, netns);
    if first.is_empty() {
        return Ok(());
    }
    signal("TERM", &first);

    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if account_pids(root, netns).is_empty() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    signal("KILL", &account_pids(root, netns));
    std::thread::sleep(Duration::from_millis(200));
    if account_pids(root, netns).is_empty() {
        Ok(())
    } else {
        Err(format!("Could not close the client of process {root}"))
    }
}

/// Mesma coisa quando só se tem o pid (descobre o netns agora).
pub fn kill_process_tree(root: u32) -> Result<(), String> {
    kill_account_tree(root, netns_id(root))
}
