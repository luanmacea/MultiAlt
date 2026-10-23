struct SendHandle(HANDLE);
unsafe impl Send for SendHandle {}

/// Release signal for the dedicated thread that owns `ROBLOX_singletonMutex`.
/// Win32 mutex ownership is per-thread: ReleaseMutex only works on the thread
/// that acquired it, and the mutex is abandoned (and can be taken by a Roblox
/// client, re-enabling single-instance mode) if that thread exits. Tokio worker
/// and blocking threads give neither guarantee, so one long-lived thread
/// acquires, holds and releases it.
static MULTI_ROBLOX_HANDLE: Mutex<Option<std::sync::mpsc::Sender<()>>> = Mutex::new(None);
static COOKIES_LOCK_HANDLE: Mutex<Option<SendHandle>> = Mutex::new(None);
/// Reserva experimental do nome `ROBLOX_singletonEvent` (ideia 3): um Mutex
/// **nosso** com esse nome. Enquanto ele existe, nenhum cliente consegue criar
/// o Event de instância única, e um teleporte não derruba outro cliente. Só
/// guardamos o handle — não é preciso ser dono do mutex, então qualquer thread
/// pode criar e fechar.
static SINGLETON_RESERVATION: Mutex<Option<SendHandle>> = Mutex::new(None);
/// A opção "Experimental: keep clients open across teleports"
/// (`General.ReserveSingletonEvent`), desligada por padrão.
static RESERVE_SINGLETON_EVENT: AtomicBool = AtomicBool::new(false);
static TRACKER: LazyLock<ProcessTracker> = LazyLock::new(ProcessTracker::new);

fn encode_wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

pub fn tracker() -> &'static ProcessTracker {
    &TRACKER
}

pub fn generate_browser_tracker_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let a = (now % 75000 + 100000) as u64;
    let b = ((now / 31) % 800000 + 100000) as u64;
    format!("{}{}", a, b)
}

/// Sobe a thread dedicada e tenta tomar posse de `ROBLOX_singletonMutex`.
/// `Ok(None)` = o mutex está com outro processo (cliente aberto, RAM legado,
/// outra ferramenta). A thread só sobrevive se a posse for conquistada.
fn spawn_singleton_mutex_owner() -> Result<Option<std::sync::mpsc::Sender<()>>, String> {
    let (acquired_tx, acquired_rx) = std::sync::mpsc::channel::<Result<bool, String>>();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    std::thread::Builder::new()
        .name("multi-roblox-mutex".into())
        .spawn(move || {
            let name = encode_wide("ROBLOX_singletonMutex");
            let h = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
            if h.is_null() {
                let _ = acquired_tx.send(Err("Failed to create mutex".into()));
                return;
            }
            let result = unsafe { WaitForSingleObject(h, 0) };
            if result != WAIT_OBJECT_0 && result != WAIT_ABANDONED_0 {
                unsafe { CloseHandle(h) };
                let _ = acquired_tx.send(Ok(false));
                return;
            }
            let _ = acquired_tx.send(Ok(true));
            // Hold ownership until asked to release (or the sender is dropped).
            let _ = release_rx.recv();
            unsafe {
                ReleaseMutex(h);
                CloseHandle(h);
            }
        })
        .map_err(|e| format!("Failed to spawn mutex thread: {}", e))?;

    match acquired_rx.recv() {
        Ok(Ok(true)) => Ok(Some(release_tx)),
        Ok(Ok(false)) => Ok(None),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("Mutex thread exited unexpectedly".into()),
    }
}

