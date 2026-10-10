// Otimização que segue o foco (ideia 18 de docs/ideias-de-outros-gerenciadores.md).
//
// Com `Optimization.FollowFocus` ligado, o cliente que o usuário está jogando
// roda a toda velocidade e os outros (só os que o app abriu) ficam com a
// política de fundo. Ver docs/features/performance.md.
//
// Como o app sabe qual janela está em uso: **pergunta**, uma vez por segundo
// (`GetForegroundWindow` + `GetWindowThreadProcessId`, as duas já importadas
// pelo binário). Nada de gancho — nem o global, proibido no app inteiro, nem o
// de eventos de acessibilidade, que faria do módulo `windows` um "leitor de
// entrada" para o `afk_input_safety_tests` (ele reprova o AFK mode citar um
// módulo assim, e reprova até o nome da API aqui). A pergunta custa microssegundos;
// mudar prioridade (`OpenProcess` + `SetPriorityClass`...) só acontece quando
// o cliente em uso troca.
//
// Regras:
// - **Só clientes que o app abriu** (rastreados e não adotados). Cliente aberto
//   pelo site é só exibido — nunca recebe prioridade, teto nem nada.
// - O cliente "em uso" é o último cliente nosso que esteve em primeiro plano.
//   Trocar para o Discord ou para o próprio MultiAlt não derruba o jogo que o
//   usuário acabou de deixar.
// - Cliente novo tem 35 s de carência a toda velocidade (carregar o jogo é a
//   parte mais pesada). Os que já estavam abertos quando a opção ligou não têm.
// - Desligar a opção (ou fechar o app) devolve cada cliente ao que o launch
//   deu: a política do perfil, se ligada, senão o normal; o teto de CPU do Job
//   volta se o perfil o tiver.

use std::time::Instant;

/// Carência de um cliente novo antes de ir para o fundo.
const FOCUS_FOLLOW_GRACE: Duration = Duration::from_secs(35);

/// A velocidade que o app quer para um cliente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClientSpeed {
    /// Prioridade normal, sem economia de energia, sem teto de CPU.
    Full,
    /// A política de fundo (`background_process_policy`) e o teto do perfil.
    Background,
}

/// O cliente em uso depois desta olhada. `foreground`: o PID dono da janela em
/// primeiro plano; `ours`: os clientes que o app abriu.
///
/// Janela de fora (Discord, o próprio app, um cliente aberto pelo site) não
/// troca o cliente em uso: o último nosso continua sendo ele, enquanto existir.
fn next_active_pid(
    previous: Option<u32>,
    foreground: Option<u32>,
    ours: &std::collections::HashSet<u32>,
) -> Option<u32> {
    if let Some(pid) = foreground.filter(|pid| ours.contains(pid)) {
        return Some(pid);
    }
    previous.filter(|pid| ours.contains(pid))
}

/// A velocidade de um cliente: o em uso e o que ainda está na carência vão a
/// toda velocidade; o resto, para o fundo.
fn desired_speed(
    pid: u32,
    active: Option<u32>,
    grace_until: Instant,
    now: Instant,
) -> ClientSpeed {
    if active == Some(pid) || now < grace_until {
        ClientSpeed::Full
    } else {
        ClientSpeed::Background
    }
}

/// O que mudar nesta olhada: só os clientes cuja velocidade desejada difere da
/// última aplicada (ou que nunca receberam nenhuma). Ordenado por PID.
fn plan_speed_changes(
    clients: &[(u32, Instant)],
    applied: &HashMap<u32, ClientSpeed>,
    active: Option<u32>,
    now: Instant,
) -> Vec<(u32, ClientSpeed)> {
    let mut out: Vec<(u32, ClientSpeed)> = clients
        .iter()
        .filter_map(|&(pid, grace_until)| {
            let want = desired_speed(pid, active, grace_until, now);
            (applied.get(&pid) != Some(&want)).then_some((pid, want))
        })
        .collect();
    out.sort_by_key(|(pid, _)| *pid);
    out
}

