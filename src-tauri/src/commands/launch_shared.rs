/// Emit one structured line to the launch console (frontend listens on the
/// "launch-log" event). `level` is one of "info" | "success" | "warn" | "error";
/// `step` is a short machine code (start, isolation, auth, target, pid, spawn,
/// wait, done) the UI can use for coloring/icons.
pub(crate) fn emit_launch_log(
    app: &tauri::AppHandle,
    user_id: i64,
    level: &str,
    step: &str,
    message: impl Into<String>,
) {
    let _ = app.emit(
        "launch-log",
        serde_json::json!({
            "userId": user_id,
            "level": level,
            "step": step,
            "message": message.into(),
        }),
    );
}

/// Linha de console que nao pertence a uma conta: inicio/fim de uma sessao de
/// Botting, por exemplo. O frontend ja aceita `userId: null` e desenha "—";
/// passar `0` faria o console imprimir "0" no lugar do nome.
pub(crate) fn emit_session_log(
    app: &tauri::AppHandle,
    level: &str,
    step: &str,
    message: impl Into<String>,
) {
    let _ = app.emit(
        "launch-log",
        serde_json::json!({
            "userId": serde_json::Value::Null,
            "level": level,
            "step": step,
            "message": message.into(),
        }),
    );
}

/// Group name used to bucket accounts that failed to launch because Roblox
/// reports them as moderated/banned.
pub(crate) const MODERATED_GROUP: &str = "moderadas";

/// Returns true if an auth-ticket error string indicates the account is
/// moderated/banned (Roblox returns 403 with `"User is moderated"`).
pub(crate) fn is_moderated_error(err: &str) -> bool {
    let e = err.to_lowercase();
    e.contains("moderated") || e.contains("is banned") || e.contains("account has been")
}

/// Move an account into the "moderadas" group and persist it, then notify the
/// frontend so it can refresh and surface a toast. No-op if already grouped.
pub(crate) fn mark_account_moderated(store: &AccountStore, app: &tauri::AppHandle, user_id: i64) {
    if let Ok(accounts) = store.get_all() {
        if let Some(mut account) = accounts.into_iter().find(|a| a.user_id == user_id) {
            if account.group != MODERATED_GROUP {
                account.group = MODERATED_GROUP.to_string();
                let _ = store.update(account);
                let _ = app.emit(
                    "account-moderated",
                    serde_json::json!({ "userId": user_id, "group": MODERATED_GROUP }),
                );
                record_moderated_history(app, user_id);
            }
        }
    }
}

/// Quais grupos de campos de uma abertura vieram da exceção da conta (e não do
/// perfil global). É o que o registro de exceções do lado Windows guarda para
/// desfazer na próxima conta sem exceção (`OverrideLedger`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AccountSourced {
    pub fps: bool,
    pub volume: bool,
    pub graphics: bool,
    /// Tela cheia e/ou tamanho de janela.
    pub window: bool,
}

#[derive(Clone, Default)]
struct WindowsClientOverrides {
    max_fps: Option<u32>,
    master_volume: Option<f32>,
    graphics: Option<GraphicsQuality>,
    /// `Some(true)` abre em tela cheia, `Some(false)` em janela, `None` deixa
    /// como o Roblox tiver gravado.
    fullscreen: Option<bool>,
    window_size: Option<(u32, u32)>,
    fast_flags: Option<serde_json::Map<String, serde_json::Value>>,
    /// O que veio da exceção da conta.
    from_account: AccountSourced,
}

/// Nível de qualidade gráfica pedido ao cliente. Existe como enum porque
/// "automático" **não é** um nível: é o Roblox decidindo sozinho, gravado num
/// campo diferente do XML. Passar `0` como se fosse um nível fazia o valor
/// cair no `clamp(1, 10)` e virar 1 — a pior qualidade, não a automática.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GraphicsQuality {
    Automatic,
    /// Nível fixo de 1 a 10 (o `clamp` é feito na escrita do XML).
    Level(u32),
}

/// Exceções de cliente de **uma conta só**, por cima do perfil global.
///
/// Ficam em `Account.fields` (portanto no `AccountData.json`, junto de
/// `RobloxVersion`), e são aplicadas no instante em que aquela conta vai abrir.
///
/// Ressalva que precisa ficar escrita: `ClientAppSettings.json` é por pasta de
/// versão do Roblox e `GlobalBasicSettings_13.xml` é por usuário do Windows —
/// os dois são **globais**. "Por conta" funciona porque a fila de launch é
/// sequencial e o patch roda imediatamente antes de cada spawn; não é
/// isolamento de verdade. Um cliente aberto ainda pode reescrever o XML antes
/// de o novo o ler — por isso o tamanho da janela é conferido pelo PID depois
/// do spawn (`spawn_client_window_enforcement`).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct AccountClientOverrides {
    pub max_fps: Option<u32>,
    pub master_volume: Option<f32>,
    pub graphics: Option<GraphicsQuality>,
    pub fullscreen: Option<bool>,
    pub window_size: Option<(u32, u32)>,
    pub start_minimized: Option<bool>,
}

/// Chave de `Account.fields` que liga as exceções. Sem ela em `"true"`, os
/// outros campos são ignorados — dá para guardar uma configuração desligada.
pub(crate) const ACCOUNT_OVERRIDES_ENABLED_FIELD: &str = "ClientOverridesEnabled";

fn field_str<'a>(fields: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    fields
        .get(key)
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
}

