//! Histórico de sessões por conta (ideia 6): entrou num jogo, trocou de
//! servidor, saiu, caiu (com o motivo do log), fechou, foi moderada. Ver
//! docs/features/history.md.
//!
//! - **Uma linha JSON por evento** em `RAMSessionHistory.jsonl`, na pasta de
//!   dados, escrita **na hora** em que o evento acontece (append). Fechar o app
//!   à força perde no máximo o que ainda não aconteceu.
//! - Sessão aberta quando o app fechou: o `RAMSessionHistory.open.json` guarda,
//!   a cada minuto, quem estava em jogo e quando; ao abrir de novo, essas
//!   sessões ganham o fim "o app fechou" naquele instante (`recover`).
//! - **Retenção:** 90 dias e no máximo `MAX_EVENTS` linhas; a compactação roda
//!   ao abrir o app (e a cada `COMPACT_EVERY` linhas novas), regravando o
//!   arquivo de forma atômica com a versão anterior em `.bak`.
//! - **Privacidade:** só ids, place, Job ID, códigos e a mensagem de kick do
//!   jogo. Nunca cookie, ticket nem nome de conta (o tipo não tem onde pôr).
//!
//! O arquivo está em `DATA_FILES` (entra no backup). O store não guarda nada
//! em memória além do contador de linhas: restaurar um backup vale na hora.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const SESSION_HISTORY_FILE_NAME: &str = "RAMSessionHistory.jsonl";
pub const RETENTION_DAYS: i64 = 90;
const DAY_MS: i64 = 86_400_000;
pub const MAX_EVENTS: usize = 50_000;
const COMPACT_EVERY: usize = 5_000;
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_MESSAGE_CHARS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistoryEventKind {
    /// Entrou num jogo (place, servidor).
    Joined,
    /// Trocou de place/servidor sem sair (teleporte): fecha a sessão anterior.
    Teleported,
    /// A pessoa saiu do jogo.
    Left,
    /// Caiu: o motivo vem do log (`dropKind`, `reason`, `code`, `message`).
    Dropped,
    /// O cliente fechou (ou deixou de ser acompanhado).
    Closed,
    /// O app fechou com a sessão aberta (recuperado ao abrir de novo).
    AppClosed,
    /// O Roblox disse que a conta está moderada (launch recusado).
    Moderated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEvent {
    pub at: i64,
    pub user_id: i64,
    pub kind: HistoryEventKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    /// `disconnected` / `kicked` / `serverShutdown` / `crashed` (o `DropKind` do monitor).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drop_kind: Option<String>,
    /// `connectionLost` / `joinedElsewhere` / `idle` / `other`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl HistoryEvent {
    pub fn new(at: i64, user_id: i64, kind: HistoryEventKind) -> Self {
        Self {
            at,
            user_id,
            kind,
            place_id: None,
            job_id: None,
            drop_kind: None,
            reason: None,
            code: None,
            message: None,
        }
    }

    pub fn at_place(mut self, place_id: Option<i64>, job_id: Option<String>) -> Self {
        self.place_id = place_id;
        self.job_id = job_id.filter(|j| !j.trim().is_empty());
        self
    }

    fn sanitized(mut self) -> Self {
        self.message = self
            .message
            .map(|m| m.chars().filter(|c| !c.is_control()).take(MAX_MESSAGE_CHARS).collect::<String>())
            .filter(|m| !m.trim().is_empty());
        self
    }
}

/// Como uma sessão terminou (o que a tela e o CSV mostram).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionEnd {
    /// Ainda em jogo.
    Ongoing,
    Left,
    Dropped,
    Teleported,
    Closed,
    AppClosed,
    /// Sem fim registrado e a conta não está mais em jogo.
    Unknown,
    Moderated,
}

/// Uma sessão: de quando entrou até como terminou.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub place_id: Option<i64>,
    pub job_id: Option<String>,
    pub end: SessionEnd,
    pub drop_kind: Option<String>,
    pub reason: Option<String>,
    pub code: Option<u32>,
    pub message: Option<String>,
}

impl SessionRecord {
    fn open(event: &HistoryEvent) -> Self {
        Self {
            started_at: event.at,
            ended_at: None,
            place_id: event.place_id,
            job_id: event.job_id.clone(),
            end: SessionEnd::Ongoing,
            drop_kind: None,
            reason: None,
            code: None,
            message: None,
        }
    }

