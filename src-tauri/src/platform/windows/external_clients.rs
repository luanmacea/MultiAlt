// ── Clientes abertos fora do app (pelo site) ────────────────────────────────
//
// Quem abre um jogo pelo site gera um `RobloxPlayerBeta` que o app não lançou,
// então o `ProcessTracker` não sabe de quem ele é. O Roblox escreve um log por
// cliente em `%LOCALAPPDATA%\Roblox\logs`, e é por ele que o cliente é
// reconhecido:
//
// - linha de dados: `2026-10-03T21:11:19.354Z,0.354978,3ed8,6,Warning [...]` —
//   o 3º campo é o id (hex) da thread que escreveu. As primeiras linhas saem da
//   thread principal, que pertence ao processo enquanto ele roda: thread → PID
//   pelo Toolhelp32 (`TH32CS_SNAPTHREAD`);
// - a conta: `[FLog::GameJoinLoadTime] ... userid:75444209,` (vale a última,
//   teleporte repete) — só aparece depois de o cliente entrar num jogo;
// - reserva quando a thread já não serve: a origem do log (horário da 1ª linha
//   menos o tempo decorrido) perto do horário de criação do processo, e só se
//   não houver ambiguidade.
//
// Nada aqui fecha, mexe ou foca cliente: só lê o log e a lista de processos.
// Ver docs/features/external-clients.md.

use std::collections::HashSet;

/// Logs mais velhos que isto não são nem listados.
pub const EXTERNAL_LOG_MAX_AGE_MS: i64 = 24 * 60 * 60 * 1000;
/// Quanto do começo do log é lido para achar a 1ª linha de dados.
const EXTERNAL_LOG_HEAD_BYTES: u64 = 16 * 1024;
/// Teto de leitura por log por varredura (o resto fica para a próxima).
const EXTERNAL_LOG_READ_MAX: u64 = 8 * 1024 * 1024;
/// Cliente sem conta reconhecida só aparece como "não identificado" depois
/// disto: antes, um launch do próprio app ainda pode estar registrando o PID.
pub const UNIDENTIFIED_GRACE_MS: i64 = 15_000;
/// A origem do log fica uns 2–3 s depois da criação do processo (medido em
/// 03/10/2026: processo 21:11:16.709, origem 21:11:19.0). Janela da regra pela
/// thread — só uma trava contra id de thread reaproveitado.
const THREAD_MATCH_BEFORE_MS: i64 = 5_000;
const THREAD_MATCH_AFTER_MS: i64 = 60_000;
/// Janela da regra de reserva (sem thread), bem mais estreita.
const TIME_MATCH_BEFORE_MS: i64 = 2_000;
const TIME_MATCH_AFTER_MS: i64 = 10_000;

/// O que a 1ª linha de dados do log diz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RobloxLogHeader {
    /// Thread que escreveu a linha (a principal do cliente).
    pub thread_id: u32,
    /// Horário da linha menos o tempo decorrido: quando o log começou (ms Unix).
    pub origin_ms: i64,
}

/// Conta e jogo, pelo que o log disse por último.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RobloxLogIdentity {
    pub user_id: Option<i64>,
    pub place_id: Option<i64>,
    pub job_id: Option<String>,
}

/// Lê `<ts>,<decorrido>,<thread hex>,...` do começo de uma linha.
fn parse_log_line_prefix(line: &str) -> Option<(i64, f64, u32)> {
    let mut parts = line.splitn(4, ',');
    let ts = parts.next()?.trim();
    let elapsed = parts.next()?.trim();
    let thread = parts.next()?.trim();
    parts.next()?;
    let ts_ms = chrono::DateTime::parse_from_rfc3339(ts).ok()?.timestamp_millis();
    let elapsed: f64 = elapsed.parse().ok()?;
    if !elapsed.is_finite() || elapsed < 0.0 {
        return None;
    }
    if thread.is_empty() || thread.len() > 8 {
        return None;
    }
    let thread_id = u32::from_str_radix(thread, 16).ok()?;
    Some((ts_ms, elapsed, thread_id))
}

/// A 1ª linha de dados do log (a 1ª linha do arquivo é um aviso sem campos).
pub fn parse_roblox_log_header(text: &str) -> Option<RobloxLogHeader> {
    text.lines().find_map(|line| {
        let (ts_ms, elapsed, thread_id) = parse_log_line_prefix(line)?;
        Some(RobloxLogHeader {
            thread_id,
            origin_ms: ts_ms - (elapsed * 1000.0).round() as i64,
        })
    })
}