/// Garante que a thread dedicada esteja segurando `ROBLOX_singletonMutex`.
/// `Ok(true)` = já segurávamos ou acabamos de conquistar.
fn acquire_multi_roblox_mutex() -> Result<bool, String> {
    let mut handle = MULTI_ROBLOX_HANDLE.lock().map_err(|e| e.to_string())?;
    if handle.is_some() {
        return Ok(true);
    }
    match spawn_singleton_mutex_owner()? {
        Some(release_tx) => {
            *handle = Some(release_tx);
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Habilita vários clientes Roblox simultâneos.
///
/// São duas travas diferentes, e elas **não** se substituem:
///
/// 1. `ROBLOX_singletonMutex` — trava legada. Só funciona preventivamente: se o
///    app pegar o mutex **antes** de qualquer cliente, os clientes seguintes
///    sobem sem instalar o modo instância única. Se um cliente já estiver
///    aberto (o usuário entrou pelo site), o mutex é dele e não há como tomá-lo
///    sem matar o processo — era exatamente aí que o app dizia "A Roblox client
///    is already running" ou matava os clientes do usuário.
/// 2. `ROBLOX_singletonEvent` — é o que o cliente **moderno** consulta ao subir:
///    se o nome existe, ele sinaliza a instância antiga e sai. Fechar esse
///    handle de fora (`singleton.rs`) apaga o nome sem tocar no processo: o
///    cliente aberto continua jogando e o próximo sobe normal. Verificado na
///    máquina do usuário, sem elevação.
///
/// Conclusão: para o cliente atual o **Event é o que importa**; o mutex fica
/// porque continua sendo a trava barata e preventiva (e builds antigas ainda
/// dependem dele). Por isso o mutex é mantido como estava e o fechamento do
/// Event entrou como etapa extra — inclusive no caminho de sucesso, para cobrir
/// o caso "app já segurava o mutex e o usuário abriu o jogo pelo site depois".
/// Sem cliente Roblox aberto a etapa nova não custa nada (sai na checagem de
/// pids) e o comportamento antigo é idêntico.
pub fn enable_multi_roblox() -> Result<bool, String> {
    match enable_multi_roblox_checked()? {
        MultiRobloxOutcome::Enabled | MultiRobloxOutcome::ProvidedByOtherProgram => Ok(true),
        MultiRobloxOutcome::Blocked => Ok(false),
        MultiRobloxOutcome::EventStillHeld { unreachable } => {
            Err(event_still_held_message(unreachable).to_string())
        }
        MultiRobloxOutcome::EventHeldByProgram { programs } => {
            Err(event_held_by_program_message(&programs))
        }
    }
}

/// O que `enable_multi_roblox_checked` concluiu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultiRobloxOutcome {
    /// Pode abrir mais um cliente.
    Enabled,
    /// O caminho antigo do `Ok(false)`: quem segura o mutex não deu para
    /// destravar (RAM legado, outra ferramenta, ou cliente sem Event a fechar).
    Blocked,
    /// Outro programa (um executor como o Real, outra ferramenta de várias
    /// instâncias) segura o `ROBLOX_singletonMutex` e nenhum cliente está
    /// aberto: o próximo cliente já sobe sem o modo instância única. Segue
    /// como `Enabled`, sem fechar nada.
    ProvidedByOtherProgram,
    /// Outro cliente **ainda** segura o `ROBLOX_singletonEvent` depois de
    /// fechar e reservar. Abrir agora faria esse cliente fechar: o launch não
    /// segue. `unreachable` = clientes cujo processo nem abriu (admin).
    EventStillHeld { unreachable: usize },
    /// A trava continua de pé porque **outro programa** (não um cliente) a
    /// segura. `programs` = os executáveis, para a mensagem.
    EventHeldByProgram { programs: String },
}

/// Launch pulado porque outro cliente ainda segura o Event. Em inglês: o
/// frontend traduz (`launchErrorText` em src/utils/robloxErrors.ts, que lê
/// estas constantes do fonte no teste).
pub const MULTI_ROBLOX_EVENT_HELD: &str = "Another Roblox window could not be unlocked. Launching now would close it, so this launch was skipped.";
/// O mesmo, quando algum processo do Roblox nem pôde ser aberto — o motivo
/// mais comum é ele rodar como administrador.
pub const MULTI_ROBLOX_EVENT_HELD_ADMIN: &str = "Another Roblox window could not be unlocked (it may be running as administrator). Launching now would close it, so this launch was skipped.";

/// Outro programa segura a trava (o app não mexe nela). O nome do
/// programa vai no fim (`event_held_by_program_message`); o frontend traduz a
/// frase e mantém o nome.
pub const MULTI_ROBLOX_EVENT_HELD_BY_PROGRAM: &str = "Another program is holding the Roblox lock that allows several windows, so this launch was skipped. Close it and try again:";

pub fn event_held_by_program_message(programs: &str) -> String {
    format!("{MULTI_ROBLOX_EVENT_HELD_BY_PROGRAM} {programs}")
}

pub fn event_still_held_message(unreachable: usize) -> &'static str {
    if unreachable > 0 {
        MULTI_ROBLOX_EVENT_HELD_ADMIN
    } else {
        MULTI_ROBLOX_EVENT_HELD
    }
}

/// Como a camada de launch responde ao resultado. `Some` = responder já;
/// `None` = seguir o caminho antigo do `Ok(false)` (mensagem, ou o último
/// recurso opt-in `AutoCloseRobloxForMultiRbx`). O Event preso **só** vira
/// matança com essa opção ligada; desligada, o launch para com a mensagem.
pub fn held_event_response(
    outcome: MultiRobloxOutcome,
    auto_close_conflicts: bool,
) -> Option<Result<(), String>> {
    match outcome {
        MultiRobloxOutcome::Enabled | MultiRobloxOutcome::ProvidedByOtherProgram => Some(Ok(())),
        MultiRobloxOutcome::EventStillHeld { unreachable } if !auto_close_conflicts => {
            Some(Err(event_still_held_message(unreachable).to_string()))
        }
        // Matar clientes não solta a trava de outro programa: sempre explica.
        MultiRobloxOutcome::EventHeldByProgram { programs } => {
            Some(Err(event_held_by_program_message(&programs)))
        }
        MultiRobloxOutcome::EventStillHeld { .. } | MultiRobloxOutcome::Blocked => None,
    }
}

/// Decisão pura do `enable_multi_roblox_checked`, tomada **depois** de fechar
/// os Events e aplicar a reserva. `event_blocks` = o nome ainda existe e não é
/// a nossa reserva (`event_blocks_next_client`).
///
/// Com cliente Roblox aberto e o Event ainda preso, não abre — nem com o mutex
/// na nossa mão (antes o resultado do fechamento era ignorado nesse caminho),
/// nem tendo fechado *algum* Event (antes "fechei algum" bastava, mesmo com
/// outro cliente ainda segurando o dele).
fn multi_roblox_outcome(
    holds_mutex: bool,
    closed_now: usize,
    roblox_running: bool,
    event_blocks: bool,
    unreachable: usize,
) -> MultiRobloxOutcome {
    if roblox_running && event_blocks {
        return MultiRobloxOutcome::EventStillHeld { unreachable };
    }
    if holds_mutex || can_open_another_client(closed_now, roblox_running, event_blocks) {
        MultiRobloxOutcome::Enabled
    } else {
        MultiRobloxOutcome::Blocked
    }
}

/// O `Blocked` sem cliente Roblox aberto quer dizer que **outro programa** é
/// dono do mutex. Se não é o RAM legado (barrado de propósito) e nenhum Event
/// de verdade barra o próximo cliente, esse programa já liberou as várias
/// instâncias: o launch segue. Antes o app recusava ("Could not acquire the
/// Roblox singleton mutex") e quem usava o executor Real não jogava.
fn with_other_program_holder(
    outcome: MultiRobloxOutcome,
    holds_mutex: bool,
    roblox_running: bool,
    event_blocks: bool,
    legacy_running: bool,
) -> MultiRobloxOutcome {
    if outcome == MultiRobloxOutcome::Blocked
        && !holds_mutex
        && !roblox_running
        && !event_blocks
        && !legacy_running
    {
        MultiRobloxOutcome::ProvidedByOtherProgram
    } else {
        outcome
    }
}

/// A trava ainda barra e quem a segura é outro programa: a resposta vira
/// `EventHeldByProgram`, com o nome dele.
fn with_foreign_holders(
    outcome: MultiRobloxOutcome,
    event_blocks: bool,
    programs: &[String],
) -> MultiRobloxOutcome {
    let refused = matches!(
        outcome,
        MultiRobloxOutcome::EventStillHeld { .. } | MultiRobloxOutcome::Blocked
    );
    if refused && event_blocks && !programs.is_empty() {
        MultiRobloxOutcome::EventHeldByProgram { programs: programs.join(", ") }
    } else {
        outcome
    }
}

/// `enable_multi_roblox` com o motivo da recusa. Nunca fecha cliente: no
/// máximo fecha handles de Event e cria/fecha a reserva do app.
pub fn enable_multi_roblox_checked() -> Result<MultiRobloxOutcome, String> {
    // Um cliente aberto fora do app pode ter publicado o Event mesmo com o
    // mutex na nossa mão; limpar é barato e não fecha ninguém. Com o mutex
    // ocupado, é o que destrava antes de o chamador cair no último recurso de
    // matar clientes (`AutoCloseRobloxForMultiRbx`).
    let holds_mutex = acquire_multi_roblox_mutex()?;
    let report = close_roblox_singleton_handles();
    apply_singleton_reservation();
    let roblox_running = !find_roblox_pids_all().is_empty();
    let event_blocks = event_blocks_next_client(
        named_event_exists(ROBLOX_SINGLETON_EVENT),
        singleton_reservation_held(),
    );
    // Ainda de pé depois dos clientes: outro programa (Fishtrap, executor)
    // segura o nome. O app só diz quem é, para a pessoa fechar.
    let foreign_programs: Vec<String> = if event_blocks {
        find_foreign_roblox_singleton_holders()
            .into_iter()
            .map(|pid| exe_name_of(pid).unwrap_or_else(|| format!("PID {pid}")))
            .collect()
    } else {
        Vec::new()
    };
    let outcome = multi_roblox_outcome(
        holds_mutex,
        report.closed,
        roblox_running,
        event_blocks,
        report.unreachable,
    );
    let outcome = with_foreign_holders(outcome, event_blocks, &foreign_programs);
    let outcome = if outcome == MultiRobloxOutcome::Blocked {
        with_other_program_holder(
            outcome,
            holds_mutex,
            roblox_running,
            event_blocks,
            !find_legacy_ram_pids().is_empty(),
        )
    } else {
        outcome
    };
    match outcome {
        MultiRobloxOutcome::ProvidedByOtherProgram => {
            eprintln!(
                "Multi Roblox: ROBLOX_singletonMutex está com outro programa e nenhum cliente aberto; várias instâncias já liberadas, launch segue"
            );
            lock_roblox_cookies()?;
        }
        MultiRobloxOutcome::Enabled => {
            if !holds_mutex {
                // O mutex pode ter sido liberado nesse meio tempo; se não foi,
                // seguimos assim mesmo — sem o Event o cliente novo não desiste.
                let _ = acquire_multi_roblox_mutex()?;
            }
            lock_roblox_cookies()?;
        }
        MultiRobloxOutcome::EventStillHeld { unreachable } => {
            eprintln!(
                "Multi Roblox: {} ainda existe depois do fechamento ({} processo(s) inacessível(is)); launch pulado para não fechar o cliente aberto",
                ROBLOX_SINGLETON_EVENT, unreachable
            );
        }
        MultiRobloxOutcome::EventHeldByProgram { ref programs } => {
            eprintln!(
                "Multi Roblox: {} preso em outro programa ({}); launch pulado",
                ROBLOX_SINGLETON_EVENT, programs
            );
        }
        MultiRobloxOutcome::Blocked => {}
    }
    Ok(outcome)
}

/// Com o mutex nas mãos de outro processo: dá para abrir mais um cliente?
///
/// Sim se algum `ROBLOX_singletonEvent` foi fechado agora — **ou** se há
/// cliente aberto e o nome nem existe mais: ele já foi fechado numa leva
/// anterior, e os clientes que o recriaram saíram. Sem o Event o próximo
/// cliente não tem instância antiga para sinalizar e sobe normal; recusar aí
/// era barrar um launch que funcionaria (relato do dono, 28/09/2026: depois de
/// fechar as alts de uma leva, todo launch dava "A Roblox client is already
/// running").
///
/// Sem cliente Roblox aberto nada muda: quem segura o mutex é outra coisa (RAM
/// legado, outra ferramenta) e o chamador segue o caminho antigo.
fn can_open_another_client(closed_now: usize, roblox_running: bool, event_exists: bool) -> bool {
    closed_now > 0 || (roblox_running && !event_exists)
}

/// Um `ROBLOX_singletonEvent` (um Event de verdade) barra o próximo cliente,
/// a não ser que seja a **nossa** reserva. `named_event_exists` já ignora um
/// objeto de outro tipo com o mesmo nome (um Mutex), então `reserved_by_us`
/// fica como salvaguarda explícita do caso em que somos nós que seguramos o
/// nome.
fn event_blocks_next_client(name_exists: bool, reserved_by_us: bool) -> bool {
    name_exists && !reserved_by_us
}

/// O que fazer com a reserva do nome `ROBLOX_singletonEvent` agora.
#[derive(Debug, PartialEq, Eq)]
enum ReservationStep {
    /// Ligada e já reservada.
    Keep,
    /// Ligada, sem reserva e o nome livre: criar o Mutex antes que um cliente
    /// crie o Event.
    Create,
    /// Ligada, mas um cliente ainda segura o Event (não deu para fechar): não
    /// há como reservar agora; o método atual segue valendo.
    Wait,
    /// Desligada com reserva feita: soltar o nome.
    Release,
    Nothing,
}

fn reservation_step(enabled: bool, held: bool, event_exists_now: bool) -> ReservationStep {
    match (enabled, held) {
        (true, true) => ReservationStep::Keep,
        (true, false) if !event_exists_now => ReservationStep::Create,
        (true, false) => ReservationStep::Wait,
        (false, true) => ReservationStep::Release,
        (false, false) => ReservationStep::Nothing,
    }
}

/// Liga/desliga a reserva experimental. Desligar solta o nome na hora.
pub fn set_singleton_reservation_enabled(enabled: bool) {
    RESERVE_SINGLETON_EVENT.store(enabled, Ordering::SeqCst);
    if !enabled {
        release_singleton_reservation();
    }
}

pub fn singleton_reservation_enabled() -> bool {
    RESERVE_SINGLETON_EVENT.load(Ordering::SeqCst)
}

pub fn singleton_reservation_held() -> bool {
    SINGLETON_RESERVATION
        .lock()
        .map(|g| g.is_some())
        .unwrap_or(false)
}

/// Executa o passo de [`reservation_step`]. Nunca fecha cliente nenhum: no
/// máximo cria ou fecha um handle **nosso**.
fn apply_singleton_reservation() {
    let enabled = singleton_reservation_enabled();
    let held = singleton_reservation_held();
    let event_exists_now = enabled && !held && named_event_exists(ROBLOX_SINGLETON_EVENT);
    match reservation_step(enabled, held, event_exists_now) {
        ReservationStep::Create => {
            let _ = reserve_named_object(ROBLOX_SINGLETON_EVENT);
        }
        ReservationStep::Release => release_singleton_reservation(),
        ReservationStep::Keep | ReservationStep::Wait | ReservationStep::Nothing => {}
    }
}

/// Cria um Mutex (sem dono) com `name` e guarda o handle. `false` se o nome já
/// é de outro tipo de objeto (um Event de cliente) ou a API falhou.
fn reserve_named_object(name: &str) -> bool {
    let Ok(mut slot) = SINGLETON_RESERVATION.lock() else {
        return false;
    };
    if slot.is_some() {
        return true;
    }
    let wide = encode_wide(name);
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, wide.as_ptr()) };
    if handle.is_null() {
        return false;
    }
    *slot = Some(SendHandle(handle));
    eprintln!("Multi Roblox (experimental): {} reservado", name);
    true
}

