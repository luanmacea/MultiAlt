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

#[cfg(test)]
mod client_log_classifier_tests {
    use super::*;

    // Linhas no formato do cliente de verdade (calibrado com os logs da
    // máquina do dono em 09/10/2026), com ids, IPs e Job IDs trocados por
    // falsos. Nunca colar linha real aqui: ela traz ticket, IP e Job ID.
    const PREFIX: &str = "2026-10-09T01:02:03.456Z,12.345678,1a2b,6";

    fn line(body: &str) -> String {
        format!("{PREFIX} {body}")
    }

    #[test]
    fn joining_a_game_carries_the_place_and_the_job() {
        let event = classify_log_line(&line(
            "[FLog::Output] ! Joining game '00000000-1111-2222-3333-444444444444' place 1234567 at 10.0.0.1",
        ));
        assert_eq!(
            event,
            Some(ClientLogEvent::JoinedGame {
                place_id: Some(1234567),
                job_id: Some("00000000-1111-2222-3333-444444444444".into()),
            })
        );
    }

    #[test]
    fn the_teleport_lines_are_teleports() {
        for body in [
            "[FLog::UgcExperienceController] UgcExperienceController: doTeleport: url ",
            "[FLog::UgcExperienceController] UgcExperienceController: finishTeleportWithJoinScriptPayload: begin",
            "[FLog::SessionTransitionFSM] Teleported.",
        ] {
            assert_eq!(classify_log_line(&line(body)), Some(ClientLogEvent::TeleportStarted), "{body}");
        }
    }

    #[test]
    fn leaving_the_game_is_voluntary() {
        for body in [
            "[FLog::SingleSurfaceApp] leaveUGCGameInternal",
            "[FLog::SessionTransitionFSM] Tearing down.",
            "[FLog::SingleSurfaceApp] returnToLuaApp: (stage:UGCGame).",
        ] {
            assert_eq!(classify_log_line(&line(body)), Some(ClientLogEvent::LeftVoluntarily), "{body}");
        }
    }

    #[test]
    fn reason_285_alone_says_nothing() {
        // Aparece em toda saída e em todo teleporte, nos três formatos.
        for body in [
            "[FLog::Network] Sending disconnect with reason: 285",
            "[FLog::Network] Disconnection Notification. Reason: 285",
            "[DFLog::RbxTransportDummyClient] Disconnected from server for reason: Player: 285 (DisconnectClientInitiated)",
        ] {
            assert_eq!(classify_log_line(&line(body)), None, "{body}");
        }
    }

    #[test]
    fn a_lost_connection_is_a_disconnect_with_its_reason() {
        assert_eq!(
            classify_log_line(&line("[FLog::Network] Disconnection Notification. Reason: 277")),
            Some(ClientLogEvent::Disconnected {
                code: 277,
                reason: DropReason::ConnectionLost
            })
        );
        assert_eq!(
            classify_log_line(&line(
                "[DFLog::RbxTransportDummyClient] Disconnected from server for reason: Server: 279 (DisconnectRaknetErrors)"
            )),
            Some(ClientLogEvent::Disconnected {
                code: 279,
                reason: DropReason::ConnectionLost
            })
        );
    }

    #[test]
    fn the_same_account_joining_elsewhere_has_its_own_reason() {
        for code in [264, 273] {
            assert_eq!(
                classify_log_line(&line(&format!("[FLog::Network] Disconnection Notification. Reason: {code}"))),
                Some(ClientLogEvent::Disconnected {
                    code,
                    reason: DropReason::JoinedElsewhere
                })
            );
        }
    }

    #[test]
    fn a_kick_is_a_kick_and_carries_the_message_when_there_is_one() {
        assert_eq!(
            classify_log_line(&line("[FLog::Network] Sending disconnect with reason: 267")),
            Some(ClientLogEvent::Kicked {
                code: Some(267),
                message: None
            })
        );
        assert_eq!(
            classify_log_line(&line(
                "[FLog::Output] You were kicked from this experience: Server is restarting (Error Code: 267)"
            )),
            Some(ClientLogEvent::Kicked {
                code: Some(267),
                message: Some("Server is restarting".into())
            })
        );
    }

    #[test]
    fn a_shutdown_for_maintenance_is_a_server_shutdown() {
        assert_eq!(
            classify_log_line(&line("[FLog::Network] Disconnection Notification. Reason: 274")),
            Some(ClientLogEvent::ServerShutdown { code: 274 })
        );
    }

