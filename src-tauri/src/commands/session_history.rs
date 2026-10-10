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

#[cfg(test)]
mod session_history_observer_tests {
    use super::*;

    fn snap(pid: u32, place: Option<i64>, job: Option<&str>) -> ClientSessionSnapshot {
        ClientSessionSnapshot {
            pid,
            place_id: place,
            job_id: job.map(str::to_string),
            drop: None,
            left: false,
            exited: false,
        }
    }

    fn one(uid: i64, s: ClientSessionSnapshot) -> HashMap<i64, ClientSessionSnapshot> {
        HashMap::from([(uid, s)])
    }

    fn kinds(events: &[HistoryEvent]) -> Vec<HistoryEventKind> {
        events.iter().map(|e| e.kind).collect()
    }

    #[test]
    fn joining_is_recorded_once_with_the_place_and_server() {
        let mut o = SessionHistoryObserver::default();
        // Cliente aberto, ainda no carregamento: nada.
        assert!(o.observe(&one(7, snap(100, None, None)), 1).is_empty());
        let events = o.observe(&one(7, snap(100, Some(10), Some("job-a"))), 2);
        assert_eq!(kinds(&events), vec![HistoryEventKind::Joined]);
        assert_eq!((events[0].place_id, events[0].job_id.as_deref()), (Some(10), Some("job-a")));
        assert!(o.observe(&one(7, snap(100, Some(10), Some("job-a"))), 3).is_empty());
    }

    #[test]
    fn a_new_server_in_the_same_client_is_a_teleport() {
        let mut o = SessionHistoryObserver::default();
        o.observe(&one(7, snap(100, Some(10), Some("a"))), 1);
        let events = o.observe(&one(7, snap(100, Some(11), Some("b"))), 2);
        assert_eq!(kinds(&events), vec![HistoryEventKind::Teleported]);
        assert_eq!(events[0].place_id, Some(11));
    }

    #[test]
    fn a_drop_carries_the_reason_from_the_log_and_rejoining_starts_again() {
        let mut o = SessionHistoryObserver::default();
        o.observe(&one(7, snap(100, Some(10), Some("a"))), 1);
        let mut dropped = snap(100, Some(10), Some("a"));
        dropped.drop = Some(ClientDrop {
            kind: DropKind::Disconnected,
            reason: Some(DropReason::ConnectionLost),
            code: Some(277),
            message: None,
            since_ms: 2,
        });
        let events = o.observe(&one(7, dropped.clone()), 2);
        assert_eq!(kinds(&events), vec![HistoryEventKind::Dropped]);
        assert_eq!(events[0].drop_kind.as_deref(), Some("disconnected"));
        assert_eq!(events[0].reason.as_deref(), Some("connectionLost"));
        assert_eq!(events[0].code, Some(277));
        // Ainda caído: não repete.
        assert!(o.observe(&one(7, dropped), 3).is_empty());
        // Entrou de novo (o monitor limpa a queda no `Joining game`).
        assert_eq!(kinds(&o.observe(&one(7, snap(100, Some(10), Some("c"))), 4)), vec![HistoryEventKind::Joined]);
    }

    #[test]
    fn leaving_and_the_client_closing_end_the_session() {
        let mut o = SessionHistoryObserver::default();
        o.observe(&one(7, snap(100, Some(10), Some("a"))), 1);
        let mut left = snap(100, Some(10), Some("a"));
        left.left = true;
        assert_eq!(kinds(&o.observe(&one(7, left), 2)), vec![HistoryEventKind::Left]);

        let mut o = SessionHistoryObserver::default();
        o.observe(&one(7, snap(100, Some(10), Some("a"))), 1);
        // Saiu do tracker (fechou): a sessão fecha.
        assert_eq!(kinds(&o.observe(&HashMap::new(), 2)), vec![HistoryEventKind::Closed]);
        // Uma conta fora de jogo que some não grava nada.
        o.observe(&one(8, snap(200, None, None)), 3);
        assert!(o.observe(&HashMap::new(), 4).is_empty());
    }

    #[test]
    fn a_new_process_for_the_account_closes_the_old_session_and_opens_a_new_one() {
        let mut o = SessionHistoryObserver::default();
        o.observe(&one(7, snap(100, Some(10), Some("a"))), 1);
        let events = o.observe(&one(7, snap(101, Some(10), Some("b"))), 2);
        assert_eq!(kinds(&events), vec![HistoryEventKind::Closed, HistoryEventKind::Joined]);
    }

    #[test]
    fn a_client_first_seen_already_dropped_has_no_session_to_end() {
        let mut o = SessionHistoryObserver::default();
        let mut dropped = snap(100, Some(10), Some("a"));
        dropped.drop = Some(ClientDrop {
            kind: DropKind::Crashed,
            reason: None,
            code: None,
            message: None,
            since_ms: 0,
        });
        assert!(o.observe(&one(7, dropped), 1).is_empty());
    }

    #[test]
    fn the_checkpoint_goes_out_every_minute_while_someone_plays_and_once_when_nobody_does() {
        let mut o = SessionHistoryObserver::default();
        o.observe(&one(7, snap(100, Some(10), Some("a"))), 1_000);
        let first = o.checkpoint_due(1_000, true).unwrap();
        assert_eq!(first.user_ids, vec![7]);
        assert!(o.checkpoint_due(30_000, false).is_none());
        assert!(o.checkpoint_due(61_000, false).is_some());
        o.observe(&HashMap::new(), 62_000);
        assert_eq!(o.checkpoint_due(62_000, true).unwrap().user_ids, Vec::<i64>::new());
        assert!(o.checkpoint_due(200_000, false).is_none());
    }

    #[test]
    fn the_log_session_remembers_where_it_joined() {
        let mut session = ClientLogSession::default();
        session.absorb("2026-10-10T01:00:00.000Z,1.0,abc,6 [FLog::Output] ! Joining game 'job-123' place 606849621 at 10.0.0.1\n", 1);
        assert_eq!(session.place_id, Some(606849621));
        assert_eq!(session.job_id.as_deref(), Some("job-123"));
    }

    #[test]
    fn the_export_file_name_carries_only_the_account_id_and_the_time() {
        use chrono::TimeZone;
        let at = chrono::Local.with_ymd_and_hms(2026, 10, 10, 9, 5, 7).unwrap();
        assert_eq!(history_export_file_name(42, at), "multialt-history-42-20261010-090507.csv");
    }
}
