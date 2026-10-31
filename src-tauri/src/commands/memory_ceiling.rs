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

/// Linha do Console de um aviso (traduzível, como as do Watcher).
fn memory_console_line(notice: &MemoryNotice) -> LogMsg {
    match notice {
        MemoryNotice::Trimmed { memory_mb, limit_mb, ok: true, .. } => log_msg!(
            "Memory at {{memory}} MB, above the {{limit}} MB limit: asked Windows to free some",
            memory = *memory_mb,
            limit = *limit_mb
        ),
        MemoryNotice::Trimmed { memory_mb, limit_mb, ok: false, .. } => log_msg!(
            "Memory at {{memory}} MB, above the {{limit}} MB limit: Windows refused to free it",
            memory = *memory_mb,
            limit = *limit_mb
        ),
        MemoryNotice::Closed { memory_mb, limit_mb, .. } => log_msg!(
            "Client closed: memory stayed at {{memory}} MB, above the {{limit}} MB limit, after freeing",
            memory = *memory_mb,
            limit = *limit_mb
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
                notify_webhook(
                    app,
                    WebhookEventKind::MemoryClosed,
                    *user_id,
                    Some(WebhookReason::MemoryOverLimit { memory_mb: *memory_mb, limit_mb: *limit_mb }),
                    webhook_place_of(*user_id),
                );
            }
        }
    }
}

#[cfg(test)]
mod memory_ceiling_tests {
    use super::*;
    use std::cell::RefCell;

    const START: i64 = 1_000_000;
    const AFTER_GRACE: i64 = START + MEMORY_STARTUP_GRACE_MS;

    #[test]
    fn the_accounts_own_limit_wins_over_the_default() {
        assert_eq!(effective_memory_limit(Some("2048"), "1024"), Some(2048));
        assert_eq!(effective_memory_limit(None, "1024"), Some(1024));
        assert_eq!(effective_memory_limit(Some(""), "1536"), Some(1536), "empty field follows the default");
    }

    #[test]
    fn zero_or_off_means_no_limit_even_with_a_default() {
        assert_eq!(effective_memory_limit(Some("0"), "1024"), None);
        assert_eq!(effective_memory_limit(Some("off"), "1024"), None);
        assert_eq!(effective_memory_limit(None, "0"), None);
        assert_eq!(effective_memory_limit(None, ""), None, "off by default");
        assert_eq!(effective_memory_limit(Some("garbage"), "abc"), None);
    }

    #[test]
    fn a_limit_is_kept_between_256_mb_and_64_gb() {
        assert_eq!(effective_memory_limit(Some("10"), ""), Some(MEMORY_LIMIT_MIN_MB));
        assert_eq!(effective_memory_limit(Some("999999"), ""), Some(MEMORY_LIMIT_MAX_MB));
    }

    #[test]
    fn over_the_limit_it_frees_memory_first() {
        let mut watch = MemoryWatch::new(7, START);
        assert_eq!(
            memory_ceiling_step(&mut watch, AFTER_GRACE, Some(2500), Some(2048), false, true),
            MemoryAction::Trim
        );
    }

    #[test]
    fn a_client_that_just_opened_is_left_alone() {
        let mut watch = MemoryWatch::new(7, START);
        assert_eq!(
            memory_ceiling_step(&mut watch, START + 5_000, Some(4000), Some(2048), false, true),
            MemoryAction::Nothing
        );
    }

    #[test]
    fn it_closes_only_if_still_over_a_minute_later_and_closing_is_on() {
        let mut watch = MemoryWatch::new(7, START);
        let t0 = AFTER_GRACE;
        assert_eq!(memory_ceiling_step(&mut watch, t0, Some(2500), Some(2048), false, true), MemoryAction::Trim);
        // Dentro da carência depois de liberar: espera.
        assert_eq!(
            memory_ceiling_step(&mut watch, t0 + 30_000, Some(2500), Some(2048), false, true),
            MemoryAction::Nothing
        );
        assert_eq!(
            memory_ceiling_step(&mut watch, t0 + MEMORY_TRIM_GRACE_MS, Some(2500), Some(2048), false, true),
            MemoryAction::Close
        );
    }