fn field_bool(fields: &HashMap<String, String>, key: &str) -> Option<bool> {
    match field_str(fields, key)?.to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

fn field_u32(fields: &HashMap<String, String>, key: &str) -> Option<u32> {
    field_str(fields, key)?.parse::<u32>().ok().filter(|v| *v > 0)
}

/// Lê as exceções de uma conta. `None` quando o interruptor está desligado ou
/// quando nenhum campo tem valor útil — assim o resto do launch continua
/// falando "esta conta não tem exceção" em vez de aplicar um struct vazio.
pub(crate) fn account_client_overrides(
    fields: &HashMap<String, String>,
) -> Option<AccountClientOverrides> {
    if field_bool(fields, ACCOUNT_OVERRIDES_ENABLED_FIELD) != Some(true) {
        return None;
    }

    let graphics = field_str(fields, "ClientOverrideGraphics").and_then(|raw| {
        if raw.eq_ignore_ascii_case("auto") || raw.eq_ignore_ascii_case("automatic") {
            Some(GraphicsQuality::Automatic)
        } else {
            raw.parse::<u32>()
                .ok()
                .filter(|lvl| (1..=10).contains(lvl))
                .map(GraphicsQuality::Level)
        }
    });

    let master_volume = field_str(fields, "ClientOverrideVolume")
        .and_then(|raw| raw.parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .map(|v| v.clamp(0.0, 1.0));

    let width = field_u32(fields, "ClientOverrideWindowWidth");
    let height = field_u32(fields, "ClientOverrideWindowHeight");
    // Meia janela não é janela: largura sem altura é configuração incompleta,
    // e aplicar só uma das duas deixaria o cliente num tamanho que ninguém
    // pediu.
    let window_size = match (width, height) {
        (Some(w), Some(h)) => Some((w, h)),
        _ => None,
    };

    let overrides = AccountClientOverrides {
        max_fps: field_u32(fields, "ClientOverrideMaxFPS"),
        master_volume,
        graphics,
        fullscreen: field_bool(fields, "ClientOverrideFullscreen"),
        window_size,
        start_minimized: field_bool(fields, "ClientOverrideStartMinimized"),
    };

    if overrides == AccountClientOverrides::default() {
        return None;
    }
    Some(overrides)
}

// ── a janela do cliente, conferida pelo PID ──────────────────────────────
//
// O `StartScreenSize` do `GlobalBasicSettings_13.xml` é **um arquivo para todos
// os clientes**. O patch roda logo antes do spawn, mas um cliente já aberto
// (a conta principal com a exceção dela, por exemplo) pode reescrever o XML
// entre o patch e o instante em que o cliente novo o lê — e a alt abria com o
// tamanho da principal (03/10/2026). O XML continua sendo gravado (é o que faz
// o cliente já nascer no tamanho certo na maioria das vezes), mas quem garante
// é a conferência depois do spawn: achada a janela do PID novo, ela recebe o
// tamanho resolvido para ESTA conta.

/// O que o launch resolveu para a janela desta conta — a mesma coisa que foi
/// gravada no XML.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ClientWindowInputs {
    /// `Some(true)` = tela cheia (exceção da conta); `Some(false)` = em janela
    /// (exceção, ou o valor do jogador que o registro das exceções pôs de volta
    /// depois da tela cheia de outra conta).
    pub fullscreen: Option<bool>,
    /// O tamanho gravado no `StartScreenSize` (exceção da conta, senão o
    /// perfil global com `OverrideClientWindowSize`).
    pub window_size: Option<(u32, u32)>,
    /// A conta tem janela própria (exceção com tamanho ou tela cheia) — fica
    /// fora da grade (`account_keeps_own_window`).
    pub keeps_own_window: bool,
    pub start_minimized: bool,
    /// `General.AutoArrangeGrid`.
    pub auto_arrange_grid: bool,
    /// O retângulo que o Watcher salvou (`SaveWindowPositions`); só o launch
    /// avulso o restaura.
    pub saved_rect: Option<(i32, i32, i32, i32)>,
}

/// O que fazer com a janela quando ela aparecer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ClientWindowPlan {
    /// Tamanho a impor, no mesmo sentido do `StartScreenSize`.
    pub size: Option<(u32, u32)>,
    /// Pôr a janela na primeira célula livre da grade.
    pub grid: bool,
    /// Posição (e tamanho, se `size` não vier) a restaurar.
    pub saved_rect: Option<(i32, i32, i32, i32)>,
    /// A conta abre em janela (tamanho no plano, ou `Fullscreen=false`
    /// resolvido): se a janela aparecer em tela cheia, essa tela cheia veio do
    /// XML regravado por outro cliente, e é tirada — uma vez.
    pub leave_fullscreen: bool,
}

impl ClientWindowPlan {
    pub fn is_noop(&self) -> bool {
        self.size.is_none() && !self.grid && self.saved_rect.is_none() && !self.leave_fullscreen
    }
}

/// O piso que `rewrite_global_basic_settings` aplica ao `StartScreenSize`: o
/// tamanho imposto na janela é o mesmo que o XML recebeu.
const MIN_CLIENT_WINDOW: (u32, u32) = (320, 240);

pub(crate) fn client_window_plan(inputs: ClientWindowInputs) -> ClientWindowPlan {
    // Tela cheia e começar minimizado são pedidos do usuário que mexer na
    // janela desfaria.
    if inputs.start_minimized || inputs.fullscreen == Some(true) {
        return ClientWindowPlan::default();
    }
    // Conta com janela própria fica fora da grade, com o tamanho e a posição
    // dela; para as outras, a grade vence a posição salva pelo Watcher.
    let grid = inputs.auto_arrange_grid && !inputs.keeps_own_window;
    ClientWindowPlan {
        size: inputs
            .window_size
            .map(|(w, h)| (w.max(MIN_CLIENT_WINDOW.0), h.max(MIN_CLIENT_WINDOW.1))),
        grid,
        saved_rect: if grid { None } else { inputs.saved_rect },
        // Com tamanho no plano a conta é "em janela" por definição (o XML
        // grava `Fullscreen=false` junto do `StartScreenSize`). Sem tamanho e
        // sem `Some(false)`, a tela cheia é preferência do próprio jogador.
        leave_fullscreen: inputs.window_size.is_some() || inputs.fullscreen == Some(false),
    }
}

/// A conta tem uma janela que é dela: exceção de launch com tamanho próprio
/// ou com tela cheia. Essas ficam fora da grade (automática e manual) — a
/// grade encolheria a conta principal ao tamanho das alts.
pub(crate) fn account_keeps_own_window(fields: &HashMap<String, String>) -> bool {
    account_client_overrides(fields)
        .map(|o| o.keeps_own_window())
        .unwrap_or(false)
}

impl AccountClientOverrides {
    /// Ver `account_keeps_own_window`.
    pub(crate) fn keeps_own_window(&self) -> bool {
        self.window_size.is_some() || self.fullscreen == Some(true)
    }
}

