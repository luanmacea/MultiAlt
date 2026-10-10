// Presets de launch e o agendador deles. Ver docs/features/presets.md.
//
// Um preset abre pelo caminho normal da fila (`launch_multiple`, sem mudar
// nada nela): mesma reserva de sequência, mesmo piso anti-captcha, mesma
// guarda de versão. O que este arquivo acrescenta é:
//
// - **quais clientes cada execução abriu** (`PRESET_RUNS`): a conta → PID de
//   antes e de depois do lote. "Fechar" mexe **só** nesses PIDs, e só se o
//   tracker ainda tiver aquele mesmo PID para a conta — cliente aberto pelo
//   site (adotado), por outra conta, pelo Auto Rejoin depois, ou pelo usuário
//   nunca entra;
// - o **agendador**, que roda só com o app aberto (uma task do tokio, nada de
//   tarefa agendada do Windows) e **não recupera** horário perdido: se o app
//   estava fechado (ou o PC dormindo) na hora, aquela vez não acontece.

use data::launch_presets::{
    due_actions, next_close_after, next_open_after, LaunchPreset, PresetAction,
};

/// De quanto em quanto tempo o agendador olha o relógio.
const PRESET_SCHEDULER_TICK: std::time::Duration = std::time::Duration::from_secs(15);
/// Intervalo maior que isto entre duas passadas = o PC dormiu (ou o relógio
/// pulou). O que venceu no meio não é recuperado.
const PRESET_MAX_TICK_GAP_SECS: i64 = 180;
/// Abrir na hora marcada com outro launch em andamento: tenta de novo até
/// este tempo, em vez de perder a vez.
const PRESET_BUSY_RETRY_FOR: std::time::Duration = std::time::Duration::from_secs(600);

/// Um cliente que uma execução do preset abriu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PresetRunClient {
    user_id: i64,
    pid: u32,
}

/// (conta, PID, adotado do site) de cada cliente no tracker.
type PresetTrackedClient = (i64, u32, bool);

static PRESET_RUNS: std::sync::LazyLock<std::sync::Mutex<HashMap<String, Vec<PresetRunClient>>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// Quem o lote abriu: a conta do preset que tem, depois, um PID que não tinha
/// antes. Cliente adotado do site nunca conta, mesmo que tenha aparecido agora.
fn clients_opened_by_run(
    before: &[PresetTrackedClient],
    after: &[PresetTrackedClient],
    user_ids: &[i64],
) -> Vec<PresetRunClient> {
    user_ids
        .iter()
        .filter_map(|uid| {
            let (_, pid, adopted) = after.iter().find(|(u, _, _)| u == uid)?;
            if *adopted {
                return None;
            }
            let previous = before.iter().find(|(u, _, _)| u == uid).map(|(_, p, _)| *p);
            (previous != Some(*pid)).then_some(PresetRunClient {
                user_id: *uid,
                pid: *pid,
            })
        })
        .collect()
}

/// Os clientes da execução que ainda são os mesmos processos: o tracker tem o
/// mesmo PID para a conta, e não é cliente do site.
fn preset_clients_still_open(run: &[PresetRunClient], tracked: &[PresetTrackedClient]) -> Vec<PresetRunClient> {
    run.iter()
        .copied()
        .filter(|client| {
            tracked
                .iter()
                .any(|(uid, pid, adopted)| *uid == client.user_id && *pid == client.pid && !adopted)
        })
        .collect()
}

/// Junta uma execução nova à anterior do mesmo preset: fica o que ainda está
/// aberto e não foi substituído por um cliente novo da mesma conta.
fn merge_preset_run(
    existing: &[PresetRunClient],
    opened: &[PresetRunClient],
    tracked: &[PresetTrackedClient],
) -> Vec<PresetRunClient> {
    let mut out: Vec<PresetRunClient> = preset_clients_still_open(existing, tracked)
        .into_iter()
        .filter(|old| !opened.iter().any(|new| new.user_id == old.user_id))
        .collect();
    out.extend_from_slice(opened);
    out
}