// O parser das linhas é o mesmo da saúde dos clientes do app
// (src-tauri/src/roblox_log.rs).
use crate::roblox_log::{digits_after, game_join_place_id, game_join_user_id, GAME_JOIN_LOAD_TIME_MARKER};

impl RobloxLogIdentity {
    /// Aplica uma linha: o que ela disser substitui o que se sabia.
    pub fn absorb_line(&mut self, line: &str) {
        if line.contains(GAME_JOIN_LOAD_TIME_MARKER) {
            if let Some(user_id) = game_join_user_id(line) {
                self.user_id = Some(user_id);
            }
            if let Some(place_id) = game_join_place_id(line) {
                self.place_id = Some(place_id);
            }
            return;
        }
        // `! Joining game '<jobId>' place <placeId> at <ip>`
        if let Some(rest) = line.split("! Joining game '").nth(1) {
            if let Some(end) = rest.find('\'') {
                let job = &rest[..end];
                if !job.is_empty() {
                    self.job_id = Some(job.to_string());
                }
                if let Some(place_id) = digits_after(&rest[end..], "' place ") {
                    self.place_id = Some(place_id);
                }
            }
            return;
        }
        // O ticket do join (`doTeleport: joinScriptUrl ...ticket={"UserId"%3a75444209...`)
        // também traz a conta, às vezes antes do GameJoinLoadTime.
        if line.contains("joinScriptUrl") {
            if let Some(user_id) = digits_after(line, "\"UserId\"%3a")
                .or_else(|| digits_after(line, "\"UserId\":"))
            {
                self.user_id = Some(user_id);
            }
        }
    }

    pub fn absorb(&mut self, text: &str) {
        for line in text.lines() {
            self.absorb_line(line);
        }
    }
}

pub fn parse_roblox_log_identity(text: &str) -> RobloxLogIdentity {
    let mut identity = RobloxLogIdentity::default();
    identity.absorb(text);
    identity
}

/// `<versão>_<YYYYMMDDTHHMMSSZ>_Player_<HEX>_last.log`, sem o CrashHandler.
pub fn is_player_log_name(name: &str) -> bool {
    name.ends_with("_last.log") && name.contains("_Player_") && !name.contains("_CrashHandler")
}

/// Horário do nome do log (resolução de segundo), em ms Unix.
pub fn player_log_name_started_ms(name: &str) -> Option<i64> {
    if !is_player_log_name(name) {
        return None;
    }
    let before_player = name.split("_Player_").next()?;
    let stamp = before_player.rsplit('_').next()?;
    let stamp = stamp.strip_suffix('Z')?;
    let naive = chrono::NaiveDateTime::parse_from_str(stamp, "%Y%m%dT%H%M%S").ok()?;
    Some(naive.and_utc().timestamp_millis())
}

/// FILETIME (100 ns desde 1601) → ms Unix.
pub fn filetime_to_unix_ms(low: u32, high: u32) -> i64 {
    let ticks = ((high as u64) << 32) | low as u64;
    (ticks / 10_000) as i64 - 11_644_473_600_000
}

/// Um `RobloxPlayerBeta` que o app ainda não conhece.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessCandidate {
    pub pid: u32,
    pub created_ms: Option<i64>,
}

fn within(origin_ms: i64, created_ms: i64, before: i64, after: i64) -> bool {
    origin_ms >= created_ms - before && origin_ms <= created_ms + after
}

