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
