// Saúde de cada cliente que o app acompanha: a queda lida do log do Roblox,
// com o motivo. Ver docs/features/watcher.md ("Quedas").
//
// Tudo aqui só **lê**: o log (aberto só para leitura) e a lista de processos.
// Quem fecha cliente continua sendo só o Watcher, e só os que o app abriu
// (`only_launched_by_app`). Cliente adotado do site só ganha o aviso na tela.
//
// O classificador e a máquina de estados são puros (testados abaixo); o laço
// do Windows só junta as peças: acha o log de cada PID pelo mesmo casamento da
// varredura de clientes de fora (`platform::windows::locate_logs_for_pids`) e
// lê só os bytes novos a cada passada.

/// Carência depois de um teleporte: a desconexão do servidor antigo não é
/// queda. Se a conta entrar no jogo novo dentro dela, nada é mostrado.
const TELEPORT_GRACE_MS: i64 = 8_000;
/// Teto de leitura do log por cliente por passada (o resto fica para a próxima).
const CLIENT_LOG_READ_MAX: u64 = 8 * 1024 * 1024;
/// Sem log achado, tenta de novo depois disto (listar a pasta tem custo).
const CLIENT_LOG_RETRY_MS: i64 = 10_000;
/// Mensagem de kick vem do jogo: corta para não virar parágrafo na tela.
const KICK_MESSAGE_MAX_CHARS: usize = 200;

/// Por que a conta caiu, quando o código diz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DropReason {
    /// 277, 279, 266, 260–262: a conexão com o servidor caiu.
    ConnectionLost,
    /// 264, 273 (e 276, só pelos concorrentes): a mesma conta entrou em outro lugar.
    JoinedElsewhere,
    /// 278: parada tempo demais (kick de inatividade do próprio Roblox).
    Idle,
    Other,
}

/// O que uma linha do log diz sobre a sessão.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientLogEvent {
    /// `! Joining game '<job>' place <place> at <ip>`.
    JoinedGame {
        place_id: Option<i64>,
        job_id: Option<String>,
    },
    /// `doTeleport:` / `finishTeleportWithJoinScriptPayload` / `Teleported.`.
    TeleportStarted,
    /// A pessoa saiu do jogo (fechou a janela, voltou para a home).
    LeftVoluntarily,
    Disconnected { code: u32, reason: DropReason },
    Kicked {
        code: Option<u32>,
        message: Option<String>,
    },
    ServerShutdown { code: u32 },
}

/// Texto depois de `marker`, se houver.
fn text_after<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    line.find(marker).map(|i| &line[i + marker.len()..])
}

