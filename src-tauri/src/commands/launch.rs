/// Captcha-safety floor. Roblox issues a "verify you're not a robot" challenge
/// when authentication-ticket redemptions from the same IP arrive too close
/// together, so never let the target spacing drop below this, regardless of
/// AccountJoinDelay / EnableMultiRbx.
const MIN_JOIN_GAP_SECS: u64 = 8;

/// Minimum residual gap between two consecutive account launches, even when the
/// account's own work (auth + PID wait) already ate the whole join delay.
const MIN_RESIDUAL_GAP_MS: u64 = 5000;

/// Human-readable launch target for the console log line.
fn launch_target_description(join_vip: bool, link_code: &str, job_id: &str) -> String {
    if join_vip || !link_code.trim().is_empty() {
        "servidor VIP/privado".to_string()
    } else if !job_id.trim().is_empty() {
        format!("servidor {}", job_id.trim())
    } else {
        "servidor público".to_string()
    }
}

/// Full isolation wipes the default Roblox install, so a launch on the default
/// install has to re-download the client first. A pinned catalog version lives
/// outside that folder and is therefore untouched.
fn isolation_wipes_install(isolation_mode: &str, version_id: Option<&str>) -> bool {
    isolation_mode.eq_ignore_ascii_case("Full") && version_id.is_none()
}

/// Old-join launches the exe directly from a known folder. That folder is about
/// to be wiped when isolation runs in Full mode on the default install, so the
/// protocol handler is used instead; a pinned version always uses old join.
fn resolve_use_old_join(
    isolation_wipes_install: bool,
    configured_old_join: bool,
    version_id: Option<&str>,
) -> bool {
    if isolation_wipes_install {
        false
    } else {
        configured_old_join || version_id.is_some()
    }
}

/// True when a client is already running on a *different* Roblox version than
/// the one about to launch. Concurrent multi-version clients are not supported.
fn has_version_conflict(
    running_keys: &HashSet<Option<String>>,
    target_version_id: &Option<String>,
) -> bool {
    running_keys.iter().any(|k| k != target_version_id)
}

/// A guarda que o launch aplica de fato. Com
/// `Versions.AllowLaunchOnOpenVersion` ligado, abrir numa versão que **já tem
/// cliente aberto** passa, mesmo com outras versões abertas ao lado — é o que
/// destrava o launch depois de o Auto Rejoin (que não checa conflito) deixar
/// duas versões abertas. Desligado (padrão), é a guarda estrita.
fn version_guard_blocks(
    running_keys: &HashSet<Option<String>>,
    target_version_id: &Option<String>,
    allow_launch_on_open_version: bool,
) -> bool {
    if allow_launch_on_open_version && running_keys.contains(target_version_id) {
        return false;
    }
    has_version_conflict(running_keys, target_version_id)
}

/// Como uma versão aparece na frase da guarda. A instalação do sistema entra
/// como texto em vez de sumir: `None` é uma "versão" como as do catálogo, e é
/// justamente a que o usuário não reconhece como "versão aberta".
fn version_display_name(key: &Option<String>) -> String {
    key.clone().unwrap_or_else(|| "system install".to_string())
}

/// Nomes das versões, em ordem estável, para a mensagem da guarda.
fn running_version_names(running_keys: &HashSet<Option<String>>) -> Vec<String> {
    let mut names: Vec<String> = running_keys.iter().map(version_display_name).collect();
    names.sort();
    names
}

/// A frase da recusa por conflito de versão — a mesma no launch de uma conta e
/// no da fila.
///
/// Ela **tem** que dizer o que fechar. A guarda compara com uma versão alvo,
/// então basta o tracker ter duas chaves distintas (o Auto Rejoin não checa
/// conflito, por desenho — `docs/features/botting.md`) para nenhum alvo
/// satisfazê-la: aí "feche o cliente" sem dizer *qual* deixa o usuário sem ação
/// possível.
///
/// E ela lista **só as versões que impedem** — as diferentes do alvo. A frase
/// antiga listava todas as abertas e dizia "Close these clients": com
/// `{None, Some(X)}` rodando e alvo `Some(X)` (o caso que motivou a lista),
/// bastava fechar os da instalação do sistema, mas o dono fechava também os de
/// X — clientes de outras contas, inclusive a principal. Quando há cliente na
/// versão certa, a frase diz que ele pode ficar.
///
/// Nada aqui sai como código interno: esta frase é desenhada crua na tela,
/// tanto no toast do launch único quanto na linha da fila.
fn version_conflict_message(
    running_keys: &HashSet<Option<String>>,
    target_version_id: &Option<String>,
    allow_launch_on_open_version: bool,
) -> String {
    let blocking: HashSet<Option<String>> = running_keys
        .iter()
        .filter(|key| *key != target_version_id)
        .cloned()
        .collect();
    let target = version_display_name(target_version_id);
    let keep = if running_keys.contains(target_version_id) {
        format!(" Clients already on {target} can stay open.")
    } else {
        String::new()
    };
    // O toggle só resolveria se o alvo já estiver aberto; aí a frase o aponta.
    let toggle = if !allow_launch_on_open_version && running_keys.contains(target_version_id) {
        " Or turn on \"Allow launching on an already open version\" in Settings > Versions."
    } else {
        ""
    };
    format!(
        "A Roblox client is already running on a different Roblox version. This account launches on {target}; close the clients on {} before launching it.{keep}{toggle} Concurrent multi-version support is planned for a future update.",
        running_version_names(&blocking).join(", ")
    )
}

/// How long to wait for the new client's PID. A Full-isolation launch has to
/// download Roblox again first, which is far slower than a normal start.
fn pid_wait_seconds(isolation_wipes_install: bool) -> u64 {
    if isolation_wipes_install {
        180
    } else {
        12
    }
}

/// `AccountJoinDelay` as seconds, with no floor applied yet.
///
/// A negative value (a hand-edited INI, or a UI that wrote one) used to wrap
/// around in the `i64 -> u64` cast and become `u64::MAX`, freezing the
/// multi-launch queue between two accounts. An invalid value falls back to the
/// default, exactly like a missing key.
fn configured_join_delay_seconds(configured: Option<i64>) -> u64 {
    configured.filter(|value| *value >= 0).unwrap_or(8) as u64
}

/// Seconds to space multi-account launches by, never below the captcha floor.
fn effective_join_delay_seconds(configured: Option<i64>) -> u64 {
    configured_join_delay_seconds(configured).max(MIN_JOIN_GAP_SECS)
}

/// A small randomized tail on the inter-account gap, so launches are neither
/// back-to-back nor perfectly periodic.
fn launch_jitter_ms(subsec_millis: u32) -> u64 {
    300 + (subsec_millis as u64) % 1200
}

/// Gap before the next account: the configured delay measured from the *start*
/// of this account's launch, minus the time already spent, but never below
/// `MIN_RESIDUAL_GAP_MS`, plus jitter.
fn next_account_wait(
    delay_seconds: u64,
    elapsed: std::time::Duration,
    jitter_ms: u64,
) -> std::time::Duration {
    std::time::Duration::from_secs(delay_seconds)
        .saturating_sub(elapsed)
        .max(std::time::Duration::from_millis(MIN_RESIDUAL_GAP_MS))
        + std::time::Duration::from_millis(jitter_ms)
}

/// Teto da espera pelo jogo (`General.WaitForGameJoin`), contado do início do
/// launch da conta: passado isto, a fila segue mesmo sem a conta ter entrado.
/// Um `AccountJoinDelay` maior que isto vira o teto.
const JOIN_WAIT_CAP_SECS: u64 = 20;

/// O que o log do cliente (commands/client_health.rs) diz da conta que acabou
/// de abrir, para a fila decidir se já pode seguir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JoinSignal {
    /// O log do cliente não foi achado (ou ele caiu): vale a espera fixa de sempre.
    NoLog,
    /// O log foi achado e a conta ainda não entrou no jogo.
    Loading,
    /// O log diz que a conta entrou no jogo.
    InGame,
}

/// Os três prazos da espera pelo jogo, todos contados como a espera fixa (do
/// início do launch da conta, com o mesmo resíduo mínimo e o mesmo jitter):
/// `floor` é o piso anti-captcha (nunca segue antes), `fixed` é a espera de
/// sempre (vale sem log) e `cap` é o teto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct JoinWaitPlan {
    floor: std::time::Duration,
    fixed: std::time::Duration,
    cap: std::time::Duration,
}

fn join_wait_plan(delay_seconds: u64, elapsed: std::time::Duration, jitter_ms: u64) -> JoinWaitPlan {
    JoinWaitPlan {
        floor: next_account_wait(MIN_JOIN_GAP_SECS, elapsed, jitter_ms),
        fixed: next_account_wait(delay_seconds, elapsed, jitter_ms),
        cap: next_account_wait(JOIN_WAIT_CAP_SECS.max(delay_seconds), elapsed, jitter_ms),
    }
}

/// A fila já pode seguir para a próxima conta? Nunca antes do piso
/// anti-captcha; com o log achado, assim que a conta entrar no jogo (ou no
/// teto); sem log, na espera fixa de sempre.
fn join_wait_done(waited: std::time::Duration, plan: JoinWaitPlan, signal: JoinSignal) -> bool {
    if waited < plan.floor {
        return false;
    }
    match signal {
        JoinSignal::InGame => true,
        JoinSignal::NoLog => waited >= plan.fixed,
        JoinSignal::Loading => waited >= plan.cap,
    }
}

/// O sinal do log para o cliente `pid` da conta. Queda também vira `NoLog`:
/// ela não vai entrar, então vale a espera fixa.
fn join_signal_from(view: Option<&ClientHealthView>) -> JoinSignal {
    match view {
        Some(view) if view.log_found && view.in_game => JoinSignal::InGame,
        Some(view) if view.log_found && view.drop.is_none() && !view.exited => JoinSignal::Loading,
        _ => JoinSignal::NoLog,
    }
}

/// Tamanho da fatia da espera entre contas. A espera inteira num `sleep` só não
/// dá chance de olhar a fila: enquanto ela dorme, o painel mostra "0 na fila" e
/// o app recusa qualquer launch novo. Curta o suficiente para o usuário não
/// sentir, longa o suficiente para não virar espera ocupada.
const WAIT_SLICE: std::time::Duration = std::time::Duration::from_millis(250);

