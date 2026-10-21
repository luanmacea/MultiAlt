// As primitivas: a única porta do Python para o app
// (docs/features/python-automation.md, "Primitivas e travas").
//
// Os blocos padrão (Python, editáveis) decidem a ordem das coisas; as travas
// moram **aqui** e valem com qualquer bloco reescrito:
// - só as contas da execução, e só cliente aberto pelo MultiAlt (tracker);
// - entrada só com o lease da fila de foco desta execução para esta conta, com
//   a janela não minimizada e **na frente** — nessa ordem de conferência;
// - cookie, senha, token e qualquer op fora da lista: `forbidden`;
// - nada aqui fecha, mata ou minimiza cliente.
// Este arquivo não chama a API de envio de entrada: tecla e clique saem pelas
// portas do `platform/windows/input.rs`, atrás do `Desk`.

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use super::errors::PyError;
use super::focus::FocusQueue;
use super::types::{AccountView, PythonLogLine};

/// Para onde vão log, status e dashboard de uma execução.
pub trait RunSink: Send + Sync {
    fn log(&self, line: PythonLogLine);
    fn status(&self, user_id: Option<i64>, text: &str);
    fn dashboard(&self, op: &str, args: &Value) -> Result<Value, PyError>;
}

/// O que o app sabe agora do cliente de uma conta (`accounts` lê isto a cada
/// chamada): aberto pelo MultiAlt e no jogo, e o lugar/servidor do último
/// "Joining game" do log do Roblox (0 / "" quando o log ainda não disse).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LiveSession {
    pub in_game: bool,
    pub place_id: i64,
    pub job_id: String,
}

/// O que as primitivas usam do Windows — o real no app, um dublê nos testes.
/// Janelas como `isize` (o HWND).
pub trait Desk: Send + Sync {
    /// Janela do cliente **rastreado** da conta (`None` sem cliente do app).
    fn window_of(&self, user_id: i64) -> Option<isize>;
    /// Área interna em pixels de tela: `(x, y, largura, altura)`.
    fn client_rect(&self, hwnd: isize) -> Option<(i32, i32, i32, i32)>;
    fn dpi(&self, hwnd: isize) -> u32;
    fn is_minimized(&self, hwnd: isize) -> bool;
    fn foreground(&self) -> isize;
    /// Traz para frente e confere.
    fn bring_forward(&self, hwnd: isize) -> bool;
    /// Devolve o foco a `target`: "restored" | "user_moved_on" | "gone" | "denied".
    fn give_focus_back(&self, target: isize, cycle: &[isize]) -> &'static str;
    /// Aperta (`up = false`) ou solta uma tecla da lista das Gravações.
    fn key(&self, key: &str, up: bool) -> bool;
    /// Clique esquerdo no ponto (frações 0–1 da área interna).
    fn click(&self, hwnd: isize, x: f64, y: f64, focus_click: bool) -> Result<(), String>;
    /// `(largura, altura, BGRA)` da área interna.
    fn capture(&self, hwnd: isize) -> Result<(u32, u32, Vec<u8>), String>;
    /// O estado de agora do cliente da conta.
    fn session(&self, user_id: i64) -> LiveSession;
}

/// A conta como o script a vê agora (o resto vem do começo da execução).
pub fn live_view(desk: &dyn Desk, account: &AccountView) -> AccountView {
    let live = desk.session(account.user_id);
    AccountView {
        in_game: live.in_game,
        place_id: live.place_id,
        job_id: live.job_id,
        ..account.clone()
    }
}

/// Estado de uma execução que as primitivas precisam lembrar entre chamadas.
pub struct RunState {
    /// Contas que acabaram de ser trazidas para frente: o próximo clique delas
    /// leva o clique de foco.
    focus_click_pending: HashSet<i64>,
    /// A janela que estava na frente antes do primeiro `bring_forward`.
    return_to: Option<isize>,
    /// As janelas que a execução trouxe para frente desde então.
    cycle: Vec<isize>,
    frame_seq: u64,
    #[cfg(target_os = "windows")]
    frames: VecDeque<crate::platform::windows::SharedFrame>,
    #[cfg(not(target_os = "windows"))]
    frames: VecDeque<()>,
    /// As linhas "Focus taken/released" saem no máximo uma vez por isto.
    pub focus_log_every: Duration,
    focus_logged_at: Option<std::time::Instant>,
    /// Vezes que a fila foi pega sem linha no terminal (contadas na próxima).
    focus_not_shown: u32,
    /// O lease cuja vez saiu no terminal (a soltura dele sai também).
    focus_logged_lease: Option<(String, std::time::Instant)>,
}

impl Default for RunState {
    fn default() -> Self {
        Self {
            focus_click_pending: HashSet::new(),
            return_to: None,
            cycle: Vec::new(),
            frame_seq: 0,
            frames: VecDeque::new(),
            focus_log_every: Duration::from_secs(1),
            focus_logged_at: None,
            focus_not_shown: 0,
            focus_logged_lease: None,
        }
    }
}

impl RunState {
    /// A linha da vez pega, se esta vez aparece no terminal.
    fn focus_taken_line(&mut self, lease: &str) -> Option<String> {
        let now = std::time::Instant::now();
        let due = self
            .focus_logged_at
            .is_none_or(|at| now.saturating_duration_since(at) >= self.focus_log_every);
        if !due {
            self.focus_not_shown += 1;
            return None;
        }
        self.focus_logged_at = Some(now);
        self.focus_logged_lease = Some((lease.to_string(), now));
        let skipped = std::mem::take(&mut self.focus_not_shown);
        Some(match skipped {
            0 => "Focus taken.".to_string(),
            1 => "Focus taken (1 turn not shown).".to_string(),
            n => format!("Focus taken ({n} turns not shown)."),
        })
    }

    /// A linha da soltura, se a vez dela apareceu.
    fn focus_released_line(&mut self, lease: &str, keys: &[String]) -> Option<String> {
        match &self.focus_logged_lease {
            Some((id, at)) if id == lease => {
                let ms = at.elapsed().as_millis();
                self.focus_logged_lease = None;
                Some(if keys.is_empty() {
                    format!("Focus released after {ms} ms.")
                } else {
                    format!("Focus released after {ms} ms (released keys: {}).", keys.join(", "))
                })
            }
            _ => None,
        }
    }
}

fn focus_line(ctx: &RunCtx, user_id: Option<i64>, message: String) {
    ctx.sink.log(PythonLogLine {
        run_id: Some(ctx.run_id.clone()),
        user_id,
        level: "system".into(),
        message,
        code: Some("focus".into()),
        hint: None,
        ts: super::types::now_ms(),
    });
}

/// Quantos quadros de captura ficam vivos por execução (no modo "todas juntas"
/// duas threads podem capturar ao mesmo tempo; o Python copia e fecha).
const FRAMES_KEPT: usize = 4;

