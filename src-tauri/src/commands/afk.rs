// AFK mode — manda uma tecla, de tempo em tempo, para a janela de cada conta
// que o usuário colocou no modo. Ver docs/features/afk-mode.md.
//
// Não é detecção de interação: a API do Windows para isso responde pela sessão
// inteira do usuário, nunca por uma janela, então "só envia se aquela conta
// estiver parada" não é uma pergunta que este app possa responder. O que existe
// aqui é envio periódico, e só. (O nome dessa API está na lista de proibições do
// `afk_input_safety_tests`, no fim deste arquivo.)
//
// Duas regras mandam no desenho:
//
// 1. `SendInput` entrega na janela em **primeiro plano**. Para acertar o cliente
//    de uma conta, o ciclo traz aquela janela para frente, manda a tecla, passa
//    para a conta seguinte e só no fim devolve o foco para onde estava. O foco
//    fica fora da janela do usuário o ciclo inteiro — ~0,44 s por conta —, e a
//    tela diz isso com esses números.
// 2. O módulo só **envia** entrada. Ler teclado do usuário é proibido — a trava
//    é o `afk_input_safety_tests`, no fim deste arquivo.

/// Lista **fechada** de teclas que o AFK mode pode enviar, com a virtual key de
/// cada uma. O usuário escolhe de dentro dela; não existe campo para digitar
/// tecla arbitrária, e o backend recusa qualquer nome que não esteja aqui.
///
/// São teclas de movimento e de ação comuns em jogo do Roblox. Ficaram fora, de
/// propósito, as que fazem outra coisa na tela: Enter (abre o chat), Tab (troca
/// de janela), Escape (menu do Roblox) e F4 (fecha o cliente junto com Alt).
const AFK_KEYS: &[(&str, u16)] = &[
    ("Space", 0x20),
    ("W", 0x57),
    ("A", 0x41),
    ("S", 0x53),
    ("D", 0x44),
    ("E", 0x45),
    ("F", 0x46),
    ("R", 0x52),
    ("Q", 0x51),
    ("1", 0x31),
    ("2", 0x32),
    ("3", 0x33),
    ("4", 0x34),
    ("5", 0x35),
];

/// Os nomes da lista fechada, na ordem em que a tela os oferece.
fn afk_key_names() -> Vec<String> {
    AFK_KEYS.iter().map(|(name, _)| (*name).to_string()).collect()
}

/// Virtual key de uma tecla da lista. `None` para qualquer outro nome — é este
/// `None` que impede tecla de fora de chegar ao `SendInput`.
fn afk_virtual_key(key: &str) -> Option<u16> {
    AFK_KEYS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, vk)| *vk)
}

/// Intervalo entre envios da mesma conta, em minutos. O teto de 120 é o mesmo
/// do diálogo; o piso de 1 evita sessão que rouba o foco sem parar.
fn clamp_afk_interval_minutes(minutes: i64) -> u64 {
    minutes.clamp(1, 120) as u64
}

/// O que o AFK mode manda para cada conta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AfkMode {
    /// Toca uma tecla da lista fechada.
    Key,
    /// Clique esquerdo num ponto relativo da janela da conta. Existe para quem
    /// quer o personagem **parado**: toda tecla da lista mexe nele.
    Click,
}

impl AfkMode {
    /// Qualquer valor desconhecido vira `Key`: é o modo que já existia.
    fn parse(raw: &str) -> AfkMode {
        if raw.trim().eq_ignore_ascii_case("click") {
            AfkMode::Click
        } else {
            AfkMode::Key
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            AfkMode::Key => "key",
            AfkMode::Click => "click",
        }
    }
}

/// Ponto do clique em porcentagem da área interna da janela (0–100 nos dois
/// eixos). Relativo de propósito: cai no mesmo lugar com a janela pequena,
/// grande ou maximizada, e nenhuma janela precisa ser redimensionada.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AfkPoint {
    x_pct: f64,
    y_pct: f64,
}

const AFK_DEFAULT_POINT: AfkPoint = AfkPoint {
    x_pct: 50.0,
    y_pct: 50.0,
};

/// Porcentagem travada em 0–100; valor que não é número vira o meio.
fn clamp_afk_percent(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 100.0)
    } else {
        50.0
    }
}

impl AfkPoint {
    fn clamped(x: f64, y: f64) -> Self {
        Self {
            x_pct: clamp_afk_percent(x),
            y_pct: clamp_afk_percent(y),
        }
    }
}

/// Área interna (cliente) de uma janela, em coordenadas de tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AfkClientRect {
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

/// Pixel de tela do ponto, **sempre dentro** da área interna: 0% é a primeira
/// coluna/linha e 100% a última, nunca a borda de fora.
fn afk_point_to_pixel(rect: AfkClientRect, point: AfkPoint) -> Option<(i32, i32)> {
    if rect.width <= 0 || rect.height <= 0 {
        return None;
    }
    let p = AfkPoint::clamped(point.x_pct, point.y_pct);
    let dx = ((p.x_pct / 100.0) * f64::from(rect.width - 1)).round() as i32;
    let dy = ((p.y_pct / 100.0) * f64::from(rect.height - 1)).round() as i32;
    Some((rect.left + dx, rect.top + dy))
}

