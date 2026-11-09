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
    /// O Roblox tirou a conta ou não a deixa entrar (ver [`NO_ACCESS_CODES`]):
    /// relançar só repetiria a recusa.
    NoAccess,
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
    /// `[FLog::GameJoinLoadTime] ... userid:<conta>`: com que conta o cliente
    /// entrou no jogo (ver `roblox_log::game_join_user_id`).
    JoinedAs { user_id: i64 },
    /// `UDMUX Address = <ip>, ...`: o IP **público** do servidor em que entrou
    /// (só vira evento se for público — IP interno nunca é geolocalizado).
    ServerAddress { address: String },
    /// `Server Prefix: ..._<AAAAMMDDTHHMMSSZ>_RCC_...`: quando o servidor subiu.
    ServerStarted { started_at_ms: i64 },
    /// A tela "confirme que é humano" do Roblox apareceu na janela
    /// (`"feature":"challengeDisplayed"` do navegador embutido do cliente).
    ChallengeShown,
    /// A pessoa resolveu a verificação (`"feature":"challengeCompleted"`).
    ChallengeCompleted,
    /// `LogWin32BTId, App, web launch, websiteBTId is <id>`: o browser tracker
    /// id da URL de launch, isto é, de qual launch (de qual conta) é este log.
    LaunchedWith { browser_tracker_id: String },
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

/// O Roblox tirou a conta do jogo ou não a deixa entrar (`Enum.ConnectionError`):
/// 268 comportamento inesperado do cliente, 281 política de segurança, 282 IP
/// bloqueado, 286 tirada do servidor privado, 287 jogo moderado, 515 jogo
/// desativado, 523 jogo restrito, 524 sem permissão, 533 privacidade, 600 banida
/// pelo criador do jogo. Relançar repetiria a recusa. Os de teleporte (770, 773)
/// ficam de fora de propósito: teleporte recusado deixa o cliente onde está.
const NO_ACCESS_CODES: [u32; 10] = [268, 281, 282, 286, 287, 515, 523, 524, 533, 600];