/// Casa logs com processos. `logs` vem do mais novo para o mais velho; volta
/// PID → índice em `logs`. Um log e um processo casam no máximo uma vez.
///
/// 1. Thread: o dono da thread do cabeçalho é o processo, e a origem do log
///    cabe na janela da criação dele (trava contra id reaproveitado).
/// 2. Reserva: sem thread, só pelo horário, e só quando o par é único dos dois
///    lados — na dúvida não casa (o cliente cai em "não identificado").
pub fn match_logs_to_processes(
    procs: &[ProcessCandidate],
    logs: &[RobloxLogHeader],
    thread_owner: &HashMap<u32, u32>,
) -> HashMap<u32, usize> {
    let mut matched: HashMap<u32, usize> = HashMap::new();
    let mut used_logs: HashSet<usize> = HashSet::new();

    for (index, header) in logs.iter().enumerate() {
        let Some(owner) = thread_owner.get(&header.thread_id) else {
            continue;
        };
        let Some(proc_) = procs.iter().find(|p| p.pid == *owner) else {
            continue;
        };
        if matched.contains_key(&proc_.pid) {
            continue;
        }
        if let Some(created) = proc_.created_ms {
            if !within(header.origin_ms, created, THREAD_MATCH_BEFORE_MS, THREAD_MATCH_AFTER_MS) {
                continue;
            }
        }
        matched.insert(proc_.pid, index);
        used_logs.insert(index);
    }

    let fits = |header: &RobloxLogHeader, proc_: &ProcessCandidate| {
        proc_
            .created_ms
            .is_some_and(|c| within(header.origin_ms, c, TIME_MATCH_BEFORE_MS, TIME_MATCH_AFTER_MS))
    };
    let open_procs: Vec<&ProcessCandidate> =
        procs.iter().filter(|p| !matched.contains_key(&p.pid)).collect();
    let open_logs: Vec<usize> = (0..logs.len()).filter(|i| !used_logs.contains(i)).collect();
    let mut by_time: Vec<(u32, usize)> = Vec::new();
    for proc_ in &open_procs {
        let fitting: Vec<usize> = open_logs
            .iter()
            .copied()
            .filter(|i| fits(&logs[*i], proc_))
            .collect();
        if fitting.len() != 1 {
            continue;
        }
        let log = fitting[0];
        let rivals = open_procs.iter().filter(|p| fits(&logs[log], p)).count();
        if rivals == 1 {
            by_time.push((proc_.pid, log));
        }
    }
    for (pid, log) in by_time {
        matched.insert(pid, log);
    }
    matched
}

/// Por que um cliente aparece como "não identificado".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UnidentifiedReason {
    /// Nenhum log casou com o processo.
    NoLog,
    /// Log achado, mas o cliente ainda não entrou num jogo (sem `userid:`).
    WaitingForGame,
    /// A conta do log não está entre as contas salvas.
    UnknownAccount,
    /// A conta do log já tem outro cliente vivo registrado no app.
    AccountBusy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnidentifiedClient {
    pub pid: u32,
    pub reason: UnidentifiedReason,
    /// A conta que o log diz, quando diz (fora das contas salvas, ou ocupada).
    pub user_id: Option<i64>,
    pub place_id: Option<i64>,
    pub job_id: Option<String>,
    /// Criação do processo (ms Unix), quando se sabe.
    pub started_at_ms: Option<i64>,
}

/// O que fazer com um cliente de fora, dada a conta que o log diz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalDecision {
    Adopt(i64),
    Unidentified(UnidentifiedReason),
}

/// `tracked_alive`: conta → PID vivo que o app já acompanha.
pub fn decide_external_client(
    has_log: bool,
    user_id: Option<i64>,
    saved_accounts: &HashSet<i64>,
    tracked_alive: &HashMap<i64, u32>,
) -> ExternalDecision {
    if !has_log {
        return ExternalDecision::Unidentified(UnidentifiedReason::NoLog);
    }
    let Some(user_id) = user_id else {
        return ExternalDecision::Unidentified(UnidentifiedReason::WaitingForGame);
    };
    if !saved_accounts.contains(&user_id) {
        return ExternalDecision::Unidentified(UnidentifiedReason::UnknownAccount);
    }
    if tracked_alive.contains_key(&user_id) {
        return ExternalDecision::Unidentified(UnidentifiedReason::AccountBusy);
    }
    ExternalDecision::Adopt(user_id)
}

/// Um log da pasta, como a listagem devolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerLogFile {
    pub path: PathBuf,
    pub name: String,
    pub modified_ms: i64,
    pub len: u64,
}

/// O que o scanner precisa do sistema. Existe para os testes trocarem o
/// Windows por um dublê; a implementação real é [`WindowsExternalClientOs`].
pub trait ExternalClientOs {
    fn roblox_pids(&self) -> Vec<u32>;
    fn process_created_ms(&self, pid: u32) -> Option<i64>;
    fn thread_owners(&self, pids: &HashSet<u32>) -> HashMap<u32, u32>;
    fn list_player_logs(&self) -> Vec<PlayerLogFile>;
    fn read_log(&self, path: &std::path::Path, offset: u64, max: u64) -> Option<Vec<u8>>;
    fn now_ms(&self) -> i64;
}