/// O inverso, para o Marcar. `None` quando o cursor não está dentro da área
/// interna (em cima da borda, da barra de título ou fora da janela).
fn afk_pixel_to_point(rect: AfkClientRect, x: i32, y: i32) -> Option<AfkPoint> {
    if rect.width <= 0 || rect.height <= 0 {
        return None;
    }
    let (dx, dy) = (x - rect.left, y - rect.top);
    if dx < 0 || dy < 0 || dx >= rect.width || dy >= rect.height {
        return None;
    }
    let span_x = f64::from((rect.width - 1).max(1));
    let span_y = f64::from((rect.height - 1).max(1));
    // Duas casas: o bastante para a ida e volta cair no mesmo pixel até em 4K.
    let round2 = |v: f64| (v * 100.0).round() / 100.0;
    Some(AfkPoint::clamped(
        round2(f64::from(dx) / span_x * 100.0),
        round2(f64::from(dy) / span_y * 100.0),
    ))
}

/// Ponto próprio da conta, dos campos `AfkClickX`/`AfkClickY`. Só vale com os
/// dois números: um sem o outro cai no padrão.
fn afk_point_from_fields(fields: &HashMap<String, String>) -> Option<AfkPoint> {
    let x = fields.get("AfkClickX")?.trim().parse::<f64>().ok()?;
    let y = fields.get("AfkClickY")?.trim().parse::<f64>().ok()?;
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    Some(AfkPoint::clamped(x, y))
}

/// O ponto de cada alvo: o próprio, se a conta tiver, senão o padrão.
fn afk_points_for_targets(
    targets: &[i64],
    default: AfkPoint,
    overrides: &HashMap<i64, AfkPoint>,
) -> HashMap<i64, AfkPoint> {
    targets
        .iter()
        .map(|uid| (*uid, overrides.get(uid).copied().unwrap_or(default)))
        .collect()
}

/// Resultado do Marcar: de que conta é a janela e onde, em porcentagem.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AfkCapturedPoint {
    user_id: i64,
    x_pct: f64,
    y_pct: f64,
}

/// O Marcar a partir das partes, para o teste não precisar de janela. `tracked`
/// é `(conta, PID)` dos clientes abertos por este app. O erro é um **código**; a
/// tela escreve a frase.
fn afk_capture_from_parts(
    cursor: (i32, i32),
    window_pid: Option<u32>,
    tracked: &[(i64, u32)],
    rect: Option<AfkClientRect>,
) -> Result<AfkCapturedPoint, String> {
    let pid = window_pid.ok_or("noWindow")?;
    let user_id = tracked
        .iter()
        .find(|(_, p)| *p == pid)
        .map(|(uid, _)| *uid)
        .ok_or("notAnAccountWindow")?;
    let rect = rect.ok_or("outsideGameArea")?;
    let point = afk_pixel_to_point(rect, cursor.0, cursor.1).ok_or("outsideGameArea")?;
    Ok(AfkCapturedPoint {
        user_id,
        x_pct: point.x_pct,
        y_pct: point.y_pct,
    })
}

/// Um passo da receita do clique. `MoveTo` é posição de tela absoluta, `Nudge`
/// é movimento relativo em pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AfkMouseStep {
    MoveTo(i32, i32),
    Nudge(i32, i32),
    Wait(u64),
    Press,
    Release,
}

/// Tamanho do tremor, em pixels.
const AFK_CLICK_JITTER_PX: i32 = 2;
/// Espera depois de chegar ao ponto, para o jogo assentar o mouse lá.
const AFK_CLICK_SETTLE_MS: u64 = 500;
/// Espera entre o clique de foco e o clique de verdade.
const AFK_CLICK_FOCUS_SETTLE_MS: u64 = 200;
/// Pausa de ~1 quadro entre a posição final e o botão descer.
const AFK_CLICK_FINAL_PAUSE_MS: u64 = 12;

/// A receita do clique no ponto `point` da área interna `rect`.
///
/// O Roblox lê o mouse pelo caminho de entrada crua do sistema: pôr o cursor no
/// lugar sem gerar movimento não chega ao jogo, e um clique parado cai onde o
/// jogo acha que o mouse estava. A receita é a que funcionou no bot de Robeats
/// do dono:
///
/// 1. chega ao ponto por movimento de entrada, com um micro-desvio de 1 px e
///    volta, e espera o jogo assentar;
/// 2. treme ±2 px em movimento relativo e reafirma a posição exata;
/// 3. clica — e esse primeiro clique pode só focar o jogo;
/// 4. espera, treme de novo e dá o clique de verdade.
///
/// Todo desvio vai para **dentro** da janela, inclusive nas bordas. `None` para
/// janela sem área.
fn afk_click_plan(rect: AfkClientRect, point: AfkPoint, hold_ms: u64) -> Option<Vec<AfkMouseStep>> {
    use AfkMouseStep::*;
    let (x, y) = afk_point_to_pixel(rect, point)?;
    // Desvio para o lado de dentro: na última coluna, para a esquerda.
    let inward = |at: i32, start: i32, len: i32, step: i32| {
        if at + step <= start + len - 1 {
            step
        } else {
            -step
        }
    };
    let micro = inward(x, rect.left, rect.width, 1);
    let jitter = inward(x, rect.left, rect.width, AFK_CLICK_JITTER_PX);
    // Janela de 1 px de largura não tem para onde tremer.
    let (micro, jitter) = if rect.width < 3 { (0, 0) } else { (micro, jitter) };

    let click = |steps: &mut Vec<AfkMouseStep>, settle: bool| {
        steps.extend([MoveTo(x, y), MoveTo(x + micro, y), MoveTo(x, y)]);
        if settle {
            steps.push(Wait(AFK_CLICK_SETTLE_MS));
        }
        steps.extend([
            Nudge(jitter, 0),
            Nudge(-jitter, 0),
            MoveTo(x, y),
            Wait(AFK_CLICK_FINAL_PAUSE_MS),
            Press,
            Wait(hold_ms),
            Release,
        ]);
    };

    let mut steps = Vec::new();
    click(&mut steps, true);
    steps.push(Wait(AFK_CLICK_FOCUS_SETTLE_MS));
    click(&mut steps, false);
    Some(steps)
}