fn drop_reason_for_code(code: u32) -> DropReason {
    match code {
        code if NO_ACCESS_CODES.contains(&code) => DropReason::NoAccess,
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
    // faixa das desconexões e os joins recusados: "error code: 403" de um asset
    // não é queda.
    let lower = line.to_ascii_lowercase();
    number_after(&lower, "error code: ").filter(|code| (256..=299).contains(code) || NO_ACCESS_CODES.contains(code))
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
    if let Some(rest) = text_after(line, "websiteBTId is ") {
        let id: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        return (!id.is_empty()).then_some(ClientLogEvent::LaunchedWith { browser_tracker_id: id });
    }
    if line.contains("\"feature\":\"challengeDisplayed\"") {
        return Some(ClientLogEvent::ChallengeShown);
    }
    if line.contains("\"feature\":\"challengeCompleted\"") {
        return Some(ClientLogEvent::ChallengeCompleted);
    }
    if let Some(user_id) = crate::roblox_log::game_join_user_id(line) {
        return Some(ClientLogEvent::JoinedAs { user_id });
    }
    if line.contains("UDMUX Address = ") {
        return crate::roblox_log::udmux_address(line)
            .filter(|address| crate::api::roblox::is_public_ip(address))
            .map(|address| ClientLogEvent::ServerAddress { address });
    }
    if line.contains("Server Prefix: ") {
        return crate::roblox_log::server_started_at_ms(line)
            .map(|started_at_ms| ClientLogEvent::ServerStarted { started_at_ms });
    }
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
    if let Some(code) = disconnect_code(line) {
        return event_for_disconnect_code(code);
    }
    disconnect_reason_text(line).and_then(event_for_disconnect_code)
}

/// A queda que o log conta só em texto, sem código: é assim que o cliente
/// registra a perda de conexão de verdade (internet caiu → aviso "Error Code:
/// 277" na tela, mas no log só `Lost connection with reason : Lost connection
/// to the game server, please reconnect`). Lê o motivo e devolve o código
/// equivalente; motivo desconhecido não vira queda.
fn disconnect_reason_text(line: &str) -> Option<u32> {
    let lower = line.to_ascii_lowercase();
    let reason = ["client has been disconnected with reason", "lost connection with reason"]
        .iter()
        .find_map(|marker| text_after(&lower, marker))?;
    if reason.contains("lost connection") || reason.contains("internet connection") {
        Some(277)
    } else if reason.contains("different device") {
        Some(273)
    } else if reason.contains("shut down") || reason.contains("shutdown") {
        Some(274)
    } else {
        None
    }
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
/// A reconexão (commands/reconnect.rs) volta para cá; place e Job ID nunca vão
/// para a tela.
///
/// O servidor do mesmo join vem nas linhas seguintes: o IP público do proxy
/// UDMUX (para a região) e o carimbo do `Server Prefix` (há quanto tempo o
/// servidor está no ar) — ver `ClientServerView`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JoinedDestination {
    pub place_id: i64,
    pub job_id: Option<String>,
    /// IP público do servidor (`UDMUX Address`), nunca um interno.
    pub udmux_address: Option<String>,
    /// Quando o servidor subiu (ms Unix), pelo `Server Prefix`.
    pub server_started_ms: Option<i64>,
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
    /// A conta com que o cliente entrou no jogo, pelo log (a última vista).
    logged_user_id: Option<i64>,
    /// A verificação "confirme que é humano" está na tela, esperando a pessoa.
    challenge: bool,
    /// O browser tracker id do launch que abriu este cliente, pelo log.
    browser_tracker_id: Option<String>,
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
                // Entrou: a verificação (se havia) passou.
                self.challenge = false;
                match (place_id, self.destination.as_mut()) {
                    (Some(place_id), _) => {
                        self.destination = Some(JoinedDestination {
                            place_id: *place_id,
                            job_id: job_id.clone(),
                            ..JoinedDestination::default()
                        });
                    }
                    // Servidor novo sem place na linha: o destino fica (é para
                    // lá que a reconexão volta), o servidor de antes não vale.
                    (None, Some(destination)) => {
                        destination.udmux_address = None;
                        destination.server_started_ms = None;
                    }
                    (None, None) => {}
                }
            }
            ClientLogEvent::ServerAddress { address } => {
                if let Some(destination) = self.destination.as_mut() {
                    destination.udmux_address = Some(address.clone());
                }
            }
            ClientLogEvent::ServerStarted { started_at_ms } => {
                if let Some(destination) = self.destination.as_mut() {
                    destination.server_started_ms = Some(*started_at_ms);
                }
            }
            ClientLogEvent::JoinedAs { user_id } => self.logged_user_id = Some(*user_id),
            ClientLogEvent::ChallengeShown => self.challenge = true,
            ClientLogEvent::ChallengeCompleted => self.challenge = false,
            ClientLogEvent::LaunchedWith { browser_tracker_id } => {
                self.browser_tracker_id = Some(browser_tracker_id.clone())
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

    /// A tela "confirme que é humano" está esperando a pessoa (apareceu, e
    /// ainda não foi resolvida nem a conta entrou no jogo).
    pub fn challenge_pending(&self) -> bool {
        self.challenge
    }

    /// A conta com que o cliente entrou no jogo, se o log já disse.
    pub fn logged_user_id(&self) -> Option<i64> {
        self.logged_user_id
    }

    /// De qual launch é este log (`websiteBTId`), se ele já disse.
    pub fn browser_tracker_id(&self) -> Option<&str> {
        self.browser_tracker_id.as_deref()
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

/// O cliente que o app abriu para `expected_user_id` entrou no jogo como
/// **outra** conta? Devolve a conta que o log mostrou (`userid:` da linha
/// `[FLog::GameJoinLoadTime]`). Acontece quando o Roblox reaproveita a sessão
/// de outra conta (cookie trocado, login feito à mão no cliente). Cliente
/// adotado do site nunca conta: ele foi reconhecido justamente por essa linha,
/// e a identificação manual é decisão da pessoa. Nada aqui fecha o cliente —
/// ele pode ser a conta principal de quem usa o app.
pub fn wrong_account(expected_user_id: i64, logged_user_id: Option<i64>, adopted: bool) -> Option<i64> {
    if adopted || expected_user_id <= 0 {
        return None;
    }
    logged_user_id.filter(|logged| *logged != expected_user_id)
}

/// O log lido por um cliente é de **outro** launch? Devolve o browser tracker
/// id que o log mostrou quando não é o da URL que o app usou para esta conta.
/// Cada conta tem o seu (`browserTrackerId` salvo), então o log de outra conta
/// nunca bate. Sem o id dos dois lados (old join, cliente do site) não dá para
/// dizer: `None`, e o log fica.
pub fn log_from_another_launch(expected: &str, seen: Option<&str>, adopted: bool) -> Option<String> {
    if adopted || expected.is_empty() {
        return None;
    }
    seen.filter(|seen| *seen != expected).map(str::to_string)
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
    /// O browser tracker id que foi na URL de launch (vazio: não se sabe).
    pub browser_tracker_id: String,
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

/// Entre o nome da conta e "Roblox" no título.
const WINDOW_TITLE_SEPARATOR: &str = " — ";

/// `<conta> — Roblox`. O nome vem primeiro (decisão do dono, 10/10/2026): ao
/// passar o mouse na barra de tarefas, o Windows mostra o começo do título, e
/// o nome aparece inteiro.
pub fn client_window_title(label: &str) -> String {
    format!("{label}{WINDOW_TITLE_SEPARATOR}{ROBLOX_WINDOW_TITLE}")
}

/// O mesmo título na ordem que a versão anterior punha (`Roblox — <conta>`).
/// Uma janela renomeada por ela e ainda aberta depois da atualização tem que
/// ser reconhecida como nossa (e passar para a ordem nova), não como título
/// estranho para as regras do Watcher.
fn previous_order_title(desired: &str) -> Option<String> {
    let label = desired.strip_suffix(&format!("{WINDOW_TITLE_SEPARATOR}{ROBLOX_WINDOW_TITLE}"))?;
    Some(format!("{ROBLOX_WINDOW_TITLE}{WINDOW_TITLE_SEPARATOR}{label}"))
}

/// O título como o Roblox o pôs: se o atual é o que o app pôs (ou poria agora,
/// depois de reabrir, inclusive na ordem da versão anterior), vale "Roblox". As
/// regras do Watcher que olham o título (título esperado, beta, sem conexão)
/// comparam isto, nunca o nome da conta.
pub fn effective_client_title<'a>(current: &'a str, applied: Option<&str>, desired: Option<&str>) -> &'a str {
    let ours = !current.is_empty()
        && (applied == Some(current)
            || desired == Some(current)
            || desired.and_then(previous_order_title).as_deref() == Some(current));
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
    /// Títulos das janelas visíveis do processo: a caixa de erro do Roblox é
    /// uma janela à parte da principal.
    fn window_titles(&self, pid: u32) -> Vec<String>;
    /// O app fechou este PID porque ele estava doente (travado, memória,
    /// sem conexão). Conta como queda: a reconexão é quem traz de volta.
    fn closed_as_unhealthy(&self, pid: u32) -> bool;
}

/// A caixa que o Roblox abre quando o cliente quebra ("An unexpected error
/// occurred and Roblox needs to quit"). O processo fica vivo até alguém clicar
/// em OK: sem isto a conta seguia "em jogo" com o jogo já morto. Título
/// **exato**: o app põe `<apelido> — Roblox` nas janelas, e um apelido
/// "Roblox Crash" não pode virar queda.
pub fn is_crash_dialog_title(title: &str) -> bool {
    let title = title.trim();
    title.eq_ignore_ascii_case("Roblox Crash")
        || title.to_ascii_lowercase().starts_with("an unexpected error occurred")
}

/// Abaixo disto (e de 10% do total) um cliente novo não cabe: cada cliente do
/// Roblox pede de 1 a 2 GB.
pub const LOW_MEMORY_FLOOR_MB: u64 = 2_048;

fn memory_figures_are_tight(free_mb: u64, total_mb: u64) -> bool {
    total_mb > 0 && free_mb < (total_mb / 10).min(LOW_MEMORY_FLOOR_MB)
}

/// O Windows está quase sem memória? Devolve (livre, total) em MB do que
/// faltou. Olha a memória física e a confirmada (RAM + arquivo de paginação):
/// é a confirmada que, ao acabar, faz o programa quebrar, mesmo com RAM livre.
pub fn tight_memory(physical: (u64, u64), commit: (u64, u64)) -> Option<(u64, u64)> {
    if memory_figures_are_tight(commit.0, commit.1) {
        Some(commit)
    } else if memory_figures_are_tight(physical.0, physical.1) {
        Some(physical)
    } else {
        None
    }
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
    /// A conta que o log mostrou, quando é outra (ver `wrong_account`).
    wrong_account: Option<i64>,
    /// A verificação pendente que já virou aviso (para avisar uma vez só).
    challenge_reported: bool,
    /// O browser tracker id do launch deste cliente (ver `TrackedClient`).
    browser_tracker_id: String,
    /// Logs que este cliente leu e eram de outro launch: nunca mais são dele.
    foreign_logs: HashSet<std::path::PathBuf>,
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
            wrong_account: None,
            challenge_reported: false,
            browser_tracker_id: client.browser_tracker_id.clone(),
            foreign_logs: HashSet::new(),
        }
    }

    /// Larga o log (era de outro launch) e volta a procurar o seu. A janela
    /// (título, travamento) é deste processo e fica como está.
    fn forget_log(&mut self) {
        if let Some(path) = self.log.take() {
            self.foreign_logs.insert(path);
        }
        self.offset = 0;
        self.next_locate_ms = 0;
        self.session = ClientLogSession::default();
        self.crashed = None;
        self.exit_seen = false;
        self.wrong_account = None;
        self.challenge_reported = false;
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
    /// O cliente entrou como outra conta do Roblox (a do log). Só clientes que
    /// o app abriu; o cliente continua aberto.
    pub wrong_account_user_id: Option<i64>,
    /// O servidor em que está: região (pelo IP público) e desde quando está no
    /// ar. `None` enquanto o log não disse nada do servidor.
    pub server: Option<ClientServerView>,
    /// A tela "confirme que é humano" do Roblox está esperando a pessoa.
    pub challenge: bool,
    /// Onde entrou por último — só para a reconexão, não vai para a tela.
    #[serde(skip)]
    pub destination: Option<JoinedDestination>,
    /// O processo terminou (e o monitor viu).
    #[serde(skip)]
    pub exited: bool,
}

/// O servidor de um cliente, como a Sessão mostra: a região já formatada por
/// `General.ServerRegionFormat` (quando a geolocalização respondeu) e quando o
/// servidor subiu. Nada aqui chama a API do Roblox: tudo vem do log, e a região
/// da geolocalização com cache (`lookup_ip_region`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientServerView {
    pub region: Option<String>,
    pub started_at_ms: Option<i64>,
}

