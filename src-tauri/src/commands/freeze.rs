// Freeze: congela o cliente das contas marcadas na página Sessão, por um tempo
// ou até soltar. O jogo para de desenhar e de gastar CPU/GPU e volta de onde
// estava; a conexão segue (só a thread da janela para). O mecanismo mora em
// platform/windows/freeze.rs; aqui fica quem está congelado, o relógio de cada
// um e o que o resto do app precisa saber. Ver docs/features/freeze.md.
//
// - Congelada, a janela fica "Não respondendo" para o Windows depois de 5 s. O
//   app sabe que foi ele: o Painel de Sessão, o Watcher ("fechar se não
//   responder") e o aviso no Discord ignoram a conta congelada
//   (`window_is_frozen`, em client_health.rs), e o AFK pula a vez dela.
// - Fechar o app solta todas (`freeze_release_all`). Se o app cair com contas
//   congeladas, a lista fica em `RAMFrozen.json` e a próxima abertura as solta
//   (`freeze_recover_on_start`).

/// Teto de um congelamento com tempo (o "até soltar" não tem teto).
const FREEZE_MAX_SECONDS: u64 = 3600;
const FREEZE_TICK: std::time::Duration = std::time::Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct FreezeEntry {
    pid: u32,
    tid: u32,
    hwnd: isize,
    since_ms: i64,
    /// `None` = até a pessoa soltar.
    until_ms: Option<i64>,
}

/// O que a tela mostra de cada conta congelada.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FrozenClientView {
    user_id: i64,
    since_ms: i64,
    until_ms: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FreezeReport {
    frozen: Vec<i64>,
    /// `(conta, motivo)` de quem não deu para congelar.
    failed: Vec<(i64, String)>,
}

static FREEZES: std::sync::LazyLock<std::sync::Mutex<HashMap<i64, FreezeEntry>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));
static FREEZE_LOOP_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Quando o congelamento acaba: `None` (até soltar) com 0 ou sem tempo; senão
/// agora + o tempo, no máximo uma hora.
fn freeze_until_ms(seconds: Option<u64>, now_ms: i64) -> Option<i64> {
    match seconds {
        Some(s) if s > 0 => Some(now_ms + (s.min(FREEZE_MAX_SECONDS) as i64) * 1000),
        _ => None,
    }
}

/// Quem já deu a hora de soltar.
fn freezes_due(entries: &HashMap<i64, FreezeEntry>, now_ms: i64) -> Vec<i64> {
    let mut due: Vec<i64> = entries
        .iter()
        .filter(|(_, e)| e.until_ms.is_some_and(|until| now_ms >= until))
        .map(|(id, _)| *id)
        .collect();
    due.sort_unstable();
    due
}

/// Congelada, mas o cliente não é mais o mesmo (fechou ou reabriu com outro
/// pid): sai da lista sem mexer em nada.
fn freezes_gone(entries: &HashMap<i64, FreezeEntry>, pid_of: impl Fn(i64) -> Option<u32>) -> Vec<i64> {
    let mut gone: Vec<i64> = entries
        .iter()
        .filter(|(id, e)| pid_of(**id) != Some(e.pid))
        .map(|(id, _)| *id)
        .collect();
    gone.sort_unstable();
    gone
}

fn freeze_views(entries: &HashMap<i64, FreezeEntry>) -> Vec<FrozenClientView> {
    let mut views: Vec<FrozenClientView> = entries
        .iter()
        .map(|(id, e)| FrozenClientView { user_id: *id, since_ms: e.since_ms, until_ms: e.until_ms })
        .collect();
    views.sort_by_key(|v| v.user_id);
    views
}

/// A conta está congelada pelo app.
pub(crate) fn client_is_frozen(user_id: i64) -> bool {
    FREEZES.lock().map(|map| map.contains_key(&user_id)).unwrap_or(false)
}

/// A janela é de um cliente congelado pelo app ("Não respondendo" de propósito).
pub(crate) fn window_is_frozen(hwnd: isize) -> bool {
    FREEZES.lock().map(|map| map.values().any(|e| e.hwnd == hwnd)).unwrap_or(false)
}