fn release_singleton_reservation() {
    if let Ok(mut slot) = SINGLETON_RESERVATION.lock() {
        if let Some(SendHandle(h)) = slot.take() {
            unsafe { CloseHandle(h) };
        }
    }
}

fn release_multi_roblox_mutex() {
    if let Ok(mut handle) = MULTI_ROBLOX_HANDLE.lock() {
        if let Some(release) = handle.take() {
            let _ = release.send(());
        }
    }
}

pub fn release_multi_roblox_handle() {
    release_multi_roblox_mutex();
    if let Ok(mut cookie_handle) = COOKIES_LOCK_HANDLE.lock() {
        if let Some(SendHandle(h)) = cookie_handle.take() {
            unsafe {
                CloseHandle(h);
            }
        }
    }
}

pub fn this_process_holds_multi_roblox() -> bool {
    MULTI_ROBLOX_HANDLE
        .lock()
        .map(|g| g.is_some())
        .unwrap_or(false)
}

pub fn disable_multi_roblox() -> Result<(), String> {
    release_multi_roblox_mutex();
    // Multi Roblox desligado (ou o app fechando): a reserva experimental sai
    // junto. Só fecha o handle nosso; nenhum cliente é tocado.
    release_singleton_reservation();

    lock_roblox_cookies()?;
    Ok(())
}