/// A próxima fatia a dormir: nunca passa do que falta, senão o gap
/// anti-captcha cresceria além do calculado.
fn next_wait_slice(remaining: std::time::Duration) -> std::time::Duration {
    remaining.min(WAIT_SLICE)
}

/// Continua esperando a próxima conta? Só enquanto sobra tempo do intervalo
/// anti-captcha, ninguém cancelou e ainda há conta esperando a vez. As três
/// condições existem por um motivo cada: sem a terceira o app fica preso depois
/// de o usuário parar a fila; sem a segunda, o "Close All Roblox" não encurta a
/// espera; sem a primeira, não é espera, é laço infinito.
fn keep_waiting_for_next_account(
    remaining: std::time::Duration,
    cancelled: bool,
    queued: usize,
) -> bool {
    !remaining.is_zero() && !cancelled && queued > 0
}

/// Dorme `wait` em fatias e volta assim que não houver mais conta esperando a vez
/// nesta sequência (fila parada pelo usuário, ou outro lote assumiu) ou o
/// cancelamento global chegar.
///
/// A reserva continua sendo solta só pelo `Drop` do dono — isto não libera nada,
/// só encurta a janela em que a tela diz "acabou" e o app diz "ainda estou
/// lançando".
async fn wait_before_next_account(
    sequence: &LaunchSequenceGuard,
    wait: std::time::Duration,
    cancelled: impl Fn() -> bool,
) {
    let deadline = std::time::Instant::now() + wait;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if !keep_waiting_for_next_account(remaining, cancelled(), sequence.queued_count()) {
            return;
        }
        tokio::time::sleep(next_wait_slice(remaining)).await;
    }
}

/// Espera a conta que acabou de abrir entrar no jogo (`General.WaitForGameJoin`):
/// segue assim que o log disser que ela entrou, sem passar do piso
/// anti-captcha nem do teto. Sai também quando a fila para ou é cancelada,
/// como `wait_before_next_account`. Devolve se a conta entrou no jogo.
async fn wait_for_game_join(
    sequence: &LaunchSequenceGuard,
    plan: JoinWaitPlan,
    signal: impl Fn() -> JoinSignal,
    cancelled: impl Fn() -> bool,
) -> bool {
    let started = std::time::Instant::now();
    loop {
        let waited = started.elapsed();
        let now = signal();
        if join_wait_done(waited, plan, now) {
            return now == JoinSignal::InGame;
        }
        let remaining = plan.cap.saturating_sub(waited);
        if !keep_waiting_for_next_account(remaining, cancelled(), sequence.queued_count()) {
            return false;
        }
        tokio::time::sleep(next_wait_slice(remaining)).await;
    }
}

/// Picks a pseudo-random public server from the list (no RNG dependency).
fn shuffle_server_index(nanos: u128, server_count: usize) -> usize {
    (nanos as usize) % server_count
}

/// Did the frontend ask for `shuffleJob`? The argument is optional so older
/// callers (and any `invoke` that omits the field) keep working — a missing
/// value means "don't shuffle".
fn shuffle_job_requested(shuffle_job: Option<bool>) -> bool {
    shuffle_job.unwrap_or(false)
}

/// Shuffling only makes sense when the user did not pick a server: an explicit
/// Job ID wins, and "follow user" resolves the server on its own.
fn should_shuffle_server(shuffle_job: bool, follow_user: bool, job_id: &str) -> bool {
    shuffle_job && !follow_user && job_id.trim().is_empty()
}

/// The saved window rectangle of an account, or `None` when any part is
/// missing or unparsable (a half-applied rectangle would misplace the window).
fn window_rect_from_fields(
    fields: &std::collections::HashMap<String, String>,
) -> Option<(i32, i32, i32, i32)> {
    let x = fields.get("Window_Position_X")?.parse::<i32>().ok()?;
    let y = fields.get("Window_Position_Y")?.parse::<i32>().ok()?;
    let w = fields.get("Window_Width")?.parse::<i32>().ok()?;
    let h = fields.get("Window_Height")?.parse::<i32>().ok()?;
    Some((x, y, w, h))
}

// ---------------------------------------------------------------------------
// Fila de launch observável
// ---------------------------------------------------------------------------
//
// O launch múltiplo sempre foi uma caixa preta: a UI só sabia "conta 3 de 16".
// Para pular uma conta ou parar o resto da fila, o usuário precisava do
// "Close All Roblox" — que mata também os clientes que já entraram.
//
// A fila abaixo publica o estado de cada conta do lote (evento `launch-queue`)
// e aceita dois cancelamentos que **nunca** fecham cliente nenhum:
//
// * `cancel_account_launch` — pula UMA conta que ainda não foi lançada;
// * `stop_launch_queue`     — pula todas as que ainda estão na fila.
//
// Uma conta em andamento (`launching`) não é abortada — não dá para desfazer o
// auth ticket no meio — e uma que já entrou (`done`) continua aberta. Isso é
// deliberadamente diferente de `cancel_launch` / `cmd_kill_all_roblox`, que
// seguem matando tudo.
//
// A lógica é pura (`LaunchQueue`, sem `AppHandle`) e a emissão do evento fica
// nos wrappers `launch_queue_*`, para os testes não precisarem de um app Tauri.

/// Estado de uma conta dentro do lote atual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
enum LaunchQueueState {
    Queued,
    Launching,
    Done,
    Failed,
    Cancelled,
}

impl LaunchQueueState {
    /// Estados finais: uma vez neles, a conta não volta atrás. É a guarda
    /// contra transição fora de ordem (ex.: o `failed` do wrapper do launch
    /// único chegando depois do `done` que o corpo já marcou).
    fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Cancelled)
    }
}

/// Uma conta da fila, do jeito que o frontend lê (`LaunchQueueEntry`).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchQueueEntry {
    user_id: i64,
    state: LaunchQueueState,
    error: Option<String>,
    updated_at_ms: u64,
}

/// Snapshot completo da fila (`LaunchQueuePayload`): é o retorno de
/// `get_launch_queue` e o payload do evento `launch-queue`.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchQueuePayload {
    entries: Vec<LaunchQueueEntry>,
    active: bool,
    place_id: i64,
    job_id: String,
}

/// Estado puro da fila. Sem I/O, sem Tauri — tudo aqui é testável direto.
#[derive(Debug, Default)]
struct LaunchQueue {
    entries: Vec<LaunchQueueEntry>,
    active: bool,
    place_id: i64,
    job_id: String,
    /// Número da sequência atual. Cresce a cada reserva aceita e é o "dono" da
    /// fila: todo helper chamado de dentro do launch passa a sua geração, e
    /// qualquer ação de um lote que já não é o atual é ignorada.
    ///
    /// Sem isto, o laço de um lote antigo — que pode estar dormindo o intervalo
    /// anti-captcha ou o perfil pós-launch — apagava a fila do lote novo ao
    /// sair, e ainda lançava as próprias contas restantes em paralelo com ele.
    generation: u64,
    /// Reserva tomada. Só o dono a solta (no `Drop` do guard), nunca o estado
    /// das entradas: uma fila momentaneamente toda terminal **não** significa
    /// que o laço acabou.
    reserved: bool,
}

impl LaunchQueue {
    /// A fila ainda é a deste lote? É a pergunta que separa "o dono agindo" de
    /// "um lote que já foi substituído tentando agir".
    fn is_owner(&self, generation: u64) -> bool {
        self.reserved && self.generation == generation
    }

    /// Reserva a fila para este lote e devolve a geração dele; devolve `None` —
    /// sem tocar em nada — quando já há sequência reservada.
    ///
    /// A reserva cobre as contas **todas** aqui, antes de qualquer launch:
    /// reservar conta a conta dentro do laço deixaria uma segunda sequência
    /// entrar no meio da primeira, e as duas disputariam o mutex do Multi
    /// Roblox, o registro e o `ClientAppSettings.json` (que é global por pasta
    /// de versão).
    ///
    /// Lote vazio não é sequência e não reserva nada: travar a fila por um lote
    /// que não vai abrir cliente nenhum seria travar o app por nada.
    fn try_start(
        &mut self,
        user_ids: &[i64],
        place_id: i64,
        job_id: &str,
        now_ms: u64,
    ) -> Option<u64> {
        if self.reserved || user_ids.is_empty() {
            return None;
        }
        self.generation = self.generation.wrapping_add(1);
        self.reserved = true;
        self.start(user_ids, place_id, job_id, now_ms);
        Some(self.generation)
    }

    /// Solta a reserva: fecha toda conta que não chegou a estado final
    /// (`queued` nunca foi tentada, `launching` acabou sem resposta) e desativa
    /// a fila. Devolve se algo mudou, para o chamador só emitir evento quando há
    /// novidade.
    ///
    /// É o ponto único de saída da sequência — chamado pelo `Drop` da reserva —,
    /// então vale para o lote que terminou, para o que falhou no meio, para o
    /// que o usuário cancelou e para o erro que abortou tudo. **Só o dono
    /// libera:** um lote antigo saindo depois que outro assumiu não pode apagar
    /// a fila de quem entrou.
    fn release(&mut self, generation: u64, now_ms: u64) -> bool {
        if !self.is_owner(generation) {
            return false;
        }
        self.reserved = false;
        for entry in self.entries.iter_mut() {
            if entry.state.is_terminal() {
                continue;
            }
            entry.state = if entry.state == LaunchQueueState::Launching {
                // A conta estava no meio do trabalho e o comando voltou sem
                // dizer no que deu: não dá para afirmar que entrou.
                entry.error = Some("Launch interrompido antes de terminar".to_string());
                LaunchQueueState::Failed
            } else {
                LaunchQueueState::Cancelled
            };
            entry.updated_at_ms = now_ms;
        }
        self.active = false;
        // A reserva mudou de mão: o snapshot precisa sair de qualquer jeito.
        true
    }

    /// Começa um lote novo. A fila é **por execução**: o lote anterior é
    /// descartado inteiro, porque a UI mostra sempre o lote atual. Só é chamada
    /// por `try_start`, que é quem garante que o lote anterior já soltou a
    /// reserva.
    fn start(&mut self, user_ids: &[i64], place_id: i64, job_id: &str, now_ms: u64) {
        self.entries = user_ids
            .iter()
            .map(|&user_id| LaunchQueueEntry {
                user_id,
                state: LaunchQueueState::Queued,
                error: None,
                updated_at_ms: now_ms,
            })
            .collect();
        self.active = true;
        self.place_id = place_id;
        self.job_id = job_id.to_string();
    }

