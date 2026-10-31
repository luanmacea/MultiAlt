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

/// Pares de setas que **se anulam** (ideia 24, 15/10/2026): a primeira tecla é
/// tocada e logo depois a segunda, do mesmo tamanho — conta como atividade, e o
/// personagem termina onde começou. `(virtual key, estendida)` de cada toque:
/// as setas são teclas estendidas, como nas Gravações.
///
/// - `Right+Left`: no controle padrão do Roblox, ← e → giram a câmera — um
///   fio para a direita e o mesmo de volta; o personagem não anda.
/// - `Up+Down`: ↑ e ↓ andam — um passo curto para frente e o mesmo para trás
///   (pode sobrar um fio de diferença com a inércia do personagem).
const AFK_KEY_PAIRS: &[(&str, [(u16, bool); 2])] = &[
    ("Right+Left", [(0x27, true), (0x25, true)]),
    ("Up+Down", [(0x26, true), (0x28, true)]),
];

/// Os nomes da lista fechada, na ordem em que a tela os oferece: as teclas
/// simples e, no fim, os pares.
fn afk_key_names() -> Vec<String> {
    AFK_KEYS
        .iter()
        .map(|(name, _)| *name)
        .chain(AFK_KEY_PAIRS.iter().map(|(name, _)| *name))
        .map(str::to_string)
        .collect()
}

/// Os toques que uma tecla da lista manda, em ordem — `(virtual key,
/// estendida)`: um só para tecla simples, dois para um par. `None` para
/// qualquer nome fora da lista: é este `None` que impede tecla de fora de
/// chegar ao `SendInput`.
fn afk_key_taps(key: &str) -> Option<Vec<(u16, bool)>> {
    if let Some(vk) = afk_virtual_key(key) {
        return Some(vec![(vk, false)]);
    }
    AFK_KEY_PAIRS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, taps)| taps.to_vec())
}

/// Virtual key de uma tecla **simples** da lista. `None` para qualquer outro
/// nome (os pares passam por `afk_key_taps`).
fn afk_virtual_key(key: &str) -> Option<u16> {
    AFK_KEYS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, vk)| *vk)
}

/// Intervalo entre envios da mesma conta, em segundos (a tela manda minutos e
/// segundos somados). O teto de 2 h é o mesmo da tela (120 min); o piso de 5 s
/// evita sessão que rouba o foco sem parar.
fn clamp_afk_interval_seconds(seconds: i64) -> u64 {
    seconds.clamp(5, 7_200) as u64
}

/// O intervalo em milissegundos, que é a unidade do relógio das contas.
fn afk_interval_ms(interval_seconds: u64) -> i64 {
    (interval_seconds as i64).saturating_mul(1_000)
}

/// O que o AFK mode manda para cada conta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AfkMode {
    /// Toca uma tecla da lista fechada.
    Key,
    /// Clique esquerdo num ponto relativo da janela da conta. Existe para quem
    /// quer o personagem **parado**: toda tecla da lista mexe nele.
    Click,
    /// Toca a gravação da conta (a própria ou a de todas) — ver
    /// docs/features/recordings.md.
    Recording,
}