pub struct RunCtx {
    pub run_id: String,
    pub accounts: Vec<AccountView>,
    pub stop: Arc<AtomicBool>,
    pub sink: Arc<dyn RunSink>,
    pub desk: Arc<dyn Desk>,
    pub focus: Arc<FocusQueue>,
    pub state: Arc<Mutex<RunState>>,
}

impl RunCtx {
    pub fn new(
        run_id: impl Into<String>,
        accounts: Vec<AccountView>,
        stop: Arc<AtomicBool>,
        sink: Arc<dyn RunSink>,
        desk: Arc<dyn Desk>,
        focus: Arc<FocusQueue>,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            accounts,
            stop,
            sink,
            desk,
            focus,
            state: Arc::new(Mutex::new(RunState::default())),
        }
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    fn account(&self, user_id: i64) -> Option<&AccountView> {
        self.accounts.iter().find(|a| a.user_id == user_id)
    }

    /// Fim da execução: solta o lease, as teclas e os quadros.
    pub fn finish(&self) -> Vec<String> {
        let released = self.focus.release_all_for_run(&self.run_id, self.desk.as_ref());
        if let Ok(mut state) = self.state.lock() {
            state.frames.clear();
        }
        released
    }
}

/// Toda op que existe. O resto é `forbidden`.
pub const OPS: &[&str] = &[
    "accounts",
    "windows",
    "focus.acquire",
    "focus.renew",
    "focus.release",
    "window.bring_forward",
    "window.restore_focus",
    "input.key",
    "input.key_down",
    "input.key_up",
    "input.type_text",
    "input.click",
    "screen.pixel",
    "screen.capture",
    "run.sleep",
    "run.stopped",
    "run.log",
    "run.status",
    "dashboard.define",
    "dashboard.set",
    "dashboard.push",
    "dashboard.clear",
];

fn is_forbidden(op: &str) -> bool {
    let lower = op.to_ascii_lowercase();
    ["cookie", "password", "token", "secret"].iter().any(|w| lower.contains(w)) || !OPS.contains(&op)
}

// ── leitura de argumentos ─────────────────────────────────────────────────

fn arg_i64(op: &str, args: &Value, key: &str) -> Result<i64, PyError> {
    args.get(key)
        .and_then(|v| v.as_i64().or_else(|| v.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64)))
        .ok_or_else(|| PyError::bad_args(op, &format!("{key} must be an integer")))
}

fn arg_u64_or(op: &str, args: &Value, key: &str, default: u64, max: u64) -> Result<u64, PyError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => {
            let n = v
                .as_u64()
                .or_else(|| v.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64))
                .ok_or_else(|| PyError::bad_args(op, &format!("{key} must be a non-negative number")))?;
            if n > max {
                return Err(PyError::bad_args(op, &format!("{key} must be at most {max}")));
            }
            Ok(n)
        }
    }
}

fn arg_frac(op: &str, args: &Value, key: &str) -> Result<f64, PyError> {
    let v = args
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| PyError::bad_args(op, &format!("{key} must be a number")))?;
    if !v.is_finite() || !(0.0..=1.0).contains(&v) {
        return Err(PyError::bad_args(
            op,
            &format!("{key} must be between 0 and 1 (a fraction of the window), got {v}"),
        ));
    }
    Ok(v)
}

fn arg_str<'a>(op: &str, args: &'a Value, key: &str) -> Result<&'a str, PyError> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| PyError::bad_args(op, &format!("{key} must be a string")))
}

fn name_for(ctx: &RunCtx, user_id: i64) -> String {
    ctx.account(user_id)
        .map(|a| a.name.clone())
        .unwrap_or_else(|| format!("account {user_id}"))
}

/// A conta é desta execução? Conta de fora é tratada como sem cliente (o script
/// não alcança conta que o usuário não escolheu).
fn member(ctx: &RunCtx, user_id: i64) -> Result<&AccountView, PyError> {
    ctx.account(user_id)
        .ok_or_else(|| PyError::untracked_window(&format!("account {user_id}")))
}

fn tracked_window(ctx: &RunCtx, user_id: i64) -> Result<isize, PyError> {
    let account = member(ctx, user_id)?;
    ctx.desk
        .window_of(user_id)
        .ok_or_else(|| PyError::untracked_window(&account.name))
}

/// A trava da entrada, na ordem do contrato: lease → janela rastreada → não
/// minimizada → na frente.
fn input_gate(ctx: &RunCtx, user_id: i64) -> Result<isize, PyError> {
    let account = member(ctx, user_id)?;
    if !ctx.focus.holds(&ctx.run_id, user_id) {
        return Err(PyError::no_focus_lease());
    }
    let hwnd = ctx
        .desk
        .window_of(user_id)
        .ok_or_else(|| PyError::untracked_window(&account.name))?;
    if ctx.desk.is_minimized(hwnd) {
        return Err(PyError::minimized(&account.name));
    }
    if ctx.desk.foreground() != hwnd {
        return Err(PyError::not_foreground(&account.name));
    }
    Ok(hwnd)
}

fn canonical_key(key: &str) -> Result<String, PyError> {
    crate::data::recordings::RECORDING_KEYS
        .iter()
        .find(|(name, _, _)| name.eq_ignore_ascii_case(key))
        .map(|(name, _, _)| (*name).to_string())
        .ok_or_else(|| PyError::unsupported_key(key))
}

/// As teclas de um caractere de `type_text`: letra (maiúscula com Shift),
/// dígito e espaço. O resto não tem tecla na lista fechada.
fn keys_for_char(c: char) -> Option<Vec<&'static str>> {
    let find = |name: &str| {
        crate::data::recordings::RECORDING_KEYS
            .iter()
            .find(|(n, _, _)| n.eq_ignore_ascii_case(name))
            .map(|(n, _, _)| *n)
    };
    if c == ' ' {
        return find("Space").map(|k| vec![k]);
    }
    if c.is_ascii_digit() || c.is_ascii_lowercase() {
        return find(&c.to_string()).map(|k| vec![k]);
    }
    if c.is_ascii_uppercase() {
        let key = find(&c.to_string())?;
        return Some(vec!["Shift", key]);
    }
    None
}