    fn close(mut self, event: &HistoryEvent, end: SessionEnd) -> Self {
        self.ended_at = Some(event.at.max(self.started_at));
        self.end = end;
        if end == SessionEnd::Dropped {
            self.drop_kind = event.drop_kind.clone();
            self.reason = event.reason.clone();
            self.code = event.code;
            self.message = event.message.clone();
        }
        self
    }
}

/// As sessões de uma conta, da mais nova para a mais velha. `still_playing`:
/// a conta está em jogo agora (senão uma sessão sem fim vira `Unknown`, não
/// "em jogo").
pub fn build_sessions(events: &[HistoryEvent], user_id: i64, still_playing: bool) -> Vec<SessionRecord> {
    let mut mine: Vec<&HistoryEvent> = events.iter().filter(|e| e.user_id == user_id).collect();
    mine.sort_by_key(|e| e.at);

    let mut out = Vec::new();
    let mut open: Option<SessionRecord> = None;
    for event in mine {
        match event.kind {
            HistoryEventKind::Joined => {
                if let Some(s) = open.take() {
                    out.push(s.close(event, SessionEnd::Closed));
                }
                open = Some(SessionRecord::open(event));
            }
            HistoryEventKind::Teleported => {
                if let Some(s) = open.take() {
                    out.push(s.close(event, SessionEnd::Teleported));
                }
                open = Some(SessionRecord::open(event));
            }
            HistoryEventKind::Left => {
                if let Some(s) = open.take() {
                    out.push(s.close(event, SessionEnd::Left));
                }
            }
            HistoryEventKind::Dropped => {
                if let Some(s) = open.take() {
                    out.push(s.close(event, SessionEnd::Dropped));
                }
            }
            HistoryEventKind::Closed => {
                if let Some(s) = open.take() {
                    out.push(s.close(event, SessionEnd::Closed));
                }
            }
            HistoryEventKind::AppClosed => {
                if let Some(s) = open.take() {
                    out.push(s.close(event, SessionEnd::AppClosed));
                }
            }
            HistoryEventKind::Moderated => out.push(SessionRecord {
                started_at: event.at,
                ended_at: Some(event.at),
                place_id: None,
                job_id: None,
                end: SessionEnd::Moderated,
                drop_kind: None,
                reason: None,
                code: None,
                message: None,
            }),
        }
    }
    if let Some(mut s) = open {
        if !still_playing {
            s.end = SessionEnd::Unknown;
        }
        out.push(s);
    }
    // Mais nova primeiro, pelo início (uma marca de moderação no meio de uma
    // sessão aberta fica antes dela).
    out.reverse();
    out.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    out
}

/// Quem estava em jogo quando o app parou de olhar, e quando.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSessionsCheckpoint {
    pub at: i64,
    pub user_ids: Vec<i64>,
}

/// Fins "o app fechou" para as sessões que o checkpoint diz que estavam abertas
/// e que o arquivo não fechou. Puro.
pub fn recovery_events(events: &[HistoryEvent], checkpoint: &OpenSessionsCheckpoint) -> Vec<HistoryEvent> {
    checkpoint
        .user_ids
        .iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .filter(|uid| {
            events
                .iter()
                .filter(|e| e.user_id == **uid && e.kind != HistoryEventKind::Moderated)
                .max_by_key(|e| e.at)
                .is_some_and(|last| {
                    matches!(last.kind, HistoryEventKind::Joined | HistoryEventKind::Teleported)
                        && last.at <= checkpoint.at
                })
        })
        .map(|uid| HistoryEvent::new(checkpoint.at, *uid, HistoryEventKind::AppClosed))
        .collect()
}

/// O que sobra depois da retenção: 90 dias e no máximo `max` eventos (os mais novos).
pub fn retained(mut events: Vec<HistoryEvent>, now_ms: i64, max: usize) -> Vec<HistoryEvent> {
    let cutoff = now_ms - RETENTION_DAYS * DAY_MS;
    events.retain(|e| e.at >= cutoff);
    events.sort_by_key(|e| e.at);
    if events.len() > max {
        events.drain(..events.len() - max);
    }
    events
}

fn sibling(file: &Path, suffix: &str) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

pub struct SessionHistoryStore {
    file_path: PathBuf,
    /// Linhas gravadas desde a última compactação (gatilho da próxima).
    appended: Mutex<usize>,
}