impl AfkMode {
    /// Qualquer valor desconhecido vira `Key`: é o modo que já existia.
    fn parse(raw: &str) -> AfkMode {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("click") {
            AfkMode::Click
        } else if raw.eq_ignore_ascii_case("recording") {
            AfkMode::Recording
        } else {
            AfkMode::Key
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            AfkMode::Key => "key",
            AfkMode::Click => "click",
            AfkMode::Recording => "recording",
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
    afk_click_plan_with(rect, point, hold_ms, true)
}

/// A receita, com o clique de foco opcional. O AFK sempre o dá (a janela acabou
/// de vir para frente); numa Gravação só o primeiro clique o dá — os seguintes
/// já acham o jogo focado, e um clique a mais apertaria o botão do jogo duas
/// vezes.
fn afk_click_plan_with(
    rect: AfkClientRect,
    point: AfkPoint,
    hold_ms: u64,
    focus_click: bool,
) -> Option<Vec<AfkMouseStep>> {
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
    if focus_click {
        steps.push(Wait(AFK_CLICK_FOCUS_SETTLE_MS));
        click(&mut steps, false);
    }
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
    /// Os passos da gravação de cada conta, já resolvidos (a própria ou a de
    /// todas). Conta sem gravação não está no mapa e é pulada sem foco nenhum.
    Recording(HashMap<i64, Vec<data::recordings::RecordingStep>>),
}

/// Por que um start não pode acontecer. No modo tecla, sem tecla escolhida o
/// modo **não liga**: inventar uma tecla padrão seria mexer no personagem sem o
/// usuário pedir. O modo clique não usa tecla.
fn validate_afk_start(mode: AfkMode, key: &str, user_ids: &[i64]) -> Result<(), String> {
    // O modo gravação também não usa tecla: quem toca é a gravação de cada conta.
    if mode == AfkMode::Key && afk_key_taps(key).is_none() {
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
    /// Modo gravação: a conta não tem gravação (nem a própria, nem a de todas).
    /// A janela nem vem para frente.
    NoRecording,
    /// Modo gravação: outra janela veio para frente no meio da gravação, e o
    /// resto dela **não** foi tocado (cairia na janela do usuário).
    FocusLost,
    /// Modo gravação: parada no meio da gravação.
    Stopped,
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
            AfkSendError::NoRecording => "noRecording",
            AfkSendError::FocusLost => "focusLost",
            AfkSendError::Stopped => "stopped",
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
            AfkSendError::NoRecording => "This account has no recording to play".into(),
            AfkSendError::FocusLost => {
                "Another window came to the front, so the rest of the recording was not played".into()
            }
            AfkSendError::Stopped => "Stopped in the middle of the recording".into(),
            AfkSendError::Internal(message) => message.clone(),
        }
    }

    /// A mesma frase de `message()` para o Console, traduzível (a de falha
    /// inesperada é texto de fora e vai como veio).
    /// `afk_send_error_log_tests` confere que as duas não se separam.
    fn log_msg(&self) -> LogMsg {
        match self {
            AfkSendError::NoWindow => log_msg!("No Roblox window for this account"),
            AfkSendError::FocusDenied => log_msg!(
                "Windows did not bring this account's Roblox window to the front, so nothing was sent"
            ),
            AfkSendError::KeyRefused => log_msg!("Windows refused the synthetic key"),
            AfkSendError::ClickRefused => log_msg!("Windows refused the synthetic click"),
            AfkSendError::NoRecording => log_msg!("This account has no recording to play"),
            AfkSendError::FocusLost => {
                log_msg!("Another window came to the front, so the rest of the recording was not played")
            }
            AfkSendError::Stopped => log_msg!("Stopped in the middle of the recording"),
            AfkSendError::Internal(message) => LogMsg::raw(message.clone()),
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

/// Remarca o relógio de quem o ciclo visitou com `finished_at_ms`, o **fim** do
/// ciclo: a espera até o próximo envio conta de quando o ciclo acabou ("10
/// segundos depois que um ciclo acabar"), então o intervalo efetivo de uma conta
/// é o intervalo mais a duração do ciclo.
///
/// Só quem o ciclo visitou é remarcado: uma parada no meio deixa o resto vencido,
/// como estava. Ciclo que falhou inteiro remarca todos os alvos com o erro.
fn afk_record_cycle(
    accounts: &mut HashMap<i64, AfkAccountRuntime>,
    targets: &[i64],
    result: &Result<Vec<(i64, Option<AfkSendError>)>, String>,
    finished_at_ms: i64,
) {
    match result {
        Ok(outcome) => {
            for (user_id, error) in outcome {
                if let Some(entry) = accounts.get_mut(user_id) {
                    entry.last_send_at_ms = finished_at_ms;
                    if error.is_none() {
                        entry.sends += 1;
                    }
                    entry.last_error = error.clone();
                }
            }
        }
        Err(error) => {
            for user_id in targets {
                if let Some(entry) = accounts.get_mut(user_id) {
                    entry.last_send_at_ms = finished_at_ms;
                    entry.last_error = Some(AfkSendError::Internal(error.clone()));
                }
            }
        }
    }
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

/// Entre os dois toques de um par de setas, antes do segundo, ainda dá para
/// enviar? Não, se o usuário mandou parar ou clicou em outra janela (o foco
/// saiu da do alvo) durante o respiro do par — o segundo toque cairia fora.
/// `foreground`/`target` são handles em número; `0` nunca é o alvo.
fn afk_can_send_next_tap(stopping: bool, foreground: isize, target: isize) -> bool {
    !stopping && target != 0 && foreground == target
}

/// Como a janela de uma conta volta ao lugar depois do ciclo (ideia 44).
///
/// Trazer para frente tira a janela de onde estava: desminimiza (o
/// `focus_window` faz `SW_RESTORE` em janela minimizada — e só nela: maximizada
/// continua maximizada) e põe por cima de tudo a que estava atrás de outras
/// janelas. Quem trabalha com os clientes minimizados, ou cobertos, não pediu
/// para vê-los na tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AfkPutBack {
    /// Estava minimizada: volta à barra de tarefas sem ativar ninguém.
    Minimize,
    /// Estava aberta atrás da janela `0`: volta para debaixo dela.
    Below(isize),
    /// Estava no topo (ou não há referência): fica onde o ciclo deixou.
    Leave,
}

/// O que fazer com a janela `target`, a partir do que ela era **antes** do
/// ciclo: minimizada ou não, e a janela logo acima dela (`0` = nenhuma, ou uma
/// "sempre no topo"; ver `window_above`).
fn afk_put_back_plan(was_minimized: bool, above: isize, target: isize) -> AfkPutBack {
    if was_minimized {
        AfkPutBack::Minimize
    } else if above != 0 && above != target {
        AfkPutBack::Below(above)
    } else {
        AfkPutBack::Leave
    }
}

/// Em que ordem devolver as janelas. Uma janela que volta para debaixo de
/// **outra janela do ciclo** espera essa outra voltar primeiro — senão ela é
/// posta debaixo de uma janela que ainda vai mudar de lugar, e a pilha não fica
/// como era. As minimizadas vão por último (saem da pilha visível de qualquer
/// jeito); quem fica onde está não entra na lista.
fn afk_put_back_order(plans: &[(isize, AfkPutBack)]) -> Vec<usize> {
    let mut order = Vec::with_capacity(plans.len());
    let mut done = vec![false; plans.len()];
    let restacks: Vec<usize> = (0..plans.len())
        .filter(|&i| matches!(plans[i].1, AfkPutBack::Below(_)))
        .collect();
    loop {
        let mut progressed = false;
        for &i in &restacks {
            if done[i] {
                continue;
            }
            let AfkPutBack::Below(above) = plans[i].1 else {
                continue;
            };
            let waits_for_another = restacks
                .iter()
                .any(|&j| j != i && !done[j] && plans[j].0 == above);
            if !waits_for_another {
                done[i] = true;
                order.push(i);
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    // Ciclo impossível numa pilha de verdade; se vier, ninguém fica de fora.
    order.extend(restacks.iter().copied().filter(|&i| !done[i]));
    order.extend((0..plans.len()).filter(|&i| plans[i].1 == AfkPutBack::Minimize));
    order
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

/// Uma janela do ciclo deve voltar ao lugar (minimizar / pôr embaixo)? Não, se
/// ela ainda é a da **frente** e o foco **não** voltou de verdade para a janela
/// do usuário (parada no meio, ou o give-back recusado): pô-la embaixo com
/// `SWP_NOACTIVATE` a deixaria ativa mas escondida sob a janela do usuário — e
/// a tecla que o usuário digitar cairia no jogo; minimizá-la faria o Windows
/// ativar outra (o piscar). As **outras** janelas do ciclo (não são a da
/// frente) voltam normalmente. Com o foco devolvido (give-back `Restored` ou
/// `UserMovedOn`), a da frente é a do usuário, não uma do ciclo, e todas voltam.
fn afk_should_put_back(is_foreground: bool, focus_returned: bool) -> bool {
    focus_returned || !is_foreground
}

// ── tela cheia na frente (ideia 25) ─────────────────────────────────────────
//
// Com um vídeo ou outro jogo em tela cheia na frente, trazer a janela do Roblox
// tiraria a pessoa do que ela está vendo. O ciclo espera (confere de novo a cada
// tique) até a tela cheia sair — ou até o teto, para a conta não cair por
// inatividade. Só geometria de janela: qual está na frente, se ela cobre o
// monitor dela inteiro e de que processo ela é. Nada de entrada é lido.

/// Quanto o ciclo espera, no máximo, depois da hora de uma conta. O Roblox
/// derruba quem fica parado 20 min; com o intervalo padrão de 10 min, 5 min de
/// espera ainda deixam folga.
const AFK_FULLSCREEN_MAX_WAIT_MS: i64 = 5 * 60_000;

/// `Afk.HideWindows`: ligada por padrão (o ciclo dá o foco às janelas do
/// Roblox sem elas aparecerem na tela — `platform/windows/hidden_focus.rs`);
/// só `"false"` desliga. Vale em todo modo desde 26/10/2026: no clique e na
/// gravação a janela fica na frente, invisível, só durante o envio
/// (`afk_hidden_alpha`, `afk_tuck_before_action`).
fn afk_hide_windows(raw: &str) -> bool {
    raw.trim() != "false"
}

/// A transparência da janela escondida. Tecla: 0. Clique e gravação: 1 de
/// 255 — com 0 o teste de acerto do Windows pula a janela e o clique cai na de
/// baixo (medido com `WindowFromPoint`, 26/10/2026); com 1 ela recebe o clique
/// e continua invisível a olho nu.
fn afk_hidden_alpha(action_is_key: bool) -> u8 {
    if action_is_key {
        0
    } else {
        1
    }
}

/// A janela volta para a profundidade de antes logo depois de ganhar o foco
/// (tecla: vai para quem tem o foco) ou só depois do envio (clique e gravação:
/// o clique cai na janela que estiver por cima do ponto).
fn afk_tuck_before_action(action_is_key: bool) -> bool {
    action_is_key
}

/// Quanto antes do ciclo o bipe de aviso toca (`Afk.BeepBeforeCycle`).
const AFK_WARNING_LEAD_MS: i64 = 3_000;
/// Aviso mais velho que isto não vale mais: o ciclo ficou segurado (tela cheia
/// na frente) e avisa de novo antes de enviar.
const AFK_WARNING_STALE_MS: i64 = 30_000;

/// `Afk.BeepBeforeCycle`: desligada por padrão; só `"true"` liga.
fn afk_beep_before_cycle_enabled(raw: &str) -> bool {
    raw.trim() == "true"
}

/// O bipe de aviso a cada tique do laço: `(avisar agora, pode enviar)`.
/// `next_due_ms`: quando vence a próxima conta; `warned_at_ms`: quando o aviso
/// deste ciclo tocou. O ciclo só sai `AFK_WARNING_LEAD_MS` depois do aviso —
/// com o tique de 1 s o aviso pode tocar com menos de 3 s pela frente, e o
/// ciclo espera o resto.
fn afk_warning_step(enabled: bool, next_due_ms: Option<i64>, now_ms: i64, warned_at_ms: Option<i64>) -> (bool, bool) {
    if !enabled {
        return (false, true);
    }
    let Some(due) = next_due_ms else {
        return (false, true);
    };
    match warned_at_ms.filter(|at| now_ms.saturating_sub(*at) <= AFK_WARNING_STALE_MS) {
        Some(at) => (false, now_ms.saturating_sub(at) >= AFK_WARNING_LEAD_MS),
        None if now_ms >= due.saturating_sub(AFK_WARNING_LEAD_MS) => (true, false),
        None => (false, true),
    }
}

/// `Afk.WaitForFullscreen`: ligada por padrão (protege quem está vendo algo em
/// tela cheia); só `"false"` desliga.
fn afk_wait_for_fullscreen_enabled(raw: &str) -> bool {
    raw.trim() != "false"
}

/// A janela da frente segura o ciclo? Só se cobre o monitor inteiro (a tela
/// cheia de um vídeo ou de outro jogo) e não é de um cliente que o app abriu,
/// da área de trabalho (que também cobre o monitor) nem do próprio MultiAlt.
/// Cliente aberto pelo site **segura**: é a pessoa jogando.
fn afk_foreground_blocks(
    foreground_pid: Option<u32>,
    covers_monitor: bool,
    app_client_pids: &HashSet<u32>,
    shell_pids: &HashSet<u32>,
    own_pid: u32,
) -> bool {
    let Some(pid) = foreground_pid else {
        return false;
    };
    covers_monitor && pid != own_pid && !app_client_pids.contains(&pid) && !shell_pids.contains(&pid)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AfkGate {
    Send,
    Wait,
}

/// Envia agora ou espera a tela cheia sair? `due_since_ms`: desde quando a
/// conta mais atrasada do ciclo está na hora.
fn afk_fullscreen_gate(
    enabled: bool,
    fullscreen_in_front: bool,
    due_since_ms: i64,
    now_ms: i64,
    max_wait_ms: i64,
) -> AfkGate {
    if !enabled || !fullscreen_in_front {
        return AfkGate::Send;
    }
    if now_ms.saturating_sub(due_since_ms) >= max_wait_ms {
        AfkGate::Send
    } else {
        AfkGate::Wait
    }
}

/// Desde quando o alvo mais atrasado está na hora (último envio + intervalo).
fn afk_due_since(
    accounts: &HashMap<i64, AfkAccountRuntime>,
    targets: &[i64],
    interval_ms: i64,
    now_ms: i64,
) -> i64 {
    targets
        .iter()
        .filter_map(|uid| accounts.get(uid))
        .map(|entry| afk_next_send_at_ms(entry.last_send_at_ms, interval_ms))
        .min()
        .unwrap_or(now_ms)
}

/// A janela da frente agora segura o ciclo? Primeiro o barato (cobre o
/// monitor?); só então a lista de processos.
#[cfg(target_os = "windows")]
fn afk_fullscreen_in_front() -> bool {
    use platform::windows;
    let foreground = windows::get_foreground_hwnd();
    if foreground.is_null() {
        return false;
    }
    let covers = windows::window_mode_of(foreground) == Some(windows::WindowMode::Fullscreen);
    if !covers {
        return false;
    }
    let app_clients: HashSet<u32> = windows::tracker()
        .get_all()
        .into_iter()
        .filter(|process| !process.adopted)
        .map(|process| process.pid)
        .collect();
    let shell: HashSet<u32> = windows::get_shell_pids().into_iter().collect();
    afk_foreground_blocks(
        windows::window_pid(foreground),
        covers,
        &app_clients,
        &shell,
        std::process::id(),
    )
}

#[derive(Debug, Clone, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct AfkAccountStatus {
    user_id: i64,
    last_send_at_ms: i64,
    next_send_at_ms: i64,
    sends: u64,
    last_error: Option<String>,
    /// `noWindow`, `focusDenied`, `keyRefused`, `clickRefused`, `noRecording`,
    /// `focusLost`, `stopped` ou `internal` — a tela escolhe a frase traduzida
    /// por aqui, em vez de casar texto em inglês.
    last_error_code: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AfkStatusPayload {
    active: bool,
    started_at_ms: Option<i64>,
    /// Segundos entre dois envios da mesma conta, contados do fim do ciclo.
    interval_seconds: u64,
    key: String,
    /// `key` ou `click`.
    mode: String,
    /// Ponto padrão do modo clique, em porcentagem.
    click_x: f64,
    click_y: f64,
    accounts: Vec<AfkAccountStatus>,
    /// O ciclo está na hora mas espera: há uma janela em tela cheia na frente
    /// (`afk_fullscreen_gate`).
    waiting_fullscreen: bool,
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
    /// Segundos entre dois envios da mesma conta (`clamp_afk_interval_seconds`).
    interval_seconds: u64,
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
    /// Ciclo segurado por uma janela em tela cheia na frente.
    waiting_fullscreen: Arc<AtomicBool>,
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
        waiting_fullscreen: Arc::new(AtomicBool::new(false)),
    }
}

/// O status que a tela recebe, montado a partir das partes — separado de
/// `afk_status_from` para o teste poder montar um sem sessão viva.
fn afk_status_from_parts(
    started_at_ms: Option<i64>,
    interval_seconds: u64,
    key: &str,
    mode: AfkMode,
    default_point: AfkPoint,
    accounts: &HashMap<i64, AfkAccountRuntime>,
) -> AfkStatusPayload {
    let interval_ms = afk_interval_ms(interval_seconds);
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
        interval_seconds,
        key: key.to_string(),
        mode: mode.as_str().to_string(),
        click_x: default_point.x_pct,
        click_y: default_point.y_pct,
        accounts: rows,
        waiting_fullscreen: false,
    }
}

#[cfg(target_os = "windows")]
fn afk_status_from(session: &AfkSession) -> AfkStatusPayload {
    let config = session
        .config
        .lock()
        .map(|c| c.clone())
        .unwrap_or_else(|_| AfkConfig {
            interval_seconds: 0,
            key: String::new(),
            mode: AfkMode::Key,
            default_point: AFK_DEFAULT_POINT,
        });
    let accounts = session
        .accounts
        .lock()
        .map(|map| map.clone())
        .unwrap_or_default();

    let mut status = afk_status_from_parts(
        Some(session.started_at_ms),
        config.interval_seconds,
        &config.key,
        config.mode,
        config.default_point,
        &accounts,
    );
    status.waiting_fullscreen = session.waiting_fullscreen.load(Ordering::Relaxed);
    status
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
    hide_windows: bool,
) -> Result<Vec<(i64, Option<AfkSendError>)>, String> {
    use platform::windows;

    // Um ciclo por vez **de verdade**: `JoinHandle::abort` não interrompe uma
    // closure de `spawn_blocking` que já começou, então o lock assíncrono lá fora
    // pode ser liberado com este corpo ainda rodando.
    let _serial = AFK_CYCLE_SEQ
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if let AfkCycleAction::Key(key) = action {
        if afk_key_taps(key).is_none() {
            return Err(format!("Key not allowed in AFK mode: {}", key));
        }
    }

    let previous_foreground = windows::get_foreground_hwnd();
    // PID de cliente que já morreu pode ter sido reaproveitado pelo Windows por
    // outro programa qualquer: só vale PID que ainda é um Roblox.
    let alive: HashSet<u32> = windows::get_roblox_pids().into_iter().collect();
    let tracker = windows::tracker();

    let window_of = |user_id: i64| {
        tracker
            .get_pid(user_id)
            .filter(|pid| alive.contains(pid))
            .and_then(windows::find_main_window)
            .filter(|hwnd| windows::window_exists(*hwnd))
    };
    // Onde cada janela estava **antes** de o ciclo mexer em qualquer uma
    // (ideia 44): minimizada ou não, e qual janela estava logo acima. Lido de
    // todas antes da primeira vir para frente — depois disso a pilha já mudou.
    let before: HashMap<isize, (bool, isize)> = targets
        .iter()
        .filter_map(|&user_id| window_of(user_id))
        .map(|hwnd| {
            let minimized = windows::window_is_minimized(hwnd);
            let above = if minimized { 0 } else { windows::window_above(hwnd) as isize };
            (hwnd as isize, (minimized, above))
        })
        .collect();

    let mut focus_taken = false;
    // Janelas que o ciclo trouxe para frente: se no fim a da frente não é
    // nenhuma delas, foi o usuário que escolheu outra (issue #23).
    let mut cycle_windows = Vec::new();
    // Como cada janela que o ciclo trouxe volta ao lugar, no fim.
    let mut put_back: Vec<(isize, AfkPutBack)> = Vec::new();
    let mut outcome: Vec<(i64, Option<AfkSendError>)> = Vec::new();
    // Janelas que o ciclo deixou transparentes: voltam a aparecer só depois
    // de devolvidas ao lugar (minimizadas ou atrás), no fim.
    let mut hidden: Vec<windows::HiddenWindow> = Vec::new();
    let action_is_key = matches!(action, AfkCycleAction::Key(_));

    for &user_id in targets {
        let hwnd = window_of(user_id);

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
                // Modo gravação sem gravação para esta conta: nada a tocar, e
                // a janela nem vem para frente.
                if let AfkCycleAction::Recording(plans) = action {
                    if !plans.contains_key(&user_id) {
                        outcome.push((user_id, Some(AfkSendError::NoRecording)));
                        continue;
                    }
                }
                // Quem trabalha com os clientes minimizados (ou cobertos) não
                // pediu para vê-los: a janela volta ao lugar no fim do ciclo.
                let (was_minimized, above) = before
                    .get(&(hwnd as isize))
                    .copied()
                    .unwrap_or_else(|| (windows::window_is_minimized(hwnd), 0));
                let plan = afk_put_back_plan(was_minimized, above, hwnd as isize);
                if !put_back.iter().any(|(h, _)| *h == hwnd as isize) {
                    put_back.push((hwnd as isize, plan));
                }
                // Recusado (o usuário mexeu em outra janela), tenta de novo
                // depois de um movimento de mouse de zero pixel — ver
                // `bring_forward_for_cycle`.
                // Foco sem aparecer (Afk.HideWindows): a janela que não estava à
                // vista fica transparente enquanto tem o foco.
                if windows::should_hide_during_focus(
                    hide_windows,
                    was_minimized,
                    !was_minimized && windows::window_partly_visible(hwnd),
                ) {
                    if let Some(saved) = windows::make_window_invisible(hwnd, afk_hidden_alpha(action_is_key)) {
                        hidden.push(saved);
                    }
                }
                let requested = windows::bring_forward_for_cycle(hwnd);
                if requested && hide_windows && afk_tuck_before_action(action_is_key) {
                    // Volta na hora para a profundidade de antes; o foco fica.
                    windows::tuck_window_after_focus(
                        hwnd,
                        was_minimized,
                        above as windows_sys::Win32::Foundation::HWND,
                    );
                }
                focus_taken = true;
                cycle_windows.push(hwnd);
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
                        AfkCycleAction::Key(key) => windows::tap_afk_key(key, AFK_KEY_HOLD_MS, || {
                            // Antes do segundo toque de um par de setas: a
                            // janela do alvo ainda está na frente e ninguém
                            // mandou parar?
                            afk_can_send_next_tap(
                                stop_flag.load(Ordering::Relaxed),
                                windows::get_foreground_hwnd() as isize,
                                hwnd as isize,
                            )
                        })
                        .err()
                        .map(|_| AfkSendError::KeyRefused),
                        AfkCycleAction::Click(points) => {
                            let point = points.get(&user_id).copied().unwrap_or(AFK_DEFAULT_POINT);
                            windows::click_afk_point(hwnd, point, AFK_KEY_HOLD_MS)
                                .err()
                                .map(|_| AfkSendError::ClickRefused)
                        }
                        AfkCycleAction::Recording(plans) => plans.get(&user_id).and_then(|steps| {
                            play_recording_in_window(hwnd, steps, stop_flag)
                                .err()
                                .map(afk_error_from_playback)
                        }),
                    }
                };

                // Clique e gravação: só agora volta para trás (o clique precisava
                // dela por cima do ponto). O foco continua com ela até o fim.
                if requested && hide_windows && !afk_tuck_before_action(action_is_key) {
                    windows::tuck_window_after_focus(
                        hwnd,
                        was_minimized,
                        above as windows_sys::Win32::Foundation::HWND,
                    );
                }
                outcome.push((user_id, result));
                std::thread::sleep(std::time::Duration::from_millis(AFK_BETWEEN_WINDOWS_MS));
            }
        }
    }

    let give_back = if afk_should_restore_focus(stop_flag.load(Ordering::Relaxed), focus_taken)
        && windows::window_exists(previous_foreground)
    {
        // Só o primeiro plano volta: a janela do usuário não é restaurada nem
        // desmaximizada (o `focus_window` restauraria uma janela minimizada).
        // A volta é conferida na tela e repetida se o Windows recusar — o que
        // acontece com o usuário mexendo em outra janela durante o ciclo.
        let back = windows::give_focus_back_after_cycle(previous_foreground, &cycle_windows);
        if back == windows::GiveBack::Denied {
            eprintln!("[afk] o Windows recusou devolver o foco à janela anterior");
        }
        Some(back)
    } else {
        None
    };

    // O foco voltou de verdade para a janela do usuário? Só então a da frente
    // não é mais uma janela do Roblox que o ciclo trouxe.
    let focus_returned = matches!(
        give_back,
        Some(windows::GiveBack::Restored | windows::GiveBack::UserMovedOn)
    );
    // Só depois de o foco voltar: minimizar a janela do Roblox enquanto ela é a
    // da frente fazia o Windows ativar a próxima da pilha (o piscar entre
    // janelas), e pô-la embaixo com `SWP_NOACTIVATE` a deixaria ativa mas
    // escondida. A da frente, quando o foco **não** voltou (parada, ou give-back
    // recusado), fica como está; as outras voltam ao lugar. Nada aqui ativa
    // janela.
    let foreground_now = windows::get_foreground_hwnd() as isize;
    for index in afk_put_back_order(&put_back) {
        let (hwnd, plan) = put_back[index];
        if !afk_should_put_back(hwnd == foreground_now, focus_returned) {
            continue;
        }
        match plan {
            AfkPutBack::Minimize => {
                windows::minimize_window_quietly(hwnd as windows_sys::Win32::Foundation::HWND);
            }
            AfkPutBack::Below(above) => {
                windows::put_window_below(
                    hwnd as windows_sys::Win32::Foundation::HWND,
                    above as windows_sys::Win32::Foundation::HWND,
                );
            }
            AfkPutBack::Leave => {}
        }
    }
    // Já de volta ao lugar: agora podem aparecer de novo.
    for saved in hidden {
        windows::restore_window_visibility(saved);
    }

    Ok(outcome)
}

/// A ação do ciclo. O ponto próprio de cada conta é lido **agora**, dos campos
/// dela: mudar o ponto de uma conta vale no ciclo seguinte, sem religar o modo.
#[cfg(target_os = "windows")]
fn afk_cycle_action(app: &tauri::AppHandle, config: &AfkConfig, targets: &[i64]) -> AfkCycleAction {
    match config.mode {
        AfkMode::Key => AfkCycleAction::Key(config.key.clone()),
        // A gravação de cada conta na vaga "On anti-AFK" é lida **agora**:
        // editar a gravação, ou trocar qual vale para a conta, vale no ciclo
        // seguinte. A vaga da reconexão é outra (commands/recordings.rs).
        AfkMode::Recording => AfkCycleAction::Recording(recording_plans_for(
            &app.state::<data::recordings::RecordingStore>().load().unwrap_or_default(),
            data::recordings::RecordingSlot::AntiAfk,
            targets,
        )),
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
    // Quando o bipe de aviso do próximo ciclo tocou (`Afk.BeepBeforeCycle`).
    let mut warned_at: Option<i64> = None;
    loop {
        if session.stop_flag.load(Ordering::Relaxed) {
            break;
        }
        // Parada de emergência por chacoalhar (`Afk.ShakeToStop`): o vigia
        // liga com a sessão e a cada tique — ligar a opção no meio vale.
        ensure_shake_watch(&app);

        let Ok(config) = session.config.lock().map(|c| c.clone()) else {
            break;
        };
        let interval_ms = afk_interval_ms(config.interval_seconds);
        let Ok(targets) = session
            .accounts
            .lock()
            .map(|map| afk_due_targets(&map, interval_ms, now_ms()))
        else {
            break;
        };

        // Tela cheia de outro programa na frente: espera em vez de roubar o
        // foco (ideia 25). Confere de novo a cada tique, até o teto.
        let gate = if targets.is_empty() {
            AfkGate::Send
        } else {
            let enabled = afk_wait_for_fullscreen_enabled(
                &app.state::<SettingsStore>().get_string("Afk", "WaitForFullscreen"),
            );
            let in_front = enabled
                && tokio::task::spawn_blocking(afk_fullscreen_in_front)
                    .await
                    .unwrap_or(false);
            let now = now_ms();
            let due_since = session
                .accounts
                .lock()
                .map(|map| afk_due_since(&map, &targets, interval_ms, now))
                .unwrap_or(now);
            afk_fullscreen_gate(enabled, in_front, due_since, now, AFK_FULLSCREEN_MAX_WAIT_MS)
        };
        // Bipe de aviso 3 s antes (a tela toca o som no `afk-cycle-soon`).
        let warning_enabled = afk_beep_before_cycle_enabled(
            &app.state::<SettingsStore>().get_string("Afk", "BeepBeforeCycle"),
        );
        let next_due = session.accounts.lock().ok().and_then(|map| {
            map.values()
                .map(|entry| afk_next_send_at_ms(entry.last_send_at_ms, interval_ms))
                .min()
        });
        let now = now_ms();
        let (warn_now, may_send) = afk_warning_step(warning_enabled, next_due, now, warned_at);
        if warn_now && gate == AfkGate::Send {
            warned_at = Some(now);
            let _ = app.emit("afk-cycle-soon", serde_json::json!({ "inMs": AFK_WARNING_LEAD_MS }));
        }
        let waiting = gate == AfkGate::Wait;
        if session.waiting_fullscreen.swap(waiting, Ordering::Relaxed) != waiting {
            if waiting {
                emit_session_log(
                    &app,
                    "info",
                    "afk",
                    "AFK mode is waiting: a full-screen window is in front",
                );
            }
            emit_afk_status(&app);
        }

        if !targets.is_empty() && !waiting && may_send {
            warned_at = None;
            let action = afk_cycle_action(&app, &config, &targets);
            let hide = afk_hide_windows(&app.state::<SettingsStore>().get_string("Afk", "HideWindows"));
            let stop = session.stop_flag.clone();
            let cycle_targets = targets.clone();
            let result = {
                let _guard = AFK_CYCLE_LOCK.lock().await;
                tokio::task::spawn_blocking(move || {
                    run_afk_cycle_blocking(&action, &cycle_targets, &stop, hide)
                })
                .await
                .unwrap_or_else(|e| Err(format!("AFK cycle failed: {}", e)))
            };
            // Lido **depois** do ciclo: a espera até o próximo envio conta do fim
            // do ciclo, que foi o pedido do dono ("10 segundos depois que um
            // ciclo acabar"). O intervalo efetivo de cada conta fica sendo o
            // intervalo mais a duração do ciclo (~0,44 s por conta visitada), e
            // isso é de propósito: com intervalo curto, marcar no começo
            // deixaria quase nenhuma folga entre um ciclo longo e o seguinte.
            let finished_at = now_ms();
            let sent = match &result {
                Ok(outcome) => outcome.iter().filter(|(_, error)| error.is_none()).count() as u32,
                Err(_) => 0,
            };

            if let Ok(mut map) = session.accounts.lock() {
                afk_record_cycle(&mut map, &targets, &result, finished_at);
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
    interval_seconds: i64,
    key: String,
    mode: String,
    click_x: f64,
    click_y: f64,
) -> Result<AfkStatusPayload, String> {
    let user_ids = dedupe_preserving_order(user_ids);
    let mode = AfkMode::parse(&mode);
    validate_afk_start(mode, &key, &user_ids)?;
    if mode == AfkMode::Recording {
        let file = app.state::<data::recordings::RecordingStore>().load()?;
        if recording_plans_for(&file, data::recordings::RecordingSlot::AntiAfk, &user_ids).is_empty() {
            return Err(
                "None of these accounts has an anti-AFK recording to play: pick one in the Recordings tab".into(),
            );
        }
    }

    stop_afk_session().await;

    let session = new_afk_session(
        AFK_MANAGER.next_session_id(),
        now_ms(),
        AfkConfig {
            interval_seconds: clamp_afk_interval_seconds(interval_seconds),
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
    let hide = afk_hide_windows(&app.state::<SettingsStore>().get_string("Afk", "HideWindows"));
    let cycle_targets = user_ids.clone();
    let outcome = {
        let _guard = AFK_CYCLE_LOCK.lock().await;
        // Sem stop_flag: este ciclo é o clique do usuário, não o agendador.
        let idle = AtomicBool::new(false);
        tokio::task::spawn_blocking(move || {
            run_afk_cycle_blocking(&action, &cycle_targets, &idle, hide)
        })
        .await
        .unwrap_or_else(|e| Err(format!("AFK cycle failed: {}", e)))?
    };

    let sent = outcome.iter().filter(|(_, error)| error.is_none()).count() as u32;
    // Como no laço: o relógio marca o fim do ciclo.
    let finished_at = now_ms();
    if let Ok(mut map) = session.accounts.lock() {
        afk_record_cycle(&mut map, &user_ids, &Ok(outcome), finished_at);
    }
    emit_afk_status(&app);

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
    _interval_seconds: i64,
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
