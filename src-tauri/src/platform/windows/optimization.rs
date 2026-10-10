use crate::data::settings::SettingsStore;
use crate::LaunchClientProfile;
use std::mem::{size_of, zeroed};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject,
    JOBOBJECT_CPU_RATE_CONTROL_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOBOBJECT_CPU_RATE_CONTROL_INFORMATION_0, JOB_OBJECT_CPU_RATE_CONTROL_ENABLE,
    JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    JobObjectCpuRateControlInformation, JobObjectExtendedLimitInformation,
};
use windows_sys::Win32::System::Threading::{
    SetPriorityClass, SetProcessInformation, BELOW_NORMAL_PRIORITY_CLASS, IDLE_PRIORITY_CLASS,
    MEMORY_PRIORITY_INFORMATION, NORMAL_PRIORITY_CLASS,
    PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION, PROCESS_POWER_THROTTLING_STATE,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION, PROCESS_SET_QUOTA,
    ProcessMemoryPriority, ProcessPowerThrottling,
};

const MEMORY_PRIORITY_VERY_LOW: u32 = 1;
const MEMORY_PRIORITY_LOW: u32 = 3;
const MEMORY_PRIORITY_NORMAL: u32 = 5;

pub(crate) const WINDOWS_FASTFLAG_ALLOWLIST: &[&str] = &[
    "DFFlagTextureQualityOverrideEnabled",
    "DFIntTextureQualityOverride",
    "DFFlagDebugRenderForceTechnologyVoxel",
    "DFFlagDebugRenderForceTechnologyFuture",
    "DFFlagRenderForceLowQualityLightmaps",
    "DFFlagDisableDPIScale",
    "FFlagDebugGraphicsDisableDirect3D11",
    "FFlagDebugGraphicsPreferD3D11FL10",
    "FIntDebugForceMSAASamples",
    "FFlagDebugSkyGray",
    "DFFlagDebugPauseVoxelizer",
    "DFFlagDebugRenderForceMoonAngularSize",
    "DFIntDebugRenderForceMoonTextureSize",
    "DFIntDebugFRMQualityLevelOverride",
    "DFIntRenderShadowIntensity",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowsPriorityClass {
    Normal,
    BelowNormal,
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowsMemoryPriority {
    Normal,
    Low,
    VeryLow,
}

#[derive(Debug, Clone)]
pub(crate) struct WindowsProcessPolicy {
    pub enabled: bool,
    pub delay_ms: u64,
    pub priority_class: WindowsPriorityClass,
    pub background_mode: bool,
    pub eco_qos: bool,
    pub ignore_timer_resolution: bool,
    pub memory_priority: WindowsMemoryPriority,
}

impl Default for WindowsProcessPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            delay_ms: 1500,
            priority_class: WindowsPriorityClass::Normal,
            background_mode: false,
            eco_qos: false,
            ignore_timer_resolution: false,
            memory_priority: WindowsMemoryPriority::Normal,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WindowsExperimentalPolicy {
    pub enable_fast_flags: bool,
    pub fast_flags_json: String,
    pub enable_job_cpu_limit: bool,
    pub job_cpu_limit_percent: u32,
    pub enable_job_memory_limit: bool,
    pub job_memory_limit_mb: u64,
}

impl Default for WindowsExperimentalPolicy {
    fn default() -> Self {
        Self {
            enable_fast_flags: false,
            fast_flags_json: String::new(),
            enable_job_cpu_limit: false,
            job_cpu_limit_percent: 25,
            enable_job_memory_limit: false,
            job_memory_limit_mb: 2048,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct WindowsOptimizationProfile {
    pub process: WindowsProcessPolicy,
    pub experimental: WindowsExperimentalPolicy,
}

fn optimization_prefix(profile: LaunchClientProfile) -> &'static str {
    match profile {
        LaunchClientProfile::Normal => "Normal",
        LaunchClientProfile::BottingPlayer => "BottingPlayer",
        LaunchClientProfile::BottingBot => "BottingBot",
    }
}

fn optimization_key(profile: LaunchClientProfile, suffix: &str) -> String {
    format!("{}{}", optimization_prefix(profile), suffix)
}

fn optimization_clamped_u64(
    settings: &SettingsStore,
    profile: LaunchClientProfile,
    suffix: &str,
    default: i64,
    min: i64,
    max: i64,
) -> u64 {
    settings
        .get_int("Optimization", &optimization_key(profile, suffix))
        .unwrap_or(default)
        .clamp(min, max) as u64
}

fn optimization_clamped_u32(
    settings: &SettingsStore,
    profile: LaunchClientProfile,
    suffix: &str,
    default: i64,
    min: i64,
    max: i64,
) -> u32 {
    optimization_clamped_u64(settings, profile, suffix, default, min, max) as u32
}

fn optimization_bool(settings: &SettingsStore, profile: LaunchClientProfile, suffix: &str) -> bool {
    settings.get_bool("Optimization", &optimization_key(profile, suffix))
}

fn optimization_string(
    settings: &SettingsStore,
    profile: LaunchClientProfile,
    suffix: &str,
) -> String {
    settings.get_string("Optimization", &optimization_key(profile, suffix))
}

fn parse_priority_class(value: &str) -> WindowsPriorityClass {
    match value.trim().to_ascii_lowercase().as_str() {
        "below_normal" => WindowsPriorityClass::BelowNormal,
        "idle" => WindowsPriorityClass::Idle,
        _ => WindowsPriorityClass::Normal,
    }
}

fn parse_memory_priority(value: &str) -> WindowsMemoryPriority {
    match value.trim().to_ascii_lowercase().as_str() {
        "low" => WindowsMemoryPriority::Low,
        "very_low" => WindowsMemoryPriority::VeryLow,
        _ => WindowsMemoryPriority::Normal,
    }
}

pub(crate) fn load_optimization_profile(
    settings: &SettingsStore,
    profile: LaunchClientProfile,
) -> WindowsOptimizationProfile {
    WindowsOptimizationProfile {
        process: WindowsProcessPolicy {
            enabled: optimization_bool(settings, profile, "EnableProcessPolicy"),
            delay_ms: optimization_clamped_u64(
                settings,
                profile,
                "ProcessPolicyDelayMs",
                1500,
                0,
                15000,
            ),
            priority_class: parse_priority_class(&optimization_string(
                settings,
                profile,
                "PriorityClass",
            )),
            background_mode: optimization_bool(settings, profile, "BackgroundMode"),
            eco_qos: optimization_bool(settings, profile, "EcoQos"),
            ignore_timer_resolution: optimization_bool(
                settings,
                profile,
                "IgnoreTimerResolution",
            ),
            memory_priority: parse_memory_priority(&optimization_string(
                settings,
                profile,
                "MemoryPriority",
            )),
        },
        experimental: WindowsExperimentalPolicy {
            enable_fast_flags: optimization_bool(settings, profile, "EnableFastFlags"),
            fast_flags_json: optimization_string(settings, profile, "FastFlagsJson"),
            enable_job_cpu_limit: optimization_bool(settings, profile, "EnableJobCpuLimit"),
            job_cpu_limit_percent: optimization_clamped_u32(
                settings,
                profile,
                "JobCpuLimitPercent",
                25,
                5,
                100,
            ),
            enable_job_memory_limit: optimization_bool(
                settings,
                profile,
                "EnableJobMemoryLimit",
            ),
            job_memory_limit_mb: optimization_clamped_u64(
                settings,
                profile,
                "JobMemoryLimitMb",
                2048,
                256,
                32768,
            ),
        },
    }
}

pub(crate) fn parse_allowlisted_fast_flags_json(
    payload: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let trimmed = payload.trim();
    if trimmed.is_empty() {
        return Err("Allowlisted fast flags JSON cannot be empty while enabled".into());
    }

    let value: serde_json::Value = serde_json::from_str(trimmed)
        .map_err(|e| format!("Invalid allowlisted fast flags JSON: {}", e))?;
    let object = value
        .as_object()
        .ok_or_else(|| "Allowlisted fast flags JSON must be a JSON object".to_string())?;

    let invalid_keys: Vec<String> = object
        .keys()
        .filter(|key| !WINDOWS_FASTFLAG_ALLOWLIST.contains(&key.as_str()))
        .cloned()
        .collect();

    if !invalid_keys.is_empty() {
        return Err(format!(
            "Only Roblox allowlisted keys are accepted: {}",
            invalid_keys.join(", ")
        ));
    }

    Ok(object.clone())
}

fn power_throttling_mask(profile: &WindowsProcessPolicy) -> u32 {
    let mut mask = 0u32;
    if profile.eco_qos {
        mask |= PROCESS_POWER_THROTTLING_EXECUTION_SPEED;
    }
    if profile.ignore_timer_resolution {
        mask |= PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION;
    }
    mask
}

fn apply_priority_class(process: HANDLE, profile: &WindowsProcessPolicy) -> Result<(), String> {
    unsafe {
        let class = if profile.background_mode {
            IDLE_PRIORITY_CLASS
        } else {
            match profile.priority_class {
                WindowsPriorityClass::Normal => NORMAL_PRIORITY_CLASS,
                WindowsPriorityClass::BelowNormal => BELOW_NORMAL_PRIORITY_CLASS,
                WindowsPriorityClass::Idle => IDLE_PRIORITY_CLASS,
            }
        };
        if SetPriorityClass(process, class) == 0 {
            return Err("Failed to set process priority class".into());
        }
    }

    Ok(())
}

fn apply_memory_priority(process: HANDLE, profile: &WindowsProcessPolicy) -> Result<(), String> {
    let memory_priority = match profile.memory_priority {
        WindowsMemoryPriority::Normal => MEMORY_PRIORITY_NORMAL,
        WindowsMemoryPriority::Low => MEMORY_PRIORITY_LOW,
        WindowsMemoryPriority::VeryLow => MEMORY_PRIORITY_VERY_LOW,
    };

    let info = MEMORY_PRIORITY_INFORMATION {
        MemoryPriority: memory_priority,
    };

    unsafe {
        if SetProcessInformation(
            process,
            ProcessMemoryPriority,
            &info as *const _ as *const _,
            size_of::<MEMORY_PRIORITY_INFORMATION>() as u32,
        ) == 0
        {
            return Err("Failed to set process memory priority".into());
        }
    }

    Ok(())
}

fn apply_power_throttling(process: HANDLE, profile: &WindowsProcessPolicy) -> Result<(), String> {
    let mask = power_throttling_mask(profile);
    if mask == 0 {
        return Ok(());
    }

    let state = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: mask,
        StateMask: mask,
    };

    unsafe {
        if SetProcessInformation(
            process,
            ProcessPowerThrottling,
            &state as *const _ as *const _,
            size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        ) == 0
        {
            return Err("Failed to set process power throttling".into());
        }
    }

    Ok(())
}

fn build_job_handle(experimental: &WindowsExperimentalPolicy) -> Result<Option<HANDLE>, String> {
    if !experimental.enable_job_cpu_limit && !experimental.enable_job_memory_limit {
        return Ok(None);
    }

    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() {
        return Err("Failed to create job object".into());
    }

    if experimental.enable_job_cpu_limit {
        let cpu_info = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION {
            ControlFlags: JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP,
            Anonymous: JOBOBJECT_CPU_RATE_CONTROL_INFORMATION_0 {
                CpuRate: experimental.job_cpu_limit_percent.saturating_mul(100),
            },
        };

        unsafe {
            if SetInformationJobObject(
                job,
                JobObjectCpuRateControlInformation,
                &cpu_info as *const _ as *const _,
                size_of::<JOBOBJECT_CPU_RATE_CONTROL_INFORMATION>() as u32,
            ) == 0
            {
                CloseHandle(job);
                return Err("Failed to configure job CPU limit".into());
            }
        }
    }

    if experimental.enable_job_memory_limit {
        let mut limit_info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limit_info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_PROCESS_MEMORY;
        limit_info.ProcessMemoryLimit =
            experimental.job_memory_limit_mb.saturating_mul(1024 * 1024) as usize;

        unsafe {
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limit_info as *const _ as *const _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
            {
                CloseHandle(job);
                return Err("Failed to configure job memory limit".into());
            }
        }
    }

    Ok(Some(job))
}

pub(crate) fn apply_optimization_to_pid(
    pid: u32,
    profile: &WindowsOptimizationProfile,
) -> Result<(), String> {
    let needs_process_settings = profile.process.enabled;
    let needs_job = profile.experimental.enable_job_cpu_limit
        || profile.experimental.enable_job_memory_limit;

    if !needs_process_settings && !needs_job {
        tracker().clear_job_handle_for_pid(pid);
        return Ok(());
    }

    unsafe {
        let mut access = PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION;
        if needs_job {
            access |= PROCESS_SET_QUOTA | PROCESS_TERMINATE;
        }

        let process = OpenProcess(access, 0, pid);
        if process.is_null() {
            return Err(format!("Failed to open Roblox process {}", pid));
        }

        let result = (|| -> Result<(), String> {
            if profile.process.enabled {
                apply_priority_class(process, &profile.process)?;
                apply_power_throttling(process, &profile.process)?;
                apply_memory_priority(process, &profile.process)?;
            }

            if let Some(job) = build_job_handle(&profile.experimental)? {
                if AssignProcessToJobObject(job, process) == 0 {
                    CloseHandle(job);
                    return Err("Failed to assign Roblox process to job object".into());
                }
                tracker().set_job_handle(pid, job);
            } else {
                tracker().clear_job_handle_for_pid(pid);
            }

            Ok(())
        })();

        CloseHandle(process);
        result
    }
}

/// O par `(ControlMask, StateMask)` do power throttling para uma troca de
/// velocidade ao vivo. Sem EcoQoS nem timer, `(0, 0)` devolve a decisão ao
/// Windows — o estado de um processo que ninguém mexeu. (O launch continua
/// pulando a chamada nesse caso: `apply_power_throttling`.)
fn live_power_throttling_masks(profile: &WindowsProcessPolicy) -> (u32, u32) {
    let mask = power_throttling_mask(profile);
    (mask, mask)
}

/// Prioridade, power throttling e prioridade de memória num processo já
/// aberto — a troca de velocidade da otimização que segue o foco
/// (`focus_follow.rs`). Mesmas APIs do launch; nada novo no binário.
pub(crate) fn apply_process_policy_live(
    pid: u32,
    profile: &WindowsProcessPolicy,
) -> Result<(), String> {
    unsafe {
        let process = OpenProcess(
            PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        );
        if process.is_null() {
            return Err(format!("Failed to open Roblox process {}", pid));
        }
        let result = (|| -> Result<(), String> {
            apply_priority_class(process, profile)?;
            let (control, state) = live_power_throttling_masks(profile);
            let throttling = PROCESS_POWER_THROTTLING_STATE {
                Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
                ControlMask: control,
                StateMask: state,
            };
            if SetProcessInformation(
                process,
                ProcessPowerThrottling,
                &throttling as *const _ as *const _,
                size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            ) == 0
            {
                return Err("Failed to set process power throttling".into());
            }
            apply_memory_priority(process, profile)
        })();
        CloseHandle(process);
        result
    }
}

/// Os flags do teto de CPU do Job: `Some(p)` liga o teto rígido em `p`%,
/// `None` desliga (sem `JOB_OBJECT_CPU_RATE_CONTROL_ENABLE` o Windows ignora o
/// resto).
fn job_cpu_cap_info(cap: Option<u32>) -> (u32, u32) {
    match cap {
        Some(percent) => (
            JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP,
            percent.clamp(5, 100).saturating_mul(100),
        ),
        None => (0, 0),
    }
}

/// Liga/desliga o teto de CPU do Job que o launch criou para o PID. `Ok(false)`
/// quando o cliente não tem Job (o perfil não pediu teto).
pub(crate) fn set_job_cpu_cap(pid: u32, cap: Option<u32>) -> Result<bool, String> {
    let (flags, rate) = job_cpu_cap_info(cap);
    let info = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION {
        ControlFlags: flags,
        Anonymous: JOBOBJECT_CPU_RATE_CONTROL_INFORMATION_0 { CpuRate: rate },
    };
    let applied = tracker().with_job_handle(pid, |job| unsafe {
        SetInformationJobObject(
            job,
            JobObjectCpuRateControlInformation,
            &info as *const _ as *const _,
            size_of::<JOBOBJECT_CPU_RATE_CONTROL_INFORMATION>() as u32,
        ) != 0
    });
    match applied {
        None => Ok(false),
        Some(true) => Ok(true),
        Some(false) => Err("Failed to change job CPU limit".into()),
    }
}

#[cfg(test)]
mod win_optimization_tests {
    use super::*;

    // Only the configuration side is covered here: every function below maps
    // settings to plain values. The Win32 appliers (`apply_optimization_to_pid`
    // and friends) need a live process handle and are left to manual testing.

    struct TempSettings {
        store: SettingsStore,
        path: PathBuf,
    }

    impl Drop for TempSettings {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    impl std::ops::Deref for TempSettings {
        type Target = SettingsStore;
        fn deref(&self) -> &SettingsStore {
            &self.store
        }
    }

    fn settings(tag: &str) -> TempSettings {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("ram4-opt-{}-{}-{}.ini", tag, nanos, n));
        TempSettings {
            store: SettingsStore::new(path.clone()),
            path,
        }
    }

    fn policy(background: bool, eco: bool, ignore_timer: bool) -> WindowsProcessPolicy {
        WindowsProcessPolicy {
            enabled: true,
            background_mode: background,
            eco_qos: eco,
            ignore_timer_resolution: ignore_timer,
            ..Default::default()
        }
    }

    // ── key composition ────────────────────────────────────────────────────

    #[test]
    fn optimization_prefix_is_distinct_per_launch_profile() {
        assert_eq!(optimization_prefix(LaunchClientProfile::Normal), "Normal");
        assert_eq!(
            optimization_prefix(LaunchClientProfile::BottingPlayer),
            "BottingPlayer"
        );
        assert_eq!(
            optimization_prefix(LaunchClientProfile::BottingBot),
            "BottingBot"
        );
    }

    #[test]
    fn optimization_key_concatenates_the_prefix_and_the_suffix() {
        assert_eq!(
            optimization_key(LaunchClientProfile::Normal, "PriorityClass"),
            "NormalPriorityClass"
        );
        assert_eq!(
            optimization_key(LaunchClientProfile::BottingBot, "EnableJobCpuLimit"),
            "BottingBotEnableJobCpuLimit"
        );
    }

    #[test]
    fn the_three_profiles_never_share_a_settings_key() {
        let profiles = [
            LaunchClientProfile::Normal,
            LaunchClientProfile::BottingPlayer,
            LaunchClientProfile::BottingBot,
        ];
        let mut seen = std::collections::HashSet::new();
        for profile in profiles {
            for suffix in [
                "EnableProcessPolicy",
                "ProcessPolicyDelayMs",
                "PriorityClass",
                "BackgroundMode",
                "EcoQos",
                "IgnoreTimerResolution",
                "MemoryPriority",
                "EnableFastFlags",
                "FastFlagsJson",
                "EnableJobCpuLimit",
                "JobCpuLimitPercent",
                "EnableJobMemoryLimit",
                "JobMemoryLimitMb",
            ] {
                assert!(
                    seen.insert(optimization_key(profile, suffix)),
                    "duplicate key for {:?}/{}",
                    profile,
                    suffix
                );
            }
        }
    }

    // ── enum parsing tables ────────────────────────────────────────────────

    #[test]
    fn parse_priority_class_maps_the_known_values() {
        assert_eq!(parse_priority_class("normal"), WindowsPriorityClass::Normal);
        assert_eq!(
            parse_priority_class("below_normal"),
            WindowsPriorityClass::BelowNormal
        );
        assert_eq!(parse_priority_class("idle"), WindowsPriorityClass::Idle);
    }

    #[test]
    fn parse_priority_class_ignores_case_and_whitespace() {
        assert_eq!(
            parse_priority_class("  BELOW_NORMAL "),
            WindowsPriorityClass::BelowNormal
        );
        assert_eq!(parse_priority_class("\tIdle\n"), WindowsPriorityClass::Idle);
    }

    #[test]
    fn parse_priority_class_falls_back_to_normal() {
        for value in ["", "   ", "high", "realtime", "below normal", "below-normal"] {
            assert_eq!(
                parse_priority_class(value),
                WindowsPriorityClass::Normal,
                "{:?} should fall back to Normal",
                value
            );
        }
    }

    #[test]
    fn parse_memory_priority_maps_the_known_values_and_falls_back_to_normal() {
        assert_eq!(parse_memory_priority("low"), WindowsMemoryPriority::Low);
        assert_eq!(
            parse_memory_priority("very_low"),
            WindowsMemoryPriority::VeryLow
        );
        assert_eq!(parse_memory_priority("normal"), WindowsMemoryPriority::Normal);
        assert_eq!(
            parse_memory_priority("  VERY_LOW  "),
            WindowsMemoryPriority::VeryLow
        );
        for value in ["", "very low", "verylow", "lowest", "5"] {
            assert_eq!(
                parse_memory_priority(value),
                WindowsMemoryPriority::Normal,
                "{:?} should fall back to Normal",
                value
            );
        }
    }

    // ── power throttling mask ──────────────────────────────────────────────

    #[test]
    fn power_throttling_mask_is_the_or_of_the_two_toggles() {
        assert_eq!(power_throttling_mask(&policy(false, false, false)), 0);
        assert_eq!(
            power_throttling_mask(&policy(false, true, false)),
            PROCESS_POWER_THROTTLING_EXECUTION_SPEED
        );
        assert_eq!(
            power_throttling_mask(&policy(false, false, true)),
            PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION
        );
        assert_eq!(
            power_throttling_mask(&policy(false, true, true)),
            PROCESS_POWER_THROTTLING_EXECUTION_SPEED
                | PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION
        );
    }

    #[test]
    fn power_throttling_mask_is_unaffected_by_background_mode() {
        assert_eq!(power_throttling_mask(&policy(true, false, false)), 0);
    }

    // ── defaults ───────────────────────────────────────────────────────────

    #[test]
    fn process_policy_defaults_are_inert() {
        let default = WindowsProcessPolicy::default();
        assert!(!default.enabled);
        assert_eq!(default.delay_ms, 1500);
        assert_eq!(default.priority_class, WindowsPriorityClass::Normal);
        assert!(!default.background_mode);
        assert!(!default.eco_qos);
        assert!(!default.ignore_timer_resolution);
        assert_eq!(default.memory_priority, WindowsMemoryPriority::Normal);
    }

    #[test]
    fn experimental_policy_defaults_are_inert() {
        let default = WindowsExperimentalPolicy::default();
        assert!(!default.enable_fast_flags);
        assert!(default.fast_flags_json.is_empty());
        assert!(!default.enable_job_cpu_limit);
        assert_eq!(default.job_cpu_limit_percent, 25);
        assert!(!default.enable_job_memory_limit);
        assert_eq!(default.job_memory_limit_mb, 2048);
    }

    // ── load_optimization_profile ──────────────────────────────────────────

    #[test]
    fn load_optimization_profile_uses_defaults_for_an_untouched_settings_file() {
        let store = settings("defaults");
        let profile = load_optimization_profile(&store, LaunchClientProfile::Normal);

        assert!(!profile.process.enabled);
        assert_eq!(profile.process.delay_ms, 1500);
        assert_eq!(profile.process.priority_class, WindowsPriorityClass::Normal);
        assert_eq!(
            profile.process.memory_priority,
            WindowsMemoryPriority::Normal
        );
        assert_eq!(profile.experimental.job_cpu_limit_percent, 25);
        assert_eq!(profile.experimental.job_memory_limit_mb, 2048);
    }

    #[test]
    fn load_optimization_profile_reads_the_keys_of_the_requested_profile_only() {
        let store = settings("perprofile");
        store
            .set("Optimization", "BottingBotEnableProcessPolicy", "true")
            .unwrap();
        store
            .set("Optimization", "BottingBotPriorityClass", "idle")
            .unwrap();
        store
            .set("Optimization", "BottingBotMemoryPriority", "very_low")
            .unwrap();

        let bot = load_optimization_profile(&store, LaunchClientProfile::BottingBot);
        assert!(bot.process.enabled);
        assert_eq!(bot.process.priority_class, WindowsPriorityClass::Idle);
        assert_eq!(bot.process.memory_priority, WindowsMemoryPriority::VeryLow);

        let normal = load_optimization_profile(&store, LaunchClientProfile::Normal);
        assert!(!normal.process.enabled);
        assert_eq!(normal.process.priority_class, WindowsPriorityClass::Normal);
    }

    #[test]
    fn load_optimization_profile_clamps_the_process_policy_delay() {
        let store = settings("clampdelay");
        for (stored, expected) in [
            ("-5000", 0u64),
            ("0", 0),
            ("1200", 1200),
            ("15000", 15000),
            ("999999", 15000),
        ] {
            store
                .set("Optimization", "NormalProcessPolicyDelayMs", stored)
                .unwrap();
            let profile = load_optimization_profile(&store, LaunchClientProfile::Normal);
            assert_eq!(profile.process.delay_ms, expected, "stored {}", stored);
        }
    }

    #[test]
    fn load_optimization_profile_clamps_the_cpu_limit_percentage() {
        let store = settings("clampcpu");
        for (stored, expected) in [
            ("0", 5u32),
            ("-100", 5),
            ("5", 5),
            ("50", 50),
            ("100", 100),
            ("1000", 100),
        ] {
            store
                .set("Optimization", "NormalJobCpuLimitPercent", stored)
                .unwrap();
            let profile = load_optimization_profile(&store, LaunchClientProfile::Normal);
            assert_eq!(
                profile.experimental.job_cpu_limit_percent, expected,
                "stored {}",
                stored
            );
        }
    }

    #[test]
    fn load_optimization_profile_clamps_the_memory_limit() {
        let store = settings("clampmem");
        for (stored, expected) in [
            ("0", 256u64),
            ("-1", 256),
            ("255", 256),
            ("256", 256),
            ("4096", 4096),
            ("32768", 32768),
            ("99999999", 32768),
        ] {
            store
                .set("Optimization", "NormalJobMemoryLimitMb", stored)
                .unwrap();
            let profile = load_optimization_profile(&store, LaunchClientProfile::Normal);
            assert_eq!(
                profile.experimental.job_memory_limit_mb, expected,
                "stored {}",
                stored
            );
        }
    }

    #[test]
    fn load_optimization_profile_falls_back_to_the_default_for_an_unparsable_number() {
        let store = settings("badnumber");
        store
            .set("Optimization", "NormalProcessPolicyDelayMs", "soon")
            .unwrap();
        store
            .set("Optimization", "NormalJobMemoryLimitMb", "lots")
            .unwrap();

        let profile = load_optimization_profile(&store, LaunchClientProfile::Normal);
        assert_eq!(profile.process.delay_ms, 1500);
        assert_eq!(profile.experimental.job_memory_limit_mb, 2048);
    }

    #[test]
    fn load_optimization_profile_carries_the_experimental_toggles_through() {
        let store = settings("experimental");
        store
            .set("Optimization", "BottingPlayerEnableFastFlags", "true")
            .unwrap();
        store
            .set(
                "Optimization",
                "BottingPlayerFastFlagsJson",
                r#"{"FFlagDebugSkyGray":"True"}"#,
            )
            .unwrap();
        store
            .set("Optimization", "BottingPlayerEnableJobCpuLimit", "true")
            .unwrap();
        store
            .set("Optimization", "BottingPlayerEnableJobMemoryLimit", "true")
            .unwrap();

        let profile = load_optimization_profile(&store, LaunchClientProfile::BottingPlayer);
        assert!(profile.experimental.enable_fast_flags);
        assert_eq!(
            profile.experimental.fast_flags_json,
            r#"{"FFlagDebugSkyGray":"True"}"#
        );
        assert!(profile.experimental.enable_job_cpu_limit);
        assert!(profile.experimental.enable_job_memory_limit);
    }

    // ── fast flag allowlist ────────────────────────────────────────────────

    #[test]
    fn the_fastflag_allowlist_has_no_duplicates() {
        let unique: std::collections::HashSet<&&str> =
            WINDOWS_FASTFLAG_ALLOWLIST.iter().collect();
        assert_eq!(unique.len(), WINDOWS_FASTFLAG_ALLOWLIST.len());
    }

    #[test]
    fn parse_allowlisted_fast_flags_json_accepts_every_allowlisted_key() {
        let object: serde_json::Map<String, serde_json::Value> = WINDOWS_FASTFLAG_ALLOWLIST
            .iter()
            .map(|k| ((*k).to_string(), serde_json::json!("True")))
            .collect();
        let payload = serde_json::to_string(&object).unwrap();

        let parsed = parse_allowlisted_fast_flags_json(&payload).expect("allowlisted payload");
        assert_eq!(parsed.len(), WINDOWS_FASTFLAG_ALLOWLIST.len());
    }

    #[test]
    fn parse_allowlisted_fast_flags_json_accepts_surrounding_whitespace() {
        let parsed =
            parse_allowlisted_fast_flags_json("  \n {\"FFlagDebugSkyGray\": \"True\"} \t ")
                .expect("valid payload");
        assert_eq!(parsed["FFlagDebugSkyGray"], "True");
    }

    #[test]
    fn parse_allowlisted_fast_flags_json_accepts_an_empty_object() {
        let parsed = parse_allowlisted_fast_flags_json("{}").expect("empty object is valid");
        assert!(parsed.is_empty());
    }

    #[test]
    fn parse_allowlisted_fast_flags_json_rejects_an_empty_payload() {
        for payload in ["", "   ", "\n\t"] {
            let err = parse_allowlisted_fast_flags_json(payload).unwrap_err();
            assert_eq!(err, "Allowlisted fast flags JSON cannot be empty while enabled");
        }
    }

    #[test]
    fn parse_allowlisted_fast_flags_json_rejects_malformed_json() {
        let err = parse_allowlisted_fast_flags_json("{not json").unwrap_err();
        assert!(
            err.starts_with("Invalid allowlisted fast flags JSON"),
            "got {}",
            err
        );
    }

    #[test]
    fn parse_allowlisted_fast_flags_json_rejects_a_non_object_document() {
        for payload in ["[]", "\"a string\"", "42", "null", "true"] {
            let err = parse_allowlisted_fast_flags_json(payload).unwrap_err();
            assert_eq!(
                err, "Allowlisted fast flags JSON must be a JSON object",
                "payload {}",
                payload
            );
        }
    }

    #[test]
    fn a_live_switch_to_full_speed_hands_power_throttling_back_to_windows() {
        // (0, 0) = "o Windows decide", como num processo que ninguém mexeu.
        assert_eq!(live_power_throttling_masks(&policy(false, false, false)), (0, 0));
        let both = PROCESS_POWER_THROTTLING_EXECUTION_SPEED
            | PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION;
        assert_eq!(live_power_throttling_masks(&policy(false, true, true)), (both, both));
    }

    #[test]
    fn the_job_cpu_cap_is_switched_off_by_clearing_every_flag() {
        assert_eq!(job_cpu_cap_info(None), (0, 0));
        let (flags, rate) = job_cpu_cap_info(Some(30));
        assert_eq!(
            flags,
            JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP
        );
        // O Windows mede em centésimos de porcento.
        assert_eq!(rate, 3000);
        // Fora da faixa da tela (5–100) não passa.
        assert_eq!(job_cpu_cap_info(Some(0)).1, 500);
        assert_eq!(job_cpu_cap_info(Some(500)).1, 10000);
    }

    #[test]
    fn parse_allowlisted_fast_flags_json_rejects_keys_outside_the_allowlist() {
        // Security boundary: arbitrary fast flags must not reach the client.
        let err = parse_allowlisted_fast_flags_json(
            r#"{"FFlagDebugSkyGray":"True","FFlagArbitraryThing":"True"}"#,
        )
        .unwrap_err();
        assert!(err.starts_with("Only Roblox allowlisted keys are accepted:"));
        assert!(err.contains("FFlagArbitraryThing"));
        assert!(!err.contains("FFlagDebugSkyGray"));
    }

    #[test]
    fn parse_allowlisted_fast_flags_json_lists_every_rejected_key() {
        let err =
            parse_allowlisted_fast_flags_json(r#"{"BadOne":"1","BadTwo":"2"}"#).unwrap_err();
        assert!(err.contains("BadOne"), "got {}", err);
        assert!(err.contains("BadTwo"), "got {}", err);
    }

    #[test]
    fn parse_allowlisted_fast_flags_json_is_case_sensitive_about_key_names() {
        let err = parse_allowlisted_fast_flags_json(r#"{"fflagdebugskygray":"True"}"#).unwrap_err();
        assert!(err.contains("fflagdebugskygray"), "got {}", err);
    }
}