fn freeze_file() -> std::path::PathBuf {
    data::settings::get_runtime_data_dir().join("RAMFrozen.json")
}

/// Grava quem está congelado (para soltar na próxima abertura se o app cair).
/// Lista vazia apaga o arquivo.
fn save_freezes(entries: &HashMap<i64, FreezeEntry>) {
    let path = freeze_file();
    if entries.is_empty() {
        let _ = std::fs::remove_file(path);
        return;
    }
    let list: Vec<&FreezeEntry> = entries.values().collect();
    if let Ok(text) = serde_json::to_string(&list) {
        let _ = std::fs::write(path, text);
    }
}

#[cfg(all(target_os = "windows", feature = "client-freeze"))]
fn thaw_entry(entry: &FreezeEntry) {
    platform::windows::thaw_thread(entry.pid, entry.tid);
}

#[cfg(not(all(target_os = "windows", feature = "client-freeze")))]
fn thaw_entry(_entry: &FreezeEntry) {}

/// Solta as contas e avisa a tela. Devolve as que soltou.
fn thaw_accounts(app: Option<&tauri::AppHandle>, user_ids: &[i64]) -> Vec<i64> {
    let Ok(mut map) = FREEZES.lock() else { return Vec::new() };
    let mut thawed = Vec::new();
    for id in user_ids {
        if let Some(entry) = map.remove(id) {
            thaw_entry(&entry);
            thawed.push(*id);
        }
    }
    if !thawed.is_empty() {
        save_freezes(&map);
        if let Some(app) = app {
            let _ = app.emit("freeze-changed", ());
        }
    }
    thawed
}

/// O relógio: solta quem deu a hora e esquece quem fechou.
fn start_freeze_loop(app: tauri::AppHandle) {
    if FREEZE_LOOP_STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(FREEZE_TICK).await;
            let (due, gone) = {
                let Ok(map) = FREEZES.lock() else { continue };
                if map.is_empty() {
                    continue;
                }
                #[cfg(target_os = "windows")]
                let tracker = platform::windows::tracker();
                #[cfg(target_os = "windows")]
                let gone = freezes_gone(&map, |id| tracker.get_pid(id));
                #[cfg(not(target_os = "windows"))]
                let gone: Vec<i64> = Vec::new();
                (freezes_due(&map, now_ms()), gone)
            };
            if !gone.is_empty() {
                if let Ok(mut map) = FREEZES.lock() {
                    for id in &gone {
                        map.remove(id);
                    }
                    save_freezes(&map);
                }
                let _ = app.emit("freeze-changed", ());
            }
            if !due.is_empty() {
                for id in thaw_accounts(Some(&app), &due) {
                    emit_launch_log(&app, id, "info", "client", String::from("Freeze ended: the client is running again."));
                }
            }
        }
    });
}

/// Congela o cliente das contas (`seconds` vazio ou 0 = até soltar). Conta já
/// congelada só troca o tempo.
#[tauri::command]
fn freeze_clients(app: tauri::AppHandle, user_ids: Vec<i64>, seconds: Option<u64>) -> Result<FreezeReport, String> {
    let now = now_ms();
    let until_ms = freeze_until_ms(seconds, now);
    let mut report = FreezeReport::default();
    #[cfg(all(target_os = "windows", feature = "client-freeze"))]
    {
        let tracker = platform::windows::tracker();
        let mut map = FREEZES.lock().map_err(|e| e.to_string())?;
        for id in user_ids {
            if let Some(entry) = map.get_mut(&id) {
                entry.until_ms = until_ms;
                report.frozen.push(id);
                continue;
            }
            let Some(pid) = tracker.get_pid(id) else {
                report.failed.push((id, "This account has no Roblox client open.".to_string()));
                continue;
            };
            let Some(hwnd) = platform::windows::find_main_window(pid) else {
                report.failed.push((id, "The Roblox window of this account was not found.".to_string()));
                continue;
            };
            match platform::windows::freeze_window_thread(hwnd) {
                Ok((pid, tid)) => {
                    map.insert(id, FreezeEntry { pid, tid, hwnd: hwnd as isize, since_ms: now, until_ms });
                    report.frozen.push(id);
                }
                Err(e) => report.failed.push((id, e)),
            }
        }
        save_freezes(&map);
    }
    #[cfg(not(all(target_os = "windows", feature = "client-freeze")))]
    {
        let _ = until_ms;
        for id in user_ids {
            report.failed.push((id, "Freeze is not available in this version.".to_string()));
        }
    }
    for id in &report.frozen {
        let line = match until_ms {
            Some(until) => format!("Frozen for {} s: the game stops until then.", (until - now) / 1000),
            None => String::from("Frozen until you unfreeze it."),
        };
        emit_launch_log(&app, *id, "info", "client", line);
    }
    for (id, reason) in &report.failed {
        emit_launch_log(&app, *id, "warn", "client", format!("Could not freeze: {reason}"));
    }
    if !report.frozen.is_empty() {
        start_freeze_loop(app.clone());
        let _ = app.emit("freeze-changed", ());
    }
    Ok(report)
}