/// Os PIDs rastreados cujas contas têm janela própria. `tracked` são pares
/// `(user_id, pid)`; conta que não está mais na lista entra na grade.
pub(crate) fn pids_keeping_own_window(
    tracked: &[(i64, u32)],
    fields_by_user: &HashMap<i64, HashMap<String, String>>,
) -> HashSet<u32> {
    tracked
        .iter()
        .filter(|(user_id, _)| {
            fields_by_user
                .get(user_id)
                .map(account_keeps_own_window)
                .unwrap_or(false)
        })
        .map(|(_, pid)| *pid)
        .collect()
}

/// `General.AutoArrangeGrid`, ligado por padrão (também quando a chave sumiu
/// do INI).
pub(crate) fn auto_arrange_grid_enabled(settings: &SettingsStore) -> bool {
    settings
        .get("General", "AutoArrangeGrid")
        .ok()
        .flatten()
        .map(|v| v != "false")
        .unwrap_or(true)
}

/// Monitores (índices de 1 em diante; vazio = todos) e gap da grade — os
/// mesmos da aba Windows da Choose Game (`GridMonitors`, `GridGap`).
pub(crate) fn grid_layout_settings(settings: &SettingsStore) -> (Vec<usize>, i32) {
    let monitors = settings
        .get_string("General", "GridMonitors")
        .split(',')
        .filter_map(|s| s.trim().parse::<usize>().ok())
        .filter(|i| *i > 0)
        .collect();
    let gap = settings
        .get_int("General", "GridGap")
        .unwrap_or(20)
        .clamp(0, 200) as i32;
    (monitors, gap)
}

/// Célula menor que o mínimo do Roblox (`General.GridAllowSmallWindows`) e
/// janelas sem moldura (`General.GridBorderless`) — as duas desligadas por
/// padrão, só para clientes que o app abriu. Ver docs/features/performance.md.
#[cfg(target_os = "windows")]
pub(crate) fn grid_window_style(settings: &SettingsStore) -> platform::windows::GridWindowStyle {
    platform::windows::GridWindowStyle {
        allow_small: settings.get_bool("General", "GridAllowSmallWindows"),
        borderless: settings.get_bool("General", "GridBorderless"),
    }
}

/// O tamanho de janela do perfil global (`OverrideClientWindowSize`), ou
/// `None` com ele desligado.
pub(crate) fn global_window_size(
    settings: &SettingsStore,
    profile: LaunchClientProfile,
) -> Option<(u32, u32)> {
    let override_window_key = profile_key(
        profile,
        "OverrideClientWindowSize",
        "BottingPlayerOverrideClientWindowSize",
        "BottingBotOverrideClientWindowSize",
    );
    let window_width_key = profile_key(
        profile,
        "ClientWindowWidth",
        "BottingPlayerClientWindowWidth",
        "BottingBotClientWindowWidth",
    );
    let window_height_key = profile_key(
        profile,
        "ClientWindowHeight",
        "BottingPlayerClientWindowHeight",
        "BottingBotClientWindowHeight",
    );

    if !settings.get_bool("General", override_window_key) {
        return None;
    }
    let w = settings.get_int("General", window_width_key).unwrap_or(1280);
    let h = settings.get_int("General", window_height_key).unwrap_or(720);
    (w > 0 && h > 0).then_some((w as u32, h as u32))
}

/// Os PIDs dos clientes rastreados cujas contas têm janela própria — os que a
/// grade não pode mexer nem contar como ocupando célula.
#[cfg(target_os = "windows")]
pub(crate) fn grid_excluded_pids(accounts: &AccountStore) -> HashSet<u32> {
    let tracked: Vec<(i64, u32)> = platform::windows::tracker()
        .get_all()
        .into_iter()
        .map(|p| (p.user_id, p.pid))
        .collect();
    let fields_by_user: HashMap<i64, HashMap<String, String>> = accounts
        .get_all()
        .unwrap_or_default()
        .into_iter()
        .map(|a| (a.user_id, a.fields))
        .collect();
    pids_keeping_own_window(&tracked, &fields_by_user)
}

/// O que o patch resolveu para a janela, devolvido para a conferência depois
/// do spawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ResolvedClientWindow {
    pub fullscreen: Option<bool>,
    pub window_size: Option<(u32, u32)>,
}

/// Espera a janela principal do PID aparecer e aplica o plano, sem prender o
/// launch (roda numa task à parte). A janela é procurada por até 45 s — o
/// mesmo teto da restauração de posição do Watcher.
///
/// Com `plan.grid`, a janela vai para a primeira célula livre da grade
/// (monitores, gap e as janelas fora da grade são lidos na hora de pôr, não no
/// launch: a fila pode levar minutos).
#[cfg(target_os = "windows")]
pub(crate) fn spawn_client_window_enforcement(
    app: &tauri::AppHandle,
    pid: u32,
    plan: ClientWindowPlan,
) {
    if plan.is_noop() {
        return;
    }
    let app = app.clone();
    tokio::spawn(async move {
        use platform::windows;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
        let found = loop {
            if windows::find_main_window(pid).is_some() {
                break true;
            }
            if std::time::Instant::now() >= deadline || !windows::is_roblox_pid_running(pid) {
                break false;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        };
        if !found {
            return;
        }
        // O cliente ainda se dimensiona logo depois de mostrar a janela; medir
        // antes disso confunde a borda com o tamanho.
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        // Uma aplicação e duas conferências: se o cliente redimensionar a
        // janela por conta própria logo depois, ela volta ao lugar escolhido
        // (a célula não é escolhida de novo).
        let mut chosen: Option<(i32, i32, i32, i32)> = None;
        // A tela cheia herdada é tirada uma vez só (`leave_fullscreen`): se o
        // Roblox voltar para ela, a conferência seguinte a vê e para.
        let mut fullscreen_exit_tried = false;
        // Lido uma vez: a conferência segura a célula escolhida com os mesmos
        // flags com que ela foi posta.
        let grid_style = plan
            .grid
            .then(|| grid_window_style(&app.state::<SettingsStore>()))
            .unwrap_or_default();
        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
            }
            let Some(hwnd) = windows::find_main_window(pid) else {
                return;
            };
            let mut first_look = chosen.is_none();
            let mode = windows::window_mode_of(hwnd).unwrap_or(windows::WindowMode::Normal);
            if windows::should_leave_fullscreen(plan.leave_fullscreen, mode, fullscreen_exit_tried) {
                match windows::leave_fullscreen(hwnd, plan.size) {
                    windows::FullscreenExit::NotFullscreen => {}
                    windows::FullscreenExit::Left(_) => {
                        fullscreen_exit_tried = true;
                        first_look = false;
                        eprintln!("[window] PID {pid}: tela cheia herdada desfeita");
                    }
                    windows::FullscreenExit::Refused => {
                        fullscreen_exit_tried = true;
                        eprintln!("[window] PID {pid}: não foi possível sair da tela cheia");
                    }
                }
            }
            let outcome = match chosen {
                Some(rect) => windows::hold_window_rect(hwnd, rect, grid_style.allow_small),
                None if plan.grid => {
                    let request = {
                        let settings = app.state::<SettingsStore>();
                        let (monitor_indices, gap) = grid_layout_settings(&settings);
                        windows::GridRequest {
                            monitor_indices,
                            gap,
                            excluded_pids: grid_excluded_pids(app.state::<AccountStore>().inner()),
                            style: grid_style,
                        }
                    };
                    windows::place_in_grid(hwnd, pid, plan.size, &request, first_look)
                }
                None => windows::enforce_client_window(hwnd, plan.size, plan.saved_rect, first_look),
            };
            match outcome {
                windows::WindowEnforcement::Skipped => return,
                windows::WindowEnforcement::Placed(rect) => chosen = Some(rect),
            }
        }
    });
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum LaunchClientProfile {
    Normal,
    BottingPlayer,
    BottingBot,
}

