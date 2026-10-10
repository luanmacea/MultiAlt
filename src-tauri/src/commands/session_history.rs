// Histórico de sessões (ideia 6): o observador que transforma o retrato do
// monitor de quedas (client_health.rs) em eventos, e os comandos da tela. Ver
// docs/features/history.md.
//
// Nada aqui lê log nem processo: quem lê é o monitor, e a cada passada dele
// (2 s) o observador compara o retrato com o anterior e grava o que mudou —
// entrou, trocou de servidor, saiu, caiu, fechou.

use data::session_history::{
    build_sessions, HistoryEvent, HistoryEventKind, OpenSessionsCheckpoint, SessionRecord,
};

/// De quanto em quanto tempo o "quem está em jogo" vai para o disco (é com ele
/// que uma sessão aberta ganha fim depois de o app fechar à força).
const SESSION_CHECKPOINT_EVERY_MS: i64 = 60_000;

/// O que o observador lembra de cada conta entre uma passada e outra.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ObservedClient {
    pid: u32,
    place_id: Option<i64>,
    job_id: Option<String>,
    /// Há uma sessão aberta (gravou `joined`/`teleported` e ainda não o fim).
    in_game: bool,
}

#[derive(Debug, Default)]
struct SessionHistoryObserver {
    clients: HashMap<i64, ObservedClient>,
    last_checkpoint_ms: i64,
    checkpoint_has_users: bool,
}

fn drop_kind_text(kind: DropKind) -> Option<String> {
    serde_json::to_value(kind).ok()?.as_str().map(str::to_string)
}

fn drop_reason_text(reason: DropReason) -> Option<String> {
    serde_json::to_value(reason).ok()?.as_str().map(str::to_string)
}

fn dropped_event(at: i64, user_id: i64, drop: &ClientDrop) -> HistoryEvent {
    let mut event = HistoryEvent::new(at, user_id, HistoryEventKind::Dropped);
    event.drop_kind = drop_kind_text(drop.kind);
    event.reason = drop.reason.and_then(drop_reason_text);
    event.code = drop.code;
    event.message = drop.message.clone();
    event
}

impl SessionHistoryObserver {
    /// Compara o retrato novo com o anterior e devolve os eventos. Puro.
    fn observe(&mut self, snapshots: &HashMap<i64, ClientSessionSnapshot>, now_ms: i64) -> Vec<HistoryEvent> {
        let mut events = Vec::new();

        // Conta que sumiu do acompanhamento (cliente fechou e saiu do tracker).
        let gone: Vec<i64> = self
            .clients
            .keys()
            .copied()
            .filter(|uid| !snapshots.contains_key(uid))
            .collect();
        for uid in gone {
            if self.clients.remove(&uid).is_some_and(|c| c.in_game) {
                events.push(HistoryEvent::new(now_ms, uid, HistoryEventKind::Closed));
            }
        }

        let mut ordered: Vec<(&i64, &ClientSessionSnapshot)> = snapshots.iter().collect();
        ordered.sort_by_key(|(uid, _)| **uid);
        for (uid, snap) in ordered {
            let uid = *uid;
            let playing = snap.place_id.is_some() && snap.drop.is_none() && !snap.left && !snap.exited;

            let same_process = self.clients.get(&uid).is_some_and(|c| c.pid == snap.pid);
            if !same_process {
                // Cliente novo da conta: o anterior, se estava em jogo, acabou.
                if self.clients.remove(&uid).is_some_and(|c| c.in_game) {
                    events.push(HistoryEvent::new(now_ms, uid, HistoryEventKind::Closed));
                }
                if playing {
                    events.push(
                        HistoryEvent::new(now_ms, uid, HistoryEventKind::Joined)
                            .at_place(snap.place_id, snap.job_id.clone()),
                    );
                }
                self.clients.insert(
                    uid,
                    ObservedClient {
                        pid: snap.pid,
                        place_id: snap.place_id,
                        job_id: snap.job_id.clone(),
                        in_game: playing,
                    },
                );
                continue;
            }

            let Some(client) = self.clients.get_mut(&uid) else { continue };
            if client.in_game {
                if let Some(drop) = &snap.drop {
                    events.push(dropped_event(now_ms, uid, drop));
                    client.in_game = false;
                } else if snap.left {
                    events.push(HistoryEvent::new(now_ms, uid, HistoryEventKind::Left));
                    client.in_game = false;
                } else if snap.exited {
                    events.push(HistoryEvent::new(now_ms, uid, HistoryEventKind::Closed));
                    client.in_game = false;
                } else if (snap.place_id, &snap.job_id) != (client.place_id, &client.job_id) && snap.place_id.is_some() {
                    events.push(
                        HistoryEvent::new(now_ms, uid, HistoryEventKind::Teleported)
                            .at_place(snap.place_id, snap.job_id.clone()),
                    );
                }
            } else if playing {
                // Entrou de novo (depois de cair ou sair, no mesmo cliente).
                events.push(
                    HistoryEvent::new(now_ms, uid, HistoryEventKind::Joined)
                        .at_place(snap.place_id, snap.job_id.clone()),
                );
                client.in_game = true;
            }
            client.place_id = snap.place_id;
            client.job_id = snap.job_id.clone();
        }
        events
    }

