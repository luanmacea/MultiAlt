const SINGLE_INSTANCE_SEMAPHORE: &str = "/RobloxPlayerUniq";

static MULTI_ROBLOX_ENABLED: AtomicBool = AtomicBool::new(false);
static TRACKER: LazyLock<ProcessTracker> = LazyLock::new(ProcessTracker::new);

unsafe extern "C" {
    fn sem_unlink(name: *const c_char) -> c_int;
}

pub fn tracker() -> &'static ProcessTracker {
    &TRACKER
}

pub fn generate_browser_tracker_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let a = (now % 75000 + 100000) as u64;
    let b = ((now / 31) % 800000 + 100000) as u64;
    format!("{}{}", a, b)
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn clear_single_instance_semaphore() -> Result<(), String> {
    let name = CString::new(SINGLE_INSTANCE_SEMAPHORE)
        .map_err(|_| "Invalid semaphore name bytes".to_string())?;
    let result = unsafe { sem_unlink(name.as_ptr()) };
    if result == 0 {
        return Ok(());
    }

    let err = std::io::Error::last_os_error();
    if err.raw_os_error() == Some(2) {
        Ok(())
    } else {
        Err(format!(
            "Failed to clear Roblox single-instance semaphore: {}",
            err
        ))
    }
}

fn pre_launch_multi_step() {
    if MULTI_ROBLOX_ENABLED.load(Ordering::Relaxed) {
        let _ = clear_single_instance_semaphore();
    }
}

pub fn enable_multi_roblox() -> Result<bool, String> {
    MULTI_ROBLOX_ENABLED.store(true, Ordering::Relaxed);
    let _ = clear_single_instance_semaphore();
    Ok(true)
}

pub fn disable_multi_roblox() -> Result<(), String> {
    MULTI_ROBLOX_ENABLED.store(false, Ordering::Relaxed);
    Ok(())
}

pub fn get_roblox_path() -> Result<String, String> {
    let mut candidates = vec![
        PathBuf::from("/Applications/Roblox.app"),
        PathBuf::from("/Applications/RobloxPlayer.app"),
    ];

    if let Some(home) = home_dir() {
        candidates.push(home.join("Applications/Roblox.app"));
        candidates.push(home.join("Applications/RobloxPlayer.app"));
    }

    for path in candidates {
        if path.exists() {
            return Ok(path.to_string_lossy().into_owned());
        }
    }

    Err("Roblox app not found (looked in /Applications and ~/Applications)".into())
}

fn parse_pid_output(output: &str) -> Vec<u32> {
    output
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .collect()
}

pub fn get_roblox_pids() -> Vec<u32> {
    let mut all = HashSet::new();
    for name in ["RobloxPlayer", "Roblox"] {
        let output = Command::new("pgrep").args(["-x", name]).output();
        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for pid in parse_pid_output(&stdout) {
                all.insert(pid);
            }
        }
    }

    let mut pids: Vec<u32> = all.into_iter().collect();
    pids.sort_unstable();
    pids
}

pub fn kill_process(pid: u32) -> Result<(), String> {
    let pid_str = pid.to_string();
    let status = Command::new("kill")
        .args(["-TERM", &pid_str])
        .status()
        .map_err(|e| format!("Failed to send TERM to {}: {}", pid, e))?;
    if !status.success() {
        return Err(format!("Failed to send TERM to process {}", pid));
    }

    std::thread::sleep(std::time::Duration::from_millis(250));

    if get_roblox_pids().contains(&pid) {
        let kill_status = Command::new("kill")
            .args(["-KILL", &pid_str])
            .status()
            .map_err(|e| format!("Failed to send KILL to {}: {}", pid, e))?;
        if !kill_status.success() {
            return Err(format!("Failed to force kill process {}", pid));
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