/// O que se sabe do servidor pelo destino, ainda sem a região.
pub fn server_view_of(destination: Option<&JoinedDestination>) -> Option<ClientServerView> {
    let destination = destination?;
    if destination.udmux_address.is_none() && destination.server_started_ms.is_none() {
        return None;
    }
    Some(ClientServerView {
        region: None,
        started_at_ms: destination.server_started_ms,
    })
}

/// Região de um IP de servidor, para a Sessão.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerRegionLookup {
    /// Consulta em andamento (desde quando).
    Pending { since_ms: i64 },
    Resolved(crate::api::roblox::IpRegion),
    /// A geolocalização não respondeu: tenta de novo depois de `SERVER_REGION_RETRY_MS`.
    Failed { at_ms: i64 },
}

/// Falha na geolocalização: espera isto antes de perguntar de novo pelo IP.
pub const SERVER_REGION_RETRY_MS: i64 = 10 * 60 * 1000;
/// Consulta pendurada há mais que isto pode ser refeita (o pedido tem teto de
/// 10 s por serviço; isto só cobre uma tarefa perdida).
const SERVER_REGION_PENDING_MAX_MS: i64 = 60_000;
/// Teto de consultas novas por passada do monitor.
pub const SERVER_REGION_LOOKUPS_PER_TICK: usize = 2;