    fn playing_users(&self) -> Vec<i64> {
        let mut users: Vec<i64> = self.clients.iter().filter(|(_, c)| c.in_game).map(|(u, _)| *u).collect();
        users.sort_unstable();
        users
    }

    /// O checkpoint a gravar agora, se for hora (a cada minuto com gente em
    /// jogo, e uma vez quando ninguém mais está em jogo).
    fn checkpoint_due(&mut self, now_ms: i64, changed: bool) -> Option<OpenSessionsCheckpoint> {
        let users = self.playing_users();
        let due = if users.is_empty() {
            self.checkpoint_has_users
        } else {
            changed || now_ms - self.last_checkpoint_ms >= SESSION_CHECKPOINT_EVERY_MS
        };
        if !due {
            return None;
        }
        self.last_checkpoint_ms = now_ms;
        self.checkpoint_has_users = !users.is_empty();
        Some(OpenSessionsCheckpoint { at: now_ms, user_ids: users })
    }
}

static SESSION_HISTORY_OBSERVER: std::sync::LazyLock<std::sync::Mutex<SessionHistoryObserver>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(SessionHistoryObserver::default()));

/// Chamado pelo monitor de quedas a cada passada: grava o que mudou na hora e
/// avisa a tela.
pub(crate) fn record_session_history(
    app: &tauri::AppHandle,
    snapshots: &HashMap<i64, ClientSessionSnapshot>,
    now_ms: i64,
) {
    let (events, checkpoint) = {
        let Ok(mut observer) = SESSION_HISTORY_OBSERVER.lock() else {
            return;
        };
        let events = observer.observe(snapshots, now_ms);
        let checkpoint = observer.checkpoint_due(now_ms, !events.is_empty());
        (events, checkpoint)
    };
    let store = app.state::<SessionHistoryStore>();
    if !events.is_empty() {
        if let Err(e) = store.append(&events, now_ms) {
            eprintln!("{e}");
        }
        let mut users: Vec<i64> = events.iter().map(|e| e.user_id).collect();
        users.sort_unstable();
        users.dedup();
        let _ = app.emit("session-history-changed", serde_json::json!({ "userIds": users }));
    }
    if let Some(checkpoint) = checkpoint {
        store.write_checkpoint(&checkpoint);
    }
}

/// O Roblox disse que a conta está moderada (launch): vira uma linha no histórico.
pub(crate) fn record_moderated_history(app: &tauri::AppHandle, user_id: i64) {
    let now = chrono::Utc::now().timestamp_millis();
    let store = app.state::<SessionHistoryStore>();
    if store
        .append(&[HistoryEvent::new(now, user_id, HistoryEventKind::Moderated)], now)
        .is_ok()
    {
        let _ = app.emit("session-history-changed", serde_json::json!({ "userIds": [user_id] }));
    }
}

/// Sessões da conta, da mais nova para a mais velha (até 90 dias).
#[tauri::command]
fn get_session_history(
    store: tauri::State<'_, SessionHistoryStore>,
    user_id: i64,
) -> Result<Vec<SessionRecord>, String> {
    let playing = SESSION_HISTORY_OBSERVER
        .lock()
        .map(|o| o.clients.get(&user_id).is_some_and(|c| c.in_game))
        .unwrap_or(false);
    Ok(build_sessions(&store.read_all(), user_id, playing))
}

/// Nome do arquivo exportado: só a conta (id) e a hora — nada vindo da tela.
fn history_export_file_name(user_id: i64, now: chrono::DateTime<chrono::Local>) -> String {
    format!("multialt-history-{user_id}-{}.csv", now.format("%Y%m%d-%H%M%S"))
}

const MAX_HISTORY_EXPORT_BYTES: usize = 8 * 1024 * 1024;

/// Grava o CSV (montado e protegido contra fórmula pela tela) em
/// `<pasta de dados>/exports/` e abre o Explorer com o arquivo selecionado.
/// Devolve o caminho.
#[tauri::command]
fn save_history_export(user_id: i64, csv: String) -> Result<String, String> {
    if csv.len() > MAX_HISTORY_EXPORT_BYTES {
        return Err("The export is too large.".into());
    }
    let dir = data::settings::get_runtime_data_dir().join("exports");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create the exports folder: {e}"))?;
    let path = dir.join(history_export_file_name(user_id, chrono::Local::now()));
    // BOM: o Excel abre o UTF-8 com acentos certos.
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(csv.as_bytes());
    std::fs::write(&path, bytes).map_err(|e| format!("Could not save the file: {e}"))?;
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn();
    }
    Ok(path.display().to_string())
}

/// Ao abrir o app: sessões que ficaram abertas ganham o fim e a retenção roda.
pub(crate) fn start_session_history(app: &tauri::AppHandle) {
    let store = app.state::<SessionHistoryStore>();
    let closed = store.recover(chrono::Utc::now().timestamp_millis());
    if closed > 0 {
        eprintln!("Session history: {closed} session(s) left open by the last run were closed");
    }
}