/// A política dos clientes de fundo: a do perfil com que a conta abriu, se o
/// usuário a ligou (`EnableProcessPolicy`); senão uma leve — prioridade abaixo
/// do normal, EcoQoS e memória baixa. Sem isso, ligar a opção com a política do
/// perfil desligada (o padrão) não faria nada.
pub(crate) fn background_process_policy(launch: &WindowsProcessPolicy) -> WindowsProcessPolicy {
    if launch.enabled {
        return WindowsProcessPolicy {
            delay_ms: 0,
            ..launch.clone()
        };
    }
    WindowsProcessPolicy {
        enabled: true,
        delay_ms: 0,
        priority_class: WindowsPriorityClass::BelowNormal,
        background_mode: false,
        eco_qos: true,
        ignore_timer_resolution: true,
        memory_priority: WindowsMemoryPriority::Low,
    }
}

/// A janela em uso: prioridade normal, economia de energia devolvida ao
/// Windows e memória normal — mesmo que o perfil peça `BackgroundMode`.
pub(crate) fn full_speed_process_policy() -> WindowsProcessPolicy {
    WindowsProcessPolicy {
        enabled: true,
        delay_ms: 0,
        priority_class: WindowsPriorityClass::Normal,
        background_mode: false,
        eco_qos: false,
        ignore_timer_resolution: false,
        memory_priority: WindowsMemoryPriority::Normal,
    }
}

/// O teto de CPU do Job para esta velocidade: `None` = não mexer (o perfil não
/// tem teto, então não existe Job com teto); `Some(None)` = desligar;
/// `Some(Some(p))` = religar com `p`%.
fn cpu_cap_for(speed: ClientSpeed, experimental: &WindowsExperimentalPolicy) -> Option<Option<u32>> {
    if !experimental.enable_job_cpu_limit {
        return None;
    }
    Some(match speed {
        ClientSpeed::Full => None,
        ClientSpeed::Background => Some(experimental.job_cpu_limit_percent),
    })
}

/// O estado a devolver quando a opção desliga ou o app fecha: o que o launch
/// deu. Política do perfil se ligada, senão a toda velocidade; o teto do Job
/// volta se o perfil o tiver.
fn release_targets(launch: &WindowsOptimizationProfile) -> (WindowsProcessPolicy, Option<Option<u32>>) {
    let process = if launch.process.enabled {
        WindowsProcessPolicy {
            delay_ms: 0,
            ..launch.process.clone()
        }
    } else {
        full_speed_process_policy()
    };
    let cap = launch
        .experimental
        .enable_job_cpu_limit
        .then_some(Some(launch.experimental.job_cpu_limit_percent));
    (process, cap)
}

/// O perfil que o launch aplica quando a opção está ligada: a prioridade fica
/// com o laço (o cliente novo está na carência, a toda velocidade); o Job
/// (teto de CPU e de memória) continua sendo criado no launch.
pub(crate) fn launch_profile_under_focus_follow(
    profile: WindowsOptimizationProfile,
    follow_focus: bool,
) -> WindowsOptimizationProfile {
    if !follow_focus {
        return profile;
    }
    WindowsOptimizationProfile {
        process: WindowsProcessPolicy {
            enabled: false,
            ..profile.process
        },
        experimental: profile.experimental,
    }
}

/// `Optimization.FollowFocus` (desligado por padrão).
pub(crate) fn focus_follow_enabled(settings: &SettingsStore) -> bool {
    settings.get_bool("Optimization", "FollowFocus")
}

#[derive(Default)]
struct FocusFollowState {
    /// O laço já está valendo (a primeira olhada depois de ligar já passou).
    running: bool,
    active: Option<u32>,
    /// Até quando cada cliente está na carência.
    grace_until: HashMap<u32, Instant>,
    /// A última velocidade aplicada em cada cliente.
    applied: HashMap<u32, ClientSpeed>,
    /// O perfil com que cada cliente abriu (o launch avisa).
    launch_profiles: HashMap<u32, LaunchClientProfile>,
}

static FOCUS_FOLLOW: LazyLock<Mutex<FocusFollowState>> =
    LazyLock::new(|| Mutex::new(FocusFollowState::default()));

fn focus_follow_state() -> std::sync::MutexGuard<'static, FocusFollowState> {
    FOCUS_FOLLOW.lock().unwrap_or_else(|e| e.into_inner())
}

