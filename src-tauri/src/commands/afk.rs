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
//    de uma conta, o ciclo traz aquela janela para frente, manda a tecla e
//    devolve o foco para onde estava. Isso rouba o foco por um piscar a cada
//    envio, e a tela diz isso com essas palavras.
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

/// Por que um start não pode acontecer. Sem tecla escolhida o modo **não liga**:
/// inventar uma tecla padrão seria mexer no personagem sem o usuário pedir.
fn validate_afk_start(key: &str, user_ids: &[i64]) -> Result<(), String> {
    if afk_virtual_key(key).is_none() {
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
    /// Falha inesperada do ciclo.
    Internal(String),
}

impl AfkSendError {
    fn code(&self) -> &'static str {
        match self {
            AfkSendError::NoWindow => "noWindow",
            AfkSendError::FocusDenied => "focusDenied",
            AfkSendError::KeyRefused => "keyRefused",
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
/// `focus_window` faz `SW_RESTORE`); quem trabalha com os clientes minimizados
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
    /// `noWindow`, `focusDenied`, `keyRefused` ou `internal` — a tela escolhe a
    /// frase traduzida por aqui, em vez de casar texto em inglês.
    last_error_code: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct AfkStatusPayload {
    active: bool,
    started_at_ms: Option<i64>,
    interval_minutes: u64,
    key: String,
    accounts: Vec<AfkAccountStatus>,
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
    key: &str,
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

    if afk_virtual_key(key).is_none() {
        return Err(format!("Key not allowed in AFK mode: {}", key));
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

                let result = if ready {
                    windows::tap_afk_key(key, AFK_KEY_HOLD_MS)
                        .err()
                        .map(|_| AfkSendError::KeyRefused)
                } else {
                    // O Windows não deixou a janela vir para frente. Mandar a
                    // tecla aqui a entregaria na janela do usuário.
                    Some(AfkSendError::FocusDenied)
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
        windows::focus_window(previous_foreground);
    }

    Ok(outcome)
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
            let key = config.key.clone();
            let stop = session.stop_flag.clone();
            let cycle_targets = targets.clone();
            // Marcado **antes** do ciclo: lido depois, o relógio de cada conta
            // escorregaria a duração do ciclo a cada volta e o intervalo
            // efetivo cresceria com o número de contas.
            let cycle_at = now_ms();
            let result = {
                let _guard = AFK_CYCLE_LOCK.lock().await;
                tokio::task::spawn_blocking(move || {
                    run_afk_cycle_blocking(&key, &cycle_targets, &stop)
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
) -> Result<AfkStatusPayload, String> {
    let user_ids = dedupe_preserving_order(user_ids);
    validate_afk_start(&key, &user_ids)?;

    stop_afk_session().await;

    let session = new_afk_session(
        AFK_MANAGER.next_session_id(),
        now_ms(),
        AfkConfig {
            interval_minutes: clamp_afk_interval_minutes(interval_minutes),
            key,
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
async fn afk_trigger_now(
    app: tauri::AppHandle,
    user_ids: Vec<i64>,
    key: String,
) -> Result<u32, String> {
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
    validate_afk_start(&key, &user_ids)?;

    let cycle_key = key.clone();
    let cycle_targets = user_ids.clone();
    let outcome = {
        let _guard = AFK_CYCLE_LOCK.lock().await;
        // Sem stop_flag: este ciclo é o clique do usuário, não o agendador.
        let idle = AtomicBool::new(false);
        tokio::task::spawn_blocking(move || {
            run_afk_cycle_blocking(&cycle_key, &cycle_targets, &idle)
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
async fn afk_trigger_now(_user_ids: Vec<i64>, _key: String) -> Result<u32, String> {
    Err("AFK mode is only available on Windows".into())
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn start_afk_mode(
    _user_ids: Vec<i64>,
    _interval_minutes: i64,
    _key: String,
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

#[cfg(test)]
mod afk_command_tests {
    use super::*;

    // ── lista fechada de teclas ────────────────────────────────────────────

    #[test]
    fn every_allowed_key_maps_to_a_virtual_key() {
        for name in afk_key_names() {
            assert!(
                afk_virtual_key(&name).is_some(),
                "a tecla oferecida na tela tem de ter virtual key: {name}"
            );
        }
        assert_eq!(afk_key_names().len(), AFK_KEYS.len());
    }

    #[test]
    fn the_allowed_list_is_exactly_the_one_the_feature_documents() {
        assert_eq!(
            afk_key_names(),
            vec!["Space", "W", "A", "S", "D", "E", "F", "R", "Q", "1", "2", "3", "4", "5"]
        );
    }

    #[test]
    fn a_key_outside_the_list_has_no_virtual_key() {
        // Teclas que fazem outra coisa (Enter abre o chat, Tab troca de janela,
        // F4 com Alt fecha o cliente) ficam fora de propósito.
        for outside in [
            "Enter", "Tab", "Escape", "F4", "Delete", "LWin", "Ctrl", "Alt", "Shift", "Z", "0",
            "6", "9", "", " ", "0x20", "spacebar", "Space ",
        ] {
            assert!(
                afk_virtual_key(outside).is_none(),
                "tecla fora da lista foi aceita: {outside:?}"
            );
        }
    }

    #[test]
    fn the_key_name_is_matched_without_case() {
        assert_eq!(afk_virtual_key("space"), afk_virtual_key("Space"));
        assert_eq!(afk_virtual_key("w"), afk_virtual_key("W"));
        assert!(afk_virtual_key("SPACE").is_some());
    }

    #[test]
    fn the_allowed_keys_carry_the_windows_virtual_key_codes() {
        assert_eq!(afk_virtual_key("Space"), Some(0x20));
        assert_eq!(afk_virtual_key("W"), Some(0x57));
        assert_eq!(afk_virtual_key("1"), Some(0x31));
        assert_eq!(afk_virtual_key("5"), Some(0x35));
    }

    // ── recusa de start ────────────────────────────────────────────────────

    #[test]
    fn afk_mode_does_not_start_without_a_key() {
        // Não existe tecla padrão: uma tecla escolhida pelo app mexeria no
        // personagem sem o usuário ter pedido.
        let err = validate_afk_start("", &[11]).expect_err("sem tecla não liga");
        assert!(err.to_lowercase().contains("key"), "{err}");
    }

    #[test]
    fn afk_mode_does_not_start_with_a_key_outside_the_list() {
        assert!(validate_afk_start("Enter", &[11]).is_err());
        assert!(validate_afk_start("F4", &[11]).is_err());
    }

    #[test]
    fn afk_mode_does_not_start_without_an_account() {
        let err = validate_afk_start("Space", &[]).expect_err("sem conta não liga");
        assert!(err.to_lowercase().contains("account"), "{err}");
    }

    #[test]
    fn afk_mode_starts_with_a_listed_key_and_one_account() {
        assert!(validate_afk_start("Space", &[11]).is_ok());
        assert!(validate_afk_start("e", &[11, 22]).is_ok());
    }

    // ── intervalo ──────────────────────────────────────────────────────────

    #[test]
    fn clamp_afk_interval_minutes_keeps_1_to_120() {
        assert_eq!(clamp_afk_interval_minutes(10), 10);
        assert_eq!(clamp_afk_interval_minutes(1), 1);
        assert_eq!(clamp_afk_interval_minutes(120), 120);
        assert_eq!(clamp_afk_interval_minutes(0), 1);
        assert_eq!(clamp_afk_interval_minutes(-30), 1);
        assert_eq!(clamp_afk_interval_minutes(121), 120);
        assert_eq!(clamp_afk_interval_minutes(i64::MAX), 120);
    }

    // ── agendador ──────────────────────────────────────────────────────────

    const MINUTE_MS: i64 = 60_000;

    #[test]
    fn an_account_is_due_only_after_a_whole_interval() {
        let last = 1_000_000;
        let interval = 10 * MINUTE_MS;
        assert!(!afk_is_due(last, interval, last));
        assert!(!afk_is_due(last, interval, last + interval - 1));
        assert!(afk_is_due(last, interval, last + interval));
        assert!(afk_is_due(last, interval, last + interval * 3));
    }

    #[test]
    fn an_account_that_just_entered_afk_mode_waits_its_first_interval() {
        // O relógio da conta começa quando ela entra no modo: ligar o AFK mode
        // não pode mexer no personagem no mesmo segundo.
        let joined = 5_000_000;
        let runtime = AfkAccountRuntime::joined(11, joined);
        assert_eq!(runtime.last_send_at_ms, joined);
        assert!(!afk_is_due(
            runtime.last_send_at_ms,
            10 * MINUTE_MS,
            joined + 1
        ));
    }

    #[test]
    fn a_clock_that_went_backwards_does_not_trigger_a_send() {
        // Relógio do sistema andando para trás dá `now` menor que o último
        // envio: isso não é hora de enviar, é hora de esperar.
        let last = 9_000_000;
        assert!(!afk_is_due(last, 10 * MINUTE_MS, last - 60_000));
    }

    #[test]
    fn the_next_send_is_one_interval_after_the_last_one() {
        assert_eq!(afk_next_send_at_ms(1_000, 60_000), 61_000);
        assert_eq!(afk_next_send_at_ms(i64::MAX, 60_000), i64::MAX);
    }

    fn session_with(entries: &[(i64, i64)]) -> HashMap<i64, AfkAccountRuntime> {
        let mut map = HashMap::new();
        for (user_id, last) in entries {
            map.insert(*user_id, AfkAccountRuntime::joined(*user_id, *last));
        }
        map
    }

    #[test]
    fn only_the_accounts_whose_interval_expired_are_targets() {
        let now = 10 * MINUTE_MS;
        let interval = 5 * MINUTE_MS;
        let accounts = session_with(&[
            (11, now - interval),     // venceu agora
            (22, now - interval - 1), // venceu há 1 ms
            (33, now - interval + 1), // falta 1 ms
        ]);

        assert_eq!(afk_due_targets(&accounts, interval, now), vec![11, 22]);
    }

    #[test]
    fn an_account_outside_afk_mode_is_never_a_target() {
        let now = 10 * MINUTE_MS;
        let interval = MINUTE_MS;
        // 99 nunca entrou no modo: nem vencida ela aparece.
        let accounts = session_with(&[(11, 0)]);
        let targets = afk_due_targets(&accounts, interval, now);
        assert_eq!(targets, vec![11]);
        assert!(!targets.contains(&99));

        assert!(
            afk_due_targets(&HashMap::new(), interval, now).is_empty(),
            "sessão sem conta não tem alvo"
        );
    }

    #[test]
    fn the_target_order_is_stable_by_account() {
        let now = 10 * MINUTE_MS;
        let interval = MINUTE_MS;
        let accounts = session_with(&[(33, 0), (11, 0), (22, 0)]);
        assert_eq!(afk_due_targets(&accounts, interval, now), vec![11, 22, 33]);
    }

    // ── parar interrompe o ciclo em andamento ──────────────────────────────

    #[test]
    fn a_stopping_session_aborts_the_cycle_instead_of_sending() {
        assert_eq!(afk_cycle_step(true, true), AfkCycleStep::Abort);
        assert_eq!(afk_cycle_step(true, false), AfkCycleStep::Abort);
    }

    #[test]
    fn a_target_whose_window_is_gone_is_skipped_not_sent() {
        assert_eq!(afk_cycle_step(false, false), AfkCycleStep::Skip);
    }

    #[test]
    fn a_live_window_of_an_afk_account_gets_the_key() {
        assert_eq!(afk_cycle_step(false, true), AfkCycleStep::Send);
    }

    #[test]
    fn a_stopping_session_does_not_restore_the_focus() {
        assert!(!afk_should_restore_focus(true, true));
        assert!(!afk_should_restore_focus(true, false));
    }

    #[test]
    fn the_focus_only_goes_back_when_the_cycle_took_it() {
        assert!(afk_should_restore_focus(false, true));
        assert!(
            !afk_should_restore_focus(false, false),
            "ciclo que não roubou foco não mexe na janela de ninguém"
        );
    }

    // ── status ─────────────────────────────────────────────────────────────

    #[test]
    fn get_afk_mode_status_reports_no_session_by_default() {
        let status = get_afk_mode_status().unwrap();
        assert!(!status.active);
        assert!(status.accounts.is_empty());
        assert!(status.key.is_empty());
    }

    // ── o alvo precisa estar em primeiro plano antes de a tecla sair ────────

    /// O `SendInput` entrega na janela em primeiro plano, e o Windows **recusa**
    /// `SetForegroundWindow` de processo que está em segundo plano — que é o
    /// caso normal do AFK mode. Sem conferir, a tecla ia para a janela em que o
    /// usuário está digitando, e o ciclo dizia que deu tudo certo.
    #[test]
    fn a_window_that_did_not_reach_the_foreground_is_not_ready() {
        // Pedido recusado pelo Windows: nada de tecla.
        assert!(!afk_window_is_ready(false, 10, 10));
        // Pedido aceito, mas quem está na frente é outra janela (a do usuário).
        assert!(!afk_window_is_ready(true, 99, 10));
        // Sem janela em primeiro plano nenhuma.
        assert!(!afk_window_is_ready(true, 0, 10));
    }

    #[test]
    fn a_window_in_the_foreground_is_ready_for_the_key() {
        assert!(afk_window_is_ready(true, 10, 10));
    }

    #[test]
    fn a_null_target_is_never_ready() {
        // Alvo nulo casaria com "nenhuma janela em primeiro plano" e a tecla
        // sairia para o vazio — ou para quem estivesse lá.
        assert!(!afk_window_is_ready(true, 0, 0));
    }

    // ── janela minimizada volta a ser minimizada ────────────────────────────

    #[test]
    fn a_window_the_user_had_minimized_goes_back_to_minimized() {
        assert!(afk_should_reminimize(true, true));
    }

    #[test]
    fn a_window_that_was_not_minimized_is_left_alone() {
        assert!(!afk_should_reminimize(false, true));
        // Ciclo que não chegou a mexer na janela não minimiza nada.
        assert!(!afk_should_reminimize(true, false));
        assert!(!afk_should_reminimize(false, false));
    }

    // ── envio manual ───────────────────────────────────────────────────────

    #[test]
    fn a_manual_send_only_reaches_accounts_that_are_in_afk_mode() {
        let accounts = session_with(&[(11, 0), (22, 0)]);
        // 99 não está no modo: o envio manual não pode tocar na janela dela.
        assert_eq!(afk_manual_targets(&[99], &accounts), Vec::<i64>::new());
        assert_eq!(afk_manual_targets(&[22, 99, 11], &accounts), vec![22, 11]);
        assert_eq!(afk_manual_targets(&[], &accounts), Vec::<i64>::new());
    }

    #[test]
    fn a_manual_send_keeps_the_order_the_user_asked_for() {
        let accounts = session_with(&[(11, 0), (22, 0), (33, 0)]);
        assert_eq!(afk_manual_targets(&[33, 11], &accounts), vec![33, 11]);
    }

    // ── erro por conta, com código que a tela entende ───────────────────────

    #[test]
    fn every_send_error_carries_a_code_and_a_message() {
        for error in [
            AfkSendError::NoWindow,
            AfkSendError::FocusDenied,
            AfkSendError::KeyRefused,
            AfkSendError::Internal("boom".into()),
        ] {
            assert!(!error.code().is_empty());
            assert!(!error.message().is_empty());
        }
        assert_eq!(AfkSendError::FocusDenied.code(), "focusDenied");
        assert_eq!(AfkSendError::NoWindow.code(), "noWindow");
        assert_eq!(AfkSendError::KeyRefused.code(), "keyRefused");
        assert_eq!(AfkSendError::Internal("boom".into()).message(), "boom");
    }

    #[test]
    fn the_status_tells_the_screen_which_error_it_was() {
        let mut runtime = AfkAccountRuntime::joined(11, 0);
        runtime.last_error = Some(AfkSendError::FocusDenied);
        let mut accounts = HashMap::new();
        accounts.insert(11, runtime);

        let status = afk_status_from_parts(Some(1), 10, "Space", &accounts);
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["accounts"][0]["lastErrorCode"], "focusDenied");
        assert!(json["accounts"][0]["lastError"].is_string());
    }

    /// Defeito que o upstream teve e que aqui não pode nascer: o prazo do
    /// primeiro envio só aparecia **depois** do primeiro ciclo, e quem ligava o
    /// modo passava o intervalo inteiro olhando um "--", sem saber se pegou.
    #[cfg(target_os = "windows")]
    #[test]
    fn the_first_deadline_is_known_the_moment_the_session_starts() {
        let started_at = 1_000;
        let session = new_afk_session(
            7,
            started_at,
            AfkConfig {
                interval_minutes: 10,
                key: "Space".into(),
            },
            &[11, 22],
        );

        let status = afk_status_from(&session);
        assert!(status.active);
        assert_eq!(status.started_at_ms, Some(started_at));
        assert_eq!(status.interval_minutes, 10);
        assert_eq!(status.accounts.len(), 2);
        for account in &status.accounts {
            assert_eq!(
                account.next_send_at_ms,
                started_at + 10 * 60_000,
                "a tela tem de saber o primeiro prazo antes de qualquer envio"
            );
            assert_eq!(account.last_send_at_ms, started_at);
            assert_eq!(account.sends, 0);
            assert!(account.last_error.is_none());
        }
    }

    #[test]
    fn get_afk_keys_offers_the_closed_list_and_nothing_else() {
        assert_eq!(get_afk_keys().unwrap(), afk_key_names());
    }

    #[test]
    fn the_status_payload_reaches_the_frontend_in_camel_case() {
        let status = AfkStatusPayload {
            active: true,
            started_at_ms: Some(7),
            interval_minutes: 10,
            key: "Space".into(),
            accounts: vec![AfkAccountStatus {
                user_id: 11,
                last_send_at_ms: 1,
                next_send_at_ms: 2,
                sends: 3,
                last_error: None,
                last_error_code: None,
            }],
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["active"], true);
        assert_eq!(json["startedAtMs"], 7);
        assert_eq!(json["intervalMinutes"], 10);
        assert_eq!(json["key"], "Space");
        assert_eq!(json["accounts"][0]["userId"], 11);
        assert_eq!(json["accounts"][0]["nextSendAtMs"], 2);
        assert_eq!(json["accounts"][0]["sends"], 3);
    }
}

#[cfg(test)]
mod afk_input_safety_tests {
    // A trava de segurança do AFK mode: ele **só envia** entrada, e só tecla da
    // lista fechada. Este módulo varre o código de verdade em vez de confiar em
    // revisão:
    //
    // 1. caminha por `src-tauri/src` inteiro (arquivo novo entra na varredura
    //    sozinho, e é essa a diferença em relação a listar dois caminhos à mão);
    // 2. considera "arquivo do AFK mode" todo arquivo cujo caminho cita `afk`,
    //    **todo arquivo que envia entrada** (cita `SendInput`) — logo um
    //    `platform/windows/input2.rs` novo cai na rede — e **todo fragmento do
    //    mesmo módulo** que um deles. `include!()` não cria módulo: os
    //    `platform/windows/*.rs` são um módulo só, `windows`, e os
    //    `commands/*.rs` são pedaços da raiz do crate. Fragmento irmão se chama
    //    sem caminho nenhum, então não há fronteira a vigiar entre eles;
    // 3. tira os módulos de teste contando chaves, não cortando no primeiro
    //    `#[cfg(test)]`: código de produção escrito **depois** de um módulo de
    //    teste continua sendo varrido (`the_scan_sees_code_after_the_test_modules`);
    // 4. reprova API de leitura de entrada (gancho global, estado de tecla,
    //    entrada crua, tradução de tecla para caractere, nome de tecla, hook de
    //    evento de UI, `AttachThreadInput` — o truque que alguém acrescentaria
    //    para "consertar" o `SetForegroundWindow` recusado — e
    //    `GetLastInputInfo`, que é a "detecção de interação" que esta
    //    funcionalidade recusa porque responde pela sessão inteira do Windows);
    // 5. reprova injeção fora da lista fechada: `INPUT_MOUSE`, `mouse_event` e
    //    `KEYEVENTF_UNICODE` mandariam entrada sem citar API proibida nenhuma;
    // 6. reprova **alcance indireto**: os arquivos do AFK mode não podem citar o
    //    nome de nenhum módulo do backend que leia entrada (hoje o
    //    `webview_recovery`, que importa `GetAsyncKeyState` legitimamente). O
    //    nome é o do **módulo**, não o do fragmento: um `windowing.rs` que
    //    lesse entrada faria do `windows` inteiro um leitor, e é `windows::` que
    //    o `commands/afk.rs` escreve;
    // 7. confere que só o módulo de entrada chama `SendInput`/`send_key`, para o
    //    caminho único até o `SendInput` continuar único amanhã.
    use std::collections::{HashMap, HashSet};
    use std::path::{Path, PathBuf};

    const FORBIDDEN: &[&str] = &[
        // gancho global de teclado / de eventos de UI
        "SetWindowsHookEx",
        "SetWinEventHook",
        // estado de tecla
        "GetAsyncKeyState",
        "GetKeyState",
        "GetKeyboardState",
        // entrada crua
        "GetRawInputData",
        "GetRawInputBuffer",
        "RegisterRawInputDevices",
        // tecla -> caractere / nome de tecla
        "ToUnicodeEx",
        "ToUnicode",
        "ToAsciiEx",
        "ToAscii",
        "GetKeyNameText",
        // fila de entrada de outra thread e estado da UI dela
        "AttachThreadInput",
        "GetGUIThreadInfo",
        // "detecção de interação": responde pela sessão inteira, não por janela
        "GetLastInputInfo",
        // envio fora da lista fechada
        "keybd_event",
        "mouse_event",
        "INPUT_MOUSE",
        "KEYEVENTF_UNICODE",
    ];

    fn backend_src() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    /// Todo `.rs` de `src-tauri/src`, com o conteúdo.
    fn backend_files() -> Vec<(String, String)> {
        fn walk(dir: &Path, out: &mut Vec<(String, String)>) {
            let entries = match std::fs::read_dir(dir) {
                Ok(entries) => entries,
                Err(e) => panic!("não consegui ler {}: {e}", dir.display()),
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
                    let text = std::fs::read_to_string(&path)
                        .unwrap_or_else(|e| panic!("não consegui ler {}: {e}", path.display()));
                    let relative = path
                        .strip_prefix(backend_src())
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.push((relative, text));
                }
            }
        }
        let mut out = Vec::new();
        walk(&backend_src(), &mut out);
        out.sort();
        out
    }

    /// O arquivo sem os módulos `#[cfg(test)]`, casando chaves — e não cortando
    /// no primeiro atributo, que deixaria de fora tudo que vem depois dele.
    fn production_only(source: &str) -> String {
        let mut out = String::new();
        let mut rest = source;
        while let Some(at) = rest.find("#[cfg(test)]") {
            out.push_str(&rest[..at]);
            let tail = &rest[at..];
            let Some(open) = tail.find('{') else {
                // Atributo sem bloco: nada a remover daqui para frente.
                rest = "";
                break;
            };
            let bytes = tail.as_bytes();
            let mut depth = 0i32;
            let mut end = tail.len();
            for (i, byte) in bytes.iter().enumerate().skip(open) {
                if quoted_brace(bytes, i) {
                    // Chave entre aspas (`'{'`, `b'}'`, `"{"`) não abre nem
                    // fecha bloco — e este próprio arquivo tem várias.
                    continue;
                }
                if *byte == b'{' {
                    depth += 1;
                } else if *byte == b'}' {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
            }
            rest = &tail[end..];
        }
        out.push_str(rest);
        out
    }

    /// A chave nesta posição está entre aspas (literal de caractere ou de texto)?
    fn quoted_brace(bytes: &[u8], at: usize) -> bool {
        if bytes[at] != b'{' && bytes[at] != b'}' {
            return false;
        }
        let before = at.checked_sub(1).map(|i| bytes[i]);
        let after = bytes.get(at + 1).copied();
        const QUOTE: u8 = b'\'';
        const DQUOTE: u8 = b'"';
        matches!(
            (before, after),
            (Some(QUOTE), Some(QUOTE)) | (Some(DQUOTE), Some(DQUOTE))
        )
    }

    /// `(caminho, corpo de produção)` de todo o backend.
    fn backend_production() -> Vec<(String, String)> {
        backend_files()
            .into_iter()
            .map(|(path, text)| (path, production_only(&text)))
            .collect()
    }

    /// Os arquivos que este puxa por `include!("...")`, resolvidos a partir da
    /// pasta dele — que é como o compilador resolve.
    fn included_paths(path: &str, body: &str) -> Vec<String> {
        const OPEN: &str = "include!(\"";
        let dir = path.rfind('/').map(|at| &path[..at]).unwrap_or("");
        let mut out = Vec::new();
        let mut rest = body;
        while let Some(at) = rest.find(OPEN) {
            let after = &rest[at + OPEN.len()..];
            let Some(end) = after.find('"') else {
                break;
            };
            let mut parts: Vec<&str> = dir.split('/').filter(|part| !part.is_empty()).collect();
            for part in after[..end].split('/') {
                match part {
                    "" | "." => {}
                    ".." => {
                        parts.pop();
                    }
                    other => parts.push(other),
                }
            }
            out.push(parts.join("/"));
            rest = &after[end..];
        }
        out
    }

    /// O arquivo que **é** o módulo de cada caminho. Fragmento `include!()` sobe
    /// até quem o inclui (e assim por diante); arquivo que ninguém inclui é o
    /// próprio módulo.
    fn module_roots(files: &[(String, String)]) -> HashMap<String, String> {
        let mut includer: HashMap<String, String> = HashMap::new();
        for (path, body) in files {
            for included in included_paths(path, body) {
                includer.insert(included, path.clone());
            }
        }
        files
            .iter()
            .map(|(path, _)| {
                let mut root = path.clone();
                // `include!` circular não compila; o teto só impede a varredura
                // de girar para sempre se alguém tentar.
                for _ in 0..32 {
                    match includer.get(&root) {
                        Some(up) => root = up.clone(),
                        None => break,
                    }
                }
                (path.clone(), root)
            })
            .collect()
    }

    /// O nome pelo qual o resto do crate chega ao módulo: o do arquivo raiz
    /// (`platform/windows.rs` → `windows`), ou o da pasta, se o raiz é `mod.rs`.
    fn module_name(root: &str) -> String {
        let path = Path::new(root);
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_default();
        if stem != "mod" {
            return stem;
        }
        path.parent()
            .and_then(|dir| dir.file_name())
            .map(|dir| dir.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    /// Módulos (fora do AFK mode) que leem entrada de propósito — hoje a
    /// recuperação da webview, que usa `GetAsyncKeyState` legitimamente. Um
    /// fragmento que lê entrada entra com o nome do módulo que o inclui.
    fn input_reader_modules(files: &[(String, String)], afk_paths: &[String]) -> Vec<String> {
        let roots = module_roots(files);
        let mut out: Vec<String> = files
            .iter()
            .filter(|(path, _)| !afk_paths.contains(path))
            .filter(|(_, body)| FORBIDDEN.iter().any(|api| body.contains(api)))
            .map(|(path, _)| module_name(roots.get(path).unwrap_or(path)))
            // A raiz do crate não se alcança por nome: `lib::` não existe.
            .filter(|name| !matches!(name.as_str(), "" | "lib" | "main"))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// As formas de **chegar** a um módulo pelo nome. Menção solta num
    /// comentário não conta; caminho de chamada conta.
    fn module_needles(stem: &str) -> Vec<String> {
        vec![
            format!("::{stem}"),
            format!("{stem}::"),
            format!("use {stem}"),
            format!("mod {stem}"),
        ]
    }

    /// Arquivo do AFK mode: o caminho cita `afk`, ou o arquivo envia entrada.
    fn is_afk_file(path: &str, body: &str) -> bool {
        path.to_ascii_lowercase().contains("afk") || body.contains("SendInput")
    }

    /// `(caminho, corpo de produção)` dos arquivos que **são** do AFK mode — é
    /// deles que sai a chamada para outro módulo.
    fn afk_seed_files() -> Vec<(String, String)> {
        backend_production()
            .into_iter()
            .filter(|(path, body)| is_afk_file(path, body))
            .collect()
    }

    /// `(caminho, corpo de produção)` de todo fragmento de módulo que tem código
    /// do AFK mode: os arquivos do AFK mode e os irmãos `include!()` deles, que
    /// se alcançam sem caminho nenhum.
    fn afk_files() -> Vec<(String, String)> {
        let files = backend_production();
        let roots = module_roots(&files);
        let afk_modules: HashSet<String> = files
            .iter()
            .filter(|(path, body)| is_afk_file(path, body))
            .filter_map(|(path, _)| roots.get(path).cloned())
            .collect();
        files
            .into_iter()
            .filter(|(path, _)| {
                roots
                    .get(path)
                    .map(|root| afk_modules.contains(root))
                    .unwrap_or(false)
            })
            .collect()
    }

    #[test]
    fn no_afk_file_reads_input_or_sends_outside_the_closed_list() {
        let files = afk_files();
        assert!(
            files.len() >= 2,
            "a varredura tem de achar pelo menos commands/afk.rs e o módulo de envio, achei {:?}",
            files.iter().map(|(p, _)| p).collect::<Vec<_>>()
        );
        for (path, body) in &files {
            for api in FORBIDDEN {
                assert!(
                    !body.contains(api),
                    "{path} usa {api}: o AFK mode só pode enviar tecla da lista fechada, nunca ler entrada. \
                     Fragmento include!() do mesmo módulo que um arquivo do AFK mode conta como AFK mode \
                     (se chama sem caminho); leitura de entrada de outra funcionalidade vai para um módulo \
                     próprio, como o webview_recovery"
                );
            }
        }
    }

    #[test]
    fn no_afk_file_reaches_a_module_that_reads_input() {
        let afk = afk_seed_files();
        let paths: Vec<String> = afk.iter().map(|(path, _)| path.clone()).collect();
        let readers = input_reader_modules(&backend_production(), &paths);

        for (path, body) in &afk {
            for reader in &readers {
                for needle in module_needles(reader) {
                    assert!(
                        !body.contains(&needle),
                        "{path} cita {needle} — o módulo {reader} lê entrada, e o AFK mode não pode chegar lá nem indiretamente"
                    );
                }
            }
        }
    }

    /// A derivação acima só protege se ela **reconhece** um módulo leitor. Este
    /// teste alimenta a mesma função com um caso fabricado: assim ela continua
    /// valendo mesmo quando a árvore não tem (ainda) nenhum arquivo leitor.
    #[test]
    fn the_indirect_reach_check_recognizes_a_reader_module() {
        let files = vec![
            (
                "webview_recovery.rs".to_string(),
                "use windows_sys::...::GetAsyncKeyState;".to_string(),
            ),
            ("commands/afk.rs".to_string(), "nada demais".to_string()),
            ("api/auth.rs".to_string(), "nada demais".to_string()),
        ];
        let readers = input_reader_modules(&files, &["commands/afk.rs".to_string()]);
        assert_eq!(readers, vec!["webview_recovery".to_string()]);

        let needles = module_needles("webview_recovery");
        assert!(needles.iter().any(|n| n == "::webview_recovery"));
        assert!(
            needles
                .iter()
                .any(|n| "let _ = crate::webview_recovery::show();".contains(n)),
            "uma chamada indireta real tem de casar com alguma agulha"
        );
    }

    /// `include!()` não cria módulo. `platform/windows/*.rs` são pedaços de **um**
    /// módulo só, `windows` — e é por esse nome que `commands/afk.rs` chama
    /// (`windows::focus_window`). Um fragmento que lê entrada faz do módulo
    /// inteiro um leitor; tratado como módulo próprio (`windowing`), ele passava
    /// com a suíte verde, porque ninguém escreve `windowing::`.
    #[test]
    fn a_fragment_that_reads_input_makes_the_module_that_includes_it_a_reader() {
        let files = vec![
            ("lib.rs".to_string(), "include!(\"commands/afk.rs\");".to_string()),
            (
                "platform/windows.rs".to_string(),
                "include!(\"windows/windowing.rs\");\ninclude!(\"windows/input.rs\");".to_string(),
            ),
            (
                "platform/windows/windowing.rs".to_string(),
                "unsafe { AttachThreadInput(a, b, 1) };".to_string(),
            ),
            ("platform/windows/input.rs".to_string(), "SendInput(1, &i, n)".to_string()),
            (
                "commands/afk.rs".to_string(),
                "use platform::windows;\nwindows::focus_window(h);".to_string(),
            ),
        ];
        let afk = vec![
            "commands/afk.rs".to_string(),
            "platform/windows/input.rs".to_string(),
        ];

        let readers = input_reader_modules(&files, &afk);
        assert_eq!(
            readers,
            vec!["windows".to_string()],
            "o fragmento é o módulo windows, não um módulo windowing"
        );
        let afk_body = &files[4].1;
        assert!(
            module_needles("windows")
                .iter()
                .any(|needle| afk_body.contains(needle.as_str())),
            "a chamada windows::focus_window tem de casar com alguma agulha"
        );
    }

    /// Na árvore de verdade: fragmento irmão de arquivo do AFK mode se alcança
    /// **sem caminho nenhum** — `input.rs` chama o que está em `windowing.rs` só
    /// pelo nome da função, e `commands/afk.rs` chama o que está em qualquer
    /// `commands/*.rs` do mesmo jeito. Por isso o módulo inteiro entra na
    /// varredura, e não só o arquivo que cita `afk` ou `SendInput`.
    #[test]
    fn every_fragment_of_a_module_with_afk_code_is_scanned() {
        let scanned: Vec<String> = afk_files().into_iter().map(|(path, _)| path).collect();
        for fragment in [
            "platform/windows.rs",
            "platform/windows/windowing.rs",
            "platform/windows/input.rs",
            "platform/windows/tracker.rs",
            "lib.rs",
            "commands/afk.rs",
            "commands/watcher.rs",
        ] {
            assert!(
                scanned.iter().any(|path| path == fragment),
                "{fragment} ficou fora da varredura do AFK mode: {scanned:?}"
            );
        }
        // Módulo de verdade, com fronteira própria, continua fora: é para ele
        // que vale a checagem de alcance indireto.
        assert!(!scanned.iter().any(|path| path == "webview_recovery.rs"));
    }

    #[test]
    fn only_the_input_module_sends_input() {
        let senders: Vec<String> = backend_files()
            .into_iter()
            .map(|(path, text)| (path, production_only(&text)))
            .filter(|(_, body)| body.contains("SendInput(") || body.contains("send_key("))
            .map(|(path, _)| path)
            .collect();
        assert_eq!(
            senders,
            vec!["platform/windows/input.rs".to_string()],
            "o caminho até o SendInput tem de continuar sendo um só"
        );
    }

    #[test]
    fn the_scan_sees_code_after_the_test_modules() {
        // `AFK_SCAN_TAIL_MARKER` mora no fim de commands/afk.rs, **depois** dos
        // módulos de teste: corte ingênuo no primeiro `#[cfg(test)]` deixaria
        // esse trecho de produção fora da varredura sem ninguém notar.
        let afk = backend_files()
            .into_iter()
            .find(|(path, _)| path == "commands/afk.rs")
            .map(|(_, text)| text)
            .expect("commands/afk.rs tem de existir");
        let body = production_only(&afk);

        assert!(body.contains("fn afk_virtual_key"), "produção do começo sumiu");
        assert!(
            body.contains("AFK_SCAN_TAIL_MARKER"),
            "a varredura não enxerga o código depois dos módulos de teste"
        );
        assert!(
            !body.contains("mod afk_command_tests"),
            "os módulos de teste continuam no texto varrido"
        );
        assert!(
            body.len() > 1_000,
            "li quase nada de commands/afk.rs ({} bytes)",
            body.len()
        );
    }

    #[test]
    fn the_scan_really_walks_the_backend() {
        let files = backend_files();
        assert!(
            files.len() > 40,
            "a caminhada achou {} arquivos, o backend tem muito mais",
            files.len()
        );
        assert!(files.iter().any(|(path, _)| path == "commands/afk.rs"));
        assert!(files
            .iter()
            .any(|(path, _)| path == "platform/windows/input.rs"));
    }
}

/// Marcador de fim de arquivo. Mora **depois** dos módulos de teste de propósito:
/// é ele que prova, em `the_scan_sees_code_after_the_test_modules`, que a
/// varredura de segurança enxerga código de produção escrito abaixo dos testes.
#[allow(dead_code)]
const AFK_SCAN_TAIL_MARKER: &str = "afk-scan-tail";