/// Primeiro número logo depois de `marker` (pula espaços e um rótulo curto,
/// como em `for reason: Player: 285`).
fn number_after(line: &str, marker: &str) -> Option<u32> {
    let rest = text_after(line, marker)?;
    let start = rest.find(|c: char| c.is_ascii_digit())?;
    // O número tem que estar perto do marcador: `Reason: 285`, `Player: 285`.
    if start > 16 {
        return None;
    }
    let digits: String = rest[start..].chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn drop_reason_for_code(code: u32) -> DropReason {
    match code {
        264 | 273 | 276 => DropReason::JoinedElsewhere,
        260..=262 | 266 | 277 | 279 => DropReason::ConnectionLost,
        278 => DropReason::Idle,
        _ => DropReason::Other,
    }
}

/// O evento de um código de desconexão (`Enum.ConnectionError` do Roblox).
/// 285 é a saída pedida pelo próprio cliente: aparece em **toda** saída e em
/// **todo** teleporte (no 0.742, depois até do join do servidor novo), então
/// não diz nada sozinho — a saída é reconhecida pelas linhas de saída.
pub fn event_for_disconnect_code(code: u32) -> Option<ClientLogEvent> {
    match code {
        285 => None,
        267 => Some(ClientLogEvent::Kicked {
            code: Some(code),
            message: None,
        }),
        274 | 275 => Some(ClientLogEvent::ServerShutdown { code }),
        _ => Some(ClientLogEvent::Disconnected {
            code,
            reason: drop_reason_for_code(code),
        }),
    }
}

fn disconnect_code(line: &str) -> Option<u32> {
    // Cliente 0.740/0.741 (transporte antigo).
    for marker in [
        "Disconnection Notification. Reason: ",
        "Sending disconnect with reason: ",
    ] {
        if let Some(code) = number_after(line, marker) {
            return Some(code);
        }
    }
    // 0.742 (RbxTransport): `Disconnected from server for reason: Player: 285 (DisconnectClientInitiated)`.
    if let Some(code) = number_after(line, "Disconnected from server for reason: ") {
        return Some(code);
    }
    // Texto do aviso de erro (`Error Code: 277`), visto nos concorrentes. Só a
    // faixa das desconexões: "error code: 403" de um asset não é queda.
    let lower = line.to_ascii_lowercase();
    number_after(&lower, "error code: ").filter(|code| (256..=299).contains(code))
}

/// `You were kicked from this experience: <mensagem> (Error Code: 267)`.
fn kick_message(line: &str) -> Option<Option<String>> {
    let lower = line.to_ascii_lowercase();
    let marker = "kicked from this experience";
    let at = lower.find(marker)?;
    let rest = line[at + marker.len()..].trim_start_matches([':', ' ']);
    let rest = match rest.to_ascii_lowercase().find("(error code") {
        Some(end) => &rest[..end],
        None => rest,
    };
    let message: String = rest.trim().chars().take(KICK_MESSAGE_MAX_CHARS).collect();
    Some((!message.is_empty()).then_some(message))
}

/// Classifica uma linha do log do cliente. Puro: nada aqui guarda a linha
/// (ela pode trazer ticket, IP e Job ID).
pub fn classify_log_line(line: &str) -> Option<ClientLogEvent> {
    if let Some(rest) = text_after(line, "! Joining game '") {
        let end = rest.find('\'')?;
        let job = &rest[..end];
        let place_id = text_after(&rest[end..], "' place ").and_then(|p| {
            p.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .ok()
        });
        return Some(ClientLogEvent::JoinedGame {
            place_id,
            job_id: (!job.is_empty()).then(|| job.to_string()),
        });
    }
    if line.contains("UgcExperienceController: doTeleport:")
        || line.contains("finishTeleportWithJoinScriptPayload")
        || line.contains("[FLog::SessionTransitionFSM] Teleported.")
    {
        return Some(ClientLogEvent::TeleportStarted);
    }
    if line.contains("[FLog::SingleSurfaceApp] leaveUGCGameInternal")
        || line.contains("[FLog::SessionTransitionFSM] Tearing down.")
        || line.contains("[FLog::SingleSurfaceApp] returnToLuaApp")
    {
        return Some(ClientLogEvent::LeftVoluntarily);
    }
    if let Some(message) = kick_message(line) {
        return Some(ClientLogEvent::Kicked {
            code: Some(267),
            message,
        });
    }
    event_for_disconnect_code(disconnect_code(line)?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DropKind {
    Disconnected,
    Kicked,
    ServerShutdown,
    /// O processo terminou sem a conta sair do jogo (e sem o app fechá-lo).
    Crashed,
}

/// A conta caiu: o que a Sessão mostra.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientDrop {
    pub kind: DropKind,
    pub reason: Option<DropReason>,
    pub code: Option<u32>,
    pub message: Option<String>,
    pub since_ms: i64,
}

impl ClientDrop {
    fn same_drop(&self, other: &ClientDrop) -> bool {
        self.kind == other.kind && self.code == other.code
    }
}

/// O que o log disse até agora sobre a sessão de um cliente.
#[derive(Debug, Clone, Default)]
pub struct ClientLogSession {
    /// Queda vista, esperando a carência do teleporte.
    pending: Option<ClientDrop>,
    drop: Option<ClientDrop>,
    last_teleport_ms: Option<i64>,
    left: bool,
    /// Já entrou num jogo alguma vez (cliente parado no mutex nunca entra).
    joined: bool,
}

impl ClientLogSession {
    pub fn apply(&mut self, event: &ClientLogEvent, now_ms: i64) {
        match event {
            ClientLogEvent::JoinedGame { .. } => {
                // Entrou num jogo (de novo): o que caiu antes já não vale.
                self.pending = None;
                self.drop = None;
                self.left = false;
                self.joined = true;
                self.last_teleport_ms = None;
            }
            ClientLogEvent::TeleportStarted => self.last_teleport_ms = Some(now_ms),
            ClientLogEvent::LeftVoluntarily => self.left = true,
            ClientLogEvent::Disconnected { code, reason } => self.note_drop(ClientDrop {
                kind: DropKind::Disconnected,
                reason: Some(*reason),
                code: Some(*code),
                message: None,
                since_ms: now_ms,
            }),
            ClientLogEvent::Kicked { code, message } => self.note_drop(ClientDrop {
                kind: DropKind::Kicked,
                reason: None,
                code: *code,
                message: message.clone(),
                since_ms: now_ms,
            }),
            ClientLogEvent::ServerShutdown { code } => self.note_drop(ClientDrop {
                kind: DropKind::ServerShutdown,
                reason: None,
                code: Some(*code),
                message: None,
                since_ms: now_ms,
            }),
        }
    }

    fn note_drop(&mut self, drop: ClientDrop) {
        // Depois que a pessoa saiu, o que vier é a desmontagem da sessão.
        if self.left {
            return;
        }
        let existing = match (&mut self.pending, &mut self.drop) {
            (Some(pending), _) => pending,
            (None, Some(current)) => current,
            (None, None) => {
                self.pending = Some(drop);
                return;
            }
        };
        // A mensagem do kick pode vir numa linha separada do código.
        if drop.kind == DropKind::Kicked && existing.message.is_none() && drop.message.is_some() {
            existing.kind = DropKind::Kicked;
            existing.reason = None;
            existing.message = drop.message;
        }
    }

    /// Confirma a queda pendente: na hora, ou depois da carência se houve
    /// teleporte perto dela.
    pub fn settle(&mut self, now_ms: i64) {
        let Some(pending) = &self.pending else {
            return;
        };
        let teleport = self
            .last_teleport_ms
            .filter(|t| (pending.since_ms - t).abs() <= TELEPORT_GRACE_MS);
        let ready = match teleport {
            Some(t) => now_ms - t.max(pending.since_ms) >= TELEPORT_GRACE_MS,
            None => true,
        };
        if ready {
            self.drop = self.pending.take();
        }
    }

    pub fn current_drop(&self) -> Option<&ClientDrop> {
        self.drop.as_ref()
    }

    /// O processo terminou dentro de um jogo, sem sair e sem queda: fechou
    /// sozinho. Cliente que nunca entrou num jogo (preso no "só um Roblox
    /// aberto", por exemplo) não conta.
    pub fn ended_without_leaving(&self) -> bool {
        self.joined && !self.left && self.drop.is_none() && self.pending.is_none()
    }

    pub fn absorb(&mut self, text: &str, now_ms: i64) {
        for line in text.lines() {
            if let Some(event) = classify_log_line(line) {
                self.apply(&event, now_ms);
            }
        }
    }
}

/// Quantos bytes de `bytes` formam linhas completas (até o último `\n`). Um
/// pedaço cheio sem quebra de linha segue assim mesmo, para não travar.
fn complete_line_bytes(bytes: &[u8], max: u64) -> usize {
    match bytes.iter().rposition(|b| *b == b'\n') {
        Some(last_newline) => last_newline + 1,
        None if bytes.len() as u64 >= max => bytes.len(),
        None => 0,
    }
}

/// Um cliente que o app acompanha, como o tracker diz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackedClient {
    pub user_id: i64,
    pub pid: u32,
    /// Aberto pelo site e reconhecido: só aviso na tela, nunca ação.
    pub adopted: bool,
}

/// O que o monitor precisa do sistema (dublê nos testes).
pub trait ClientHealthOs {
    fn alive_pids(&self) -> HashSet<u32>;
    /// PID → log, para os PIDs sem log. `taken`: logs que já têm dono.
    fn locate_logs(&self, pids: &[u32], taken: &HashSet<std::path::PathBuf>) -> HashMap<u32, std::path::PathBuf>;
    fn read_log(&self, path: &std::path::Path, offset: u64, max: u64) -> Option<Vec<u8>>;
    /// O próprio app fechou este PID (Fechar, Auto Rejoin, Watcher…).
    fn terminated_by_app(&self, pid: u32) -> bool;
}

#[derive(Debug, Clone)]
struct ClientHealthEntry {
    pid: u32,
    adopted: bool,
    log: Option<std::path::PathBuf>,
    offset: u64,
    next_locate_ms: i64,
    session: ClientLogSession,
    crashed: Option<ClientDrop>,
    exit_seen: bool,
    reported: Option<ClientDrop>,
}

impl ClientHealthEntry {
    fn new(client: &TrackedClient) -> Self {
        Self {
            pid: client.pid,
            adopted: client.adopted,
            log: None,
            offset: 0,
            next_locate_ms: 0,
            session: ClientLogSession::default(),
            crashed: None,
            exit_seen: false,
            reported: None,
        }
    }

    fn current_drop(&self) -> Option<&ClientDrop> {
        self.crashed.as_ref().or(self.session.current_drop())
    }
}

/// O que a Sessão e o Watcher leem de um cliente.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientHealthView {
    pub pid: u32,
    /// O log do cliente foi achado: a queda vem dele, não do título da janela.
    pub log_found: bool,
    pub drop: Option<ClientDrop>,
}