    #[test]
    fn noise_that_looks_like_a_disconnect_is_ignored() {
        for body in [
            // Transporte secundário falhando depois de um teleporte: o jogo seguiu.
            "[DFLog::RbxTransportDummyClient] Failed to establish connection to server at 10.0.0.1:1234, reason IO: 12",
            "[FLog::Network] Connection lost: connectMode: Disconnect ASAP, timeMS:123456, connectionTime 1317",
            "[DFLog::SignalRCoreError] ID: 1 Disconnected - stop() called",
            "[FLog::WndProcessCheck] waitForNewPlayerProcess new waiting for mutex result is 0X102, ERROR Unknown error code 0x102.",
            "[FLog::Error] Asset load failed, error code: 403",
            "[DFLog::NetworkClient] Client:Disconnect",
        ] {
            assert_eq!(classify_log_line(&line(body)), None, "{body}");
        }
    }

    #[test]
    fn the_generic_error_code_text_counts_only_in_the_disconnect_range() {
        assert_eq!(
            classify_log_line("Disconnected (Error Code: 277) lost connection"),
            Some(ClientLogEvent::Disconnected {
                code: 277,
                reason: DropReason::ConnectionLost
            })
        );
        assert_eq!(classify_log_line("error code: 503 while fetching"), None);
    }

    /// Sonda dos logs de verdade da máquina, só leitura: conta os eventos e o
    /// estado final de cada log, sem imprimir linha nenhuma (elas têm ticket,
    /// IP e Job ID). Rode à mão com
    /// `cargo test --all-features client_log_real_probe -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn client_log_real_probe() {
        let Some(dir) = std::env::var_os("LOCALAPPDATA")
            .map(|base| std::path::PathBuf::from(base).join("Roblox").join("logs"))
        else {
            return;
        };
        let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
        let mut finals: std::collections::BTreeMap<String, usize> = Default::default();
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.contains("_Player_") || name.contains("CrashHandler") {
                continue;
            }
            let Ok(bytes) = std::fs::read(entry.path()) else { continue };
            let text = String::from_utf8_lossy(&bytes);
            let mut session = ClientLogSession::default();
            for line in text.lines() {
                if let Some(event) = classify_log_line(line) {
                    let key = match &event {
                        ClientLogEvent::JoinedGame { .. } => "joined".to_string(),
                        ClientLogEvent::TeleportStarted => "teleport".to_string(),
                        ClientLogEvent::LeftVoluntarily => "left".to_string(),
                        ClientLogEvent::Disconnected { code, .. } => format!("disconnected {code}"),
                        ClientLogEvent::Kicked { code, .. } => format!("kicked {code:?}"),
                        ClientLogEvent::ServerShutdown { code } => format!("shutdown {code}"),
                    };
                    *counts.entry(key).or_default() += 1;
                    session.apply(&event, 0);
                }
            }
            session.settle(i64::MAX / 2);
            let state = match session.current_drop() {
                Some(drop) => format!("drop {:?} {:?}", drop.kind, drop.code),
                None if session.ended_without_leaving() => "ended without leaving".to_string(),
                None => "left".to_string(),
            };
            *finals.entry(state).or_default() += 1;
        }
        println!("events: {counts:#?}");
        println!("final state per log: {finals:#?}");
    }

    #[test]
    fn a_long_kick_message_is_cut() {
        let long = "x".repeat(500);
        let Some(ClientLogEvent::Kicked { message: Some(m), .. }) =
            classify_log_line(&format!("You were kicked from this experience: {long}"))
        else {
            panic!("expected a kick");
        };
        assert_eq!(m.chars().count(), KICK_MESSAGE_MAX_CHARS);
    }
}

#[cfg(test)]
mod client_log_session_tests {
    use super::*;

    const T0: i64 = 1_791_000_000_000;

    fn joined() -> ClientLogEvent {
        ClientLogEvent::JoinedGame {
            place_id: Some(1),
            job_id: Some("job".into()),
        }
    }

    fn lost() -> ClientLogEvent {
        ClientLogEvent::Disconnected {
            code: 277,
            reason: DropReason::ConnectionLost,
        }
    }

    #[test]
    fn a_drop_with_no_teleport_shows_at_once() {
        let mut s = ClientLogSession::default();
        s.apply(&joined(), T0);
        s.apply(&lost(), T0 + 1_000);
        s.settle(T0 + 1_000);
        let drop = s.current_drop().expect("dropped");
        assert_eq!(drop.kind, DropKind::Disconnected);
        assert_eq!(drop.reason, Some(DropReason::ConnectionLost));
        assert_eq!(drop.code, Some(277));
    }