    fn state_of(&self, user_id: i64) -> Option<LaunchQueueState> {
        self.entries
            .iter()
            .find(|entry| entry.user_id == user_id)
            .map(|entry| entry.state)
    }

    /// Transição guardada. Devolve `false` — sem tocar em nada — para uma conta
    /// fora da fila, para a transição ao mesmo estado, para qualquer volta a
    /// `queued` e para qualquer tentativa de sair de um estado final.
    fn set_state(
        &mut self,
        user_id: i64,
        state: LaunchQueueState,
        error: Option<String>,
        now_ms: u64,
    ) -> bool {
        if state == LaunchQueueState::Queued {
            return false;
        }
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.user_id == user_id) else {
            return false;
        };
        if entry.state.is_terminal() || entry.state == state {
            return false;
        }
        entry.state = state;
        entry.error = error;
        entry.updated_at_ms = now_ms;
        true
    }

    /// Cancelamento pedido pelo usuário para UMA conta. Só vale para quem ainda
    /// não foi lançada: `launching` não dá para abortar no meio do auth e
    /// `done` já tem cliente aberto — cancelar **nunca** fecha cliente.
    fn request_cancel(&mut self, user_id: i64, now_ms: u64) -> bool {
        if self.state_of(user_id) != Some(LaunchQueueState::Queued) {
            return false;
        }
        self.set_state(user_id, LaunchQueueState::Cancelled, None, now_ms)
    }

    /// Cancela todas as contas que ainda estão `queued` e devolve quantas
    /// foram. Não toca na conta em andamento nem nas que já entraram.
    fn cancel_queued(&mut self, now_ms: u64) -> usize {
        let mut cancelled = 0;
        for entry in self.entries.iter_mut() {
            if entry.state == LaunchQueueState::Queued {
                entry.state = LaunchQueueState::Cancelled;
                entry.error = None;
                entry.updated_at_ms = now_ms;
                cancelled += 1;
            }
        }
        cancelled
    }

    /// Transição pedida pelo **dono** da sequência. É a mesma de `set_state`,
    /// mas recusada quando a fila já é de outro lote: sem isto, o laço de um
    /// lote antigo reescrevia o estado de uma conta que o lote novo também tem.
    fn set_state_owned(
        &mut self,
        generation: u64,
        user_id: i64,
        state: LaunchQueueState,
        error: Option<String>,
        now_ms: u64,
    ) -> bool {
        if !self.is_owner(generation) {
            return false;
        }
        self.set_state(user_id, state, error, now_ms)
    }

    /// `cancel_queued` do dono da sequência (fim de lote por erro fatal ou por
    /// "Close All Roblox" visto de dentro do laço). Um lote antigo não cancela
    /// as contas de quem entrou depois dele.
    fn cancel_queued_owned(&mut self, generation: u64, now_ms: u64) -> usize {
        if !self.is_owner(generation) {
            return 0;
        }
        self.cancel_queued(now_ms)
    }

    /// Quantas contas **desta** sequência ainda estão esperando a vez. Zero
    /// significa que não há mais nada para lançar — porque o lote acabou, porque
    /// o usuário parou a fila, ou porque outro lote assumiu.
    ///
    /// É o que o laço olha antes e durante a espera entre contas: dormir o
    /// intervalo anti-captcha sem mais ninguém na fila deixava o Painel de Sessão
    /// dizendo "0 na fila", com o Stop desabilitado, enquanto todo launch novo
    /// era recusado por até `AccountJoinDelay` segundos.
    fn queued_count_for(&self, generation: u64) -> usize {
        if !self.is_owner(generation) {
            return 0;
        }
        self.entries
            .iter()
            .filter(|entry| entry.state == LaunchQueueState::Queued)
            .count()
    }

    /// O laço pergunta isto antes de trabalhar numa conta. Devolve `true` para
    /// quem foi cancelado **e** para o caso de a fila não ser mais deste lote:
    /// se outra sequência assumiu, este laço não pode abrir mais cliente nenhum.
    /// O desenho antigo devolvia `false` aqui ("a entrada sumiu, segue o jogo") e
    /// era exatamente por onde duas sequências acabavam lançando juntas.
    fn is_cancelled_for(&self, generation: u64, user_id: i64) -> bool {
        if !self.is_owner(generation) {
            return true;
        }
        self.state_of(user_id) == Some(LaunchQueueState::Cancelled)
    }

    /// Fim do lote: a UI para de mostrar a fila como ativa, mas as entradas
    /// continuam lá para o usuário ver o resultado conta a conta. A reserva
    /// **não** cai aqui — quem a solta é o `Drop` do guard —, e um lote que já
    /// não é o dono não desativa a fila de quem é.
    fn finish(&mut self, generation: u64) -> bool {
        if !self.is_owner(generation) || !self.active {
            return false;
        }
        self.active = false;
        true
    }

    fn snapshot(&self) -> LaunchQueuePayload {
        LaunchQueuePayload {
            entries: self.entries.clone(),
            active: self.active,
            place_id: self.place_id,
            job_id: self.job_id.clone(),
        }
    }
}

static LAUNCH_QUEUE: std::sync::LazyLock<std::sync::Mutex<LaunchQueue>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(LaunchQueue::default()));

fn launch_queue_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Roda `f` com o lock da fila e devolve o resultado junto com o snapshot já
/// pronto. O lock é solto **antes** de qualquer emissão e nunca atravessa um
/// `.await`: todos os chamadores são síncronos e só emitem depois daqui.
fn with_launch_queue<R>(f: impl FnOnce(&mut LaunchQueue) -> R) -> (R, LaunchQueuePayload) {
    let mut queue = match LAUNCH_QUEUE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let result = f(&mut queue);
    let snapshot = queue.snapshot();
    drop(queue);
    (result, snapshot)
}

fn emit_launch_queue(app: &tauri::AppHandle, payload: &LaunchQueuePayload) {
    let _ = app.emit("launch-queue", payload);
}

/// Código devolvido quando o usuário dispara um launch com outro em andamento.
/// É um código, não uma frase: quem traduz é o frontend
/// (`isLaunchAlreadyActiveError` em `src/utils/robloxErrors.ts`).
const LAUNCH_ALREADY_ACTIVE: &str = "launch-already-active";

/// A reserva da sequência de launch. Enquanto ela existe, nenhuma outra
/// sequência entra; quando sai de escopo, a fila é liberada.
///
/// A liberação é no `Drop` porque os caminhos de saída do launch são muitos —
/// erro por conta, erro que aborta o lote, `?` no meio, cancelamento pelo
/// usuário, `panic` — e uma reserva esquecida em qualquer um deles travaria o
/// app até reiniciar.
struct LaunchSequenceGuard {
    app: tauri::AppHandle,
    /// A geração que este guard reservou. Toda escrita na fila passa por ela, e
    /// é isso que impede um lote antigo de mexer na fila de um lote novo.
    generation: u64,
}

impl Drop for LaunchSequenceGuard {
    fn drop(&mut self) {
        let (changed, payload) =
            with_launch_queue(|queue| queue.release(self.generation, launch_queue_now_ms()));
        if changed {
            emit_launch_queue(&self.app, &payload);
        }
    }
}

impl LaunchSequenceGuard {
    /// Aplica uma transição desta sequência e publica o evento. Devolve `false`
    /// quando a transição foi recusada (conta fora da fila, conta já num estado
    /// final, ou a fila já é de outro lote).
    fn mark(&self, user_id: i64, state: LaunchQueueState, error: Option<String>) -> bool {
        let (changed, payload) = with_launch_queue(|queue| {
            queue.set_state_owned(self.generation, user_id, state, error, launch_queue_now_ms())
        });
        if changed {
            emit_launch_queue(&self.app, &payload);
        }
        // `Done` é a definição do próprio app para "o cliente subiu": é aqui que
        // `last_use` tem de andar. Antes ele só era escrito ao criar ou re-adicionar a
        // conta, então a coluna "3d"/"2mo" e a bolinha de envelhecimento mediam idade
        // do cadastro, não inatividade de jogo. Marcar num ponto só cobre conta única,
        // lote e as duas plataformas. Falhar ao gravar não derruba um launch que deu
        // certo — por isso o erro é ignorado de propósito.
        if changed && launch_state_means_used(state) {
            let _ = self.app.state::<AccountStore>().mark_used(user_id);
        }
        changed
    }

    /// O laço pergunta isto antes de trabalhar numa conta: `true` para quem foi
    /// cancelado e para o caso de outra sequência ter assumido a fila.
    fn is_cancelled(&self, user_id: i64) -> bool {
        with_launch_queue(|queue| queue.is_cancelled_for(self.generation, user_id)).0
    }

    /// Cancela tudo que ainda está `queued` **deste** lote e devolve a contagem.
    fn cancel_remaining(&self) -> usize {
        let (cancelled, payload) = with_launch_queue(|queue| {
            queue.cancel_queued_owned(self.generation, launch_queue_now_ms())
        });
        if cancelled > 0 {
            emit_launch_queue(&self.app, &payload);
        }
        cancelled
    }

    /// Quantas contas desta sequência ainda esperam a vez.
    fn queued_count(&self) -> usize {
        with_launch_queue(|queue| queue.queued_count_for(self.generation)).0
    }

    /// Fim do lote: a fila deixa de aparecer como ativa. A reserva continua
    /// deste guard até ele sair de escopo.
    fn finish(&self) {
        let (changed, payload) = with_launch_queue(|queue| queue.finish(self.generation));
        if changed {
            emit_launch_queue(&self.app, &payload);
        }
    }

    /// Aborta o lote inteiro por um erro fatal: a conta em andamento (quando há
    /// uma) vira `failed` com a mensagem e quem nunca chegou a ser tentado vira
    /// `cancelled`.
    fn abort(&self, user_id: Option<i64>, error: &str) {
        if let Some(user_id) = user_id {
            self.mark(user_id, LaunchQueueState::Failed, Some(error.to_string()));
        }
        self.cancel_remaining();
        self.finish();
    }
}