/// Intervalo que o agendador avalia nesta passada, ou `None` quando não deve
/// avaliar nada: relógio parado/para trás, ou um buraco grande demais (PC
/// dormindo) — sem recuperação.
fn preset_scheduler_window(
    last: chrono::NaiveDateTime,
    now: chrono::NaiveDateTime,
) -> Option<(chrono::NaiveDateTime, chrono::NaiveDateTime)> {
    if now <= last || (now - last).num_seconds() > PRESET_MAX_TICK_GAP_SECS {
        return None;
    }
    Some((last, now))
}

fn preset_tracked_clients() -> Vec<PresetTrackedClient> {
    #[cfg(target_os = "windows")]
    {
        return platform::windows::tracker()
            .get_all()
            .into_iter()
            .map(|p| (p.user_id, p.pid, p.adopted))
            .collect();
    }
    #[cfg(target_os = "macos")]
    {
        return platform::macos::tracker()
            .get_all()
            .into_iter()
            .map(|p| (p.user_id, p.pid, false))
            .collect();
    }
    #[allow(unreachable_code)]
    Vec::new()
}

/// Fecha o cliente da conta **só se** o tracker ainda tem aquele PID para ela.
/// Conferido de novo aqui, colado no fechamento.
fn close_preset_client(user_id: i64, pid: u32) -> bool {
    #[cfg(target_os = "windows")]
    {
        let tracker = platform::windows::tracker();
        if tracker.get_pid(user_id) != Some(pid) {
            return false;
        }
        if tracker.get_all().iter().any(|p| p.user_id == user_id && p.adopted) {
            return false;
        }
        return tracker.kill_for_user(user_id);
    }
    #[cfg(target_os = "macos")]
    {
        let tracker = platform::macos::tracker();
        if tracker.get_pid(user_id) != Some(pid) {
            return false;
        }
        return tracker.kill_for_user(user_id);
    }
    #[allow(unreachable_code)]
    {
        let _ = (user_id, pid);
        false
    }
}

fn preset_run_snapshot(preset_id: &str) -> Vec<PresetRunClient> {
    PRESET_RUNS
        .lock()
        .ok()
        .and_then(|runs| runs.get(preset_id).cloned())
        .unwrap_or_default()
}

/// O que a tela mostra de cada preset: ele, a próxima abertura/fechamento e
/// quantos clientes da última execução ainda estão abertos.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchPresetView {
    #[serde(flatten)]
    preset: LaunchPreset,
    next_open_at: Option<i64>,
    next_close_at: Option<i64>,
    open_clients: usize,
    /// As contas desses clientes: a tela pergunta antes de fechar, nomeando-as.
    open_user_ids: Vec<i64>,
}

fn local_naive_to_ms(at: chrono::NaiveDateTime) -> Option<i64> {
    use chrono::TimeZone;
    chrono::Local
        .from_local_datetime(&at)
        .earliest()
        .map(|d| d.timestamp_millis())
}

#[tauri::command]
fn get_launch_presets(store: tauri::State<'_, LaunchPresetStore>) -> Result<Vec<LaunchPresetView>, String> {
    let now = chrono::Local::now().naive_local();
    let tracked = preset_tracked_clients();
    Ok(store
        .list()?
        .into_iter()
        .map(|preset| {
            let schedule = preset.schedule.clone().unwrap_or_default();
            let still_open = preset_clients_still_open(&preset_run_snapshot(&preset.id), &tracked);
            LaunchPresetView {
                next_open_at: next_open_after(&schedule, now).and_then(local_naive_to_ms),
                next_close_at: next_close_after(&schedule, now).and_then(local_naive_to_ms),
                open_clients: still_open.len(),
                open_user_ids: still_open.iter().map(|c| c.user_id).collect(),
                preset,
            }
        })
        .collect())
}

#[tauri::command]
fn save_launch_preset(
    store: tauri::State<'_, LaunchPresetStore>,
    preset: LaunchPreset,
) -> Result<LaunchPreset, String> {
    store.upsert(preset, chrono::Utc::now().timestamp_millis())
}