    #[test]
    fn a_drop_right_after_a_teleport_waits_the_grace_and_a_join_cancels_it() {
        let mut s = ClientLogSession::default();
        s.apply(&joined(), T0);
        s.apply(&ClientLogEvent::TeleportStarted, T0 + 1_000);
        s.apply(&lost(), T0 + 1_500);
        s.settle(T0 + 3_000);
        assert!(s.current_drop().is_none(), "still inside the teleport grace");
        s.apply(&joined(), T0 + 4_000);
        s.settle(T0 + 20_000);
        assert!(s.current_drop().is_none(), "the teleport landed: not a drop");
    }

    #[test]
    fn a_drop_after_a_teleport_that_never_lands_shows_after_the_grace() {
        let mut s = ClientLogSession::default();
        s.apply(&ClientLogEvent::TeleportStarted, T0);
        s.apply(&lost(), T0 + 500);
        s.settle(T0 + 500 + TELEPORT_GRACE_MS - 1);
        assert!(s.current_drop().is_none());
        s.settle(T0 + 500 + TELEPORT_GRACE_MS);
        assert!(s.current_drop().is_some());
    }

    #[test]
    fn a_client_that_never_joined_a_game_did_not_crash_when_it_ends() {
        let mut s = ClientLogSession::default();
        assert!(!s.ended_without_leaving());
        s.apply(&joined(), T0);
        assert!(s.ended_without_leaving());
    }

    #[test]
    fn joining_again_clears_the_drop() {
        let mut s = ClientLogSession::default();
        s.apply(&lost(), T0);
        s.settle(T0);
        assert!(s.current_drop().is_some());
        s.apply(&joined(), T0 + 30_000);
        assert!(s.current_drop().is_none());
    }

    #[test]
    fn what_comes_after_leaving_is_not_a_drop() {
        let mut s = ClientLogSession::default();
        s.apply(&joined(), T0);
        s.apply(&ClientLogEvent::LeftVoluntarily, T0 + 1_000);
        s.apply(&lost(), T0 + 1_100);
        s.settle(T0 + 2_000);
        assert!(s.current_drop().is_none());
        assert!(!s.ended_without_leaving());
    }

    #[test]
    fn the_kick_message_on_a_later_line_joins_the_kick() {
        let mut s = ClientLogSession::default();
        s.apply(
            &ClientLogEvent::Kicked {
                code: Some(267),
                message: None,
            },
            T0,
        );
        s.settle(T0);
        s.apply(
            &ClientLogEvent::Kicked {
                code: Some(267),
                message: Some("bye".into()),
            },
            T0 + 100,
        );
        let drop = s.current_drop().unwrap();
        assert_eq!(drop.kind, DropKind::Kicked);
        assert_eq!(drop.message.as_deref(), Some("bye"));
        assert_eq!(drop.since_ms, T0, "the first line marks when it dropped");
    }

    #[test]
    fn replaying_a_whole_log_ends_in_its_last_state() {
        // O monitor lê o log inteiro na 1ª vez (app reaberto com o cliente já
        // em jogo): queda antiga seguida de join não aparece.
        let log = "\
x [FLog::Output] ! Joining game 'a' place 1 at 10.0.0.1
x [FLog::Network] Disconnection Notification. Reason: 277
x [FLog::Output] ! Joining game 'b' place 1 at 10.0.0.2
x [FLog::UgcExperienceController] UgcExperienceController: doTeleport: url
x [DFLog::NetworkClient] Client:Disconnect
x [FLog::SessionTransitionFSM] Teleported.
x [FLog::Output] ! Joining game 'c' place 2 at 10.0.0.3
x [DFLog::RbxTransportDummyClient] Disconnected from server for reason: Player: 285 (DisconnectClientInitiated)
";
        let mut s = ClientLogSession::default();
        s.absorb(log, T0);
        s.settle(T0 + 60_000);
        assert!(s.current_drop().is_none());
        assert!(s.ended_without_leaving());
    }
}

#[cfg(test)]
mod client_health_monitor_tests {
    use super::*;
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    const T0: i64 = 1_791_000_000_000;

    #[derive(Default)]
    struct FakeOs {
        alive: RefCell<HashSet<u32>>,
        logs: RefCell<HashMap<u32, (PathBuf, String)>>,
        terminated: RefCell<HashSet<u32>>,
        locate_calls: RefCell<u32>,
    }