impl SessionHistoryStore {
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            file_path,
            appended: Mutex::new(0),
        }
    }

    fn checkpoint_path(&self) -> PathBuf {
        self.file_path.with_file_name("RAMSessionHistory.open.json")
    }

    /// Todos os eventos. Linha quebrada (app fechado no meio de uma escrita) é
    /// ignorada, não derruba o resto.
    pub fn read_all(&self) -> Vec<HistoryEvent> {
        let Ok(meta) = fs::metadata(&self.file_path) else {
            return Vec::new();
        };
        if meta.len() > MAX_FILE_BYTES {
            eprintln!("Session history is too large to read ({} bytes)", meta.len());
            return Vec::new();
        }
        let Ok(text) = fs::read_to_string(&self.file_path) else {
            return Vec::new();
        };
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<HistoryEvent>(l).ok())
            .collect()
    }

    /// Grava os eventos no fim do arquivo, um por linha, na hora.
    pub fn append(&self, events: &[HistoryEvent], now_ms: i64) -> Result<(), String> {
        if events.is_empty() {
            return Ok(());
        }
        let mut lines = String::new();
        for event in events {
            let line = serde_json::to_string(&event.clone().sanitized())
                .map_err(|e| format!("Failed to serialize a history event: {e}"))?;
            lines.push_str(&line);
            lines.push('\n');
        }
        if let Some(parent) = self.file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)
            .map_err(|e| format!("Failed to open the session history: {e}"))?;
        file.write_all(lines.as_bytes())
            .map_err(|e| format!("Failed to write the session history: {e}"))?;
        let _ = file.flush();

        let due = match self.appended.lock() {
            Ok(mut n) => {
                *n += events.len();
                *n >= COMPACT_EVERY
            }
            Err(_) => false,
        };
        if due {
            let _ = self.compact(now_ms);
        }
        Ok(())
    }

    /// Aplica a retenção e regrava (atômico, com `.bak`) se algo saiu.
    pub fn compact(&self, now_ms: i64) -> Result<bool, String> {
        if let Ok(mut n) = self.appended.lock() {
            *n = 0;
        }
        let all = self.read_all();
        let before = all.len();
        let raw_lines = fs::read_to_string(&self.file_path)
            .map(|t| t.lines().filter(|l| !l.trim().is_empty()).count())
            .unwrap_or(0);
        let kept = retained(all, now_ms, MAX_EVENTS);
        if kept.len() == before && raw_lines == before {
            return Ok(false);
        }
        let mut bytes = Vec::new();
        for event in &kept {
            let line = serde_json::to_string(event).map_err(|e| e.to_string())?;
            bytes.extend_from_slice(line.as_bytes());
            bytes.push(b'\n');
        }
        fs::copy(&self.file_path, sibling(&self.file_path, ".bak"))
            .map_err(|e| format!("Failed to keep the previous session history: {e}"))?;
        let tmp = sibling(&self.file_path, ".tmp");
        crate::data::versions::write_all_synced(&tmp, &bytes).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            format!("Failed to write the session history: {e}")
        })?;
        crate::data::versions::atomic_replace(&tmp, &self.file_path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            e
        })?;
        Ok(true)
    }

    /// Guarda quem está em jogo agora (vazio = apaga o arquivo).
    pub fn write_checkpoint(&self, checkpoint: &OpenSessionsCheckpoint) {
        let path = self.checkpoint_path();
        if checkpoint.user_ids.is_empty() {
            let _ = fs::remove_file(&path);
            return;
        }
        let Ok(bytes) = serde_json::to_vec(checkpoint) else {
            return;
        };
        let tmp = sibling(&path, ".tmp");
        if fs::write(&tmp, &bytes).is_ok() && crate::data::versions::atomic_replace(&tmp, &path).is_err() {
            let _ = fs::remove_file(&tmp);
        }
    }

    /// Ao abrir o app: fecha as sessões que o app deixou abertas e aplica a
    /// retenção. Devolve quantas sessões foram fechadas.
    pub fn recover(&self, now_ms: i64) -> usize {
        let path = self.checkpoint_path();
        let mut closed = 0;
        if let Some(checkpoint) = fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<OpenSessionsCheckpoint>(&b).ok())
        {
            let ends = recovery_events(&self.read_all(), &checkpoint);
            closed = ends.len();
            if self.append(&ends, now_ms).is_ok() {
                let _ = fs::remove_file(&path);
            }
        } else {
            let _ = fs::remove_file(&path);
        }
        if let Err(e) = self.compact(now_ms) {
            eprintln!("Session history compaction failed: {e}");
        }
        closed
    }
}

