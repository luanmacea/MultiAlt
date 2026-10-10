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
    state.applied.clear();
    state.grace_until.clear();
}

/// A parte de velocidade da olhada (`Optimization.FollowFocus`).
fn speed_tick(
    settings: &SettingsStore,
    state: &mut FocusFollowState,
    ours: &std::collections::HashSet<u32>,
) {
    let now = Instant::now();
    let first_look = !state.running;
    state.running = true;
    for &pid in ours {
        state.grace_until.entry(pid).or_insert(if first_look {
            // Já estava aberto quando a opção ligou: sem carência.
            now
        } else {
            now + FOCUS_FOLLOW_GRACE
        });
    }

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
        let launch = launch_optimization_for(settings, state, pid);
        let process = match speed {
            ClientSpeed::Full => full_speed_process_policy(),
            ClientSpeed::Background => background_process_policy(&launch.process),
        };
        apply_client_speed(pid, &process, cpu_cap_for(speed, &launch.experimental), &alive);
        state.applied.insert(pid, speed);
    }
}

/// Uma olhada: chamada a cada segundo pelo laço de `commands/focus_follow.rs`.
/// Serve à otimização que segue o foco e ao volume ao vivo (`live_audio.rs`),
/// que dividem o "cliente em uso".
pub fn focus_follow_tick(settings: &SettingsStore) {
    let follow = focus_follow_enabled(settings);
    let mute = mute_background_enabled(settings);
    let ours = launched_client_pids();

    let active = {
        let mut state = focus_follow_state();
        // Cliente que saiu do rastreamento: esquece (o PID pode voltar a outro
        // processo).
        state.applied.retain(|pid, _| ours.contains(pid));
        state.grace_until.retain(|pid, _| ours.contains(pid));
        state.launch_profiles.retain(|pid, _| ours.contains(pid));

        state.active = if follow || mute {
            let foreground = window_pid(unsafe { GetForegroundWindow() });
            next_active_pid(state.active, foreground, &ours)
        } else {
            None
        };

        if follow {
            speed_tick(settings, &mut state, &ours);
        } else if state.running || !state.applied.is_empty() {
            release_locked(settings, &mut state);
        }
        state.active
    };

    live_audio_tick(mute, &ours, active);
}

/// Ao fechar o app: devolve os clientes ao estado do launch e desmuta o que o
/// app mutou.
pub fn release_focus_follow(settings: &SettingsStore) {
    {
        let mut state = focus_follow_state();
        release_locked(settings, &mut state);
    }
    release_live_audio();
}