/// A geolocalização disse onde fica (e não só devolveu o IP).
fn region_has_location(region: &crate::api::roblox::IpRegion) -> bool {
    !region.city.trim().is_empty() || !region.country_code.trim().is_empty()
}

/// Os IPs de servidor que ainda precisam de região, sem repetir e só públicos.
pub fn server_addresses_to_locate(
    views: &HashMap<i64, ClientHealthView>,
    lookups: &HashMap<String, ServerRegionLookup>,
    now_ms: i64,
) -> Vec<String> {
    let mut wanted: Vec<String> = Vec::new();
    let mut ids: Vec<&i64> = views.keys().collect();
    ids.sort();
    for id in ids {
        let Some(address) = views[id]
            .destination
            .as_ref()
            .and_then(|d| d.udmux_address.as_deref())
        else {
            continue;
        };
        if !crate::api::roblox::is_public_ip(address) || wanted.iter().any(|w| w == address) {
            continue;
        }
        let due = match lookups.get(address) {
            None => true,
            Some(ServerRegionLookup::Resolved(_)) => false,
            Some(ServerRegionLookup::Pending { since_ms }) => now_ms - since_ms >= SERVER_REGION_PENDING_MAX_MS,
            Some(ServerRegionLookup::Failed { at_ms }) => now_ms - at_ms >= SERVER_REGION_RETRY_MS,
        };
        if due {
            wanted.push(address.to_string());
        }
    }
    wanted
}

/// O resultado de uma consulta, como fica guardado.
pub fn server_region_outcome(region: crate::api::roblox::IpRegion, now_ms: i64) -> ServerRegionLookup {
    if region_has_location(&region) {
        ServerRegionLookup::Resolved(region)
    } else {
        ServerRegionLookup::Failed { at_ms: now_ms }
    }
}

/// Põe a região já conhecida no servidor de cada retrato.
pub fn apply_server_regions(
    views: &mut HashMap<i64, ClientHealthView>,
    lookups: &HashMap<String, ServerRegionLookup>,
    template: &str,
) {
    for view in views.values_mut() {
        let address = view.destination.as_ref().and_then(|d| d.udmux_address.clone());
        let (Some(server), Some(address)) = (view.server.as_mut(), address) else {
            continue;
        };
        if let Some(ServerRegionLookup::Resolved(region)) = lookups.get(&address) {
            server.region = Some(crate::api::roblox::format_region(region, template));
        }
    }
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
    /// O cliente aberto para `user_id` entrou como `logged_user_id`.
    WrongAccount { user_id: i64, logged_user_id: i64 },
    /// O log que este cliente lia era de outro launch: largado, e o certo
    /// volta a ser procurado.
    ForeignLog { user_id: i64 },
    /// A verificação "confirme que é humano" apareceu (`pending`) ou saiu da
    /// tela (resolvida, entrou no jogo ou o cliente fechou).
    /// `resolved`: saiu porque a pessoa resolveu (ou a conta entrou no jogo),
    /// não porque o cliente fechou com ela na tela.
    Challenge { user_id: i64, pending: bool, resolved: bool },
}