#[cfg(test)]
mod session_history_store_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!("ram-history-{}-{nanos}-{n}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn file(&self) -> PathBuf {
            self.0.join(SESSION_HISTORY_FILE_NAME)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const NOW: i64 = 1_800_000_000_000;

    fn joined(at: i64, uid: i64, place: i64) -> HistoryEvent {
        HistoryEvent::new(at, uid, HistoryEventKind::Joined).at_place(Some(place), Some(format!("job-{place}")))
    }

    #[test]
    fn each_event_is_one_line_written_at_once() {
        let dir = TempDir::new();
        let store = SessionHistoryStore::new(dir.file());
        store.append(&[joined(1, 7, 10)], NOW).unwrap();
        store.append(&[HistoryEvent::new(2, 7, HistoryEventKind::Left)], NOW).unwrap();

        let text = fs::read_to_string(dir.file()).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.lines().next().unwrap().contains("\"placeId\":10"), "{text}");
        assert_eq!(store.read_all().len(), 2);
    }

    #[test]
    fn a_half_written_last_line_is_skipped_not_fatal() {
        let dir = TempDir::new();
        let store = SessionHistoryStore::new(dir.file());
        store.append(&[joined(1, 7, 10)], NOW).unwrap();
        // App morto no meio da escrita.
        let mut f = fs::OpenOptions::new().append(true).open(dir.file()).unwrap();
        f.write_all(b"{\"at\":2,\"userId\":7,\"ki").unwrap();
        drop(f);
        assert_eq!(store.read_all(), vec![joined(1, 7, 10)]);
    }

    #[test]
    fn retention_keeps_90_days_and_the_newest_events() {
        let old = joined(NOW - 91 * DAY_MS, 1, 1);
        let recent = joined(NOW - 89 * DAY_MS, 1, 2);
        assert_eq!(retained(vec![old, recent.clone()], NOW, 10), vec![recent]);

        let many: Vec<_> = (0..5).map(|i| joined(NOW - i, 1, i)).collect();
        let kept = retained(many, NOW, 3);
        assert_eq!(kept.iter().map(|e| e.at).collect::<Vec<_>>(), vec![NOW - 2, NOW - 1, NOW]);
    }

    #[test]
    fn compaction_rewrites_atomically_and_keeps_a_bak() {
        let dir = TempDir::new();
        let store = SessionHistoryStore::new(dir.file());
        store
            .append(&[joined(NOW - 100 * DAY_MS, 1, 1), joined(NOW - DAY_MS, 1, 2)], NOW)
            .unwrap();
        assert!(store.compact(NOW).unwrap());
        assert_eq!(store.read_all().len(), 1);
        assert!(sibling(&dir.file(), ".bak").exists());
        // Nada a tirar: não regrava.
        assert!(!store.compact(NOW).unwrap());
    }

    #[test]
    fn a_session_left_open_when_the_app_died_ends_at_the_last_checkpoint() {
        let dir = TempDir::new();
        let store = SessionHistoryStore::new(dir.file());
        store.append(&[joined(NOW - 10_000, 7, 10), joined(NOW - 9_000, 8, 10)], NOW).unwrap();
        store.append(&[HistoryEvent::new(NOW - 8_000, 8, HistoryEventKind::Left)], NOW).unwrap();
        store.write_checkpoint(&OpenSessionsCheckpoint {
            at: NOW - 5_000,
            user_ids: vec![7, 8],
        });

        assert_eq!(store.recover(NOW), 1, "only 7 was still open");
        let sessions = build_sessions(&store.read_all(), 7, false);
        assert_eq!(sessions[0].end, SessionEnd::AppClosed);
        assert_eq!(sessions[0].ended_at, Some(NOW - 5_000));
        assert!(!store.checkpoint_path().exists());
        // Abrir de novo não fecha duas vezes.
        assert_eq!(store.recover(NOW + 1), 0);
    }

    #[test]
    fn an_empty_checkpoint_removes_the_file() {
        let dir = TempDir::new();
        let store = SessionHistoryStore::new(dir.file());
        store.write_checkpoint(&OpenSessionsCheckpoint { at: 1, user_ids: vec![1] });
        assert!(store.checkpoint_path().exists());
        store.write_checkpoint(&OpenSessionsCheckpoint { at: 2, user_ids: vec![] });
        assert!(!store.checkpoint_path().exists());
    }

    #[test]
    fn the_file_holds_only_ids_places_codes_and_the_game_message() {
        // Privacidade: o formato não tem onde pôr cookie, ticket ou nome.
        let mut event = HistoryEvent::new(1, 7, HistoryEventKind::Dropped).at_place(Some(1), Some("job".into()));
        event.drop_kind = Some("kicked".into());
        event.reason = Some("other".into());
        event.code = Some(267);
        event.message = Some("You were kicked\nfor spam".into());
        let json = serde_json::to_value(event.sanitized()).unwrap();
        let keys: HashSet<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
        let allowed: HashSet<&str> =
            ["at", "userId", "kind", "placeId", "jobId", "dropKind", "reason", "code", "message"].into();
        assert!(keys.is_subset(&allowed), "{keys:?}");
        assert_eq!(json["message"], "You were kickedfor spam", "control characters are dropped");
    }
}