/// Reserva a sequência, substitui a fila pelo lote que está começando e publica
/// o estado inicial. Recusa (sem tocar na fila) quando já há um launch em
/// andamento: dois lotes ao mesmo tempo disputariam o mutex do Multi Roblox, o
/// registro e o patch de client settings, e a UI mostraria só o segundo.
///
/// O valor devolvido **tem que ser amarrado a uma variável** que viva até o fim
/// do launch: é ele que segura a reserva.
#[must_use = "a reserva da sequência é liberada quando este valor sai de escopo"]
fn launch_queue_start(
    app: &tauri::AppHandle,
    user_ids: &[i64],
    place_id: i64,
    job_id: &str,
) -> Result<LaunchSequenceGuard, String> {
    let (generation, payload) = with_launch_queue(|queue| {
        queue.try_start(user_ids, place_id, job_id, launch_queue_now_ms())
    });
    let Some(generation) = generation else {
        return Err(LAUNCH_ALREADY_ACTIVE.to_string());
    };
    emit_launch_queue(app, &payload);
    Ok(LaunchSequenceGuard {
        app: app.clone(),
        generation,
    })
}

/// Quais estados da fila contam como "a conta foi usada". Só `Done` — é a
/// definição do próprio app para "o cliente subiu". Fica separado para poder ser
/// testado sem `AppHandle`: o risco real aqui é alguém passar a marcar em
/// `Launching`, e aí `last_use` voltaria a medir tentativa em vez de uso.
fn launch_state_means_used(state: LaunchQueueState) -> bool {
    matches!(state, LaunchQueueState::Done)
}

/// Cancela tudo que ainda está `queued` na fila atual, seja ela de quem for.
/// É o caminho da UI ("parar a fila", "Close All Roblox"), que age sobre o que
/// está na tela — e por isso **não** passa pela geração de ninguém.
fn launch_queue_cancel_all_queued(app: &tauri::AppHandle) -> usize {
    let (cancelled, payload) = with_launch_queue(|queue| queue.cancel_queued(launch_queue_now_ms()));
    if cancelled > 0 {
        emit_launch_queue(app, &payload);
    }
    cancelled
}

/// Estado atual da fila, para a UI montar a lista ao abrir.
#[tauri::command]
fn get_launch_queue() -> LaunchQueuePayload {
    with_launch_queue(|_| ()).1
}

/// Cancela UMA conta que ainda não foi lançada; o loop a pula quando chegar
/// nela. Nunca fecha cliente: uma conta em andamento (`launching`) ou que já
/// entrou (`done`) devolve `false` e nada muda.
#[tauri::command]
fn cancel_account_launch(app: tauri::AppHandle, user_id: i64) -> bool {
    let (cancelled, payload) =
        with_launch_queue(|queue| queue.request_cancel(user_id, launch_queue_now_ms()));
    if cancelled {
        emit_launch_queue(&app, &payload);
    }
    cancelled
}

/// Para o resto da fila: cancela só quem ainda está `queued` e devolve quantas
/// contas foram canceladas. A conta em andamento termina e quem já entrou
/// continua aberto — ao contrário do "Close All Roblox".
#[tauri::command]
fn stop_launch_queue(app: tauri::AppHandle) -> usize {
    launch_queue_cancel_all_queued(&app)
}

#[cfg(target_os = "windows")]
#[tauri::command]
async fn launch_roblox(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    settings: tauri::State<'_, SettingsStore>,
    versions: tauri::State<'_, data::versions::VersionsCatalogStore>,
    user_id: i64,
    place_id: i64,
    job_id: String,
    launch_data: String,
    follow_user: bool,
    join_vip: bool,
    link_code: String,
    // Opcional para aceitar chamadas que não mandam o campo (Tauri trata a
    // chave ausente como `None`); `None` = não sortear.
    shuffle_job: Option<bool>,
) -> Result<(), String> {
    // O launch de uma conta única também alimenta a fila (com uma entrada só),
    // para a UI mostrar e cancelar do mesmo jeito que num lote — e por isso ele
    // reserva a sequência igual a um lote: um launch avulso disparado durante
    // uma fila brigaria com ela pelo mutex e pelo client settings.
    let sequence = launch_queue_start(&app, &[user_id], place_id, &job_id)?;
    let result = launch_roblox_windows(
        app.clone(),
        &sequence,
        state,
        settings,
        versions,
        user_id,
        place_id,
        job_id,
        launch_data,
        follow_user,
        join_vip,
        link_code,
        shuffle_job,
    )
    .await;
    if let Err(err) = &result {
        // `set_state` ignora quem já está num estado final, então isto só pega
        // os erros que escaparam do corpo sem marcar nada.
        sequence.mark(user_id, LaunchQueueState::Failed, Some(err.clone()));
    }
    sequence.finish();
    result
}