pub(crate) fn profile_key(
    profile: LaunchClientProfile,
    normal: &'static str,
    player: &'static str,
    bot: &'static str,
) -> &'static str {
    match profile {
        LaunchClientProfile::Normal => normal,
        LaunchClientProfile::BottingPlayer => player,
        LaunchClientProfile::BottingBot => bot,
    }
}

pub(crate) fn effective_launch_profile(
    settings: &SettingsStore,
    profile: LaunchClientProfile,
) -> LaunchClientProfile {
    if matches!(profile, LaunchClientProfile::Normal) || !botting_uses_shared_client_profile(settings)
    {
        profile
    } else {
        LaunchClientProfile::Normal
    }
}

fn custom_client_settings_path(settings: &SettingsStore, profile: LaunchClientProfile) -> String {
    let key = profile_key(
        profile,
        "CustomClientSettings",
        "BottingPlayerCustomClientSettings",
        "BottingBotCustomClientSettings",
    );
    settings.get_string("General", key)
}

pub(crate) fn start_minimized_for_profile(
    settings: &SettingsStore,
    profile: LaunchClientProfile,
) -> bool {
    let key = profile_key(
        profile,
        "StartRobloxMinimized",
        "BottingPlayerStartRobloxMinimized",
        "BottingBotStartRobloxMinimized",
    );
    settings.get_bool("General", key)
}

pub(crate) fn botting_uses_shared_client_profile(settings: &SettingsStore) -> bool {
    settings
        .get("General", "BottingUseSharedClientProfile")
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(true)
}

#[cfg(target_os = "windows")]
fn windows_client_overrides(
    settings: &SettingsStore,
    allow_fps_override: bool,
    profile: LaunchClientProfile,
    account: Option<&AccountClientOverrides>,
) -> WindowsClientOverrides {
    let unlock_fps_key = profile_key(
        profile,
        "UnlockFPS",
        "BottingPlayerUnlockFPS",
        "BottingBotUnlockFPS",
    );
    let max_fps_key = profile_key(
        profile,
        "MaxFPSValue",
        "BottingPlayerMaxFPSValue",
        "BottingBotMaxFPSValue",
    );

    let max_fps = if allow_fps_override && settings.get_bool("General", unlock_fps_key) {
        let fps = settings.get_int("General", max_fps_key).unwrap_or(120);
        if fps > 0 {
            Some(fps as u32)
        } else {
            None
        }
    } else {
        None
    };

    let override_volume_key = profile_key(
        profile,
        "OverrideClientVolume",
        "BottingPlayerOverrideClientVolume",
        "BottingBotOverrideClientVolume",
    );
    let client_volume_key = profile_key(
        profile,
        "ClientVolume",
        "BottingPlayerClientVolume",
        "BottingBotClientVolume",
    );

    let master_volume = if settings.get_bool("General", override_volume_key) {
        Some(
            settings
                .get_float("General", client_volume_key)
                .unwrap_or(0.5)
                .clamp(0.0, 1.0) as f32,
        )
    } else {
        None
    };

    let override_graphics_key = profile_key(
        profile,
        "OverrideClientGraphics",
        "BottingPlayerOverrideClientGraphics",
        "BottingBotOverrideClientGraphics",
    );
    let graphics_level_key = profile_key(
        profile,
        "ClientGraphicsLevel",
        "BottingPlayerClientGraphicsLevel",
        "BottingBotClientGraphicsLevel",
    );

    let graphics = if settings.get_bool("General", override_graphics_key) {
        let lvl = settings
            .get_int("General", graphics_level_key)
            .unwrap_or(10);
        if lvl > 0 {
            Some(GraphicsQuality::Level(lvl.clamp(1, 10) as u32))
        } else {
            None
        }
    } else {
        None
    };

    let window_size = global_window_size(settings, profile);

    let optimization_profile = platform::windows::load_optimization_profile(settings, profile);
    let fast_flags = if optimization_profile.experimental.enable_fast_flags {
        match platform::windows::parse_allowlisted_fast_flags_json(
            &optimization_profile.experimental.fast_flags_json,
        ) {
            Ok(flags) => Some(flags),
            Err(err) => {
                eprintln!("Skipped allowlisted fast flags for {:?}: {}", profile, err);
                None
            }
        }
    } else {
        None
    };

    // A conta entra **por cima**: cada campo que ela define substitui o global,
    // e o que ela deixa vazio continua vindo do perfil. O FPS respeita o
    // `allow_fps_override` igual ao global — quando o usuário aponta um
    // ClientAppSettings.json próprio, ninguém mexe no FPS dele, nem a exceção.
    let mut resolved = WindowsClientOverrides {
        max_fps,
        master_volume,
        graphics,
        fullscreen: None,
        window_size,
        fast_flags,
        from_account: AccountSourced::default(),
    };

    if let Some(acc) = account {
        if allow_fps_override {
            if let Some(fps) = acc.max_fps {
                resolved.max_fps = Some(fps);
                resolved.from_account.fps = true;
            }
        }
        if let Some(volume) = acc.master_volume {
            resolved.master_volume = Some(volume);
            resolved.from_account.volume = true;
        }
        if let Some(graphics) = acc.graphics {
            resolved.graphics = Some(graphics);
            resolved.from_account.graphics = true;
        }
        resolved.from_account.window = acc.fullscreen.is_some() || acc.window_size.is_some();
        if let Some(fullscreen) = acc.fullscreen {
            resolved.fullscreen = Some(fullscreen);
            // Tela cheia com um tamanho de janela ao lado é contraditório: o XML
            // grava `Fullscreen=false` junto de `StartScreenSize`. Quem pediu
            // tela cheia e não pediu tamanho fica só com a tela cheia.
            if fullscreen && acc.window_size.is_none() {
                resolved.window_size = None;
            }
        }
        if let Some(size) = acc.window_size {
            resolved.window_size = Some(size);
        }
    }

    resolved
}