#[derive(Debug, Clone)]
struct ExternalPidState {
    created_ms: Option<i64>,
    first_seen_ms: i64,
    log: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
struct ExternalLogState {
    /// Até onde a identidade já foi lida (sempre num fim de linha).
    offset: u64,
    identity: RobloxLogIdentity,
}

#[derive(Debug, Clone, Copy, Default)]
struct LogHeaderState {
    header: Option<RobloxLogHeader>,
    /// Tamanho do arquivo quando o cabeçalho foi tentado (só tenta de novo se crescer).
    tried_len: u64,
}

/// Acha o log de cada processo: cabeçalho de cada log da pasta + dono da
/// thread que o escreveu (`match_logs_to_processes`). Usado pela varredura de
/// clientes de fora **e** pelo monitor de quedas (`commands/client_health.rs`),
/// que precisa do log dos clientes que o próprio app abriu.
///
/// Guarda os cabeçalhos já lidos e a última tentativa: se nada mudou (mesmos
/// PIDs, mesmos logs e tamanhos), não tira outro retrato das threads.
#[derive(Debug, Default)]
pub struct LogLocator {
    headers: HashMap<PathBuf, LogHeaderState>,
    last_attempt: Option<(Vec<u32>, Vec<(PathBuf, u64)>)>,
}

impl LogLocator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Esquece tudo (nenhum processo para casar).
    pub fn clear(&mut self) {
        self.headers.clear();
        self.last_attempt = None;
    }

    /// Mantém só os cabeçalhos dos logs em `keep`.
    pub fn retain(&mut self, keep: &HashSet<PathBuf>) {
        self.headers.retain(|path, _| keep.contains(path));
    }

    pub fn forget(&mut self, path: &PathBuf) {
        self.headers.remove(path);
    }

    /// PID → log, para os `procs` sem log. `taken`: logs que já têm dono.
    pub fn locate(
        &mut self,
        os: &dyn ExternalClientOs,
        procs: &[ProcessCandidate],
        taken: &HashSet<PathBuf>,
        now: i64,
    ) -> HashMap<u32, PathBuf> {
        let mut found = HashMap::new();
        if procs.is_empty() {
            return found;
        }
        // Um log não pode ser de processo criado depois dele: corta os velhos
        // antes de abrir qualquer arquivo.
        let oldest_created = procs.iter().filter_map(|p| p.created_ms).min();
        let mut files: Vec<PlayerLogFile> = os
            .list_player_logs()
            .into_iter()
            .filter(|f| is_player_log_name(&f.name))
            .filter(|f| now - f.modified_ms <= EXTERNAL_LOG_MAX_AGE_MS)
            .filter(|f| !taken.contains(&f.path))
            .filter(|f| match (oldest_created, player_log_name_started_ms(&f.name)) {
                (Some(created), Some(started)) => started >= created - 60_000,
                _ => true,
            })
            .collect();
        files.sort_by(|a, b| {
            let ka = player_log_name_started_ms(&a.name).unwrap_or(a.modified_ms);
            let kb = player_log_name_started_ms(&b.name).unwrap_or(b.modified_ms);
            kb.cmp(&ka)
        });

        let mut pids: Vec<u32> = procs.iter().map(|p| p.pid).collect();
        pids.sort_unstable();
        let signature = (
            pids,
            files.iter().map(|f| (f.path.clone(), f.len)).collect::<Vec<_>>(),
        );
        if self.last_attempt.as_ref() == Some(&signature) {
            return found;
        }
        self.last_attempt = Some(signature);

        let mut headers: Vec<RobloxLogHeader> = Vec::new();
        let mut header_paths: Vec<PathBuf> = Vec::new();
        for file in &files {
            let entry = self.headers.entry(file.path.clone()).or_default();
            if entry.header.is_none() && (entry.tried_len == 0 || file.len > entry.tried_len) {
                entry.tried_len = file.len.max(1);
                if let Some(bytes) = os.read_log(&file.path, 0, EXTERNAL_LOG_HEAD_BYTES) {
                    entry.header = parse_roblox_log_header(&String::from_utf8_lossy(&bytes));
                }
            }
            if let Some(header) = entry.header {
                headers.push(header);
                header_paths.push(file.path.clone());
            }
        }
        if headers.is_empty() {
            return found;
        }

        let pid_set: HashSet<u32> = procs.iter().map(|p| p.pid).collect();
        let owners = os.thread_owners(&pid_set);
        for (pid, index) in match_logs_to_processes(procs, &headers, &owners) {
            found.insert(pid, header_paths[index].clone());
        }
        found
    }
}