    /// Sem a opção de fechar e continuando acima: pede de novo, mas só depois
    /// do intervalo mínimo (5 min) — nunca a cada minuto.
    #[test]
    fn without_the_close_option_it_frees_again_only_after_the_minimum_interval() {
        let mut watch = MemoryWatch::new(7, START);
        let t0 = AFTER_GRACE;
        assert_eq!(memory_ceiling_step(&mut watch, t0, Some(2500), Some(2048), false, false), MemoryAction::Trim);
        let mut t = t0 + 2_000;
        while t < t0 + MEMORY_RETRIM_INTERVAL_MS {
            assert_eq!(
                memory_ceiling_step(&mut watch, t, Some(2500), Some(2048), false, false),
                MemoryAction::Nothing,
                "trimmed again {} ms after the last one",
                t - t0
            );
            t += 2_000;
        }
        assert_eq!(
            memory_ceiling_step(&mut watch, t0 + MEMORY_RETRIM_INTERVAL_MS, Some(2500), Some(2048), false, false),
            MemoryAction::Trim
        );
    }

    /// Liberou e a memória caiu: subir de novo não libera na hora. Só depois de
    /// ficar acima por um tempo seguido **e** de passado o intervalo mínimo.
    #[test]
    fn going_back_over_after_a_trim_waits_for_a_sustained_rise_and_the_interval() {
        let mut watch = MemoryWatch::new(7, START);
        let t0 = AFTER_GRACE;
        memory_ceiling_step(&mut watch, t0, Some(2500), Some(2048), false, true);
        // Liberou e ficou abaixo.
        assert_eq!(
            memory_ceiling_step(&mut watch, t0 + 2_000, Some(400), Some(2048), false, true),
            MemoryAction::Nothing
        );
        assert_eq!(watch.over_since_ms, None);
        assert_eq!(watch.trimmed_at_ms, Some(t0), "the last trim is remembered");
        // Subiu de novo cedo (1 min depois): o intervalo mínimo ainda não passou.
        let mut t = t0 + 60_000;
        while t < t0 + MEMORY_RETRIM_INTERVAL_MS {
            assert_eq!(
                memory_ceiling_step(&mut watch, t, Some(2300), Some(2048), false, true),
                MemoryAction::Nothing,
                "acted {} ms after the last trim",
                t - t0
            );
            t += 2_000;
        }
        // Passou o intervalo e está acima há mais que o tempo seguido: libera
        // (não fecha: esta subida ainda não foi liberada).
        assert_eq!(
            memory_ceiling_step(&mut watch, t0 + MEMORY_RETRIM_INTERVAL_MS, Some(2300), Some(2048), false, true),
            MemoryAction::Trim
        );
    }

    /// Um pico curto depois do intervalo mínimo não basta: a subida tem que
    /// durar o tempo seguido.
    #[test]
    fn a_short_spike_after_the_interval_does_not_trim() {
        let mut watch = MemoryWatch::new(7, START);
        let t0 = AFTER_GRACE;
        memory_ceiling_step(&mut watch, t0, Some(2500), Some(2048), false, true);
        memory_ceiling_step(&mut watch, t0 + 2_000, Some(400), Some(2048), false, true);
        let late = t0 + 10 * 60_000;
        assert_eq!(memory_ceiling_step(&mut watch, late, Some(2300), Some(2048), false, true), MemoryAction::Nothing);
        assert_eq!(
            memory_ceiling_step(&mut watch, late + 20_000, Some(2300), Some(2048), false, true),
            MemoryAction::Nothing
        );
        // Caiu antes de completar: recomeça a contagem.
        memory_ceiling_step(&mut watch, late + 22_000, Some(1900), Some(2048), false, true);
        assert_eq!(
            memory_ceiling_step(&mut watch, late + 24_000, Some(2300), Some(2048), false, true),
            MemoryAction::Nothing
        );
        assert_eq!(
            memory_ceiling_step(
                &mut watch,
                late + 24_000 + MEMORY_RETRIM_SUSTAIN_MS - 2_000,
                Some(2300),
                Some(2048),
                false,
                true
            ),
            MemoryAction::Nothing
        );
        assert_eq!(
            memory_ceiling_step(
                &mut watch,
                late + 24_000 + MEMORY_RETRIM_SUSTAIN_MS,
                Some(2300),
                Some(2048),
                false,
                true
            ),
            MemoryAction::Trim
        );
    }

