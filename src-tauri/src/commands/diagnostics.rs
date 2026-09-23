#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MutexDiagnosis {
    holder: &'static str,
    roblox_pids: Vec<u32>,
    legacy_ram_pids: Vec<u32>,
    this_process_holds: bool,
}

#[tauri::command]
fn kill_legacy_ram_processes() -> Result<u32, String> {
    #[cfg(target_os = "windows")]
    {
        let pids = platform::windows::find_legacy_ram_pids();
        let mut killed = 0u32;
        for pid in pids {
            if platform::windows::kill_process(pid).is_ok() {
                killed += 1;
            }
        }
        if killed > 0 {
            std::thread::sleep(std::time::Duration::from_millis(400));
        }
        Ok(killed)
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(0)
    }
}

/// Who owns the Roblox singleton mutex, in priority order: this app first (it
/// holds the handle itself), then a running client, then the legacy manager.
fn mutex_holder_label(
    this_process_holds: bool,
    roblox_pids: &[u32],
    legacy_ram_pids: &[u32],
) -> &'static str {
    if this_process_holds {
        "thisProcess"
    } else if !roblox_pids.is_empty() {
        "roblox"
    } else if !legacy_ram_pids.is_empty() {
        "legacyRam"
    } else {
        "free"
    }
}

#[tauri::command]
fn diagnose_mutex_holder() -> Result<MutexDiagnosis, String> {
    #[cfg(target_os = "windows")]
    {
        let roblox_pids = platform::windows::get_roblox_pids();
        let legacy_ram_pids = platform::windows::find_legacy_ram_pids();
        let this_process_holds = platform::windows::this_process_holds_multi_roblox();

        let holder = mutex_holder_label(this_process_holds, &roblox_pids, &legacy_ram_pids);

        Ok(MutexDiagnosis {
            holder,
            roblox_pids,
            legacy_ram_pids,
            this_process_holds,
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(MutexDiagnosis {
            holder: "free",
            roblox_pids: Vec::new(),
            legacy_ram_pids: Vec::new(),
            this_process_holds: false,
        })
    }
}