#[cfg(test)]
mod session_history_build_tests {
    use super::*;

    fn ev(at: i64, kind: HistoryEventKind) -> HistoryEvent {
        HistoryEvent::new(at, 1, kind)
    }
    fn at_place(at: i64, kind: HistoryEventKind, place: i64, job: &str) -> HistoryEvent {
        ev(at, kind).at_place(Some(place), Some(job.into()))
    }

    #[test]
    fn a_session_runs_from_join_to_how_it_ended_newest_first() {
        let mut drop = ev(300, HistoryEventKind::Dropped);
        drop.drop_kind = Some("disconnected".into());
        drop.reason = Some("connectionLost".into());
        drop.code = Some(277);
        let events = vec![
            at_place(100, HistoryEventKind::Joined, 10, "a"),
            ev(200, HistoryEventKind::Left),
            at_place(250, HistoryEventKind::Joined, 20, "b"),
            drop,
        ];
        let sessions = build_sessions(&events, 1, false);
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].place_id, Some(20));
        assert_eq!(sessions[0].end, SessionEnd::Dropped);
        assert_eq!(sessions[0].code, Some(277));
        assert_eq!(sessions[0].reason.as_deref(), Some("connectionLost"));
        assert_eq!((sessions[1].started_at, sessions[1].ended_at), (100, Some(200)));
        assert_eq!(sessions[1].end, SessionEnd::Left);
    }

    #[test]
    fn a_teleport_ends_one_session_and_starts_the_next() {
        let events = vec![
            at_place(100, HistoryEventKind::Joined, 10, "a"),
            at_place(160, HistoryEventKind::Teleported, 11, "b"),
        ];
        let sessions = build_sessions(&events, 1, true);
        assert_eq!(sessions[0].end, SessionEnd::Ongoing);
        assert_eq!(sessions[0].place_id, Some(11));
        assert_eq!(sessions[1].end, SessionEnd::Teleported);
        assert_eq!(sessions[1].ended_at, Some(160));
    }

    #[test]
    fn an_open_session_of_an_account_no_longer_playing_is_unknown_not_ongoing() {
        let events = vec![at_place(100, HistoryEventKind::Joined, 10, "a")];
        assert_eq!(build_sessions(&events, 1, false)[0].end, SessionEnd::Unknown);
        assert_eq!(build_sessions(&events, 1, true)[0].end, SessionEnd::Ongoing);
    }

    #[test]
    fn other_accounts_and_moderation_markers() {
        let events = vec![
            at_place(100, HistoryEventKind::Joined, 10, "a"),
            HistoryEvent::new(120, 2, HistoryEventKind::Joined),
            ev(130, HistoryEventKind::Moderated),
        ];
        let sessions = build_sessions(&events, 1, true);
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].end, SessionEnd::Moderated);
        assert_eq!(sessions[1].end, SessionEnd::Ongoing);
    }

    #[test]
    fn recovery_only_closes_sessions_still_open_in_the_file() {
        let events = vec![
            at_place(100, HistoryEventKind::Joined, 10, "a"),
            HistoryEvent::new(100, 2, HistoryEventKind::Joined),
            HistoryEvent::new(150, 2, HistoryEventKind::Closed),
        ];
        let ends = recovery_events(&events, &OpenSessionsCheckpoint { at: 200, user_ids: vec![1, 2, 3, 1] });
        assert_eq!(ends, vec![HistoryEvent::new(200, 1, HistoryEventKind::AppClosed)]);
    }
}