    /// Voltou a subir depois de liberar, com a opção de fechar ligada: libera
    /// de novo primeiro e só fecha se continuar acima um minuto depois disso.
    #[test]
    fn closing_needs_a_trim_in_the_same_rise() {
        let mut watch = MemoryWatch::new(7, START);
        let t0 = AFTER_GRACE;
        memory_ceiling_step(&mut watch, t0, Some(2500), Some(2048), false, true);
        memory_ceiling_step(&mut watch, t0 + 2_000, Some(400), Some(2048), false, true);
        let back = t0 + 20 * 60_000;
        // Acima de novo: nada até a subida durar; depois libera, não fecha.
        assert_eq!(memory_ceiling_step(&mut watch, back, Some(2500), Some(2048), false, true), MemoryAction::Nothing);
        let retrim = back + MEMORY_RETRIM_SUSTAIN_MS;
        assert_eq!(memory_ceiling_step(&mut watch, retrim, Some(2500), Some(2048), false, true), MemoryAction::Trim);
        assert_eq!(
            memory_ceiling_step(&mut watch, retrim + 30_000, Some(2500), Some(2048), false, true),
            MemoryAction::Nothing
        );
        assert_eq!(
            memory_ceiling_step(&mut watch, retrim + MEMORY_TRIM_GRACE_MS, Some(2500), Some(2048), false, true),
            MemoryAction::Close
        );
    }

    /// Liberou e caiu para baixo antes do minuto: não fecha, mesmo com a
    /// opção ligada e mesmo que volte a subir logo depois.
    #[test]
    fn going_under_after_a_trim_never_closes_without_a_new_trim() {
        let mut watch = MemoryWatch::new(7, START);
        let t0 = AFTER_GRACE;
        memory_ceiling_step(&mut watch, t0, Some(2500), Some(2048), false, true);
        memory_ceiling_step(&mut watch, t0 + 10_000, Some(600), Some(2048), false, true);
        let mut t = t0 + 12_000;
        while t < t0 + MEMORY_RETRIM_INTERVAL_MS {
            assert_eq!(
                memory_ceiling_step(&mut watch, t, Some(2500), Some(2048), false, true),
                MemoryAction::Nothing
            );
            t += 2_000;
        }
    }

    /// A sequência que o dono viu (11/10/2026): o número ficava acima do
    /// limite logo depois de cada liberação. Em 20 min de leituras acima, com
    /// a opção de fechar desligada, o app libera no máximo uma vez a cada 5 min.
    #[test]
    fn a_client_that_stays_over_is_trimmed_at_most_every_five_minutes() {
        let mut watch = MemoryWatch::new(7, START);
        let mut trims = Vec::new();
        let mut t = AFTER_GRACE;
        while t < AFTER_GRACE + 20 * 60_000 {
            if memory_ceiling_step(&mut watch, t, Some(4000), Some(2048), false, false) == MemoryAction::Trim {
                trims.push(t);
            }
            t += 2_000;
        }
        assert_eq!(trims.len(), 4, "{trims:?}");
        for pair in trims.windows(2) {
            assert!(pair[1] - pair[0] >= MEMORY_RETRIM_INTERVAL_MS);
        }
    }

    /// Tirar o limite esquece a subida, mas não a última liberação: pôr de
    /// novo logo depois não libera em seguida.
    #[test]
    fn removing_and_restoring_the_limit_does_not_trim_right_away() {
        let mut watch = MemoryWatch::new(7, START);
        let t0 = AFTER_GRACE;
        memory_ceiling_step(&mut watch, t0, Some(2500), Some(2048), false, false);
        assert_eq!(memory_ceiling_step(&mut watch, t0 + 2_000, Some(2500), None, false, false), MemoryAction::Nothing);
        assert_eq!(watch.over_since_ms, None);
        assert_eq!(
            memory_ceiling_step(&mut watch, t0 + 4_000, Some(2500), Some(2048), false, false),
            MemoryAction::Nothing
        );
    }