/// `base_path`: pasta da versão do Roblox que vai realmente abrir (`None` =
/// build padrão/produção, usada pelo servidor HTTP local, que não tem conta no
/// contexto). Sem isso, `ClientAppSettings.json` era sempre escrito na pasta de
/// produção mesmo quando a conta abre numa versão do catálogo — o cliente que
/// de fato abre nunca lia o FPS/volume/qualidade/fullscreen/fast flags
/// aplicados.
#[cfg(target_os = "windows")]
pub(crate) fn patch_client_settings_for_launch(
    settings: &SettingsStore,
    profile: LaunchClientProfile,
    account: Option<&AccountClientOverrides>,
    base_path: Option<&str>,
) -> ResolvedClientWindow {
    use platform::windows;

    let effective_profile = effective_launch_profile(settings, profile);
    let custom_settings = custom_client_settings_path(settings, effective_profile);
    let custom_settings = custom_settings.trim();
    let mut custom_applied = false;

    // Legacy behavior: custom settings file overrides FPS unlock when valid.
    if !custom_settings.is_empty()
        && std::path::Path::new(custom_settings).exists()
        && windows::copy_custom_client_settings(base_path, custom_settings).is_ok()
    {
        custom_applied = true;
    }

    let mut overrides =
        windows_client_overrides(settings, !custom_applied, effective_profile, account);
    if custom_applied {
        overrides.fast_flags = None;
    }
    let fullscreen_from_ledger = windows::apply_runtime_client_settings(
        base_path,
        overrides.max_fps,
        overrides.master_volume,
        overrides.graphics,
        overrides.fullscreen,
        overrides.window_size,
        overrides.fast_flags.as_ref(),
        overrides.from_account,
    )
    .ok()
    .flatten();
    ResolvedClientWindow {
        // Sem pedido desta conta nem do perfil global, vale o que o registro
        // pôs de volta depois da exceção de outra conta (a tela cheia da
        // principal, 03/10/2026) — é ele que diz "esta alt é em janela".
        fullscreen: overrides.fullscreen.or(fullscreen_from_ledger),
        window_size: overrides.window_size,
    }
}

#[cfg(target_os = "macos")]
fn fps_unlock_target(settings: &SettingsStore, profile: LaunchClientProfile) -> Option<u32> {
    let unlock_fps_key = profile_key(
        profile,
        "UnlockFPS",
        "BottingPlayerUnlockFPS",
        "BottingBotUnlockFPS",
    );
    let max_fps_key = profile_key(
        profile,
        "MaxFPSValue",
        "BottingPlayerMaxFPSValue",
        "BottingBotMaxFPSValue",
    );

    if !settings.get_bool("General", unlock_fps_key) {
        return None;
    }
    settings
        .get_int("General", max_fps_key)
        .filter(|fps| *fps > 0)
        .map(|fps| fps as u32)
}

#[cfg(target_os = "macos")]
fn patch_client_settings_for_launch(
    settings: &SettingsStore,
    profile: LaunchClientProfile,
    account: Option<&AccountClientOverrides>,
    base_path: Option<&str>,
) {
    use platform::macos;

    // macOS não tem catálogo de versões instaladas (ver docs/features/launch.md);
    // o parâmetro existe só para manter a mesma assinatura do lado Windows.
    let _ = base_path;

    let custom_settings = custom_client_settings_path(settings, profile);
    let custom_settings = custom_settings.trim();

    // Keep the same override precedence as Windows.
    if !custom_settings.is_empty()
        && std::path::Path::new(custom_settings).exists()
        && macos::copy_custom_client_settings(custom_settings).is_ok()
    {
        return;
    }

    // No macOS só o FPS é aplicável hoje; as outras exceções por conta dependem
    // do `GlobalBasicSettings_13.xml`, que é do lado Windows.
    let fps = account
        .and_then(|a| a.max_fps)
        .or_else(|| fps_unlock_target(settings, profile));
    if let Some(fps) = fps {
        let _ = macos::apply_fps_unlock(fps);
    }
}