/// Pixel de tela na escala do envio absoluto (0..65535 sobre a área de trabalho
/// virtual, todos os monitores). Fora da área é travado.
fn afk_absolute_input(x: i32, y: i32, desktop: AfkClientRect) -> (i32, i32) {
    let scale = |at: i32, start: i32, len: i32| {
        let span = f64::from((len - 1).max(1));
        ((f64::from(at - start) * 65535.0 / span).round() as i32).clamp(0, 65535)
    };
    (scale(x, desktop.left, desktop.width), scale(y, desktop.top, desktop.height))
}

/// O que o ciclo faz em cada conta. No clique o ponto já vem resolvido por
/// conta (o próprio ou o padrão), então o corpo bloqueante não lê store nenhum.
#[derive(Debug, Clone)]
enum AfkCycleAction {
    Key(String),
    Click(HashMap<i64, AfkPoint>),
}

/// Por que um start não pode acontecer. No modo tecla, sem tecla escolhida o
/// modo **não liga**: inventar uma tecla padrão seria mexer no personagem sem o
/// usuário pedir. O modo clique não usa tecla.
fn validate_afk_start(mode: AfkMode, key: &str, user_ids: &[i64]) -> Result<(), String> {
    if mode == AfkMode::Key && afk_virtual_key(key).is_none() {
        return Err("Choose one of the AFK mode keys before starting".into());
    }
    if user_ids.is_empty() {
        return Err("Put at least one account in AFK mode before starting".into());
    }
    Ok(())
}

/// Por que uma conta não recebeu a tecla. O código vai para a tela, que escreve
/// a frase traduzida — a mensagem em inglês fica para log e para caso novo.
#[derive(Debug, Clone, PartialEq, Eq)]
enum AfkSendError {
    /// A conta não tem janela de cliente de pé.
    NoWindow,
    /// O Windows recusou trazer a janela para o primeiro plano. **Nada foi
    /// enviado**: a tecla cairia na janela que o usuário está usando.
    FocusDenied,
    /// O `SendInput` foi recusado (ou o "solta a tecla" não passou).
    KeyRefused,
    /// O clique foi recusado (ou o "solta o botão" não passou).
    ClickRefused,
    /// Falha inesperada do ciclo.
    Internal(String),
}

impl AfkSendError {
    fn code(&self) -> &'static str {
        match self {
            AfkSendError::NoWindow => "noWindow",
            AfkSendError::FocusDenied => "focusDenied",
            AfkSendError::KeyRefused => "keyRefused",
            AfkSendError::ClickRefused => "clickRefused",
            AfkSendError::Internal(_) => "internal",
        }
    }

    fn message(&self) -> String {
        match self {
            AfkSendError::NoWindow => "No Roblox window for this account".into(),
            AfkSendError::FocusDenied => {
                "Windows did not bring this account's Roblox window to the front, so nothing was sent"
                    .into()
            }
            AfkSendError::KeyRefused => "Windows refused the synthetic key".into(),
            AfkSendError::ClickRefused => "Windows refused the synthetic click".into(),
            AfkSendError::Internal(message) => message.clone(),
        }
    }
}

/// Estado de uma conta que **está** no AFK mode.
#[derive(Debug, Clone)]
struct AfkAccountRuntime {
    user_id: i64,
    /// Último envio (ou a entrada no modo, enquanto não houve envio). É daqui
    /// que sai o "está na hora desta conta?".
    last_send_at_ms: i64,
    sends: u64,
    last_error: Option<AfkSendError>,
}

impl AfkAccountRuntime {
    /// Conta entrando no modo agora: o relógio dela começa aqui, então o
    /// primeiro envio só sai depois de um intervalo inteiro.
    fn joined(user_id: i64, at_ms: i64) -> Self {
        Self {
            user_id,
            last_send_at_ms: at_ms,
            sends: 0,
            last_error: None,
        }
    }
}

/// Está na hora desta conta? Só o tempo decorrido desde o último envio decide.
/// Relógio do sistema andando para trás dá diferença negativa — e aí não é hora.
fn afk_is_due(last_send_at_ms: i64, interval_ms: i64, now_ms: i64) -> bool {
    interval_ms > 0 && now_ms.saturating_sub(last_send_at_ms) >= interval_ms
}

fn afk_next_send_at_ms(last_send_at_ms: i64, interval_ms: i64) -> i64 {
    last_send_at_ms.saturating_add(interval_ms)
}

/// As contas que o ciclo deve visitar agora, em ordem estável de user id.
///
/// O mapa é o da sessão: conta que **não** está no AFK mode não tem entrada
/// nele e portanto nunca vira alvo, por mais tempo que passe.
fn afk_due_targets(
    accounts: &HashMap<i64, AfkAccountRuntime>,
    interval_ms: i64,
    now_ms: i64,
) -> Vec<i64> {
    let mut due: Vec<i64> = accounts
        .values()
        .filter(|entry| afk_is_due(entry.last_send_at_ms, interval_ms, now_ms))
        .map(|entry| entry.user_id)
        .collect();
    due.sort_unstable();
    due
}

/// O que o ciclo faz com o próximo alvo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AfkCycleStep {
    /// Sessão parando: o ciclo em andamento é abandonado onde está, sem enviar
    /// o que faltava.
    Abort,
    /// A conta não tem janela de Roblox de pé: pula sem enviar nada e sem mexer
    /// em janela de ninguém.
    Skip,
    Send,
}

