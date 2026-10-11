// Teto de memória que libera RAM antes de fechar o cliente. Ver
// docs/features/watcher.md ("Teto de memória").
//
// Cada cliente **que o app abriu** pode ter um limite de memória: o padrão de
// todos (`Optimization.MemoryLimit`, MB, 0 = sem limite) ou o da conta (campo
// `MemoryLimit`, que a lista "Em jogo" da página Session grava). Passou do
// limite: primeiro o app pede ao Windows para tirar da RAM o que o cliente não
// está usando (`trim_working_set`, em platform/windows). Só se continuar acima
// um minuto depois **e** a opção própria de fechar estiver ligada
// (`Optimization.CloseOverMemoryLimit`, desligada por padrão), o cliente é
// fechado. Sem ela, o app pede de novo — no máximo a cada 5 minutos, e só com
// o cliente acima por um minuto seguido (histerese: nunca libera em laço). Até
// 11/10/2026 a opção era o "Close If Memory Low" do Watcher, que liga também a
// regra de memória baixa — coisa sem relação; agora as duas são separadas.
//
// Nunca toca cliente aberto pelo site (adotado), nem a janela que a pessoa está
// usando agora. A leitura de memória é a **memória privada** (o número da coluna
// "Memória" do Gerenciador de Tarefas, `get_process_memory_mb`), a mesma que a
// tela mostra e a mesma do Watcher.
//
// O pedido ao Windows só existe com a feature `memory-trim` (nas duas edições,
// como o plano das ideias decidiu); sem ela, o teto inteiro fica desligado e a
// tela não mostra a opção.

/// Carência de um cliente recém-aberto: carregando, a memória sobe e desce.
const MEMORY_STARTUP_GRACE_MS: i64 = 30_000;
/// Depois de liberar, quanto esperar antes de conferir de novo (e fechar, se a
/// opção estiver ligada e o cliente continuar acima o tempo todo).
const MEMORY_TRIM_GRACE_MS: i64 = 60_000;
/// Intervalo mínimo entre duas liberações do mesmo cliente. Até 11/10/2026 era
/// um minuto, e com a leitura errada (working set inteiro, ver
/// `get_process_memory_mb`) o cliente nunca ficava abaixo: liberava em laço.
const MEMORY_RETRIM_INTERVAL_MS: i64 = 300_000;
/// Depois de uma liberação, quanto tempo seguido acima do limite antes de
/// liberar de novo (um pico curto não conta).
const MEMORY_RETRIM_SUSTAIN_MS: i64 = 60_000;
/// Limite mínimo aceito: abaixo disso o cliente nem joga, e o app ficaria
/// pedindo para liberar memória o tempo todo.
const MEMORY_LIMIT_MIN_MB: u64 = 256;
const MEMORY_LIMIT_MAX_MB: u64 = 65_536;

/// O pedido ao Windows existe neste binário?
fn memory_trim_supported() -> bool {
    cfg!(all(target_os = "windows", feature = "memory-trim"))
}

/// Um valor de limite escrito (INI ou campo da conta): `Some(0)` = sem limite,
/// `Some(mb)` = limite, `None` = vazio ou ilegível.
fn parse_memory_limit(raw: &str) -> Option<u64> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if raw.eq_ignore_ascii_case("off") {
        return Some(0);
    }
    let mb = raw.parse::<u64>().ok()?;
    Some(if mb == 0 {
        0
    } else {
        mb.clamp(MEMORY_LIMIT_MIN_MB, MEMORY_LIMIT_MAX_MB)
    })
}

/// O limite que vale para a conta: o campo dela vence; sem ele, o padrão.
/// `None` = sem limite.
fn effective_memory_limit(account_field: Option<&str>, default_raw: &str) -> Option<u64> {
    let own = account_field.and_then(parse_memory_limit);
    let mb = own.or_else(|| parse_memory_limit(default_raw)).unwrap_or(0);
    (mb > 0).then_some(mb)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryAction {
    Nothing,
    /// Pedir ao Windows para liberar a memória do cliente.
    Trim,
    /// Continuou acima depois de liberar, e fechar está permitido.
    Close,
}

/// O acompanhamento de um cliente.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MemoryWatch {
    pid: u32,
    first_seen_ms: i64,
    /// Quando o app pediu para liberar pela última vez. Fica guardado mesmo
    /// depois de a memória voltar para baixo: é dele que conta o intervalo
    /// mínimo até a próxima liberação.
    trimmed_at_ms: Option<i64>,
    /// Desde quando a memória está acima do limite sem parar (`None` = abaixo).
    over_since_ms: Option<i64>,
}