/// Mudança que vira evento e linha no Console.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthNotice {
    Dropped {
        user_id: i64,
        drop: ClientDrop,
        adopted: bool,
    },
    /// A conta voltou a um jogo depois de uma queda.
    Recovered { user_id: i64 },
}

#[derive(Debug, Default)]
pub struct ClientHealthMonitor {
    entries: HashMap<i64, ClientHealthEntry>,
}

impl ClientHealthMonitor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tick(
        &mut self,
        os: &dyn ClientHealthOs,
        tracked: &[TrackedClient],
        now_ms: i64,
    ) -> Vec<HealthNotice> {
        let mut notices = Vec::new();
        // Conta que saiu do tracker, ou que trocou de PID, começa do zero.
        self.entries.retain(|uid, entry| {
            tracked
                .iter()
                .any(|t| t.user_id == *uid && t.pid == entry.pid)
        });
        if tracked.is_empty() {
            return notices;
        }
        for client in tracked {
            let entry = self
                .entries
                .entry(client.user_id)
                .or_insert_with(|| ClientHealthEntry::new(client));
            entry.adopted = client.adopted;
        }

        let alive = os.alive_pids();

        let taken: HashSet<std::path::PathBuf> =
            self.entries.values().filter_map(|e| e.log.clone()).collect();
        let need: Vec<u32> = self
            .entries
            .values()
            .filter(|e| e.log.is_none() && alive.contains(&e.pid) && now_ms >= e.next_locate_ms)
            .map(|e| e.pid)
            .collect();
        if !need.is_empty() {
            let found = os.locate_logs(&need, &taken);
            for entry in self.entries.values_mut() {
                if !need.contains(&entry.pid) {
                    continue;
                }
                match found.get(&entry.pid) {
                    Some(path) => entry.log = Some(path.clone()),
                    None => entry.next_locate_ms = now_ms + CLIENT_LOG_RETRY_MS,
                }
            }
        }

        for client in tracked {
            let Some(entry) = self.entries.get_mut(&client.user_id) else {
                continue;
            };
            if alive.contains(&entry.pid) {
                if let Some(path) = entry.log.clone() {
                    if let Some(bytes) = os.read_log(&path, entry.offset, CLIENT_LOG_READ_MAX) {
                        let consumed = complete_line_bytes(&bytes, CLIENT_LOG_READ_MAX);
                        if consumed > 0 {
                            entry
                                .session
                                .absorb(&String::from_utf8_lossy(&bytes[..consumed]), now_ms);
                            entry.offset += consumed as u64;
                        }
                    }
                }
            } else if !entry.exit_seen {
                entry.exit_seen = true;
                // Sem log não dá para saber se a pessoa fechou a janela: não chuta.
                if entry.log.is_some()
                    && entry.session.ended_without_leaving()
                    && !os.terminated_by_app(entry.pid)
                {
                    entry.crashed = Some(ClientDrop {
                        kind: DropKind::Crashed,
                        reason: None,
                        code: None,
                        message: None,
                        since_ms: now_ms,
                    });
                }
            }
            entry.session.settle(now_ms);

            let current = entry.current_drop().cloned();
            let changed = match (&current, &entry.reported) {
                (Some(now), Some(before)) => !now.same_drop(before),
                (None, None) => false,
                _ => true,
            };
            if changed {
                match &current {
                    Some(drop) => notices.push(HealthNotice::Dropped {
                        user_id: client.user_id,
                        drop: drop.clone(),
                        adopted: entry.adopted,
                    }),
                    None => notices.push(HealthNotice::Recovered {
                        user_id: client.user_id,
                    }),
                }
            }
            entry.reported = current;
        }
        notices
    }

    pub fn views(&self) -> HashMap<i64, ClientHealthView> {
        self.entries
            .iter()
            .map(|(uid, entry)| {
                (
                    *uid,
                    ClientHealthView {
                        pid: entry.pid,
                        log_found: entry.log.is_some(),
                        drop: entry.current_drop().cloned(),
                    },
                )
            })
            .collect()
    }
}

