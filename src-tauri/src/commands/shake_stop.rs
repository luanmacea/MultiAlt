// Parada de emergência por chacoalhar o mouse (`Afk.ShakeToStop`, desligada por
// padrão — escolha do dono). Ver docs/features/afk-mode.md#parada-de-emergência.
//
// Durante um ciclo do Modo AFK e durante uma gravação tocando, o app tira o
// foco e leva o cursor até a janela de cada conta: o usuário não consegue
// chegar ao botão Parar. Com a opção ligada, chacoalhar o mouse para tudo.
//
// O que este arquivo faz e o que não faz:
// - lê **só a posição** do cursor (`cursor_position`), a cada 15 ms, e só
//   enquanto há sessão do Modo AFK ou reprodução de gravação rodando;
// - nunca lê botão nem tecla, nunca envia entrada: a trava do
//   `afk_input_safety_tests` vale aqui como em todo o Modo AFK;
// - o movimento que o próprio app faz (o clique do AFK e das gravações, que
//   leva o cursor e o devolve) é marcado pelo módulo de entrada
//   (`app_cursor_move`) e descartado: amostra tirada durante ele não conta, e a
//   linha de base recomeça depois dele;
// - ao reconhecer o chacoalhar, liga as mesmas bandeiras dos botões Parar (a da
//   sessão do Modo AFK e a da reprodução avulsa). As teclas apertadas são
//   soltas pelos caminhos de sempre (`play_recording_with`); nenhum cliente é
//   fechado.

use std::collections::VecDeque;

/// Quanto cada trecho horizontal tem de andar para a virada contar.
const SHAKE_LEG_MIN_PX: i32 = 80;
/// Quantas viradas de direção fazem um chacoalhar...
const SHAKE_REVERSALS: usize = 4;
/// ...dentro desta janela de tempo.
const SHAKE_WINDOW_MS: u64 = 1_000;
/// De quanto em quanto tempo o cursor é lido.
#[cfg(target_os = "windows")]
const SHAKE_POLL_MS: u64 = 15;
/// De quanto em quanto tempo o vigia relê a opção (desligar no meio para ele).
#[cfg(target_os = "windows")]
const SHAKE_SETTING_RECHECK_MS: u64 = 1_000;
/// Folga para decidir a direção do primeiro trecho: tremida do sensor não
/// escolhe direção.
const SHAKE_START_PX: i32 = 8;

/// O evento que a tela recebe.
const SHAKE_STOP_EVENT: &str = "afk-emergency-stop";

/// `Afk.ShakeToStop`: desligada a menos que esteja `"true"`.
fn shake_stop_enabled(raw: &str) -> bool {
    raw.trim().eq_ignore_ascii_case("true")
}

/// Reconhece o chacoalhar a partir de amostras `(x, y, t_ms)` do cursor.
///
/// Regra: conta as **viradas** de direção horizontal em que o trecho antes e o
/// trecho depois andaram pelo menos `SHAKE_LEG_MIN_PX`; `SHAKE_REVERSALS`
/// viradas dentro de `SHAKE_WINDOW_MS` = chacoalhar. Movimento só vertical,
/// devagar ou tremida curta não chega lá.
#[derive(Debug, Default)]
struct ShakeDetector {
    /// Onde começou o trecho atual (`None`: sem linha de base ainda).
    leg_start: Option<i32>,
    /// O ponto mais longe do trecho atual, na direção dele.
    extreme: i32,
    /// `-1`, `0` (direção ainda não decidida) ou `1`.
    dir: i32,
    /// Quando cada virada que contou aconteceu.
    reversals: VecDeque<u64>,
}

impl ShakeDetector {
    fn reset(&mut self) {
        *self = Self::default();
    }