/// O cliente deste retrato já foi liberado pelo teto (neste PID)? O "Close If
/// Memory Low" do Watcher não o julga: depois de liberar, a memória fica baixa
/// até o jogo precisar de novo, e a regra fecharia um cliente saudável.
fn view_was_trimmed(view: Option<&ClientMemoryView>) -> bool {
    view.is_some_and(|v| v.trimmed_at_ms.is_some())
}

/// `view_was_trimmed` do cliente da conta agora (para o Watcher).
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn client_memory_was_trimmed(user_id: i64) -> bool {
    view_was_trimmed(client_memory_of(user_id).as_ref())
}

impl MemoryWatch {
    fn new(pid: u32, now_ms: i64) -> Self {
        Self {
            pid,
            first_seen_ms: now_ms,
            trimmed_at_ms: None,
            over_since_ms: None,
        }
    }
}

/// A decisão de uma passada para um cliente. `in_use`: é a janela que a
/// pessoa está usando agora (nunca é tocada; o estado fica como estava).
fn memory_ceiling_step(
    watch: &mut MemoryWatch,
    now_ms: i64,
    memory_mb: Option<u64>,
    limit_mb: Option<u64>,
    in_use: bool,
    close_allowed: bool,
) -> MemoryAction {
    let (Some(memory), Some(limit)) = (memory_mb, limit_mb) else {
        // Sem leitura ou sem limite: a subida acaba, a última liberação fica
        // (tirar e pôr o limite de volta não libera de novo na hora).
        watch.over_since_ms = None;
        return MemoryAction::Nothing;
    };
    if now_ms.saturating_sub(watch.first_seen_ms) < MEMORY_STARTUP_GRACE_MS {
        return MemoryAction::Nothing;
    }
    if memory <= limit {
        watch.over_since_ms = None;
        return MemoryAction::Nothing;
    }
    let over_since = *watch.over_since_ms.get_or_insert(now_ms);
    if in_use {
        return MemoryAction::Nothing;
    }
    let Some(trimmed_at) = watch.trimmed_at_ms else {
        // Primeira vez acima: libera.
        watch.trimmed_at_ms = Some(now_ms);
        return MemoryAction::Trim;
    };
    let since_trim = now_ms.saturating_sub(trimmed_at);
    if trimmed_at >= over_since {
        // Liberou nesta subida e continua acima desde então.
        if since_trim < MEMORY_TRIM_GRACE_MS {
            return MemoryAction::Nothing;
        }
        if close_allowed {
            return MemoryAction::Close;
        }
    }
    // Liberar de novo (sem a opção de fechar, ou numa subida nova depois de uma
    // liberação antiga): só com a subida durando e o intervalo mínimo passado.
    if now_ms.saturating_sub(over_since) >= MEMORY_RETRIM_SUSTAIN_MS && since_trim >= MEMORY_RETRIM_INTERVAL_MS {
        watch.trimmed_at_ms = Some(now_ms);
        return MemoryAction::Trim;
    }
    MemoryAction::Nothing
}

/// O que a tela recebe de cada cliente (em `get_running_instances`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientMemoryView {
    /// Memória privada agora (a do Gerenciador de Tarefas), relida a cada
    /// passada de 2 s.
    pub memory_mb: Option<u64>,
    /// O limite que vale agora (`None` = sem limite).
    pub limit_mb: Option<u64>,
    /// Acima do limite nesta passada.
    pub over: bool,
    /// Quando o app pediu para liberar pela última vez (neste PID). Fica depois
    /// de a memória cair: a tela explica que ela fica mais baixa até o jogo
    /// precisar de novo.
    pub trimmed_at_ms: Option<i64>,
}