#[tauri::command]
fn delete_launch_preset(store: tauri::State<'_, LaunchPresetStore>, id: String) -> Result<bool, String> {
    let deleted = store.delete(&id)?;
    // Esquece a execução: um preset apagado não fecha mais nada.
    if let Ok(mut runs) = PRESET_RUNS.lock() {
        runs.remove(&id);
    }
    Ok(deleted)
}

/// Arrange in grid com os monitores e o espaçamento da aba Windows.
fn arrange_after_preset(app: &tauri::AppHandle) {
    let settings = app.state::<SettingsStore>();
    let monitors: Vec<usize> = settings
        .get_string("General", "GridMonitors")
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let gap = settings.get_int("General", "GridGap").unwrap_or(20).clamp(0, 200) as i32;
    if let Err(e) = arrange_windows_grid(app.state(), app.state(), monitors, gap) {
        eprintln!("Preset grid failed: {e}");
    }
}

/// Abre o preset pela fila normal e anota quais clientes ele abriu.
async fn run_launch_preset(app: &tauri::AppHandle, preset: &LaunchPreset) -> Result<usize, String> {
    let known: HashSet<i64> = app
        .state::<AccountStore>()
        .get_all()?
        .into_iter()
        .map(|a| a.user_id)
        .collect();
    let user_ids: Vec<i64> = preset.user_ids.iter().copied().filter(|id| known.contains(id)).collect();
    if user_ids.is_empty() {
        return Err("None of this preset's accounts are in the app anymore.".into());
    }

    let before = preset_tracked_clients();
    #[cfg(target_os = "windows")]
    launch_multiple(
        app.clone(),
        app.state(),
        app.state(),
        app.state(),
        user_ids.clone(),
        preset.place_id,
        preset.job_id.clone(),
        String::new(),
        Some(false),
    )
    .await?;
    #[cfg(not(target_os = "windows"))]
    launch_multiple(
        app.clone(),
        app.state(),
        app.state(),
        user_ids.clone(),
        preset.place_id,
        preset.job_id.clone(),
        String::new(),
        Some(false),
    )
    .await?;
    let after = preset_tracked_clients();

    let opened = clients_opened_by_run(&before, &after, &user_ids);
    if let Ok(mut runs) = PRESET_RUNS.lock() {
        let merged = merge_preset_run(runs.get(&preset.id).map(Vec::as_slice).unwrap_or(&[]), &opened, &after);
        runs.insert(preset.id.clone(), merged);
    }
    if preset.arrange_grid && !opened.is_empty() {
        arrange_after_preset(app);
    }
    Ok(opened.len())
}

/// Fecha os clientes que as execuções deste preset abriram e que ainda são os
/// mesmos processos. Nada mais.
async fn close_preset_run(preset_id: &str) -> usize {
    let still_open = preset_clients_still_open(&preset_run_snapshot(preset_id), &preset_tracked_clients());
    let mut closed = 0;
    for client in still_open {
        let ok = tokio::task::spawn_blocking(move || close_preset_client(client.user_id, client.pid))
            .await
            .unwrap_or(false);
        if ok {
            closed += 1;
        }
    }
    let tracked = preset_tracked_clients();
    if let Ok(mut runs) = PRESET_RUNS.lock() {
        if let Some(run) = runs.get_mut(preset_id) {
            *run = preset_clients_still_open(run, &tracked);
        }
    }
    closed
}

/// Linha no Console (sem conta: é do preset) e evento para a tela.
fn emit_preset_outcome(
    app: &tauri::AppHandle,
    preset: &LaunchPreset,
    action: PresetAction,
    scheduled: bool,
    outcome: &Result<usize, String>,
) {
    let (level, message) = match (action, outcome) {
        (PresetAction::Open, Ok(n)) => ("success", format!("Preset \"{}\": {n} cliente(s) aberto(s)", preset.name)),
        (PresetAction::Open, Err(e)) => ("error", format!("Preset \"{}\" não abriu: {e}", preset.name)),
        (PresetAction::Close, Ok(n)) => ("info", format!("Preset \"{}\": {n} cliente(s) fechado(s)", preset.name)),
        (PresetAction::Close, Err(e)) => ("error", format!("Preset \"{}\" não fechou: {e}", preset.name)),
    };
    emit_session_log(app, level, "preset", message);
    let _ = app.emit(
        "launch-preset",
        serde_json::json!({
            "presetId": preset.id,
            "name": preset.name,
            "action": match action { PresetAction::Open => "open", PresetAction::Close => "close" },
            "scheduled": scheduled,
            "ok": outcome.is_ok(),
            "count": outcome.as_ref().ok().copied().unwrap_or(0),
            "error": outcome.as_ref().err(),
        }),
    );
}

