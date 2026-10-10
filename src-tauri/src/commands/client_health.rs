// Saúde de cada cliente que o app acompanha: a queda lida do log do Roblox,
// com o motivo, e o nome da conta no título da janela. Ver
// docs/features/watcher.md ("Quedas" e "Nome da conta na janela").
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

/// Onde a conta entrou por último (`! Joining game '<job>' place <place>`).
/// A reconexão (commands/reconnect.rs) volta para cá; nunca vai para a tela.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinedDestination {
    pub place_id: i64,
    pub job_id: Option<String>,
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
    /// Place e servidor do último `Joining game` (histórico de sessões).
    place_id: Option<i64>,
    job_id: Option<String>,
    destination: Option<JoinedDestination>,
}

impl ClientLogSession {
    pub fn apply(&mut self, event: &ClientLogEvent, now_ms: i64) {
        match event {
            ClientLogEvent::JoinedGame { place_id, job_id } => {
                // Entrou num jogo (de novo): o que caiu antes já não vale.
                self.place_id = *place_id;
                self.job_id = job_id.clone();
                self.pending = None;
                self.drop = None;
                self.left = false;
                self.joined = true;
                self.last_teleport_ms = None;
                if let Some(place_id) = place_id {
                    self.destination = Some(JoinedDestination {
                        place_id: *place_id,
                        job_id: job_id.clone(),
                    });
                }
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

    /// Está num jogo agora: entrou, e depois disso não caiu nem saiu. É o
    /// sinal que a fila de launch espera antes da próxima conta (launch.rs).
    pub fn in_game(&self) -> bool {
        self.joined && !self.left && self.drop.is_none() && self.pending.is_none()
    }

    /// Onde a conta entrou por último, se o log disse.
    pub fn destination(&self) -> Option<&JoinedDestination> {
        self.destination.as_ref()
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedClient {
    pub user_id: i64,
    pub pid: u32,
    /// Aberto pelo site e reconhecido: só aviso na tela, nunca ação.
    pub adopted: bool,
    /// O nome que vai no título da janela (já mascarado com os nomes
    /// ocultos). `None`: opção desligada — o título volta a ser "Roblox".
    pub window_label: Option<String>,
}

// ── Nome da conta no título da janela ──────────────────────────────────────

/// O título que o Roblox põe na janela do cliente em jogo.
pub const ROBLOX_WINDOW_TITLE: &str = "Roblox";
/// O que a tela mostra no lugar de um nome escondido por inteiro (igual a
/// `HIDDEN_NAME` em src/utils/accountName.ts).
const HIDDEN_ACCOUNT_NAME: &str = "************";

/// Espelho de `maskAccountName` (src/utils/accountName.ts), com a mesma conta
/// de letras do JavaScript (unidades UTF-16): com os nomes ocultos, a barra de
/// tarefas não pode mostrar mais do que a tela do app. Os dois lados leem os
/// mesmos casos de `src/utils/accountNameCases.json`.
pub fn mask_account_name(name: &str, hidden: bool, preview_letters: i64) -> String {
    if !hidden {
        return name.to_string();
    }
    let units: Vec<u16> = name.encode_utf16().collect();
    if preview_letters > 0 && (preview_letters as usize) < units.len() {
        let mut shown = String::from_utf16_lossy(&units[..preview_letters as usize]);
        shown.push_str("********");
        return shown;
    }
    HIDDEN_ACCOUNT_NAME.to_string()
}

/// Alias || Username, mascarado como a tela do app mostra.
pub fn window_account_label(alias: &str, username: &str, hidden: bool, preview_letters: i64) -> String {
    let raw = if alias.is_empty() { username } else { alias };
    mask_account_name(raw, hidden, preview_letters)
}

/// `Roblox — <conta>`.
pub fn client_window_title(label: &str) -> String {
    format!("{ROBLOX_WINDOW_TITLE} — {label}")
}

/// O título como o Roblox o pôs: se o atual é o que o app pôs (ou poria agora,
/// depois de reabrir), vale "Roblox". As regras do Watcher que olham o título
/// (título esperado, beta, sem conexão) comparam isto, nunca o nome da conta.
pub fn effective_client_title<'a>(current: &'a str, applied: Option<&str>, desired: Option<&str>) -> &'a str {
    let ours = !current.is_empty() && (applied == Some(current) || desired == Some(current));
    if ours {
        ROBLOX_WINDOW_TITLE
    } else {
        current
    }
}

/// O título a pôr agora, se precisar mudar. Só mexe numa janela que está com o
/// título normal do Roblox (ou o nosso): se o Roblox mostra outra coisa (erro,
/// beta), deixa como está, para as regras do Watcher continuarem vendo.
pub fn plan_window_title(current: &str, applied: Option<&str>, desired: Option<&str>) -> Option<String> {
    if effective_client_title(current, applied, desired) != ROBLOX_WINDOW_TITLE {
        return None;
    }
    let want = desired.unwrap_or(ROBLOX_WINDOW_TITLE);
    (current != want).then(|| want.to_string())
}

/// O que o monitor precisa do sistema (dublê nos testes).
pub trait ClientHealthOs {
    fn alive_pids(&self) -> HashSet<u32>;
    /// PID → log, para os PIDs sem log. `taken`: logs que já têm dono.
    fn locate_logs(&self, pids: &[u32], taken: &HashSet<std::path::PathBuf>) -> HashMap<u32, std::path::PathBuf>;
    fn read_log(&self, path: &std::path::Path, offset: u64, max: u64) -> Option<Vec<u8>>;
    /// O próprio app fechou este PID (Fechar, Auto Rejoin, Watcher…).
    fn terminated_by_app(&self, pid: u32) -> bool;
    /// Janela principal do cliente (HWND como número).
    fn main_window(&self, pid: u32) -> Option<isize>;
    fn window_title(&self, hwnd: isize) -> String;
    fn set_window_title(&self, hwnd: isize, title: &str) -> bool;
    /// A janela está "Não respondendo" (`IsHungAppWindow`).
    fn is_hung(&self, hwnd: isize) -> bool;
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
    /// O título que o app pôs na janela (para reconhecê-lo depois).
    applied_title: Option<String>,
    hung: HungWatch,
    not_responding: bool,
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
            applied_title: None,
            hung: HungWatch::default(),
            not_responding: false,
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
    /// O título que o app pôs na janela (nome da conta), se pôs.
    pub window_title: Option<String>,
    /// A janela está "Não respondendo" há 30 s ou mais (só clientes do app).
    pub not_responding: bool,
    /// Está num jogo agora (o log disse que entrou, e não caiu nem saiu).
    pub in_game: bool,
    /// Onde entrou por último — só para a reconexão, não vai para a tela.
    #[serde(skip)]
    pub destination: Option<JoinedDestination>,
    /// O processo terminou (e o monitor viu).
    #[serde(skip)]
    pub exited: bool,
}

/// Retrato de um cliente para o histórico de sessões: o jogo em que está, se
/// caiu, se a pessoa saiu e se o processo já terminou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientSessionSnapshot {
    pub pid: u32,
    pub place_id: Option<i64>,
    pub job_id: Option<String>,
    pub drop: Option<ClientDrop>,
    pub left: bool,
    pub exited: bool,
}

/// Quanto tempo a janela fica "Não respondendo" antes de o app avisar (e de
/// o Watcher poder fechá-la, se a opção estiver ligada).
pub const HUNG_THRESHOLD_MS: i64 = 30_000;

/// Desde quando a janela não responde. O Windows já só chama de travada a
/// janela que passou 5 s sem tratar mensagens (`IsHungAppWindow`); os 30 s
/// daqui tiram do caminho a carga de um jogo pesado.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HungWatch {
    since_ms: Option<i64>,
}

impl HungWatch {
    pub fn update(&mut self, hung: bool, now_ms: i64) {
        if hung {
            self.since_ms.get_or_insert(now_ms);
        } else {
            self.since_ms = None;
        }
    }