    /// O Watcher ("Close If Memory Low") não julga um cliente que o teto
    /// liberou: depois de liberar, a memória fica baixa até o jogo precisar.
    #[test]
    fn a_trimmed_client_is_known_to_the_low_memory_rule() {
        let os = FakeOs {
            memory: HashMap::from([(10, 2500)]),
            ..Default::default()
        };
        let mut monitor = MemoryCeilingMonitor::default();
        let clients = [client(1, 10, Some(2048))];
        monitor.tick(&os, &clients, false, START);
        assert!(monitor.views[&1].trimmed_at_ms.is_none());
        monitor.tick(&os, &clients, false, AFTER_GRACE);
        assert!(view_was_trimmed(monitor.views.get(&1)));
        assert!(!view_was_trimmed(None));
    }

    /// A leitura vai para a tela a cada passada (a cada 2 s), não só na
    /// primeira: o número da linha "Em jogo" acompanha o cliente.
    #[test]
    fn the_view_follows_every_reading() {
        let mut os = FakeOs {
            memory: HashMap::from([(10, 1200)]),
            ..Default::default()
        };
        let mut monitor = MemoryCeilingMonitor::default();
        let clients = [client(1, 10, Some(2048))];
        monitor.tick(&os, &clients, false, START);
        assert_eq!(monitor.views[&1].memory_mb, Some(1200));
        os.memory.insert(10, 1350);
        monitor.tick(&os, &clients, false, START + 2_000);
        assert_eq!(monitor.views[&1].memory_mb, Some(1350));
        os.memory.insert(10, 980);
        monitor.tick(&os, &clients, false, AFTER_GRACE + 60_000);
        assert_eq!(monitor.views[&1].memory_mb, Some(980));
    }

    #[test]
    fn the_window_in_use_is_never_touched() {
        let mut watch = MemoryWatch::new(7, START);
        assert_eq!(
            memory_ceiling_step(&mut watch, AFTER_GRACE, Some(4000), Some(2048), true, true),
            MemoryAction::Nothing
        );
        assert_eq!(watch.trimmed_at_ms, None);
    }

    #[test]
    fn without_a_limit_nothing_happens() {
        let mut watch = MemoryWatch::new(7, START);
        assert_eq!(
            memory_ceiling_step(&mut watch, AFTER_GRACE, Some(9000), None, false, true),
            MemoryAction::Nothing
        );
    }

    /// Dublê do sistema: memória por PID, quem está em uso, e o que foi pedido.
    #[derive(Default)]
    struct FakeOs {
        memory: HashMap<u32, u64>,
        foreground: Option<u32>,
        trimmed: RefCell<Vec<u32>>,
        closed: RefCell<Vec<i64>>,
    }

    impl MemoryOs for FakeOs {
        fn memory_mb(&self, pid: u32) -> Option<u64> {
            self.memory.get(&pid).copied()
        }
        fn foreground_pid(&self) -> Option<u32> {
            self.foreground
        }
        fn trim(&self, pid: u32) -> bool {
            self.trimmed.borrow_mut().push(pid);
            true
        }
        fn close(&self, user_id: i64) -> bool {
            self.closed.borrow_mut().push(user_id);
            true
        }
    }

    fn client(user_id: i64, pid: u32, limit: Option<u64>) -> MemoryClient {
        MemoryClient { user_id, pid, limit_mb: limit }
    }

    #[test]
    fn the_monitor_frees_then_closes_only_the_client_over_its_limit() {
        let os = FakeOs {
            memory: HashMap::from([(10, 2500), (20, 900)]),
            ..Default::default()
        };
        let clients = [client(1, 10, Some(2048)), client(2, 20, Some(2048))];
        let mut monitor = MemoryCeilingMonitor::default();
        assert!(monitor.tick(&os, &clients, true, START).is_empty());
        let notices = monitor.tick(&os, &clients, true, AFTER_GRACE);
        assert_eq!(
            notices,
            vec![MemoryNotice::Trimmed { user_id: 1, memory_mb: 2500, limit_mb: 2048, ok: true }]
        );
        assert_eq!(*os.trimmed.borrow(), vec![10]);
        let view = monitor.views.get(&1).cloned().unwrap();
        assert!(view.over);
        assert_eq!(view.memory_mb, Some(2500));
        assert_eq!(view.trimmed_at_ms, Some(AFTER_GRACE));
        assert!(!monitor.views[&2].over);

        let notices = monitor.tick(&os, &clients, true, AFTER_GRACE + MEMORY_TRIM_GRACE_MS);
        assert_eq!(
            notices,
            vec![MemoryNotice::Closed { user_id: 1, memory_mb: 2500, limit_mb: 2048 }]
        );
        assert_eq!(*os.closed.borrow(), vec![1]);
        assert!(!monitor.views.contains_key(&1));
    }