/// O launch avisa com que perfil (já resolvido) o cliente abriu — é dele que
/// sai a política de fundo e o estado a devolver.
pub(crate) fn remember_launch_profile(pid: u32, profile: LaunchClientProfile) {
    focus_follow_state().launch_profiles.insert(pid, profile);
}

/// O launch acabou de mexer no processo (criou o Job com teto): a próxima
/// olhada reaplica a velocidade desejada em vez de confiar na última.
pub fn focus_follow_forget_applied(pid: u32) {
    focus_follow_state().applied.remove(&pid);
}

/// Os clientes que o app abriu: rastreados e não adotados.
fn launched_client_pids() -> std::collections::HashSet<u32> {
    tracker()
        .get_all()
        .into_iter()
        .filter(|p| !p.adopted)
        .map(|p| p.pid)
        .collect()
}

fn launch_optimization_for(
    settings: &SettingsStore,
    state: &FocusFollowState,
    pid: u32,
) -> WindowsOptimizationProfile {
    let profile = state
        .launch_profiles
        .get(&pid)
        .copied()
        .unwrap_or(LaunchClientProfile::Normal);
    load_optimization_profile(settings, profile)
}

/// Aplica uma velocidade num cliente. O PID é conferido antes (o Windows
/// reaproveita PIDs: nunca mexer num processo que não é mais o Roblox).
fn apply_client_speed(
    pid: u32,
    process: &WindowsProcessPolicy,
    cap: Option<Option<u32>>,
    alive: &std::collections::HashSet<u32>,
) {
    if !alive.contains(&pid) {
        return;
    }
    if let Err(err) = apply_process_policy_live(pid, process) {
        eprintln!("[focus] PID {pid}: {err}");
    }
    if let Some(cap) = cap {
        if let Err(err) = set_job_cpu_cap(pid, cap) {
            eprintln!("[focus] PID {pid}: {err}");
        }
    }
}

/// Devolve todo cliente que o laço mexeu ao estado do launch e para.
fn release_locked(settings: &SettingsStore, state: &mut FocusFollowState) {
    if !state.applied.is_empty() {
        let alive: std::collections::HashSet<u32> = get_roblox_pids().into_iter().collect();
        let pids: Vec<u32> = state.applied.keys().copied().collect();
        for pid in pids {
            let (process, cap) = release_targets(&launch_optimization_for(settings, state, pid));
            apply_client_speed(pid, &process, cap, &alive);
        }
    }
    state.running = false;
    state.active = None;
    state.applied.clear();
    state.grace_until.clear();
}

/// Uma olhada: chamada a cada segundo pelo laço de `commands/focus_follow.rs`.
pub fn focus_follow_tick(settings: &SettingsStore) {
    let enabled = focus_follow_enabled(settings);
    let mut state = focus_follow_state();
    let ours = launched_client_pids();

    // Cliente que saiu do rastreamento: esquece (o PID pode voltar a outro
    // processo).
    state.applied.retain(|pid, _| ours.contains(pid));
    state.grace_until.retain(|pid, _| ours.contains(pid));
    state.launch_profiles.retain(|pid, _| ours.contains(pid));

    if !enabled {
        if state.running || !state.applied.is_empty() {
            release_locked(settings, &mut state);
        }
        return;
    }

    let now = Instant::now();
    let first_look = !state.running;
    state.running = true;
    for &pid in &ours {
        state.grace_until.entry(pid).or_insert(if first_look {
            // Já estava aberto quando a opção ligou: sem carência.
            now
        } else {
            now + FOCUS_FOLLOW_GRACE
        });
    }

    let foreground = window_pid(unsafe { GetForegroundWindow() });
    state.active = next_active_pid(state.active, foreground, &ours);

    let clients: Vec<(u32, Instant)> = state
        .grace_until
        .iter()
        .map(|(pid, until)| (*pid, *until))
        .collect();
    let changes = plan_speed_changes(&clients, &state.applied, state.active, now);
    if changes.is_empty() {
        return;
    }
    let alive: std::collections::HashSet<u32> = get_roblox_pids().into_iter().collect();
    for (pid, speed) in changes {
        let launch = launch_optimization_for(settings, &state, pid);
        let process = match speed {
            ClientSpeed::Full => full_speed_process_policy(),
            ClientSpeed::Background => background_process_policy(&launch.process),
        };
        apply_client_speed(pid, &process, cpu_cap_for(speed, &launch.experimental), &alive);
        state.applied.insert(pid, speed);
    }
}