fn afk_cycle_step(stopping: bool, window_alive: bool) -> AfkCycleStep {
    if stopping {
        AfkCycleStep::Abort
    } else if !window_alive {
        AfkCycleStep::Skip
    } else {
        AfkCycleStep::Send
    }
}

/// A janela do alvo chegou de fato ao primeiro plano?
///
/// `SendInput` entrega na janela em **primeiro plano**, e o Windows recusa
/// `SetForegroundWindow` de processo que não está em primeiro plano nem recebeu o
/// último evento de entrada — que é o caso normal do AFK mode, com o app em
/// segundo plano. Sem esta conferência a tecla ia, **todo ciclo**, para a janela
/// em que o usuário está digitando, e o ciclo se declarava bem-sucedido.
///
/// `foreground` e `target` são handles de janela em forma de número; `0` é
/// "nenhuma janela", e alvo nulo nunca está pronto.
fn afk_window_is_ready(focus_requested: bool, foreground: isize, target: isize) -> bool {
    focus_requested && target != 0 && foreground == target
}

/// A janela volta a ser minimizada? Trazer para frente desminimiza (o
/// `focus_window` faz `SW_RESTORE` em janela minimizada — e só nela: maximizada
/// continua maximizada); quem trabalha com os clientes minimizados
/// não pediu para vê-los na tela. Só vale para janela que o ciclo mexeu, e só se
/// o usuário a tinha minimizado.
fn afk_should_reminimize(was_minimized: bool, focus_attempted: bool) -> bool {
    was_minimized && focus_attempted
}

/// As contas de um envio manual: **interseção** com quem está no AFK mode, na
/// ordem que o usuário pediu.
///
/// Sem isso o botão "enviar agora" puxava para frente e teclava cliente de conta
/// que nunca entrou no modo — o único ponto que contrariava a regra de não mexer
/// em cliente de conta fora do modo.
fn afk_manual_targets(requested: &[i64], accounts: &HashMap<i64, AfkAccountRuntime>) -> Vec<i64> {
    requested
        .iter()
        .copied()
        .filter(|user_id| accounts.contains_key(user_id))
        .collect()
}

/// Devolver o foco é coisa de ciclo que terminou: quem está parando não mexe em
/// foco nenhum, e ciclo que não roubou o foco não tem o que devolver.
fn afk_should_restore_focus(stopping: bool, focus_taken: bool) -> bool {
    !stopping && focus_taken
}

#[derive(Debug, Clone, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct AfkAccountStatus {
    user_id: i64,
    last_send_at_ms: i64,
    next_send_at_ms: i64,
    sends: u64,
    last_error: Option<String>,
    /// `noWindow`, `focusDenied`, `keyRefused`, `clickRefused` ou `internal` — a
    /// tela escolhe a frase traduzida por aqui, em vez de casar texto em inglês.
    last_error_code: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AfkStatusPayload {
    active: bool,
    started_at_ms: Option<i64>,
    interval_minutes: u64,
    key: String,
    /// `key` ou `click`.
    mode: String,
    /// Ponto padrão do modo clique, em porcentagem.
    click_x: f64,
    click_y: f64,
    accounts: Vec<AfkAccountStatus>,
}

impl Default for AfkStatusPayload {
    /// Sem sessão: o que a tela mostra antes de ligar.
    fn default() -> Self {
        afk_status_from_parts(None, 0, "", AfkMode::Key, AFK_DEFAULT_POINT, &HashMap::new())
    }
}

/// Tempo que a janela fica em frente antes da tecla sair: sem essa folga o
/// `SendInput` chega antes de o Roblox virar a janela em foco e a tecla cai na
/// janela anterior.
#[cfg(target_os = "windows")]
const AFK_FOCUS_SETTLE_MS: u64 = 150;
/// Quanto a tecla fica pressionada. Toque curto: o objetivo é o jogo registrar
/// entrada, não andar com o personagem.
#[cfg(target_os = "windows")]
const AFK_KEY_HOLD_MS: u64 = 40;
/// Respiro entre duas contas do mesmo ciclo.
///
/// Folga + tecla + respiro dão ~0,44 s por conta, e o foco só volta para a
/// janela do usuário no fim do ciclo: é o "cerca de meio segundo cada" e o "uns
/// 4 segundos com 10 contas" do `AfkDialog`
/// (`a_cycle_keeps_the_focus_about_half_a_second_per_account`).
#[cfg(target_os = "windows")]
const AFK_BETWEEN_WINDOWS_MS: u64 = 250;
/// De quanto em quanto tempo o laço olha o relógio das contas.
#[cfg(target_os = "windows")]
const AFK_TICK_MS: i64 = 1_000;

#[cfg(target_os = "windows")]
#[derive(Debug, Clone)]
struct AfkConfig {
    interval_minutes: u64,
    key: String,
    mode: AfkMode,
    /// Ponto de quem não tem ponto próprio (modo clique).
    default_point: AfkPoint,
}

#[cfg(target_os = "windows")]
#[derive(Clone)]
struct AfkSession {
    id: u64,
    stop_flag: Arc<AtomicBool>,
    /// Ligado pelo laço quando ele realmente saiu. Quem manda parar olha isto
    /// antes de esperar: sem ele, laço que terminou **antes** de o `notified()`
    /// ser registrado fazia o parar esperar os 2 s inteiros.
    stopped: Arc<AtomicBool>,
    stopped_notify: Arc<tokio::sync::Notify>,
    started_at_ms: i64,
    config: Arc<Mutex<AfkConfig>>,
    accounts: Arc<Mutex<HashMap<i64, AfkAccountRuntime>>>,
}