    #[test]
    fn the_monitor_skips_the_window_in_use_and_clients_that_left() {
        let os = FakeOs {
            memory: HashMap::from([(10, 4000)]),
            foreground: Some(10),
            ..Default::default()
        };
        let mut monitor = MemoryCeilingMonitor::default();
        monitor.tick(&os, &[client(1, 10, Some(1024))], true, START);
        assert!(monitor.tick(&os, &[client(1, 10, Some(1024))], true, AFTER_GRACE).is_empty());
        assert!(os.trimmed.borrow().is_empty());
        // A conta fechou: sai do retrato.
        monitor.tick(&os, &[], true, AFTER_GRACE + 2_000);
        assert!(monitor.views.is_empty());
    }

    #[test]
    fn a_new_client_of_the_same_account_gets_a_new_grace() {
        let os = FakeOs {
            memory: HashMap::from([(10, 4000), (11, 4000)]),
            ..Default::default()
        };
        let mut monitor = MemoryCeilingMonitor::default();
        monitor.tick(&os, &[client(1, 10, Some(1024))], true, START);
        assert!(monitor.tick(&os, &[client(1, 11, Some(1024))], true, AFTER_GRACE).is_empty());
    }

    #[test]
    fn the_console_line_says_what_happened() {
        assert_eq!(
            memory_console_line(&MemoryNotice::Trimmed { user_id: 1, memory_mb: 2500, limit_mb: 2048, ok: true }).render(),
            "Memory at 2500 MB, above the 2048 MB limit: asked Windows to free some"
        );
        assert!(memory_console_line(&MemoryNotice::Closed { user_id: 1, memory_mb: 2500, limit_mb: 2048 })
            .render()
            .starts_with("Client closed"));
    }

    #[test]
    fn the_view_reaches_the_screen_in_camel_case() {
        let view = ClientMemoryView {
            memory_mb: Some(1200),
            limit_mb: Some(2048),
            over: false,
            trimmed_at_ms: None,
        };
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["memoryMb"], 1200);
        assert_eq!(json["limitMb"], 2048);
        assert_eq!(json["over"], false);
        assert!(json["trimmedAtMs"].is_null());
    }

    fn temp_settings(tag: &str) -> SettingsStore {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        SettingsStore::new(std::env::temp_dir().join(format!("ram-memceil-{tag}-{nanos}.ini")))
    }

    /// Fechar acima do limite tem opção própria, desligada por padrão.
    #[test]
    fn closing_over_the_limit_is_off_by_default() {
        let settings = temp_settings("close-default");
        assert!(!memory_close_allowed(&settings));
    }

    /// Até 11/10/2026 o teto usava o "Close If Memory Low" do Watcher, que
    /// também liga a regra de memória **baixa** (outra coisa). Ligar a regra do
    /// Watcher não deixa mais o teto fechar ninguém.
    #[test]
    fn the_watchers_low_memory_switch_no_longer_allows_closing() {
        let settings = temp_settings("close-watcher");
        settings.set("Watcher", "Enabled", "true").unwrap();
        settings.set("Watcher", "CloseRbxMemory", "true").unwrap();
        assert!(!memory_close_allowed(&settings));
    }

    /// A opção própria vale sozinha: o teto roda com o Watcher desligado, e a
    /// opção mora ao lado do limite, não no Watcher.
    #[test]
    fn the_own_option_allows_closing_even_with_the_watcher_off() {
        let settings = temp_settings("close-own");
        settings.set("Optimization", "CloseOverMemoryLimit", "true").unwrap();
        assert!(memory_close_allowed(&settings));
        settings.set("Optimization", "CloseOverMemoryLimit", "false").unwrap();
        assert!(!memory_close_allowed(&settings));
    }

    #[test]
    fn the_memory_ceiling_follows_the_build_feature() {
        assert_eq!(
            memory_trim_supported(),
            cfg!(all(target_os = "windows", feature = "memory-trim"))
        );
    }
}