#[tauri::command]
async fn launch_preset(app: tauri::AppHandle, id: String) -> Result<usize, String> {
    let preset = app
        .state::<LaunchPresetStore>()
        .get(&id)?
        .ok_or_else(|| "This preset no longer exists.".to_string())?;
    let outcome = run_launch_preset(&app, &preset).await;
    // A recusa "launch em andamento" já vira aviso na tela de quem clicou.
    if !matches!(&outcome, Err(e) if e == LAUNCH_ALREADY_ACTIVE) {
        emit_preset_outcome(&app, &preset, PresetAction::Open, false, &outcome);
    }
    outcome
}

#[tauri::command]
async fn close_preset_clients(app: tauri::AppHandle, id: String) -> Result<usize, String> {
    let preset = app
        .state::<LaunchPresetStore>()
        .get(&id)?
        .ok_or_else(|| "This preset no longer exists.".to_string())?;
    let closed = close_preset_run(&id).await;
    emit_preset_outcome(&app, &preset, PresetAction::Close, false, &Ok(closed));
    Ok(closed)
}

async fn run_scheduled_preset_action(app: &tauri::AppHandle, preset_id: &str, action: PresetAction) {
    let Ok(Some(preset)) = app.state::<LaunchPresetStore>().get(preset_id) else {
        return;
    };
    match action {
        PresetAction::Open => {
            let started = std::time::Instant::now();
            let outcome = loop {
                let outcome = run_launch_preset(app, &preset).await;
                match &outcome {
                    Err(e) if e == LAUNCH_ALREADY_ACTIVE && started.elapsed() < PRESET_BUSY_RETRY_FOR => {
                        tokio::time::sleep(PRESET_SCHEDULER_TICK).await;
                    }
                    _ => break outcome,
                }
            };
            emit_preset_outcome(app, &preset, PresetAction::Open, true, &outcome);
        }
        PresetAction::Close => {
            // Sem cliente desta execução aberto, a hora de fechar não faz nada
            // (e não enche o Console).
            if preset_clients_still_open(&preset_run_snapshot(preset_id), &preset_tracked_clients()).is_empty() {
                return;
            }
            let closed = close_preset_run(preset_id).await;
            emit_preset_outcome(app, &preset, PresetAction::Close, true, &Ok(closed));
        }
    }
}

/// Liga o agendador. Roda enquanto o app estiver aberto; as ações saem numa
/// fila própria, uma por vez (dois presets na mesma hora abrem um depois do
/// outro, e a passada do relógio nunca espera um launch).
pub(crate) fn start_preset_scheduler(app: tauri::AppHandle) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(String, PresetAction)>();
    let worker = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some((id, action)) = rx.recv().await {
            run_scheduled_preset_action(&worker, &id, action).await;
        }
    });
    tauri::async_runtime::spawn(async move {
        let mut last = chrono::Local::now().naive_local();
        loop {
            tokio::time::sleep(PRESET_SCHEDULER_TICK).await;
            let now = chrono::Local::now().naive_local();
            if let Some((from, to)) = preset_scheduler_window(last, now) {
                let presets = app.state::<LaunchPresetStore>().list().unwrap_or_default();
                for preset in presets {
                    let Some(schedule) = &preset.schedule else { continue };
                    for action in due_actions(schedule, from, to) {
                        let _ = tx.send((preset.id.clone(), action));
                    }
                }
            }
            last = now;
        }
    });
}

