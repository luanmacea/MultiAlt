fn find_pids_for_exes(exe_names: &[&str]) -> Vec<u32> {
    let mut pids = Vec::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot.is_null() || snapshot == INVALID_HANDLE_VALUE {
            return pids;
        }

        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let name_len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..name_len]);
                for needle in exe_names {
                    if name.eq_ignore_ascii_case(needle) {
                        pids.push(entry.th32ProcessID);
                        break;
                    }
                }
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }
    pids
}

/// Nome do executável de `pid` (ex.: `Fishtrap.exe`), para mensagens.
pub fn exe_name_of(pid: u32) -> Option<String> {
    let mut found = None;
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot.is_null() || snapshot == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                if entry.th32ProcessID == pid {
                    let len = entry
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    found = Some(String::from_utf16_lossy(&entry.szExeFile[..len]));
                    break;
                }
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }
    found
}

pub fn get_roblox_pids() -> Vec<u32> {
    find_pids_for_exes(&["RobloxPlayerBeta.exe"])
}

/// PIDs do Explorer: a área de trabalho é uma janela dele que cobre o monitor
/// inteiro, e não é "tela cheia" para o Modo AFK esperar.
pub fn get_shell_pids() -> Vec<u32> {
    find_pids_for_exes(&["explorer.exe"])
}

pub fn find_roblox_pids_all() -> Vec<u32> {
    find_pids_for_exes(&[
        "RobloxPlayerBeta.exe",
        "RobloxPlayerLauncher.exe",
        "RobloxCrashHandler.exe",
    ])
}

pub fn find_legacy_ram_pids() -> Vec<u32> {
    let current = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default();
    let candidates = ["Roblox Account Manager.exe", "RBX Alt Manager.exe"];
    let pids = find_pids_for_exes(&candidates);

    if current.eq_ignore_ascii_case("Roblox Account Manager.exe")
        || current.eq_ignore_ascii_case("RBX Alt Manager.exe")
    {
        let self_pid = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() };
        pids.into_iter().filter(|p| *p != self_pid).collect()
    } else {
        pids
    }
}

/// PIDs que o próprio app mandou fechar (Fechar, Auto Rejoin, Watcher...). O
/// monitor de quedas (`commands/client_health.rs`) usa isto para não chamar de
/// "fechou sozinho" o cliente que o app fechou. Guarda só os últimos.
static TERMINATED_BY_APP: LazyLock<Mutex<std::collections::VecDeque<u32>>> =
    LazyLock::new(|| Mutex::new(std::collections::VecDeque::new()));
const TERMINATED_BY_APP_MEMORY: usize = 64;

fn note_terminated_by_app(pid: u32) {
    if let Ok(mut pids) = TERMINATED_BY_APP.lock() {
        pids.retain(|p| *p != pid);
        pids.push_back(pid);
        while pids.len() > TERMINATED_BY_APP_MEMORY {
            pids.pop_front();
        }
    }
}

/// PIDs que o app fechou por estarem doentes (travado, memória, sem conexão),
/// não por pedido de alguém. A reconexão os trata como queda.
static CLOSED_AS_UNHEALTHY: LazyLock<Mutex<std::collections::VecDeque<u32>>> =
    LazyLock::new(|| Mutex::new(std::collections::VecDeque::new()));

/// Anotar **antes** de fechar: a passada do monitor pode ver o processo morto
/// logo em seguida.
pub fn note_closed_as_unhealthy(pid: u32) {
    if let Ok(mut pids) = CLOSED_AS_UNHEALTHY.lock() {
        pids.retain(|p| *p != pid);
        pids.push_back(pid);
        while pids.len() > TERMINATED_BY_APP_MEMORY {
            pids.pop_front();
        }
    }
}

pub fn was_closed_as_unhealthy(pid: u32) -> bool {
    CLOSED_AS_UNHEALTHY
        .lock()
        .map(|pids| pids.contains(&pid))
        .unwrap_or(false)
}

pub fn was_terminated_by_app(pid: u32) -> bool {
    TERMINATED_BY_APP
        .lock()
        .map(|pids| pids.contains(&pid))
        .unwrap_or(false)
}

pub fn kill_process(pid: u32) -> Result<(), String> {
    note_terminated_by_app(pid);
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if handle.is_null() {
            return Err(format!("Failed to open process {}", pid));
        }
        let result = TerminateProcess(handle, 1);
        CloseHandle(handle);
        if result == 0 {
            return Err(format!("Failed to terminate process {}", pid));
        }
    }
    Ok(())
}

pub fn kill_all_roblox() -> u32 {
    let pids = get_roblox_pids();
    let mut killed = 0u32;
    for pid in pids {
        if kill_process(pid).is_ok() {
            killed += 1;
        }
    }
    killed
}