/// Um cliente do app nesta passada.
#[derive(Debug, Clone)]
struct MemoryClient {
    user_id: i64,
    pid: u32,
    limit_mb: Option<u64>,
}

/// O que o monitor precisa do sistema (dublê nos testes).
trait MemoryOs {
    fn memory_mb(&self, pid: u32) -> Option<u64>;
    fn foreground_pid(&self) -> Option<u32>;
    fn trim(&self, pid: u32) -> bool;
    /// Fecha o cliente da conta (o mesmo `kill_for_user` do Watcher).
    fn close(&self, user_id: i64) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MemoryNotice {
    Trimmed { user_id: i64, memory_mb: u64, limit_mb: u64, ok: bool },
    Closed { user_id: i64, memory_mb: u64, limit_mb: u64 },
}

#[derive(Default)]
struct MemoryCeilingMonitor {
    watches: HashMap<i64, MemoryWatch>,
    views: HashMap<i64, ClientMemoryView>,
}

impl MemoryCeilingMonitor {
    fn tick(
        &mut self,
        os: &impl MemoryOs,
        clients: &[MemoryClient],
        close_allowed: bool,
        now_ms: i64,
    ) -> Vec<MemoryNotice> {
        let present: HashSet<i64> = clients.iter().map(|c| c.user_id).collect();
        self.watches.retain(|uid, _| present.contains(uid));
        self.views.retain(|uid, _| present.contains(uid));
        let foreground = os.foreground_pid();
        let mut notices = Vec::new();
        for client in clients {
            let watch = self
                .watches
                .entry(client.user_id)
                .or_insert_with(|| MemoryWatch::new(client.pid, now_ms));
            if watch.pid != client.pid {
                *watch = MemoryWatch::new(client.pid, now_ms);
            }
            let memory = os.memory_mb(client.pid);
            let action = memory_ceiling_step(
                watch,
                now_ms,
                memory,
                client.limit_mb,
                foreground == Some(client.pid),
                close_allowed,
            );
            let (memory_mb, limit_mb) = (memory.unwrap_or(0), client.limit_mb.unwrap_or(0));
            match action {
                MemoryAction::Nothing => {}
                MemoryAction::Trim => {
                    let ok = os.trim(client.pid);
                    notices.push(MemoryNotice::Trimmed {
                        user_id: client.user_id,
                        memory_mb,
                        limit_mb,
                        ok,
                    });
                }
                MemoryAction::Close => {
                    if os.close(client.user_id) {
                        notices.push(MemoryNotice::Closed {
                            user_id: client.user_id,
                            memory_mb,
                            limit_mb,
                        });
                        self.watches.remove(&client.user_id);
                        self.views.remove(&client.user_id);
                        continue;
                    }
                }
            }
            let over = matches!((memory, client.limit_mb), (Some(m), Some(l)) if m > l);
            let trimmed_at_ms = self.watches.get(&client.user_id).and_then(|w| w.trimmed_at_ms);
            self.views.insert(
                client.user_id,
                ClientMemoryView {
                    memory_mb: memory,
                    limit_mb: client.limit_mb,
                    over,
                    trimmed_at_ms,
                },
            );
        }
        notices
    }
}

/// Linha do Console de um aviso (português, como as do Watcher).
fn memory_console_line(notice: &MemoryNotice) -> String {
    match notice {
        MemoryNotice::Trimmed { memory_mb, limit_mb, ok: true, .. } => format!(
            "Memória em {memory_mb} MB, acima do limite de {limit_mb} MB: pedi ao Windows para liberar"
        ),
        MemoryNotice::Trimmed { memory_mb, limit_mb, ok: false, .. } => format!(
            "Memória em {memory_mb} MB, acima do limite de {limit_mb} MB: o Windows recusou liberar"
        ),
        MemoryNotice::Closed { memory_mb, limit_mb, .. } => format!(
            "Cliente fechado: a memória continuou em {memory_mb} MB, acima do limite de {limit_mb} MB, depois de liberar"
        ),
    }
}

/// Último retrato, para o `get_running_instances`.
static CLIENT_MEMORY_VIEWS: LazyLock<Mutex<HashMap<i64, ClientMemoryView>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn client_memory_of(user_id: i64) -> Option<ClientMemoryView> {
    CLIENT_MEMORY_VIEWS.lock().ok()?.get(&user_id).cloned()
}

#[cfg(target_os = "windows")]
struct WindowsMemoryOs;

#[cfg(target_os = "windows")]
impl MemoryOs for WindowsMemoryOs {
    fn memory_mb(&self, pid: u32) -> Option<u64> {
        platform::windows::get_process_memory_mb(pid)
    }
    fn foreground_pid(&self) -> Option<u32> {
        platform::windows::window_pid(platform::windows::get_foreground_hwnd())
    }
    fn trim(&self, pid: u32) -> bool {
        platform::windows::trim_working_set(pid)
    }
    fn close(&self, user_id: i64) -> bool {
        platform::windows::tracker().kill_for_user(user_id)
    }
}

#[cfg(target_os = "windows")]
static MEMORY_CEILING_MONITOR: LazyLock<Mutex<MemoryCeilingMonitor>> =
    LazyLock::new(|| Mutex::new(MemoryCeilingMonitor::default()));

/// Pode fechar o cliente que continua acima do limite depois de liberar?
/// `Optimization.CloseOverMemoryLimit`, desligado por padrão, ao lado do
/// limite. Não depende do Watcher: o teto roda com ele desligado, e o "Close
/// If Memory Low" dele é outra regra (memória **baixa**, cliente travado).
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn memory_close_allowed(settings: &SettingsStore) -> bool {
    settings.get_bool("Optimization", "CloseOverMemoryLimit")
}

/// Uma passada (no laço do monitor de quedas, a cada 2 s). Só clientes que o
/// app abriu; sem cliente, só lê o tracker.
#[cfg(target_os = "windows")]
fn memory_ceiling_pass(app: &tauri::AppHandle) -> Vec<MemoryNotice> {
    if !memory_trim_supported() {
        return Vec::new();
    }
    let launched: Vec<(i64, u32)> = platform::windows::tracker()
        .get_all()
        .into_iter()
        .filter(|process| !process.adopted)
        .map(|process| (process.user_id, process.pid))
        .collect();
    let settings = app.state::<SettingsStore>();
    let default_raw = settings.get_string("Optimization", "MemoryLimit");
    let close_allowed = memory_close_allowed(&settings);
    let fields: HashMap<i64, String> = if launched.is_empty() {
        HashMap::new()
    } else {
        app.state::<AccountStore>()
            .get_all()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|a| a.fields.get("MemoryLimit").map(|v| (a.user_id, v.clone())))
            .collect()
    };
    let clients: Vec<MemoryClient> = launched
        .into_iter()
        .map(|(user_id, pid)| MemoryClient {
            user_id,
            pid,
            limit_mb: effective_memory_limit(fields.get(&user_id).map(String::as_str), &default_raw),
        })
        .collect();
    let Ok(mut monitor) = MEMORY_CEILING_MONITOR.lock() else {
        return Vec::new();
    };
    let notices = monitor.tick(&WindowsMemoryOs, &clients, close_allowed, now_ms());
    if let Ok(mut views) = CLIENT_MEMORY_VIEWS.lock() {
        *views = monitor.views.clone();
    }
    notices
}

#[cfg(target_os = "windows")]
pub(crate) async fn memory_ceiling_tick(app: &tauri::AppHandle) {
    let handle = app.clone();
    let notices = tokio::task::spawn_blocking(move || memory_ceiling_pass(&handle))
        .await
        .unwrap_or_default();
    for notice in &notices {
        match notice {
            MemoryNotice::Trimmed { user_id, .. } => {
                emit_launch_log(app, *user_id, "info", "memory", memory_console_line(notice));
            }
            MemoryNotice::Closed { user_id, memory_mb, limit_mb } => {
                let _ = app.emit(
                    "roblox-memory-limit",
                    serde_json::json!({
                        "userId": user_id,
                        "memoryMb": memory_mb,
                        "limitMb": limit_mb,
                    }),
                );
                emit_launch_log(app, *user_id, "warn", "memory", memory_console_line(notice));
            }
        }
    }
}