#[cfg(test)]
mod launch_preset_run_tests {
    use super::*;

    fn client(user_id: i64, pid: u32) -> PresetRunClient {
        PresetRunClient { user_id, pid }
    }

    #[test]
    fn the_run_owns_only_the_new_pids_of_its_own_accounts() {
        let before = vec![(1, 100, false), (9, 900, false)];
        // Conta 1 abriu de novo (PID novo), conta 2 abriu pela primeira vez,
        // conta 9 não é do preset e também mudou.
        let after = vec![(1, 101, false), (2, 200, false), (9, 901, false)];
        assert_eq!(
            clients_opened_by_run(&before, &after, &[1, 2, 3]),
            vec![client(1, 101), client(2, 200)]
        );
    }

    #[test]
    fn an_account_that_did_not_open_again_is_not_the_runs() {
        // A conta já estava aberta e o lote não a reabriu (pulada, falhou):
        // o cliente é de antes, não desta execução.
        let before = vec![(1, 100, false)];
        assert!(clients_opened_by_run(&before, &before, &[1]).is_empty());
    }

    #[test]
    fn a_website_client_is_never_the_runs() {
        let after = vec![(1, 100, true)];
        assert!(clients_opened_by_run(&[], &after, &[1]).is_empty());
    }

    #[test]
    fn closing_only_touches_the_same_process_still_tracked() {
        let run = vec![client(1, 101), client(2, 200), client(3, 300)];
        let tracked = vec![
            (1, 101, false), // o mesmo: fecha
            (2, 222, false), // reaberto por outro caminho depois: não é deste preset
            (3, 300, true),  // virou cliente do site (PID reaproveitado): nunca
            (4, 400, false), // outra conta: nunca
        ];
        assert_eq!(preset_clients_still_open(&run, &tracked), vec![client(1, 101)]);
    }

    #[test]
    fn a_second_run_replaces_the_account_and_keeps_the_rest_still_open() {
        let existing = vec![client(1, 101), client(2, 200), client(3, 300)];
        let opened = vec![client(2, 201)];
        let tracked = vec![(1, 101, false), (2, 201, false)]; // 3 já fechou
        assert_eq!(
            merge_preset_run(&existing, &opened, &tracked),
            vec![client(1, 101), client(2, 201)]
        );
    }

    #[test]
    fn the_view_names_the_accounts_whose_windows_are_still_open() {
        // A tela pergunta antes de fechar e lista essas contas (`openUserIds`).
        let view = LaunchPresetView {
            preset: LaunchPreset { id: "p".into(), name: "Farm".into(), ..Default::default() },
            next_open_at: None,
            next_close_at: None,
            open_clients: 2,
            open_user_ids: vec![1, 2],
        };
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["openClients"], 2);
        assert_eq!(json["openUserIds"], serde_json::json!([1, 2]));
        assert_eq!(json["name"], "Farm");
    }

    #[test]
    fn the_scheduler_does_not_catch_up_after_a_long_gap() {
        let at = |s: &str| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap();
        assert!(preset_scheduler_window(at("2026-10-12 07:59:50"), at("2026-10-12 08:00:05")).is_some());
        // PC dormiu das 07:00 às 09:00: o 08:00 não acontece.
        assert!(preset_scheduler_window(at("2026-10-12 07:00:00"), at("2026-10-12 09:00:00")).is_none());
        // Relógio voltou.
        assert!(preset_scheduler_window(at("2026-10-12 08:00:00"), at("2026-10-12 07:59:00")).is_none());
    }

    #[test]
    fn the_scheduler_never_uses_the_windows_task_scheduler() {
        // Requisito da ideia 13: agendamento só com o app aberto. Nada de
        // `schtasks`, COM do Task Scheduler ou entrada de inicialização.
        let source = include_str!("launch_presets.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or_default();
        let code: String = production
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
            .to_ascii_lowercase();
        for forbidden in ["schtasks", "itaskservice", "taskscheduler", "\\run"] {
            assert!(!code.contains(forbidden), "found {forbidden}");
        }
    }
}
