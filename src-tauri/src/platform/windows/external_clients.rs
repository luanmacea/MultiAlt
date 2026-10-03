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

/// Dígitos logo depois de `marker` (o primeiro, se o marcador repetir).
fn digits_after(line: &str, marker: &str) -> Option<i64> {
    let start = line.find(marker)? + marker.len();
    let digits: String = line[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let value: i64 = digits.parse().ok()?;
    (value > 0).then_some(value)
}

impl RobloxLogIdentity {
    /// Aplica uma linha: o que ela disser substitui o que se sabia.
    pub fn absorb_line(&mut self, line: &str) {
        if line.contains("[FLog::GameJoinLoadTime]") {
            if let Some(user_id) = digits_after(line, "userid:") {
                self.user_id = Some(user_id);
            }
            if let Some(place_id) = digits_after(line, "placeid:") {
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
    header: Option<RobloxLogHeader>,
    /// Tamanho do arquivo quando o cabeçalho foi tentado (só tenta de novo se crescer).
    header_tried_len: u64,
    /// Até onde a identidade já foi lida (sempre num fim de linha).
    offset: u64,
    identity: RobloxLogIdentity,
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
    /// PIDs sem log e logs vistos na última tentativa de casar: se nada mudou,
    /// não adianta tirar outro retrato das threads.
    last_match_attempt: Option<(Vec<u32>, Vec<(PathBuf, u64)>)>,
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
            self.last_match_attempt = None;
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
            }
        }
        outcome
    }

    fn try_match_logs(&mut self, os: &dyn ExternalClientOs, unmatched: &[u32], now: i64) {
        let assigned: HashSet<PathBuf> = self.pids.values().filter_map(|s| s.log.clone()).collect();
        // Um log não pode ser de processo criado depois dele: corta os velhos
        // antes de abrir qualquer arquivo.
        let oldest_created = unmatched
            .iter()
            .filter_map(|pid| self.pids.get(pid).and_then(|s| s.created_ms))
            .min();
        let mut files: Vec<PlayerLogFile> = os
            .list_player_logs()
            .into_iter()
            .filter(|f| is_player_log_name(&f.name))
            .filter(|f| now - f.modified_ms <= EXTERNAL_LOG_MAX_AGE_MS)
            .filter(|f| !assigned.contains(&f.path))
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

        let signature = (
            unmatched.to_vec(),
            files.iter().map(|f| (f.path.clone(), f.len)).collect::<Vec<_>>(),
        );
        if self.last_match_attempt.as_ref() == Some(&signature) {
            return;
        }
        self.last_match_attempt = Some(signature);

        let mut headers: Vec<RobloxLogHeader> = Vec::new();
        let mut header_paths: Vec<PathBuf> = Vec::new();
        for file in &files {
            let entry = self.logs.entry(file.path.clone()).or_default();
            if entry.header.is_none() && (entry.header_tried_len == 0 || file.len > entry.header_tried_len) {
                entry.header_tried_len = file.len.max(1);
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
            return;
        }

        let pid_set: HashSet<u32> = unmatched.iter().copied().collect();
        let owners = os.thread_owners(&pid_set);
        let procs: Vec<ProcessCandidate> = unmatched
            .iter()
            .map(|pid| ProcessCandidate {
                pid: *pid,
                created_ms: self.pids.get(pid).and_then(|s| s.created_ms),
            })
            .collect();
        for (pid, index) in match_logs_to_processes(&procs, &headers, &owners) {
            if let Some(state) = self.pids.get_mut(&pid) {
                state.log = Some(header_paths[index].clone());
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

#[cfg(test)]
mod external_client_log_tests {
    use super::*;

    const HEADER: &str = "[FLog::Output] All use of Roblox services must comply with Roblox's Terms of Use.\n\
2026-10-03T21:11:19.354Z,0.354978,3ed8,6,Warning [FLog::RobloxStarter] Starting module: Logging\n\
2026-10-03T21:11:19.357Z,0.357979,3e0c,6,Info [FLog::UpdateController] Update check thread started\n";

    fn ms(rfc3339: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(rfc3339).unwrap().timestamp_millis()
    }

    #[test]
    fn the_header_is_the_thread_of_the_first_data_line() {
        let header = parse_roblox_log_header(HEADER).expect("header");
        assert_eq!(header.thread_id, 0x3ed8);
        // 21:11:19.354 - 0.355 s
        assert_eq!(header.origin_ms, ms("2026-10-03T21:11:18.999Z"));
    }

    #[test]
    fn a_log_without_data_lines_has_no_header() {
        assert_eq!(parse_roblox_log_header(""), None);
        assert_eq!(parse_roblox_log_header("[FLog::Output] only the banner\n"), None);
        assert_eq!(parse_roblox_log_header("not,a,log,line\n"), None);
        // Uma linha cortada no meio (o arquivo acabou de nascer).
        assert_eq!(parse_roblox_log_header("2026-10-03T21:11:19.354Z,0.35"), None);
    }

    #[test]
    fn the_header_skips_lines_that_are_not_data() {
        let text = "banner\ngarbage line\n2026-10-03T10:00:00.500Z,0.500000,1a2b,6,Info x\n";
        let header = parse_roblox_log_header(text).unwrap();
        assert_eq!(header.thread_id, 0x1a2b);
        assert_eq!(header.origin_ms, ms("2026-10-03T10:00:00.000Z"));
    }

    #[test]
    fn identity_takes_the_last_userid_place_and_job() {
        let text = "\
2026-10-03T21:11:20.521Z,1.521379,3dcc,6 [FLog::Output] ! Joining game '6ace6a0d-236b-41b4-b906-fca178b6b2f1' place 139020444733179 at 10.9.3.162\n\
2026-10-03T21:11:20.521Z,1.521379,3dcc,6 [FLog::GameJoinLoadTime] Report game_join_loadtime: placeid:139020444733179, join_time:0.64, universeid:10696894951, referral_page:RequestGame, sid:e9268116, clienttime:1, userid:75444209, \n\
2026-10-03T21:11:50.926Z,31.926266,3dcc,6 [FLog::Output] ! Joining game '26effa71-d57d-44e0-906e-71f8d31cf4e9' place 115948685086136 at 10.206.18.10\n\
2026-10-03T21:11:50.926Z,31.926266,3dcc,6 [FLog::GameJoinLoadTime] Report game_join_loadtime: placeid:115948685086136, join_time:0, universeid:10696894951, referral_page:, sid:b7b25dc9, clienttime:1791061910.62, userid:75444210, \n";
        let identity = parse_roblox_log_identity(text);
        assert_eq!(identity.user_id, Some(75444210));
        assert_eq!(identity.place_id, Some(115948685086136));
        assert_eq!(
            identity.job_id.as_deref(),
            Some("26effa71-d57d-44e0-906e-71f8d31cf4e9")
        );
    }

    #[test]
    fn a_client_that_has_not_joined_yet_has_no_account() {
        let identity = parse_roblox_log_identity(HEADER);
        assert_eq!(identity, RobloxLogIdentity::default());
    }

    #[test]
    fn the_join_ticket_also_identifies_the_account() {
        let line = "2026-10-03T21:11:50.826Z,31.826572,3ed8,6,Info [FLog::UgcExperienceController] UgcExperienceController: doTeleport: joinScriptUrl https://assetgame.roblox.com/Game/Join.ashx?ticketVersion=2&ticket={\"UserId\"%3a75444209%2c\"UserName\"%3a\"x\"";
        assert_eq!(parse_roblox_log_identity(line).user_id, Some(75444209));
    }

    #[test]
    fn the_tracker_cookie_does_not_identify_the_account() {
        // `rbxuid=` vem do cookie RBXEventTrackerV2, compartilhado entre contas:
        // não é prova de quem está no cliente.
        let line = "2026-10-03T21:11:19.478Z,0.478003,3ed8,6 [FLog::LogWin32BTId] LogWin32BTId, App, cookie Native => Engine set RBXEventTrackerV2 as CreateDate=09/28/2026&rbxid=74279897&rbxuid=75444209&browserid=1";
        assert_eq!(parse_roblox_log_identity(line).user_id, None);
    }

    #[test]
    fn identity_absorbs_chunks_incrementally() {
        let mut identity = RobloxLogIdentity::default();
        identity.absorb("x [FLog::GameJoinLoadTime] Report game_join_loadtime: placeid:1, userid:10, \n");
        assert_eq!(identity.user_id, Some(10));
        identity.absorb("nothing relevant here\n");
        assert_eq!(identity.user_id, Some(10), "a chunk without a join keeps the account");
        identity.absorb("x [FLog::GameJoinLoadTime] Report game_join_loadtime: placeid:2, userid:20, \n");
        assert_eq!(identity.user_id, Some(20));
        assert_eq!(identity.place_id, Some(2));
    }

    #[test]
    fn player_log_names_exclude_the_crash_handler_and_other_files() {
        assert!(is_player_log_name(
            "0.741.0.7411058_20261003T211119Z_Player_F380C_last.log"
        ));
        assert!(!is_player_log_name(
            "0.741.0.7411058_20261003T211119Z_Player_8CA1F_CrashHandler_last.log"
        ));
        assert!(!is_player_log_name("RobloxPlayerInstaller_2C17E.log"));
        assert!(!is_player_log_name("cacert.pem"));
    }

    #[test]
    fn the_log_name_carries_its_start_time() {
        assert_eq!(
            player_log_name_started_ms("0.741.0.7411058_20261003T211119Z_Player_F380C_last.log"),
            Some(ms("2026-10-03T21:11:19Z"))
        );
        assert_eq!(player_log_name_started_ms("weird_Player_X_last.log"), None);
    }

    #[test]
    fn filetime_converts_to_unix_milliseconds() {
        // 1970-01-01 = 116444736000000000 ticks de 100 ns.
        let epoch: u64 = 116_444_736_000_000_000;
        assert_eq!(filetime_to_unix_ms(epoch as u32, (epoch >> 32) as u32), 0);
        let later = epoch + 1_500 * 10_000;
        assert_eq!(filetime_to_unix_ms(later as u32, (later >> 32) as u32), 1_500);
    }
}

#[cfg(test)]
mod external_client_match_tests {
    use super::*;

    const T0: i64 = 1_791_061_876_709; // criação do processo

    fn header(thread_id: u32, origin_ms: i64) -> RobloxLogHeader {
        RobloxLogHeader { thread_id, origin_ms }
    }

    fn proc_(pid: u32, created_ms: Option<i64>) -> ProcessCandidate {
        ProcessCandidate { pid, created_ms }
    }

    #[test]
    fn the_thread_owner_links_a_log_to_its_process() {
        let owners = HashMap::from([(0x3ed8, 11456)]);
        let matched = match_logs_to_processes(
            &[proc_(11456, Some(T0))],
            &[header(0x3ed8, T0 + 2_300)],
            &owners,
        );
        assert_eq!(matched.get(&11456), Some(&0));
    }

    #[test]
    fn a_reused_thread_id_from_an_older_log_does_not_match() {
        // O log é de um cliente que morreu uma hora antes; o id da thread foi
        // reaproveitado no cliente novo.
        let owners = HashMap::from([(0x3ed8, 11456)]);
        let matched = match_logs_to_processes(
            &[proc_(11456, Some(T0))],
            &[header(0x3ed8, T0 - 3_600_000)],
            &owners,
        );
        assert!(matched.is_empty());
    }

    #[test]
    fn the_newest_log_wins_when_two_point_at_the_same_process() {
        let owners = HashMap::from([(0x10, 7), (0x20, 7)]);
        let matched = match_logs_to_processes(
            &[proc_(7, Some(T0))],
            &[header(0x10, T0 + 2_000), header(0x20, T0 + 2_500)],
            &owners,
        );
        assert_eq!(matched.get(&7), Some(&0));
    }

    #[test]
    fn a_thread_of_a_process_that_is_not_a_candidate_is_ignored() {
        // Thread de um cliente que o app já acompanha (não é candidato).
        let owners = HashMap::from([(0x10, 99)]);
        let matched = match_logs_to_processes(&[proc_(7, None)], &[header(0x10, T0)], &owners);
        assert!(matched.is_empty());
    }

    #[test]
    fn without_the_thread_an_unambiguous_start_time_matches() {
        let matched = match_logs_to_processes(
            &[proc_(7, Some(T0))],
            &[header(0x10, T0 + 2_300), header(0x20, T0 - 600_000)],
            &HashMap::new(),
        );
        assert_eq!(matched.get(&7), Some(&0));
    }

    #[test]
    fn without_the_thread_an_ambiguous_start_time_does_not_match() {
        // Dois clientes abertos juntos: o horário sozinho não decide.
        let matched = match_logs_to_processes(
            &[proc_(7, Some(T0)), proc_(8, Some(T0 + 500))],
            &[header(0x10, T0 + 2_300), header(0x20, T0 + 2_600)],
            &HashMap::new(),
        );
        assert!(matched.is_empty(), "{:?}", matched);
    }

    #[test]
    fn the_thread_rule_resolves_what_the_clock_cannot() {
        let owners = HashMap::from([(0x10, 8), (0x20, 7)]);
        let matched = match_logs_to_processes(
            &[proc_(7, Some(T0)), proc_(8, Some(T0 + 500))],
            &[header(0x10, T0 + 2_300), header(0x20, T0 + 2_600)],
            &owners,
        );
        assert_eq!(matched.get(&8), Some(&0));
        assert_eq!(matched.get(&7), Some(&1));
    }

    #[test]
    fn without_a_creation_time_only_the_thread_can_match() {
        let matched =
            match_logs_to_processes(&[proc_(7, None)], &[header(0x10, T0)], &HashMap::new());
        assert!(matched.is_empty());
        let owners = HashMap::from([(0x10, 7)]);
        let matched = match_logs_to_processes(&[proc_(7, None)], &[header(0x10, T0)], &owners);
        assert_eq!(matched.get(&7), Some(&0));
    }

    #[test]
    fn decide_adopts_only_a_saved_account_with_no_live_client() {
        let saved = HashSet::from([10, 20]);
        let busy = HashMap::from([(20, 555)]);
        assert_eq!(
            decide_external_client(true, Some(10), &saved, &busy),
            ExternalDecision::Adopt(10)
        );
        assert_eq!(
            decide_external_client(true, Some(20), &saved, &busy),
            ExternalDecision::Unidentified(UnidentifiedReason::AccountBusy)
        );
        assert_eq!(
            decide_external_client(true, Some(30), &saved, &busy),
            ExternalDecision::Unidentified(UnidentifiedReason::UnknownAccount)
        );
        assert_eq!(
            decide_external_client(true, None, &saved, &busy),
            ExternalDecision::Unidentified(UnidentifiedReason::WaitingForGame)
        );
        assert_eq!(
            decide_external_client(false, Some(10), &saved, &busy),
            ExternalDecision::Unidentified(UnidentifiedReason::NoLog)
        );
    }

    #[test]
    fn unidentified_client_serializes_in_camel_case() {
        let json = serde_json::to_value(UnidentifiedClient {
            pid: 42,
            reason: UnidentifiedReason::WaitingForGame,
            user_id: None,
            place_id: Some(1),
            job_id: None,
            started_at_ms: Some(5),
        })
        .unwrap();
        assert_eq!(json["pid"], 42);
        assert_eq!(json["reason"], "waitingForGame");
        assert_eq!(json["placeId"], 1);
        assert_eq!(json["startedAtMs"], 5);
        assert!(json["userId"].is_null());
    }
}

#[cfg(test)]
mod external_client_scan_tests {
    use super::*;
    use std::cell::RefCell;

    const NOW: i64 = 1_791_061_900_000;

    /// Dublê do sistema: processos, threads e arquivos de log em memória.
    #[derive(Default)]
    struct FakeOs {
        pids: RefCell<Vec<u32>>,
        created: HashMap<u32, i64>,
        threads: RefCell<HashMap<u32, u32>>,
        logs: RefCell<Vec<(PlayerLogFile, String)>>,
        now: RefCell<i64>,
        thread_snapshots: RefCell<u32>,
        bytes_read: RefCell<u64>,
    }

    impl FakeOs {
        fn add_log(&self, name: &str, content: &str) {
            let file = PlayerLogFile {
                path: PathBuf::from(format!("C:/logs/{name}")),
                name: name.to_string(),
                modified_ms: *self.now.borrow(),
                len: content.len() as u64,
            };
            self.logs.borrow_mut().push((file, content.to_string()));
        }

        fn append(&self, name: &str, more: &str) {
            let mut logs = self.logs.borrow_mut();
            let entry = logs.iter_mut().find(|(f, _)| f.name == name).unwrap();
            entry.1.push_str(more);
            entry.0.len = entry.1.len() as u64;
        }
    }

    impl ExternalClientOs for FakeOs {
        fn roblox_pids(&self) -> Vec<u32> {
            self.pids.borrow().clone()
        }
        fn process_created_ms(&self, pid: u32) -> Option<i64> {
            self.created.get(&pid).copied()
        }
        fn thread_owners(&self, pids: &HashSet<u32>) -> HashMap<u32, u32> {
            *self.thread_snapshots.borrow_mut() += 1;
            self.threads
                .borrow()
                .iter()
                .filter(|(_, pid)| pids.contains(pid))
                .map(|(t, p)| (*t, *p))
                .collect()
        }
        fn list_player_logs(&self) -> Vec<PlayerLogFile> {
            self.logs.borrow().iter().map(|(f, _)| f.clone()).collect()
        }
        fn read_log(&self, path: &std::path::Path, offset: u64, max: u64) -> Option<Vec<u8>> {
            let logs = self.logs.borrow();
            let (_, content) = logs.iter().find(|(f, _)| f.path == path)?;
            let bytes = content.as_bytes();
            let start = (offset as usize).min(bytes.len());
            let end = (start + max as usize).min(bytes.len());
            *self.bytes_read.borrow_mut() += (end - start) as u64;
            Some(bytes[start..end].to_vec())
        }
        fn now_ms(&self) -> i64 {
            *self.now.borrow()
        }
    }

    // Processo criado em CREATED; a origem do log 2,3 s depois.
    const CREATED: i64 = NOW - 60_000;
    const LOG_NAME: &str = "0.741.0.7411058_20261003T211119Z_Player_F380C_last.log";

    fn header_line(thread: &str) -> String {
        header_line_at(thread, CREATED + 2_300)
    }

    fn header_line_at(thread: &str, origin_ms: i64) -> String {
        let origin = chrono::DateTime::from_timestamp_millis(origin_ms)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        format!("[FLog::Output] banner\n{origin},0.000000,{thread},6,Warning [FLog::RobloxStarter] Starting\n")
    }

    fn join_line(user_id: i64) -> String {
        format!("x,1.0,3dcc,6 [FLog::GameJoinLoadTime] Report game_join_loadtime: placeid:606849621, userid:{user_id}, \n")
    }

    fn log_name_for(created: i64) -> String {
        let stamp = chrono::DateTime::from_timestamp_millis(created + 2_300)
            .unwrap()
            .format("%Y%m%dT%H%M%SZ")
            .to_string();
        format!("0.741.0.7411058_{stamp}_Player_F380C_last.log")
    }

    fn os_with_one_client() -> FakeOs {
        let os = FakeOs {
            created: HashMap::from([(11456, CREATED)]),
            ..Default::default()
        };
        *os.now.borrow_mut() = NOW;
        *os.pids.borrow_mut() = vec![11456];
        os.threads.borrow_mut().insert(0x3ed8, 11456);
        os
    }

    #[test]
    fn a_client_opened_from_the_site_is_adopted_for_its_saved_account() {
        let os = os_with_one_client();
        let name = log_name_for(CREATED);
        os.add_log(&name, &format!("{}{}", header_line("3ed8"), join_line(75444209)));
        let mut scanner = ExternalClientScanner::new();

        let outcome = scanner.scan(&os, &HashMap::new(), &HashSet::from([75444209]));
        assert_eq!(outcome.adopt, vec![(11456, 75444209)]);
        assert!(outcome.unidentified.is_empty());
    }

    #[test]
    fn nothing_is_read_when_every_client_is_already_tracked() {
        let os = os_with_one_client();
        os.add_log(&log_name_for(CREATED), &header_line("3ed8"));
        let mut scanner = ExternalClientScanner::new();

        let outcome = scanner.scan(&os, &HashMap::from([(1, 11456)]), &HashSet::from([1]));
        assert_eq!(outcome, ExternalScanOutcome::default());
        assert_eq!(*os.bytes_read.borrow(), 0);
        assert_eq!(*os.thread_snapshots.borrow(), 0);
    }

    #[test]
    fn a_client_waits_until_it_joins_a_game_then_is_adopted() {
        let os = os_with_one_client();
        let name = log_name_for(CREATED);
        os.add_log(&name, &header_line("3ed8"));
        let mut scanner = ExternalClientScanner::new();
        let saved = HashSet::from([75444209]);

        let first = scanner.scan(&os, &HashMap::new(), &saved);
        assert!(first.adopt.is_empty());
        assert_eq!(first.unidentified.len(), 1);
        assert_eq!(first.unidentified[0].reason, UnidentifiedReason::WaitingForGame);

        os.append(&name, &join_line(75444209));
        let read_before = *os.bytes_read.borrow();
        let second = scanner.scan(&os, &HashMap::new(), &saved);
        assert_eq!(second.adopt, vec![(11456, 75444209)]);
        // Só o pedaço novo foi lido.
        assert_eq!(
            *os.bytes_read.borrow() - read_before,
            join_line(75444209).len() as u64
        );
    }

    #[test]
    fn a_client_younger_than_the_grace_is_not_listed_yet() {
        let os = FakeOs {
            created: HashMap::from([(11456, NOW - 3_000)]),
            ..Default::default()
        };
        *os.now.borrow_mut() = NOW;
        *os.pids.borrow_mut() = vec![11456];
        let mut scanner = ExternalClientScanner::new();

        let outcome = scanner.scan(&os, &HashMap::new(), &HashSet::new());
        assert!(outcome.unidentified.is_empty(), "a launch of the app may still claim it");
    }

    #[test]
    fn an_account_that_is_not_saved_stays_unidentified_with_its_user_id() {
        let os = os_with_one_client();
        os.add_log(&log_name_for(CREATED), &format!("{}{}", header_line("3ed8"), join_line(5)));
        let mut scanner = ExternalClientScanner::new();

        let outcome = scanner.scan(&os, &HashMap::new(), &HashSet::from([75444209]));
        assert!(outcome.adopt.is_empty());
        assert_eq!(outcome.unidentified[0].reason, UnidentifiedReason::UnknownAccount);
        assert_eq!(outcome.unidentified[0].user_id, Some(5));
        assert_eq!(outcome.unidentified[0].place_id, Some(606849621));
    }

    #[test]
    fn an_account_already_running_elsewhere_is_not_taken_over() {
        let os = os_with_one_client();
        os.pids.borrow_mut().push(777);
        os.add_log(&log_name_for(CREATED), &format!("{}{}", header_line("3ed8"), join_line(10)));
        let mut scanner = ExternalClientScanner::new();

        let outcome = scanner.scan(&os, &HashMap::from([(10, 777)]), &HashSet::from([10]));
        assert!(outcome.adopt.is_empty());
        assert_eq!(outcome.unidentified[0].reason, UnidentifiedReason::AccountBusy);
    }

    #[test]
    fn an_account_whose_tracked_client_died_is_adopted_again() {
        // Depois de reiniciar o app (ou de o PID antigo morrer sem o Watcher
        // ver), o PID registrado não está mais vivo: vale o novo.
        let os = os_with_one_client();
        os.add_log(&log_name_for(CREATED), &format!("{}{}", header_line("3ed8"), join_line(10)));
        let mut scanner = ExternalClientScanner::new();

        let outcome = scanner.scan(&os, &HashMap::from([(10, 999_999)]), &HashSet::from([10]));
        assert_eq!(outcome.adopt, vec![(11456, 10)]);
    }

    #[test]
    fn a_client_with_no_matching_log_is_unidentified_and_the_threads_are_not_rescanned() {
        let os = os_with_one_client();
        os.threads.borrow_mut().clear();
        // Nome recente, mas o cabeçalho é de outro cliente (origem 5 min antes,
        // thread de ninguém): nem a thread nem o horário casam.
        os.add_log(&log_name_for(CREATED), &header_line_at("1111", CREATED - 300_000));
        let mut scanner = ExternalClientScanner::new();

        let first = scanner.scan(&os, &HashMap::new(), &HashSet::new());
        assert_eq!(first.unidentified[0].reason, UnidentifiedReason::NoLog);
        let snapshots = *os.thread_snapshots.borrow();
        assert_eq!(snapshots, 1);

        let again = scanner.scan(&os, &HashMap::new(), &HashSet::new());
        assert_eq!(again.unidentified[0].reason, UnidentifiedReason::NoLog);
        assert_eq!(
            *os.thread_snapshots.borrow(),
            snapshots,
            "nothing changed, so the thread snapshot is not taken again"
        );
    }

    #[test]
    fn crash_handler_and_stale_logs_are_never_opened() {
        let os = os_with_one_client();
        os.add_log(
            "0.741.0.7411058_20261003T211119Z_Player_8CA1F_CrashHandler_last.log",
            &header_line("3ed8"),
        );
        let mut scanner = ExternalClientScanner::new();
        scanner.scan(&os, &HashMap::new(), &HashSet::new());
        assert_eq!(*os.bytes_read.borrow(), 0);

        // Um log modificado há mais de 24 h também fica de fora.
        let old = FakeOs::default();
        *old.now.borrow_mut() = NOW - EXTERNAL_LOG_MAX_AGE_MS - 1;
        old.add_log(LOG_NAME, &header_line("3ed8"));
        *old.now.borrow_mut() = NOW;
        *old.pids.borrow_mut() = vec![11456];
        let mut scanner = ExternalClientScanner::new();
        scanner.scan(&old, &HashMap::new(), &HashSet::new());
        assert_eq!(*old.bytes_read.borrow(), 0);
    }

    /// Sonda da máquina de verdade, só leitura: lista de processos, threads e
    /// logs. Não registra nada no tracker nem toca no cliente. Rode à mão com
    /// `cargo test --all-features external_client_real_probe -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn external_client_real_probe() {
        let os = WindowsExternalClientOs;
        let mut scanner = ExternalClientScanner::new();
        let started = std::time::Instant::now();
        let first = scanner.scan(&os, &HashMap::new(), &HashSet::new());
        let first_ms = started.elapsed().as_millis();
        let started = std::time::Instant::now();
        let second = scanner.scan(&os, &HashMap::new(), &HashSet::new());
        let second_ms = started.elapsed().as_millis();
        println!("pids: {:?}", os.roblox_pids());
        println!("first scan ({first_ms} ms): {:?}", first);
        println!("second scan ({second_ms} ms): {:?}", second);
    }

    #[test]
    fn two_site_clients_of_the_same_account_adopt_only_one() {
        let os = os_with_one_client();
        os.pids.borrow_mut().push(22222);
        os.threads.borrow_mut().insert(0x4444, 22222);
        os.add_log(&log_name_for(CREATED), &format!("{}{}", header_line("3ed8"), join_line(10)));
        let second = format!("{}", log_name_for(CREATED)).replace("F380C", "AAAAA");
        os.add_log(&second, &format!("{}{}", header_line("4444"), join_line(10)));
        let mut scanner = ExternalClientScanner::new();
        let mut created = os.created.clone();
        created.insert(22222, CREATED);
        let os = FakeOs { created, ..os };

        let outcome = scanner.scan(&os, &HashMap::new(), &HashSet::from([10]));
        assert_eq!(outcome.adopt.len(), 1);
        assert_eq!(outcome.unidentified.len(), 1);
        assert_eq!(outcome.unidentified[0].reason, UnidentifiedReason::AccountBusy);
    }
}