/// Ao fechar o app: devolve os clientes ao estado do launch.
pub fn release_focus_follow(settings: &SettingsStore) {
    let mut state = focus_follow_state();
    release_locked(settings, &mut state);
}

#[cfg(test)]
mod focus_follow_tests {
    use super::*;
    use std::collections::HashSet;

    fn set(pids: &[u32]) -> HashSet<u32> {
        pids.iter().copied().collect()
    }

    // ── cliente em uso ─────────────────────────────────────────────────────

    #[test]
    fn the_focused_launched_client_becomes_the_active_one() {
        assert_eq!(next_active_pid(None, Some(20), &set(&[10, 20])), Some(20));
        assert_eq!(next_active_pid(Some(10), Some(20), &set(&[10, 20])), Some(20));
    }

    #[test]
    fn focusing_another_app_keeps_the_last_client_in_use() {
        // Alt-tab para o Discord (PID 999, não é nosso): o jogo que o usuário
        // acabou de deixar continua a toda velocidade.
        assert_eq!(next_active_pid(Some(10), Some(999), &set(&[10, 20])), Some(10));
        assert_eq!(next_active_pid(Some(10), None, &set(&[10, 20])), Some(10));
    }

    #[test]
    fn a_client_opened_from_the_website_is_never_the_active_one() {
        // O cliente adotado não entra em `ours`: focá-lo não promove nada — e
        // ele próprio nunca recebe política (não está em `ours`).
        assert_eq!(next_active_pid(None, Some(77), &set(&[10, 20])), None);
    }

    #[test]
    fn the_active_client_is_dropped_when_it_closes() {
        assert_eq!(next_active_pid(Some(10), Some(999), &set(&[20])), None);
    }

    // ── velocidade desejada ────────────────────────────────────────────────

    #[test]
    fn the_active_client_runs_at_full_speed_and_the_others_go_to_the_background() {
        let now = Instant::now();
        assert_eq!(desired_speed(10, Some(10), now, now), ClientSpeed::Full);
        assert_eq!(desired_speed(20, Some(10), now, now), ClientSpeed::Background);
    }

    #[test]
    fn a_new_client_keeps_full_speed_during_the_grace_period() {
        let now = Instant::now();
        let until = now + FOCUS_FOLLOW_GRACE;
        assert_eq!(desired_speed(20, Some(10), until, now), ClientSpeed::Full);
        assert_eq!(
            desired_speed(20, Some(10), until, until + Duration::from_millis(1)),
            ClientSpeed::Background
        );
    }

    #[test]
    fn the_grace_period_is_35_seconds() {
        assert_eq!(FOCUS_FOLLOW_GRACE, Duration::from_secs(35));
    }

    #[test]
    fn with_no_client_in_use_every_loaded_client_is_in_the_background() {
        let now = Instant::now();
        assert_eq!(desired_speed(10, None, now, now), ClientSpeed::Background);
    }

    // ── plano de mudanças ──────────────────────────────────────────────────

    #[test]
    fn only_clients_whose_speed_changes_are_touched() {
        let now = Instant::now();
        let clients = [(10, now), (20, now), (30, now)];
        let mut applied = HashMap::new();
        applied.insert(10, ClientSpeed::Full);
        applied.insert(20, ClientSpeed::Background);
        // 30 nunca recebeu nada; 10 e 20 já estão como devem.
        assert_eq!(
            plan_speed_changes(&clients, &applied, Some(10), now),
            vec![(30, ClientSpeed::Background)]
        );
    }

    #[test]
    fn switching_windows_swaps_exactly_two_clients() {
        let now = Instant::now();
        let clients = [(10, now), (20, now), (30, now)];
        let mut applied = HashMap::new();
        applied.insert(10, ClientSpeed::Full);
        applied.insert(20, ClientSpeed::Background);
        applied.insert(30, ClientSpeed::Background);
        assert_eq!(
            plan_speed_changes(&clients, &applied, Some(20), now),
            vec![(10, ClientSpeed::Background), (20, ClientSpeed::Full)]
        );
    }

    // ── políticas ──────────────────────────────────────────────────────────