async fn sleep_or_stop(ctx: &RunCtx, ms: u64) -> Result<(), PyError> {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
    loop {
        if ctx.stopped() {
            return Err(PyError::stopped());
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Ok(());
        }
        tokio::time::sleep((deadline - now).min(Duration::from_millis(25))).await;
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    match tokio::task::spawn_blocking(f).await {
        Ok(v) => v,
        Err(e) => std::panic::resume_unwind(e.into_panic()),
    }
}

/// Recorte (frações) da captura inteira.
fn crop(w: u32, h: u32, pixels: &[u8], region: Option<(f64, f64, f64, f64)>) -> (u32, u32, Vec<u8>) {
    let Some((rx, ry, rw, rh)) = region else {
        return (w, h, pixels.to_vec());
    };
    let x0 = ((rx * w as f64).floor() as u32).min(w.saturating_sub(1));
    let y0 = ((ry * h as f64).floor() as u32).min(h.saturating_sub(1));
    let cw = ((rw * w as f64).round() as u32).clamp(1, w - x0);
    let ch = ((rh * h as f64).round() as u32).clamp(1, h - y0);
    let mut out = Vec::with_capacity((cw * ch * 4) as usize);
    for row in y0..y0 + ch {
        let start = ((row * w + x0) * 4) as usize;
        out.extend_from_slice(&pixels[start..start + (cw * 4) as usize]);
    }
    (cw, ch, out)
}

fn pixel_at(w: u32, h: u32, pixels: &[u8], x: f64, y: f64) -> [u8; 3] {
    let px = ((x * w as f64).floor() as u32).min(w.saturating_sub(1));
    let py = ((y * h as f64).floor() as u32).min(h.saturating_sub(1));
    let i = ((py * w + px) * 4) as usize;
    // BGRA -> RGB
    [pixels[i + 2], pixels[i + 1], pixels[i]]
}

/// Ops que valem mesmo depois do Parar: soltar o que está preso e conversar.
fn allowed_after_stop(op: &str) -> bool {
    matches!(
        op,
        "run.stopped" | "run.log" | "run.status" | "focus.release" | "input.key_up" | "window.restore_focus" | "accounts"
    )
}

/// Executa uma op. O que volta vai como `result` (ou `error`) da resposta.
pub async fn dispatch(ctx: &RunCtx, op: &str, args: Value) -> Result<Value, PyError> {
    if is_forbidden(op) {
        return Err(PyError::forbidden());
    }
    if ctx.stopped() && !allowed_after_stop(op) {
        return Err(PyError::stopped());
    }
    let args = if args.is_null() { json!({}) } else { args };
    if !args.is_object() {
        return Err(PyError::bad_args(op, "args must be an object"));
    }
    match op {
        "accounts" => {
            let live: Vec<AccountView> = ctx.accounts.iter().map(|a| live_view(ctx.desk.as_ref(), a)).collect();
            Ok(serde_json::to_value(&live).unwrap_or_default())
        }
        "windows" => {
            let front = ctx.desk.foreground();
            let list: Vec<Value> = ctx
                .accounts
                .iter()
                .filter_map(|a| {
                    let hwnd = ctx.desk.window_of(a.user_id)?;
                    let (x, y, width, height) = ctx.desk.client_rect(hwnd).unwrap_or((0, 0, 0, 0));
                    Some(json!({
                        "user_id": a.user_id,
                        "hwnd": hwnd,
                        "x": x, "y": y, "width": width, "height": height,
                        "dpi": ctx.desk.dpi(hwnd),
                        "minimized": ctx.desk.is_minimized(hwnd),
                        "foreground": front == hwnd,
                    }))
                })
                .collect();
            Ok(Value::Array(list))
        }
        "focus.acquire" => {
            let user_id = arg_i64(op, &args, "user_id")?;
            member(ctx, user_id)?;
            let timeout = arg_u64_or(op, &args, "timeout_ms", 20_000, 600_000)?;
            let lease = ctx.focus.acquire(&ctx.run_id, user_id, timeout, &ctx.stop).await?;
            let line = ctx.state.lock().unwrap().focus_taken_line(&lease);
            if let Some(line) = line {
                focus_line(ctx, Some(user_id), line);
            }
            Ok(json!({ "lease": lease }))
        }
        "focus.renew" => {
            let lease = arg_str(op, &args, "lease")?;
            ctx.focus.renew(&ctx.run_id, lease)?;
            Ok(json!(true))
        }
        "focus.release" => {
            let lease = arg_str(op, &args, "lease")?;
            if let Some((user_id, keys)) = ctx.focus.release(&ctx.run_id, lease, ctx.desk.as_ref()) {
                let line = ctx.state.lock().unwrap().focus_released_line(lease, &keys);
                if let Some(line) = line {
                    focus_line(ctx, Some(user_id), line);
                }
            }
            Ok(json!(true))
        }
        "window.bring_forward" => {
            let user_id = arg_i64(op, &args, "user_id")?;
            let account = member(ctx, user_id)?.clone();
            if !ctx.focus.holds(&ctx.run_id, user_id) {
                return Err(PyError::no_focus_lease());
            }
            let hwnd = tracked_window(ctx, user_id)?;
            let before = ctx.desk.foreground();
            let desk = ctx.desk.clone();
            let ok = blocking(move || desk.bring_forward(hwnd)).await;
            {
                let mut state = ctx.state.lock().unwrap();
                if state.return_to.is_none() && before != 0 && !state.cycle.contains(&before) && before != hwnd {
                    state.return_to = Some(before);
                }
                if !state.cycle.contains(&hwnd) {
                    state.cycle.push(hwnd);
                }
                state.focus_click_pending.insert(user_id);
            }
            if !ok {
                return Err(PyError::not_foreground(&account.name));
            }
            Ok(json!(true))
        }
        "window.restore_focus" => {
            // Só com a fila livre (e segurando-a até o fim): com alguém na vez
            // ou esperando, a volta fica para quem soltar por último.
            let Some(queue_guard) = ctx.focus.hold_if_free() else {
                return Ok(json!("denied"));
            };
            let (target, cycle) = {
                let mut state = ctx.state.lock().unwrap();
                let target = state.return_to.take();
                let cycle = std::mem::take(&mut state.cycle);
                (target, cycle)
            };
            let Some(target) = target else {
                return Ok(json!("restored"));
            };
            let desk = ctx.desk.clone();
            let outcome = blocking(move || desk.give_focus_back(target, &cycle)).await;
            drop(queue_guard);
            Ok(json!(outcome))
        }
        "input.key" => {
            let user_id = arg_i64(op, &args, "user_id")?;
            let key = canonical_key(arg_str(op, &args, "key")?)?;
            let hold = arg_u64_or(op, &args, "hold_ms", 50, 10_000)?;
            input_gate(ctx, user_id)?;
            if !ctx.desk.key(&key, false) {
                return Err(PyError::bad_args(op, "Windows refused the key"));
            }
            ctx.focus.note_key_down(&ctx.run_id, &key);
            // Parar no meio do toque ainda solta a tecla.
            let _ = sleep_or_stop(ctx, hold).await;
            ctx.desk.key(&key, true);
            ctx.focus.note_key_up(&ctx.run_id, &key);
            Ok(json!(true))
        }
        "input.key_down" => {
            let user_id = arg_i64(op, &args, "user_id")?;
            let key = canonical_key(arg_str(op, &args, "key")?)?;
            input_gate(ctx, user_id)?;
            if !ctx.desk.key(&key, false) {
                return Err(PyError::bad_args(op, "Windows refused the key"));
            }
            ctx.focus.note_key_down(&ctx.run_id, &key);
            Ok(json!(true))
        }
        "input.key_up" => {
            let user_id = arg_i64(op, &args, "user_id")?;
            let key = canonical_key(arg_str(op, &args, "key")?)?;
            input_gate(ctx, user_id)?;
            ctx.desk.key(&key, true);
            ctx.focus.note_key_up(&ctx.run_id, &key);
            Ok(json!(true))
        }
        "input.type_text" => {
            let user_id = arg_i64(op, &args, "user_id")?;
            let text = arg_str(op, &args, "text")?.to_string();
            let delay = arg_u64_or(op, &args, "delay_ms", 30, 5_000)?;
            if text.chars().count() > 2_000 {
                return Err(PyError::bad_args(op, "text is longer than 2000 characters"));
            }
            // Confere tudo antes de mandar qualquer tecla.
            let mut plan = Vec::new();
            for c in text.chars() {
                plan.push(keys_for_char(c).ok_or_else(|| PyError::unsupported_key(&c.to_string()))?);
            }
            input_gate(ctx, user_id)?;
            for (i, keys) in plan.iter().enumerate() {
                if i > 0 {
                    sleep_or_stop(ctx, delay).await?;
                    input_gate(ctx, user_id)?;
                }
                for key in keys {
                    ctx.desk.key(key, false);
                }
                tokio::time::sleep(Duration::from_millis(15)).await;
                for key in keys.iter().rev() {
                    ctx.desk.key(key, true);
                }
            }
            Ok(json!(true))
        }
        "input.click" => {
            let user_id = arg_i64(op, &args, "user_id")?;
            let x = arg_frac(op, &args, "x")?;
            let y = arg_frac(op, &args, "y")?;
            let hwnd = input_gate(ctx, user_id)?;
            let focus_click = ctx.state.lock().unwrap().focus_click_pending.remove(&user_id);
            let desk = ctx.desk.clone();
            blocking(move || desk.click(hwnd, x, y, focus_click))
                .await
                .map_err(|e| PyError::bad_args(op, &e))?;
            Ok(json!(true))
        }
        "screen.pixel" => {
            let user_id = arg_i64(op, &args, "user_id")?;
            let x = arg_frac(op, &args, "x")?;
            let y = arg_frac(op, &args, "y")?;
            let (w, h, pixels) = capture_for(ctx, user_id).await?;
            Ok(json!(pixel_at(w, h, &pixels, x, y)))
        }
        "screen.capture" => {
            let user_id = arg_i64(op, &args, "user_id")?;
            let region = match args.get("region") {
                None | Some(Value::Null) => None,
                Some(r) => Some((
                    arg_frac(op, r, "x")?,
                    arg_frac(op, r, "y")?,
                    arg_frac(op, r, "w")?,
                    arg_frac(op, r, "h")?,
                )),
            };
            let (w, h, pixels) = capture_for(ctx, user_id).await?;
            let (cw, ch, bytes) = crop(w, h, &pixels, region);
            let name = publish_frame(ctx, &bytes).map_err(|e| PyError::capture_failed(&name_for(ctx, user_id), &e))?;
            Ok(json!({ "shm": name, "width": cw, "height": ch, "stride": cw * 4, "format": "BGRA" }))
        }
        "run.sleep" => {
            let ms = arg_u64_or(op, &args, "ms", 0, 24 * 3_600_000)?;
            sleep_or_stop(ctx, ms).await?;
            Ok(json!(true))
        }
        "run.stopped" => Ok(json!(ctx.stopped())),
        "run.log" => {
            let level = match args.get("level").and_then(Value::as_str).unwrap_or("log") {
                "warn" => "warn",
                "error" => "error",
                _ => "log",
            };
            let message = args.get("message").and_then(Value::as_str).unwrap_or("").to_string();
            let user_id = args.get("user_id").and_then(Value::as_i64);
            ctx.sink.log(PythonLogLine {
                run_id: Some(ctx.run_id.clone()),
                user_id,
                level: level.into(),
                message,
                code: None,
                hint: None,
                ts: super::types::now_ms(),
            });
            Ok(Value::Null)
        }
        "run.status" => {
            let text = args.get("text").and_then(Value::as_str).unwrap_or("");
            let user_id = args.get("user_id").and_then(Value::as_i64);
            ctx.sink.status(user_id, text);
            Ok(Value::Null)
        }
        op if op.starts_with("dashboard.") => ctx.sink.dashboard(op, &args),
        _ => Err(PyError::forbidden()),
    }
}

async fn capture_for(ctx: &RunCtx, user_id: i64) -> Result<(u32, u32, Vec<u8>), PyError> {
    let account = member(ctx, user_id)?.clone();
    let hwnd = tracked_window(ctx, user_id)?;
    if ctx.desk.is_minimized(hwnd) {
        return Err(PyError::minimized(&account.name));
    }
    let desk = ctx.desk.clone();
    let (w, h, pixels) = blocking(move || desk.capture(hwnd))
        .await
        .map_err(|e| PyError::capture_failed(&account.name, &e))?;
    if w == 0 || h == 0 || pixels.len() < (w * h * 4) as usize {
        return Err(PyError::capture_failed(&account.name, "empty image"));
    }
    Ok((w, h, pixels))
}

#[cfg(target_os = "windows")]
fn publish_frame(ctx: &RunCtx, bytes: &[u8]) -> Result<String, String> {
    let mut state = ctx.state.lock().unwrap();
    state.frame_seq += 1;
    let safe_run: String = ctx.run_id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    let name = format!("Local\\multialt-cap-{}-{}-{}", std::process::id(), safe_run, state.frame_seq);
    let frame = crate::platform::windows::SharedFrame::create(&name, bytes)?;
    state.frames.push_back(frame);
    while state.frames.len() > FRAMES_KEPT {
        state.frames.pop_front();
    }
    Ok(name)
}

#[cfg(not(target_os = "windows"))]
fn publish_frame(_ctx: &RunCtx, _bytes: &[u8]) -> Result<String, String> {
    Err("screen capture is only available on Windows".into())
}

// ── o Desk de verdade ─────────────────────────────────────────────────────

/// O Windows de verdade: tracker, janelas e as portas do `input.rs`.
pub struct RealDesk;

#[cfg(target_os = "windows")]
impl Desk for RealDesk {
    fn window_of(&self, user_id: i64) -> Option<isize> {
        use crate::platform::windows;
        windows::tracker()
            .get_pid(user_id)
            .and_then(windows::find_main_window)
            .map(|h| h as isize)
    }
    fn client_rect(&self, hwnd: isize) -> Option<(i32, i32, i32, i32)> {
        crate::platform::windows::client_rect_on_screen(hwnd as _)
            .map(|r| (r.left, r.top, r.width, r.height))
    }
    fn dpi(&self, hwnd: isize) -> u32 {
        crate::platform::windows::window_dpi(hwnd as _)
    }
    fn is_minimized(&self, hwnd: isize) -> bool {
        crate::platform::windows::window_is_minimized(hwnd as _)
    }
    fn foreground(&self) -> isize {
        crate::platform::windows::get_foreground_hwnd() as isize
    }
    fn bring_forward(&self, hwnd: isize) -> bool {
        crate::platform::windows::bring_forward_for_cycle(hwnd as _)
    }
    fn give_focus_back(&self, target: isize, cycle: &[isize]) -> &'static str {
        use crate::platform::windows::GiveBack;
        let cycle: Vec<_> = cycle.iter().map(|h| *h as _).collect();
        match crate::platform::windows::give_focus_back_after_cycle(target as _, &cycle) {
            GiveBack::Restored => "restored",
            GiveBack::UserMovedOn => "user_moved_on",
            GiveBack::Gone => "gone",
            GiveBack::Denied => "denied",
        }
    }
    fn key(&self, key: &str, up: bool) -> bool {
        crate::platform::windows::press_recording_key(key, up)
    }
    fn click(&self, hwnd: isize, x: f64, y: f64, focus_click: bool) -> Result<(), String> {
        let point = crate::AfkPoint::clamped(x * 100.0, y * 100.0);
        crate::platform::windows::click_recording_point(hwnd as _, point, 40, focus_click)
    }
    fn capture(&self, hwnd: isize) -> Result<(u32, u32, Vec<u8>), String> {
        crate::platform::windows::capture_client_bgra(hwnd as _)
    }
    fn session(&self, user_id: i64) -> LiveSession {
        // Só leitura do que o monitor de saúde já sabe (log do Roblox): nada
        // novo é lido por causa disto.
        let Some(pid) = crate::platform::windows::tracker().get_pid(user_id) else {
            return LiveSession::default();
        };
        let health = crate::client_health_of(user_id, pid);
        let destination = health.as_ref().and_then(|h| h.destination.clone());
        LiveSession {
            // Sem o log ainda, o cliente aberto pelo app conta como no jogo.
            in_game: health.as_ref().map(|h| h.in_game || !h.log_found).unwrap_or(true),
            place_id: destination.as_ref().map(|d| d.place_id).unwrap_or(0),
            job_id: destination.and_then(|d| d.job_id).unwrap_or_default(),
        }
    }
}