/// Último retrato do monitor, para o `get_running_instances` e o Watcher.
static CLIENT_HEALTH_VIEWS: LazyLock<Mutex<HashMap<i64, ClientHealthView>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// A saúde do cliente `pid` da conta (`None` se o monitor ainda não o viu).
pub(crate) fn client_health_of(user_id: i64, pid: u32) -> Option<ClientHealthView> {
    CLIENT_HEALTH_VIEWS
        .lock()
        .ok()?
        .get(&user_id)
        .filter(|view| view.pid == pid)
        .cloned()
}

/// Linha do Console (português, como as do Watcher) para uma queda.
fn drop_console_line(drop: &ClientDrop) -> String {
    let code = drop.code.map(|c| format!(" (código {c})")).unwrap_or_default();
    match drop.kind {
        DropKind::Disconnected => match drop.reason {
            Some(DropReason::ConnectionLost) => format!("Caiu: perdeu a conexão{code}"),
            Some(DropReason::JoinedElsewhere) => {
                format!("Caiu: a conta entrou em outro lugar{code}")
            }
            Some(DropReason::Idle) => format!("Caiu: ficou parada tempo demais{code}"),
            _ => format!("Caiu{code}"),
        },
        DropKind::Kicked => match &drop.message {
            Some(message) => format!("Foi expulso: {message}"),
            None => format!("Foi expulso do jogo{code}"),
        },
        DropKind::ServerShutdown => format!("O servidor fechou{code}"),
        DropKind::Crashed => String::from("O cliente fechou sem sair do jogo"),
    }
}