    /// Uma amostra. `true` = chacoalhar reconhecido (e o detector recomeça).
    /// O `y` não entra na regra: é só a direção horizontal que conta.
    fn feed(&mut self, x: i32, _y: i32, t_ms: u64) -> bool {
        while let Some(&first) = self.reversals.front() {
            if t_ms.saturating_sub(first) > SHAKE_WINDOW_MS {
                self.reversals.pop_front();
            } else {
                break;
            }
        }
        let Some(start) = self.leg_start else {
            self.leg_start = Some(x);
            self.extreme = x;
            self.dir = 0;
            return false;
        };
        if self.dir == 0 {
            let moved = x - start;
            if moved.abs() >= SHAKE_START_PX {
                self.dir = moved.signum();
                self.extreme = x;
            }
            return false;
        }
        if (x - self.extreme) * self.dir > 0 {
            // Ainda indo para o mesmo lado.
            self.extreme = x;
            return false;
        }
        if (self.extreme - x) * self.dir >= SHAKE_LEG_MIN_PX {
            // Voltou o bastante: virou. Conta se o trecho que acabou também
            // foi longo.
            if (self.extreme - start).abs() >= SHAKE_LEG_MIN_PX {
                self.reversals.push_back(t_ms);
            }
            self.leg_start = Some(self.extreme);
            self.extreme = x;
            self.dir = -self.dir;
            if self.reversals.len() >= SHAKE_REVERSALS {
                self.reset();
                return true;
            }
        }
        false
    }
}

/// O que o módulo de entrada diz sobre o cursor: o app está movendo agora?, e
/// quantas vezes um movimento do app começou ou terminou até aqui.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AppCursorMark {
    moving: bool,
    generation: u64,
}

/// O detector com o filtro do movimento do próprio app.
///
/// Cada leitura do cursor vem entre duas marcas, lidas antes e depois dela. A
/// amostra é descartada (e a linha de base recomeça) se o app estava movendo o
/// cursor em qualquer das duas, ou se um movimento começou ou terminou entre
/// elas. A primeira amostra limpa depois de um movimento do app vira a linha
/// de base nova: o salto do cursor devolvido nunca conta como trecho.
#[derive(Debug, Default)]
struct ShakeWatch {
    detector: ShakeDetector,
    last_generation: Option<u64>,
}

impl ShakeWatch {
    fn observe(
        &mut self,
        before: AppCursorMark,
        position: Option<(i32, i32)>,
        after: AppCursorMark,
        t_ms: u64,
    ) -> bool {
        if before.moving || after.moving || before.generation != after.generation {
            self.detector.reset();
            self.last_generation = None;
            return false;
        }
        if self.last_generation != Some(before.generation) {
            self.detector.reset();
            self.last_generation = Some(before.generation);
        }
        let Some((x, y)) = position else {
            self.detector.reset();
            return false;
        };
        self.detector.feed(x, y, t_ms)
    }
}

// ── o movimento do próprio app ────────────────────────────────────────────

/// Quantos movimentos do app estão em andamento (o clique pode se aninhar).
static APP_CURSOR_MOVES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Sobe no começo e no fim de cada movimento do app.
static APP_CURSOR_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Marca "o app está movendo o cursor" enquanto vive. Quem cria é o módulo de
/// entrada, no começo do clique (e do movimento de zero pixel); o fim é o
/// `drop`, depois de o cursor voltar para onde estava.
#[must_use]
pub(crate) struct AppCursorMove(());

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn app_cursor_move() -> AppCursorMove {
    use std::sync::atomic::Ordering;
    APP_CURSOR_MOVES.fetch_add(1, Ordering::SeqCst);
    APP_CURSOR_GENERATION.fetch_add(1, Ordering::SeqCst);
    AppCursorMove(())
}

impl Drop for AppCursorMove {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        // A geração sobe antes de o contador descer: quem leu "parado" com a
        // geração velha vê a mudança na leitura de depois.
        APP_CURSOR_GENERATION.fetch_add(1, Ordering::SeqCst);
        APP_CURSOR_MOVES.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn app_cursor_mark() -> AppCursorMark {
    use std::sync::atomic::Ordering;
    let generation = APP_CURSOR_GENERATION.load(Ordering::SeqCst);
    let moving = APP_CURSOR_MOVES.load(Ordering::SeqCst) > 0;
    AppCursorMark { moving, generation }
}

// ── parar ─────────────────────────────────────────────────────────────────

/// O que a tela recebe no `afk-emergency-stop`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct EmergencyStopPayload {
    /// Por que parou: hoje só `shake`.
    reason: String,
    /// Havia sessão do Modo AFK, e ela recebeu o parar.
    afk: bool,
    /// Havia gravação tocando avulsa, e ela recebeu o parar.
    recordings: bool,
}

