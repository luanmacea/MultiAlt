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

pub fn get_roblox_pids() -> Vec<u32> {
    find_pids_for_exes(&["RobloxPlayerBeta.exe"])
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

pub fn kill_process(pid: u32) -> Result<(), String> {
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

#[cfg(test)]
mod win_process_tests {
    use super::*;

    // Enumeration only. `kill_process` / `kill_all_roblox` terminate real
    // processes and are deliberately never called from a test.

    #[test]
    fn find_pids_for_exes_returns_nothing_for_a_name_that_cannot_exist() {
        let pids = find_pids_for_exes(&["ram4-no-such-process-9f3c1d.exe"]);
        assert!(pids.is_empty(), "unexpected matches: {:?}", pids);
    }

    #[test]
    fn find_pids_for_exes_returns_nothing_for_an_empty_needle_list() {
        assert!(find_pids_for_exes(&[]).is_empty());
    }

    #[test]
    fn find_pids_for_exes_matches_the_current_executable_case_insensitively() {
        let Some(name) = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        else {
            return;
        };
        let self_pid = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() };

        assert!(
            find_pids_for_exes(&[&name]).contains(&self_pid),
            "the test runner {} should have been found",
            name
        );
        assert!(
            find_pids_for_exes(&[&name.to_ascii_uppercase()]).contains(&self_pid),
            "matching must ignore case"
        );
    }

    #[test]
    fn find_pids_for_exes_does_not_report_a_pid_twice_for_one_process() {
        // The inner loop breaks on the first matching needle, so listing the
        // same exe twice must not double-count it.
        let Some(name) = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        else {
            return;
        };
        let self_pid = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() };
        let pids = find_pids_for_exes(&[&name, &name.to_ascii_uppercase()]);
        assert_eq!(
            pids.iter().filter(|p| **p == self_pid).count(),
            1,
            "the current process was counted more than once"
        );
    }

    #[test]
    fn get_roblox_pids_is_a_subset_of_find_roblox_pids_all() {
        // find_roblox_pids_all widens the player match with the launcher and
        // the crash handler, so it can only ever be a superset.
        let players = get_roblox_pids();
        let all = find_roblox_pids_all();
        for pid in &players {
            assert!(
                all.contains(pid),
                "pid {} is a player but missing from the full list",
                pid
            );
        }
    }

    #[test]
    fn find_legacy_ram_pids_never_reports_the_current_process() {
        let self_pid = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() };
        assert!(
            !find_legacy_ram_pids().contains(&self_pid),
            "the running app must never be listed as a legacy install"
        );
    }
}