#[cfg(target_os = "windows")]
struct AfkManager {
    session: Mutex<Option<AfkSession>>,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    next_id: AtomicU64,
}

#[cfg(target_os = "windows")]
impl AfkManager {
    fn new() -> Self {
        Self {
            session: Mutex::new(None),
            task: Mutex::new(None),
            next_id: AtomicU64::new(1),
        }
    }

    fn next_session_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    fn get_session(&self) -> Option<AfkSession> {
        self.session.lock().ok().and_then(|s| s.as_ref().cloned())
    }

    fn replace_session(&self, session: Option<AfkSession>) {
        if let Ok(mut guard) = self.session.lock() {
            *guard = session;
        }
    }

    fn set_task(&self, handle: tauri::async_runtime::JoinHandle<()>) {
        if let Ok(mut guard) = self.task.lock() {
            if let Some(previous) = guard.replace(handle) {
                previous.abort();
            }
        }
    }

    fn abort_task(&self) {
        if let Ok(mut guard) = self.task.lock() {
            if let Some(previous) = guard.take() {
                previous.abort();
            }
        }
    }
}

#[cfg(target_os = "windows")]
static AFK_MANAGER: LazyLock<AfkManager> = LazyLock::new(AfkManager::new);

/// Um ciclo por vez. O laço da sessão e o "enviar agora" do usuário disputam as
/// mesmas janelas e o mesmo foco: dois ciclos ao mesmo tempo mandariam tecla
/// para a janela que o outro acabou de trazer para frente.
#[cfg(target_os = "windows")]
static AFK_CYCLE_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

/// O mesmo "um ciclo por vez", mas do lado bloqueante: `abort` numa task de
/// `spawn_blocking` não para a closure que já começou, então o lock assíncrono
/// sozinho não impede dois corpos de ciclo se sobreporem.
#[cfg(target_os = "windows")]
static AFK_CYCLE_SEQ: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Sessão nova. Cada conta entra com o relógio marcando `started_at_ms`, e é o
/// que faz a tela **já saber** quando é o primeiro envio (`started_at` +
/// intervalo) antes de qualquer tecla sair: sem isso, quem liga o modo passa o
/// intervalo inteiro olhando um "--" sem saber se funcionou.
#[cfg(target_os = "windows")]
fn new_afk_session(
    id: u64,
    started_at_ms: i64,
    config: AfkConfig,
    user_ids: &[i64],
) -> AfkSession {
    let accounts: HashMap<i64, AfkAccountRuntime> = user_ids
        .iter()
        .map(|uid| (*uid, AfkAccountRuntime::joined(*uid, started_at_ms)))
        .collect();
    AfkSession {
        id,
        stop_flag: Arc::new(AtomicBool::new(false)),
        stopped: Arc::new(AtomicBool::new(false)),
        stopped_notify: Arc::new(tokio::sync::Notify::new()),
        started_at_ms,
        config: Arc::new(Mutex::new(config)),
        accounts: Arc::new(Mutex::new(accounts)),
    }
}

/// O status que a tela recebe, montado a partir das partes — separado de
/// `afk_status_from` para o teste poder montar um sem sessão viva.
fn afk_status_from_parts(
    started_at_ms: Option<i64>,
    interval_minutes: u64,
    key: &str,
    mode: AfkMode,
    default_point: AfkPoint,
    accounts: &HashMap<i64, AfkAccountRuntime>,
) -> AfkStatusPayload {
    let interval_ms = (interval_minutes as i64).saturating_mul(60_000);
    let mut rows: Vec<AfkAccountStatus> = accounts
        .values()
        .map(|entry| AfkAccountStatus {
            user_id: entry.user_id,
            last_send_at_ms: entry.last_send_at_ms,
            next_send_at_ms: afk_next_send_at_ms(entry.last_send_at_ms, interval_ms),
            sends: entry.sends,
            last_error: entry.last_error.as_ref().map(|e| e.message()),
            last_error_code: entry.last_error.as_ref().map(|e| e.code().to_string()),
        })
        .collect();
    rows.sort_by_key(|a| a.user_id);

    AfkStatusPayload {
        active: started_at_ms.is_some(),
        started_at_ms,
        interval_minutes,
        key: key.to_string(),
        mode: mode.as_str().to_string(),
        click_x: default_point.x_pct,
        click_y: default_point.y_pct,
        accounts: rows,
    }
}

#[cfg(target_os = "windows")]
fn afk_status_from(session: &AfkSession) -> AfkStatusPayload {
    let config = session
        .config
        .lock()
        .map(|c| c.clone())
        .unwrap_or_else(|_| AfkConfig {
            interval_minutes: 0,
            key: String::new(),
            mode: AfkMode::Key,
            default_point: AFK_DEFAULT_POINT,
        });
    let accounts = session
        .accounts
        .lock()
        .map(|map| map.clone())
        .unwrap_or_default();

    afk_status_from_parts(
        Some(session.started_at_ms),
        config.interval_minutes,
        &config.key,
        config.mode,
        config.default_point,
        &accounts,
    )
}

#[cfg(target_os = "windows")]
fn current_afk_status() -> AfkStatusPayload {
    match AFK_MANAGER.get_session() {
        Some(session) => afk_status_from(&session),
        None => AfkStatusPayload::default(),
    }
}

#[cfg(not(target_os = "windows"))]
fn current_afk_status() -> AfkStatusPayload {
    AfkStatusPayload::default()
}

