// Camada de SO do Linux (Sober). A decisão — caminhos, argv, deep link, cópia
// do perfil, cookie — mora em `platform/sober.rs` (pura, testada no Windows);
// aqui só se executa: processos, /proc, PATH e variáveis de ambiente.
//
// O formato espelha `platform/macos`: `tracker()`, `get_roblox_pids`,
// `kill_process`, `kill_all_roblox`, `enable/disable_multi_roblox`,
// `get_roblox_path` e `generate_browser_tracker_id`. O que o Linux tem a mais
// é `launch_account` (perfil + cookie + rede isolada) e `diagnose`.

mod diagnostics;
mod launch;
mod process;
mod tracker;

pub use diagnostics::diagnose;
pub use launch::{launch_account, LaunchFailure, LaunchRequest};
pub use process::{is_pid_alive, kill_process_tree};
pub use tracker::tracker;

use std::time::{SystemTime, UNIX_EPOCH};

pub fn generate_browser_tracker_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let a = (now % 75000 + 100000) as u64;
    let b = ((now / 31) % 800000 + 100000) as u64;
    format!("{}{}", a, b)
}

/// Várias contas ao mesmo tempo é do desenho (um netns por cliente), não de um
/// mutex a tomar: não há o que ligar.
pub fn enable_multi_roblox() -> Result<bool, String> {
    Ok(true)
}

pub fn disable_multi_roblox() -> Result<(), String> {
    Ok(())
}

/// O caminho do `flatpak`, que é o que de fato se executa; o erro explica o
/// que falta instalar.
pub fn get_roblox_path() -> Result<String, String> {
    let report = diagnose();
    if let Some(flatpak) = report.flatpak.as_ref().filter(|_| report.sober_installed) {
        return Ok(flatpak.to_string_lossy().into_owned());
    }
    Err(report
        .problems()
        .into_iter()
        .next()
        .unwrap_or_else(|| "Sober was not found".to_string()))
}

/// Os clientes que ESTE app abriu e que seguem rodando (a raiz de cada um).
/// Clientes abertos fora do app não entram: nunca são nossos para fechar.
pub fn get_roblox_pids() -> Vec<u32> {
    let mut pids = tracker().get_tracked_pids();
    pids.retain(|pid| is_pid_alive(*pid));
    pids.sort_unstable();
    pids
}

/// Fecha UM cliente (e só a árvore dele).
pub fn kill_process(pid: u32) -> Result<(), String> {
    kill_process_tree(pid)
}

/// Fecha os clientes que o app abriu — nunca varre o sistema atrás de
/// processos com nome parecido.
pub fn kill_all_roblox() -> u32 {
    let mut killed = 0u32;
    for pid in get_roblox_pids() {
        if kill_process(pid).is_ok() {
            killed += 1;
        }
    }
    killed
}