#[cfg(target_os = "windows")]
const CLIENT_HEALTH_TICK: std::time::Duration = std::time::Duration::from_secs(2);

#[cfg(target_os = "windows")]
struct WindowsClientHealthOs;

#[cfg(target_os = "windows")]
impl ClientHealthOs for WindowsClientHealthOs {
    fn alive_pids(&self) -> HashSet<u32> {
        platform::windows::get_roblox_pids().into_iter().collect()
    }
    fn locate_logs(&self, pids: &[u32], taken: &HashSet<std::path::PathBuf>) -> HashMap<u32, std::path::PathBuf> {
        platform::windows::locate_logs_for_pids(pids, taken)
    }
    fn read_log(&self, path: &std::path::Path, offset: u64, max: u64) -> Option<Vec<u8>> {
        platform::windows::read_roblox_log(path, offset, max)
    }
    fn terminated_by_app(&self, pid: u32) -> bool {
        platform::windows::was_terminated_by_app(pid)
    }
}

#[cfg(target_os = "windows")]
static CLIENT_HEALTH_MONITOR: LazyLock<Mutex<ClientHealthMonitor>> =
    LazyLock::new(|| Mutex::new(ClientHealthMonitor::new()));

/// Uma passada de verdade: lê o tracker, roda o monitor e publica o retrato.
#[cfg(target_os = "windows")]
fn run_client_health_tick() -> Vec<HealthNotice> {
    let tracked: Vec<TrackedClient> = platform::windows::tracker()
        .get_all()
        .into_iter()
        .map(|p| TrackedClient {
            user_id: p.user_id,
            pid: p.pid,
            adopted: p.adopted,
        })
        .collect();
    let now_ms = chrono::Utc::now().timestamp_millis();
    let Ok(mut monitor) = CLIENT_HEALTH_MONITOR.lock() else {
        return Vec::new();
    };
    let notices = monitor.tick(&WindowsClientHealthOs, &tracked, now_ms);
    if let Ok(mut views) = CLIENT_HEALTH_VIEWS.lock() {
        *views = monitor.views();
    }
    notices
}

/// Liga o monitor em segundo plano (sempre, como a varredura de clientes de
/// fora): sem cliente rastreado, cada passada custa só ler o tracker.
#[cfg(target_os = "windows")]
pub(crate) fn start_client_health_monitor(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let notices = tokio::task::spawn_blocking(run_client_health_tick)
                .await
                .unwrap_or_default();
            for notice in notices {
                emit_health_notice(&app, &notice);
            }
            tokio::time::sleep(CLIENT_HEALTH_TICK).await;
        }
    });
}

#[cfg(target_os = "windows")]
fn emit_health_notice(app: &tauri::AppHandle, notice: &HealthNotice) {
    match notice {
        HealthNotice::Dropped {
            user_id,
            drop,
            adopted,
        } => {
            let _ = app.emit(
                "roblox-client-health",
                serde_json::json!({
                    "userId": user_id,
                    "drop": drop,
                    "adopted": adopted,
                }),
            );
            emit_launch_log(app, *user_id, "warn", "client", drop_console_line(drop));
        }
        HealthNotice::Recovered { user_id } => {
            let _ = app.emit(
                "roblox-client-health",
                serde_json::json!({
                    "userId": user_id,
                    "drop": serde_json::Value::Null,
                }),
            );
            emit_launch_log(app, *user_id, "info", "client", String::from("Entrou num jogo de novo"));
        }
    }
}