fn crashed_drop(now_ms: i64) -> ClientDrop {
    ClientDrop {
        kind: DropKind::Crashed,
        reason: None,
        code: None,
        message: None,
        since_ms: now_ms,
    }
}

/// Lê o que o cliente escreveu no log desde a última passada.
fn read_new_log_lines(os: &dyn ClientHealthOs, entry: &mut ClientHealthEntry, now_ms: i64) {
    let Some(path) = entry.log.clone() else {
        return;
    };
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

#[derive(Debug, Default)]
pub struct ClientHealthMonitor {
    entries: HashMap<i64, ClientHealthEntry>,
    /// Logs que um cliente largou por serem de outro launch, pelo browser
    /// tracker id que mostraram: o cliente daquele launch os pega direto.
    handed_back: HashMap<String, std::path::PathBuf>,
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
        if tracked.is_empty() && self.entries.is_empty() {
            return notices;
        }
        let alive = os.alive_pids();
        // Quem limpa o tracker (Watcher, launch) pode passar antes daqui. O
        // cliente que morreu assim ainda é visto uma última vez: sem isso a
        // queda sumia e a reconexão nem começava (relatos de 08/11/2026).
        let forgotten: Vec<TrackedClient> = self
            .entries
            .iter()
            .filter(|(uid, entry)| {
                !entry.exit_seen && !alive.contains(&entry.pid) && !tracked.iter().any(|t| t.user_id == **uid)
            })
            .map(|(uid, entry)| TrackedClient {
                user_id: *uid,
                pid: entry.pid,
                adopted: entry.adopted,
                window_label: None,
                browser_tracker_id: entry.browser_tracker_id.clone(),
            })
            .collect();
        let tracked: Vec<TrackedClient> = tracked.iter().cloned().chain(forgotten).collect();
        let tracked = tracked.as_slice();
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
            entry.browser_tracker_id = client.browser_tracker_id.clone();
        }

        // Log devolvido por outro cliente: vai direto para o launch dele.
        if !self.handed_back.is_empty() {
            for entry in self.entries.values_mut() {
                if entry.log.is_some() || entry.browser_tracker_id.is_empty() {
                    continue;
                }
                if let Some(path) = self.handed_back.remove(&entry.browser_tracker_id) {
                    entry.log = Some(path);
                }
            }
            let live: HashSet<&str> = self.entries.values().map(|e| e.browser_tracker_id.as_str()).collect();
            self.handed_back.retain(|id, _| live.contains(id.as_str()));
        }

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
                match found.get(&entry.pid).filter(|path| !entry.foreign_logs.contains(*path)) {
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
                read_new_log_lines(os, entry, now_ms);
                if entry.crashed.is_none()
                    && os.window_titles(entry.pid).iter().any(|title| is_crash_dialog_title(title))
                {
                    entry.crashed = Some(crashed_drop(now_ms));
                }
            } else if !entry.exit_seen {
                entry.exit_seen = true;
                // O que ele escreveu nos últimos 2 s: fechar a janela escreve a
                // saída e termina antes da passada seguinte.
                read_new_log_lines(os, entry, now_ms);
                // Sem log não dá para saber se a pessoa fechou a janela: não chuta.
                let closed_alone = entry.log.is_some()
                    && entry.session.ended_without_leaving()
                    && !os.terminated_by_app(entry.pid);
                let closed_unhealthy = entry.session.current_drop().is_none() && os.closed_as_unhealthy(entry.pid);
                if entry.crashed.is_none() && (closed_alone || closed_unhealthy) {
                    entry.crashed = Some(crashed_drop(now_ms));
                }
            }
            entry.session.settle(now_ms);

            // O log é de outro launch (o casamento log → processo errou): ele
            // volta para quem é dele, e este cliente procura o seu de novo.
            // Nunca vira "entrou como outra conta".
            if let Some(other) = log_from_another_launch(
                &entry.browser_tracker_id,
                entry.session.browser_tracker_id(),
                entry.adopted,
            ) {
                if let Some(path) = entry.log.clone() {
                    self.handed_back.insert(other, path);
                }
                entry.forget_log();
                notices.push(HealthNotice::ForeignLog { user_id: client.user_id });
                continue;
            }

            // Entrou como outra conta: avisa uma vez (por conta vista no log).
            let wrong = wrong_account(client.user_id, entry.session.logged_user_id(), entry.adopted);
            if let Some(logged_user_id) = wrong.filter(|_| wrong != entry.wrong_account) {
                notices.push(HealthNotice::WrongAccount {
                    user_id: client.user_id,
                    logged_user_id,
                });
            }
            entry.wrong_account = wrong;

            // Verificação do Roblox na tela: avisa quando aparece e quando sai.
            let still_on_screen = entry.session.challenge_pending();
            let challenge = !entry.exit_seen && still_on_screen;
            if challenge != entry.challenge_reported {
                notices.push(HealthNotice::Challenge {
                    user_id: client.user_id,
                    pending: challenge,
                    resolved: !still_on_screen,
                });
                entry.challenge_reported = challenge;
            }

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
                        // A caixa de erro aberta não é "em jogo": sem isto a
                        // reconexão diria que a conta já voltou.
                        in_game: !entry.exit_seen && entry.crashed.is_none() && entry.session.in_game(),
                        wrong_account_user_id: entry.wrong_account,
                        server: server_view_of(entry.session.destination()),
                        challenge: !entry.exit_seen && entry.session.challenge_pending(),
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

/// O processo ainda existe? O tracker só esquece um cliente morto quando o
/// Watcher ou um launch passam: até lá ele não pode aparecer como aberto.
pub(crate) fn client_still_open(view: Option<&ClientHealthView>) -> bool {
    !view.is_some_and(|v| v.exited)
}

/// A saúde do cliente `pid` da conta (`None` se o monitor ainda não o viu).
pub(crate) fn client_health_of(user_id: i64, pid: u32) -> Option<ClientHealthView> {
    CLIENT_HEALTH_VIEWS
        .lock()
        .ok()?
        .get(&user_id)
        .filter(|view| view.pid == pid)
        .cloned()
}

/// Linha do Console para o cliente que entrou como outra conta.
fn wrong_account_console_line(logged_user_id: i64, queue_stopped: bool, queue_asked: bool) -> LogMsg {
    if queue_asked {
        log_msg!(
            "Joined as another Roblox account (user {{userId}}). The launch queue waits for your answer",
            userId = logged_user_id
        )
    } else if queue_stopped {
        log_msg!(
            "Joined as another Roblox account (user {{userId}}). The launch queue was paused",
            userId = logged_user_id
        )
    } else {
        log_msg!("Joined as another Roblox account (user {{userId}})", userId = logged_user_id)
    }
}

/// Linha do Console (traduzível, como as do Watcher) para uma queda. O código
/// do Roblox, quando há, vai como `{{code}}` (cada frase tem a variante com e
/// sem ele).
fn drop_console_line(drop: &ClientDrop) -> LogMsg {
    let code = drop.code;
    match drop.kind {
        DropKind::Disconnected => match (drop.reason, code) {
            (Some(DropReason::ConnectionLost), Some(code)) => {
                log_msg!("Dropped: lost the connection (code {{code}})", code = code)
            }
            (Some(DropReason::ConnectionLost), None) => log_msg!("Dropped: lost the connection"),
            (Some(DropReason::JoinedElsewhere), Some(code)) => {
                log_msg!("Dropped: the account joined somewhere else (code {{code}})", code = code)
            }
            (Some(DropReason::JoinedElsewhere), None) => log_msg!("Dropped: the account joined somewhere else"),
            (Some(DropReason::Idle), Some(code)) => {
                log_msg!("Dropped: it stayed idle for too long (code {{code}})", code = code)
            }
            (Some(DropReason::Idle), None) => log_msg!("Dropped: it stayed idle for too long"),
            (Some(DropReason::NoAccess), Some(code)) => {
                log_msg!("Dropped: Roblox won't let this account back in (code {{code}})", code = code)
            }
            (Some(DropReason::NoAccess), None) => log_msg!("Dropped: Roblox won't let this account back in"),
            (_, Some(code)) => log_msg!("Dropped (code {{code}})", code = code),
            (_, None) => log_msg!("Dropped"),
        },
        DropKind::Kicked => match (&drop.message, code) {
            // A mensagem da expulsão é do jogo (texto de fora): vai como veio.
            (Some(message), _) => log_msg!("Kicked: {{message}}", message = message.clone()),
            (None, Some(code)) => log_msg!("Kicked from the game (code {{code}})", code = code),
            (None, None) => log_msg!("Kicked from the game"),
        },
        DropKind::ServerShutdown => match code {
            Some(code) => log_msg!("The server shut down (code {{code}})", code = code),
            None => log_msg!("The server shut down"),
        },
        DropKind::Crashed => log_msg!("The client closed without leaving the game"),
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
    fn window_titles(&self, pid: u32) -> Vec<String> {
        platform::windows::window_titles_of(pid)
    }
    fn closed_as_unhealthy(&self, pid: u32) -> bool {
        platform::windows::was_closed_as_unhealthy(pid)
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

/// O Windows está quase sem memória agora? (livre, total) em MB do que faltou.
#[cfg(target_os = "windows")]
pub(crate) fn windows_memory_tight_now() -> Option<(u64, u64)> {
    let (physical, commit) = platform::windows::system_memory_detail()?;
    tight_memory(physical, commit)
}

/// O aperto de memória da passada de agora e da anterior. Quando o monitor vê
/// o cliente morto a memória dele já voltou: o que explica a queda é o retrato
/// de antes.
#[cfg(target_os = "windows")]
static RECENT_TIGHT_MEMORY: LazyLock<Mutex<[Option<(u64, u64)>; 2]>> = LazyLock::new(|| Mutex::new([None, None]));

#[cfg(target_os = "windows")]
fn remember_tight_memory() {
    let now = windows_memory_tight_now();
    if let Ok(mut recent) = RECENT_TIGHT_MEMORY.lock() {
        recent[1] = recent[0];
        recent[0] = now;
    }
}

#[cfg(target_os = "windows")]
fn recent_tight_memory() -> Option<(u64, u64)> {
    let recent = RECENT_TIGHT_MEMORY.lock().ok()?;
    recent[1].or(recent[0])
}

/// Uma passada de verdade: lê o tracker, roda o monitor e publica o retrato.
#[cfg(target_os = "windows")]
fn run_client_health_tick(app: &tauri::AppHandle) -> Vec<HealthNotice> {
    let tracked_processes = platform::windows::tracker().get_all();
    if !tracked_processes.is_empty() {
        remember_tight_memory();
    }
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
            browser_tracker_id: p.browser_tracker_id.clone(),
        })
        .collect();
    let now_ms = chrono::Utc::now().timestamp_millis();
    let Ok(mut monitor) = CLIENT_HEALTH_MONITOR.lock() else {
        return Vec::new();
    };
    let notices = monitor.tick(&WindowsClientHealthOs, &tracked, now_ms);
    let mut fresh = monitor.views();
    if fresh.values().any(|view| view.server.is_some()) {
        let template = server_region_template(app.state::<SettingsStore>().inner());
        if let Ok(lookups) = SERVER_REGION_LOOKUPS.lock() {
            apply_server_regions(&mut fresh, &lookups, &template);
        }
    }
    if let Ok(mut views) = CLIENT_HEALTH_VIEWS.lock() {
        *views = fresh;
    }
    let snapshots = monitor.session_snapshots();
    drop(monitor);
    // Histórico de sessões (ideia 6): grava cada mudança na hora.
    record_session_history(app, &snapshots, now_ms);
    notices
}

/// Região de cada IP de servidor visto nos logs (Sessão → servidor).
#[cfg(target_os = "windows")]
static SERVER_REGION_LOOKUPS: LazyLock<Mutex<HashMap<String, ServerRegionLookup>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Pede a região dos IPs de servidor novos, em segundo plano: a geolocalização
/// (com cache em memória e disco, `lookup_ip_region`) não segura o monitor. Só
/// IP público; nenhuma chamada à API do Roblox. Quem mostra é a passada
/// seguinte (`apply_server_regions`).
#[cfg(target_os = "windows")]
fn server_regions_after_health_tick() {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let views = match CLIENT_HEALTH_VIEWS.lock() {
        Ok(views) => views.clone(),
        Err(_) => return,
    };
    let wanted = {
        let Ok(mut lookups) = SERVER_REGION_LOOKUPS.lock() else {
            return;
        };
        let wanted: Vec<String> = server_addresses_to_locate(&views, &lookups, now_ms)
            .into_iter()
            .take(SERVER_REGION_LOOKUPS_PER_TICK)
            .collect();
        for address in &wanted {
            lookups.insert(address.clone(), ServerRegionLookup::Pending { since_ms: now_ms });
        }
        wanted
    };
    for address in wanted {
        tauri::async_runtime::spawn(async move {
            let region = api::roblox::lookup_ip_region(&address).await;
            let outcome = server_region_outcome(region, chrono::Utc::now().timestamp_millis());
            if let Ok(mut lookups) = SERVER_REGION_LOOKUPS.lock() {
                lookups.insert(address, outcome);
            }
        });
    }
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
            // Região do servidor de cada cliente (Sessão), pelo IP do log.
            server_regions_after_health_tick();
            // Aviso no Discord (desligado por padrão; só põe na fila).
            webhook_after_health_tick(&app, &notices);
            // Reconexão automática da conta que caiu (commands/reconnect.rs).
            reconnect_after_health_tick(&app, &notices);
            // Conta cujo cliente fechou no meio do jogo (commands/closed_clients.rs).
            closed_clients_after_health_tick(&app);
            // PC acordado durante Modo AFK / Auto Rejoin / reconexão (keep_awake.rs).
            keep_awake_tick(&app);
            // Teto de memória dos clientes do app (commands/memory_ceiling.rs).
            memory_ceiling_tick(&app).await;
            // Multi Roblox também para quem abre pelo site ou pelo navegador.
            if app.state::<SettingsStore>().get_bool("General", "EnableMultiRbx") {
                let now_ms = chrono::Utc::now().timestamp_millis();
                let _ = tokio::task::spawn_blocking(move || platform::windows::keep_roblox_clients_unlocked(now_ms)).await;
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
            // Fechou sem explicação com o Windows sem memória: diz à pessoa.
            if drop.kind == DropKind::Crashed {
                if let Some((free, total)) = recent_tight_memory() {
                    emit_launch_log(
                        app,
                        *user_id,
                        "warn",
                        "client",
                        log_msg!(
                            "Windows was almost out of memory when this client closed ({{free}} MB free of {{total}} MB). Open fewer clients or increase the page file",
                            free = free,
                            total = total
                        ),
                    );
                }
            }
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
                log_msg!("Not responding for 30 s")
            } else {
                log_msg!("Responding again")
            };
            emit_launch_log(app, *user_id, if *not_responding { "warn" } else { "info" }, "client", line);
        }
        HealthNotice::WrongAccount {
            user_id,
            logged_user_id,
        } => {
            // Quem ainda espera a vez: pergunta (padrão), segue ou para, como a
            // pessoa escolheu em `General.WrongAccountQueue`. Ninguém é fechado
            // — nem este cliente, que pode ser a conta principal da pessoa.
            let choice = launch_queue_has_queued().then(|| {
                WrongAccountQueue::from_setting(
                    &app.state::<SettingsStore>().get_string("General", "WrongAccountQueue"),
                )
            });
            let (queue_stopped, queue_asked) = match choice {
                Some(WrongAccountQueue::Stop) => (launch_queue_cancel_all_queued(app) > 0, false),
                Some(WrongAccountQueue::Ask) => {
                    hold_launch_queue();
                    (false, true)
                }
                Some(WrongAccountQueue::Continue) | None => (false, false),
            };
            let _ = app.emit(
                "roblox-client-wrong-account",
                serde_json::json!({
                    "userId": user_id,
                    "loggedUserId": logged_user_id,
                    "queueStopped": queue_stopped,
                    "queueAsked": queue_asked,
                }),
            );
            emit_launch_log(
                app,
                *user_id,
                "warn",
                "client",
                wrong_account_console_line(*logged_user_id, queue_stopped, queue_asked),
            );
        }
        HealthNotice::ForeignLog { user_id } => {
            // Fica no Console: é o rastro de que o casamento log → processo
            // errou (relato de 31/10/2026), sem alarme para a pessoa.
            emit_launch_log(
                app,
                *user_id,
                "info",
                "client",
                log_msg!("The Roblox log read for this client was another account's; looking for its own"),
            );
        }
        HealthNotice::Challenge { user_id, pending, resolved } => {
            // A conta guarda a marca para a próxima vez que o app abrir.
            if let Some(mark) = human_check_mark(*pending, *resolved) {
                let store = app.state::<AccountStore>();
                if store.set_human_check(*user_id, mark).unwrap_or(false) {
                    let at = store
                        .get_all()
                        .ok()
                        .and_then(|all| all.into_iter().find(|a| a.user_id == *user_id))
                        .and_then(|a| a.human_check_at);
                    let _ = app.emit("account-human-check", serde_json::json!({ "userId": user_id, "at": at }));
                }
            }
            let _ = app.emit(
                "roblox-client-health",
                serde_json::json!({
                    "userId": user_id,
                    "challenge": pending,
                }),
            );
            let line = if *pending {
                log_msg!("Roblox asked for a verification (confirm you are human): solve it in the game window")
            } else {
                log_msg!("The Roblox verification left the screen")
            };
            emit_launch_log(app, *user_id, if *pending { "warn" } else { "info" }, "client", line);
        }
        HealthNotice::Recovered { user_id } => {
            let _ = app.emit(
                "roblox-client-health",
                serde_json::json!({
                    "userId": user_id,
                    "drop": serde_json::Value::Null,
                }),
            );
            emit_launch_log(app, *user_id, "info", "client", "Joined a game again");
        }
    }
}

/// O que fazer com a marca de verificação humana da conta a cada aviso:
/// `Some(true)` marca (apareceu), `Some(false)` limpa (resolvida, ou entrou no
/// jogo), `None` deixa como está (o cliente fechou com ela na tela — a próxima
/// abertura vai pedir de novo).
fn human_check_mark(pending: bool, resolved: bool) -> Option<bool> {
    if pending {
        Some(true)
    } else if resolved {
        Some(false)
    } else {
        None
    }
}