fn lock_roblox_cookies() -> Result<(), String> {
    let mut handle = COOKIES_LOCK_HANDLE.lock().map_err(|e| e.to_string())?;
    if handle.is_some() || is_773_fix_disabled() {
        return Ok(());
    }

    let Some(path) = get_roblox_cookies_path() else {
        return Ok(());
    };

    let wide = encode_wide(path.as_os_str());
    unsafe {
        let cookie_handle = CreateFileW(
            wide.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        );
        if cookie_handle == INVALID_HANDLE_VALUE {
            eprintln!("Warning: Could not lock RobloxCookies.dat for the 773 fix");
            return Ok(());
        }
        *handle = Some(SendHandle(cookie_handle));
    }

    Ok(())
}

fn get_roblox_cookies_path() -> Option<PathBuf> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")?;
    let path = PathBuf::from(local_app_data)
        .join("Roblox")
        .join("LocalStorage")
        .join("RobloxCookies.dat");
    path.exists().then_some(path)
}

fn is_773_fix_disabled() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|dir| dir.join("no773fix.txt")))
        .map(|path| path.exists())
        .unwrap_or(false)
}

pub fn get_roblox_path() -> Result<String, String> {
    // The registry handler can point at an account test-channel build (see
    // `launch_url`); prefer the production build once it has been resolved.
    if let Some(dir) = cached_production_player_dir() {
        return Ok(dir);
    }

    unsafe {
        let key_name = encode_wide("roblox\\DefaultIcon");
        let mut hkey: windows_sys::Win32::System::Registry::HKEY = std::ptr::null_mut();

        if RegOpenKeyExW(HKEY_CLASSES_ROOT, key_name.as_ptr(), 0, KEY_READ, &mut hkey) == 0 {
            let mut buf = [0u16; 512];
            let mut buf_size = (buf.len() * 2) as u32;
            let mut value_type = 0u32;

            let result = RegQueryValueExW(
                hkey,
                std::ptr::null(),
                std::ptr::null_mut(),
                &mut value_type,
                buf.as_mut_ptr() as *mut u8,
                &mut buf_size,
            );

            RegCloseKey(hkey);

            if result == 0 && value_type == REG_SZ {
                let len = (buf_size as usize / 2).saturating_sub(1);
                let path = String::from_utf16_lossy(&buf[..len]);
                if let Some(parent) = std::path::Path::new(&path).parent() {
                    if parent.exists() {
                        return Ok(parent.to_string_lossy().into_owned());
                    }
                }
            }
        }
    }

    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
    for dir in candidate_versions_dirs(&local_app_data) {
        if let Some((_, path)) = scan_versions_dir(&dir) {
            return Ok(path);
        }
    }

    Err("Roblox installation not found".into())
}

