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
    /// Quantas execuções da automação Python receberam o parar
    /// (docs/features/python-automation.md).
    python: usize,
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
        python: 0,
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
    afk_session_stop_flag().is_some()
        || RECORDING_PLAYBACKS.load(Ordering::SeqCst) > 0
        || python::runs_active() > 0
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
    let mut payload = apply_emergency_stop(
        afk_stop.as_deref(),
        &RECORDING_PLAYBACK_STOP,
        &RECORDING_PLAYBACK_STOP_GEN,
        RECORDING_PLAYBACKS.load(Ordering::SeqCst),
    );
    // As execuções Python também param (todas).
    payload.python = python::emergency_stop();
    emit_session_log(
        app,
        "warn",
        "afk",
        String::from("Parada de emergência: o mouse foi chacoalhado"),
    );
    let _ = app.emit(SHAKE_STOP_EVENT, payload);
}

#[cfg(test)]
mod shake_stop_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Amostras a cada 15 ms de um caminho de pontos `(x, y)` ligados em reta,
    /// `ms_per_leg` por trecho. Devolve `(x, y, t)`.
    fn path(points: &[(i32, i32)], ms_per_leg: u64) -> Vec<(i32, i32, u64)> {
        let mut out = vec![(points[0].0, points[0].1, 0)];
        let mut t = 0u64;
        for pair in points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let steps = (ms_per_leg / 15).max(1);
            for i in 1..=steps {
                t += 15;
                let f = i as f64 / steps as f64;
                let x = a.0 + ((b.0 - a.0) as f64 * f).round() as i32;
                let y = a.1 + ((b.1 - a.1) as f64 * f).round() as i32;
                out.push((x, y, t));
            }
        }
        out
    }

    fn trips(samples: &[(i32, i32, u64)]) -> bool {
        let mut detector = ShakeDetector::default();
        samples.iter().any(|&(x, y, t)| detector.feed(x, y, t))
    }

    /// `n` idas e voltas horizontais de `amplitude` px em volta de `x`.
    fn zigzag(x: i32, y: i32, amplitude: i32, legs: usize) -> Vec<(i32, i32)> {
        let mut points = vec![(x, y)];
        for i in 0..legs {
            let side = if i % 2 == 0 { amplitude } else { -amplitude };
            points.push((x + side, y));
        }
        points
    }

    // ── a regra ───────────────────────────────────────────────────────────

    #[test]
    fn a_vigorous_horizontal_shake_stops() {
        // ±150 px, um trecho a cada 120 ms: o chacoalhar de quem quer parar.
        assert!(trips(&path(&zigzag(800, 500, 150, 6), 120)));
    }

    #[test]
    fn a_diagonal_shake_counts_by_its_horizontal_part() {
        let points = vec![(800, 500), (950, 400), (650, 600), (950, 400), (650, 600), (950, 400), (650, 600)];
        assert!(trips(&path(&points, 120)));
    }

    #[test]
    fn a_small_fast_jiggle_does_not_stop() {
        // ±30 px (60 px por trecho), rápido, por 3 s: mexer o mouse sem querer.
        assert!(!trips(&path(&zigzag(800, 500, 30, 50), 60)));
    }

    #[test]
    fn a_vertical_only_shake_does_not_stop() {
        let points: Vec<(i32, i32)> = (0..10).map(|i| (800, if i % 2 == 0 { 300 } else { 700 })).collect();
        assert!(!trips(&path(&points, 100)));
    }

    #[test]
    fn a_slow_back_and_forth_does_not_stop() {
        // Trechos longos, mas cada um leva 600 ms: viradas espalhadas demais.
        assert!(!trips(&path(&zigzag(800, 500, 300, 10), 600)));
    }

    #[test]
    fn three_reversals_are_not_enough() {
        // 4 trechos = 3 viradas, mesmo rápidos e longos.
        assert!(!trips(&path(&zigzag(800, 500, 150, 4), 100)));
        // O 5º trecho completa a 4ª virada.
        assert!(trips(&path(&zigzag(800, 500, 150, 5), 100)));
    }

    #[test]
    fn a_normal_mouse_trip_across_the_screen_does_not_stop() {
        // Ir até um botão, passar um pouco e voltar: uma virada só.
        let points = vec![(100, 100), (1500, 700), (1420, 690), (1440, 700)];
        assert!(!trips(&path(&points, 300)));
    }

    #[test]
    fn the_thresholds_are_the_documented_ones() {
        // docs/features/afk-mode.md cita estes números.
        assert_eq!(SHAKE_LEG_MIN_PX, 80);
        assert_eq!(SHAKE_REVERSALS, 4);
        assert_eq!(SHAKE_WINDOW_MS, 1_000);
    }

    // ── o movimento do próprio app ────────────────────────────────────────

    /// Um "desktop" de teste: o cursor e as marcas do módulo de entrada, num
    /// relógio de milissegundos.
    struct Desk {
        t: u64,
        cursor: (i32, i32),
        moves: usize,
        generation: u64,
        watch: ShakeWatch,
        /// Os pontos que o vigia leu sem filtro nenhum (o controle do teste).
        raw: Vec<(i32, i32, u64)>,
        tripped: bool,
    }

    impl Desk {
        fn new(cursor: (i32, i32)) -> Self {
            Self {
                t: 0,
                cursor,
                moves: 0,
                generation: 0,
                watch: ShakeWatch::default(),
                raw: Vec::new(),
                tripped: false,
            }
        }
        fn mark(&self) -> AppCursorMark {
            AppCursorMark {
                moving: self.moves > 0,
                generation: self.generation,
            }
        }
        /// O tempo anda; o vigia lê a cada 15 ms.
        fn advance(&mut self, ms: u64) {
            let end = self.t + ms;
            while self.t < end {
                self.t += 1;
                if self.t % 15 == 0 {
                    let before = self.mark();
                    self.raw.push((self.cursor.0, self.cursor.1, self.t));
                    let after = self.mark();
                    if self.watch.observe(before, Some(self.cursor), after, self.t) {
                        self.tripped = true;
                    }
                }
            }
        }
        fn begin(&mut self) {
            self.moves += 1;
            self.generation += 1;
        }
        fn end(&mut self) {
            self.generation += 1;
            self.moves -= 1;
        }
    }

    /// O clique do AFK e das gravações como o `click_point_with_plan` faz:
    /// marca, segue a receita (`afk_click_plan_with`), devolve o cursor e só
    /// então desmarca.
    fn app_click(desk: &mut Desk, rect: AfkClientRect, focus_click: bool) {
        let back = desk.cursor;
        desk.begin();
        let steps = afk_click_plan_with(rect, AFK_DEFAULT_POINT, 40, focus_click).expect("janela com área");
        for step in steps {
            match step {
                AfkMouseStep::MoveTo(x, y) => desk.cursor = (x, y),
                AfkMouseStep::Nudge(dx, dy) => desk.cursor = (desk.cursor.0 + dx, desk.cursor.1 + dy),
                AfkMouseStep::Wait(ms) => desk.advance(ms),
                AfkMouseStep::Press | AfkMouseStep::Release => desk.advance(1),
            }
        }
        desk.cursor = back;
        desk.end();
    }

    fn window(left: i32, top: i32) -> AfkClientRect {
        AfkClientRect {
            left,
            top,
            width: 800,
            height: 600,
        }
    }

    #[test]
    fn app_clicks_across_several_windows_never_trip_it() {
        let mut desk = Desk::new((960, 540));
        // Um ciclo de 8 contas alternando entre janelas à esquerda e à direita,
        // três ciclos seguidos, com o usuário parado no meio da tela.
        let windows = [
            window(0, 0),
            window(1100, 0),
            window(0, 500),
            window(1100, 500),
            window(50, 100),
            window(1200, 300),
            window(0, 0),
            window(1100, 500),
        ];
        for _ in 0..3 {
            for rect in windows {
                desk.advance(150); // a janela vem para frente
                app_click(&mut desk, rect, true);
                desk.advance(250); // respiro entre contas
            }
            desk.advance(1_000);
        }
        // E uma gravação em cada janela: quatro cliques seguidos, 30 ms entre
        // eles. É ela que vai e volta depressa — cada clique leva o cursor e o
        // devolve ao usuário.
        for rect in windows {
            desk.advance(150);
            for i in 0..4 {
                app_click(&mut desk, rect, i == 0);
                desk.advance(30);
            }
            desk.advance(250);
        }
        assert!(!desk.tripped, "o clique do próprio app parou o Modo AFK");
        // Controle: os mesmos pontos sem o filtro **seriam** um chacoalhar —
        // senão este teste passaria sozinho.
        assert!(trips(&desk.raw), "o caminho do teste não vai e volta o bastante");
    }

    #[test]
    fn the_user_shaking_between_app_clicks_still_stops() {
        let mut desk = Desk::new((960, 540));
        app_click(&mut desk, window(0, 0), true);
        for &(x, y) in &zigzag(960, 540, 150, 6)[1..] {
            desk.cursor = (x, y);
            desk.advance(120);
        }
        assert!(desk.tripped);
    }

    #[test]
    fn a_sample_read_while_the_app_starts_moving_is_discarded() {
        let mut watch = ShakeWatch::default();
        let still = AppCursorMark { moving: false, generation: 0 };
        // Linha de base e três viradas do usuário.
        let mut t = 0;
        for x in [500, 700, 500, 700, 500] {
            t += 100;
            assert!(!watch.observe(still, Some((x, 300)), still, t));
        }
        // O app começou a mover entre a marca de antes e a de depois: o ponto
        // lido pode ser o dele, então não pode completar a 4ª virada.
        let started = AppCursorMark { moving: true, generation: 1 };
        assert!(!watch.observe(still, Some((700, 300)), started, t + 15));
        // E o que veio antes do movimento do app não soma com o que vem depois.
        let done = AppCursorMark { moving: false, generation: 2 };
        assert!(!watch.observe(done, Some((500, 300)), done, t + 30));
        assert!(!watch.observe(done, Some((700, 300)), done, t + 130));
    }

    #[test]
    fn a_lost_cursor_reading_restarts_the_baseline() {
        let mut watch = ShakeWatch::default();
        let still = AppCursorMark { moving: false, generation: 0 };
        let mut t = 0;
        for x in [500, 700, 500, 700] {
            t += 100;
            watch.observe(still, Some((x, 300)), still, t);
        }
        assert!(!watch.observe(still, None, still, t + 50));
        assert!(!watch.observe(still, Some((500, 300)), still, t + 100));
    }

    #[test]
    fn the_app_move_mark_lasts_until_the_guard_is_dropped() {
        let before = app_cursor_mark();
        let guard = app_cursor_move();
        let during = app_cursor_mark();
        assert!(during.moving);
        assert_ne!(during.generation, before.generation);
        drop(guard);
        let after = app_cursor_mark();
        assert_ne!(after.generation, during.generation);
    }

    /// O clique e o movimento de zero pixel do módulo de entrada marcam o
    /// movimento do app — e o clique marca **antes** de mover e só desmarca
    /// depois do `SetCursorPos` que devolve o cursor (a marca vive até o fim
    /// da função).
    #[test]
    fn the_input_module_marks_its_own_cursor_moves() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/platform/windows/input.rs"),
        )
        .expect("input.rs");
        for function in ["fn click_point_with_plan(", "pub fn nudge_for_focus_back("] {
            let at = source.find(function).unwrap_or_else(|| panic!("{function} sumiu"));
            let body_start = at + source[at..].find('{').expect("corpo");
            let body = &source[body_start..];
            let mark = body.find("let _app_move = crate::app_cursor_move();");
            assert!(mark.is_some(), "{function} não marca o movimento do app");
            let first_move = ["send_mouse(", "SetCursorPos("]
                .iter()
                .filter_map(|needle| body.find(needle))
                .min()
                .expect("o corpo move o cursor");
            assert!(mark.unwrap() < first_move, "{function} marca depois de mover");
        }
    }

    // ── parar ─────────────────────────────────────────────────────────────

    #[test]
    fn the_emergency_stop_sets_the_afk_and_the_playback_stop_flags() {
        let afk = AtomicBool::new(false);
        let playback = AtomicBool::new(false);
        let playback_gen = std::sync::atomic::AtomicU64::new(3);
        let payload = apply_emergency_stop(Some(&afk), &playback, &playback_gen, 1);
        assert!(afk.load(Ordering::SeqCst), "a sessão do Modo AFK não recebeu o parar");
        assert!(playback.load(Ordering::SeqCst), "a reprodução avulsa não recebeu o parar");
        // O chacoalhar sobe a geração do "parar": uma reprodução já na fila (um
        // launch ou reconexão em lote) aborta ao pegar a vez, não só a que toca.
        assert_eq!(playback_gen.load(Ordering::SeqCst), 4);
        assert_eq!(
            payload,
            EmergencyStopPayload {
                reason: "shake".into(),
                afk: true,
                recordings: true,
                python: 0,
            }
        );
    }

    #[test]
    fn without_an_afk_session_only_the_playback_is_stopped() {
        let playback = AtomicBool::new(false);
        let playback_gen = std::sync::atomic::AtomicU64::new(0);
        let payload = apply_emergency_stop(None, &playback, &playback_gen, 0);
        assert!(playback.load(Ordering::SeqCst));
        assert_eq!(playback_gen.load(Ordering::SeqCst), 1);
        assert!(!payload.afk);
        assert!(!payload.recordings);
    }

    #[test]
    fn the_payload_reaches_the_frontend_in_camel_case() {
        let json = serde_json::to_value(EmergencyStopPayload {
            reason: "shake".into(),
            afk: true,
            recordings: false,
            python: 2,
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "reason": "shake", "afk": true, "recordings": false, "python": 2 })
        );
        assert_eq!(SHAKE_STOP_EVENT, "afk-emergency-stop");
    }

    #[test]
    fn shake_to_stop_is_off_unless_turned_on() {
        assert!(!shake_stop_enabled(""));
        assert!(!shake_stop_enabled("false"));
        assert!(!shake_stop_enabled("yes"));
        assert!(shake_stop_enabled("true"));
        assert!(shake_stop_enabled("True"));
    }
}
