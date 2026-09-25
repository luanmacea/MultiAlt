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
}

impl LaunchQueue {
    /// Começa um lote novo. A fila é **por execução**: o lote anterior é
    /// descartado inteiro, porque a UI mostra sempre o lote atual.
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

    /// Fim do lote: a UI para de mostrar a fila como ativa, mas as entradas
    /// continuam lá para o usuário ver o resultado conta a conta.
    fn finish(&mut self) {
        self.active = false;
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

/// Substitui a fila pelo lote que está começando e publica o estado inicial.
fn launch_queue_start(app: &tauri::AppHandle, user_ids: &[i64], place_id: i64, job_id: &str) {
    let (_, payload) =
        with_launch_queue(|queue| queue.start(user_ids, place_id, job_id, launch_queue_now_ms()));
    emit_launch_queue(app, &payload);
}

/// Aplica uma transição e publica o evento. Devolve `false` quando a transição
/// foi recusada (conta fora da fila ou já num estado final).
/// Quais estados da fila contam como "a conta foi usada". Só `Done` — é a
/// definição do próprio app para "o cliente subiu". Fica separado para poder ser
/// testado sem `AppHandle`: o risco real aqui é alguém passar a marcar em
/// `Launching`, e aí `last_use` voltaria a medir tentativa em vez de uso.
fn launch_state_means_used(state: LaunchQueueState) -> bool {
    matches!(state, LaunchQueueState::Done)
}

fn launch_queue_mark(
    app: &tauri::AppHandle,
    user_id: i64,
    state: LaunchQueueState,
    error: Option<String>,
) -> bool {
    let (changed, payload) =
        with_launch_queue(|queue| queue.set_state(user_id, state, error, launch_queue_now_ms()));
    if changed {
        emit_launch_queue(app, &payload);
    }
    // `Done` é a definição do próprio app para "o cliente subiu": é aqui que
    // `last_use` tem de andar. Antes ele só era escrito ao criar ou re-adicionar a
    // conta, então a coluna "3d"/"2mo" e a bolinha de envelhecimento mediam idade
    // do cadastro, não inatividade de jogo. Marcar num ponto só cobre conta única,
    // lote e as duas plataformas. Falhar ao gravar não derruba um launch que deu
    // certo — por isso o erro é ignorado de propósito.
    if changed && launch_state_means_used(state) {
        let _ = app.state::<AccountStore>().mark_used(user_id);
    }
    changed
}

/// O loop pergunta isto antes de trabalhar numa conta. Só devolve `true` para
/// quem foi explicitamente cancelado: se a entrada sumiu (um lote novo
/// substituiu a fila), o launch em andamento segue normalmente.
fn launch_queue_is_cancelled(user_id: i64) -> bool {
    with_launch_queue(|queue| queue.state_of(user_id)).0 == Some(LaunchQueueState::Cancelled)
}

/// Cancela tudo que ainda está `queued` e devolve a contagem.
fn launch_queue_cancel_remaining(app: &tauri::AppHandle) -> usize {
    let (cancelled, payload) = with_launch_queue(|queue| queue.cancel_queued(launch_queue_now_ms()));
    if cancelled > 0 {
        emit_launch_queue(app, &payload);
    }
    cancelled
}

fn launch_queue_finish(app: &tauri::AppHandle) {
    let (_, payload) = with_launch_queue(|queue| queue.finish());
    emit_launch_queue(app, &payload);
}

/// Aborta o lote inteiro por um erro fatal: a conta em andamento (quando há
/// uma) vira `failed` com a mensagem e quem nunca chegou a ser tentado vira
/// `cancelled`.
fn launch_queue_abort(app: &tauri::AppHandle, user_id: Option<i64>, error: &str) {
    if let Some(user_id) = user_id {
        launch_queue_mark(
            app,
            user_id,
            LaunchQueueState::Failed,
            Some(error.to_string()),
        );
    }
    launch_queue_cancel_remaining(app);
    launch_queue_finish(app);
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
    launch_queue_cancel_remaining(&app)
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
    // para a UI mostrar e cancelar do mesmo jeito que num lote.
    launch_queue_start(&app, &[user_id], place_id, &job_id);
    let result = launch_roblox_windows(
        app.clone(),
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
        launch_queue_mark(&app, user_id, LaunchQueueState::Failed, Some(err.clone()));
    }
    launch_queue_finish(&app);
    result
}

#[cfg(target_os = "windows")]
#[allow(clippy::too_many_arguments)]
async fn launch_roblox_windows(
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
    shuffle_job: Option<bool>,
) -> Result<(), String> {
    use platform::windows;

    launch_queue_mark(&app, user_id, LaunchQueueState::Launching, None);

    let target_desc = launch_target_description(join_vip, &link_code, &job_id);
    emit_launch_log(
        &app,
        user_id,
        "info",
        "start",
        format!("Iniciando launch — place {place_id} ({target_desc})"),
    );

    let is_teleport = settings.get_bool("Developer", "IsTeleport");
    let configured_old_join = settings.get_bool("Developer", "UseOldJoin");
    let auto_close_last_process = settings.get_bool("General", "AutoCloseLastProcess");
    let auto_close_multi_conflicts = settings.get_bool("General", "AutoCloseRobloxForMultiRbx");
    let start_minimized = settings.get_bool("General", "StartRobloxMinimized");

    let account_snapshot_for_version = state.get_all()?;
    let account_version_override = account_snapshot_for_version
        .iter()
        .find(|a| a.user_id == user_id)
        .and_then(|a| a.fields.get("RobloxVersion").cloned())
        .filter(|v| !v.trim().is_empty());

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
    if has_version_conflict(&running_keys, &resolved_version_id) {
        return Err(
            "A Roblox client is already running on a different version. Close it before launching this account on a different Roblox version. Concurrent multi-version support is planned for a future update.".into(),
        );
    }

    let multi_rbx = settings.get_bool("General", "EnableMultiRbx");
    if multi_rbx {
        ensure_multi_roblox_enabled(auto_close_multi_conflicts).await?;
    } else {
        let _ = windows::disable_multi_roblox();
    }

    windows::refresh_production_version().await;
    patch_client_settings_for_launch(&settings, LaunchClientProfile::Normal);

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

    let pids_before = windows::get_roblox_pids();

    let pid_wait_secs = pid_wait_seconds(isolation_will_wipe_install);
    let pending_id = tracker.add_pending_launch(
        user_id,
        resolved_version_id.clone(),
        std::time::Duration::from_secs(pid_wait_secs + 30),
    );

    let spawn_result = if use_old_join {
        let base_path = if resolved_version_id.is_none() {
            windows::default_player_dir(&resolved_base_path).await
        } else {
            resolved_base_path.clone()
        };
        windows::launch_old_join_from(
            &base_path,
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
        launch_queue_mark(
            &app,
            user_id,
            LaunchQueueState::Failed,
            Some("PID não detectado no tempo esperado".to_string()),
        );
    }
    if let Some(pid) = detected_pid {
        emit_launch_log(&app, user_id, "success", "pid", format!("Cliente iniciado (PID {pid})"));
        launch_queue_mark(&app, user_id, LaunchQueueState::Done, None);
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

        let accounts = state.get_all()?;
        if let Some(account) = accounts.iter().find(|a| a.user_id == user_id) {
            if let Some((x, y, w, h)) = window_rect_from_fields(&account.fields) {
                let target_pid = pid;
                tokio::spawn(async move {
                    for _ in 0..45 {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        if let Some(hwnd) = windows::find_main_window(target_pid) {
                            windows::set_window_position(hwnd, x, y, w, h);
                            break;
                        }
                    }
                });
            }
        }

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
    // para a UI mostrar e cancelar do mesmo jeito que num lote.
    launch_queue_start(&app, &[user_id], place_id, &job_id);
    launch_queue_mark(&app, user_id, LaunchQueueState::Launching, None);
    let result = launch_roblox_other(
        app.clone(),
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
        launch_queue_mark(&app, user_id, LaunchQueueState::Failed, Some(err.clone()));
    }
    launch_queue_finish(&app);
    result
}

#[cfg(not(target_os = "windows"))]
#[allow(clippy::too_many_arguments)]
async fn launch_roblox_other(
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
    shuffle_job: Option<bool>,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use platform::macos;

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

        patch_client_settings_for_launch(&settings, LaunchClientProfile::Normal);

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
            launch_queue_mark(&app, user_id, LaunchQueueState::Done, None);
        } else {
            // Sem PID não dá para afirmar que a conta entrou.
            launch_queue_mark(
                &app,
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

    let shuffle_job = shuffle_job_requested(shuffle_job);
    let delay = effective_join_delay_seconds(settings.get_int("General", "AccountJoinDelay"));
    let multi_rbx = settings.get_bool("General", "EnableMultiRbx");
    let async_join = settings.get_bool("General", "AsyncJoin");
    let is_teleport = settings.get_bool("Developer", "IsTeleport");
    let configured_old_join = settings.get_bool("Developer", "UseOldJoin");
    let auto_close_last_process = settings.get_bool("General", "AutoCloseLastProcess");
    let auto_close_multi_conflicts = settings.get_bool("General", "AutoCloseRobloxForMultiRbx");
    let start_minimized = settings.get_bool("General", "StartRobloxMinimized");
    let tracker = windows::tracker();
    tracker.reset_launch_cancelled();

    // A fila é por execução: este lote substitui o anterior.
    launch_queue_start(&app, &user_ids, place_id, &job_id);

    let isolation_report = match run_pre_launch_isolation(&app, &settings).await {
        Ok(value) => value,
        Err(err) => {
            launch_queue_abort(&app, None, &err);
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
            launch_queue_abort(&app, None, &err);
            return Err(err);
        }
    };

    for (i, &uid) in user_ids.iter().enumerate() {
        if tracker.is_launch_cancelled() {
            // "Close All Roblox": nada do que sobrou vai ser tentado.
            launch_queue_cancel_remaining(&app);
            break;
        }

        // A conta pode ter sido cancelada (`cancel_account_launch` ou
        // `stop_launch_queue`) enquanto a fila andava: pular é só não fazer
        // nada por ela — nenhum cliente já aberto é tocado.
        if launch_queue_is_cancelled(uid) {
            continue;
        }
        launch_queue_mark(&app, uid, LaunchQueueState::Launching, None);

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

        let (acct_base_path, acct_version_id) = match windows::resolve_roblox_install_path(
            acct_version_override.as_deref(),
            &settings,
            &versions,
        ) {
            Ok(value) => value,
            Err(err) => {
                launch_queue_mark(&app, uid, LaunchQueueState::Failed, Some(err.clone()));
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
        if has_version_conflict(&running_keys, &acct_version_id) {
            launch_queue_mark(
                &app,
                uid,
                LaunchQueueState::Failed,
                Some("version-conflict".to_string()),
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
                launch_queue_abort(&app, Some(uid), &err);
                return Err(err);
            }
        } else {
            let _ = windows::disable_multi_roblox();
        }

        windows::refresh_production_version().await;
        patch_client_settings_for_launch(&settings, LaunchClientProfile::Normal);

        if auto_close_last_process && tracker.get_pid(uid).is_some() {
            let closed = tracker.kill_for_user_graceful_async(uid, 4500).await;
            if !closed {
                launch_queue_mark(
                    &app,
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
                launch_queue_abort(&app, Some(uid), &err);
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
                launch_queue_mark(&app, uid, LaunchQueueState::Failed, Some(e));
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
                launch_queue_mark(&app, uid, LaunchQueueState::Failed, Some(err));
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        // "Close All Roblox" may have been clicked while this account was
        // fetching its ticket; don't spawn a client right after the kill.
        if tracker.is_launch_cancelled() {
            // Esta conta não chegou a abrir cliente, então vira `cancelled`
            // (e não `failed`); o resto da fila também.
            launch_queue_mark(&app, uid, LaunchQueueState::Cancelled, None);
            launch_queue_cancel_remaining(&app);
            break;
        }

        let pids_before = windows::get_roblox_pids();

        let acct_pid_wait_secs = pid_wait_seconds(acct_isolation_wipes_install);
        let acct_pending_id = tracker.add_pending_launch(
            uid,
            acct_version_id.clone(),
            std::time::Duration::from_secs(acct_pid_wait_secs + 30),
        );

        let launch_result = if acct_use_old_join {
            let base_path = if acct_version_id.is_none() {
                windows::default_player_dir(&acct_base_path).await
            } else {
                acct_base_path.clone()
            };
            windows::launch_old_join_from(
                &base_path,
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
            launch_queue_mark(&app, uid, LaunchQueueState::Failed, Some(err.clone()));
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
            launch_queue_mark(&app, uid, LaunchQueueState::Failed, Some("PID não detectado no tempo esperado".to_string()));
        }
        if let Some(pid) = acct_detected_pid {
            emit_launch_log(&app, uid, "success", "pid", format!("Cliente iniciado (PID {pid})"));
            launch_queue_mark(&app, uid, LaunchQueueState::Done, None);
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
            if start_minimized {
                let baseline = pids_before.clone();
                tokio::spawn(async move {
                    minimize_new_roblox_windows(baseline, std::time::Duration::from_secs(14)).await;
                });
            }
        }

        if i < user_ids.len() - 1 {
            if async_join {
                tracker.reset_next_account();
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
                while !tracker.is_next_account() && !tracker.is_launch_cancelled() {
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
                let wait = next_account_wait(delay, iter_start.elapsed(), jitter_ms);
                emit_launch_log(
                    &app,
                    uid,
                    "info",
                    "wait",
                    format!("Aguardando {}s antes da próxima conta (anti-captcha)", wait.as_secs()),
                );
                tokio::time::sleep(wait).await;
            }
        }
    }

    launch_queue_finish(&app);
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
        let tracker = macos::tracker();
        tracker.reset_launch_cancelled();

        // A fila é por execução: este lote substitui o anterior.
        launch_queue_start(&app, &user_ids, place_id, &job_id);

        for (i, &uid) in user_ids.iter().enumerate() {
            if tracker.is_launch_cancelled() {
                launch_queue_cancel_remaining(&app);
                break;
            }

            // Conta cancelada pela UI enquanto a fila andava: pular é só não
            // fazer nada por ela — nenhum cliente já aberto é tocado.
            if launch_queue_is_cancelled(uid) {
                continue;
            }
            launch_queue_mark(&app, uid, LaunchQueueState::Launching, None);

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
                        launch_queue_abort(&app, Some(uid), &err);
                        return Err(err);
                    }
                };
                if !enabled {
                    let err =
                        "Failed to enable Multi Roblox. Close all Roblox processes and try again."
                            .to_string();
                    launch_queue_abort(&app, Some(uid), &err);
                    return Err(err);
                }
            } else {
                let _ = macos::disable_multi_roblox();
            }

            patch_client_settings_for_launch(&settings, LaunchClientProfile::Normal);

            if auto_close_last_process && tracker.get_pid(uid).is_some() {
                tracker.kill_for_user(uid);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }

            let browser_tracker_id = match get_or_create_browser_tracker_id(&state, uid) {
                Ok(value) => value,
                Err(err) => {
                    launch_queue_abort(&app, Some(uid), &err);
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
                    launch_queue_mark(&app, uid, LaunchQueueState::Failed, Some(err));
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
                    launch_queue_mark(&app, uid, LaunchQueueState::Failed, Some(err));
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    continue;
                }
            };

            // Cancelado durante o auth: esta conta não chega a abrir cliente.
            if tracker.is_launch_cancelled() {
                launch_queue_mark(&app, uid, LaunchQueueState::Cancelled, None);
                launch_queue_cancel_remaining(&app);
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
                launch_queue_mark(&app, uid, LaunchQueueState::Failed, Some(err.clone()));
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }

            if let Some(pid) =
                wait_for_new_roblox_pid(&pids_before, std::time::Duration::from_secs(12)).await
            {
                tracker.track(uid, pid, browser_tracker_id);
                launch_queue_mark(&app, uid, LaunchQueueState::Done, None);
            } else {
                // Sem PID não dá para afirmar que a conta entrou.
                launch_queue_mark(&app, uid, LaunchQueueState::Failed, Some("PID não detectado no tempo esperado".to_string()));
            }

            if i < user_ids.len() - 1 {
                if async_join {
                    tracker.reset_next_account();
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
                    while !tracker.is_next_account() && !tracker.is_launch_cancelled() {
                        if std::time::Instant::now() > deadline {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                } else {
                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                }
            }
        }

        launch_queue_finish(&app);
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
    launch_queue_cancel_remaining(&app);
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

/// Arrange every open Roblox window into a grid across the selected monitors
/// (1-based indices; empty = all monitors).
#[tauri::command]
fn arrange_windows_grid(monitor_indices: Vec<usize>, gap: i32) -> Result<GridArrangeResult, String> {
    #[cfg(target_os = "windows")]
    {
        let (arranged, total) = platform::windows::arrange_roblox_grid(&monitor_indices, gap)?;
        return Ok(GridArrangeResult { arranged, total });
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (monitor_indices, gap);
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
                browser_tracker_id: p.browser_tracker_id,
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