    pub fn is_not_responding(&self, now_ms: i64) -> bool {
        self.since_ms
            .is_some_and(|since| now_ms - since >= HUNG_THRESHOLD_MS)
    }
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
    /// A janela ficou (ou deixou de ficar) "Não respondendo" por 30 s.
    NotResponding { user_id: i64, not_responding: bool },
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

            let window = alive
                .contains(&entry.pid)
                .then(|| os.main_window(entry.pid))
                .flatten();
            let hung = window.is_some_and(|hwnd| os.is_hung(hwnd));
            // "Não respondendo" só nos clientes que o app abriu: o do site é
            // da pessoa, e o app não tem o que fazer com ele.
            let was_not_responding = entry.not_responding;
            entry.hung.update(hung && !entry.adopted, now_ms);
            let not_responding = entry.hung.is_not_responding(now_ms);
            entry.not_responding = not_responding;
            if not_responding != was_not_responding {
                notices.push(HealthNotice::NotResponding {
                    user_id: client.user_id,
                    not_responding,
                });
            }
            // Janela travada: nem o título se mexe (só volta a conferir quando
            // ela responder).
            if let (Some(hwnd), false) = (window, hung) {
                Self::apply_window_title(os, entry, hwnd, client.window_label.as_deref());
            }

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

    /// Põe (ou tira) o nome da conta no título. O Roblox pode trocar o título
    /// de volta (teleporte): cada passada confere e põe de novo — ler o título
    /// não custa nada.
    fn apply_window_title(
        os: &dyn ClientHealthOs,
        entry: &mut ClientHealthEntry,
        hwnd: isize,
        label: Option<&str>,
    ) {
        let desired = label
            .filter(|l| !l.trim().is_empty())
            .map(client_window_title);
        let current = os.window_title(hwnd);
        if current != ROBLOX_WINDOW_TITLE
            && effective_client_title(&current, entry.applied_title.as_deref(), desired.as_deref())
                == ROBLOX_WINDOW_TITLE
        {
            // Já é nosso (inclusive o que ficou de antes de o app reabrir).
            entry.applied_title = Some(current.clone());
        }
        if let Some(next) =
            plan_window_title(&current, entry.applied_title.as_deref(), desired.as_deref())
        {
            if os.set_window_title(hwnd, &next) {
                entry.applied_title = (next != ROBLOX_WINDOW_TITLE).then_some(next);
            }
        }
    }