/// Varre uma pasta `Versions` (do Roblox oficial ou de um bootstrapper de
/// terceiros) e devolve a instalação válida mais recente nela, se houver.
/// Toca disco — sem teste direto; o que é testado é a lista de pastas que
/// alimenta esta função (`candidate_versions_dirs`).
fn scan_versions_dir(dir: &std::path::Path) -> Option<(SystemTime, String)> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut best: Option<(SystemTime, String)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("version-") && entry.path().join("RobloxPlayerBeta.exe").exists() {
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    if best.as_ref().map_or(true, |(t, _)| modified > *t) {
                        best = Some((modified, entry.path().to_string_lossy().into_owned()));
                    }
                }
            }
        }
    }
    best
}

/// Pastas `Versions` candidatas para achar uma instalação do Roblox, na ordem
/// em que `get_roblox_path` deve procurar. `Roblox` vem sempre primeiro — a
/// instalação oficial ganha de qualquer bootstrapper de terceiros presente na
/// mesma máquina — e só depois os bootstrappers mais usados (Bloxstrap,
/// Fishstrap, Voidstrap), que instalam o `RobloxPlayerBeta.exe` na própria
/// pasta em vez de `%LOCALAPPDATA%\Roblox`.
fn candidate_versions_dirs(local_app_data: &str) -> Vec<PathBuf> {
    let local = PathBuf::from(local_app_data);
    ["Roblox", "Bloxstrap", "Fishstrap", "Voidstrap"]
        .iter()
        .map(|root| local.join(root).join("Versions"))
        .collect()
}