/// Resultado de uma varredura: quem adotar e quem mostrar como não identificado.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExternalScanOutcome {
    /// (PID, conta) a registrar no tracker.
    pub adopt: Vec<(u32, i64)>,
    pub unidentified: Vec<UnidentifiedClient>,
}

/// Guarda o que já se descobriu por PID e por log, para cada varredura só ler
/// o que é novo. Sem candidato, a varredura custa só a lista de processos.
#[derive(Debug, Default)]
pub struct ExternalClientScanner {
    pids: HashMap<u32, ExternalPidState>,
    logs: HashMap<PathBuf, ExternalLogState>,
    locator: LogLocator,
}

impl ExternalClientScanner {
    pub fn new() -> Self {
        Self::default()
    }

    /// `tracked`: conta → PID que o app acompanha (vivo ou não).
    pub fn scan(
        &mut self,
        os: &dyn ExternalClientOs,
        tracked: &HashMap<i64, u32>,
        saved_accounts: &HashSet<i64>,
    ) -> ExternalScanOutcome {
        let now = os.now_ms();
        let alive: HashSet<u32> = os.roblox_pids().into_iter().collect();
        let tracked_pids: HashSet<u32> = tracked.values().copied().collect();
        let tracked_alive: HashMap<i64, u32> = tracked
            .iter()
            .filter(|(_, pid)| alive.contains(pid))
            .map(|(uid, pid)| (*uid, *pid))
            .collect();

        // Esquece PIDs que morreram ou que o app já acompanha.
        self.pids
            .retain(|pid, _| alive.contains(pid) && !tracked_pids.contains(pid));
        let mut candidates: Vec<u32> = alive
            .iter()
            .copied()
            .filter(|pid| !tracked_pids.contains(pid))
            .collect();
        candidates.sort_unstable();

        if candidates.is_empty() {
            self.logs.clear();
            self.locator.clear();
            return ExternalScanOutcome::default();
        }

        for pid in &candidates {
            self.pids.entry(*pid).or_insert_with(|| ExternalPidState {
                created_ms: os.process_created_ms(*pid),
                first_seen_ms: now,
                log: None,
            });
        }

        let unmatched: Vec<u32> = candidates
            .iter()
            .copied()
            .filter(|pid| self.pids.get(pid).is_some_and(|s| s.log.is_none()))
            .collect();
        if !unmatched.is_empty() {
            self.try_match_logs(os, &unmatched, now);
        }

        // Logs que nenhum processo vivo usa e que já saíram da janela somem.
        let in_use: HashSet<PathBuf> = self.pids.values().filter_map(|s| s.log.clone()).collect();
        if unmatched.is_empty() {
            self.logs.retain(|path, _| in_use.contains(path));
            self.locator.retain(&in_use);
        }

        let mut outcome = ExternalScanOutcome::default();
        for pid in candidates {
            let Some(state) = self.pids.get(&pid).cloned() else {
                continue;
            };
            let identity = match &state.log {
                Some(path) => self.read_identity(os, path),
                None => RobloxLogIdentity::default(),
            };
            match decide_external_client(
                state.log.is_some(),
                identity.user_id,
                saved_accounts,
                &tracked_alive,
            ) {
                ExternalDecision::Adopt(user_id) => {
                    // Dois clientes de fora da mesma conta: só o primeiro.
                    if outcome.adopt.iter().any(|(_, uid)| *uid == user_id) {
                        outcome.unidentified.push(UnidentifiedClient {
                            pid,
                            reason: UnidentifiedReason::AccountBusy,
                            user_id: Some(user_id),
                            place_id: identity.place_id,
                            job_id: identity.job_id.clone(),
                            started_at_ms: state.created_ms,
                        });
                        continue;
                    }
                    outcome.adopt.push((pid, user_id));
                }
                ExternalDecision::Unidentified(reason) => {
                    let since = state.created_ms.unwrap_or(state.first_seen_ms).min(state.first_seen_ms);
                    if now - since < UNIDENTIFIED_GRACE_MS {
                        continue;
                    }
                    outcome.unidentified.push(UnidentifiedClient {
                        pid,
                        reason,
                        user_id: identity.user_id,
                        place_id: identity.place_id,
                        job_id: identity.job_id.clone(),
                        started_at_ms: state.created_ms,
                    });
                }
            }
        }
        // Adotado sai do cache: daqui em diante quem cuida é o tracker.
        for (pid, _) in &outcome.adopt {
            if let Some(path) = self.pids.remove(pid).and_then(|s| s.log) {
                self.logs.remove(&path);
                self.locator.forget(&path);
            }
        }
        outcome
    }