    /// Onde cada cliente está e como a sessão dele terminou, para o histórico
    /// de sessões (commands/session_history.rs). Só leitura do que o monitor já
    /// sabe: nada novo é lido do log por causa disto.
    pub fn session_snapshots(&self) -> HashMap<i64, ClientSessionSnapshot> {
        self.entries
            .iter()
            .map(|(uid, entry)| {
                (
                    *uid,
                    ClientSessionSnapshot {
                        pid: entry.pid,
                        place_id: entry.session.place_id,
                        job_id: entry.session.job_id.clone(),
                        drop: entry.current_drop().cloned(),
                        left: entry.session.left,
                        exited: entry.exit_seen,
                    },
                )
            })
            .collect()
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
                        window_title: entry.applied_title.clone(),
                        not_responding: entry.not_responding,
                        in_game: !entry.exit_seen && entry.session.in_game(),
                        destination: entry.session.destination().cloned(),
                        exited: entry.exit_seen,
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
    fn main_window(&self, pid: u32) -> Option<isize> {
        platform::windows::find_main_window(pid).map(|hwnd| hwnd as isize)
    }
    fn window_title(&self, hwnd: isize) -> String {
        platform::windows::get_window_title(hwnd as _)
    }
    fn set_window_title(&self, hwnd: isize, title: &str) -> bool {
        platform::windows::set_window_title(hwnd as _, title)
    }
    fn is_hung(&self, hwnd: isize) -> bool {
        platform::windows::is_window_hung(hwnd as _)
    }
}

/// O nome de cada conta para o título da janela, ou nada com a opção
/// desligada. Mesma regra da tela: Alias || Username, mascarado com os nomes
/// ocultos (`General.HideUsernames` + `HiddenNameLetters`, que a tela grava).
#[cfg(target_os = "windows")]
fn window_labels(app: &tauri::AppHandle) -> HashMap<i64, String> {
    let settings = app.state::<SettingsStore>();
    if settings.get_string("General", "ShowAccountNameOnWindow") == "false" {
        return HashMap::new();
    }
    let hidden = settings.get_bool("General", "HideUsernames");
    let letters = settings.get_int("General", "HiddenNameLetters").unwrap_or(0);
    app.state::<AccountStore>()
        .get_all()
        .unwrap_or_default()
        .into_iter()
        .map(|a| (a.user_id, window_account_label(&a.alias, &a.username, hidden, letters)))
        .collect()
}

#[cfg(target_os = "windows")]
static CLIENT_HEALTH_MONITOR: LazyLock<Mutex<ClientHealthMonitor>> =
    LazyLock::new(|| Mutex::new(ClientHealthMonitor::new()));

/// Uma passada de verdade: lê o tracker, roda o monitor e publica o retrato.
#[cfg(target_os = "windows")]
fn run_client_health_tick(app: &tauri::AppHandle) -> Vec<HealthNotice> {
    let tracked_processes = platform::windows::tracker().get_all();
    let mut labels = if tracked_processes.is_empty() {
        HashMap::new()
    } else {
        window_labels(app)
    };
    let tracked: Vec<TrackedClient> = tracked_processes
        .into_iter()
        .map(|p| TrackedClient {
            user_id: p.user_id,
            pid: p.pid,
            adopted: p.adopted,
            window_label: labels.remove(&p.user_id),
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
    let snapshots = monitor.session_snapshots();
    drop(monitor);
    // Histórico de sessões (ideia 6): grava cada mudança na hora.
    record_session_history(app, &snapshots, now_ms);
    notices
}

/// Liga o monitor em segundo plano (sempre, como a varredura de clientes de
/// fora): sem cliente rastreado, cada passada custa só ler o tracker.
#[cfg(target_os = "windows")]
pub(crate) fn start_client_health_monitor(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let handle = app.clone();
            let notices = tokio::task::spawn_blocking(move || run_client_health_tick(&handle))
                .await
                .unwrap_or_default();
            for notice in &notices {
                emit_health_notice(&app, notice);
            }
            // Reconexão automática da conta que caiu (commands/reconnect.rs).
            reconnect_after_health_tick(&app, &notices);
            // PC acordado durante Modo AFK / Auto Rejoin / reconexão (keep_awake.rs).
            keep_awake_tick(&app);
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
        HealthNotice::NotResponding {
            user_id,
            not_responding,
        } => {
            let _ = app.emit(
                "roblox-client-health",
                serde_json::json!({
                    "userId": user_id,
                    "notResponding": not_responding,
                }),
            );
            let line = if *not_responding {
                "Não respondendo há 30 s"
            } else {
                "Voltou a responder"
            };
            emit_launch_log(app, *user_id, if *not_responding { "warn" } else { "info" }, "client", String::from(line));
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
