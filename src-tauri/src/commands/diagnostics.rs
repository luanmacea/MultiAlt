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

// â”€â”€ DiagnÃ³stico "o launch nÃ£o faz nada" (ideia 16) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//
// Uma lista de checagens para quem relata "clico e nada acontece". **SÃ³ lÃª**:
// nunca fecha cliente, nunca mexe em registro, nunca baixa build. A Ãºnica
// escrita Ã© o arquivo de prova da checagem de pasta, criado e apagado na hora.
//
// O backend devolve sÃ³ `id` + `reason` (+ um nÃºmero quando faz sentido); a
// frase que a pessoa lÃª sai do frontend (`src/utils/diagnostics.ts`), traduzida.
// Assim nada de caminho, nome de conta ou PID vai para a tela â€” e o resumo do
// "Reportar problema" (ideia 28) pode levar o resultado sem anonimizar nada.

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
enum CheckStatus {
    Ok,
    Warn,
    Problem,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticCheck {
    id: &'static str,
    status: CheckStatus,
    reason: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<u32>,
}

impl DiagnosticCheck {
    fn new(id: &'static str, status: CheckStatus, reason: &'static str) -> Self {
        Self { id, status, reason, count: None }
    }

    fn with_count(mut self, count: u32) -> Self {
        self.count = Some(count);
        self
    }
}

/// Um processo do Roblox sem janela hÃ¡ mais que isto conta como "preso": um
/// cliente subindo leva alguns segundos para mostrar a janela; 150 s Ã© o limite
/// que o RobloxKeeper (ideia 16) usa, folgado para mÃ¡quina lenta.
const STUCK_PROCESS_MIN_AGE_SECS: u64 = 150;

/// Hosts consultados na checagem de internet: pedidos sem conta, pequenos.
/// Qualquer resposta HTTP (atÃ© 404) prova que o Roblox Ã© alcanÃ§Ã¡vel.
const REACHABILITY_PROBES: &[(&str, &str)] = &[("users", "/v1/users/1"), ("auth", "/v2/metadata")];

fn install_check(build_found: bool) -> DiagnosticCheck {
    if build_found {
        DiagnosticCheck::new("robloxInstall", CheckStatus::Ok, "found")
    } else {
        // NÃ£o Ã© defeito por si: o launch baixa a build de produÃ§Ã£o sozinho.
        // Vira problema quando a internet tambÃ©m falha â€” a frase diz isso.
        DiagnosticCheck::new("robloxInstall", CheckStatus::Warn, "missing")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FolderProbe {
    Writable,
    /// A pasta ainda nÃ£o existe, mas a mais prÃ³xima que existe aceita escrita
    /// (o app a cria quando precisar).
    CanBeCreated,
    NotWritable,
    /// Nem dÃ¡ para saber onde fica (variÃ¡vel de ambiente faltando).
    Unknown,
}

fn folder_check(id: &'static str, probe: FolderProbe) -> DiagnosticCheck {
    match probe {
        FolderProbe::Writable | FolderProbe::CanBeCreated => DiagnosticCheck::new(id, CheckStatus::Ok, "writable"),
        FolderProbe::NotWritable => DiagnosticCheck::new(id, CheckStatus::Problem, "notWritable"),
        FolderProbe::Unknown => DiagnosticCheck::new(id, CheckStatus::Warn, "unknown"),
    }
}

/// Tenta criar e apagar um arquivo de prova em `dir` (ou na pasta mais
/// prÃ³xima que existir acima dela). NÃ£o cria a pasta: se ela nÃ£o existe, sÃ³
/// diz se daria para criar.
fn probe_folder_writable(dir: &std::path::Path) -> FolderProbe {
    let mut target = dir.to_path_buf();
    let mut exists = target.is_dir();
    while !exists {
        match target.parent() {
            Some(parent) if parent != target.as_path() => {
                target = parent.to_path_buf();
                exists = target.is_dir();
            }
            _ => return FolderProbe::NotWritable,
        }
    }
    let probe = target.join(format!(".multialt-write-check-{}.tmp", std::process::id()));
    let writable = std::fs::write(&probe, b"ok").is_ok();
    let _ = std::fs::remove_file(&probe);
    match (writable, target.as_path() == dir) {
        (false, _) => FolderProbe::NotWritable,
        (true, true) => FolderProbe::Writable,
        (true, false) => FolderProbe::CanBeCreated,
    }
}

fn internet_check(reached: usize, total: usize) -> DiagnosticCheck {
    if total == 0 || reached == total {
        DiagnosticCheck::new("internet", CheckStatus::Ok, "reachable")
    } else if reached == 0 {
        DiagnosticCheck::new("internet", CheckStatus::Problem, "unreachable")
    } else {
        DiagnosticCheck::new("internet", CheckStatus::Warn, "partial")
    }
}

/// Quantos hosts do Roblox responderam (com qualquer status HTTP). Timeout,
/// DNS ou TLS quebrado contam como "nÃ£o alcanÃ§ou".
async fn probe_roblox_hosts() -> (usize, usize) {
    let client = match api::http_client::builder_with(
        std::time::Duration::from_secs(6),
        std::time::Duration::from_secs(10),
    )
    .build()
    {
        Ok(client) => client,
        Err(_) => return (0, REACHABILITY_PROBES.len()),
    };
    let mut reached = 0;
    for (sub, path) in REACHABILITY_PROBES {
        let url = format!("{}{}", api::endpoints::host(sub), path);
        if client.get(url).send().await.is_ok() {
            reached += 1;
        }
    }
    (reached, REACHABILITY_PROBES.len())
}

/// Um processo do Roblox visto pela checagem de presos.
#[derive(Debug, Clone, Copy)]
struct RobloxProcessView {
    has_window: bool,
    /// HÃ¡ quanto tempo o processo existe; `None` = nÃ£o deu para ler.
    age_secs: Option<u64>,
}

/// Quantos processos estÃ£o **sem janela hÃ¡ tempo demais**. Sem idade conhecida
/// o processo nÃ£o entra: melhor nÃ£o acusar um cliente que acabou de abrir.
fn count_stuck_processes(processes: &[RobloxProcessView], min_age_secs: u64) -> usize {
    processes
        .iter()
        .filter(|p| !p.has_window && p.age_secs.is_some_and(|age| age >= min_age_secs))
        .count()
}

fn stuck_processes_check(stuck: usize) -> DiagnosticCheck {
    if stuck == 0 {
        DiagnosticCheck::new("stuckProcesses", CheckStatus::Ok, "none")
    } else {
        DiagnosticCheck::new("stuckProcesses", CheckStatus::Warn, "stuck").with_count(stuck as u32)
    }
}

/// Estado do Multi Roblox para quem vai abrir mais uma conta.
fn multi_roblox_check(enabled: bool, holder: &str, roblox_running: bool) -> DiagnosticCheck {
    if !enabled {
        return if roblox_running {
            // Com um cliente aberto e o Multi Roblox desligado, a conta nova
            // derruba a que estÃ¡ aberta (ou nÃ£o sobe) â€” o "nÃ£o faz nada" clÃ¡ssico.
            DiagnosticCheck::new("multiRoblox", CheckStatus::Warn, "offWithClients")
        } else {
            DiagnosticCheck::new("multiRoblox", CheckStatus::Ok, "off")
        };
    }
    match holder {
        "legacyRam" => DiagnosticCheck::new("multiRoblox", CheckStatus::Problem, "legacyRam"),
        "thisProcess" => DiagnosticCheck::new("multiRoblox", CheckStatus::Ok, "held"),
        "roblox" => DiagnosticCheck::new("multiRoblox", CheckStatus::Ok, "clientOpen"),
        _ => DiagnosticCheck::new("multiRoblox", CheckStatus::Ok, "free"),
    }
}

#[cfg(target_os = "windows")]
fn collect_roblox_process_views() -> Vec<RobloxProcessView> {
    use platform::windows::ExternalClientOs;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let os = platform::windows::WindowsExternalClientOs;
    platform::windows::get_roblox_pids()
        .into_iter()
        .map(|pid| RobloxProcessView {
            has_window: platform::windows::find_main_window(pid).is_some(),
            age_secs: os
                .process_created_ms(pid)
                .map(|created| (now_ms.saturating_sub(created).max(0) / 1000) as u64),
        })
        .collect()
}

/// Roda todas as checagens. Read-only: nunca fecha nada.
#[tauri::command]
async fn run_launch_diagnostics(
    settings: tauri::State<'_, SettingsStore>,
) -> Result<Vec<DiagnosticCheck>, String> {
    let mut checks = Vec::new();

    #[cfg(target_os = "windows")]
    checks.push(install_check(platform::windows::get_roblox_path().is_ok()));

    checks.push(folder_check(
        "dataFolder",
        probe_folder_writable(&data::settings::get_runtime_data_dir()),
    ));
    checks.push(folder_check(
        "versionsFolder",
        match data::versions::ram_managed_versions_root() {
            Some(dir) => probe_folder_writable(&dir),
            None => FolderProbe::Unknown,
        },
    ));

    let (reached, total) = probe_roblox_hosts().await;
    checks.push(internet_check(reached, total));

    #[cfg(target_os = "windows")]
    {
        let views = tauri::async_runtime::spawn_blocking(collect_roblox_process_views)
            .await
            .unwrap_or_default();
        checks.push(stuck_processes_check(count_stuck_processes(
            &views,
            STUCK_PROCESS_MIN_AGE_SECS,
        )));

        let roblox_pids = platform::windows::get_roblox_pids();
        let legacy_ram_pids = platform::windows::find_legacy_ram_pids();
        let holder = mutex_holder_label(
            platform::windows::this_process_holds_multi_roblox(),
            &roblox_pids,
            &legacy_ram_pids,
        );
        checks.push(multi_roblox_check(
            settings.get_bool("General", "EnableMultiRbx"),
            holder,
            !roblox_pids.is_empty(),
        ));
    }
    #[cfg(not(target_os = "windows"))]
    let _ = &settings;

    Ok(checks)
}