#[cfg(target_os = "windows")]
fn emit_afk_status(app: &tauri::AppHandle) {
    let _ = app.emit("afk-status", current_afk_status());
}

/// Ciclo concluído, com quantas contas receberam a tecla. É o gancho do aviso
/// sonoro opcional da tela: o usuário está usando o PC e o piscar do foco fica
/// sem explicação se nada avisa que foi o app.
#[cfg(target_os = "windows")]
fn emit_afk_cycle(app: &tauri::AppHandle, sent: u32) {
    let _ = app.emit("afk-cycle", serde_json::json!({ "sent": sent }));
}

/// Uma passada de envio pelas contas vencidas. Devolve, por conta, o erro que
/// houve (ou `None` quando a tecla saiu).
///
/// Só toca em janela de conta que está no AFK mode: o alvo vem do tracker, que
/// sabe qual PID é de qual conta. Nada aqui fecha, mata ou minimiza janela.
#[cfg(target_os = "windows")]
fn run_afk_cycle_blocking(
    action: &AfkCycleAction,
    targets: &[i64],
    stop_flag: &AtomicBool,
) -> Result<Vec<(i64, Option<AfkSendError>)>, String> {
    use platform::windows;

    // Um ciclo por vez **de verdade**: `JoinHandle::abort` não interrompe uma
    // closure de `spawn_blocking` que já começou, então o lock assíncrono lá fora
    // pode ser liberado com este corpo ainda rodando.
    let _serial = AFK_CYCLE_SEQ
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if let AfkCycleAction::Key(key) = action {
        if afk_virtual_key(key).is_none() {
            return Err(format!("Key not allowed in AFK mode: {}", key));
        }
    }

    let previous_foreground = windows::get_foreground_hwnd();
    // PID de cliente que já morreu pode ter sido reaproveitado pelo Windows por
    // outro programa qualquer: só vale PID que ainda é um Roblox.
    let alive: HashSet<u32> = windows::get_roblox_pids().into_iter().collect();
    let tracker = windows::tracker();

    let mut focus_taken = false;
    let mut outcome: Vec<(i64, Option<AfkSendError>)> = Vec::new();

    for &user_id in targets {
        let hwnd = tracker
            .get_pid(user_id)
            .filter(|pid| alive.contains(pid))
            .and_then(windows::find_main_window)
            .filter(|hwnd| windows::window_exists(*hwnd));

        match afk_cycle_step(stop_flag.load(Ordering::Relaxed), hwnd.is_some()) {
            AfkCycleStep::Abort => break,
            AfkCycleStep::Skip => {
                outcome.push((user_id, Some(AfkSendError::NoWindow)));
            }
            AfkCycleStep::Send => {
                let hwnd = match hwnd {
                    Some(hwnd) => hwnd,
                    None => continue,
                };
                // Quem trabalha com os clientes minimizados não pediu para
                // vê-los: o estado é devolvido depois do envio.
                let was_minimized = windows::window_is_minimized(hwnd);
                let requested = windows::focus_window(hwnd);
                focus_taken = true;
                std::thread::sleep(std::time::Duration::from_millis(AFK_FOCUS_SETTLE_MS));

                let ready = afk_window_is_ready(
                    requested,
                    windows::get_foreground_hwnd() as isize,
                    hwnd as isize,
                );

                let result = if !ready {
                    // O Windows não deixou a janela vir para frente. Tecla ou
                    // clique aqui cairiam na janela do usuário.
                    Some(AfkSendError::FocusDenied)
                } else {
                    match action {
                        AfkCycleAction::Key(key) => windows::tap_afk_key(key, AFK_KEY_HOLD_MS)
                            .err()
                            .map(|_| AfkSendError::KeyRefused),
                        AfkCycleAction::Click(points) => {
                            let point = points.get(&user_id).copied().unwrap_or(AFK_DEFAULT_POINT);
                            windows::click_afk_point(hwnd, point, AFK_KEY_HOLD_MS)
                                .err()
                                .map(|_| AfkSendError::ClickRefused)
                        }
                    }
                };

                if afk_should_reminimize(was_minimized, true) {
                    windows::minimize_window(hwnd);
                }
                outcome.push((user_id, result));
                std::thread::sleep(std::time::Duration::from_millis(AFK_BETWEEN_WINDOWS_MS));
            }
        }
    }

    if afk_should_restore_focus(stop_flag.load(Ordering::Relaxed), focus_taken)
        && windows::window_exists(previous_foreground)
    {
        // Só o primeiro plano volta: a janela do usuário não é restaurada nem
        // desmaximizada (o `focus_window` restauraria uma janela minimizada).
        windows::give_focus_back(previous_foreground);
    }

    Ok(outcome)
}

/// A ação do ciclo. O ponto próprio de cada conta é lido **agora**, dos campos
/// dela: mudar o ponto de uma conta vale no ciclo seguinte, sem religar o modo.
#[cfg(target_os = "windows")]
fn afk_cycle_action(app: &tauri::AppHandle, config: &AfkConfig, targets: &[i64]) -> AfkCycleAction {
    match config.mode {
        AfkMode::Key => AfkCycleAction::Key(config.key.clone()),
        AfkMode::Click => {
            let overrides: HashMap<i64, AfkPoint> = app
                .state::<AccountStore>()
                .get_all()
                .unwrap_or_default()
                .into_iter()
                .filter(|account| targets.contains(&account.user_id))
                .filter_map(|account| {
                    afk_point_from_fields(&account.fields).map(|point| (account.user_id, point))
                })
                .collect();
            AfkCycleAction::Click(afk_points_for_targets(
                targets,
                config.default_point,
                &overrides,
            ))
        }
    }
}

