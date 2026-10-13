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
    /// Início da sessão aberta (o `at` do `joined`/`teleported`).
    since_ms: i64,
}

/// O jogo em que uma conta está agora e desde quando (página Session). É a
/// sessão aberta do histórico — o mesmo início do "Playing now".
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentSession {
    pub user_id: i64,
    pub place_id: i64,
    pub job_id: Option<String>,
    pub since_ms: i64,
    /// Servidor privado/VIP? `None` = não se sabe (conta aberta pelo site, ou
    /// teleportada para outro place).
    pub private_server: Option<bool>,
}

/// Público ou privado. O `! Joining game` do log não diz; diz o launch que o
/// app fez (`remember_launch_target`), e só enquanto a conta continua no place
/// para onde foi mandada.
fn private_server_of(target: Option<&LaunchedTarget>, place_id: i64) -> Option<bool> {
    target.filter(|t| t.place_id == place_id).map(|t| t.private)
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
                        since_ms: now_ms,
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
                    client.since_ms = now_ms;
                }
            } else if playing {
                // Entrou de novo (depois de cair ou sair, no mesmo cliente).
                events.push(
                    HistoryEvent::new(now_ms, uid, HistoryEventKind::Joined)
                        .at_place(snap.place_id, snap.job_id.clone()),
                );
                client.in_game = true;
                client.since_ms = now_ms;
            }
            client.place_id = snap.place_id;
            client.job_id = snap.job_id.clone();
        }
        events
    }

    /// Quem está num jogo agora, onde e desde quando (sem o tipo de servidor:
    /// quem sabe isso é o launch, ver `get_current_sessions`).
    fn current_sessions(&self) -> Vec<CurrentSession> {
        let mut sessions: Vec<CurrentSession> = self
            .clients
            .iter()
            .filter(|(_, c)| c.in_game)
            .filter_map(|(uid, c)| {
                Some(CurrentSession {
                    user_id: *uid,
                    place_id: c.place_id?,
                    job_id: c.job_id.clone(),
                    since_ms: c.since_ms,
                    private_server: None,
                })
            })
            .collect();
        sessions.sort_by_key(|s| s.user_id);
        sessions
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

/// Quem está num jogo agora, onde, desde quando e se o servidor é privado
/// (página Session → "In game"). Só memória: o observador já sabe, nada é lido
/// do disco nem do log. A tela relê no `session-history-changed`, que sai a
/// cada entrada, teleporte, queda ou saída — sem polling.
#[tauri::command]
fn get_current_sessions() -> Vec<CurrentSession> {
    let mut sessions = SESSION_HISTORY_OBSERVER
        .lock()
        .map(|o| o.current_sessions())
        .unwrap_or_default();
    for session in &mut sessions {
        session.private_server =
            private_server_of(launched_target_of(session.user_id).as_ref(), session.place_id);
    }
    sessions
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

    /// A Sessão mostra o jogo e o tempo em jogo de cada conta: o mesmo início
    /// que o histórico grava ("Playing now"), sem ler nada de novo.
    #[test]
    fn the_current_session_says_where_and_since_when() {
        let mut o = SessionHistoryObserver::default();
        o.observe(&one(7, snap(100, None, None)), 500);
        assert!(o.current_sessions().is_empty(), "loading is not a session yet");

        o.observe(&one(7, snap(100, Some(10), Some("a"))), 1_000);
        o.observe(&one(7, snap(100, Some(10), Some("a"))), 5_000);
        assert_eq!(
            o.current_sessions(),
            vec![CurrentSession {
                user_id: 7,
                place_id: 10,
                job_id: Some("a".into()),
                since_ms: 1_000,
                private_server: None,
            }]
        );

        // Teleporte: sessão nova, como no histórico.
        o.observe(&one(7, snap(100, Some(11), Some("b"))), 6_000);
        let now = o.current_sessions();
        assert_eq!((now[0].place_id, now[0].since_ms), (11, 6_000));

        // Caiu: não está jogando.
        let mut dropped = snap(100, Some(11), Some("b"));
        dropped.drop = Some(ClientDrop {
            kind: DropKind::Crashed,
            reason: None,
            code: None,
            message: None,
            since_ms: 7_000,
        });
        o.observe(&one(7, dropped), 7_000);
        assert!(o.current_sessions().is_empty());

        // Entrou de novo: conta a partir de agora.
        o.observe(&one(7, snap(100, Some(11), Some("c"))), 9_000);
        assert_eq!(o.current_sessions()[0].since_ms, 9_000);

        // Cliente novo da conta: o início é o do processo novo.
        o.observe(&one(7, snap(101, Some(11), Some("d"))), 12_000);
        assert_eq!(o.current_sessions()[0].since_ms, 12_000);
    }

    #[test]
    fn current_sessions_come_in_account_order() {
        let mut o = SessionHistoryObserver::default();
        let both = HashMap::from([(9, snap(200, Some(5), None)), (3, snap(100, Some(6), None))]);
        o.observe(&both, 1);
        let ids: Vec<i64> = o.current_sessions().iter().map(|s| s.user_id).collect();
        assert_eq!(ids, vec![3, 9]);
    }

    /// Público ou privado: o log não diz; diz o launch que o app fez, e só
    /// enquanto a conta continua no place para onde foi mandada.
    #[test]
    fn the_server_kind_comes_from_where_the_app_sent_the_account() {
        let target = |place_id: i64, private: bool| LaunchedTarget {
            place_id,
            job_id: if private { "vip:abc".into() } else { String::new() },
            launch_data: String::new(),
            join_vip: private,
            link_code: String::new(),
            private,
        };
        assert_eq!(private_server_of(Some(&target(10, true)), 10), Some(true));
        assert_eq!(private_server_of(Some(&target(10, false)), 10), Some(false));
        // Teleportou para outro place: o launch não diz mais nada.
        assert_eq!(private_server_of(Some(&target(10, true)), 11), None);
        // Aberta fora do app: não se sabe.
        assert_eq!(private_server_of(None, 10), None);
    }

    #[test]
    fn the_export_file_name_carries_only_the_account_id_and_the_time() {
        use chrono::TimeZone;
        let at = chrono::Local.with_ymd_and_hms(2026, 10, 10, 9, 5, 7).unwrap();
        assert_eq!(history_export_file_name(42, at), "multialt-history-42-20261010-090507.csv");
    }
}