    fn try_match_logs(&mut self, os: &dyn ExternalClientOs, unmatched: &[u32], now: i64) {
        let assigned: HashSet<PathBuf> = self.pids.values().filter_map(|s| s.log.clone()).collect();
        let procs: Vec<ProcessCandidate> = unmatched
            .iter()
            .map(|pid| ProcessCandidate {
                pid: *pid,
                created_ms: self.pids.get(pid).and_then(|s| s.created_ms),
            })
            .collect();
        for (pid, path) in self.locator.locate(os, &procs, &assigned, now) {
            if let Some(state) = self.pids.get_mut(&pid) {
                state.log = Some(path);
            }
        }
    }

    /// Lê só o que o log ganhou desde a última vez, até o último fim de linha.
    fn read_identity(&mut self, os: &dyn ExternalClientOs, path: &PathBuf) -> RobloxLogIdentity {
        let entry = self.logs.entry(path.clone()).or_default();
        if let Some(bytes) = os.read_log(path, entry.offset, EXTERNAL_LOG_READ_MAX) {
            let consumed = match bytes.iter().rposition(|b| *b == b'\n') {
                Some(last_newline) => last_newline + 1,
                // Pedaço cheio sem quebra de linha: segue assim mesmo.
                None if bytes.len() as u64 >= EXTERNAL_LOG_READ_MAX => bytes.len(),
                None => 0,
            };
            if consumed > 0 {
                entry
                    .identity
                    .absorb(&String::from_utf8_lossy(&bytes[..consumed]));
                entry.offset += consumed as u64;
            }
        }
        entry.identity.clone()
    }
}

static EXTERNAL_SCANNER: LazyLock<Mutex<ExternalClientScanner>> =
    LazyLock::new(|| Mutex::new(ExternalClientScanner::new()));
static UNIDENTIFIED_CLIENTS: LazyLock<Mutex<Vec<UnidentifiedClient>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// O sistema de verdade: Toolhelp32, GetProcessTimes e a pasta de logs.
pub struct WindowsExternalClientOs;

pub fn roblox_logs_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join("Roblox").join("logs"))
}

fn system_time_ms(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl ExternalClientOs for WindowsExternalClientOs {
    fn roblox_pids(&self) -> Vec<u32> {
        get_roblox_pids()
    }

    fn process_created_ms(&self, pid: u32) -> Option<i64> {
        use windows_sys::Win32::Foundation::FILETIME;
        use windows_sys::Win32::System::Threading::{
            GetProcessTimes, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return None;
            }
            let mut created: FILETIME = std::mem::zeroed();
            let mut exited: FILETIME = std::mem::zeroed();
            let mut kernel: FILETIME = std::mem::zeroed();
            let mut user: FILETIME = std::mem::zeroed();
            let ok = GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user);
            CloseHandle(handle);
            if ok == 0 {
                return None;
            }
            Some(filetime_to_unix_ms(created.dwLowDateTime, created.dwHighDateTime))
        }
    }

    fn thread_owners(&self, pids: &HashSet<u32>) -> HashMap<u32, u32> {
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
        };
        let mut owners = HashMap::new();
        if pids.is_empty() {
            return owners;
        }
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snapshot.is_null() || snapshot == INVALID_HANDLE_VALUE {
                return owners;
            }
            let mut entry: THREADENTRY32 = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
            if Thread32First(snapshot, &mut entry) != 0 {
                loop {
                    if pids.contains(&entry.th32OwnerProcessID) {
                        owners.insert(entry.th32ThreadID, entry.th32OwnerProcessID);
                    }
                    if Thread32Next(snapshot, &mut entry) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snapshot);
        }
        owners
    }

    fn list_player_logs(&self) -> Vec<PlayerLogFile> {
        let Some(dir) = roblox_logs_dir() else {
            return Vec::new();
        };
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !is_player_log_name(&name) {
                    return None;
                }
                let meta = entry.metadata().ok()?;
                Some(PlayerLogFile {
                    path: entry.path(),
                    name,
                    modified_ms: meta.modified().map(system_time_ms).unwrap_or(0),
                    len: meta.len(),
                })
            })
            .collect()
    }

    fn read_log(&self, path: &std::path::Path, offset: u64, max: u64) -> Option<Vec<u8>> {
        use std::io::{Read, Seek, SeekFrom};
        // Só leitura, com compartilhamento: o Roblox continua escrevendo.
        let mut file = std::fs::File::open(path).ok()?;
        file.seek(SeekFrom::Start(offset)).ok()?;
        let mut buf = Vec::new();
        file.take(max).read_to_end(&mut buf).ok()?;
        Some(buf)
    }

    fn now_ms(&self) -> i64 {
        system_time_ms(SystemTime::now())
    }
}