fn save_browser_tracker_id(
    state: &AccountStore,
    user_id: i64,
    browser_tracker_id: &str,
) -> Result<(), String> {
    let accounts = state.get_all()?;
    if let Some(mut account) = accounts.into_iter().find(|a| a.user_id == user_id) {
        account.browser_tracker_id = browser_tracker_id.to_string();
        state.update(account)?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub(crate) fn get_or_create_browser_tracker_id(
    state: &AccountStore,
    user_id: i64,
) -> Result<String, String> {
    let accounts = state.get_all()?;
    if let Some(existing) = accounts
        .iter()
        .find(|a| a.user_id == user_id)
        .map(|a| a.browser_tracker_id.trim().to_string())
        .filter(|id| !id.is_empty())
    {
        return Ok(existing);
    }

    let generated = platform::windows::generate_browser_tracker_id();
    save_browser_tracker_id(state, user_id, &generated)?;
    Ok(generated)
}

#[cfg(target_os = "macos")]
fn get_or_create_browser_tracker_id(state: &AccountStore, user_id: i64) -> Result<String, String> {
    let accounts = state.get_all()?;
    if let Some(existing) = accounts
        .iter()
        .find(|a| a.user_id == user_id)
        .map(|a| a.browser_tracker_id.trim().to_string())
        .filter(|id| !id.is_empty())
    {
        return Ok(existing);
    }

    let generated = platform::macos::generate_browser_tracker_id();
    save_browser_tracker_id(state, user_id, &generated)?;
    Ok(generated)
}

#[cfg(target_os = "windows")]
pub(crate) async fn wait_for_new_roblox_pid(
    pids_before: &[u32],
    timeout: std::time::Duration,
) -> Option<u32> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let pids_after = platform::windows::get_roblox_pids();
        if let Some(pid) = pids_after
            .iter()
            .find(|p| !pids_before.contains(p))
            .copied()
        {
            return Some(pid);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }
}

#[cfg(target_os = "macos")]
async fn wait_for_new_roblox_pid(pids_before: &[u32], timeout: std::time::Duration) -> Option<u32> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let pids_after = platform::macos::get_roblox_pids();
        if let Some(pid) = pids_after
            .iter()
            .find(|p| !pids_before.contains(p))
            .copied()
        {
            return Some(pid);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }
}

#[derive(Debug, Clone, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct BottingAccountStatusPayload {
    user_id: i64,
    is_player: bool,
    disconnected: bool,
    phase: String,
    retry_count: u32,
    next_restart_at_ms: Option<i64>,
    player_grace_until_ms: Option<i64>,
    last_error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct BottingStatusPayload {
    active: bool,
    started_at_ms: Option<i64>,
    place_id: i64,
    job_id: String,
    launch_data: String,
    interval_minutes: i64,
    launch_delay_seconds: i64,
    player_grace_minutes: i64,
    player_user_ids: Vec<i64>,
    user_ids: Vec<i64>,
    accounts: Vec<BottingAccountStatusPayload>,
}

#[cfg(target_os = "windows")]
async fn minimize_new_roblox_windows(pids_before: Vec<u32>, timeout: std::time::Duration) {
    let deadline = std::time::Instant::now() + timeout;
    let mut minimized: HashSet<u32> = HashSet::new();
    loop {
        for pid in platform::windows::get_roblox_pids() {
            if pids_before.contains(&pid) || minimized.contains(&pid) {
                continue;
            }
            if let Some(hwnd) = platform::windows::find_main_window(pid) {
                let _ = platform::windows::minimize_window(hwnd);
                minimized.insert(pid);
            }
        }

        if std::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
}

#[cfg(target_os = "windows")]
pub(crate) async fn apply_windows_post_launch_profile(
    app: Option<&tauri::AppHandle>,
    settings: &SettingsStore,
    profile: LaunchClientProfile,
    pid: u32,
) {
    let effective_profile = effective_launch_profile(settings, profile);
    // A otimização que segue o foco tira daqui a política de fundo deste
    // cliente e o estado a devolver quando ela desliga.
    platform::windows::remember_launch_profile(pid, effective_profile);
    let follow_focus = platform::windows::focus_follow_enabled(settings);
    let optimization_profile = platform::windows::launch_profile_under_focus_follow(
        platform::windows::load_optimization_profile(settings, effective_profile),
        follow_focus,
    );
    let has_process_policy = optimization_profile.process.enabled;
    let has_job_limits = optimization_profile.experimental.enable_job_cpu_limit
        || optimization_profile.experimental.enable_job_memory_limit;
    if !has_process_policy && !has_job_limits {
        return;
    }
    if has_process_policy && optimization_profile.process.delay_ms > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(
            optimization_profile.process.delay_ms,
        ))
        .await;
    }

    let applied = platform::windows::apply_optimization_to_pid(pid, &optimization_profile);
    if follow_focus {
        // O Job acabou de nascer com o teto ligado: o laço reaplica a
        // velocidade (o cliente novo está na carência, sem teto).
        platform::windows::focus_follow_forget_applied(pid);
    }
    if let Err(err) = applied {
        eprintln!("Failed to apply Windows optimization to pid {}: {}", pid, err);
        if let Some(app) = app {
            let _ = app.emit(
                "roblox-optimization-warning",
                serde_json::json!({
                    "pid": pid,
                    "message": err,
                }),
            );
        }
    }
}

#[cfg(target_os = "windows")]
async fn ensure_multi_roblox_enabled(auto_close_conflicts: bool) -> Result<(), String> {
    let enabled = platform::windows::enable_multi_roblox()?;
    if enabled {
        return Ok(());
    }

    let roblox_pids = platform::windows::get_roblox_pids();
    let legacy_pids = platform::windows::find_legacy_ram_pids();

    if !roblox_pids.is_empty() {
        if auto_close_conflicts {
            let killed = platform::windows::kill_all_roblox();
            if killed > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(700)).await;
            }
            let _ = platform::windows::tracker().cleanup_dead_processes();
            let enabled_after = platform::windows::enable_multi_roblox()?;
            if enabled_after {
                return Ok(());
            }
        }
        return Err(
            "A Roblox client is already running. Close it or enable Auto-close Roblox for Multi-Roblox in settings.".into(),
        );
    }

    if !legacy_pids.is_empty() {
        return Err(
            "The legacy Roblox Account Manager is running and holds the Roblox singleton mutex. Close it before launching from this app.".into(),
        );
    }

    platform::windows::release_multi_roblox_handle();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let enabled_retry = platform::windows::enable_multi_roblox()?;
    if enabled_retry {
        return Ok(());
    }

    Err(
        "Could not acquire the Roblox singleton mutex. Another program may be holding it. Close any Roblox-related tools and try again.".into(),
    )
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone)]
struct BottingConfig {
    user_ids: Vec<i64>,
    place_id: i64,
    job_id: String,
    launch_data: String,
    player_user_ids: HashSet<i64>,
    interval_minutes: u64,
    launch_delay_seconds: u64,
    retry_max: u32,
    retry_base_seconds: u64,
    player_grace_minutes: u64,
    /// A sessao foi aberta sobre contas que **ja estavam em jogo**: na primeira
    /// passagem elas nao sao fechadas nem relancadas, so entram no ciclo.
    adopt_running: bool,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone)]