#[cfg(target_os = "windows")]
#[allow(clippy::too_many_arguments)]
async fn launch_roblox_windows(
    app: tauri::AppHandle,
    // A reserva da sequência é do chamador; o corpo recebe o guard porque toda
    // escrita na fila tem de ser assinada por ela (ver `LaunchSequenceGuard`).
    sequence: &LaunchSequenceGuard,
    state: tauri::State<'_, AccountStore>,
    settings: tauri::State<'_, SettingsStore>,
    versions: tauri::State<'_, data::versions::VersionsCatalogStore>,
    user_id: i64,
    place_id: i64,
    job_id: String,
    launch_data: String,
    follow_user: bool,
    join_vip: bool,
    link_code: String,
    shuffle_job: Option<bool>,
) -> Result<(), String> {
    use platform::windows;

    sequence.mark(user_id, LaunchQueueState::Launching, None);

    let target_desc = launch_target_description(join_vip, &link_code, &job_id);
    emit_launch_log(
        &app,
        user_id,
        "info",
        "start",
        format!("Iniciando launch — place {place_id} ({target_desc})"),
    );

    // Conta banida/encerrada é pulada aqui, com a frase certa, em vez de falhar
    // no auth ticket depois do isolamento (ver commands/moderation.rs).
    if let Some(reason) = moderation_launch_block(state.inner(), &app, &settings, user_id).await {
        emit_launch_log(&app, user_id, "warn", "moderated", reason.clone());
        return Err(reason);
    }

    let is_teleport = settings.get_bool("Developer", "IsTeleport");
    let configured_old_join = settings.get_bool("Developer", "UseOldJoin");
    let auto_close_last_process = settings.get_bool("General", "AutoCloseLastProcess");
    let auto_close_multi_conflicts = settings.get_bool("General", "AutoCloseRobloxForMultiRbx");
    let account_snapshot_for_version = state.get_all()?;
    let account_version_override = account_snapshot_for_version
        .iter()
        .find(|a| a.user_id == user_id)
        .and_then(|a| a.fields.get("RobloxVersion").cloned())
        .filter(|v| !v.trim().is_empty());
    // Exceções desta conta (FPS, volume, qualidade, tela cheia, minimizar).
    // Lidas antes do `start_minimized` porque podem trocá-lo.
    let account_overrides = account_snapshot_for_version
        .iter()
        .find(|a| a.user_id == user_id)
        .and_then(|a| account_client_overrides(&a.fields));

    let start_minimized = account_overrides
        .as_ref()
        .and_then(|o| o.start_minimized)
        .unwrap_or_else(|| settings.get_bool("General", "StartRobloxMinimized"));

    let (resolved_base_path, resolved_version_id) =
        windows::resolve_roblox_install_path(account_version_override.as_deref(), &settings, &versions)?;
    let isolation_will_wipe_install = isolation_wipes_install(
        &settings.get_string("Isolation", "Mode"),
        resolved_version_id.as_deref(),
    );
    let use_old_join = resolve_use_old_join(
        isolation_will_wipe_install,
        configured_old_join,
        resolved_version_id.as_deref(),
    );

    if let Some(report) = run_pre_launch_isolation(&app, &settings).await? {
        let _ = app.emit("isolation-report", &report);
        emit_launch_log(&app, user_id, "info", "isolation", "Isolamento pré-launch aplicado");
        if windows::has_pending_fast_flags() {
            tokio::spawn(apply_pending_fast_flags_when_ready(
                std::time::Duration::from_secs(240),
            ));
        }
    }

    let tracker_check = windows::tracker();
    let _ = tracker_check.cleanup_dead_processes();
    let running_keys = tracker_check.running_version_keys();
    let allow_launch_on_open_version = settings.get_bool("Versions", "AllowLaunchOnOpenVersion");
    if version_guard_blocks(&running_keys, &resolved_version_id, allow_launch_on_open_version) {
        return Err(version_conflict_message(
            &running_keys,
            &resolved_version_id,
            allow_launch_on_open_version,
        ));
    }

    let multi_rbx = settings.get_bool("General", "EnableMultiRbx");
    if multi_rbx {
        ensure_multi_roblox_enabled(auto_close_multi_conflicts).await?;
    } else {
        let _ = windows::disable_multi_roblox();
    }

    windows::refresh_production_version().await;
    // A pasta de onde o cliente vai abrir, resolvida uma vez: o patch (FPS,
    // fast flags) e o spawn do old join usam esta mesma — antes o patch ia
    // para `resolved_base_path` e o old join sem versão do catálogo abria a
    // build do canal do registro.
    let client_dir = windows::client_dir(
        windows::client_source(use_old_join, resolved_version_id.is_some()),
        &resolved_base_path,
    )
    .await;

    let tracker = windows::tracker();
    if auto_close_last_process && tracker.get_pid(user_id).is_some() {
        let closed = tracker.kill_for_user_graceful_async(user_id, 4500).await;
        if !closed {
            return Err("Previous Roblox instance did not close before relaunch".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }

    let mut resolved_launch = resolve_launch_job(&job_id, join_vip, &link_code);
    if follow_user {
        resolved_launch.join_vip = false;
        resolved_launch.link_code.clear();
    }

    let mut actual_job = resolved_launch.job_id.clone();
    if should_shuffle_server(shuffle_job_requested(shuffle_job), follow_user, &actual_job) {
        if let Some(job) = pick_shuffled_public_job(state.inner(), user_id, place_id).await {
            actual_job = job;
        }
    }

    let browser_tracker_id = get_or_create_browser_tracker_id(&state, user_id)?;
    emit_launch_log(&app, user_id, "info", "auth", "Solicitando authentication ticket...");
    let ticket = match run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::auth::get_auth_ticket(&cookie).await
    })
    .await
    {
        Ok(t) => {
            emit_launch_log(&app, user_id, "success", "auth", "Authentication ticket obtido");
            t
        }
        Err(e) => {
            emit_launch_log(&app, user_id, "error", "auth", format!("Falha no auth ticket: {e}"));
            if is_moderated_error(&e) {
                mark_account_moderated(state.inner(), &app, user_id);
                emit_launch_log(&app, user_id, "warn", "moderated", "Conta movida para o grupo 'moderadas'");
            }
            return Err(e);
        }
    };
    let private_join = run_with_session_retry(state.inner(), user_id, |cookie| {
        let resolved_launch = resolved_launch.clone();
        async move { resolve_private_join(&cookie, place_id, &resolved_launch).await }
    })
    .await?;
    if private_join.use_private_join {
        emit_launch_log(&app, user_id, "info", "target", "Alvo resolvido: servidor privado/VIP");
    } else if !actual_job.trim().is_empty() {
        emit_launch_log(&app, user_id, "info", "target", format!("Alvo resolvido: {}", actual_job.trim()));
    }

    // O XML é compartilhado: o patch é o último passo antes do spawn, para não
    // dar a um cliente já aberto o intervalo do fechamento e da rede para
    // reescrevê-lo. Mesmo assim ele pode ser reescrito até o cliente novo o
    // ler — quem garante o tamanho é `spawn_client_window_enforcement`.
    let resolved_window = patch_client_settings_for_launch(
        &settings,
        LaunchClientProfile::Normal,
        account_overrides.as_ref(),
        Some(&client_dir),
    );

    let pids_before = windows::get_roblox_pids();

    let pid_wait_secs = pid_wait_seconds(isolation_will_wipe_install);
    let pending_id = tracker.add_pending_launch(
        user_id,
        resolved_version_id.clone(),
        std::time::Duration::from_secs(pid_wait_secs + 30),
    );

    let spawn_result = if use_old_join {
        windows::launch_old_join_from(
            &client_dir,
            &ticket,
            private_join.place_id,
            &actual_job,
            &launch_data,
            follow_user,
            private_join.use_private_join,
            &private_join.access_code,
            &private_join.link_code,
            is_teleport,
        )
    } else {
        let url = windows::build_launch_url(
            &ticket,
            private_join.place_id,
            &actual_job,
            &browser_tracker_id,
            &launch_data,
            follow_user,
            private_join.use_private_join,
            &private_join.access_code,
            &private_join.link_code,
            is_teleport,
        );
        windows::launch_url(&url).await
    };
    if let Err(err) = spawn_result {
        emit_launch_log(&app, user_id, "error", "spawn", format!("Falha ao abrir o cliente: {err}"));
        tracker.clear_pending_launch(pending_id);
        return Err(err);
    }

    let detected_pid =
        wait_for_new_roblox_pid(&pids_before, std::time::Duration::from_secs(pid_wait_secs)).await;
    if detected_pid.is_none() && !isolation_will_wipe_install {
        emit_launch_log(&app, user_id, "warn", "pid", "PID não detectado no tempo esperado");
        tracker.clear_pending_launch(pending_id);
    }
    if detected_pid.is_none() {
        // Sem PID não dá para afirmar que a conta entrou (nem rastreá-la), e a
        // fila não deve mostrar um "done" que não aconteceu.
        sequence.mark(
            user_id,
            LaunchQueueState::Failed,
            Some("PID não detectado no tempo esperado".to_string()),
        );
    }
    if let Some(pid) = detected_pid {
        emit_launch_log(&app, user_id, "success", "pid", format!("Cliente iniciado (PID {pid})"));
        sequence.mark(user_id, LaunchQueueState::Done, None);
        tracker.clear_pending_launch(pending_id);
        tracker.track_with_version(
            user_id,
            pid,
            browser_tracker_id.clone(),
            resolved_version_id.clone(),
        );
        if let Some(version_id) = resolved_version_id.as_deref() {
            if let Some((channel, hash)) = version_id.split_once(':') {
                versions.touch_launched(channel, hash);
            }
        }
        apply_windows_post_launch_profile(Some(&app), &settings, LaunchClientProfile::Normal, pid)
            .await;

        let saved_rect = state
            .get_all()?
            .iter()
            .find(|a| a.user_id == user_id)
            .and_then(|a| window_rect_from_fields(&a.fields));
        spawn_client_window_enforcement(
            &app,
            pid,
            client_window_plan(ClientWindowInputs {
                fullscreen: resolved_window.fullscreen,
                window_size: resolved_window.window_size,
                keeps_own_window: account_overrides
                    .as_ref()
                    .is_some_and(|o| o.keeps_own_window()),
                start_minimized,
                auto_arrange_grid: auto_arrange_grid_enabled(&settings),
                saved_rect,
            }),
        );

        if start_minimized {
            let baseline = pids_before.clone();
            tokio::spawn(async move {
                minimize_new_roblox_windows(baseline, std::time::Duration::from_secs(14)).await;
            });
        }
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn launch_roblox(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    settings: tauri::State<'_, SettingsStore>,
    user_id: i64,
    place_id: i64,
    job_id: String,
    launch_data: String,
    follow_user: bool,
    join_vip: bool,
    link_code: String,
    // Opcional para aceitar chamadas que não mandam o campo (Tauri trata a
    // chave ausente como `None`); `None` = não sortear.
    shuffle_job: Option<bool>,
) -> Result<(), String> {
    // O launch de uma conta única também alimenta a fila (com uma entrada só),
    // para a UI mostrar e cancelar do mesmo jeito que num lote — e por isso ele
    // reserva a sequência igual a um lote: um launch avulso disparado durante
    // uma fila brigaria com ela pelo mutex e pelo client settings.
    let sequence = launch_queue_start(&app, &[user_id], place_id, &job_id)?;
    sequence.mark(user_id, LaunchQueueState::Launching, None);
    let result = launch_roblox_other(
        app.clone(),
        &sequence,
        state,
        settings,
        user_id,
        place_id,
        job_id,
        launch_data,
        follow_user,
        join_vip,
        link_code,
        shuffle_job,
    )
    .await;
    if let Err(err) = &result {
        // `set_state` ignora quem já está num estado final, então isto só pega
        // os erros que escaparam do corpo sem marcar nada.
        sequence.mark(user_id, LaunchQueueState::Failed, Some(err.clone()));
    }
    sequence.finish();
    result
}

#[cfg(not(target_os = "windows"))]
#[allow(clippy::too_many_arguments)]
async fn launch_roblox_other(
    app: tauri::AppHandle,
    // Ver o caminho Windows: o guard vem do chamador porque é ele que assina as
    // escritas na fila.
    sequence: &LaunchSequenceGuard,
    state: tauri::State<'_, AccountStore>,
    settings: tauri::State<'_, SettingsStore>,
    user_id: i64,
    place_id: i64,
    job_id: String,
    launch_data: String,
    follow_user: bool,
    join_vip: bool,
    link_code: String,
    shuffle_job: Option<bool>,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use platform::macos;

        if let Some(reason) = moderation_launch_block(state.inner(), &app, &settings, user_id).await {
            emit_launch_log(&app, user_id, "warn", "moderated", reason.clone());
            return Err(reason);
        }

        let is_teleport = settings.get_bool("Developer", "IsTeleport");
        let use_old_join = settings.get_bool("Developer", "UseOldJoin");
        let auto_close_last_process = settings.get_bool("General", "AutoCloseLastProcess");

        let multi_rbx = settings.get_bool("General", "EnableMultiRbx");
        if multi_rbx {
            let enabled = macos::enable_multi_roblox()?;
            if !enabled {
                return Err(
                    "Failed to enable Multi Roblox. Close all Roblox processes and try again."
                        .into(),
                );
            }
        } else {
            let _ = macos::disable_multi_roblox();
        }

        let account_overrides = state
            .get_all()
            .ok()
            .and_then(|list| list.into_iter().find(|a| a.user_id == user_id))
            .and_then(|a| account_client_overrides(&a.fields));
        patch_client_settings_for_launch(
            &settings,
            LaunchClientProfile::Normal,
            account_overrides.as_ref(),
            None,
        );

        let tracker = macos::tracker();
        if auto_close_last_process && tracker.get_pid(user_id).is_some() {
            tracker.kill_for_user(user_id);
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        let mut resolved_launch = resolve_launch_job(&job_id, join_vip, &link_code);
        if follow_user {
            resolved_launch.join_vip = false;
            resolved_launch.link_code.clear();
        }

        let mut actual_job = resolved_launch.job_id.clone();
        if should_shuffle_server(shuffle_job_requested(shuffle_job), follow_user, &actual_job) {
            if let Some(job) = pick_shuffled_public_job(state.inner(), user_id, place_id).await {
                actual_job = job;
            }
        }

        let browser_tracker_id = get_or_create_browser_tracker_id(&state, user_id)?;
        let ticket = run_with_session_retry(state.inner(), user_id, |cookie| async move {
            api::auth::get_auth_ticket(&cookie).await
        })
        .await?;
        let private_join = run_with_session_retry(state.inner(), user_id, |cookie| {
            let resolved_launch = resolved_launch.clone();
            async move { resolve_private_join(&cookie, place_id, &resolved_launch).await }
        })
        .await?;

        let pids_before = macos::get_roblox_pids();

        if use_old_join {
            macos::launch_old_join(
                &ticket,
                private_join.place_id,
                &actual_job,
                &launch_data,
                follow_user,
                private_join.use_private_join,
                &private_join.access_code,
                &private_join.link_code,
                is_teleport,
            )?;
        } else {
            let url = macos::build_launch_url(
                &ticket,
                private_join.place_id,
                &actual_job,
                &browser_tracker_id,
                &launch_data,
                follow_user,
                private_join.use_private_join,
                &private_join.access_code,
                &private_join.link_code,
                is_teleport,
            );
            macos::launch_url(&url)?;
        }

        if let Some(pid) =
            wait_for_new_roblox_pid(&pids_before, std::time::Duration::from_secs(12)).await
        {
            tracker.track(user_id, pid, browser_tracker_id);
            sequence.mark(user_id, LaunchQueueState::Done, None);
        } else {
            // Sem PID não dá para afirmar que a conta entrou.
            sequence.mark(
                user_id,
                LaunchQueueState::Failed,
                Some("PID não detectado no tempo esperado".to_string()),
            );
        }

        return Ok(());
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = (
            app,
            sequence,
            state,
            settings,
            user_id,
            place_id,
            job_id,
            launch_data,
            follow_user,
            join_vip,
            link_code,
            shuffle_job,
        );
        Err("Launching is only supported on Windows and macOS".into())
    }
}

#[cfg(target_os = "windows")]
#[tauri::command]
async fn launch_multiple(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    settings: tauri::State<'_, SettingsStore>,
    versions: tauri::State<'_, data::versions::VersionsCatalogStore>,
    user_ids: Vec<i64>,
    place_id: i64,
    job_id: String,
    launch_data: String,
    // Opcional para aceitar chamadas que não mandam o campo (Tauri trata a
    // chave ausente como `None`); `None` = não sortear.
    shuffle_job: Option<bool>,
) -> Result<(), String> {
    use platform::windows;

    if user_ids.is_empty() {
        // Lote sem conta não é launch: sem esta saída ele reservaria a sequência
        // e rodaria o isolamento pré-launch para não abrir nada.
        return Ok(());
    }

    let shuffle_job = shuffle_job_requested(shuffle_job);
    let delay = effective_join_delay_seconds(settings.get_int("General", "AccountJoinDelay"));
    let multi_rbx = settings.get_bool("General", "EnableMultiRbx");
    let async_join = settings.get_bool("General", "AsyncJoin");
    let is_teleport = settings.get_bool("Developer", "IsTeleport");
    let configured_old_join = settings.get_bool("Developer", "UseOldJoin");
    let auto_close_last_process = settings.get_bool("General", "AutoCloseLastProcess");
    let auto_close_multi_conflicts = settings.get_bool("General", "AutoCloseRobloxForMultiRbx");
    let start_minimized = settings.get_bool("General", "StartRobloxMinimized");
    // A fila é por execução: este lote substitui o anterior — desde que o
    // anterior tenha acabado. A reserva cobre o lote inteiro aqui, antes de
    // qualquer launch, e antes de mexer no tracker: um lote recusado não pode
    // apagar o cancelamento (`Close All Roblox`) do lote que está rodando.
    let sequence = launch_queue_start(&app, &user_ids, place_id, &job_id)?;

    let tracker = windows::tracker();
    tracker.reset_launch_cancelled();

    let isolation_report = match run_pre_launch_isolation(&app, &settings).await {
        Ok(value) => value,
        Err(err) => {
            sequence.abort(None, &err);
            return Err(err);
        }
    };
    if let Some(report) = isolation_report {
        let _ = app.emit("isolation-report", &report);
        if windows::has_pending_fast_flags() {
            tokio::spawn(apply_pending_fast_flags_when_ready(
                std::time::Duration::from_secs(240),
            ));
        }
    }

    let accounts = match state.get_all() {
        Ok(value) => value,
        Err(err) => {
            sequence.abort(None, &err);
            return Err(err);
        }
    };

    for (i, &uid) in user_ids.iter().enumerate() {
        if tracker.is_launch_cancelled() {
            // "Close All Roblox": nada do que sobrou vai ser tentado.
            sequence.cancel_remaining();
            break;
        }

        // A conta pode ter sido cancelada (`cancel_account_launch` ou
        // `stop_launch_queue`) enquanto a fila andava: pular é só não fazer
        // nada por ela — nenhum cliente já aberto é tocado.
        if sequence.is_cancelled(uid) {
            continue;
        }
        sequence.mark(uid, LaunchQueueState::Launching, None);

        let iter_start = std::time::Instant::now();

        let account = accounts.iter().find(|a| a.user_id == uid);
        // Always launch into the selected place/job. Per-account "saved game"
        // overrides were removed so every account joins exactly the game the
        // user picked (previously a saved SavedPlaceId/SavedJobId silently sent
        // some accounts to a different server).
        let acct_place = place_id;
        let acct_job = job_id.clone();
        let acct_version_override = account
            .and_then(|a| a.fields.get("RobloxVersion").cloned())
            .filter(|v| !v.trim().is_empty());
        // Exceções desta conta. A fila é sequencial e o patch roda logo antes de
        // cada spawn, então cada cliente abre com o que a sua conta pediu.
        let acct_overrides = account.and_then(|a| account_client_overrides(&a.fields));
        let acct_start_minimized = acct_overrides
            .as_ref()
            .and_then(|o| o.start_minimized)
            .unwrap_or(start_minimized);

        let acct_target_desc = launch_target_description(false, "", &acct_job);
        emit_launch_log(
            &app,
            uid,
            "info",
            "start",
            format!(
                "Conta {}/{} — place {acct_place} ({acct_target_desc})",
                i + 1,
                user_ids.len()
            ),
        );

        // Banida/encerrada: pula esta conta e segue a fila (commands/moderation.rs).
        if let Some(reason) = moderation_launch_block(state.inner(), &app, &settings, uid).await {
            emit_launch_log(&app, uid, "warn", "moderated", reason.clone());
            sequence.mark(uid, LaunchQueueState::Failed, Some(reason));
            continue;
        }

        let (acct_base_path, acct_version_id) = match windows::resolve_roblox_install_path(
            acct_version_override.as_deref(),
            &settings,
            &versions,
        ) {
            Ok(value) => value,
            Err(err) => {
                sequence.mark(uid, LaunchQueueState::Failed, Some(err.clone()));
                let _ = app.emit(
                    "launch-progress",
                    serde_json::json!({
                        "userId": uid,
                        "index": i,
                        "total": user_ids.len(),
                        "error": "version-resolve-failed",
                        "message": err,
                    }),
                );
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };
        let acct_isolation_wipes_install = isolation_wipes_install(
            &settings.get_string("Isolation", "Mode"),
            acct_version_id.as_deref(),
        );
        let acct_use_old_join = resolve_use_old_join(
            acct_isolation_wipes_install,
            configured_old_join,
            acct_version_id.as_deref(),
        );

        let _ = tracker.cleanup_dead_processes();
        let running_keys = tracker.running_version_keys();
        // Lido a cada conta: ligar o toggle no meio da fila já vale para a próxima.
        let allow_launch_on_open_version = settings.get_bool("Versions", "AllowLaunchOnOpenVersion");
        if version_guard_blocks(&running_keys, &acct_version_id, allow_launch_on_open_version) {
            // O painel de sessão desenha `entry.error` cru: aqui vai frase, não
            // o código `version-conflict` que viaja no evento `launch-progress`.
            sequence.mark(
                uid,
                LaunchQueueState::Failed,
                Some(version_conflict_message(
                    &running_keys,
                    &acct_version_id,
                    allow_launch_on_open_version,
                )),
            );
            let _ = app.emit(
                "launch-progress",
                serde_json::json!({
                    "userId": uid,
                    "index": i,
                    "total": user_ids.len(),
                    "error": "version-conflict",
                }),
            );
            continue;
        }

        let _ = app.emit(
            "launch-progress",
            serde_json::json!({
                "userId": uid,
                "index": i,
                "total": user_ids.len(),
            }),
        );

        let mut resolved_launch = resolve_launch_job(&acct_job, false, "");

        // Sorteio por conta: cada conta busca a lista de servidores públicos e
        // escolhe o seu, então o lote se espalha em vez de empilhar todo mundo
        // no mesmo servidor. `follow_user` não existe no multi launch.
        if should_shuffle_server(shuffle_job, false, &resolved_launch.job_id) {
            if let Some(job) = pick_shuffled_public_job(state.inner(), uid, acct_place).await {
                emit_launch_log(
                    &app,
                    uid,
                    "info",
                    "target",
                    format!("Servidor sorteado para esta conta: {job}"),
                );
                resolved_launch.job_id = job;
            }
        }

        if multi_rbx {
            // Falha de Multi Roblox aborta a fila inteira (diferente dos erros
            // por conta): sem o mutex, todo cliente novo derruba o anterior.
            if let Err(err) = ensure_multi_roblox_enabled(auto_close_multi_conflicts).await {
                sequence.abort(Some(uid), &err);
                return Err(err);
            }
        } else {
            let _ = windows::disable_multi_roblox();
        }

        windows::refresh_production_version().await;
        // Mesma pasta para o patch e para o spawn do old join (ver o launch de
        // uma conta, acima).
        let acct_client_dir = windows::client_dir(
            windows::client_source(acct_use_old_join, acct_version_id.is_some()),
            &acct_base_path,
        )
        .await;

        if auto_close_last_process && tracker.get_pid(uid).is_some() {
            let closed = tracker.kill_for_user_graceful_async(uid, 4500).await;
            if !closed {
                sequence.mark(
                    uid,
                    LaunchQueueState::Failed,
                    Some("Previous Roblox instance did not close before relaunch".to_string()),
                );
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }

        let browser_tracker_id = match get_or_create_browser_tracker_id(&state, uid) {
            Ok(value) => value,
            Err(err) => {
                sequence.abort(Some(uid), &err);
                return Err(err);
            }
        };
        emit_launch_log(&app, uid, "info", "auth", "Solicitando authentication ticket...");
        let ticket = match run_with_session_retry(state.inner(), uid, |cookie| async move {
            api::auth::get_auth_ticket(&cookie).await
        })
        .await
        {
            Ok(t) => {
                emit_launch_log(&app, uid, "success", "auth", "Authentication ticket obtido");
                t
            }
            Err(e) => {
                emit_launch_log(&app, uid, "error", "auth", format!("Falha no auth ticket: {e}"));
                if is_moderated_error(&e) {
                    mark_account_moderated(state.inner(), &app, uid);
                    emit_launch_log(&app, uid, "warn", "moderated", "Conta movida para o grupo 'moderadas'");
                }
                sequence.mark(uid, LaunchQueueState::Failed, Some(e));
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };
        let private_join = match run_with_session_retry(state.inner(), uid, |cookie| {
            let resolved_launch = resolved_launch.clone();
            async move { resolve_private_join(&cookie, acct_place, &resolved_launch).await }
        })
        .await
        {
            Ok(value) => value,
            Err(err) => {
                sequence.mark(uid, LaunchQueueState::Failed, Some(err));
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        // "Close All Roblox" may have been clicked while this account was
        // fetching its ticket; don't spawn a client right after the kill.
        if tracker.is_launch_cancelled() {
            // Esta conta não chegou a abrir cliente, então vira `cancelled`
            // (e não `failed`); o resto da fila também.
            sequence.mark(uid, LaunchQueueState::Cancelled, None);
            sequence.cancel_remaining();
            break;
        }

        // Último passo antes do spawn (ver o launch de uma conta, acima): os
        // clientes que este lote já abriu reescrevem o XML enquanto esta conta
        // espera o ticket.
        let acct_window = patch_client_settings_for_launch(
            &settings,
            LaunchClientProfile::Normal,
            acct_overrides.as_ref(),
            Some(&acct_client_dir),
        );

        let pids_before = windows::get_roblox_pids();

        let acct_pid_wait_secs = pid_wait_seconds(acct_isolation_wipes_install);
        let acct_pending_id = tracker.add_pending_launch(
            uid,
            acct_version_id.clone(),
            std::time::Duration::from_secs(acct_pid_wait_secs + 30),
        );

        let launch_result = if acct_use_old_join {
            windows::launch_old_join_from(
                &acct_client_dir,
                &ticket,
                private_join.place_id,
                &resolved_launch.job_id,
                &launch_data,
                false,
                private_join.use_private_join,
                &private_join.access_code,
                &private_join.link_code,
                is_teleport,
            )
        } else {
            let url = windows::build_launch_url(
                &ticket,
                private_join.place_id,
                &resolved_launch.job_id,
                &browser_tracker_id,
                &launch_data,
                false,
                private_join.use_private_join,
                &private_join.access_code,
                &private_join.link_code,
                is_teleport,
            );
            windows::launch_url(&url).await
        };

        if let Err(err) = &launch_result {
            emit_launch_log(&app, uid, "error", "spawn", format!("Falha ao abrir o cliente: {err}"));
            sequence.mark(uid, LaunchQueueState::Failed, Some(err.clone()));
            tracker.clear_pending_launch(acct_pending_id);
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            continue;
        }

        let acct_detected_pid = wait_for_new_roblox_pid(
            &pids_before,
            std::time::Duration::from_secs(acct_pid_wait_secs),
        )
        .await;
        if acct_detected_pid.is_none() && !acct_isolation_wipes_install {
            emit_launch_log(&app, uid, "warn", "pid", "PID não detectado no tempo esperado");
            tracker.clear_pending_launch(acct_pending_id);
        }
        if acct_detected_pid.is_none() {
            // Sem PID não dá para afirmar que a conta entrou (nem rastreá-la).
            sequence.mark(uid, LaunchQueueState::Failed, Some("PID não detectado no tempo esperado".to_string()));
        }
        if let Some(pid) = acct_detected_pid {
            emit_launch_log(&app, uid, "success", "pid", format!("Cliente iniciado (PID {pid})"));
            sequence.mark(uid, LaunchQueueState::Done, None);
            tracker.clear_pending_launch(acct_pending_id);
            tracker.track_with_version(uid, pid, browser_tracker_id, acct_version_id.clone());
            if let Some(version_id) = acct_version_id.as_deref() {
                if let Some((channel, hash)) = version_id.split_once(':') {
                    versions.touch_launched(channel, hash);
                }
            }
            apply_windows_post_launch_profile(
                Some(&app),
                &settings,
                LaunchClientProfile::Normal,
                pid,
            )
            .await;
            spawn_client_window_enforcement(
                &app,
                pid,
                client_window_plan(ClientWindowInputs {
                    fullscreen: acct_window.fullscreen,
                    window_size: acct_window.window_size,
                    keeps_own_window: acct_overrides
                        .as_ref()
                        .is_some_and(|o| o.keeps_own_window()),
                    start_minimized: acct_start_minimized,
                    // Lido a cada conta: desligar no meio da fila já vale.
                    auto_arrange_grid: auto_arrange_grid_enabled(&settings),
                    saved_rect: None,
                }),
            );
            if acct_start_minimized {
                let baseline = pids_before.clone();
                tokio::spawn(async move {
                    minimize_new_roblox_windows(baseline, std::time::Duration::from_secs(14)).await;
                });
            }
        }

        // Só espera se ainda houver conta para lançar: com a fila parada pelo
        // usuário, esperar aqui é prender o app sem ter o que fazer depois.
        if i < user_ids.len() - 1 && sequence.queued_count() > 0 {
            if async_join {
                tracker.reset_next_account();
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
                while !tracker.is_next_account()
                    && !tracker.is_launch_cancelled()
                    && sequence.queued_count() > 0
                {
                    if std::time::Instant::now() > deadline {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            } else {
                // Space launches by `delay` measured from the START of this account's
                // launch — subtract the time already spent (auth + waiting for the PID)
                // instead of stacking another full delay on top of it.
                //
                // But always keep a minimum residual gap plus a little jitter
                // between consecutive accounts. If this account's own work
                // (auth + PID wait) already took >= `delay`, the naive
                // `target - elapsed` collapses to zero and the next
                // authentication-ticket request fires back-to-back, which is
                // what trips Roblox's captcha. A non-zero, slightly randomized
                // gap avoids both back-to-back requests and a perfectly
                // periodic cadence.
                let jitter_ms = launch_jitter_ms(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.subsec_millis())
                        .unwrap_or(0),
                );
                // Lido a cada conta: desligar no meio da fila já vale.
                let wait_for_join = settings.get_string("General", "WaitForGameJoin") != "false";
                match acct_detected_pid.filter(|_| wait_for_join) {
                    Some(pid) => {
                        // Segue assim que o log disser que a conta entrou no
                        // jogo (client_health.rs), nunca antes do piso
                        // anti-captcha; sem log, a espera fixa de sempre.
                        let plan = join_wait_plan(delay, iter_start.elapsed(), jitter_ms);
                        emit_launch_log(
                            &app,
                            uid,
                            "info",
                            "wait",
                            format!(
                                "Esperando a conta entrar no jogo antes da próxima (entre {}s e {}s)",
                                plan.floor.as_secs(),
                                plan.cap.as_secs()
                            ),
                        );
                        let joined = wait_for_game_join(
                            &sequence,
                            plan,
                            || join_signal_from(client_health_of(uid, pid).as_ref()),
                            || tracker.is_launch_cancelled(),
                        )
                        .await;
                        if joined {
                            emit_launch_log(&app, uid, "info", "wait", "Entrou no jogo: próxima conta");
                        }
                    }
                    None => {
                        let wait = next_account_wait(delay, iter_start.elapsed(), jitter_ms);
                        emit_launch_log(
                            &app,
                            uid,
                            "info",
                            "wait",
                            format!("Aguardando {}s antes da próxima conta (anti-captcha)", wait.as_secs()),
                        );
                        wait_before_next_account(&sequence, wait, || tracker.is_launch_cancelled()).await;
                    }
                }
            }
        }
    }

    sequence.finish();
    let _ = app.emit("launch-complete", serde_json::json!({}));
    Ok(())
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn launch_multiple(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    settings: tauri::State<'_, SettingsStore>,
    user_ids: Vec<i64>,
    place_id: i64,
    job_id: String,
    launch_data: String,
    // Opcional para aceitar chamadas que não mandam o campo (Tauri trata a
    // chave ausente como `None`); `None` = não sortear.
    shuffle_job: Option<bool>,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use platform::macos;

        if user_ids.is_empty() {
            // Ver o caminho Windows: lote vazio não reserva sequência nenhuma.
            return Ok(());
        }

        let shuffle_job = shuffle_job_requested(shuffle_job);
        // macOS mantém o espaçamento próprio (sem o piso de 8 s do Windows),
        // mas já sem o cast que fazia um valor negativo virar `u64::MAX`.
        let delay = configured_join_delay_seconds(settings.get_int("General", "AccountJoinDelay"));
        let multi_rbx = settings.get_bool("General", "EnableMultiRbx");
        let delay = if multi_rbx { delay.max(12) } else { delay };
        let async_join = settings.get_bool("General", "AsyncJoin");
        let is_teleport = settings.get_bool("Developer", "IsTeleport");
        let use_old_join = settings.get_bool("Developer", "UseOldJoin");
        let auto_close_last_process = settings.get_bool("General", "AutoCloseLastProcess");
        // A fila é por execução: este lote substitui o anterior — desde que o
        // anterior tenha acabado. A reserva cobre o lote inteiro aqui, antes de
        // qualquer launch, e antes de mexer no tracker: um lote recusado não
        // pode apagar o cancelamento (`Close All Roblox`) do lote que roda.
        let sequence = launch_queue_start(&app, &user_ids, place_id, &job_id)?;

        let tracker = macos::tracker();
        tracker.reset_launch_cancelled();

        for (i, &uid) in user_ids.iter().enumerate() {
            if tracker.is_launch_cancelled() {
                sequence.cancel_remaining();
                break;
            }

            // Conta cancelada pela UI enquanto a fila andava: pular é só não
            // fazer nada por ela — nenhum cliente já aberto é tocado.
            if sequence.is_cancelled(uid) {
                continue;
            }
            sequence.mark(uid, LaunchQueueState::Launching, None);

            if let Some(reason) = moderation_launch_block(state.inner(), &app, &settings, uid).await {
                emit_launch_log(&app, uid, "warn", "moderated", reason.clone());
                sequence.mark(uid, LaunchQueueState::Failed, Some(reason));
                continue;
            }

            // Always launch into the selected place/job (per-account saved-game
            // overrides removed — see the Windows path for rationale).
            let acct_place = place_id;
            let acct_job = job_id.clone();

            let _ = app.emit(
                "launch-progress",
                serde_json::json!({
                    "userId": uid,
                    "index": i,
                    "total": user_ids.len(),
                }),
            );

            let mut resolved_launch = resolve_launch_job(&acct_job, false, "");

            // Sorteio por conta (ver o caminho Windows): cada conta escolhe o
            // seu servidor público, não o mesmo para todas.
            if should_shuffle_server(shuffle_job, false, &resolved_launch.job_id) {
                if let Some(job) = pick_shuffled_public_job(state.inner(), uid, acct_place).await {
                    resolved_launch.job_id = job;
                }
            }

            if multi_rbx {
                let enabled = match macos::enable_multi_roblox() {
                    Ok(value) => value,
                    Err(err) => {
                        sequence.abort(Some(uid), &err);
                        return Err(err);
                    }
                };
                if !enabled {
                    let err =
                        "Failed to enable Multi Roblox. Close all Roblox processes and try again."
                            .to_string();
                    sequence.abort(Some(uid), &err);
                    return Err(err);
                }
            } else {
                let _ = macos::disable_multi_roblox();
            }

            let acct_overrides = accounts
                .iter()
                .find(|a| a.user_id == uid)
                .and_then(|a| account_client_overrides(&a.fields));
            patch_client_settings_for_launch(
                &settings,
                LaunchClientProfile::Normal,
                acct_overrides.as_ref(),
                None,
            );

            if auto_close_last_process && tracker.get_pid(uid).is_some() {
                tracker.kill_for_user(uid);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }

            let browser_tracker_id = match get_or_create_browser_tracker_id(&state, uid) {
                Ok(value) => value,
                Err(err) => {
                    sequence.abort(Some(uid), &err);
                    return Err(err);
                }
            };
            let ticket = match run_with_session_retry(state.inner(), uid, |cookie| async move {
                api::auth::get_auth_ticket(&cookie).await
            })
            .await
            {
                Ok(t) => t,
                Err(err) => {
                    sequence.mark(uid, LaunchQueueState::Failed, Some(err));
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    continue;
                }
            };
            let private_join =
                match run_with_session_retry(state.inner(), uid, |cookie| {
                    let resolved_launch = resolved_launch.clone();
                    async move { resolve_private_join(&cookie, acct_place, &resolved_launch).await }
                })
                .await
                {
                Ok(value) => value,
                Err(err) => {
                    sequence.mark(uid, LaunchQueueState::Failed, Some(err));
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    continue;
                }
            };

            // Cancelado durante o auth: esta conta não chega a abrir cliente.
            if tracker.is_launch_cancelled() {
                sequence.mark(uid, LaunchQueueState::Cancelled, None);
                sequence.cancel_remaining();
                break;
            }

            let pids_before = macos::get_roblox_pids();

            let launch_result = if use_old_join {
                macos::launch_old_join(
                    &ticket,
                    private_join.place_id,
                    &resolved_launch.job_id,
                    &launch_data,
                    false,
                    private_join.use_private_join,
                    &private_join.access_code,
                    &private_join.link_code,
                    is_teleport,
                )
            } else {
                let url = macos::build_launch_url(
                    &ticket,
                    private_join.place_id,
                    &resolved_launch.job_id,
                    &browser_tracker_id,
                    &launch_data,
                    false,
                    private_join.use_private_join,
                    &private_join.access_code,
                    &private_join.link_code,
                    is_teleport,
                );
                macos::launch_url(&url)
            };

            if let Err(err) = &launch_result {
                sequence.mark(uid, LaunchQueueState::Failed, Some(err.clone()));
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }

            if let Some(pid) =
                wait_for_new_roblox_pid(&pids_before, std::time::Duration::from_secs(12)).await
            {
                tracker.track(uid, pid, browser_tracker_id);
                sequence.mark(uid, LaunchQueueState::Done, None);
            } else {
                // Sem PID não dá para afirmar que a conta entrou.
                sequence.mark(uid, LaunchQueueState::Failed, Some("PID não detectado no tempo esperado".to_string()));
            }

            // Ver o caminho Windows: sem conta esperando a vez, não há por que
            // esperar — e a espera é fatiada para o cancelamento cortá-la.
            if i < user_ids.len() - 1 && sequence.queued_count() > 0 {
                if async_join {
                    tracker.reset_next_account();
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
                    while !tracker.is_next_account()
                        && !tracker.is_launch_cancelled()
                        && sequence.queued_count() > 0
                    {
                        if std::time::Instant::now() > deadline {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                } else {
                    wait_before_next_account(
                        &sequence,
                        std::time::Duration::from_secs(delay),
                        || tracker.is_launch_cancelled(),
                    )
                    .await;
                }
            }
        }

        sequence.finish();
        let _ = app.emit("launch-complete", serde_json::json!({}));
        return Ok(());
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = (
            app,
            state,
            settings,
            user_ids,
            place_id,
            job_id,
            launch_data,
            shuffle_job,
        );
        Err("Launching is only supported on Windows and macOS".into())
    }
}

/// Levanta a flag de cancelamento no tracker. Separada do comando para poder
/// ser testada sem `AppHandle`.
fn signal_cancel_launch() {
    #[cfg(target_os = "windows")]
    {
        platform::windows::tracker().cancel_launch();
    }
    #[cfg(target_os = "macos")]
    {
        platform::macos::tracker().cancel_launch();
    }
}

#[tauri::command]
fn cancel_launch(app: tauri::AppHandle) -> Result<(), String> {
    signal_cancel_launch();
    // O loop só marcaria as restantes como `cancelled` no próximo checkpoint,
    // que pode levar mais de 8 s (o intervalo anti-captcha). Quem apertou
    // "Close All" precisa ver a fila esvaziar na hora.
    launch_queue_cancel_all_queued(&app);
    Ok(())
}

#[tauri::command]
fn next_account() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        platform::windows::tracker().signal_next_account();
    }
    #[cfg(target_os = "macos")]
    {
        platform::macos::tracker().signal_next_account();
    }
    Ok(())
}

#[tauri::command]
fn cmd_kill_roblox(user_id: i64) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        return Ok(platform::windows::tracker().kill_for_user(user_id));
    }
    #[cfg(target_os = "macos")]
    {
        return Ok(platform::macos::tracker().kill_for_user(user_id));
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = user_id;
        Err("Not supported on this platform".into())
    }
}

#[tauri::command]
fn focus_roblox_window(user_id: i64) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        let tracker = platform::windows::tracker();
        let Some(pid) = tracker.get_pid(user_id) else {
            return Ok(false);
        };
        let Some(hwnd) = platform::windows::find_main_window(pid) else {
            return Ok(false);
        };
        return Ok(platform::windows::focus_window(hwnd));
    }
    #[cfg(target_os = "macos")]
    {
        let _ = user_id;
        return Ok(false);
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = user_id;
        Ok(false)
    }
}

#[derive(serde::Serialize)]
struct GridArrangeResult {
    arranged: usize,
    total: usize,
}

/// List physical monitors so the Console can offer them as grid targets.
#[tauri::command]
fn list_display_monitors() -> Result<serde_json::Value, String> {
    #[cfg(target_os = "windows")]
    {
        return Ok(serde_json::to_value(platform::windows::list_monitors())
            .unwrap_or_else(|_| serde_json::json!([])));
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(serde_json::json!([]))
    }
}

/// Arrange the open Roblox windows into the grid across the selected monitors
/// (1-based indices; empty = all monitors) — the same slots the automatic grid
/// uses. Accounts with their own window size (launch exception) are left
/// alone, and the cell is the global window size when it is on.
#[tauri::command]
fn arrange_windows_grid(
    state: tauri::State<'_, AccountStore>,
    settings: tauri::State<'_, SettingsStore>,
    monitor_indices: Vec<usize>,
    gap: i32,
) -> Result<GridArrangeResult, String> {
    #[cfg(target_os = "windows")]
    {
        let size = global_window_size(&settings, LaunchClientProfile::Normal);
        let excluded = grid_excluded_pids(state.inner());
        let (arranged, total) =
            platform::windows::arrange_roblox_grid(&monitor_indices, gap, size, &excluded)?;
        return Ok(GridArrangeResult { arranged, total });
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (state, settings, monitor_indices, gap);
        Err("Grid de janelas só é suportado no Windows.".to_string())
    }
}

#[tauri::command]
fn cmd_kill_all_roblox() -> Result<u32, String> {
    #[cfg(target_os = "windows")]
    {
        let killed = platform::windows::kill_all_roblox();
        let tracker = platform::windows::tracker();
        tracker.cancel_launch();
        let all = tracker.get_all();
        for p in all {
            tracker.untrack(p.user_id);
        }
        return Ok(killed);
    }
    #[cfg(target_os = "macos")]
    {
        let killed = platform::macos::kill_all_roblox();
        let tracker = platform::macos::tracker();
        tracker.cancel_launch();
        let all = tracker.get_all();
        for p in all {
            tracker.untrack(p.user_id);
        }
        return Ok(killed);
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        Err("Not supported on this platform".into())
    }
}

#[derive(serde::Serialize)]
struct RunningInstance {
    pid: u32,
    user_id: i64,
    browser_tracker_id: String,
    /// Aberto fora do app (pelo site) e reconhecido depois — ver external_clients.rs.
    adopted: bool,
    /// Queda lida do log do cliente — ver client_health.rs. `None` enquanto o
    /// monitor não viu este PID.
    health: Option<ClientHealthView>,
}

#[tauri::command]
fn get_running_instances() -> Result<Vec<RunningInstance>, String> {
    #[cfg(target_os = "windows")]
    {
        return Ok(platform::windows::tracker()
            .get_all()
            .into_iter()
            .map(|p| RunningInstance {
                pid: p.pid,
                user_id: p.user_id,
                health: client_health_of(p.user_id, p.pid),
                browser_tracker_id: p.browser_tracker_id,
                adopted: p.adopted,
            })
            .collect());
    }
    #[cfg(target_os = "macos")]
    {
        return Ok(platform::macos::tracker()
            .get_all()
            .into_iter()
            .map(|p| RunningInstance {
                pid: p.pid,
                user_id: p.user_id,
                browser_tracker_id: p.browser_tracker_id,
                adopted: false,
                health: None,
            })
            .collect());
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        Ok(Vec::new())
    }
}

#[tauri::command]
fn cmd_enable_multi_roblox() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        return platform::windows::enable_multi_roblox();
    }
    #[cfg(target_os = "macos")]
    {
        return platform::macos::enable_multi_roblox();
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        Err("Not supported on this platform".into())
    }
}

#[tauri::command]
fn cmd_disable_multi_roblox() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        return platform::windows::disable_multi_roblox();
    }
    #[cfg(target_os = "macos")]
    {
        return platform::macos::disable_multi_roblox();
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        Ok(())
    }
}

#[tauri::command]
fn cmd_get_roblox_path() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        return platform::windows::get_roblox_path();
    }
    #[cfg(target_os = "macos")]
    {
        return platform::macos::get_roblox_path();
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        Err("Not supported on this platform".into())
    }
}

#[tauri::command]
fn cmd_apply_fps_unlock(max_fps: u32) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        return platform::windows::apply_fps_unlock(max_fps);
    }
    #[cfg(target_os = "macos")]
    {
        return platform::macos::apply_fps_unlock(max_fps);
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = max_fps;
        Err("Not supported on this platform".into())
    }
}

// Máquina de estados da fila de launch observável. Tudo aqui roda sobre
// `LaunchQueue` (lógica pura) — os wrappers `launch_queue_*` só acrescentam o
// `AppHandle` e a emissão do evento, que não dá para instanciar em teste.