#[cfg(target_os = "windows")]
async fn run_afk_session(app: tauri::AppHandle, session: AfkSession) {
    loop {
        if session.stop_flag.load(Ordering::Relaxed) {
            break;
        }

        let Ok(config) = session.config.lock().map(|c| c.clone()) else {
            break;
        };
        let interval_ms = (config.interval_minutes as i64).saturating_mul(60_000);
        let Ok(targets) = session
            .accounts
            .lock()
            .map(|map| afk_due_targets(&map, interval_ms, now_ms()))
        else {
            break;
        };

        if !targets.is_empty() {
            let action = afk_cycle_action(&app, &config, &targets);
            let stop = session.stop_flag.clone();
            let cycle_targets = targets.clone();
            // Marcado **antes** do ciclo: lido depois, o relógio de cada conta
            // escorregaria a duração do ciclo a cada volta e o intervalo
            // efetivo cresceria com o número de contas.
            let cycle_at = now_ms();
            let result = {
                let _guard = AFK_CYCLE_LOCK.lock().await;
                tokio::task::spawn_blocking(move || {
                    run_afk_cycle_blocking(&action, &cycle_targets, &stop)
                })
                .await
                .unwrap_or_else(|e| Err(format!("AFK cycle failed: {}", e)))
            };
            let sent = match &result {
                Ok(outcome) => outcome.iter().filter(|(_, error)| error.is_none()).count() as u32,
                Err(_) => 0,
            };

            if let Ok(mut map) = session.accounts.lock() {
                let attempted_at = cycle_at;
                match result {
                    Ok(outcome) => {
                        // Só quem o ciclo visitou tem o relógio remarcado: uma
                        // parada no meio deixa o resto vencido, como estava.
                        for (user_id, error) in outcome {
                            if let Some(entry) = map.get_mut(&user_id) {
                                entry.last_send_at_ms = attempted_at;
                                if error.is_none() {
                                    entry.sends += 1;
                                }
                                entry.last_error = error;
                            }
                        }
                    }
                    Err(error) => {
                        for user_id in &targets {
                            if let Some(entry) = map.get_mut(user_id) {
                                entry.last_send_at_ms = attempted_at;
                                entry.last_error = Some(AfkSendError::Internal(error.clone()));
                            }
                        }
                    }
                }
            }
            emit_afk_status(&app);
            if sent > 0 {
                emit_afk_cycle(&app, sent);
            }
        }

        sleep_interruptible(&session.stop_flag, AFK_TICK_MS).await;
    }

    // Sessão trocada por outra enquanto esta terminava: quem limpa é a nova.
    let owns_session = AFK_MANAGER
        .get_session()
        .map(|s| s.id == session.id)
        .unwrap_or(false);
    if owns_session {
        AFK_MANAGER.replace_session(None);
    }
    session.stopped.store(true, Ordering::SeqCst);
    // `notify_one` guarda a permissão: quem for esperar depois disto não fica
    // preso até o timeout.
    session.stopped_notify.notify_one();
    let _ = app.emit("afk-stopped", ());
    emit_afk_status(&app);
}

/// Manda a sessão parar e espera até 2 s ela confirmar. Depois disso a sessão é
/// descartada de qualquer jeito: sessão parada à força não fica no caminho da
/// próxima.
#[cfg(target_os = "windows")]
async fn stop_afk_session() {
    let Some(session) = AFK_MANAGER.get_session() else {
        AFK_MANAGER.abort_task();
        return;
    };
    session.stop_flag.store(true, Ordering::SeqCst);
    if !session.stopped.load(Ordering::SeqCst) {
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            session.stopped_notify.notified(),
        )
        .await;
    }
    AFK_MANAGER.abort_task();
    AFK_MANAGER.replace_session(None);
}

#[cfg(target_os = "windows")]
#[tauri::command]
async fn start_afk_mode(
    app: tauri::AppHandle,
    user_ids: Vec<i64>,
    interval_minutes: i64,
    key: String,
    mode: String,
    click_x: f64,
    click_y: f64,
) -> Result<AfkStatusPayload, String> {
    let user_ids = dedupe_preserving_order(user_ids);
    let mode = AfkMode::parse(&mode);
    validate_afk_start(mode, &key, &user_ids)?;

    stop_afk_session().await;

    let session = new_afk_session(
        AFK_MANAGER.next_session_id(),
        now_ms(),
        AfkConfig {
            interval_minutes: clamp_afk_interval_minutes(interval_minutes),
            key,
            mode,
            default_point: AfkPoint::clamped(click_x, click_y),
        },
        &user_ids,
    );

    AFK_MANAGER.replace_session(Some(session.clone()));
    let handle = tauri::async_runtime::spawn(run_afk_session(app.clone(), session));
    AFK_MANAGER.set_task(handle);

    let status = current_afk_status();
    emit_afk_status(&app);
    Ok(status)
}

#[cfg(target_os = "windows")]
#[tauri::command]
async fn stop_afk_mode(app: tauri::AppHandle) -> Result<(), String> {
    stop_afk_session().await;
    emit_afk_status(&app);
    Ok(())
}

