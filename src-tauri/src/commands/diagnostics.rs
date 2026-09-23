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

#[cfg(test)]
mod diagnostics_tests {
    use super::*;

    #[test]
    fn mutex_holder_label_reports_free_when_nothing_holds_it() {
        assert_eq!(mutex_holder_label(false, &[], &[]), "free");
    }

    #[test]
    fn mutex_holder_label_reports_this_process_first() {
        // This app holding the handle wins over every other signal, otherwise
        // the UI would tell the user to close a client that is not the blocker.
        assert_eq!(mutex_holder_label(true, &[], &[]), "thisProcess");
        assert_eq!(mutex_holder_label(true, &[1234], &[5678]), "thisProcess");
    }

    #[test]
    fn mutex_holder_label_prefers_a_running_client_over_the_legacy_manager() {
        assert_eq!(mutex_holder_label(false, &[1234], &[5678]), "roblox");
        assert_eq!(mutex_holder_label(false, &[1234], &[]), "roblox");
    }

    #[test]
    fn mutex_holder_label_falls_back_to_the_legacy_manager() {
        assert_eq!(mutex_holder_label(false, &[], &[5678]), "legacyRam");
    }

    #[test]
    fn mutex_holder_label_treats_many_pids_the_same_as_one() {
        let many: Vec<u32> = (1..=500).collect();
        assert_eq!(mutex_holder_label(false, &many, &[]), "roblox");
        assert_eq!(mutex_holder_label(false, &[], &many), "legacyRam");
    }

    #[test]
    fn mutex_diagnosis_serializes_with_the_camel_case_keys_the_ui_reads() {
        let diagnosis = MutexDiagnosis {
            holder: "legacyRam",
            roblox_pids: vec![1, 2],
            legacy_ram_pids: vec![3],
            this_process_holds: false,
        };
        let json = serde_json::to_value(&diagnosis).unwrap();
        assert_eq!(json["holder"], "legacyRam");
        assert_eq!(json["robloxPids"], serde_json::json!([1, 2]));
        assert_eq!(json["legacyRamPids"], serde_json::json!([3]));
        assert_eq!(json["thisProcessHolds"], false);
    }

    #[test]
    fn diagnose_mutex_holder_is_self_consistent_on_this_machine() {
        // Read-only probe: whatever the machine state, the reported holder must
        // match the pid lists in the same payload.
        let diagnosis = diagnose_mutex_holder().expect("diagnosis should succeed");
        assert_eq!(
            diagnosis.holder,
            mutex_holder_label(
                diagnosis.this_process_holds,
                &diagnosis.roblox_pids,
                &diagnosis.legacy_ram_pids,
            )
        );
    }
}