/// Liga as bandeiras dos botões Parar: a da sessão do Modo AFK (quando há) e a
/// da reprodução avulsa. A reprodução avulsa passa pelo `request_playback_stop`,
/// que também sobe a geração do "parar" — assim uma reprodução já na fila (um
/// launch ou uma reconexão em lote arma várias) aborta ao pegar a vez, não só a
/// que toca. Não fecha nada, não espera nada.
fn apply_emergency_stop(
    afk_stop: Option<&std::sync::atomic::AtomicBool>,
    playback_stop: &std::sync::atomic::AtomicBool,
    playback_stop_gen: &std::sync::atomic::AtomicU64,
    playbacks_running: usize,
) -> EmergencyStopPayload {
    use std::sync::atomic::Ordering;
    if let Some(flag) = afk_stop {
        flag.store(true, Ordering::SeqCst);
    }
    request_playback_stop(playback_stop, playback_stop_gen);
    EmergencyStopPayload {
        reason: "shake".to_string(),
        afk: afk_stop.is_some(),
        recordings: playbacks_running > 0,
    }
}

// ── o vigia (Windows) ─────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
static SHAKE_WATCH_RUNNING: AtomicBool = AtomicBool::new(false);

/// Sessão do Modo AFK que ainda não recebeu o parar.
#[cfg(target_os = "windows")]
fn afk_session_stop_flag() -> Option<Arc<AtomicBool>> {
    AFK_MANAGER
        .get_session()
        .map(|session| session.stop_flag)
        .filter(|flag| !flag.load(Ordering::SeqCst))
}

/// Há algo que o chacoalhar para?
#[cfg(target_os = "windows")]
fn shake_watch_needed() -> bool {
    afk_session_stop_flag().is_some() || RECORDING_PLAYBACKS.load(Ordering::SeqCst) > 0
}

#[cfg(target_os = "windows")]
fn shake_stop_setting(app: &tauri::AppHandle) -> bool {
    shake_stop_enabled(&app.state::<SettingsStore>().get_string("Afk", "ShakeToStop"))
}

/// Liga o vigia se a opção está ligada e há o que parar. Chamado no começo da
/// sessão do Modo AFK, a cada tique dela (ligar a opção no meio vale) e no
/// começo de cada reprodução avulsa. Um vigia só, para tudo.
#[cfg(target_os = "windows")]
pub(crate) fn ensure_shake_watch(app: &tauri::AppHandle) {
    if !shake_stop_setting(app) || !shake_watch_needed() {
        return;
    }
    if SHAKE_WATCH_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || loop {
        run_shake_watch(&app);
        SHAKE_WATCH_RUNNING.store(false, Ordering::SeqCst);
        // Algo começou enquanto o vigia saía: quem chamou viu "rodando" e não
        // abriu outro, então este continua.
        if !(shake_stop_setting(&app) && shake_watch_needed())
            || SHAKE_WATCH_RUNNING.swap(true, Ordering::SeqCst)
        {
            break;
        }
    });
}

#[cfg(target_os = "windows")]
fn run_shake_watch(app: &tauri::AppHandle) {
    let started = std::time::Instant::now();
    let mut watch = ShakeWatch::default();
    let mut setting_checked_at = 0u64;
    loop {
        let t_ms = started.elapsed().as_millis() as u64;
        if t_ms.saturating_sub(setting_checked_at) >= SHAKE_SETTING_RECHECK_MS {
            setting_checked_at = t_ms;
            if !shake_stop_setting(app) {
                return;
            }
        }
        if !shake_watch_needed() {
            return;
        }
        let before = app_cursor_mark();
        let position = platform::windows::cursor_position();
        let after = app_cursor_mark();
        if watch.observe(before, position, after, t_ms) {
            trigger_emergency_stop(app);
        }
        std::thread::sleep(std::time::Duration::from_millis(SHAKE_POLL_MS));
    }
}

#[cfg(target_os = "windows")]
fn trigger_emergency_stop(app: &tauri::AppHandle) {
    let afk_stop = afk_session_stop_flag();
    let payload = apply_emergency_stop(
        afk_stop.as_deref(),
        &RECORDING_PLAYBACK_STOP,
        &RECORDING_PLAYBACK_STOP_GEN,
        RECORDING_PLAYBACKS.load(Ordering::SeqCst),
    );
    emit_session_log(
        app,
        "warn",
        "afk",
        String::from("Parada de emergência: o mouse foi chacoalhado"),
    );
    let _ = app.emit(SHAKE_STOP_EVENT, payload);
}