/// Liga e desliga contas de uma sessão em andamento. Conta que sai para de
/// receber tecla; o cliente dela **não** é tocado. Lista vazia para a sessão.
#[cfg(target_os = "windows")]
#[tauri::command]
async fn set_afk_accounts(
    app: tauri::AppHandle,
    user_ids: Vec<i64>,
) -> Result<AfkStatusPayload, String> {
    let user_ids = dedupe_preserving_order(user_ids);
    let Some(session) = AFK_MANAGER.get_session() else {
        return Err("AFK mode is not running".into());
    };

    if user_ids.is_empty() {
        stop_afk_session().await;
        let status = current_afk_status();
        emit_afk_status(&app);
        return Ok(status);
    }

    let now = now_ms();
    if let Ok(mut map) = session.accounts.lock() {
        map.retain(|user_id, _| user_ids.contains(user_id));
        for uid in &user_ids {
            map.entry(*uid)
                .or_insert_with(|| AfkAccountRuntime::joined(*uid, now));
        }
    }

    let status = current_afk_status();
    emit_afk_status(&app);
    Ok(status)
}

/// Um ciclo agora, nas contas que o usuário escolheu na tela. Existe para ele
/// conferir que o envio funciona sem esperar o intervalo — e é uma ação dele,
/// explícita, não do agendador.
///
/// Com sessão em andamento, o relógio das contas visitadas é remarcado: senão o
/// envio manual seria seguido de outro logo depois.
#[cfg(target_os = "windows")]
#[tauri::command]
async fn afk_trigger_now(app: tauri::AppHandle, user_ids: Vec<i64>) -> Result<u32, String> {
    let requested = dedupe_preserving_order(user_ids);

    // Envio manual só alcança conta que **está** no modo: fora dele, nem trazer
    // a janela para frente é permitido.
    let Some(session) = AFK_MANAGER.get_session() else {
        return Err("Start AFK mode before sending the key by hand".into());
    };
    let user_ids = session
        .accounts
        .lock()
        .map(|map| afk_manual_targets(&requested, &map))
        .unwrap_or_default();
    if user_ids.is_empty() {
        return Err("None of those accounts is in AFK mode".into());
    }
    // Tecla ou clique: o da sessão ligada, não um que a tela mande agora.
    let config = session
        .config
        .lock()
        .map(|c| c.clone())
        .map_err(|_| "AFK mode is not running".to_string())?;
    validate_afk_start(config.mode, &config.key, &user_ids)?;

    let action = afk_cycle_action(&app, &config, &user_ids);
    let cycle_targets = user_ids.clone();
    let outcome = {
        let _guard = AFK_CYCLE_LOCK.lock().await;
        // Sem stop_flag: este ciclo é o clique do usuário, não o agendador.
        let idle = AtomicBool::new(false);
        tokio::task::spawn_blocking(move || {
            run_afk_cycle_blocking(&action, &cycle_targets, &idle)
        })
        .await
        .unwrap_or_else(|e| Err(format!("AFK cycle failed: {}", e)))?
    };

    if let Ok(mut map) = session.accounts.lock() {
        let attempted_at = now_ms();
        for (user_id, error) in &outcome {
            if let Some(entry) = map.get_mut(user_id) {
                entry.last_send_at_ms = attempted_at;
                if error.is_none() {
                    entry.sends += 1;
                }
                entry.last_error = error.clone();
            }
        }
    }
    emit_afk_status(&app);

    let sent = outcome.iter().filter(|(_, error)| error.is_none()).count() as u32;
    if sent > 0 {
        emit_afk_cycle(&app, sent);
    }
    Ok(sent)
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn afk_trigger_now(_user_ids: Vec<i64>) -> Result<u32, String> {
    Err("AFK mode is only available on Windows".into())
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn start_afk_mode(
    _user_ids: Vec<i64>,
    _interval_minutes: i64,
    _key: String,
    _mode: String,
    _click_x: f64,
    _click_y: f64,
) -> Result<AfkStatusPayload, String> {
    Err("AFK mode is only available on Windows".into())
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn stop_afk_mode() -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn set_afk_accounts(_user_ids: Vec<i64>) -> Result<AfkStatusPayload, String> {
    Err("AFK mode is only available on Windows".into())
}

/// O Marcar: lê a posição do cursor **uma vez** (a contagem regressiva é da
/// tela) e diz de que conta é a janela embaixo dele e onde, em porcentagem. Só
/// vale cliente aberto por este app e que ainda é um Roblox — PID reaproveitado
/// pelo Windows não conta. Não lê botão nem tecla.
#[cfg(target_os = "windows")]
#[tauri::command]
fn afk_capture_point() -> Result<AfkCapturedPoint, String> {
    use platform::windows;
    let cursor = windows::cursor_position().ok_or("noCursor")?;
    let hwnd = windows::root_window_at(cursor.0, cursor.1);
    let alive: HashSet<u32> = windows::get_roblox_pids().into_iter().collect();
    let tracked: Vec<(i64, u32)> = windows::tracker()
        .get_all()
        .into_iter()
        .filter(|process| alive.contains(&process.pid))
        .map(|process| (process.user_id, process.pid))
        .collect();
    afk_capture_from_parts(
        cursor,
        windows::window_pid(hwnd),
        &tracked,
        windows::client_rect_on_screen(hwnd),
    )
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
fn afk_capture_point() -> Result<AfkCapturedPoint, String> {
    Err("AFK mode is only available on Windows".into())
}

#[tauri::command]
fn get_afk_mode_status() -> Result<AfkStatusPayload, String> {
    Ok(current_afk_status())
}

/// A lista fechada de teclas, para a tela oferecer exatamente o que o backend
/// aceita — em vez de manter uma segunda lista no frontend, que sairia do lugar.
#[tauri::command]
fn get_afk_keys() -> Result<Vec<String>, String> {
    Ok(afk_key_names())
}