struct BottingAccountRuntime {
    user_id: i64,
    is_player: bool,
    disconnected: bool,
    manual_restart_pending: bool,
    manual_restart_keep_schedule: bool,
    manual_restart_saved_next_restart_at_ms: Option<i64>,
    phase: &'static str,
    retry_count: u32,
    next_restart_at_ms: Option<i64>,
    player_grace_until_ms: Option<i64>,
    last_error: Option<String>,
}

#[cfg(target_os = "windows")]
#[derive(Clone)]
struct BottingSession {
    id: u64,
    stop_flag: Arc<AtomicBool>,
    stopped_notify: Arc<tokio::sync::Notify>,
    started_at_ms: i64,
    config: Arc<Mutex<BottingConfig>>,
    accounts: Arc<Mutex<HashMap<i64, BottingAccountRuntime>>>,
}

#[cfg(target_os = "windows")]
struct BottingManager {
    session: Mutex<Option<BottingSession>>,
    next_id: AtomicU64,
}

#[cfg(target_os = "windows")]
impl BottingManager {
    fn new() -> Self {
        Self {
            session: Mutex::new(None),
            next_id: AtomicU64::new(1),
        }
    }

    fn next_session_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    fn get_session(&self) -> Option<BottingSession> {
        self.session.lock().ok().and_then(|s| s.as_ref().cloned())
    }

    fn replace_session(&self, session: Option<BottingSession>) {
        if let Ok(mut guard) = self.session.lock() {
            *guard = session;
        }
    }
}

#[cfg(target_os = "windows")]
static BOTTING_MANAGER: LazyLock<BottingManager> = LazyLock::new(BottingManager::new);

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[derive(Debug, Clone, Default)]
struct ResolvedLaunchJob {
    job_id: String,
    join_vip: bool,
    link_code: String,
}

fn decode_url_component(value: &str) -> String {
    urlencoding::decode(value)
        .map(|v| v.into_owned())
        .unwrap_or_else(|_| value.to_string())
}

fn extract_query_param_value(input: &str, key: &str) -> Option<String> {
    for part in input.split(['?', '&']) {
        let pair = part.split('#').next().unwrap_or(part);
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        if !k.eq_ignore_ascii_case(key) {
            continue;
        }

        let decoded = decode_url_component(v.trim());
        let value = decoded.trim();
        if value.is_empty()
            || value.eq_ignore_ascii_case("null")
            || value.eq_ignore_ascii_case("undefined")
        {
            continue;
        }

        return Some(value.to_string());
    }
    None
}

fn extract_query_param_value_recursive(input: &str, key: &str) -> Option<String> {
    if let Some(value) = extract_query_param_value(input, key) {
        return Some(value);
    }

    let decoded = decode_url_component(input);
    if decoded != input {
        return extract_query_param_value(&decoded, key);
    }

    None
}

fn strip_ascii_prefix<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    let head = value.get(..prefix.len())?;
    if !head.eq_ignore_ascii_case(prefix) {
        return None;
    }
    value.get(prefix.len()..)
}

fn looks_like_share_link(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("/share?")
        || lower.contains("/share-links")
        || lower.contains("navigation/share_links")
        || lower.contains("type=server")
        || lower.contains("pid=server")
}

fn extract_private_server_link_code(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(rest) = strip_ascii_prefix(trimmed, "vip:") {
        let decoded = decode_url_component(rest.trim());
        let code = decoded.trim();
        if !code.is_empty() {
            return Some(code.to_string());
        }
    }

    if let Some(code) = extract_query_param_value_recursive(trimmed, "privateServerLinkCode") {
        return Some(code);
    }
    if let Some(code) = extract_query_param_value_recursive(trimmed, "linkCode") {
        return Some(code);
    }

    let starts_with_code = trimmed
        .get(..5)
        .map(|head| head.eq_ignore_ascii_case("code="))
        .unwrap_or(false);
    if looks_like_share_link(trimmed) || starts_with_code {
        if let Some(code) = extract_query_param_value_recursive(trimmed, "code") {
            return Some(code);
        }
    }

    None
}

fn resolve_launch_job(
    raw_job_id: &str,
    explicit_join_vip: bool,
    explicit_link_code: &str,
) -> ResolvedLaunchJob {
    let trimmed_job = raw_job_id.trim();
    let mut job_id = trimmed_job.to_string();

    let mut link_code = extract_private_server_link_code(explicit_link_code).unwrap_or_else(|| {
        decode_url_component(explicit_link_code.trim())
            .trim()
            .to_string()
    });

    let mut join_vip = explicit_join_vip;
    if let Some(rest) = strip_ascii_prefix(trimmed_job, "vip:") {
        join_vip = true;
        job_id = rest.trim().to_string();
    }

    if link_code.is_empty() {
        if let Some(code) = extract_private_server_link_code(trimmed_job) {
            link_code = code;
        }
    }

    if join_vip && link_code.is_empty() {
        if job_id.trim().is_empty() {
            join_vip = false;
        } else {
            link_code = decode_url_component(job_id.trim()).trim().to_string();
        }
    }

    ResolvedLaunchJob {
        job_id,
        join_vip,
        link_code,
    }
}

fn looks_like_access_code(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }

    let parts: Vec<&str> = trimmed.split('-').collect();
    if parts.len() != 5 {
        return false;
    }

    parts
        .iter()
        .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

fn looks_like_share_link_code(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.len() == 32
        && trimmed.chars().any(|c| c.is_ascii_alphabetic())
        && trimmed.chars().all(|c| c.is_ascii_hexdigit())
}

fn extract_place_id_from_url(value: &str) -> Option<i64> {
    let lower = value.to_ascii_lowercase();
    let marker = "/games/";
    let start = lower.find(marker)? + marker.len();
    let rest = value.get(start..)?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<i64>().ok().filter(|id| *id > 0)
}