/// Uma varredura de verdade: lê, decide e registra no tracker quem foi
/// reconhecido. `saved_accounts`: conta → browser tracker id salvo.
/// Devolve (PID, conta) de quem foi adotado agora.
pub fn scan_external_clients(saved_accounts: &HashMap<i64, String>) -> Vec<(u32, i64)> {
    let tracker = tracker();
    let tracked: HashMap<i64, u32> = tracker
        .get_all()
        .into_iter()
        .map(|p| (p.user_id, p.pid))
        .collect();
    let ids: HashSet<i64> = saved_accounts.keys().copied().collect();
    let outcome = match EXTERNAL_SCANNER.lock() {
        Ok(mut scanner) => scanner.scan(&WindowsExternalClientOs, &tracked, &ids),
        Err(_) => return Vec::new(),
    };
    for (pid, user_id) in &outcome.adopt {
        let bt = saved_accounts.get(user_id).cloned().unwrap_or_default();
        tracker.track_adopted(*user_id, *pid, bt);
    }
    if let Ok(mut list) = UNIDENTIFIED_CLIENTS.lock() {
        *list = outcome.unidentified;
    }
    outcome.adopt
}

static TRACKED_LOG_LOCATOR: LazyLock<Mutex<LogLocator>> =
    LazyLock::new(|| Mutex::new(LogLocator::new()));

/// O log de cada cliente que o app acompanha (lançado por ele ou adotado),
/// para o monitor de quedas. Mesmo casamento da varredura de fora (thread do
/// cabeçalho → PID), com cache próprio. `taken`: logs que já têm dono.
pub fn locate_logs_for_pids(pids: &[u32], taken: &HashSet<PathBuf>) -> HashMap<u32, PathBuf> {
    let os = WindowsExternalClientOs;
    let procs: Vec<ProcessCandidate> = pids
        .iter()
        .map(|pid| ProcessCandidate {
            pid: *pid,
            created_ms: os.process_created_ms(*pid),
        })
        .collect();
    match TRACKED_LOG_LOCATOR.lock() {
        Ok(mut locator) => {
            if procs.is_empty() {
                locator.clear();
                return HashMap::new();
            }
            locator.locate(&os, &procs, taken, os.now_ms())
        }
        Err(_) => HashMap::new(),
    }
}

/// Lê `max` bytes do log a partir de `offset` (só leitura, com compartilhamento).
pub fn read_roblox_log(path: &std::path::Path, offset: u64, max: u64) -> Option<Vec<u8>> {
    WindowsExternalClientOs.read_log(path, offset, max)
}

/// A última lista de clientes não identificados.
pub fn unidentified_clients() -> Vec<UnidentifiedClient> {
    UNIDENTIFIED_CLIENTS
        .lock()
        .map(|list| list.clone())
        .unwrap_or_default()
}

/// Tira um PID da lista na hora (identificado à mão), sem esperar a varredura.
pub fn forget_unidentified_client(pid: u32) {
    if let Ok(mut list) = UNIDENTIFIED_CLIENTS.lock() {
        list.retain(|c| c.pid != pid);
    }
}