#[cfg(not(target_os = "windows"))]
impl Desk for RealDesk {
    fn window_of(&self, _user_id: i64) -> Option<isize> {
        None
    }
    fn client_rect(&self, _hwnd: isize) -> Option<(i32, i32, i32, i32)> {
        None
    }
    fn dpi(&self, _hwnd: isize) -> u32 {
        96
    }
    fn is_minimized(&self, _hwnd: isize) -> bool {
        false
    }
    fn foreground(&self) -> isize {
        0
    }
    fn bring_forward(&self, _hwnd: isize) -> bool {
        false
    }
    fn give_focus_back(&self, _target: isize, _cycle: &[isize]) -> &'static str {
        "denied"
    }
    fn key(&self, _key: &str, _up: bool) -> bool {
        false
    }
    fn click(&self, _hwnd: isize, _x: f64, _y: f64, _focus_click: bool) -> Result<(), String> {
        Err("input is only available on Windows".into())
    }
    fn capture(&self, _hwnd: isize) -> Result<(u32, u32, Vec<u8>), String> {
        Err("capture is only available on Windows".into())
    }
    fn session(&self, _user_id: i64) -> LiveSession {
        LiveSession::default()
    }
}

#[cfg(test)]
pub mod python_primitives_tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::Instant;

    /// O dublê do Windows: janelas por conta, quem está na frente, e o que foi
    /// "enviado".
    #[derive(Default)]
    pub struct FakeDesk {
        pub windows: Mutex<HashMap<i64, isize>>,
        pub minimized: Mutex<HashSet<isize>>,
        pub front: Mutex<isize>,
        pub sent: Mutex<Vec<String>>,
        pub image: Mutex<Option<(u32, u32, Vec<u8>)>>,
        pub sessions: Mutex<HashMap<i64, LiveSession>>,
    }

    impl FakeDesk {
        pub fn with_window(user_id: i64, hwnd: isize) -> Arc<Self> {
            let desk = FakeDesk::default();
            desk.windows.lock().unwrap().insert(user_id, hwnd);
            Arc::new(desk)
        }
        pub fn sent(&self) -> Vec<String> {
            self.sent.lock().unwrap().clone()
        }
    }

    impl Desk for FakeDesk {
        fn window_of(&self, user_id: i64) -> Option<isize> {
            self.windows.lock().unwrap().get(&user_id).copied()
        }
        fn client_rect(&self, _hwnd: isize) -> Option<(i32, i32, i32, i32)> {
            Some((100, 100, 800, 600))
        }
        fn dpi(&self, _hwnd: isize) -> u32 {
            96
        }
        fn is_minimized(&self, hwnd: isize) -> bool {
            self.minimized.lock().unwrap().contains(&hwnd)
        }
        fn foreground(&self) -> isize {
            *self.front.lock().unwrap()
        }
        fn bring_forward(&self, hwnd: isize) -> bool {
            *self.front.lock().unwrap() = hwnd;
            self.minimized.lock().unwrap().remove(&hwnd);
            true
        }
        fn give_focus_back(&self, target: isize, _cycle: &[isize]) -> &'static str {
            *self.front.lock().unwrap() = target;
            "restored"
        }
        fn key(&self, key: &str, up: bool) -> bool {
            self.sent.lock().unwrap().push(format!("{}{key}", if up { "up:" } else { "down:" }));
            true
        }
        fn click(&self, hwnd: isize, x: f64, y: f64, focus_click: bool) -> Result<(), String> {
            self.sent.lock().unwrap().push(format!("click:{hwnd}:{x:.2}:{y:.2}:{focus_click}"));
            Ok(())
        }
        fn capture(&self, _hwnd: isize) -> Result<(u32, u32, Vec<u8>), String> {
            self.image.lock().unwrap().clone().ok_or_else(|| "no image".to_string())
        }
        fn session(&self, user_id: i64) -> LiveSession {
            if self.window_of(user_id).is_none() {
                return LiveSession::default();
            }
            self.sessions.lock().unwrap().get(&user_id).cloned().unwrap_or(LiveSession {
                in_game: true,
                ..Default::default()
            })
        }
    }

    #[derive(Default)]
    pub struct NullSink {
        pub lines: Mutex<Vec<PythonLogLine>>,
    }
    impl RunSink for NullSink {
        fn log(&self, line: PythonLogLine) {
            self.lines.lock().unwrap().push(line);
        }
        fn status(&self, _user_id: Option<i64>, _text: &str) {}
        fn dashboard(&self, _op: &str, _args: &Value) -> Result<Value, PyError> {
            Ok(json!(true))
        }
    }

    pub fn account(user_id: i64, name: &str) -> AccountView {
        AccountView {
            user_id,
            name: name.into(),
            alias: String::new(),
            group: "Default".into(),
            in_game: true,
            place_id: 0,
            job_id: String::new(),
        }
    }

    pub fn own_queue() -> Arc<FocusQueue> {
        let lock: &'static tokio::sync::Mutex<()> = Box::leak(Box::new(tokio::sync::Mutex::new(())));
        Arc::new(FocusQueue::new(lock))
    }

    pub fn ctx_with(desk: Arc<FakeDesk>, queue: Arc<FocusQueue>) -> RunCtx {
        RunCtx::new(
            "r1",
            vec![account(1, "Main_01"), account(2, "Alt_02")],
            Arc::new(AtomicBool::new(false)),
            Arc::new(NullSink::default()),
            desk,
            queue,
        )
    }

    async fn lease(ctx: &RunCtx, user_id: i64) -> String {
        dispatch(ctx, "focus.acquire", json!({"user_id": user_id, "timeout_ms": 1000}))
            .await
            .unwrap()["lease"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn code(r: Result<Value, PyError>) -> &'static str {
        r.expect_err("expected an error").code.as_str()
    }

    #[tokio::test]
    async fn input_without_lease_is_blocked() {
        let desk = FakeDesk::with_window(1, 11);
        *desk.front.lock().unwrap() = 11;
        let ctx = ctx_with(desk.clone(), own_queue());
        for (op, args) in [
            ("input.key", json!({"user_id": 1, "key": "Space"})),
            ("input.key_down", json!({"user_id": 1, "key": "W"})),
            ("input.click", json!({"user_id": 1, "x": 0.5, "y": 0.5})),
            ("input.type_text", json!({"user_id": 1, "text": "hi"})),
            ("window.bring_forward", json!({"user_id": 1})),
        ] {
            assert_eq!(code(dispatch(&ctx, op, args).await), "no_focus_lease", "{op}");
        }
        assert!(desk.sent().is_empty(), "nothing may be sent without the lease");
        // Lease de uma conta não vale para a outra.
        desk.windows.lock().unwrap().insert(2, 22);
        lease(&ctx, 1).await;
        *desk.front.lock().unwrap() = 22;
        assert_eq!(code(dispatch(&ctx, "input.key", json!({"user_id": 2, "key": "Space"})).await), "no_focus_lease");
    }

    #[tokio::test]
    async fn input_to_background_window_is_blocked() {
        let desk = FakeDesk::with_window(1, 11);
        *desk.front.lock().unwrap() = 99; // a janela do usuário
        let ctx = ctx_with(desk.clone(), own_queue());
        let id = lease(&ctx, 1).await;
        let error = dispatch(&ctx, "input.key", json!({"user_id": 1, "key": "Space"})).await.unwrap_err();
        assert_eq!(error.code.as_str(), "not_foreground");
        assert!(error.message.contains("Main_01"));
        assert_eq!(code(dispatch(&ctx, "input.click", json!({"user_id": 1, "x": 0.1, "y": 0.1})).await), "not_foreground");
        assert!(desk.sent().is_empty());

        desk.minimized.lock().unwrap().insert(11);
        assert_eq!(code(dispatch(&ctx, "input.key", json!({"user_id": 1, "key": "Space"})).await), "minimized");

        // Trazida para frente, a entrada passa e o primeiro clique leva o de foco.
        dispatch(&ctx, "window.bring_forward", json!({"user_id": 1})).await.unwrap();
        dispatch(&ctx, "input.click", json!({"user_id": 1, "x": 0.25, "y": 0.75})).await.unwrap();
        dispatch(&ctx, "input.click", json!({"user_id": 1, "x": 0.25, "y": 0.75})).await.unwrap();
        assert_eq!(desk.sent(), vec!["click:11:0.25:0.75:true", "click:11:0.25:0.75:false"]);
        // Com o lease ainda na mão a fila não está livre: nada volta ainda.
        assert_eq!(dispatch(&ctx, "window.restore_focus", json!({})).await.unwrap(), json!("denied"));
        assert_eq!(*desk.front.lock().unwrap(), 11);
        // Solto o lease, a volta do foco vai para a janela do usuário.
        dispatch(&ctx, "focus.release", json!({"lease": id})).await.unwrap();
        assert_eq!(dispatch(&ctx, "window.restore_focus", json!({})).await.unwrap(), json!("restored"));
        assert_eq!(*desk.front.lock().unwrap(), 99);
    }

    #[tokio::test]
    async fn untracked_account_is_blocked() {
        let desk = Arc::new(FakeDesk::default()); // nenhuma janela rastreada
        let ctx = ctx_with(desk.clone(), own_queue());
        lease(&ctx, 1).await;
        let error = dispatch(&ctx, "input.key", json!({"user_id": 1, "key": "Space"})).await.unwrap_err();
        assert_eq!(error.code.as_str(), "untracked_window");
        assert_eq!(error.message, "Blocked: Main_01 has no Roblox client opened by MultiAlt.");
        assert_eq!(code(dispatch(&ctx, "screen.pixel", json!({"user_id": 1, "x": 0.5, "y": 0.5})).await), "untracked_window");
        // Conta fora da execução: nem lease ela pega.
        assert_eq!(code(dispatch(&ctx, "focus.acquire", json!({"user_id": 77})).await), "untracked_window");
        assert_eq!(code(dispatch(&ctx, "screen.pixel", json!({"user_id": 77, "x": 0.5, "y": 0.5})).await), "untracked_window");
        assert!(desk.sent().is_empty());
    }

    #[tokio::test]
    async fn secrets_are_forbidden() {
        let ctx = ctx_with(FakeDesk::with_window(1, 11), own_queue());
        for op in ["account.cookie", "cookies", "get_password", "auth.token", "accounts.cookie", "shell.exec", "", "Accounts"] {
            assert_eq!(code(dispatch(&ctx, op, json!({})).await), "forbidden", "{op:?}");
        }
        let accounts = dispatch(&ctx, "accounts", json!({})).await.unwrap();
        let text = accounts.to_string().to_ascii_lowercase();
        assert!(!text.contains("cookie") && !text.contains("password"));
        assert_eq!(accounts.as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn lease_expiry_releases_held_keys() {
        let desk = FakeDesk::with_window(1, 11);
        *desk.front.lock().unwrap() = 11;
        let queue = own_queue();
        let ctx = ctx_with(desk.clone(), queue.clone());
        lease(&ctx, 1).await;
        dispatch(&ctx, "input.key_down", json!({"user_id": 1, "key": "w"})).await.unwrap();
        dispatch(&ctx, "input.key_down", json!({"user_id": 1, "key": "Shift"})).await.unwrap();
        // Ainda dentro dos 30 s: nada acontece.
        assert!(queue.expire_idle("r1", Instant::now(), desk.as_ref()).is_none());
        let expired = queue
            .expire_idle("r1", Instant::now() + Duration::from_millis(LEASE_IDLE_PLUS), desk.as_ref())
            .expect("idle lease is released");
        assert_eq!(expired.user_id, 1);
        assert_eq!(expired.released_keys, vec!["Shift".to_string(), "W".to_string()]);
        assert_eq!(desk.sent(), vec!["down:W", "down:Shift", "up:Shift", "up:W"]);
        // Sem lease, a entrada volta a ser recusada.
        assert_eq!(code(dispatch(&ctx, "input.key", json!({"user_id": 1, "key": "Space"})).await), "no_focus_lease");
        // E a fila ficou livre para outro.
        let other = RunCtx::new("r2", vec![account(1, "Main_01")], Arc::new(AtomicBool::new(false)), Arc::new(NullSink::default()), desk.clone(), queue.clone());
        assert!(dispatch(&other, "focus.acquire", json!({"user_id": 1, "timeout_ms": 200})).await.is_ok());
        // O fim da execução também solta.
        dispatch(&other, "input.key_down", json!({"user_id": 1, "key": "A"})).await.ok();
        let _ = other.finish();
        assert!(!queue.holds("r2", 1));
    }

    const LEASE_IDLE_PLUS: u64 = super::super::focus::LEASE_IDLE_MS + 1_000;

    /// Pegar e soltar a fila aparece no terminal, no máximo uma vez por
    /// segundo por execução (um laço que pega a fila a cada ação não inunda o
    /// log); as vezes puladas são contadas na linha seguinte.
    #[tokio::test]
    async fn focus_turns_are_logged_without_flooding() {
        let desk = FakeDesk::with_window(1, 11);
        let sink = Arc::new(NullSink::default());
        let ctx = RunCtx::new("r1", vec![account(1, "Main_01")], Arc::new(AtomicBool::new(false)), sink.clone(), desk, own_queue());
        for _ in 0..3 {
            let id = lease(&ctx, 1).await;
            dispatch(&ctx, "focus.release", json!({"lease": id})).await.unwrap();
        }
        let focus_lines = |sink: &NullSink| -> Vec<(Option<i64>, String, String)> {
            sink.lines
                .lock()
                .unwrap()
                .iter()
                .filter(|l| l.code.as_deref() == Some("focus"))
                .map(|l| (l.user_id, l.level.clone(), l.message.clone()))
                .collect()
        };
        let lines = focus_lines(&sink);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(lines[0], (Some(1), "system".to_string(), "Focus taken.".to_string()));
        assert!(lines[1].2.starts_with("Focus released after "), "{lines:?}");
        // Passado o intervalo, a próxima vez aparece e diz quantas não apareceram.
        ctx.state.lock().unwrap().focus_log_every = Duration::ZERO;
        let id = lease(&ctx, 1).await;
        dispatch(&ctx, "focus.release", json!({"lease": id})).await.unwrap();
        let lines = focus_lines(&sink);
        assert_eq!(lines[2].2, "Focus taken (2 turns not shown).");
        assert!(lines[3].2.starts_with("Focus released after "));
        assert_eq!(lines.len(), 4);
    }

    /// `accounts` é o estado de agora, não o do começo da execução: cliente que
    /// abriu ou fechou e o jogo/servidor em que está.
    #[tokio::test]
    async fn accounts_are_read_live() {
        let desk = Arc::new(FakeDesk::default());
        let ctx = ctx_with(desk.clone(), own_queue());
        let first = dispatch(&ctx, "accounts", json!({})).await.unwrap();
        assert_eq!(first[0]["in_game"], json!(false), "no client yet");
        desk.windows.lock().unwrap().insert(1, 11);
        desk.sessions.lock().unwrap().insert(1, LiveSession { in_game: true, place_id: 920587237, job_id: "abc-123".into() });
        let now = dispatch(&ctx, "accounts", json!({})).await.unwrap();
        assert_eq!(now[0]["user_id"], json!(1));
        assert_eq!(now[0]["name"], json!("Main_01"));
        assert_eq!(now[0]["in_game"], json!(true));
        assert_eq!(now[0]["place_id"], json!(920587237));
        assert_eq!(now[0]["job_id"], json!("abc-123"));
        // Sem cliente para a conta 2: nada de jogo.
        assert_eq!(now[1]["in_game"], json!(false));
        assert_eq!(now[1]["place_id"], json!(0));
    }

    #[tokio::test]
    async fn sleep_stops_when_the_run_is_stopped() {
        let ctx = ctx_with(FakeDesk::with_window(1, 11), own_queue());
        let stop = ctx.stop.clone();
        let started = Instant::now();
        let stopper = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(80)).await;
            stop.store(true, Ordering::SeqCst);
        });
        let result = dispatch(&ctx, "run.sleep", json!({"ms": 60_000})).await;
        stopper.await.unwrap();
        assert_eq!(result.unwrap_err().code.as_str(), "stopped");
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(dispatch(&ctx, "run.stopped", json!({})).await.unwrap(), json!(true));
        // Depois do Parar, nada de entrada.
        assert_eq!(code(dispatch(&ctx, "input.key", json!({"user_id": 1, "key": "Space"})).await), "stopped");
    }

    #[tokio::test]
    async fn pixel_reads_the_fraction_point() {
        let desk = FakeDesk::with_window(1, 11);
        // 4x2 BGRA; o pixel (3, 1) é vermelho.
        let (w, h) = (4u32, 2u32);
        let mut img = vec![0u8; (w * h * 4) as usize];
        let i = ((1 * w + 3) * 4) as usize;
        img[i..i + 4].copy_from_slice(&[0, 0, 255, 255]);
        let j = ((0 * w + 1) * 4) as usize;
        img[j..j + 4].copy_from_slice(&[10, 20, 30, 255]);
        *desk.image.lock().unwrap() = Some((w, h, img));
        let ctx = ctx_with(desk.clone(), own_queue());
        // Pixel não precisa de lease nem da janela na frente.
        assert_eq!(dispatch(&ctx, "screen.pixel", json!({"user_id": 1, "x": 0.99, "y": 0.99})).await.unwrap(), json!([255, 0, 0]));
        assert_eq!(dispatch(&ctx, "screen.pixel", json!({"user_id": 1, "x": 1.0, "y": 1.0})).await.unwrap(), json!([255, 0, 0]));
        assert_eq!(dispatch(&ctx, "screen.pixel", json!({"user_id": 1, "x": 0.3, "y": 0.0})).await.unwrap(), json!([30, 20, 10]));
        assert_eq!(code(dispatch(&ctx, "screen.pixel", json!({"user_id": 1, "x": 2, "y": 0.5})).await), "bad_args");
        desk.minimized.lock().unwrap().insert(11);
        assert_eq!(code(dispatch(&ctx, "screen.pixel", json!({"user_id": 1, "x": 0.5, "y": 0.5})).await), "minimized");
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn capture_publishes_a_named_frame_with_the_region() {
        let desk = FakeDesk::with_window(1, 11);
        let (w, h) = (10u32, 10u32);
        let img: Vec<u8> = (0..w * h * 4).map(|i| (i % 256) as u8).collect();
        *desk.image.lock().unwrap() = Some((w, h, img.clone()));
        let ctx = ctx_with(desk, own_queue());
        let result = dispatch(&ctx, "screen.capture", json!({"user_id": 1, "region": {"x": 0.5, "y": 0.5, "w": 0.5, "h": 0.5}}))
            .await
            .unwrap();
        assert_eq!(result["width"], json!(5));
        assert_eq!(result["height"], json!(5));
        assert_eq!(result["stride"], json!(20));
        assert_eq!(result["format"], json!("BGRA"));
        let name = result["shm"].as_str().unwrap();
        assert!(name.starts_with("Local\\multialt-cap-"));
        let bytes = crate::platform::windows::SharedFrame::read_by_name(name, 100).unwrap();
        let first = ((5 * w + 5) * 4) as usize;
        assert_eq!(&bytes[..20], &img[first..first + 20]);
        ctx.finish();
        assert!(crate::platform::windows::SharedFrame::read_by_name(name, 100).is_none());
    }

    #[tokio::test]
    async fn type_text_rejects_unmapped_chars() {
        let desk = FakeDesk::with_window(1, 11);
        *desk.front.lock().unwrap() = 11;
        let ctx = ctx_with(desk.clone(), own_queue());
        lease(&ctx, 1).await;
        let error = dispatch(&ctx, "input.type_text", json!({"user_id": 1, "text": "hi!", "delay_ms": 0})).await.unwrap_err();
        assert_eq!(error.code.as_str(), "unsupported_key");
        assert_eq!(error.message, "Key \"!\" is not supported.");
        assert!(desk.sent().is_empty(), "nothing is typed when one char is unsupported");
        dispatch(&ctx, "input.type_text", json!({"user_id": 1, "text": "Hi 1", "delay_ms": 0})).await.unwrap();
        assert_eq!(
            desk.sent(),
            vec!["down:Shift", "down:H", "up:H", "up:Shift", "down:I", "up:I", "down:Space", "up:Space", "down:1", "up:1"]
        );
        assert_eq!(code(dispatch(&ctx, "input.key", json!({"user_id": 1, "key": "Enter"})).await), "unsupported_key");
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn focus_queue_is_shared_with_afk() {
        let desk = FakeDesk::with_window(1, 11);
        let ctx = ctx_with(desk, FocusQueue::global());
        // Um ciclo do Modo AFK (ou uma gravação) segura a fila...
        let afk_cycle = crate::AFK_CYCLE_LOCK.lock().await;
        let error = dispatch(&ctx, "focus.acquire", json!({"user_id": 1, "timeout_ms": 100})).await.unwrap_err();
        assert_eq!(error.code.as_str(), "focus_timeout");
        assert_eq!(error.message, "Waited 100 ms for the focus queue (AFK mode or a recording may be running).");
        drop(afk_cycle);
        // ... e o lease do Python segura o AFK.
        let lease = lease(&ctx, 1).await;
        assert!(crate::AFK_CYCLE_LOCK.try_lock().is_err(), "AFK must wait for the Python lease");
        dispatch(&ctx, "focus.release", json!({"lease": lease})).await.unwrap();
        assert!(crate::AFK_CYCLE_LOCK.try_lock().is_ok());
    }

    /// A volta do foco não rouba a janela de quem vem a seguir: outra thread
    /// da mesma execução com o lease, o Modo AFK (que segura a fila sem lease)
    /// ou alguém esperando a vez.
    #[tokio::test]
    async fn restore_focus_only_runs_when_the_queue_is_free() {
        let desk = FakeDesk::with_window(1, 11);
        desk.windows.lock().unwrap().insert(2, 22);
        *desk.front.lock().unwrap() = 99; // a janela do usuário
        let lock: &'static tokio::sync::Mutex<()> = Box::leak(Box::new(tokio::sync::Mutex::new(())));
        let queue = Arc::new(FocusQueue::new(lock));
        let ctx = Arc::new(ctx_with(desk.clone(), queue.clone()));

        // A thread da conta 1 usa a janela e solta o lease...
        let first = lease(&ctx, 1).await;
        dispatch(&ctx, "window.bring_forward", json!({"user_id": 1})).await.unwrap();
        dispatch(&ctx, "focus.release", json!({"lease": first})).await.unwrap();
        // ... mas a da conta 2 (mesma execução) já pegou a vez.
        let second = lease(&ctx, 2).await;
        dispatch(&ctx, "window.bring_forward", json!({"user_id": 2})).await.unwrap();
        assert_eq!(dispatch(&ctx, "window.restore_focus", json!({})).await.unwrap(), json!("denied"));
        assert_eq!(*desk.front.lock().unwrap(), 22, "the window of whoever holds the queue stays in front");
        dispatch(&ctx, "focus.release", json!({"lease": second})).await.unwrap();

        // Um ciclo do Modo AFK segura a fila sem lease.
        let afk = lock.lock().await;
        assert_eq!(dispatch(&ctx, "window.restore_focus", json!({})).await.unwrap(), json!("denied"));
        assert_eq!(*desk.front.lock().unwrap(), 22);
        drop(afk);

        // Alguém esperando a vez também conta.
        let holder = lease(&ctx, 1).await;
        let waiter_ctx = ctx.clone();
        let waiter = tokio::spawn(async move {
            dispatch(&waiter_ctx, "focus.acquire", json!({"user_id": 2, "timeout_ms": 5000})).await
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(queue.hold_if_free().is_none());
        dispatch(&ctx, "focus.release", json!({"lease": holder})).await.unwrap();
        let next = waiter.await.unwrap().unwrap()["lease"].as_str().unwrap().to_string();
        assert_eq!(dispatch(&ctx, "window.restore_focus", json!({})).await.unwrap(), json!("denied"));
        dispatch(&ctx, "focus.release", json!({"lease": next})).await.unwrap();

        // Fila livre: quem solta por último devolve a janela do usuário.
        assert_eq!(dispatch(&ctx, "window.restore_focus", json!({})).await.unwrap(), json!("restored"));
        assert_eq!(*desk.front.lock().unwrap(), 99);
        assert!(lock.try_lock().is_ok(), "the restore lets go of the queue");
    }

    #[tokio::test]
    async fn renew_and_release_need_this_runs_lease() {
        let queue = own_queue();
        let ctx = ctx_with(FakeDesk::with_window(1, 11), queue.clone());
        let id = lease(&ctx, 1).await;
        assert!(dispatch(&ctx, "focus.renew", json!({"lease": id})).await.is_ok());
        assert_eq!(code(dispatch(&ctx, "focus.renew", json!({"lease": "L999"})).await), "no_focus_lease");
        let other = RunCtx::new("r2", vec![account(1, "Main_01")], Arc::new(AtomicBool::new(false)), Arc::new(NullSink::default()), FakeDesk::with_window(1, 11), queue.clone());
        dispatch(&other, "focus.release", json!({"lease": id})).await.unwrap();
        assert!(queue.holds("r1", 1), "another run cannot release this lease");
        assert_eq!(dispatch(&other, "window.restore_focus", json!({})).await.unwrap(), json!("denied"));
        dispatch(&ctx, "focus.release", json!({"lease": id})).await.unwrap();
        assert!(!queue.holds("r1", 1));
    }
}