    impl FakeOs {
        fn with_client(pid: u32, log: &str) -> Self {
            let os = Self::default();
            os.alive.borrow_mut().insert(pid);
            os.logs
                .borrow_mut()
                .insert(pid, (PathBuf::from(format!("C:/logs/{pid}.log")), log.to_string()));
            os
        }
        fn append(&self, pid: u32, more: &str) {
            self.logs.borrow_mut().get_mut(&pid).unwrap().1.push_str(more);
        }
    }

    impl ClientHealthOs for FakeOs {
        fn alive_pids(&self) -> HashSet<u32> {
            self.alive.borrow().clone()
        }
        fn locate_logs(&self, pids: &[u32], taken: &HashSet<PathBuf>) -> HashMap<u32, PathBuf> {
            *self.locate_calls.borrow_mut() += 1;
            self.logs
                .borrow()
                .iter()
                .filter(|(pid, (path, _))| pids.contains(pid) && !taken.contains(path))
                .map(|(pid, (path, _))| (*pid, path.clone()))
                .collect()
        }
        fn read_log(&self, path: &Path, offset: u64, max: u64) -> Option<Vec<u8>> {
            let logs = self.logs.borrow();
            let (_, content) = logs.values().find(|(p, _)| p == path)?;
            let bytes = content.as_bytes();
            let start = (offset as usize).min(bytes.len());
            let end = (start + max as usize).min(bytes.len());
            Some(bytes[start..end].to_vec())
        }
        fn terminated_by_app(&self, pid: u32) -> bool {
            self.terminated.borrow().contains(&pid)
        }
    }

    const JOIN: &str = "x [FLog::Output] ! Joining game 'a' place 1 at 10.0.0.1\n";
    const LOST: &str = "x [FLog::Network] Disconnection Notification. Reason: 277\n";
    const LEAVE: &str = "x [FLog::SingleSurfaceApp] leaveUGCGameInternal\n";

    fn ours(user_id: i64, pid: u32) -> TrackedClient {
        TrackedClient {
            user_id,
            pid,
            adopted: false,
        }
    }

    #[test]
    fn a_drop_in_the_log_becomes_one_notice_and_shows_in_the_view() {
        let os = FakeOs::with_client(100, JOIN);
        let mut monitor = ClientHealthMonitor::new();
        assert!(monitor.tick(&os, &[ours(1, 100)], T0).is_empty());

        os.append(100, LOST);
        let notices = monitor.tick(&os, &[ours(1, 100)], T0 + 2_000);
        assert_eq!(notices.len(), 1);
        assert!(matches!(&notices[0], HealthNotice::Dropped { user_id: 1, drop, .. } if drop.code == Some(277)));
        assert!(monitor.tick(&os, &[ours(1, 100)], T0 + 4_000).is_empty(), "reported once");

        let view = monitor.views().remove(&1).unwrap();
        assert!(view.log_found);
        assert_eq!(view.drop.unwrap().reason, Some(DropReason::ConnectionLost));
    }

    #[test]
    fn rejoining_clears_the_drop_with_a_notice() {
        let os = FakeOs::with_client(100, &format!("{JOIN}{LOST}"));
        let mut monitor = ClientHealthMonitor::new();
        monitor.tick(&os, &[ours(1, 100)], T0);
        os.append(100, JOIN);
        let notices = monitor.tick(&os, &[ours(1, 100)], T0 + 2_000);
        assert_eq!(notices, vec![HealthNotice::Recovered { user_id: 1 }]);
        assert!(monitor.views()[&1].drop.is_none());
    }

    #[test]
    fn an_adopted_client_is_watched_too_and_flagged_as_adopted() {
        // Só aviso na tela: quem decide fechar é o Watcher, que ignora adotados.
        let os = FakeOs::with_client(100, &format!("{JOIN}{LOST}"));
        let mut monitor = ClientHealthMonitor::new();
        let notices = monitor.tick(
            &os,
            &[TrackedClient {
                user_id: 1,
                pid: 100,
                adopted: true,
            }],
            T0,
        );
        assert!(matches!(&notices[0], HealthNotice::Dropped { adopted: true, .. }));
    }

    #[test]
    fn a_process_that_ends_without_leaving_crashed() {
        let os = FakeOs::with_client(100, JOIN);
        let mut monitor = ClientHealthMonitor::new();
        monitor.tick(&os, &[ours(1, 100)], T0);
        os.alive.borrow_mut().clear();
        let notices = monitor.tick(&os, &[ours(1, 100)], T0 + 2_000);
        assert!(matches!(&notices[0], HealthNotice::Dropped { drop, .. } if drop.kind == DropKind::Crashed));
    }