/// Picks a random public server for **this** account.
///
/// Cada conta chama este helper por conta própria: num launch múltiplo com
/// `shuffleJob` ligado, as contas acabam espalhadas por servidores diferentes
/// (a lista é buscada de novo e o índice é sorteado por conta), em vez de todas
/// caírem no mesmo servidor. Devolve `None` — e o chamador mantém o Job ID
/// vazio, entrando num servidor público qualquer — quando a listagem falha ou
/// vem vazia.
async fn pick_shuffled_public_job(
    accounts: &AccountStore,
    user_id: i64,
    place_id: i64,
) -> Option<String> {
    // Sem `run_with_session_retry` de propósito: o refresh dele chama
    // `signoutfromallsessionsandreauthenticate`, que derruba as sessões abertas
    // da conta. Sortear servidor é leitura opcional — se falhar, o launch segue
    // com o Job vazio (servidor público qualquer).
    let cookie = get_cookie(accounts, user_id).ok()?;
    let response = api::roblox::get_servers(place_id, "Public", None, Some(&cookie))
        .await
        .ok()?;

    if response.data.is_empty() {
        return None;
    }

    let index = shuffle_server_index(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        response.data.len(),
    );
    Some(response.data[index].id.clone())
}

#[derive(Debug, Clone)]
struct ResolvedPrivateJoin {
    place_id: i64,
    link_code: String,
    access_code: String,
    use_private_join: bool,
}

async fn resolve_private_join(
    cookie: &str,
    place_id: i64,
    launch: &ResolvedLaunchJob,
) -> Result<ResolvedPrivateJoin, String> {
    let mut resolved_place_id = place_id;
    let mut resolved_link_code = launch.link_code.trim().to_string();

    if !resolved_link_code.is_empty() {
        if let Some(url_place_id) = extract_place_id_from_url(&launch.job_id) {
            resolved_place_id = url_place_id;
        }
    }

    if !resolved_link_code.is_empty()
        && (looks_like_share_link(&launch.job_id)
            || looks_like_share_link_code(&resolved_link_code))
    {
        let (maybe_place_id, resolved_code) =
            api::roblox::resolve_share_server_link(cookie, &resolved_link_code).await?;
        if let Some(pid) = maybe_place_id {
            resolved_place_id = pid;
        }
        if !resolved_code.trim().is_empty() {
            resolved_link_code = resolved_code.trim().to_string();
        }
    }

    let mut access_code = String::new();
    if looks_like_access_code(&resolved_link_code) {
        access_code = resolved_link_code.clone();
        resolved_link_code.clear();
    }

    let use_private_join =
        launch.join_vip || !resolved_link_code.is_empty() || !access_code.is_empty();

    Ok(ResolvedPrivateJoin {
        place_id: resolved_place_id,
        link_code: resolved_link_code,
        access_code,
        use_private_join,
    })
}

#[cfg(target_os = "windows")]
fn backoff_delay_seconds(base: u64, retry_count: u32, retry_max: u32) -> u64 {
    let exp = retry_count.saturating_sub(1).min(retry_max.max(1));
    let scaled = base.saturating_mul(1_u64 << exp.min(12));
    scaled.clamp(5, 300)
}

#[cfg(target_os = "windows")]
async fn wait_for_launch_slot(
    last_launch_at: &mut Option<std::time::Instant>,
    launch_delay_seconds: u64,
) {
    if let Some(last) = *last_launch_at {
        let required_gap = std::time::Duration::from_secs(launch_delay_seconds);
        let elapsed = last.elapsed();
        if elapsed < required_gap {
            tokio::time::sleep(required_gap - elapsed).await;
        }
    }
    *last_launch_at = Some(std::time::Instant::now());
}

#[cfg(target_os = "windows")]
fn is_429_related_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("429")
        || lower.contains("too many requests")
        || lower.contains("authentifizierung fehlgeschlagen")
        || lower.contains("authentication failed")
}

#[cfg(target_os = "windows")]
fn title_looks_auth_failure(title: &str) -> bool {
    let t = title.to_lowercase();
    t.contains("authentifizierung fehlgeschlagen")
        || t.contains("authentication failed")
        || t.contains("fehlercode: 429")
        || t.contains("error code: 429")
}

#[cfg(target_os = "windows")]
async fn detect_auth_failure_window(pid: u32) -> bool {
    for _ in 0..20 {
        if let Some(hwnd) = platform::windows::find_main_window(pid) {
            let title = platform::windows::get_window_title(hwnd);
            if !title.is_empty() && title_looks_auth_failure(&title) {
                return true;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }
    false
}

#[cfg(target_os = "windows")]
fn botting_status_from_session(session: &BottingSession) -> BottingStatusPayload {
    let config = match session.config.lock() {
        Ok(c) => c.clone(),
        Err(_) => {
            return BottingStatusPayload {
                active: false,
                ..BottingStatusPayload::default()
            }
        }
    };
    let mut accounts: Vec<BottingAccountStatusPayload> = match session.accounts.lock() {
        Ok(map) => map
            .values()
            .map(|a| BottingAccountStatusPayload {
                user_id: a.user_id,
                is_player: a.is_player,
                disconnected: a.disconnected,
                phase: a.phase.to_string(),
                retry_count: a.retry_count,
                next_restart_at_ms: a.next_restart_at_ms,
                player_grace_until_ms: a.player_grace_until_ms,
                last_error: a.last_error.clone(),
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    accounts.sort_by_key(|a| a.user_id);
    let mut player_user_ids: Vec<i64> = config.player_user_ids.iter().copied().collect();
    player_user_ids.sort();
    BottingStatusPayload {
        active: !session.stop_flag.load(Ordering::Relaxed),
        started_at_ms: Some(session.started_at_ms),
        place_id: config.place_id,
        job_id: config.job_id,
        launch_data: config.launch_data,
        interval_minutes: config.interval_minutes as i64,
        launch_delay_seconds: config.launch_delay_seconds as i64,
        player_grace_minutes: config.player_grace_minutes as i64,
        player_user_ids,
        user_ids: config.user_ids,
        accounts,
    }
}

#[cfg(target_os = "windows")]
fn current_botting_status() -> BottingStatusPayload {
    if let Some(session) = BOTTING_MANAGER.get_session() {
        botting_status_from_session(&session)
    } else {
        BottingStatusPayload::default()
    }
}

#[cfg(target_os = "windows")]
fn emit_botting_status(app: &tauri::AppHandle) {
    let _ = app.emit("botting-status", current_botting_status());
}