    #[test]
    fn the_background_uses_the_profile_policy_when_the_user_turned_it_on() {
        let launch = WindowsProcessPolicy {
            enabled: true,
            delay_ms: 1500,
            priority_class: WindowsPriorityClass::Idle,
            background_mode: true,
            eco_qos: false,
            ignore_timer_resolution: false,
            memory_priority: WindowsMemoryPriority::VeryLow,
        };
        let bg = background_process_policy(&launch);
        assert_eq!(bg.priority_class, WindowsPriorityClass::Idle);
        assert!(bg.background_mode);
        assert_eq!(bg.memory_priority, WindowsMemoryPriority::VeryLow);
        // Sem espera: a troca de janela tem que ser imediata.
        assert_eq!(bg.delay_ms, 0);
    }

    #[test]
    fn the_background_has_a_light_policy_when_the_profile_policy_is_off() {
        let bg = background_process_policy(&WindowsProcessPolicy::default());
        assert!(bg.enabled);
        assert_eq!(bg.priority_class, WindowsPriorityClass::BelowNormal);
        assert!(!bg.background_mode, "Idle é pesado demais como padrão");
        assert!(bg.eco_qos);
        assert_eq!(bg.memory_priority, WindowsMemoryPriority::Low);
    }

    #[test]
    fn full_speed_ignores_background_mode_and_eco_qos() {
        let full = full_speed_process_policy();
        assert_eq!(full.priority_class, WindowsPriorityClass::Normal);
        assert!(!full.background_mode);
        assert!(!full.eco_qos);
        assert!(!full.ignore_timer_resolution);
        assert_eq!(full.memory_priority, WindowsMemoryPriority::Normal);
    }

    #[test]
    fn the_cpu_cap_is_lifted_for_the_active_client_and_restored_in_the_background() {
        let capped = WindowsExperimentalPolicy {
            enable_job_cpu_limit: true,
            job_cpu_limit_percent: 30,
            ..Default::default()
        };
        assert_eq!(cpu_cap_for(ClientSpeed::Full, &capped), Some(None));
        assert_eq!(cpu_cap_for(ClientSpeed::Background, &capped), Some(Some(30)));
    }

    #[test]
    fn without_a_cpu_cap_in_the_profile_the_job_is_never_touched() {
        let none = WindowsExperimentalPolicy::default();
        assert_eq!(cpu_cap_for(ClientSpeed::Full, &none), None);
        assert_eq!(cpu_cap_for(ClientSpeed::Background, &none), None);
    }

    #[test]
    fn turning_the_option_off_gives_each_client_back_what_the_launch_gave() {
        let mut profile = WindowsOptimizationProfile::default();
        let (process, cap) = release_targets(&profile);
        assert_eq!(process.priority_class, WindowsPriorityClass::Normal);
        assert!(!process.eco_qos);
        assert_eq!(cap, None);

        profile.process.enabled = true;
        profile.process.priority_class = WindowsPriorityClass::BelowNormal;
        profile.experimental.enable_job_cpu_limit = true;
        profile.experimental.job_cpu_limit_percent = 20;
        let (process, cap) = release_targets(&profile);
        assert_eq!(process.priority_class, WindowsPriorityClass::BelowNormal);
        assert_eq!(cap, Some(Some(20)));
    }

    #[test]
    fn with_the_option_on_the_launch_leaves_the_priority_to_the_loop_but_keeps_the_job() {
        let mut profile = WindowsOptimizationProfile::default();
        profile.process.enabled = true;
        profile.experimental.enable_job_memory_limit = true;

        let under = launch_profile_under_focus_follow(profile.clone(), true);
        assert!(!under.process.enabled);
        assert!(under.experimental.enable_job_memory_limit);

        let off = launch_profile_under_focus_follow(profile, false);
        assert!(off.process.enabled);
    }

    #[test]
    fn the_option_is_off_by_default() {
        let path = std::env::temp_dir().join(format!(
            "ram4-focus-follow-{}-{}.ini",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let settings = SettingsStore::new(path.clone());
        assert!(!focus_follow_enabled(&settings));
        settings.set("Optimization", "FollowFocus", "true").unwrap();
        assert!(focus_follow_enabled(&settings));
        let _ = std::fs::remove_file(&path);
    }
}