fn get_client_settings_file() -> Result<PathBuf, String> {
    get_client_settings_file_in(&get_roblox_path()?)
}

/// Parte pura de `get_client_settings_file`: monta o caminho do
/// `ClientAppSettings.json` dentro de uma pasta base qualquer, em vez de sempre
/// resolver a build de produção. É o que deixa `patch_client_settings_for_launch`
/// (`commands/launch_shared.rs`) escrever no `ClientSettings` da versão que a
/// conta vai de fato abrir — `None` continua significando "a build padrão"
/// (é o caso do servidor HTTP local, que não tem conta no contexto).
fn get_client_settings_file_in(base_path: &str) -> Result<PathBuf, String> {
    let settings_dir = std::path::Path::new(base_path).join("ClientSettings");

    if !settings_dir.exists() {
        std::fs::create_dir_all(&settings_dir)
            .map_err(|e| format!("Failed to create ClientSettings: {}", e))?;
    }

    Ok(settings_dir.join("ClientAppSettings.json"))
}

/// Versão do Windows para o resumo do "Reportar problema" (ideia 28), lida do
/// registro (`HKLM\...\Windows NT\CurrentVersion`) — a mesma leitura que o
/// isolamento já faz, sem API nova no binário.
pub fn os_version_label() -> String {
    const KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let read = |name: &str| read_string_value(HKEY_LOCAL_MACHINE as _, KEY, name);
    format_windows_version(
        read("ProductName").as_deref(),
        read("DisplayVersion").as_deref(),
        read("CurrentBuild").as_deref(),
    )
}

/// Monta "Windows 11 Home (24H2, build 26200)". O `ProductName` do registro diz
/// "Windows 10" também no 11; a build (22000 ou mais) é o que separa os dois.
fn format_windows_version(product: Option<&str>, display: Option<&str>, build: Option<&str>) -> String {
    let build_number = build.and_then(|b| b.trim().parse::<u32>().ok());
    let mut name = product
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .unwrap_or("Windows")
        .to_string();
    if build_number.is_some_and(|b| b >= 22_000) && name.starts_with("Windows 10") {
        name = name.replacen("Windows 10", "Windows 11", 1);
    }
    let details: Vec<String> = [
        display.map(str::trim).filter(|d| !d.is_empty()).map(str::to_string),
        build_number.map(|b| format!("build {b}")),
    ]
    .into_iter()
    .flatten()
    .collect();
    if details.is_empty() {
        name
    } else {
        format!("{name} ({})", details.join(", "))
    }
}