/// Solta o cliente das contas.
#[tauri::command]
fn unfreeze_clients(app: tauri::AppHandle, user_ids: Vec<i64>) -> usize {
    let thawed = thaw_accounts(Some(&app), &user_ids);
    for id in &thawed {
        emit_launch_log(&app, *id, "info", "client", String::from("Unfrozen: the client is running again."));
    }
    thawed.len()
}

#[tauri::command]
fn frozen_clients() -> Vec<FrozenClientView> {
    FREEZES.lock().map(|map| freeze_views(&map)).unwrap_or_default()
}

/// Fechando o app: solta todas (nenhum cliente fica parado sem o app).
fn freeze_release_all() {
    let ids: Vec<i64> = FREEZES.lock().map(|map| map.keys().copied().collect()).unwrap_or_default();
    thaw_accounts(None, &ids);
}

/// O app caiu com contas congeladas: solta as que ainda estão de pé.
fn freeze_recover_on_start() {
    let path = freeze_file();
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let entries: Vec<FreezeEntry> = serde_json::from_str(&text).unwrap_or_default();
    for entry in &entries {
        thaw_entry(entry);
    }
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod freeze_tests {
    use super::*;

    fn entry(pid: u32, until_ms: Option<i64>) -> FreezeEntry {
        FreezeEntry { pid, tid: pid + 1, hwnd: pid as isize, since_ms: 0, until_ms }
    }

    #[test]
    fn a_freeze_with_time_ends_then_and_none_or_zero_lasts_until_unfrozen() {
        assert_eq!(freeze_until_ms(Some(30), 1_000), Some(31_000));
        assert_eq!(freeze_until_ms(None, 1_000), None);
        assert_eq!(freeze_until_ms(Some(0), 1_000), None);
        assert_eq!(freeze_until_ms(Some(99_999), 0), Some(3_600_000), "at most one hour");
    }

    #[test]
    fn only_timed_freezes_that_reached_their_time_are_due() {
        let map = HashMap::from([(1, entry(10, Some(5_000))), (2, entry(20, Some(9_000))), (3, entry(30, None))]);
        assert_eq!(freezes_due(&map, 4_999), Vec::<i64>::new());
        assert_eq!(freezes_due(&map, 5_000), vec![1]);
        assert_eq!(freezes_due(&map, 1_000_000), vec![1, 2], "until-unfrozen never comes due");
    }

    #[test]
    fn a_client_that_closed_or_reopened_leaves_the_list() {
        let map = HashMap::from([(1, entry(10, None)), (2, entry(20, None)), (3, entry(30, None))]);
        let gone = freezes_gone(&map, |id| match id {
            1 => Some(10),
            2 => Some(99),
            _ => None,
        });
        assert_eq!(gone, vec![2, 3]);
    }

    #[test]
    fn the_screen_gets_every_frozen_account_in_order() {
        let map = HashMap::from([(7, entry(70, Some(1))), (3, entry(30, None))]);
        let views = freeze_views(&map);
        assert_eq!(views.iter().map(|v| v.user_id).collect::<Vec<_>>(), vec![3, 7]);
        assert_eq!(views[1].until_ms, Some(1));
    }
}