    #[test]
    fn closing_the_window_or_the_app_closing_it_is_not_a_crash() {
        let os = FakeOs::with_client(100, &format!("{JOIN}{LEAVE}"));
        let mut monitor = ClientHealthMonitor::new();
        monitor.tick(&os, &[ours(1, 100)], T0);
        os.alive.borrow_mut().clear();
        assert!(monitor.tick(&os, &[ours(1, 100)], T0 + 2_000).is_empty());

        let os = FakeOs::with_client(200, JOIN);
        let mut monitor = ClientHealthMonitor::new();
        monitor.tick(&os, &[ours(2, 200)], T0);
        os.alive.borrow_mut().clear();
        os.terminated.borrow_mut().insert(200);
        assert!(monitor.tick(&os, &[ours(2, 200)], T0 + 2_000).is_empty());
    }

    #[test]
    fn without_a_log_nothing_is_guessed() {
        let os = FakeOs::default();
        os.alive.borrow_mut().insert(100);
        let mut monitor = ClientHealthMonitor::new();
        monitor.tick(&os, &[ours(1, 100)], T0);
        assert!(!monitor.views()[&1].log_found);
        os.alive.borrow_mut().clear();
        assert!(monitor.tick(&os, &[ours(1, 100)], T0 + 2_000).is_empty());
    }

    #[test]
    fn a_missing_log_is_looked_for_again_only_after_the_retry_delay() {
        let os = FakeOs::default();
        os.alive.borrow_mut().insert(100);
        let mut monitor = ClientHealthMonitor::new();
        monitor.tick(&os, &[ours(1, 100)], T0);
        monitor.tick(&os, &[ours(1, 100)], T0 + 2_000);
        assert_eq!(*os.locate_calls.borrow(), 1);
        monitor.tick(&os, &[ours(1, 100)], T0 + CLIENT_LOG_RETRY_MS);
        assert_eq!(*os.locate_calls.borrow(), 2);
    }

    #[test]
    fn a_new_pid_for_the_account_starts_clean() {
        let os = FakeOs::with_client(100, &format!("{JOIN}{LOST}"));
        os.alive.borrow_mut().insert(300);
        os.logs
            .borrow_mut()
            .insert(300, (PathBuf::from("C:/logs/300.log"), JOIN.to_string()));
        let mut monitor = ClientHealthMonitor::new();
        monitor.tick(&os, &[ours(1, 100)], T0);
        assert!(monitor.views()[&1].drop.is_some());
        monitor.tick(&os, &[ours(1, 300)], T0 + 2_000);
        let view = &monitor.views()[&1];
        assert_eq!(view.pid, 300);
        assert!(view.drop.is_none());
    }

    #[test]
    fn only_new_bytes_are_read_and_a_half_line_waits() {
        let os = FakeOs::with_client(100, JOIN);
        let mut monitor = ClientHealthMonitor::new();
        monitor.tick(&os, &[ours(1, 100)], T0);
        // Linha ainda sendo escrita: não conta até ganhar o fim de linha.
        os.append(100, "x [FLog::Network] Disconnection Notification. Reason: 27");
        assert!(monitor.tick(&os, &[ours(1, 100)], T0 + 2_000).is_empty());
        os.append(100, "7\n");
        let notices = monitor.tick(&os, &[ours(1, 100)], T0 + 4_000);
        assert!(matches!(&notices[0], HealthNotice::Dropped { drop, .. } if drop.code == Some(277)));
    }

    #[test]
    fn the_console_line_says_why_in_plain_words() {
        let drop = |kind, reason, code, message: Option<&str>| ClientDrop {
            kind,
            reason,
            code,
            message: message.map(String::from),
            since_ms: T0,
        };
        assert_eq!(
            drop_console_line(&drop(DropKind::Disconnected, Some(DropReason::ConnectionLost), Some(277), None)),
            "Caiu: perdeu a conexão (código 277)"
        );
        assert_eq!(
            drop_console_line(&drop(DropKind::Kicked, None, Some(267), Some("bye"))),
            "Foi expulso: bye"
        );
        assert_eq!(
            drop_console_line(&drop(DropKind::ServerShutdown, None, Some(274), None)),
            "O servidor fechou (código 274)"
        );
    }
}
