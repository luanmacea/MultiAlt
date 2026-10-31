use windows_sys::Win32::Graphics::Gdi::{
    MonitorFromPoint, MonitorFromWindow, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, GetWindow, GetWindowLongW, GetWindowPlacement, IsZoomed, SetWindowLongW,
    SetWindowPlacement, SetWindowPos, WINDOWPLACEMENT,
    GWL_EXSTYLE, GWL_STYLE, GW_HWNDPREV, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOOWNERZORDER, SWP_NOSENDCHANGING, SWP_NOSIZE, SWP_NOZORDER, SW_SHOWMINNOACTIVE,
    SW_SHOWNOACTIVATE, WS_CAPTION, WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW, WS_POPUP, WS_THICKFRAME,
};

struct EnumWindowData {
    target_pid: u32,
    result_hwnd: HWND,
}

unsafe extern "system" fn enum_window_callback(hwnd: HWND, lparam: isize) -> i32 {
    let data = &mut *(lparam as *mut EnumWindowData);
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == data.target_pid && IsWindowVisible(hwnd) != 0 {
        data.result_hwnd = hwnd;
        return 0;
    }
    1
}

pub fn find_main_window(target_pid: u32) -> Option<HWND> {
    let mut data = EnumWindowData {
        target_pid,
        result_hwnd: std::ptr::null_mut(),
    };
    unsafe {
        EnumWindows(Some(enum_window_callback), &mut data as *mut _ as isize);
    }
    if !data.result_hwnd.is_null() {
        Some(data.result_hwnd)
    } else {
        None
    }
}

pub fn get_window_position(hwnd: HWND) -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect) != 0 {
            Some((
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
            ))
        } else {
            None
        }
    }
}

pub fn set_window_position(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) -> bool {
    unsafe { MoveWindow(hwnd, x, y, w, h, 1) != 0 }
}

pub fn minimize_window(hwnd: HWND) -> bool {
    unsafe { ShowWindow(hwnd, SW_MINIMIZE) != 0 }
}

/// A janela está minimizada? O AFK mode pergunta antes de trazer a janela para
/// frente, para devolvê-la ao estado em que o usuário a deixou.
pub fn window_is_minimized(hwnd: HWND) -> bool {
    if hwnd.is_null() {
        return false;
    }
    unsafe { IsIconic(hwnd) != 0 }
}

/// A janela de topo sob um ponto da tela — a janela do programa, não um
/// controle filho dela. É o Marcar do AFK mode que pergunta.
pub fn root_window_at(x: i32, y: i32) -> HWND {
    let hwnd = unsafe { WindowFromPoint(POINT { x, y }) };
    if hwnd.is_null() {
        return hwnd;
    }
    unsafe { GetAncestor(hwnd, GA_ROOT) }
}

/// O PID dono da janela, ou `None` para janela nula.
pub fn window_pid(hwnd: HWND) -> Option<u32> {
    if hwnd.is_null() {
        return None;
    }
    let mut pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    (pid != 0).then_some(pid)
}

/// Para que a janela vem para frente — é isso que decide se o estado dela
/// (minimizada, normal, maximizada) pode mudar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusPurpose {
    /// Trazer uma janela para frente para ela receber algo: o cliente da conta
    /// no AFK mode, o "focar a janela" da conta. Minimizada é restaurada (não dá
    /// para receber tecla minimizada); normal ou maximizada fica como está.
    Bring,
    /// Devolver o primeiro plano à janela que estava lá antes — a janela em que
    /// o usuário estava trabalhando. Nunca mexe no estado dela.
    GiveBack,
}

/// O `ShowWindow` que vai antes do `SetForegroundWindow`, se algum.
///
/// `SW_RESTORE` só em janela **minimizada**: numa maximizada ele a devolve ao
/// tamanho normal. Aplicado sempre, o AFK mode tirava do maximizado o cliente
/// alvo e — ao devolver o foco — a janela em que o usuário estava trabalhando,
/// a cada ciclo, até quando o foco tinha sido negado e nada foi enviado.
fn show_command_before_focus(purpose: FocusPurpose, minimized: bool) -> Option<i32> {
    match purpose {
        FocusPurpose::Bring if minimized => Some(SW_RESTORE),
        FocusPurpose::Bring | FocusPurpose::GiveBack => None,
    }
}

fn bring_to_foreground(hwnd: HWND, purpose: FocusPurpose) -> bool {
    if let Some(command) = show_command_before_focus(purpose, window_is_minimized(hwnd)) {
        unsafe {
            let _ = ShowWindow(hwnd, command);
        }
    }
    unsafe { SetForegroundWindow(hwnd) != 0 }
}

/// Traz a janela para o primeiro plano. Minimizada volta ao tamanho de antes;
/// normal ou maximizada vem como está.
pub fn focus_window(hwnd: HWND) -> bool {
    bring_to_foreground(hwnd, FocusPurpose::Bring)
}

/// Devolve o primeiro plano à janela que estava lá, **sem mexer no estado
/// dela**: nem restaurar, nem desmaximizar.
pub fn give_focus_back(hwnd: HWND) -> bool {
    bring_to_foreground(hwnd, FocusPurpose::GiveBack)
}

/// Como terminou a devolução do foco no fim de um ciclo do AFK mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GiveBack {
    /// A janela do usuário está na frente de novo.
    Restored,
    /// O usuário escolheu outra janela durante o ciclo: ela fica na frente.
    UserMovedOn,
    /// A janela de antes não existe mais.
    Gone,
    /// O Windows recusou todas as tentativas.
    Denied,
}

/// O que a devolução do foco usa do Windows — trocável nos testes.
trait ForegroundApi {
    fn foreground(&self) -> isize;
    fn exists(&self, hwnd: isize) -> bool;
    fn set_foreground(&self, hwnd: isize) -> bool;
    /// `SetForegroundWindow` depois de um movimento de mouse de zero pixel
    /// (`nudge_for_focus_back`): o Windows só aceita o pedido de quem gerou a
    /// última entrada, e o usuário pode ter mexido em outra janela depois da
    /// tecla do ciclo. Não lê entrada nenhuma.
    fn set_foreground_after_nudge(&self, hwnd: isize) -> bool;
    /// `focus_window`: traz a janela da conta (desminimiza, se preciso).
    fn bring(&self, hwnd: isize) -> bool;
    /// `focus_window` depois do movimento de zero pixel.
    fn bring_after_nudge(&self, hwnd: isize) -> bool;
    fn settle(&self);
}

/// Tentativas ao todo: a simples e, se recusada, as depois do movimento de zero pixel.
const GIVE_BACK_ATTEMPTS: usize = 3;
const GIVE_BACK_SETTLE_MS: u64 = 60;

/// Devolve o primeiro plano a `target` depois de o ciclo ter trazido as
/// janelas `cycle_windows` para frente (issue #23).
///
/// O `SetForegroundWindow` é recusado sem aviso quando outra janela recebeu a
/// última entrada do usuário — quem trabalha no outro monitor durante o ciclo.
/// Por isso o resultado é conferido na tela e a volta tenta de novo; e se o que
/// está na frente não é nenhuma janela do ciclo, foi o usuário que escolheu
/// outra janela, e ela fica.
fn give_focus_back_with(api: &impl ForegroundApi, target: isize, cycle_windows: &[isize]) -> GiveBack {
    if !api.exists(target) {
        return GiveBack::Gone;
    }
    for attempt in 0..GIVE_BACK_ATTEMPTS {
        let front = api.foreground();
        if front == target {
            return GiveBack::Restored;
        }
        if front != 0 && !cycle_windows.contains(&front) {
            return GiveBack::UserMovedOn;
        }
        if attempt == 0 {
            api.set_foreground(target);
        } else {
            api.set_foreground_after_nudge(target);
        }
        api.settle();
    }
    let front = api.foreground();
    if front == target {
        GiveBack::Restored
    } else if front != 0 && !cycle_windows.contains(&front) {
        GiveBack::UserMovedOn
    } else {
        GiveBack::Denied
    }
}

/// Traz a janela da conta para frente no começo do envio do AFK mode e diz se
/// ela ficou na frente.
///
/// Mesma regra da volta: o Windows recusa quem não gerou a última entrada — o
/// usuário trabalhando em outra janela. Recusado, manda o movimento de zero
/// pixel e tenta mais uma vez. Medido no teste do dono (10/10/2026): sem isso,
/// com o foco devolvido de verdade no fim do ciclo, todo ciclo seguinte pulava
/// todas as contas.
fn bring_forward_with(api: &impl ForegroundApi, target: isize) -> bool {
    if api.foreground() == target {
        return true;
    }
    api.bring(target);
    api.settle();
    if api.foreground() == target {
        return true;
    }
    api.bring_after_nudge(target);
    api.settle();
    api.foreground() == target
}

struct RealDesktop;

impl ForegroundApi for RealDesktop {
    fn foreground(&self) -> isize {
        unsafe { GetForegroundWindow() as isize }
    }
    fn exists(&self, hwnd: isize) -> bool {
        hwnd != 0 && window_exists(hwnd as HWND)
    }
    fn set_foreground(&self, hwnd: isize) -> bool {
        give_focus_back(hwnd as HWND)
    }
    fn set_foreground_after_nudge(&self, hwnd: isize) -> bool {
        nudge_for_focus_back();
        give_focus_back(hwnd as HWND)
    }
    fn bring(&self, hwnd: isize) -> bool {
        focus_window(hwnd as HWND)
    }
    fn bring_after_nudge(&self, hwnd: isize) -> bool {
        nudge_for_focus_back();
        focus_window(hwnd as HWND)
    }
    fn settle(&self) {
        std::thread::sleep(Duration::from_millis(GIVE_BACK_SETTLE_MS));
    }
}

/// Começo do envio do AFK mode para uma conta: traz a janela dela e diz se
/// ficou na frente, tentando de novo se o Windows recusar.
pub fn bring_forward_for_cycle(hwnd: HWND) -> bool {
    bring_forward_with(&RealDesktop, hwnd as isize)
}

/// Fim do ciclo do AFK mode: devolve o foco à janela em que o usuário estava,
/// conferindo na tela e tentando de novo se o Windows recusar.
pub fn give_focus_back_after_cycle(target: HWND, cycle_windows: &[HWND]) -> GiveBack {
    let cycle: Vec<isize> = cycle_windows.iter().map(|h| *h as isize).collect();
    give_focus_back_with(&RealDesktop, target as isize, &cycle)
}

// ── devolver a janela da conta ao lugar (ideia 44) ──────────────────────────
//
// Trazer a janela para frente a tira de onde ela estava: a minimizada fica na
// tela, e a que estava aberta atrás de outras janelas fica por cima delas. O
// ciclo do AFK mode anota antes onde cada uma estava e, no fim, depois de
// devolver o foco, põe cada uma de volta **sem ativar janela nenhuma**.

/// O `ShowWindow` que devolve à barra de tarefas a janela que o ciclo
/// desminimizou. `SW_MINIMIZE` não serve: com a janela do Roblox em primeiro
/// plano, ele faz o Windows **ativar a próxima janela** da pilha — às vezes
/// outro Roblox —, e as janelas piscavam umas por cima das outras.
/// `SW_SHOWMINNOACTIVE` minimiza sem ativar ninguém, e a janela lembra se
/// estava maximizada.
const QUIET_MINIMIZE: i32 = SW_SHOWMINNOACTIVE;

/// Os flags do `SetWindowPos` que devolve a janela à profundidade de antes:
/// só a ordem muda — nem ativa, nem move, nem redimensiona.
const PUT_BACK_SWP_FLAGS: u32 = SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE;

/// Teto da caminhada pela pilha de janelas em `window_above` (nunca chega
/// perto disso; é só para não depender de a pilha não mudar no meio).
const WINDOW_ABOVE_MAX_STEPS: usize = 1_024;

/// Uma janela da pilha acima de outra, como `window_above` a enxerga.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ZNeighbor {
    hwnd: isize,
    visible: bool,
    minimized: bool,
    topmost: bool,
}

/// A janela de referência para devolver uma janela à profundidade de antes: a
/// primeira acima dela que **aparece na tela** (visível e não minimizada).
/// Janela "sempre no topo" não serve — pôr outra janela embaixo dela a faria
/// sempre no topo também (a mesma armadilha que o Nudge Nest anota) —, e aí a
/// resposta é `0`: a janela já estava no topo das janelas comuns.
fn pick_window_above(chain: impl IntoIterator<Item = ZNeighbor>) -> isize {
    for neighbor in chain {
        if !neighbor.visible || neighbor.minimized {
            continue;
        }
        return if neighbor.topmost { 0 } else { neighbor.hwnd };
    }
    0
}

/// A janela logo acima de `hwnd` (ver `pick_window_above`), ou nula quando ela
/// está no topo. Só lê a ordem das janelas.
pub fn window_above(hwnd: HWND) -> HWND {
    if hwnd.is_null() {
        return std::ptr::null_mut();
    }
    let chain = std::iter::successors(Some(hwnd), |current| {
        let previous = unsafe { GetWindow(*current, GW_HWNDPREV) };
        (!previous.is_null()).then_some(previous)
    })
    .skip(1)
    .take(WINDOW_ABOVE_MAX_STEPS)
    .map(|candidate| ZNeighbor {
        hwnd: candidate as isize,
        visible: unsafe { IsWindowVisible(candidate) } != 0,
        minimized: window_is_minimized(candidate),
        topmost: (unsafe { GetWindowLongW(candidate, GWL_EXSTYLE) } as u32 & WS_EX_TOPMOST) != 0,
    });
    pick_window_above(chain) as HWND
}

/// Devolve à barra de tarefas, sem ativar janela nenhuma, a janela que o ciclo
/// desminimizou. Diz se ela ficou minimizada.
pub fn minimize_window_quietly(hwnd: HWND) -> bool {
    if !window_exists(hwnd) {
        return false;
    }
    unsafe {
        // O retorno do `ShowWindow` é "estava visível", não "deu certo".
        let _ = ShowWindow(hwnd, QUIET_MINIMIZE);
    }
    window_is_minimized(hwnd)
}

/// Põe `hwnd` logo abaixo de `above` na pilha, sem ativar, mover nem
/// redimensionar nada.
pub fn put_window_below(hwnd: HWND, above: HWND) -> bool {
    if !window_exists(hwnd) || !window_exists(above) {
        return false;
    }
    unsafe { SetWindowPos(hwnd, above, 0, 0, 0, 0, PUT_BACK_SWP_FLAGS) != 0 }
}

fn wait_for_process_exit(pid: u32, timeout: Duration) -> bool {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        if !get_roblox_pids().contains(&pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    !get_roblox_pids().contains(&pid)
}

fn is_roblox_pid_alive(pid: u32) -> bool {
    get_roblox_pids().contains(&pid)
}

pub fn get_foreground_hwnd() -> HWND {
    unsafe { GetForegroundWindow() }
}

pub fn get_window_title(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        let read = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if read > 0 {
            String::from_utf16_lossy(&buf[..read as usize])
        } else {
            String::new()
        }
    }
}

/// A janela está "Não respondendo" (o Windows diz isso de quem passou 5 s sem
/// tratar mensagens). Só pergunta ao sistema: não manda nada para a janela.
pub fn is_window_hung(hwnd: HWND) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::IsHungAppWindow;
    unsafe { IsHungAppWindow(hwnd) != 0 }
}

/// Troca o título da janela de um cliente (nome da conta na barra de tarefas,
/// `commands/client_health.rs`). `WM_SETTEXT` com teto de 1 s e desistindo se
/// a janela estiver travada: um cliente "Não respondendo" nunca prende o app.
pub fn set_window_title(hwnd: HWND, title: &str) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, SMTO_ABORTIFHUNG, WM_SETTEXT,
    };
    let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let mut result: usize = 0;
    unsafe {
        SendMessageTimeoutW(
            hwnd,
            WM_SETTEXT,
            0,
            wide.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            1000,
            &mut result,
        ) != 0
            && result != 0
    }
}

// ── tamanho da janela imposto pelo PID, depois do spawn ────────────────────
//
// O `StartScreenSize` do XML é compartilhado por todos os clientes (ver
// `spawn_client_window_enforcement`, em `commands/launch_shared.rs`). Depois que
// a janela do PID novo aparece, ela recebe o tamanho resolvido para a conta
// dela — o mesmo que o XML deveria ter dado.
//
// O que o `StartScreenSize` mede (área cliente ou janela inteira, com borda e
// barra de título) não está documentado pelo Roblox. Em vez de chutar, a
// primeira olhada em cada janela aprende: o patch acabou de gravar o tamanho
// desta conta, então se a janela nasceu com a área cliente igual a ele, o
// sentido é área cliente; se nasceu com o retângulo inteiro igual, é janela
// inteira. O aprendido vale para as próximas janelas (inclusive as que
// nasceram com o tamanho errado, que não ensinam nada). Antes de aprender, o
// padrão é área cliente — o que um jogo costuma querer dizer com "tamanho da
// tela". A borda é medida na própria janela (GetWindowRect − GetClientRect),
// então DPI e estilo de janela entram sozinhos.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SizeSemantics {
    /// O tamanho é o da área cliente; a borda vai por fora.
    Client = 1,
    /// O tamanho é o do retângulo inteiro da janela.
    Outer = 2,
}

impl SizeSemantics {
    const UNKNOWN: u8 = 0;

    fn from_u8(value: u8) -> Self {
        if value == SizeSemantics::Outer as u8 {
            SizeSemantics::Outer
        } else {
            SizeSemantics::Client
        }
    }
}

static LEARNED_SIZE_SEMANTICS: std::sync::atomic::AtomicU8 =
    std::sync::atomic::AtomicU8::new(SizeSemantics::UNKNOWN);

fn learned_size_semantics() -> SizeSemantics {
    SizeSemantics::from_u8(LEARNED_SIZE_SEMANTICS.load(Ordering::Relaxed))
}

/// O sentido que uma janela recém-aberta revela, comparando o tamanho que o
/// XML pediu com o retângulo e a área cliente dela. `None` quando nenhum dos
/// dois bate (o XML tinha o tamanho de outra conta).
fn infer_size_semantics(
    target: (i32, i32),
    outer: (i32, i32),
    client: (i32, i32),
) -> Option<SizeSemantics> {
    if client == target {
        Some(SizeSemantics::Client)
    } else if outer == target {
        Some(SizeSemantics::Outer)
    } else {
        None
    }
}

/// O retângulo inteiro que dá à janela o tamanho pedido.
fn outer_size_for_target(
    target: (i32, i32),
    semantics: SizeSemantics,
    outer: (i32, i32),
    client: (i32, i32),
) -> (i32, i32) {
    match semantics {
        SizeSemantics::Outer => target,
        SizeSemantics::Client => (
            target.0 + (outer.0 - client.0).max(0),
            target.1 + (outer.1 - client.1).max(0),
        ),
    }
}

/// Em que estado a janela está.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMode {
    Normal,
    /// Maximizada, com barra de título.
    Maximized,
    /// Sem barra de título cobrindo o monitor inteiro: a tela cheia do Roblox.
    Fullscreen,
}

fn window_mode(
    zoomed: bool,
    style: u32,
    rect: (i32, i32, i32, i32),
    monitor: (i32, i32, i32, i32),
) -> WindowMode {
    let captionless = style & WS_CAPTION != WS_CAPTION;
    let covers = rect.0 <= monitor.0
        && rect.1 <= monitor.1
        && rect.0 + rect.2 >= monitor.0 + monitor.2
        && rect.1 + rect.3 >= monitor.1 + monitor.3;
    if captionless && covers {
        WindowMode::Fullscreen
    } else if zoomed {
        WindowMode::Maximized
    } else {
        WindowMode::Normal
    }
}

/// Maximizada ou em tela cheia: sem um plano que diga o contrário, o usuário
/// pediu, e mexer desfaria.
fn is_fullscreen_like(
    zoomed: bool,
    style: u32,
    rect: (i32, i32, i32, i32),
    monitor: (i32, i32, i32, i32),
) -> bool {
    window_mode(zoomed, style, rect, monitor) != WindowMode::Normal
}

// ── sair da tela cheia herdada ─────────────────────────────────────────────
//
// O `Fullscreen` do XML também é um arquivo para todos: o cliente da conta
// principal com a exceção "Tela: cheia" o regrava, e a alt sem exceção nascia
// em tela cheia (03/10/2026). Quando o plano da alt diz "em janela"
// (`leave_fullscreen`), a janela dela é tirada da tela cheia **sem foco e sem
// tecla**: o estilo de janela comum volta (`WS_OVERLAPPEDWINDOW` no lugar do
// `WS_POPUP`) e a janela recebe o tamanho do plano, centralizada na área de
// trabalho do monitor em que estava. Mandar F11 exigiria trazer a janela para
// frente a cada alt (o `SendInput` do `input.rs` só entrega na janela em
// primeiro plano) — roubaria o foco de quem está jogando na principal.
//
// Uma tentativa só: se o Roblox voltar a janela para a tela cheia, o app não
// briga com ele (a conferência seguinte vê a tela cheia e para).

/// O plano pede "em janela", a janela está na tela cheia do Roblox e ainda não
/// se tentou tirá-la. Maximizada não conta: não é a tela cheia herdada.
pub fn should_leave_fullscreen(requested: bool, mode: WindowMode, already_tried: bool) -> bool {
    requested && mode == WindowMode::Fullscreen && !already_tried
}

/// A grade vai pôr a janela numa célula, mas ela nasceu maximizada (o Roblox
/// abre como foi fechado: quem joga maximizado pelo site tem
/// `StartMaximized=true`). Sem isto a grade desistia dela (relato do Discord,
/// reproduzido em 25/10/2026: as janelas ficavam maximizadas no monitor
/// principal). Uma vez só: se maximizar de novo depois, o app não briga.
pub fn should_leave_maximized(grid: bool, mode: WindowMode, already_tried: bool) -> bool {
    grid && mode == WindowMode::Maximized && !already_tried
}

/// Tira a janela de maximizada sem ativá-la (volta ao tamanho normal dela).
pub fn leave_maximized(hwnd: HWND) -> bool {
    if hwnd.is_null() || unsafe { IsZoomed(hwnd) } == 0 {
        return false;
    }
    unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
    (unsafe { IsZoomed(hwnd) }) == 0
}

/// O estilo da janela comum a partir do da tela cheia: sai o `WS_POPUP`, volta
/// a moldura inteira (barra de título, borda, botões); o resto fica.
fn windowed_style(style: u32) -> u32 {
    (style & !WS_POPUP) | WS_OVERLAPPEDWINDOW
}

/// O retângulo da janela ao sair da tela cheia. `work`: área de trabalho do
/// monitor; `frame`: a moldura que o estilo comum acrescenta (largura,
/// altura). Sem tamanho no plano, um tamanho modesto (1280x720, ou 2/3 da área
/// de trabalho se ela for menor). Nunca maior que a área de trabalho.
fn exit_fullscreen_rect(
    work: (i32, i32, i32, i32),
    size: Option<(u32, u32)>,
    semantics: SizeSemantics,
    frame: (i32, i32),
) -> (i32, i32, i32, i32) {
    let target = match size {
        Some((w, h)) => (w as i32, h as i32),
        None => ((work.2 * 2 / 3).min(1280), (work.3 * 2 / 3).min(720)),
    };
    let (w, h) = match semantics {
        SizeSemantics::Client => (target.0 + frame.0.max(0), target.1 + frame.1.max(0)),
        SizeSemantics::Outer => target,
    };
    let (w, h) = (w.min(work.2).max(1), h.min(work.3).max(1));
    (work.0 + (work.2 - w) / 2, work.1 + (work.3 - h) / 2, w, h)
}

/// O resultado de tentar tirar a janela da tela cheia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullscreenExit {
    /// A janela não estava na tela cheia do Roblox: nada feito.
    NotFullscreen,
    /// Saiu: a janela está em modo janela, no retângulo devolvido.
    Left((i32, i32, i32, i32)),
    /// O Windows recusou, ou a janela continua cobrindo o monitor.
    Refused,
}

fn monitor_info_of(hwnd: HWND) -> Option<MONITORINFO> {
    unsafe {
        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if hmon.is_null() {
            return None;
        }
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(hmon, &mut mi) == 0 {
            return None;
        }
        Some(mi)
    }
}

/// O estado da janela agora; `None` se ela sumiu.
pub fn window_mode_of(hwnd: HWND) -> Option<WindowMode> {
    if hwnd.is_null() {
        return None;
    }
    current_window_mode(hwnd)
}

fn current_window_mode(hwnd: HWND) -> Option<WindowMode> {
    let rect = get_window_position(hwnd)?;
    let zoomed = unsafe { IsZoomed(hwnd) != 0 };
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    let monitor = monitor_rect_of(hwnd)?;
    Some(window_mode(zoomed, style, rect, monitor))
}

/// A moldura que `style` acrescenta em volta da área cliente.
fn frame_for_style(hwnd: HWND, style: u32) -> (i32, i32) {
    let ex_style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if unsafe { AdjustWindowRectEx(&mut rect, style, 0, ex_style) } == 0 {
        return (0, 0);
    }
    (rect.right - rect.left, rect.bottom - rect.top)
}

/// Tira da tela cheia do Roblox uma janela cujo plano é "em janela", sem
/// ativá-la nem mudar a ordem das janelas. Ver o bloco acima.
pub fn leave_fullscreen(hwnd: HWND, size: Option<(u32, u32)>) -> FullscreenExit {
    if hwnd.is_null() || window_is_minimized(hwnd) {
        return FullscreenExit::NotFullscreen;
    }
    if current_window_mode(hwnd) != Some(WindowMode::Fullscreen) {
        return FullscreenExit::NotFullscreen;
    }
    let Some(info) = monitor_info_of(hwnd) else {
        return FullscreenExit::Refused;
    };
    let w = info.rcWork;
    let work = (w.left, w.top, w.right - w.left, w.bottom - w.top);

    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    let windowed = windowed_style(style);
    let rect = exit_fullscreen_rect(work, size, learned_size_semantics(), frame_for_style(hwnd, windowed));
    unsafe {
        if IsZoomed(hwnd) != 0 {
            // Popup maximizado: volta ao estado normal sem ativar.
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        SetWindowLongW(hwnd, GWL_STYLE, windowed as i32);
        let moved = SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            rect.0,
            rect.1,
            rect.2,
            rect.3,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_FRAMECHANGED,
        ) != 0;
        if !moved {
            return FullscreenExit::Refused;
        }
    }
    match current_window_mode(hwnd) {
        Some(WindowMode::Fullscreen) | None => FullscreenExit::Refused,
        Some(_) => FullscreenExit::Left(get_window_position(hwnd).unwrap_or(rect)),
    }
}

/// Para onde a janela vai: a posição salva (ou a atual) e o tamanho do plano
/// (ou o salvo, ou o atual).
fn client_window_target(
    current: (i32, i32, i32, i32),
    client: (i32, i32),
    size: Option<(u32, u32)>,
    saved_rect: Option<(i32, i32, i32, i32)>,
    semantics: SizeSemantics,
) -> (i32, i32, i32, i32) {
    let (x, y) = saved_rect.map(|r| (r.0, r.1)).unwrap_or((current.0, current.1));
    let (w, h) = match size {
        Some((tw, th)) => outer_size_for_target(
            (tw as i32, th as i32),
            semantics,
            (current.2, current.3),
            client,
        ),
        None => saved_rect
            .map(|r| (r.2, r.3))
            .unwrap_or((current.2, current.3)),
    };
    (x, y, w, h)
}

fn client_area_size(hwnd: HWND) -> Option<(i32, i32)> {
    unsafe {
        let mut rect: RECT = std::mem::zeroed();
        if GetClientRect(hwnd, &mut rect) == 0 {
            return None;
        }
        Some((rect.right - rect.left, rect.bottom - rect.top))
    }
}

fn monitor_rect_of(hwnd: HWND) -> Option<(i32, i32, i32, i32)> {
    let r = monitor_info_of(hwnd)?.rcMonitor;
    Some((r.left, r.top, r.right - r.left, r.bottom - r.top))
}

fn window_is_fullscreen_like(hwnd: HWND, rect: (i32, i32, i32, i32)) -> bool {
    let zoomed = unsafe { IsZoomed(hwnd) != 0 };
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    match monitor_rect_of(hwnd) {
        Some(monitor) => is_fullscreen_like(zoomed, style, rect, monitor),
        None => zoomed,
    }
}

/// Move/redimensiona sem roubar o foco nem mudar a ordem das janelas.
fn place_window(hwnd: HWND, rect: (i32, i32, i32, i32)) -> bool {
    place_window_with(hwnd, rect, grid_swp_flags(false))
}

fn place_window_with(hwnd: HWND, rect: (i32, i32, i32, i32), flags: u32) -> bool {
    unsafe { SetWindowPos(hwnd, std::ptr::null_mut(), rect.0, rect.1, rect.2, rect.3, flags) != 0 }
}

pub fn is_roblox_pid_running(pid: u32) -> bool {
    is_roblox_pid_alive(pid)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowEnforcement {
    /// Minimizada, maximizada ou em tela cheia (ou a janela sumiu): fica como
    /// está, e a conferência para por aqui.
    Skipped,
    /// A janela está (ou já estava) neste retângulo.
    Placed((i32, i32, i32, i32)),
}

/// A janela num estado em que dá para mexer: o retângulo e a área cliente
/// dela, ou `None` se está minimizada, maximizada ou em tela cheia.
fn movable_window(hwnd: HWND) -> Option<((i32, i32, i32, i32), (i32, i32))> {
    if hwnd.is_null() || window_is_minimized(hwnd) {
        return None;
    }
    let current = get_window_position(hwnd)?;
    let client = client_area_size(hwnd)?;
    if window_is_fullscreen_like(hwnd, current) {
        return None;
    }
    Some((current, client))
}

/// Primeira olhada na janela: o tamanho dela ainda é o que o XML deu, e é
/// dela que se aprende o sentido do `StartScreenSize`.
fn learn_size_semantics(
    size: Option<(u32, u32)>,
    current: (i32, i32, i32, i32),
    client: (i32, i32),
) {
    if let Some((tw, th)) = size {
        if let Some(learned) =
            infer_size_semantics((tw as i32, th as i32), (current.2, current.3), client)
        {
            LEARNED_SIZE_SEMANTICS.store(learned as u8, Ordering::Relaxed);
        }
    }
}

fn move_to(hwnd: HWND, current: (i32, i32, i32, i32), target: (i32, i32, i32, i32)) -> WindowEnforcement {
    move_to_with(hwnd, current, target, false)
}

/// `allow_small`: ver `grid_swp_flags`.
fn move_to_with(
    hwnd: HWND,
    current: (i32, i32, i32, i32),
    target: (i32, i32, i32, i32),
    allow_small: bool,
) -> WindowEnforcement {
    if target == current || place_window_with(hwnd, target, grid_swp_flags(allow_small)) {
        WindowEnforcement::Placed(target)
    } else {
        WindowEnforcement::Skipped
    }
}

/// Aplica o plano numa janela fora da grade. `first_look`: é a primeira vez
/// que o app olha para esta janela (ver `learn_size_semantics`).
pub fn enforce_client_window(
    hwnd: HWND,
    size: Option<(u32, u32)>,
    saved_rect: Option<(i32, i32, i32, i32)>,
    first_look: bool,
) -> WindowEnforcement {
    let Some((current, client)) = movable_window(hwnd) else {
        return WindowEnforcement::Skipped;
    };
    if first_look {
        learn_size_semantics(size, current, client);
    }
    let target = client_window_target(current, client, size, saved_rect, learned_size_semantics());
    move_to(hwnd, current, target)
}

/// Devolve a janela ao retângulo já escolhido, se o cliente a tiver mexido.
/// `allow_small`: a célula é menor que o mínimo do Roblox (grade com
/// `GridAllowSmallWindows`) — sem isso a conferência devolveria o tamanho
/// mínimo que ela acabou de tirar.
pub fn hold_window_rect(hwnd: HWND, rect: (i32, i32, i32, i32), allow_small: bool) -> WindowEnforcement {
    let Some((current, _)) = movable_window(hwnd) else {
        return WindowEnforcement::Skipped;
    };
    move_to_with(hwnd, current, rect, allow_small)
}

// ── Grid layout of Roblox windows across monitors ──────────────────────────
//
// Ported from the Robeats "calibração" grid feature: enumerate physical
// monitors, normalize every Roblox window to the most common size, then place
// them cell-by-cell (left→right, top→bottom) across the selected monitors with
// a gap. A cell is only used if the window fits entirely inside the monitor;
// when a monitor runs out of cells, the next one is used.
//
// Grade por células fixas (03/10/2026): as células ("slots") são todas as que
// cabem nos monitores escolhidos, na ordem acima. Uma janela ocupa a célula em
// que está o centro dela. A janela nova (grade automática no launch) pega a
// primeira célula livre, e nenhuma janela já aberta se mexe — o usuário pode
// estar jogando nela. Grade cheia: dá a volta e sobrepõe a partir da célula 0
// (a menos ocupada, empate na de menor índice). O botão manual usa as mesmas
// células, na ordem das janelas, e também dá a volta. Janelas de contas com
// tamanho próprio (exceção de launch) ficam fora dos dois: nem são movidas,
// nem contam como ocupando célula.

#[derive(Clone, Serialize)]
pub struct MonitorInfoDto {
    pub index: usize, // 1-based, in left→top order
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub primary: bool,
}

#[derive(Clone, Copy)]
pub struct RobloxWin {
    pub hwnd: isize,
    pub pid: u32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GridCell {
    x: i32,
    y: i32,
}

struct MonitorCollect {
    list: Vec<(RECT, bool)>,
}

unsafe extern "system" fn monitor_enum_cb(
    hmon: HMONITOR,
    _hdc: HDC,
    _rc: *mut RECT,
    lparam: isize,
) -> i32 {
    let data = &mut *(lparam as *mut MonitorCollect);
    let mut mi: MONITORINFO = std::mem::zeroed();
    mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(hmon, &mut mi) != 0 {
        let primary = (mi.dwFlags & MONITORINFOF_PRIMARY) != 0;
        // Use the work area (rcWork) so windows don't hide behind the taskbar.
        data.list.push((mi.rcWork, primary));
    }
    1
}

/// Physical monitors (work areas), ordered left→top, 1-based index.
pub fn list_monitors() -> Vec<MonitorInfoDto> {
    let mut data = MonitorCollect { list: Vec::new() };
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(monitor_enum_cb),
            &mut data as *mut _ as isize,
        );
    }
    data.list.sort_by_key(|(r, _)| (r.left, r.top));
    data.list
        .into_iter()
        .enumerate()
        .map(|(i, (r, primary))| MonitorInfoDto {
            index: i + 1,
            x: r.left,
            y: r.top,
            width: r.right - r.left,
            height: r.bottom - r.top,
            primary,
        })
        .collect()
}

struct RobloxWinCollect {
    pids: std::collections::HashSet<u32>,
    out: Vec<RobloxWin>,
}

unsafe extern "system" fn roblox_win_cb(hwnd: HWND, lparam: isize) -> i32 {
    let data = &mut *(lparam as *mut RobloxWinCollect);
    if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
        return 1;
    }
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if !data.pids.contains(&pid) {
        return 1;
    }
    let mut rect: RECT = std::mem::zeroed();
    if GetWindowRect(hwnd, &mut rect) == 0 {
        return 1;
    }
    let w = rect.right - rect.left;
    let h = rect.bottom - rect.top;
    if w < 200 || h < 200 {
        return 1;
    }
    data.out.push(RobloxWin {
        hwnd: hwnd as isize,
        pid,
        x: rect.left,
        y: rect.top,
        w,
        h,
    });
    1
}

/// Janelas de grade: as visíveis e as minimizadas (no retângulo que voltam a
/// ter ao restaurar), em ordem de leitura. Só as visíveis contavam, então uma
/// janela minimizada não ocupava célula nenhuma e as abertas depois caíam em
/// cima dela.
fn grid_windows(mut visible: Vec<RobloxWin>, minimized: Vec<RobloxWin>) -> Vec<RobloxWin> {
    visible.extend(minimized);
    visible.sort_by_key(|w| (w.y, w.x));
    visible
}

/// O `GetWindowPlacement` dá `rcNormalPosition` em coordenadas da **área de
/// trabalho**, cuja origem é o canto da área útil do monitor principal — não o
/// da tela. Com a barra de tarefas no topo ou à esquerda, as duas diferem.
/// `work` = canto da área útil, `monitor` = canto do monitor.
fn workspace_offset(work: (i32, i32), monitor: (i32, i32)) -> (i32, i32) {
    (work.0 - monitor.0, work.1 - monitor.1)
}

fn workspace_to_screen(rect: (i32, i32, i32, i32), offset: (i32, i32)) -> (i32, i32, i32, i32) {
    (rect.0 + offset.0, rect.1 + offset.1, rect.2, rect.3)
}

fn screen_to_workspace(rect: (i32, i32, i32, i32), offset: (i32, i32)) -> (i32, i32, i32, i32) {
    (rect.0 - offset.0, rect.1 - offset.1, rect.2, rect.3)
}

/// O deslocamento da área de trabalho do monitor principal.
fn primary_workspace_offset() -> (i32, i32) {
    unsafe {
        let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return (0, 0);
        }
        workspace_offset(
            (info.rcWork.left, info.rcWork.top),
            (info.rcMonitor.left, info.rcMonitor.top),
        )
    }
}

struct MinimizedCollect {
    pids: std::collections::HashSet<u32>,
    offset: (i32, i32),
    out: Vec<RobloxWin>,
}

unsafe extern "system" fn minimized_win_cb(hwnd: HWND, lparam: isize) -> i32 {
    let data = &mut *(lparam as *mut MinimizedCollect);
    if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) == 0 {
        return 1;
    }
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if !data.pids.contains(&pid) {
        return 1;
    }
    let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
    placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
    if GetWindowPlacement(hwnd, &mut placement) == 0 {
        return 1;
    }
    let r = placement.rcNormalPosition;
    let (x, y, w, h) =
        workspace_to_screen((r.left, r.top, r.right - r.left, r.bottom - r.top), data.offset);
    if w < 200 || h < 200 {
        return 1;
    }
    data.out.push(RobloxWin {
        hwnd: hwnd as isize,
        pid,
        x,
        y,
        w,
        h,
    });
    1
}

/// Roblox windows that are minimized, at the rectangle they get back when
/// restored (not the -32000 placeholder of a minimized window).
pub fn list_minimized_roblox_windows() -> Vec<RobloxWin> {
    let mut data = MinimizedCollect {
        pids: get_roblox_pids().into_iter().collect(),
        offset: primary_workspace_offset(),
        out: Vec::new(),
    };
    unsafe {
        EnumWindows(Some(minimized_win_cb), &mut data as *mut _ as isize);
    }
    data.out
}

/// Muda onde uma janela **minimizada** volta ao restaurar, sem restaurá-la nem
/// roubar o foco. `rect` em coordenadas da tela.
fn place_minimized_window(hwnd: HWND, rect: (i32, i32, i32, i32)) -> bool {
    let (x, y, w, h) = screen_to_workspace(rect, primary_workspace_offset());
    unsafe {
        let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
        placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        if GetWindowPlacement(hwnd, &mut placement) == 0 {
            return false;
        }
        placement.rcNormalPosition = RECT {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        placement.showCmd = SW_SHOWMINNOACTIVE as u32;
        // Maximizada e depois minimizada: o Windows a restauraria maximizada,
        // por cima da grade inteira. Organizar em grade a põe na célula.
        placement.flags &= !windows_sys::Win32::UI::WindowsAndMessaging::WPF_RESTORETOMAXIMIZED;
        SetWindowPlacement(hwnd, &placement) != 0
    }
}

/// Visible, non-minimized Roblox windows, ordered top→bottom, left→right.
pub fn list_roblox_windows() -> Vec<RobloxWin> {
    let pids: std::collections::HashSet<u32> = get_roblox_pids().into_iter().collect();
    let mut data = RobloxWinCollect {
        pids,
        out: Vec::new(),
    };
    unsafe {
        EnumWindows(Some(roblox_win_cb), &mut data as *mut _ as isize);
    }
    data.out.sort_by_key(|w| (w.y, w.x));
    data.out
}

/// Most frequent (width, height); ties broken by largest area.
fn most_common_size(wins: &[RobloxWin]) -> (i32, i32) {
    use std::collections::HashMap as Map;
    let mut counts: Map<(i32, i32), usize> = Map::new();
    for w in wins {
        *counts.entry((w.w, w.h)).or_insert(0) += 1;
    }
    let top = counts.values().copied().max().unwrap_or(0);
    counts
        .into_iter()
        .filter(|(_, c)| *c == top)
        .map(|(s, _)| s)
        .max_by_key(|(w, h)| (*w as i64) * (*h as i64))
        .unwrap_or((800, 600))
}

/// Distribute `count` windows of `win_w`×`win_h` into cells across `monitors`.
fn compute_grid(
    monitors: &[MonitorInfoDto],
    win_w: i32,
    win_h: i32,
    count: usize,
    gap: i32,
) -> Vec<GridCell> {
    let mut cells: Vec<GridCell> = Vec::new();
    for mon in monitors {
        let mut row = 0;
        while cells.len() < count {
            let cell_y = mon.y + gap + row * (win_h + gap);
            if cell_y + win_h > mon.y + mon.height {
                break; // row doesn't fit vertically -> monitor exhausted
            }
            let mut col = 0;
            let mut placed_in_row = false;
            while cells.len() < count {
                let cell_x = mon.x + gap + col * (win_w + gap);
                if cell_x + win_w > mon.x + mon.width {
                    break; // column doesn't fit -> end of row
                }
                cells.push(GridCell { x: cell_x, y: cell_y });
                placed_in_row = true;
                col += 1;
            }
            if !placed_in_row {
                break; // not even the first column fits -> monitor useless for this size
            }
            row += 1;
        }
    }
    cells.truncate(count);
    cells
}

/// Teto de células: um monitor gigante com janelas pequenas não gera uma lista
/// sem fim.
const MAX_GRID_SLOTS: usize = 512;

/// Todas as células de `cell_w`×`cell_h` que cabem nos monitores, em ordem.
fn grid_slots(monitors: &[MonitorInfoDto], cell_w: i32, cell_h: i32, gap: i32) -> Vec<GridCell> {
    if cell_w <= 0 || cell_h <= 0 {
        return Vec::new();
    }
    compute_grid(monitors, cell_w, cell_h, MAX_GRID_SLOTS, gap.max(0))
}

/// A célula da grade cruza o retângulo `(x, y, w, h)`?
fn slot_overlaps(slot: &GridCell, cell: (i32, i32), rect: (i32, i32, i32, i32)) -> bool {
    let (x, y, w, h) = rect;
    slot.x < x + w && x < slot.x + cell.0 && slot.y < y + h && y < slot.y + cell.1
}

/// Quantas janelas ocupam cada célula (pelo centro) ou a cobrem por fora.
fn slot_occupancy(
    slots: &[GridCell],
    cell: (i32, i32),
    occupants: &[(i32, i32, i32, i32)],
    blockers: &[(i32, i32, i32, i32)],
) -> Vec<usize> {
    let mut counts = vec![0usize; slots.len()];
    for &(x, y, w, h) in occupants {
        let (cx, cy) = (x + w / 2, y + h / 2);
        if let Some(i) = slots
            .iter()
            .position(|s| cx >= s.x && cx < s.x + cell.0 && cy >= s.y && cy < s.y + cell.1)
        {
            counts[i] += 1;
        }
    }
    for (i, slot) in slots.iter().enumerate() {
        if blockers.iter().any(|&b| slot_overlaps(slot, cell, b)) {
            counts[i] += 1;
        }
    }
    counts
}

/// A grade não tem célula livre: a próxima janela cairia por cima de outra.
fn grid_is_full(
    slots: &[GridCell],
    cell: (i32, i32),
    occupants: &[(i32, i32, i32, i32)],
    blockers: &[(i32, i32, i32, i32)],
) -> bool {
    !slot_occupancy(slots, cell, occupants, blockers).contains(&0)
}

/// A maior célula, na mesma proporção de `cell`, em que cabem `count` janelas
/// sem dar a volta (fora as células cobertas por `blockers`). `cell` mesmo se
/// já cabem; `None` se nem no piso (`MIN_SMALL_GRID_CELL`) cabem.
///
/// Relato do Discord (25/10/2026): com mais janelas do que cabem no monitor no
/// tamanho mínimo do Roblox, a grade dava a volta e empilhava ("they come on
/// top of one another"). Encolher a célula é o que faz todas caberem.
fn cell_that_fits(
    monitors: &[MonitorInfoDto],
    cell: (i32, i32),
    gap: i32,
    count: usize,
    blockers: &[(i32, i32, i32, i32)],
) -> Option<(i32, i32)> {
    let fits = |c: (i32, i32)| -> bool {
        let slots = grid_slots(monitors, c.0, c.1, gap);
        let free = slots.iter().filter(|s| !blockers.iter().any(|&b| slot_overlaps(s, c, b))).count();
        free >= count
    };
    if count == 0 || cell.0 <= 0 || cell.1 <= 0 || fits(cell) {
        return Some(cell);
    }
    let mut w = cell.0;
    while w > MIN_SMALL_GRID_CELL.0 {
        w -= 1;
        let h = ((w as i64 * cell.1 as i64) / cell.0 as i64) as i32;
        if h < MIN_SMALL_GRID_CELL.1 {
            break;
        }
        if fits((w, h)) {
            return Some((w, h));
        }
    }
    None
}

/// Formatos aceitos no "melhor tamanho": deitado, de 4:3 a 16:9 — nem
/// esticado para um lado, nem em pé.
const AUTO_FIT_RATIOS: [(i64, i64); 4] = [(4, 3), (3, 2), (16, 10), (16, 9)];

/// A maior célula, num dos `AUTO_FIT_RATIOS`, em que `count` janelas cabem nos
/// `monitors` com o `gap` (fora as células cobertas por `blockers`). `None` sem
/// nada a arrumar, sem monitor, ou se nem no piso (`MIN_SMALL_GRID_CELL`) cabem.
///
/// A melhor célula sempre encosta numa borda: ou a largura é a de `c` colunas
/// num monitor, ou a altura é a de `r` linhas. Basta testar esses tamanhos, do
/// maior para o menor, em vez de varrer pixel a pixel.
fn best_fit_cell(
    monitors: &[MonitorInfoDto],
    gap: i32,
    count: usize,
    blockers: &[(i32, i32, i32, i32)],
) -> Option<(i32, i32)> {
    if count == 0 || monitors.is_empty() {
        return None;
    }
    let gap = gap.max(0);
    let fits = |c: (i32, i32)| -> bool {
        grid_slots(monitors, c.0, c.1, gap)
            .iter()
            .filter(|s| !blockers.iter().any(|&b| slot_overlaps(s, c, b)))
            .count()
            >= count
    };
    let lines = count.min(MAX_GRID_SLOTS) as i32;
    let mut best: Option<(i32, i32)> = None;
    for &(a, b) in &AUTO_FIT_RATIOS {
        let mut widths: Vec<i32> = Vec::new();
        for m in monitors {
            for n in 1..=lines {
                // `n` colunas: gap + n*w + (n-1)*gap <= largura.
                widths.push((m.width - n * gap) / n);
                // `n` linhas, convertidas em largura pelo formato.
                let h = (m.height - n * gap) / n;
                widths.push(((h as i64 * a) / b) as i32);
            }
        }
        widths.sort_unstable_by(|x, y| y.cmp(x));
        widths.dedup();
        for w in widths {
            let h = ((w as i64 * b) / a) as i32;
            if w < MIN_SMALL_GRID_CELL.0 || h < MIN_SMALL_GRID_CELL.1 {
                break;
            }
            if fits((w, h)) {
                let area = |c: (i32, i32)| c.0 as i64 * c.1 as i64;
                if best.map_or(true, |cur| area((w, h)) > area(cur)) {
                    best = Some((w, h));
                }
                break;
            }
        }
    }
    best
}

/// A célula para a janela nova: a menos ocupada, empate na de menor índice —
/// ou seja, a primeira livre; com a grade cheia, dá a volta a partir da 0.
/// `occupants`: retângulos `(x, y, w, h)` das janelas da grade já abertas; uma
/// janela ocupa a célula em que está o centro dela.
/// `blockers`: janelas fora da grade (contas com tamanho próprio, como a
/// principal). Elas não se mexem, então toda célula que elas cobrem conta como
/// ocupada — senão a alt nasce por cima da principal.
fn pick_grid_slot(
    slots: &[GridCell],
    cell: (i32, i32),
    occupants: &[(i32, i32, i32, i32)],
    blockers: &[(i32, i32, i32, i32)],
) -> Option<usize> {
    let counts = slot_occupancy(slots, cell, occupants, blockers);
    counts
        .iter()
        .enumerate()
        .min_by_key(|(i, c)| (**c, *i))
        .map(|(i, _)| i)
}

/// Células que nenhuma janela fora da grade cobre (o botão manual usa só
/// estas). Se todas estiverem cobertas, devolve a grade inteira.
fn unblocked_slots(slots: &[GridCell], cell: (i32, i32), blockers: &[(i32, i32, i32, i32)]) -> Vec<GridCell> {
    let free: Vec<GridCell> = slots
        .iter()
        .copied()
        .filter(|s| !blockers.iter().any(|&b| slot_overlaps(s, cell, b)))
        .collect();
    if free.is_empty() {
        slots.to_vec()
    } else {
        free
    }
}

/// O tamanho da célula: o pedido, ou o que a janela de fato ficou se ela não
/// encolheu até lá. O Roblox tem tamanho mínimo (~800x600 de área útil): um
/// 520x420 na configuração vira 816x638, e células de 520 sobrepõem as janelas.
///
/// Com `allow_small` (`GridAllowSmallWindows`) a janela é posta sem o aviso de
/// "vou mudar de tamanho" (`SWP_NOSENDCHANGING`), que é onde o Roblox impõe o
/// mínimo: a célula fica do tamanho pedido, com um piso de 200x200 (janela
/// menor que isso a grade nem enxerga, ver `roblox_win_cb`).
fn grid_cell_size(requested: (i32, i32), actual: (i32, i32), allow_small: bool) -> (i32, i32) {
    if allow_small {
        return (
            requested.0.max(MIN_SMALL_GRID_CELL.0),
            requested.1.max(MIN_SMALL_GRID_CELL.1),
        );
    }
    (requested.0.max(actual.0), requested.1.max(actual.1))
}

// ── grade menor que o mínimo e sem moldura (ideia 22) ──────────────────────
//
// Duas opções da grade, desligadas por padrão, só para clientes que o app
// abriu (rastreados e não adotados):
// - `General.GridAllowSmallWindows`: célula menor que o mínimo do Roblox. A
//   janela é posta com `SWP_NOSENDCHANGING` — o `WM_WINDOWPOSCHANGING` não é
//   enviado e o Roblox não tem onde aumentar o tamanho. Só constante nova, a
//   mesma `SetWindowPos` de sempre.
// - `General.GridBorderless`: tira a barra de título e a borda
//   (`WS_CAPTION | WS_THICKFRAME`) e as janelas encostam. A moldura original
//   fica guardada e volta quando a opção desliga (olhada de 1 s), quando o
//   botão manual arruma sem ela, ou ao fechar o app.

/// Piso da célula com `allow_small`.
const MIN_SMALL_GRID_CELL: (i32, i32) = (200, 200);

/// Os bits de moldura que a grade sem moldura tira e devolve.
const GRID_FRAME_BITS: u32 = WS_CAPTION | WS_THICKFRAME;

/// Como a grade trata as janelas que o app abriu.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GridWindowStyle {
    /// `General.GridAllowSmallWindows`.
    pub allow_small: bool,
    /// `General.GridBorderless`.
    pub borderless: bool,
    /// `General.GridAutoFit`: a célula é a maior em que todas as janelas cabem
    /// nos monitores escolhidos (`best_fit_cell`), não o tamanho configurado.
    pub auto_fit: bool,
}

impl GridWindowStyle {
    /// Célula menor que o mínimo do Roblox: pedida, ou implícita no "melhor
    /// tamanho" (sem ela a conta dá células que o Roblox cresce de volta).
    pub fn small_cells(&self) -> bool {
        self.allow_small || self.auto_fit
    }
}

/// Os flags do `SetWindowPos` da grade: nunca rouba o foco nem mexe na ordem;
/// com `allow_small`, também não avisa a janela (ver acima).
fn grid_swp_flags(allow_small: bool) -> u32 {
    let base = SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
    if allow_small {
        base | SWP_NOSENDCHANGING
    } else {
        base
    }
}

/// O estilo sem moldura: sai a barra de título e a borda; o resto fica (o
/// botão da barra de tarefas continua minimizando).
fn borderless_style(style: u32) -> u32 {
    style & !GRID_FRAME_BITS
}

/// O estilo com a moldura devolvida: só os bits de moldura que a janela tinha
/// antes voltam — uma janela que nunca teve borda não ganha uma.
fn restored_frame_style(current: u32, original: u32) -> u32 {
    current | (original & GRID_FRAME_BITS)
}

/// Janelas sem moldura pela grade: `hwnd -> (pid, estilo original)`.
static BORDERLESS_WINDOWS: LazyLock<Mutex<HashMap<isize, (u32, u32)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn borderless_windows() -> std::sync::MutexGuard<'static, HashMap<isize, (u32, u32)>> {
    BORDERLESS_WINDOWS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Aplica um estilo e manda a janela redesenhar a moldura, sem mover, sem
/// redimensionar e sem foco.
fn apply_frame_style(hwnd: HWND, style: u32) {
    unsafe {
        SetWindowLongW(hwnd, GWL_STYLE, style as i32);
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE
                | SWP_NOSIZE
                | SWP_NOZORDER
                | SWP_NOACTIVATE
                | SWP_NOOWNERZORDER
                | SWP_FRAMECHANGED,
        );
    }
}

/// Tira a moldura da janela de um cliente do app e guarda a original.
fn make_borderless(hwnd: HWND, pid: u32) {
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    let bare = borderless_style(style);
    if bare == style {
        return;
    }
    borderless_windows()
        .entry(hwnd as isize)
        .or_insert((pid, style));
    apply_frame_style(hwnd, bare);
}

/// Devolve a moldura a uma janela que a grade tirou (se ela ainda é do mesmo
/// cliente).
fn restore_border(hwnd: isize, pid: u32, original: u32) {
    let hwnd = hwnd as HWND;
    if window_pid(hwnd) != Some(pid) {
        return;
    }
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    let framed = restored_frame_style(style, original);
    if framed != style {
        apply_frame_style(hwnd, framed);
    }
}

/// Devolve a moldura a todas as janelas que a grade tirou.
pub fn release_borderless_windows() {
    let taken: Vec<(isize, (u32, u32))> = borderless_windows().drain().collect();
    for (hwnd, (pid, original)) in taken {
        restore_border(hwnd, pid, original);
    }
}

/// Devolve a moldura às janelas destes PIDs (botão manual sem a opção).
fn release_borderless_for(pids: &std::collections::HashSet<u32>) {
    let taken: Vec<(isize, (u32, u32))> = {
        let mut map = borderless_windows();
        let hwnds: Vec<isize> = map
            .iter()
            .filter(|(_, (pid, _))| pids.contains(pid))
            .map(|(hwnd, _)| *hwnd)
            .collect();
        hwnds
            .into_iter()
            .filter_map(|hwnd| map.remove(&hwnd).map(|v| (hwnd, v)))
            .collect()
    };
    for (hwnd, (pid, original)) in taken {
        restore_border(hwnd, pid, original);
    }
}

/// A célula que a grade deu a cada janela de cliente aberto pelo app, e se a
/// janela estava minimizada na última olhada. Ao restaurar uma minimizada o
/// Roblox impõe o mínimo (~802x631) por cima da célula — quando a célula é
/// menor que isso, as janelas voltavam umas por cima das outras (bateria de
/// testes de 25/10/2026). Vale tanto para a que estava minimizada na hora de
/// organizar quanto para a que a pessoa minimizou depois.
#[derive(Debug, Clone, Copy)]
struct GridCellMemory {
    rect: (i32, i32, i32, i32),
    was_minimized: bool,
}

static GRID_CELLS: std::sync::LazyLock<Mutex<HashMap<isize, GridCellMemory>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// `Some(rect)` anota a célula da janela (`minimized`: se ela está minimizada
/// agora); `None` esquece (a grade a deixou fora).
fn remember_grid_cell(hwnd: isize, rect: Option<(i32, i32, i32, i32)>, minimized: bool) {
    let mut map = GRID_CELLS.lock().unwrap_or_else(|e| e.into_inner());
    match rect {
        Some(rect) => {
            map.insert(hwnd, GridCellMemory { rect, was_minimized: minimized });
        }
        None => {
            map.remove(&hwnd);
        }
    }
}

/// Janela em que o Windows impõe o tamanho mínimo (o `WM_GETMINMAXINFO` só vale
/// para janela redimensionável): moldura normal, sem estar maximizada nem em
/// tela cheia.
fn window_enforces_min_size(hwnd: HWND, rect: (i32, i32, i32, i32)) -> bool {
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    frame_enforces_min_size(style) && unsafe { IsZoomed(hwnd) } == 0 && !window_is_fullscreen_like(hwnd, rect)
}

fn frame_enforces_min_size(style: u32) -> bool {
    style & WS_THICKFRAME != 0 && style & WS_POPUP == 0
}

/// O que a olhada faz com uma janela anotada.
#[derive(Debug, PartialEq, Eq)]
enum GridCellStep {
    /// Fechou: sai da lista.
    Forget,
    /// Minimizada: anota, para pegar a volta.
    MarkMinimized,
    /// Acabou de voltar de minimizada, na própria célula: vai para a célula
    /// no tamanho dela.
    Snap,
    /// Nada a fazer (maximizada, em tela cheia, arrastada pela pessoa...).
    Keep,
}

/// `at_cell`: o canto da janela está no canto da célula — é onde ela volta ao
/// restaurar. Janela que a pessoa arrastou para outro lugar não é tocada, e
/// janela que não estava minimizada também não: quem a redimensionou no canto
/// foi a pessoa, e a olhada não briga com ela.
fn grid_cell_step(exists: bool, minimized: bool, maximized: bool, was_minimized: bool, at_cell: bool) -> GridCellStep {
    if !exists {
        GridCellStep::Forget
    } else if minimized {
        GridCellStep::MarkMinimized
    } else if was_minimized && !maximized && at_cell {
        GridCellStep::Snap
    } else {
        GridCellStep::Keep
    }
}

/// Tolerância do "no canto da célula", em pixels.
const AT_CELL_TOLERANCE: i32 = 2;

fn is_at_cell(current: Option<(i32, i32, i32, i32)>, cell: (i32, i32, i32, i32)) -> bool {
    current.is_some_and(|r| (r.0 - cell.0).abs() <= AT_CELL_TOLERANCE && (r.1 - cell.1).abs() <= AT_CELL_TOLERANCE)
}

/// A olhada (a cada 250 ms, `start_grid_snap_loop`): a janela anotada que
/// voltou de minimizada vai para a célula sem o aviso de tamanho, como as
/// visíveis.
pub fn snap_restored_small_windows() {
    let entries: Vec<(isize, GridCellMemory)> = {
        let map = GRID_CELLS.lock().unwrap_or_else(|e| e.into_inner());
        if map.is_empty() {
            return;
        }
        map.iter().map(|(h, m)| (*h, *m)).collect()
    };
    let _guard = GRID_PLACEMENT.lock().unwrap_or_else(|e| e.into_inner());
    for (raw, memory) in entries {
        let hwnd = raw as HWND;
        let (exists, minimized, maximized) = unsafe {
            (
                windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(hwnd) != 0,
                IsIconic(hwnd) != 0,
                IsZoomed(hwnd) != 0,
            )
        };
        let at_cell = exists && !minimized && is_at_cell(get_window_position(hwnd), memory.rect);
        let step = grid_cell_step(exists, minimized, maximized, memory.was_minimized, at_cell);
        let mut map = GRID_CELLS.lock().unwrap_or_else(|e| e.into_inner());
        match step {
            GridCellStep::Forget => {
                map.remove(&raw);
            }
            GridCellStep::MarkMinimized => {
                if let Some(m) = map.get_mut(&raw) {
                    m.was_minimized = true;
                }
            }
            GridCellStep::Snap => {
                let _ = place_window_with(hwnd, memory.rect, grid_swp_flags(true));
                if let Some(m) = map.get_mut(&raw) {
                    m.was_minimized = false;
                }
            }
            GridCellStep::Keep => {
                if let Some(m) = map.get_mut(&raw) {
                    m.was_minimized = false;
                }
            }
        }
    }
}

/// A olhada de 1 s: cliente que saiu do rastreamento é esquecido; opção
/// desligada devolve as molduras.
pub fn grid_borders_tick(settings: &SettingsStore) {
    let ours = launched_client_pids();
    let pending = {
        let mut map = borderless_windows();
        map.retain(|_, (pid, _)| ours.contains(pid));
        !map.is_empty()
    };
    if pending && !settings.get_bool("General", "GridBorderless") {
        release_borderless_windows();
    }
}

/// O botão manual põe a i-ésima janela na célula i, dando a volta.
fn manual_slot(index: usize, slot_count: usize) -> usize {
    index % slot_count.max(1)
}

/// Os monitores escolhidos (índices de 1 em diante; vazio = todos), sem os
/// índices que não existem.
fn selected_monitors(all: &[MonitorInfoDto], indices: &[usize]) -> Vec<MonitorInfoDto> {
    if indices.is_empty() {
        return all.to_vec();
    }
    indices
        .iter()
        .filter_map(|&i| i.checked_sub(1).and_then(|idx| all.get(idx)).cloned())
        .collect()
}

/// Uma janela da grade por vez: duas janelas da fila aparecendo juntas não
/// escolhem a mesma célula.
static GRID_PLACEMENT: Mutex<()> = Mutex::new(());

/// Onde e com quem a janela nova divide a grade.
pub struct GridRequest {
    /// Índices de 1 em diante; vazio = todos os monitores.
    pub monitor_indices: Vec<usize>,
    pub gap: i32,
    /// PIDs fora da grade (contas com janela própria).
    pub excluded_pids: std::collections::HashSet<u32>,
    /// Célula menor que o mínimo e sem moldura.
    pub style: GridWindowStyle,
}

/// Põe a janela do PID na primeira célula livre da grade. A célula tem o
/// tamanho global (`size`, no sentido do `StartScreenSize`), ou, sem ele, o
/// tamanho com que a janela abriu. Sem célula que caiba, só o tamanho é
/// imposto. `first_look`: ver `enforce_client_window` — falso quando o app
/// acabou de tirar a janela da tela cheia (o tamanho dela é o do app, não o
/// que o XML deu, e não ensina nada).
pub fn place_in_grid(
    hwnd: HWND,
    pid: u32,
    size: Option<(u32, u32)>,
    request: &GridRequest,
    first_look: bool,
) -> WindowEnforcement {
    let _guard = GRID_PLACEMENT.lock().unwrap_or_else(|e| e.into_inner());
    let Some((current, client)) = movable_window(hwnd) else {
        return WindowEnforcement::Skipped;
    };
    if first_look {
        learn_size_semantics(size, current, client);
    }
    // A grade só é chamada para clientes que o app abriu. A moldura sai antes
    // de medir: sem ela a janela inteira é área útil.
    let small = request.style.small_cells();
    let (current, client) = if request.style.borderless {
        make_borderless(hwnd, pid);
        match movable_window(hwnd) {
            Some(measured) => measured,
            None => return WindowEnforcement::Skipped,
        }
    } else {
        (current, client)
    };
    let requested = match size {
        Some((tw, th)) => outer_size_for_target(
            (tw as i32, th as i32),
            learned_size_semantics(),
            (current.2, current.3),
            client,
        ),
        None => (current.2, current.3),
    };
    let others: Vec<RobloxWin> = grid_windows(list_roblox_windows(), list_minimized_roblox_windows())
        .into_iter()
        .filter(|w| w.pid != pid)
        .collect();
    let (blocker_wins, occupant_wins): (Vec<RobloxWin>, Vec<RobloxWin>) =
        others.into_iter().partition(|w| request.excluded_pids.contains(&w.pid));
    // A grade já foi encolhida para caber mais janelas (`cell_that_fits`): a
    // nova entra no tamanho dela, não no mínimo do Roblox — senão as células
    // não batem e ela cai por cima das outras.
    // E vale também para célula maior: com a grade organizada em 1272x695 (a
    // base era uma janela que a pessoa aumentou), a nova num 816x639 caía numa
    // célula que só existe nesse tamanho, por cima das outras. Vale com janela
    // aberta pelo site na grade: ela também vai sem o mínimo (ver
    // `arrange_roblox_grid_locked`).
    let grid_in_use = !occupant_wins.is_empty();
    if request.style.auto_fit {
        let monitors = selected_monitors(&list_monitors(), &request.monitor_indices);
        let rect = |w: &RobloxWin| (w.x, w.y, w.w, w.h);
        let occupants: Vec<(i32, i32, i32, i32)> = occupant_wins.iter().map(rect).collect();
        let blockers: Vec<(i32, i32, i32, i32)> = blocker_wins.iter().map(rect).collect();
        if let Some(best) = best_fit_cell(&monitors, request.gap, occupants.len() + 1, &blockers) {
            // Uma janela a mais muda o melhor tamanho: rearruma todas nele.
            // Com o tamanho igual, a nova só pega a primeira célula livre e
            // ninguém se mexe.
            if !occupant_wins.is_empty() && grid_cell_in_use(&occupant_wins) != Some(best) {
                let _ = arrange_roblox_grid_locked(
                    &request.monitor_indices,
                    request.gap,
                    size,
                    &request.excluded_pids,
                    request.style,
                    false,
                );
                return match get_window_position(hwnd) {
                    Some(r) => WindowEnforcement::Placed(r),
                    None => WindowEnforcement::Skipped,
                };
            }
            let slots = grid_slots(&monitors, best.0, best.1, request.gap);
            let target = match pick_grid_slot(&slots, best, &occupants, &blockers) {
                Some(i) => (slots[i].x, slots[i].y, best.0, best.1),
                None => (current.0, current.1, best.0, best.1),
            };
            remember_grid_cell(hwnd as isize, Some(target), false);
            return move_to_with(hwnd, current, target, true);
        }
    }
    // Janela que já está no tamanho da grade (uma organização anterior a pegou,
    // quando várias contas abrem quase juntas) também vai sem o mínimo do
    // Roblox: medi-la de novo a fazia crescer até ele, por cima das outras
    // (teste real `real_grid_scenario_launch_placement_with_more_windows_than_cells`,
    // 26/10/2026). A célula é a que as outras já têm, então não há o que medir.
    let (small, requested) = match grid_cell_in_use(&occupant_wins) {
        Some(cell) if grid_in_use => (small || cell == requested || grid_already_shrunk(cell, requested), cell),
        _ => (small, requested),
    };
    let (cell_w, cell_h) =
        grid_cell_size(requested, accepted_size(hwnd, current, requested, small), small);
    let monitors = selected_monitors(&list_monitors(), &request.monitor_indices);
    let slots = grid_slots(&monitors, cell_w, cell_h, request.gap);
    let rect = |w: &RobloxWin| (w.x, w.y, w.w, w.h);
    let occupants: Vec<(i32, i32, i32, i32)> = occupant_wins.iter().map(rect).collect();
    let blockers: Vec<(i32, i32, i32, i32)> = blocker_wins.iter().map(rect).collect();
    // Grade lotada: em vez de pôr esta por cima de outra, rearruma todas num
    // tamanho em que caibam.
    if grid_is_full(&slots, (cell_w, cell_h), &occupants, &blockers)
        && cell_that_fits(&monitors, (cell_w, cell_h), request.gap, occupants.len() + 1, &blockers).is_some()
    {
        let _ = arrange_roblox_grid_locked(
            &request.monitor_indices,
            request.gap,
            size,
            &request.excluded_pids,
            request.style,
            false,
        );
        return match get_window_position(hwnd) {
            Some(r) => WindowEnforcement::Placed(r),
            None => WindowEnforcement::Skipped,
        };
    }
    let target = match pick_grid_slot(&slots, (cell_w, cell_h), &occupants, &blockers) {
        Some(i) => (slots[i].x, slots[i].y, cell_w, cell_h),
        None => (current.0, current.1, cell_w, cell_h),
    };
    remember_grid_cell(hwnd as isize, Some(target), false);
    move_to_with(hwnd, current, target, small)
}

/// O tamanho de célula que a grade está usando: o das outras janelas, quando
/// ele é de fato o da grade — de duas ou mais delas, ou da única aberta. Todas
/// de tamanhos diferentes (arrastadas uma a uma) não dizem nada.
fn grid_cell_in_use(occupants: &[RobloxWin]) -> Option<(i32, i32)> {
    if occupants.len() == 1 {
        return Some((occupants[0].w, occupants[0].h));
    }
    let common = most_common_size(occupants);
    let holders = occupants.iter().filter(|w| (w.w, w.h) == common).count();
    (holders >= 2).then_some(common)
}

/// A grade já foi encolhida (a célula das outras é menor que a da janela nova
/// em **alguma** medida). Exigir as duas deixava passar a célula mais larga e
/// mais baixa que o mínimo — que sai quando a base da grade era uma janela em
/// tela cheia — e a nova caía numa célula de 816x639 por cima das outras.
fn grid_already_shrunk(common: (i32, i32), requested: (i32, i32)) -> bool {
    common.0 < requested.0 || common.1 < requested.1
}

/// O tamanho que a janela aceita de verdade: pede `requested` no lugar onde ela
/// está e lê de volta. O mínimo do Roblox é aplicado na hora (WM_GETMINMAXINFO),
/// então a leitura já vem com o tamanho final.
fn accepted_size(
    hwnd: HWND,
    current: (i32, i32, i32, i32),
    requested: (i32, i32),
    allow_small: bool,
) -> (i32, i32) {
    // Pede sempre, mesmo já no tamanho: uma janela que a grade encolheu antes
    // já está nele, e pular o pedido escondia o mínimo — na segunda
    // "Organizar em grade" a célula parecia servir, as janelas iam sem
    // `SWP_NOSENDCHANGING` e cresciam umas por cima das outras.
    place_window_with(
        hwnd,
        (current.0, current.1, requested.0, requested.1),
        grid_swp_flags(allow_small),
    );
    get_window_position(hwnd).map(|r| (r.2, r.3)).unwrap_or(requested)
}

/// Arrange the open Roblox windows into the grid across the selected monitors
/// (1-based indices; empty = all). `size`: the global window size (in the
/// `StartScreenSize` sense) when it is on; without it the cell is the most
/// common window size. Windows of `excluded_pids` (accounts with their own
/// window size) are left alone. Returns (arranged, total_windows).
pub fn arrange_roblox_grid(
    monitor_indices: &[usize],
    gap: i32,
    size: Option<(u32, u32)>,
    excluded_pids: &std::collections::HashSet<u32>,
    style: GridWindowStyle,
) -> Result<(usize, usize), String> {
    let _guard = GRID_PLACEMENT.lock().unwrap_or_else(|e| e.into_inner());
    arrange_roblox_grid_locked(monitor_indices, gap, size, excluded_pids, style, true)
}

/// `arrange_roblox_grid` com a trava da grade já tomada (a colocação do launch
/// chama isto quando a grade lota).
fn arrange_roblox_grid_locked(
    monitor_indices: &[usize],
    gap: i32,
    size: Option<(u32, u32)>,
    excluded_pids: &std::collections::HashSet<u32>,
    style: GridWindowStyle,
    restore_minimized: bool,
) -> Result<(usize, usize), String> {
    // Moldura só nos clientes que o app abriu. A célula pequena vale para
    // todas as janelas da grade, inclusive a do cliente aberto pelo site:
    // antes, uma janela dele na grade fazia **nenhuma** encolher (relato do
    // Discord, 30/10/2026: "Windows stopped shrinking"). O `SWP_NOSENDCHANGING`
    // passa por baixo do mínimo em qualquer janela do Roblox, seja quem a abriu.
    let own: std::collections::HashSet<u32> = launched_client_pids()
        .into_iter()
        .filter(|pid| !excluded_pids.contains(pid))
        .collect();
    if style.borderless {
        for w in list_roblox_windows().iter().filter(|w| own.contains(&w.pid)) {
            make_borderless(w.hwnd as HWND, w.pid);
        }
    } else {
        release_borderless_for(&own);
    }
    // As minimizadas entram na grade e ganham a célula delas (ficam minimizadas,
    // e voltam ao lugar certo ao restaurar): sem isso, as visíveis ocupavam as
    // mesmas células e tudo se sobrepunha ao restaurar.
    let minimized = list_minimized_roblox_windows();
    let minimized_hwnds: std::collections::HashSet<isize> = minimized.iter().map(|w| w.hwnd).collect();
    let (excluded, wins): (Vec<RobloxWin>, Vec<RobloxWin>) =
        grid_windows(list_roblox_windows(), minimized)
            .into_iter()
            .partition(|w| excluded_pids.contains(&w.pid));
    let mut small = style.small_cells();
    let blockers: Vec<(i32, i32, i32, i32)> = excluded.iter().map(|w| (w.x, w.y, w.w, w.h)).collect();
    if wins.is_empty() {
        return Err("Nenhuma janela de Roblox aberta foi encontrada.".to_string());
    }
    let monitors_all = list_monitors();
    if monitors_all.is_empty() {
        return Err("Nenhum monitor foi detectado.".to_string());
    }
    let monitors = selected_monitors(&monitors_all, monitor_indices);
    if monitors.is_empty() {
        return Err("Selecione ao menos um monitor válido.".to_string());
    }
    // Medir e testar o tamanho só numa janela visível: numa minimizada o
    // retângulo vivo não diz nada.
    // E de preferência numa com moldura normal: em tela cheia (sem borda),
    // maximizada ou sem moldura o Windows não impõe o mínimo, a medida dizia
    // que a célula pequena servia e as outras cresciam por cima umas das outras.
    let visible: Vec<RobloxWin> = wins.iter().filter(|w| !minimized_hwnds.contains(&w.hwnd)).copied().collect();
    let probe = visible
        .iter()
        .find(|w| window_enforces_min_size(w.hwnd as HWND, (w.x, w.y, w.w, w.h)))
        .or_else(|| visible.first())
        .copied();
    // "Melhor tamanho": a célula sai da conta, não do tamanho configurado.
    let auto_cell = if style.auto_fit {
        best_fit_cell(&monitors, gap, wins.len(), &blockers)
    } else {
        None
    };
    let (cell_w, cell_h) = match (auto_cell, size) {
        (Some(best), _) => best,
        (None, Some((tw, th))) => {
            let first = probe.unwrap_or(wins[0]);
            let client = if probe.is_some() {
                client_area_size(first.hwnd as HWND).unwrap_or((first.w, first.h))
            } else {
                (first.w, first.h)
            };
            outer_size_for_target(
                (tw as i32, th as i32),
                learned_size_semantics(),
                (first.w, first.h),
                client,
            )
        }
        (None, None) => most_common_size(&wins),
    };
    let (cell_w, cell_h) = match probe {
        // A célula do "melhor tamanho" já é a final: medir a janela aqui só
        // traria o mínimo do Roblox de volta.
        _ if auto_cell.is_some() => (cell_w, cell_h),
        Some(first) => grid_cell_size(
            (cell_w, cell_h),
            accepted_size(
                first.hwnd as HWND,
                (first.x, first.y, first.w, first.h),
                (cell_w, cell_h),
                small,
            ),
            small,
        ),
        None => (cell_w, cell_h),
    };
    // Mais janelas do que cabem: encolhe a célula até caberem todas, em vez de
    // dar a volta e empilhar.
    let (cell_w, cell_h) = match cell_that_fits(&monitors, (cell_w, cell_h), gap, wins.len(), &blockers) {
        Some(fit) if fit != (cell_w, cell_h) => {
            small = true;
            fit
        }
        _ => (cell_w, cell_h),
    };
    let slots = unblocked_slots(&grid_slots(&monitors, cell_w, cell_h, gap), (cell_w, cell_h), &blockers);
    if slots.is_empty() {
        return Err("Nenhuma célula coube nos monitores (janelas grandes demais para o gap?).".to_string());
    }
    let mut arranged = 0;
    for (i, w) in wins.iter().enumerate() {
        let cell = slots[manual_slot(i, slots.len())];
        let hwnd = w.hwnd as HWND;
        if minimized_hwnds.contains(&w.hwnd) && restore_minimized {
            // O botão traz a minimizada de volta, já na célula (pedido do dono,
            // 26/10/2026: abria uma por uma). Antes de restaurar, a posição
            // normal vira a célula e sai o "voltar maximizada"; restaura sem
            // roubar o foco e põe no tamanho da célula como as visíveis.
            let rect = (cell.x, cell.y, cell_w, cell_h);
            let _ = place_minimized_window(hwnd, rect);
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            if place_window_with(hwnd, rect, grid_swp_flags(small)) {
                arranged += 1;
            }
            remember_grid_cell(w.hwnd, (small || own.contains(&w.pid)).then_some(rect), false);
            continue;
        }
        if minimized_hwnds.contains(&w.hwnd) {
            let rect = (cell.x, cell.y, cell_w, cell_h);
            if place_minimized_window(hwnd, rect) {
                arranged += 1;
            }
            // Célula que pode ser menor que o mínimo: ao restaurar o Roblox a
            // aumentaria de volta; a olhada de 1 s a põe na célula
            // (`snap_restored_small_windows`). Vale para todo cliente aberto
            // pelo app, não só quando a célula encolheu agora: na segunda
            // organização o `rcNormalPosition` já é pequeno, a célula cabe sem
            // encolher e `small` fica falso (bateria de 25/10/2026).
            remember_grid_cell(w.hwnd, (small || own.contains(&w.pid)).then_some(rect), true);
            continue;
        }
        if unsafe { IsZoomed(hwnd) } != 0 {
            unsafe {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
        }
        if place_window_with(hwnd, (cell.x, cell.y, cell_w, cell_h), grid_swp_flags(small)) {
            arranged += 1;
        }
        // A pessoa pode minimizá-la depois: ao voltar, ela volta para cá.
        remember_grid_cell(
            w.hwnd,
            (small || own.contains(&w.pid)).then_some((cell.x, cell.y, cell_w, cell_h)),
            false,
        );
    }
    Ok((arranged, wins.len()))
}

/// Os contadores de memória de um processo (`PROCESS_MEMORY_COUNTERS_EX2`). Os
/// dez primeiros campos são o `PROCESS_MEMORY_COUNTERS` antigo; os três do fim
/// só existem no Windows 10 1809+ (antes disso a chamada com este tamanho falha
/// e a leitura cai para o tamanho antigo).
#[repr(C)]
#[derive(Default)]
struct ProcessMemoryCountersEx2 {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
    private_usage: usize,
    private_working_set_size: usize,
    shared_commit_usage: u64,
}

/// Tamanho do `PROCESS_MEMORY_COUNTERS` antigo: até `peak_pagefile_usage`.
const PROCESS_MEMORY_COUNTERS_SIZE: u32 = (2 * std::mem::size_of::<u32>() + 8 * std::mem::size_of::<usize>()) as u32;

/// A leitura em MB a partir dos contadores: a memória privada (`private_ws`,
/// bytes) quando o Windows a deu; senão o working set inteiro. `Some(0)` com
/// working set > 0 é tratado como "não deu" (nenhum processo vivo tem 0 de
/// memória privada).
fn memory_reading_mb(private_ws: Option<usize>, working_set: usize) -> u64 {
    let bytes = match private_ws {
        Some(private) if private > 0 || working_set == 0 => private,
        _ => working_set,
    };
    bytes as u64 / 1024 / 1024
}

/// Lê os contadores com o handle já aberto: primeiro o formato novo (com a
/// memória privada), depois o antigo. `None` se as duas leituras falharam.
unsafe fn read_memory_counters(handle: HANDLE) -> Option<(Option<usize>, usize)> {
    extern "system" {
        fn K32GetProcessMemoryInfo(
            process: HANDLE,
            ppsmemcounters: *mut ProcessMemoryCountersEx2,
            cb: u32,
        ) -> i32;
    }
    let full = std::mem::size_of::<ProcessMemoryCountersEx2>() as u32;
    let mut counters = ProcessMemoryCountersEx2 {
        cb: full,
        ..Default::default()
    };
    if K32GetProcessMemoryInfo(handle, &mut counters, full) != 0 {
        return Some((Some(counters.private_working_set_size), counters.working_set_size));
    }
    let mut counters = ProcessMemoryCountersEx2 {
        cb: PROCESS_MEMORY_COUNTERS_SIZE,
        ..Default::default()
    };
    if K32GetProcessMemoryInfo(handle, &mut counters, PROCESS_MEMORY_COUNTERS_SIZE) != 0 {
        return Some((None, counters.working_set_size));
    }
    None
}

/// Abre o processo com `access` e lê os contadores.
fn read_process_memory_with(pid: u32, access: u32) -> Option<(Option<usize>, usize)> {
    unsafe {
        let handle = OpenProcess(access, 0, pid);
        if handle.is_null() {
            return None;
        }
        let counters = read_memory_counters(handle);
        CloseHandle(handle);
        counters
    }
}

/// Memória do processo em MB, **a mesma da coluna "Memória" do Gerenciador de
/// Tarefas**: o working set privado (`PrivateWorkingSetSize`). É o número da
/// linha "Em jogo" da página Session, o que o teto de memória compara com o
/// limite e o que o "Close If Memory Low" do Watcher compara com o mínimo.
///
/// Até 11/10/2026 era o `WorkingSetSize` (working set inteiro, com as páginas
/// compartilhadas e a memória de vídeo mapeada): num cliente Roblox dava ~4 GB
/// com o Gerenciador mostrando ~1,2 GB, quase não mexia (a parte compartilhada é
/// estável) e ficava acima do limite logo depois de cada liberação.
///
/// Sem a memória privada (Windows anterior ao 10 1809), cai para o working set
/// inteiro — maior que o do Gerenciador, mas é o que dá para ler sem percorrer
/// as páginas do processo. Abre com o acesso mínimo (o mesmo do Gerenciador) e,
/// se o sistema recusar, com o acesso antigo (leitura de memória).
pub fn get_process_memory_mb(pid: u32) -> Option<u64> {
    use windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION;
    if pid == 0 {
        return None;
    }
    let (private_ws, working_set) = read_process_memory_with(pid, PROCESS_QUERY_LIMITED_INFORMATION)
        .or_else(|| read_process_memory_with(pid, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ))?;
    Some(memory_reading_mb(private_ws, working_set))
}

/// Memória física livre e total do Windows, em MB (o aviso "Windows quase sem
/// memória" da página Discord). Só leitura.
pub fn system_memory_mb() -> Option<(u64, u64)> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    const MB: u64 = 1024 * 1024;
    // SAFETY: MEMORYSTATUSEX é uma struct C simples; zerada e com o
    // `dwLength` certo é o que a função pede.
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
        return None;
    }
    Some((status.ullAvailPhys / MB, status.ullTotalPhys / MB))
}

#[cfg(test)]
mod win_process_memory_tests {
    use super::*;

    const MB: usize = 1024 * 1024;

    /// O número do Gerenciador de Tarefas: a memória privada, não o working
    /// set inteiro (o caso do dono: 4 GB no app, 1,2 GB no Gerenciador).
    #[test]
    fn the_reading_is_the_private_working_set() {
        assert_eq!(memory_reading_mb(Some(1229 * MB), 4096 * MB), 1229);
    }

    /// Windows sem a memória privada: cai para o working set inteiro.
    #[test]
    fn without_the_private_number_it_falls_back_to_the_working_set() {
        assert_eq!(memory_reading_mb(None, 4096 * MB), 4096);
        assert_eq!(memory_reading_mb(Some(0), 4096 * MB), 4096, "0 private bytes = not supported");
        assert_eq!(memory_reading_mb(Some(0), 0), 0);
    }

    /// O formato antigo é o começo do novo (a queda para o tamanho antigo
    /// depende disso).
    #[test]
    fn the_old_counters_are_the_start_of_the_new_ones() {
        let offset = std::mem::offset_of!(ProcessMemoryCountersEx2, private_usage);
        assert_eq!(offset as u32, PROCESS_MEMORY_COUNTERS_SIZE);
        if cfg!(target_pointer_width = "64") {
            assert_eq!(PROCESS_MEMORY_COUNTERS_SIZE, 72);
            assert_eq!(std::mem::size_of::<ProcessMemoryCountersEx2>(), 96);
        }
    }

    /// Leitura de verdade, no próprio processo de teste: com o acesso mínimo
    /// dá a memória privada, menor ou igual ao working set inteiro.
    #[test]
    fn the_own_process_reads_with_the_minimal_access() {
        use windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION;
        let (private_ws, working_set) =
            read_process_memory_with(std::process::id(), PROCESS_QUERY_LIMITED_INFORMATION).expect("read");
        let private_ws = private_ws.expect("Windows 10 1809+ gives the private working set");
        assert!(private_ws > 0);
        assert!(private_ws <= working_set);
        assert!(get_process_memory_mb(std::process::id()).is_some());
        assert_eq!(get_process_memory_mb(0), None);
    }
}

#[cfg(test)]
mod win_windowing_tests {
    use super::*;

    // Only the pure geometry is covered: `most_common_size` and `compute_grid`
    // together are everything `arrange_roblox_grid` decides before it calls
    // MoveWindow. Enumeration and window moving need a live desktop.

    fn win(x: i32, y: i32, w: i32, h: i32) -> RobloxWin {
        RobloxWin {
            hwnd: 0,
            pid: 0,
            x,
            y,
            w,
            h,
        }
    }

    fn monitor(index: usize, x: i32, y: i32, width: i32, height: i32) -> MonitorInfoDto {
        MonitorInfoDto {
            index,
            x,
            y,
            width,
            height,
            primary: index == 1,
        }
    }

    // ── most_common_size ───────────────────────────────────────────────────

    #[test]
    fn most_common_size_picks_the_modal_window_size() {
        let wins = vec![
            win(0, 0, 800, 600),
            win(0, 0, 800, 600),
            win(0, 0, 1280, 720),
        ];
        assert_eq!(most_common_size(&wins), (800, 600));
    }

    #[test]
    fn most_common_size_breaks_ties_with_the_largest_area() {
        let wins = vec![
            win(0, 0, 800, 600),
            win(0, 0, 1280, 720),
            win(0, 0, 640, 480),
        ];
        assert_eq!(most_common_size(&wins), (1280, 720));
    }

    #[test]
    fn most_common_size_falls_back_to_800x600_for_no_windows() {
        assert_eq!(most_common_size(&[]), (800, 600));
    }

    #[test]
    fn most_common_size_returns_the_only_size_when_all_windows_match() {
        let wins = vec![win(0, 0, 1920, 1080); 5];
        assert_eq!(most_common_size(&wins), (1920, 1080));
    }

    // ── compute_grid ───────────────────────────────────────────────────────

    #[test]
    fn compute_grid_lays_cells_out_left_to_right_then_top_to_bottom() {
        // 1000x1000 monitor, 400x400 windows, gap 10:
        // x: 10, 420 (830 + 400 > 1000 -> row ends)
        // y: 10, 420 (830 + 400 > 1000 -> monitor exhausted)
        let monitors = vec![monitor(1, 0, 0, 1000, 1000)];
        let cells = compute_grid(&monitors, 400, 400, 4, 10);

        assert_eq!(cells.len(), 4);
        assert_eq!((cells[0].x, cells[0].y), (10, 10));
        assert_eq!((cells[1].x, cells[1].y), (420, 10));
        assert_eq!((cells[2].x, cells[2].y), (10, 420));
        assert_eq!((cells[3].x, cells[3].y), (420, 420));
    }

    #[test]
    fn compute_grid_never_returns_more_cells_than_requested() {
        let monitors = vec![monitor(1, 0, 0, 1000, 1000)];
        for count in 0..=4 {
            assert_eq!(compute_grid(&monitors, 400, 400, count, 10).len(), count);
        }
    }

    #[test]
    fn compute_grid_stops_when_the_monitor_runs_out_of_room() {
        // Only 4 cells fit; asking for 10 yields 4.
        let monitors = vec![monitor(1, 0, 0, 1000, 1000)];
        assert_eq!(compute_grid(&monitors, 400, 400, 10, 10).len(), 4);
    }

    #[test]
    fn compute_grid_honours_the_monitor_origin() {
        let monitors = vec![monitor(1, -1920, 120, 1000, 1000)];
        let cells = compute_grid(&monitors, 400, 400, 2, 10);
        assert_eq!((cells[0].x, cells[0].y), (-1910, 130));
        assert_eq!((cells[1].x, cells[1].y), (-1500, 130));
    }

    #[test]
    fn compute_grid_continues_onto_the_next_monitor() {
        // First monitor fits exactly one cell, second takes the remainder.
        let monitors = vec![
            monitor(1, 0, 0, 500, 500),
            monitor(2, 2000, 0, 1000, 1000),
        ];
        let cells = compute_grid(&monitors, 400, 400, 3, 10);

        assert_eq!(cells.len(), 3);
        assert_eq!((cells[0].x, cells[0].y), (10, 10));
        assert_eq!((cells[1].x, cells[1].y), (2010, 10));
        assert_eq!((cells[2].x, cells[2].y), (2420, 10));
    }

    #[test]
    fn compute_grid_skips_a_monitor_too_small_for_even_one_window() {
        let monitors = vec![
            monitor(1, 0, 0, 200, 200), // 400x400 cannot fit
            monitor(2, 1000, 0, 1000, 1000),
        ];
        let cells = compute_grid(&monitors, 400, 400, 2, 10);
        assert_eq!(cells.len(), 2);
        assert_eq!((cells[0].x, cells[0].y), (1010, 10));
        assert_eq!((cells[1].x, cells[1].y), (1420, 10));
    }

    #[test]
    fn compute_grid_returns_nothing_when_the_windows_are_larger_than_every_monitor() {
        let monitors = vec![monitor(1, 0, 0, 800, 600)];
        assert!(compute_grid(&monitors, 1920, 1080, 4, 10).is_empty());
    }

    #[test]
    fn compute_grid_returns_nothing_without_monitors() {
        assert!(compute_grid(&[], 400, 400, 4, 10).is_empty());
    }

    #[test]
    fn compute_grid_with_a_zero_gap_still_leaves_no_overlap() {
        let monitors = vec![monitor(1, 0, 0, 800, 800)];
        let cells = compute_grid(&monitors, 400, 400, 4, 0);
        assert_eq!(cells.len(), 4);
        assert_eq!((cells[0].x, cells[0].y), (0, 0));
        assert_eq!((cells[1].x, cells[1].y), (400, 0));
        assert_eq!((cells[2].x, cells[2].y), (0, 400));
        assert_eq!((cells[3].x, cells[3].y), (400, 400));

        // No two cells of the same size may overlap.
        for (i, a) in cells.iter().enumerate() {
            for b in cells.iter().skip(i + 1) {
                let overlaps_x = a.x < b.x + 400 && b.x < a.x + 400;
                let overlaps_y = a.y < b.y + 400 && b.y < a.y + 400;
                assert!(!(overlaps_x && overlaps_y), "cells overlap");
            }
        }
    }

    #[test]
    fn a_larger_gap_fits_fewer_cells() {
        let monitors = vec![monitor(1, 0, 0, 1000, 1000)];
        let small_gap = compute_grid(&monitors, 400, 400, 16, 10).len();
        let large_gap = compute_grid(&monitors, 400, 400, 16, 150).len();
        assert_eq!(small_gap, 4);
        assert!(
            large_gap < small_gap,
            "a bigger gap should fit fewer cells ({} vs {})",
            large_gap,
            small_gap
        );
    }

    #[test]
    fn every_cell_stays_inside_its_monitor_work_area() {
        let monitors = vec![
            monitor(1, 0, 0, 1920, 1040),
            monitor(2, 1920, -200, 2560, 1400),
        ];
        let (w, h) = (640, 360);
        let cells = compute_grid(&monitors, w, h, 40, 12);
        assert!(!cells.is_empty());

        for cell in &cells {
            let inside = monitors.iter().any(|m| {
                cell.x >= m.x
                    && cell.y >= m.y
                    && cell.x + w <= m.x + m.width
                    && cell.y + h <= m.y + m.height
            });
            assert!(inside, "cell ({}, {}) escaped every monitor", cell.x, cell.y);
        }
    }

    #[test]
    fn monitor_info_dto_serializes_its_fields() {
        let json = serde_json::to_value(monitor(1, -100, 0, 1920, 1080)).unwrap();
        assert_eq!(json["index"], 1);
        assert_eq!(json["x"], -100);
        assert_eq!(json["width"], 1920);
        assert_eq!(json["height"], 1080);
        assert_eq!(json["primary"], true);
    }
}

#[cfg(test)]
mod win_grid_auto_fit_tests {
    use super::*;

    // "Melhor tamanho em cada monitor" (General.GridAutoFit, pedido do dono em
    // 26/10/2026): ele chutava largura e altura até achar o maior tamanho em que
    // todas as contas cabiam. A célula sai da conta: a maior, em formato
    // deitado (entre 4:3 e 16:9), em que `count` janelas cabem com o gap.

    fn monitor(index: usize, x: i32, y: i32, width: i32, height: i32) -> MonitorInfoDto {
        MonitorInfoDto { index, x, y, width, height, primary: index == 1 }
    }

    fn fits(monitors: &[MonitorInfoDto], cell: (i32, i32), gap: i32, count: usize, blockers: &[(i32, i32, i32, i32)]) -> bool {
        grid_slots(monitors, cell.0, cell.1, gap)
            .iter()
            .filter(|s| !blockers.iter().any(|&b| slot_overlaps(s, cell, b)))
            .count()
            >= count
    }

    #[test]
    fn one_window_takes_the_whole_monitor() {
        let m = [monitor(1, 0, 0, 1920, 1080)];
        assert_eq!(best_fit_cell(&m, 0, 1, &[]), Some((1920, 1080)));
    }

    #[test]
    fn four_windows_on_one_monitor_make_the_largest_two_by_two() {
        let m = [monitor(1, 0, 0, 1920, 1080)];
        let cell = best_fit_cell(&m, 20, 4, &[]).expect("fits");
        assert_eq!(cell, (924, 519));
        assert!(fits(&m, cell, 20, 4, &[]));
    }

    #[test]
    fn the_cell_is_the_largest_that_fits_and_one_pixel_more_does_not() {
        let m = [monitor(1, 0, 0, 2560, 1400), monitor(2, 2560, 0, 1920, 1040)];
        for count in [2usize, 3, 5, 7, 9, 12, 16] {
            let (w, h) = best_fit_cell(&m, 20, count, &[]).expect("fits");
            assert!(fits(&m, (w, h), 20, count, &[]), "{count} windows must fit in {w}x{h}");
            let ratio = w as f64 / h as f64;
            assert!((4.0 / 3.0 - 0.01..=16.0 / 9.0 + 0.01).contains(&ratio), "{count}: {w}x{h} is not landscape");
            // Uma célula maior no mesmo formato já não cabe.
            let bigger = (w + 2, ((w + 2) as i64 * h as i64 / w as i64) as i32 + 1);
            assert!(!fits(&m, bigger, 20, count, &[]), "{count}: {w}x{h} was not the largest");
        }
    }

    #[test]
    fn more_windows_never_get_a_bigger_cell() {
        let m = [monitor(1, 0, 0, 2560, 1400)];
        let mut last = i64::MAX;
        for count in 1..=30 {
            let (w, h) = best_fit_cell(&m, 10, count, &[]).expect("fits");
            let area = w as i64 * h as i64;
            assert!(area <= last, "{count} windows got a bigger cell than {}", count - 1);
            last = area;
        }
    }

    #[test]
    fn a_window_outside_the_grid_is_left_room() {
        let m = [monitor(1, 0, 0, 1920, 1080)];
        // A principal com tamanho próprio no canto esquerdo.
        let blockers = [(0, 0, 960, 1080)];
        let cell = best_fit_cell(&m, 0, 2, &blockers).expect("fits");
        assert!(fits(&m, cell, 0, 2, &blockers));
        assert!(cell.0 <= 960);
    }

    #[test]
    fn nothing_to_arrange_or_no_room_gives_none() {
        let m = [monitor(1, 0, 0, 1920, 1080)];
        assert_eq!(best_fit_cell(&m, 10, 0, &[]), None);
        assert_eq!(best_fit_cell(&[], 10, 3, &[]), None);
        // Abaixo do piso da célula pequena (200x200) não serve.
        assert_eq!(best_fit_cell(&[monitor(1, 0, 0, 800, 600)], 10, 40, &[]), None);
    }

    #[test]
    fn auto_fit_implies_cells_smaller_than_the_roblox_minimum() {
        let style = GridWindowStyle { allow_small: false, borderless: false, auto_fit: true };
        assert!(style.small_cells());
        assert!(!GridWindowStyle::default().small_cells());
    }
}

#[cfg(test)]
mod win_grid_slot_tests {
    use super::*;

    // A grade por células fixas: a janela nova pega a primeira célula livre e
    // nenhuma janela já aberta se mexe.

    fn monitor(index: usize, x: i32, y: i32, width: i32, height: i32) -> MonitorInfoDto {
        MonitorInfoDto {
            index,
            x,
            y,
            width,
            height,
            primary: index == 1,
        }
    }

    fn slots_1000() -> Vec<GridCell> {
        // 1000x1000, células 400x400, gap 10: (10,10) (420,10) (10,420) (420,420).
        grid_slots(&[monitor(1, 0, 0, 1000, 1000)], 400, 400, 10)
    }

    #[test]
    fn grid_slots_are_every_cell_that_fits_the_monitors_in_order() {
        let slots = slots_1000();
        let xy: Vec<(i32, i32)> = slots.iter().map(|c| (c.x, c.y)).collect();
        assert_eq!(xy, vec![(10, 10), (420, 10), (10, 420), (420, 420)]);

        let two = grid_slots(
            &[monitor(1, 0, 0, 500, 500), monitor(2, 2000, 0, 1000, 500)],
            400,
            400,
            10,
        );
        let xy: Vec<(i32, i32)> = two.iter().map(|c| (c.x, c.y)).collect();
        assert_eq!(xy, vec![(10, 10), (2010, 10), (2420, 10)]);
    }

    #[test]
    fn grid_slots_refuse_a_degenerate_cell() {
        assert!(grid_slots(&[monitor(1, 0, 0, 1000, 1000)], 0, 400, 10).is_empty());
        assert!(grid_slots(&[monitor(1, 0, 0, 1000, 1000)], 400, -1, 10).is_empty());
    }

    // Relato (23/10/2026): com as janelas organizadas e minimizadas, duas contas
    // abertas à mão caíam nas células 0 e 1 — as das minimizadas — e, ao
    // restaurar, tudo ficava por cima. A janela minimizada ocupa a célula do
    // retângulo que ela volta a ter ao restaurar.

    fn robloxwin(x: i32, y: i32, w: i32, h: i32) -> RobloxWin {
        RobloxWin { hwnd: 0, pid: 0, x, y, w, h }
    }

    // Relato do Discord (25/10/2026): mais janelas do que cabem no tamanho
    // mínimo do Roblox → a grade dava a volta e empilhava.

    #[test]
    fn the_cell_shrinks_until_every_window_fits() {
        let monitors = [monitor(1, 0, 0, 2560, 1392)];
        // 816x639 + gap 8: 3 x 2 = 6 células.
        assert_eq!(grid_slots(&monitors, 816, 639, 8).len(), 6);
        assert_eq!(cell_that_fits(&monitors, (816, 639), 8, 6, &[]), Some((816, 639)));
        let (w, h) = cell_that_fits(&monitors, (816, 639), 8, 8, &[]).expect("fits smaller");
        assert!(w < 816 && h < 639, "{w}x{h}");
        assert!(grid_slots(&monitors, w, h, 8).len() >= 8);
        // A mesma proporção.
        assert!(((w as f64 / h as f64) - 816.0 / 639.0).abs() < 0.01);
    }

    #[test]
    fn it_gives_up_below_the_smallest_cell() {
        let tiny = [monitor(1, 0, 0, 400, 400)];
        assert_eq!(cell_that_fits(&tiny, (816, 639), 8, 50, &[]), None);
    }

    #[test]
    fn a_grid_with_no_free_cell_is_full() {
        let slots = slots_1000();
        let all = [(10, 10, 400, 400), (420, 10, 400, 400), (10, 420, 400, 400), (420, 420, 400, 400)];
        assert!(grid_is_full(&slots, (400, 400), &all, &[]));
        assert!(!grid_is_full(&slots, (400, 400), &all[..3], &[]));
        // Célula coberta por uma janela fora da grade também conta.
        assert!(grid_is_full(&slots, (400, 400), &all[..3], &[(420, 420, 400, 400)]));
    }

    #[test]
    fn minimized_windows_join_the_visible_ones_in_reading_order() {
        let visible = vec![robloxwin(420, 10, 400, 400)];
        let minimized = vec![robloxwin(10, 420, 400, 400), robloxwin(10, 10, 400, 400)];
        let all = grid_windows(visible, minimized);
        let xy: Vec<(i32, i32)> = all.iter().map(|w| (w.x, w.y)).collect();
        assert_eq!(xy, vec![(10, 10), (420, 10), (10, 420)]);
    }

    #[test]
    fn a_minimized_window_still_occupies_its_slot() {
        let visible = vec![robloxwin(10, 10, 400, 400)];
        let minimized = vec![robloxwin(420, 10, 400, 400)];
        let all = grid_windows(visible, minimized);
        let occupants: Vec<(i32, i32, i32, i32)> = all.iter().map(|w| (w.x, w.y, w.w, w.h)).collect();
        // A nova janela não pode cair na célula 1, onde a minimizada volta.
        assert_eq!(pick_grid_slot(&slots_1000(), (400, 400), &occupants, &[]), Some(2));
    }

    #[test]
    fn workspace_coordinates_are_shifted_by_the_taskbar_edge() {
        // Barra de tarefas embaixo: a área de trabalho começa em (0, 0).
        assert_eq!(workspace_offset((0, 0), (0, 0)), (0, 0));
        // Barra no topo (48 px) e à esquerda (60 px).
        assert_eq!(workspace_offset((0, 48), (0, 0)), (0, 48));
        assert_eq!(workspace_offset((60, 0), (0, 0)), (60, 0));
        // Monitor principal que não começa em (0, 0).
        assert_eq!(workspace_offset((-1920, 48), (-1920, 0)), (0, 48));
    }

    #[test]
    fn screen_and_workspace_rects_round_trip() {
        let offset = (0, 48);
        let workspace = (100, 20, 800, 600);
        let screen = workspace_to_screen(workspace, offset);
        assert_eq!(screen, (100, 68, 800, 600));
        assert_eq!(screen_to_workspace(screen, offset), workspace);
    }

    #[test]
    fn the_first_window_takes_the_first_slot() {
        assert_eq!(pick_grid_slot(&slots_1000(), (400, 400), &[], &[]), Some(0));
    }

    #[test]
    fn the_next_window_takes_the_first_free_slot() {
        let occupants = [(10, 10, 400, 400), (10, 420, 400, 400)];
        assert_eq!(pick_grid_slot(&slots_1000(), (400, 400), &occupants, &[]), Some(1));
    }

    /// A janela que o usuário arrastou um pouco continua ocupando a célula
    /// enquanto o centro dela estiver lá dentro.
    #[test]
    fn a_window_nudged_inside_its_slot_still_occupies_it() {
        let occupants = [(60, 35, 400, 400)];
        assert_eq!(pick_grid_slot(&slots_1000(), (400, 400), &occupants, &[]), Some(1));
    }

    #[test]
    fn a_window_outside_every_slot_occupies_none() {
        let occupants = [(5000, 5000, 400, 400)];
        assert_eq!(pick_grid_slot(&slots_1000(), (400, 400), &occupants, &[]), Some(0));
    }

    /// Grade cheia: dá a volta e sobrepõe a partir da célula 0.
    #[test]
    fn a_full_grid_wraps_around_from_the_first_slot() {
        let full = [
            (10, 10, 400, 400),
            (420, 10, 400, 400),
            (10, 420, 400, 400),
            (420, 420, 400, 400),
        ];
        assert_eq!(pick_grid_slot(&slots_1000(), (400, 400), &full, &[]), Some(0));

        let mut fuller = full.to_vec();
        fuller.push((10, 10, 400, 400));
        assert_eq!(pick_grid_slot(&slots_1000(), (400, 400), &fuller, &[]), Some(1));
    }

    #[test]
    fn no_slots_means_no_placement() {
        assert_eq!(pick_grid_slot(&[], (400, 400), &[], &[]), None);
    }

    /// Teste real (03/10/2026): a principal de 1000x1000 fica fora da grade em
    /// (0,0); as alts de 816x638 não podem cair por cima dela.
    #[test]
    fn a_window_kept_out_of_the_grid_blocks_the_slots_under_it() {
        let slots = grid_slots(&[monitor(1, 0, 0, 2560, 1392)], 816, 638, 20);
        let main = [(0, 0, 1000, 1000)];
        let picked = pick_grid_slot(&slots, (816, 638), &[], &main).unwrap();
        assert_eq!((slots[picked].x, slots[picked].y), (1692, 20));
    }

    #[test]
    fn with_every_slot_blocked_the_grid_still_wraps_from_the_first() {
        let slots = slots_1000();
        let everything = [(0, 0, 1000, 1000)];
        assert_eq!(pick_grid_slot(&slots, (400, 400), &[], &everything), Some(0));
    }

    /// O Roblox não deixa a janela menor que ~800x600: com 520x420 na
    /// configuração, a célula tem de ser do tamanho que a janela ficou, senão as
    /// janelas se sobrepõem.
    #[test]
    fn the_cell_grows_to_the_size_the_window_really_took() {
        assert_eq!(grid_cell_size((536, 459), (816, 638), false), (816, 638));
        assert_eq!(grid_cell_size((1296, 759), (816, 638), false), (1296, 759));
        assert_eq!(grid_cell_size((900, 500), (816, 638), false), (900, 638));
    }

    #[test]
    fn manual_arrangement_skips_slots_under_windows_kept_out_of_the_grid() {
        let slots = slots_1000();
        let free = unblocked_slots(&slots, (400, 400), &[(0, 0, 410, 410)]);
        let xy: Vec<(i32, i32)> = free.iter().map(|c| (c.x, c.y)).collect();
        assert_eq!(xy, vec![(420, 10), (10, 420), (420, 420)]);
        // Tudo bloqueado: usa a grade inteira em vez de não arrumar nada.
        assert_eq!(unblocked_slots(&slots, (400, 400), &[(0, 0, 1000, 1000)]).len(), 4);
    }

    #[test]
    fn manual_arrangement_wraps_around_the_slots() {
        assert_eq!(manual_slot(0, 4), 0);
        assert_eq!(manual_slot(3, 4), 3);
        assert_eq!(manual_slot(5, 4), 1);
    }

    #[test]
    fn selected_monitors_default_to_all_and_drop_invalid_indices() {
        let all = vec![monitor(1, 0, 0, 1920, 1080), monitor(2, 1920, 0, 1920, 1080)];
        let idx = |m: Vec<MonitorInfoDto>| m.iter().map(|m| m.index).collect::<Vec<_>>();
        assert_eq!(idx(selected_monitors(&all, &[])), vec![1, 2]);
        assert_eq!(idx(selected_monitors(&all, &[2])), vec![2]);
        assert_eq!(idx(selected_monitors(&all, &[0, 2, 7])), vec![2]);
    }
}

#[cfg(test)]
mod win_grid_style_tests {
    use super::*;
    use std::collections::HashSet;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        WS_CLIPCHILDREN, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_SYSMENU, WS_VISIBLE,
    };

    // ── célula menor que o mínimo ──────────────────────────────────────────

    #[test]
    fn with_small_windows_allowed_the_cell_keeps_the_requested_size() {
        // O Roblox levaria 520x420 a 816x638; com a opção, fica 520x420.
        assert_eq!(grid_cell_size((520, 420), (816, 638), true), (520, 420));
    }

    #[test]
    fn small_cells_never_go_below_what_the_grid_can_see() {
        assert_eq!(grid_cell_size((120, 90), (816, 638), true), (200, 200));
    }

    #[test]
    fn without_the_option_the_cell_still_grows_to_the_roblox_minimum() {
        assert_eq!(grid_cell_size((520, 420), (816, 638), false), (816, 638));
    }

    #[test]
    fn only_small_cells_skip_the_size_change_notice() {
        let base = SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
        assert_eq!(grid_swp_flags(false), base);
        assert_eq!(grid_swp_flags(true), base | SWP_NOSENDCHANGING);
        // Nunca rouba o foco de quem está jogando.
        assert_ne!(grid_swp_flags(true) & SWP_NOACTIVATE, 0);
        assert_ne!(grid_swp_flags(false) & SWP_NOACTIVATE, 0);
    }

    /// A olhada só encaixa a janela que acabou de voltar de minimizada na
    /// própria célula; não briga com quem arrastou ou redimensionou.
    #[test]
    fn the_grid_snaps_a_window_only_when_it_comes_back_from_minimized_at_its_cell() {
        assert_eq!(grid_cell_step(false, false, false, true, true), GridCellStep::Forget);
        assert_eq!(grid_cell_step(true, true, false, false, false), GridCellStep::MarkMinimized);
        assert_eq!(grid_cell_step(true, false, false, true, true), GridCellStep::Snap);
        // Voltou maximizada, ou a pessoa a arrastou antes de minimizar: fica.
        assert_eq!(grid_cell_step(true, false, true, true, true), GridCellStep::Keep);
        assert_eq!(grid_cell_step(true, false, false, true, false), GridCellStep::Keep);
        // Não estava minimizada: quem mudou o tamanho foi a pessoa.
        assert_eq!(grid_cell_step(true, false, false, false, true), GridCellStep::Keep);
    }

    /// A medida do mínimo só serve numa janela com moldura normal.
    #[test]
    fn only_a_resizable_framed_window_shows_the_minimum_size() {
        assert!(frame_enforces_min_size(WS_OVERLAPPEDWINDOW));
        assert!(!frame_enforces_min_size(WS_POPUP));
        assert!(!frame_enforces_min_size(WS_OVERLAPPEDWINDOW & !GRID_FRAME_BITS));
    }

    #[test]
    fn the_corner_of_the_cell_tolerates_a_couple_of_pixels() {
        assert!(is_at_cell(Some((100, 200, 800, 600)), (101, 198, 500, 400)));
        assert!(!is_at_cell(Some((110, 200, 800, 600)), (100, 200, 500, 400)));
        assert!(!is_at_cell(None, (100, 200, 500, 400)));
    }

    /// O tamanho da grade é o de duas ou mais janelas (ou da única aberta).
    #[test]
    fn the_cell_in_use_is_the_size_shared_by_the_grid_windows() {
        let at = |w, h| RobloxWin { hwnd: 0, pid: 0, x: 0, y: 0, w, h };
        assert_eq!(grid_cell_in_use(&[at(1272, 695)]), Some((1272, 695)));
        assert_eq!(grid_cell_in_use(&[at(1272, 695), at(1272, 695), at(816, 639)]), Some((1272, 695)));
        assert_eq!(grid_cell_in_use(&[at(900, 700), at(1000, 700), at(816, 639)]), None);
        assert_eq!(grid_cell_in_use(&[]), None);
    }

    /// Célula das outras mais larga e mais baixa que a da nova também conta
    /// como grade encolhida.
    #[test]
    fn a_grid_shrunk_in_one_dimension_counts_as_shrunk() {
        assert!(grid_already_shrunk((845, 461), (816, 639)));
        assert!(grid_already_shrunk((600, 450), (816, 639)));
        assert!(!grid_already_shrunk((816, 639), (816, 639)));
        assert!(!grid_already_shrunk((1000, 700), (816, 639)));
    }

    // ── sem moldura ────────────────────────────────────────────────────────

    #[test]
    fn borderless_removes_the_title_bar_and_the_border_only() {
        let roblox = WS_OVERLAPPEDWINDOW | WS_VISIBLE | WS_CLIPCHILDREN;
        let bare = borderless_style(roblox);
        assert_eq!(bare & WS_CAPTION, 0);
        assert_eq!(bare & WS_THICKFRAME, 0);
        assert_ne!(bare & WS_VISIBLE, 0);
        assert_ne!(bare & WS_CLIPCHILDREN, 0);
        // O botão da barra de tarefas continua minimizando.
        assert_ne!(bare & WS_MINIMIZEBOX, 0);
        assert_ne!(bare & WS_MAXIMIZEBOX, 0);
        assert_ne!(bare & WS_SYSMENU, 0);
    }

    #[test]
    fn restoring_gives_back_exactly_the_frame_the_window_had() {
        let roblox = WS_OVERLAPPEDWINDOW | WS_VISIBLE | WS_CLIPCHILDREN;
        assert_eq!(restored_frame_style(borderless_style(roblox), roblox), roblox);
        // Uma janela sem borda redimensionável não ganha uma.
        let fixed = (WS_OVERLAPPEDWINDOW & !WS_THICKFRAME) | WS_VISIBLE;
        let restored = restored_frame_style(borderless_style(fixed), fixed);
        assert_eq!(restored & WS_THICKFRAME, 0);
        assert_eq!(restored, fixed);
    }

    #[test]
    fn a_borderless_cell_is_not_mistaken_for_fullscreen() {
        let bare = borderless_style(WS_OVERLAPPEDWINDOW | WS_VISIBLE);
        let monitor = (0, 0, 1920, 1080);
        assert_eq!(window_mode(false, bare, (0, 0, 640, 480), monitor), WindowMode::Normal);
    }

    #[test]
    fn the_grid_options_are_off_by_default() {
        assert_eq!(
            GridWindowStyle::default(),
            GridWindowStyle {
                allow_small: false,
                borderless: false,
                auto_fit: false
            }
        );
    }
}

#[cfg(test)]
mod win_client_window_tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{WS_CLIPCHILDREN, WS_VISIBLE};

    // Só as decisões: medir e mover a janela de verdade precisa de um cliente
    // Roblox aberto.

    /// Janela típica do Windows 11 a 100%: 8 px de borda de cada lado, 31 px
    /// de barra de título.
    const OUTER: (i32, i32) = (536, 459);
    const CLIENT: (i32, i32) = (520, 420);

    #[test]
    fn a_window_whose_client_area_matches_the_xml_reveals_client_semantics() {
        assert_eq!(
            infer_size_semantics((520, 420), OUTER, CLIENT),
            Some(SizeSemantics::Client)
        );
    }

    #[test]
    fn a_window_whose_outer_size_matches_the_xml_reveals_outer_semantics() {
        assert_eq!(
            infer_size_semantics((536, 459), OUTER, CLIENT),
            Some(SizeSemantics::Outer)
        );
    }

    /// O caso do bug: o XML foi reescrito por outro cliente e a janela abriu
    /// com o tamanho de outra conta — ela não ensina nada.
    #[test]
    fn a_window_that_matches_neither_teaches_nothing() {
        assert_eq!(infer_size_semantics((1000, 1000), OUTER, CLIENT), None);
    }

    #[test]
    fn with_client_semantics_the_frame_is_added_on_top_of_the_size() {
        assert_eq!(
            outer_size_for_target((520, 420), SizeSemantics::Client, OUTER, CLIENT),
            (536, 459)
        );
        // A borda medida é a desta janela: com outro DPI ela é outra.
        assert_eq!(
            outer_size_for_target((520, 420), SizeSemantics::Client, (540, 469), (520, 420)),
            (540, 469)
        );
    }

    #[test]
    fn with_outer_semantics_the_size_is_the_window_rectangle() {
        assert_eq!(
            outer_size_for_target((520, 420), SizeSemantics::Outer, OUTER, CLIENT),
            (520, 420)
        );
    }

    #[test]
    fn the_learned_semantics_starts_as_client_area() {
        assert_eq!(SizeSemantics::from_u8(SizeSemantics::UNKNOWN), SizeSemantics::Client);
        assert_eq!(SizeSemantics::from_u8(SizeSemantics::Outer as u8), SizeSemantics::Outer);
    }

    // ── client_window_target ────────────────────────────────────────────────

    #[test]
    fn a_size_only_plan_keeps_the_window_where_it_is() {
        let target = client_window_target(
            (300, 200, 1016, 1039),
            (1000, 1000),
            Some((520, 420)),
            None,
            SizeSemantics::Client,
        );
        // Borda desta janela: 16 x 39.
        assert_eq!(target, (300, 200, 536, 459));
    }

    #[test]
    fn a_saved_rectangle_moves_the_window_and_the_size_still_comes_from_the_plan() {
        let target = client_window_target(
            (300, 200, 1016, 1039),
            (1000, 1000),
            Some((1000, 1000)),
            Some((-1910, 20, 700, 500)),
            SizeSemantics::Client,
        );
        assert_eq!(target, (-1910, 20, 1016, 1039));
    }

    #[test]
    fn a_saved_rectangle_without_a_size_restores_the_saved_size() {
        let target = client_window_target(
            (300, 200, 1016, 1039),
            (1000, 1000),
            None,
            Some((-1910, 20, 700, 500)),
            SizeSemantics::Client,
        );
        assert_eq!(target, (-1910, 20, 700, 500));
    }

    // ── tela cheia / maximizada ─────────────────────────────────────────────

    const MONITOR: (i32, i32, i32, i32) = (0, 0, 1920, 1080);

    #[test]
    fn a_maximized_window_is_left_alone() {
        assert!(is_fullscreen_like(true, WS_CAPTION, (0, 0, 1920, 1040), MONITOR));
    }

    #[test]
    fn a_captionless_window_covering_the_monitor_is_fullscreen() {
        assert!(is_fullscreen_like(false, 0, (0, 0, 1920, 1080), MONITOR));
    }

    #[test]
    fn a_normal_window_is_not_fullscreen() {
        assert!(!is_fullscreen_like(false, WS_CAPTION, (100, 100, 536, 459), MONITOR));
        // Sem barra de título, mas menor que o monitor: janela comum.
        assert!(!is_fullscreen_like(false, 0, (100, 100, 536, 459), MONITOR));
    }

    // ── sair da tela cheia herdada (03/10/2026) ─────────────────────────────

    /// Maximizada (com barra de título) e tela cheia do Roblox (sem barra,
    /// cobrindo o monitor) são coisas diferentes: só a segunda é tirada.
    #[test]
    fn the_window_mode_tells_maximized_from_fullscreen() {
        assert_eq!(window_mode(true, WS_CAPTION, (0, 0, 1920, 1040), MONITOR), WindowMode::Maximized);
        assert_eq!(window_mode(false, WS_POPUP, (0, 0, 1920, 1080), MONITOR), WindowMode::Fullscreen);
        // Popup maximizado cobrindo o monitor continua sendo tela cheia.
        assert_eq!(window_mode(true, WS_POPUP, (0, 0, 1920, 1080), MONITOR), WindowMode::Fullscreen);
        assert_eq!(window_mode(false, WS_OVERLAPPEDWINDOW, (100, 100, 536, 459), MONITOR), WindowMode::Normal);
        // Monitor da esquerda, com origem negativa.
        assert_eq!(
            window_mode(false, WS_POPUP, (-1920, 0, 1920, 1080), (-1920, 0, 1920, 1080)),
            WindowMode::Fullscreen
        );
    }

    #[test]
    fn leaving_fullscreen_only_when_the_plan_asks_and_once() {
        assert!(should_leave_fullscreen(true, WindowMode::Fullscreen, false));
        // Uma tentativa só: se o Roblox voltar para a tela cheia, o app não briga.
        assert!(!should_leave_fullscreen(true, WindowMode::Fullscreen, true));
        // Plano sem "em janela": a tela cheia é do jogador.
        assert!(!should_leave_fullscreen(false, WindowMode::Fullscreen, false));
        // Maximizada não é tela cheia herdada — o usuário (ou o Roblox dele) pôs.
        assert!(!should_leave_fullscreen(true, WindowMode::Maximized, false));
        assert!(!should_leave_fullscreen(true, WindowMode::Normal, false));
    }

    /// Com a grade, a janela maximizada volta ao normal — uma vez: se o Roblox
    /// (ou a pessoa) maximizar de novo depois, o app não briga.
    #[test]
    fn leaving_maximized_only_for_the_grid_and_once() {
        assert!(should_leave_maximized(true, WindowMode::Maximized, false));
        assert!(!should_leave_maximized(true, WindowMode::Maximized, true));
        assert!(!should_leave_maximized(false, WindowMode::Maximized, false));
        assert!(!should_leave_maximized(true, WindowMode::Normal, false));
        assert!(!should_leave_maximized(true, WindowMode::Fullscreen, false));
    }

    #[test]
    fn the_windowed_style_brings_back_the_frame_and_drops_the_popup() {
        let fullscreen = WS_POPUP | WS_VISIBLE | WS_CLIPCHILDREN;
        let windowed = windowed_style(fullscreen);
        assert_eq!(windowed & WS_POPUP, 0);
        assert_eq!(windowed & WS_OVERLAPPEDWINDOW, WS_OVERLAPPEDWINDOW);
        // O resto do estilo fica como estava.
        assert_eq!(windowed & (WS_VISIBLE | WS_CLIPCHILDREN), WS_VISIBLE | WS_CLIPCHILDREN);
    }

    const WORK: (i32, i32, i32, i32) = (0, 0, 1920, 1040);

    /// A janela sai da tela cheia já no tamanho do plano, centralizada na área
    /// de trabalho do monitor em que estava.
    #[test]
    fn the_window_leaves_fullscreen_at_the_planned_size_centered() {
        let rect = exit_fullscreen_rect(WORK, Some((520, 420)), SizeSemantics::Client, (16, 39));
        assert_eq!(rect, (692, 290, 536, 459));
        let outer = exit_fullscreen_rect(WORK, Some((520, 420)), SizeSemantics::Outer, (16, 39));
        assert_eq!((outer.2, outer.3), (520, 420));
    }

    #[test]
    fn without_a_planned_size_the_window_gets_a_modest_one() {
        let rect = exit_fullscreen_rect(WORK, None, SizeSemantics::Client, (16, 39));
        // 1280 x (1040 * 2/3), mais a borda.
        assert_eq!((rect.2, rect.3), (1296, 732));
        let small = exit_fullscreen_rect((0, 0, 1200, 900), None, SizeSemantics::Client, (0, 0));
        assert_eq!((small.2, small.3), (800, 600));
    }

    #[test]
    fn the_exit_rectangle_never_leaves_the_work_area() {
        let rect = exit_fullscreen_rect((-1920, 0, 1920, 1040), Some((4000, 3000)), SizeSemantics::Client, (16, 39));
        assert_eq!(rect, (-1920, 0, 1920, 1040));
    }
}

#[cfg(test)]
mod win_focus_tests {
    use super::*;

    // Só a decisão é coberta: `ShowWindow` e `SetForegroundWindow` de verdade
    // precisam de janela e de desktop.

    #[test]
    fn a_minimized_window_is_restored_before_it_comes_to_the_front() {
        assert_eq!(
            show_command_before_focus(FocusPurpose::Bring, true),
            Some(SW_RESTORE)
        );
    }

    /// `SW_RESTORE` numa janela maximizada a devolve ao tamanho normal: era o
    /// AFK mode tirando do maximizado o cliente alvo a cada ciclo.
    #[test]
    fn a_maximized_or_normal_window_comes_to_the_front_as_it_is() {
        assert_eq!(show_command_before_focus(FocusPurpose::Bring, false), None);
    }

    /// Devolver o foco é devolver a janela em que o usuário estava trabalhando:
    /// nem desmaximizar, nem restaurar — nem quando o foco tinha sido negado e
    /// nada foi enviado.
    #[test]
    fn giving_the_focus_back_never_changes_the_window_state() {
        assert_eq!(show_command_before_focus(FocusPurpose::GiveBack, false), None);
        assert_eq!(show_command_before_focus(FocusPurpose::GiveBack, true), None);
    }

    // ── devolver a janela da conta ao lugar (ideia 44) ──────────────────────

    /// `SW_MINIMIZE` na janela em primeiro plano faz o Windows ativar a próxima
    /// da pilha (outro Roblox, às vezes): era o piscar entre janelas.
    #[test]
    fn the_window_goes_back_to_the_taskbar_without_activating_another() {
        assert_eq!(QUIET_MINIMIZE, SW_SHOWMINNOACTIVE);
        assert_ne!(QUIET_MINIMIZE, SW_MINIMIZE);
    }

    #[test]
    fn putting_a_window_back_in_depth_never_activates_moves_or_resizes_it() {
        assert_ne!(PUT_BACK_SWP_FLAGS & SWP_NOACTIVATE, 0);
        assert_ne!(PUT_BACK_SWP_FLAGS & SWP_NOMOVE, 0);
        assert_ne!(PUT_BACK_SWP_FLAGS & SWP_NOSIZE, 0);
        // A ordem é justamente o que muda.
        assert_eq!(PUT_BACK_SWP_FLAGS & SWP_NOZORDER, 0);
    }

    fn neighbor(hwnd: isize, visible: bool, minimized: bool, topmost: bool) -> ZNeighbor {
        ZNeighbor {
            hwnd,
            visible,
            minimized,
            topmost,
        }
    }

    #[test]
    fn the_window_above_is_the_first_one_that_shows_on_screen() {
        let chain = [
            neighbor(1, false, false, false),
            neighbor(2, true, true, false),
            neighbor(3, true, false, false),
            neighbor(4, true, false, false),
        ];
        assert_eq!(pick_window_above(chain), 3);
    }

    /// Embaixo de uma janela "sempre no topo", a janela da conta viraria
    /// "sempre no topo" também.
    #[test]
    fn an_always_on_top_window_is_never_the_reference() {
        let chain = [neighbor(1, true, false, true), neighbor(2, true, false, false)];
        assert_eq!(pick_window_above(chain), 0);
    }

    #[test]
    fn a_window_at_the_top_has_nothing_above_it() {
        assert_eq!(pick_window_above([]), 0);
        assert_eq!(pick_window_above([neighbor(1, false, false, false)]), 0);
        assert_eq!(window_above(std::ptr::null_mut()), std::ptr::null_mut());
    }

    #[test]
    fn a_null_window_is_never_put_back() {
        assert!(!minimize_window_quietly(std::ptr::null_mut()));
        assert!(!put_window_below(std::ptr::null_mut(), std::ptr::null_mut()));
    }

    // ── devolver o foco depois do ciclo (issue #23) ─────────────────────────

    /// Windows de mentira: guarda quem está na frente e recusa o
    /// `SetForegroundWindow` quantas vezes mandarem — o que o Windows faz
    /// quando o usuário mexe em outra janela durante o ciclo.
    struct FakeDesktop {
        foreground: std::cell::Cell<isize>,
        plain_refusals: std::cell::Cell<u32>,
        nudged_refusals: std::cell::Cell<u32>,
        user_clicks_during_settle: Option<isize>,
        calls: std::cell::RefCell<Vec<&'static str>>,
    }

    impl FakeDesktop {
        fn new(foreground: isize) -> Self {
            Self {
                foreground: std::cell::Cell::new(foreground),
                plain_refusals: std::cell::Cell::new(0),
                nudged_refusals: std::cell::Cell::new(0),
                user_clicks_during_settle: None,
                calls: std::cell::RefCell::new(Vec::new()),
            }
        }
    }

    impl ForegroundApi for FakeDesktop {
        fn foreground(&self) -> isize {
            self.foreground.get()
        }
        fn exists(&self, hwnd: isize) -> bool {
            hwnd != 0
        }
        fn set_foreground(&self, hwnd: isize) -> bool {
            self.calls.borrow_mut().push("plain");
            if self.plain_refusals.get() > 0 {
                self.plain_refusals.set(self.plain_refusals.get() - 1);
                return false;
            }
            self.foreground.set(hwnd);
            true
        }
        fn set_foreground_after_nudge(&self, hwnd: isize) -> bool {
            self.calls.borrow_mut().push("nudged");
            if self.nudged_refusals.get() > 0 {
                self.nudged_refusals.set(self.nudged_refusals.get() - 1);
                return false;
            }
            self.foreground.set(hwnd);
            true
        }
        fn bring(&self, hwnd: isize) -> bool {
            self.calls.borrow_mut().push("bring");
            if self.plain_refusals.get() > 0 {
                self.plain_refusals.set(self.plain_refusals.get() - 1);
                return false;
            }
            self.foreground.set(hwnd);
            true
        }
        fn bring_after_nudge(&self, hwnd: isize) -> bool {
            self.calls.borrow_mut().push("bring_nudged");
            if self.nudged_refusals.get() > 0 {
                self.nudged_refusals.set(self.nudged_refusals.get() - 1);
                return false;
            }
            self.foreground.set(hwnd);
            true
        }
        fn settle(&self) {
            if let Some(other) = self.user_clicks_during_settle {
                self.foreground.set(other);
            }
        }
    }

    const USER: isize = 100;
    const ROBLOX: isize = 7;

    #[test]
    fn the_focus_goes_back_when_windows_allows_it() {
        let desk = FakeDesktop::new(ROBLOX);
        assert_eq!(give_focus_back_with(&desk, USER, &[ROBLOX]), GiveBack::Restored);
        assert_eq!(desk.foreground(), USER);
    }

    /// Issue #23: com o usuário mexendo no outro monitor, o Windows recusa o
    /// `SetForegroundWindow` e o Roblox ficava na frente. Agora a recusa é
    /// vista e a volta tenta de novo depois de um movimento de mouse de zero
    /// pixel, que devolve a este processo o direito de trocar o foco.
    #[test]
    fn a_refused_give_back_is_retried_and_the_focus_returns() {
        let desk = FakeDesktop::new(ROBLOX);
        desk.plain_refusals.set(5);
        assert_eq!(give_focus_back_with(&desk, USER, &[ROBLOX]), GiveBack::Restored);
        assert_eq!(desk.foreground(), USER);
        assert!(desk.calls.borrow().contains(&"nudged"));
    }

    /// A chamada pode "dar certo" e a janela não vir: vale o que está na
    /// frente depois, não o retorno.
    #[test]
    fn the_result_is_checked_on_screen_not_trusted_from_the_call() {
        struct Liar(std::cell::Cell<isize>);
        impl ForegroundApi for Liar {
            fn foreground(&self) -> isize {
                self.0.get()
            }
            fn exists(&self, _: isize) -> bool {
                true
            }
            fn set_foreground(&self, _: isize) -> bool {
                true
            }
            fn set_foreground_after_nudge(&self, hwnd: isize) -> bool {
                self.0.set(hwnd);
                true
            }
            fn bring(&self, _: isize) -> bool {
                true
            }
            fn bring_after_nudge(&self, hwnd: isize) -> bool {
                self.0.set(hwnd);
                true
            }
            fn settle(&self) {}
        }
        let desk = Liar(std::cell::Cell::new(ROBLOX));
        assert_eq!(give_focus_back_with(&desk, USER, &[ROBLOX]), GiveBack::Restored);
        assert_eq!(desk.foreground(), USER);
    }

    /// Tudo recusado: o resultado diz isso, sem insistir para sempre.
    #[test]
    fn a_give_back_refused_every_time_is_reported_and_stops() {
        let desk = FakeDesktop::new(ROBLOX);
        desk.plain_refusals.set(99);
        desk.nudged_refusals.set(99);
        assert_eq!(give_focus_back_with(&desk, USER, &[ROBLOX]), GiveBack::Denied);
        assert!(desk.calls.borrow().len() <= 4);
    }

    /// Clicou em outra janela durante o ciclo: a pessoa já foi para outro
    /// lugar. Puxar de volta a janela de antes roubaria o foco dela.
    #[test]
    fn a_window_the_user_picked_during_the_cycle_keeps_the_focus() {
        let desk = FakeDesktop::new(55);
        assert_eq!(give_focus_back_with(&desk, USER, &[ROBLOX]), GiveBack::UserMovedOn);
        assert_eq!(desk.foreground(), 55);
        assert!(desk.calls.borrow().is_empty());
    }

    #[test]
    fn a_user_click_while_waiting_stops_the_retries() {
        let mut desk = FakeDesktop::new(ROBLOX);
        desk.plain_refusals.set(5);
        desk.user_clicks_during_settle = Some(55);
        assert_eq!(give_focus_back_with(&desk, USER, &[ROBLOX]), GiveBack::UserMovedOn);
        assert_eq!(desk.foreground(), 55);
    }

    // ── trazer a janela da conta no começo do envio ─────────────────────────

    /// Teste do dono (10/10/2026) com o hotfix do #23: com o foco devolvido de
    /// verdade, o ciclo seguinte encontrava a janela dele na frente e o Windows
    /// recusava trazer o Roblox — todas as contas ficavam "não enviada". Mesma
    /// causa e mesma saída: o movimento de zero pixel e uma nova tentativa.
    #[test]
    fn a_refused_bring_is_retried_after_the_nudge() {
        let desk = FakeDesktop::new(USER);
        desk.plain_refusals.set(1);
        assert!(bring_forward_with(&desk, ROBLOX));
        assert_eq!(desk.foreground(), ROBLOX);
        assert_eq!(*desk.calls.borrow(), vec!["bring", "bring_nudged"]);
    }

    #[test]
    fn a_bring_that_works_needs_no_nudge() {
        let desk = FakeDesktop::new(USER);
        assert!(bring_forward_with(&desk, ROBLOX));
        assert_eq!(*desk.calls.borrow(), vec!["bring"]);
    }

    /// Recusado mesmo assim: devolve falso e o ciclo não envia nada (a
    /// conferência de `afk_window_is_ready` continua valendo).
    #[test]
    fn a_bring_refused_twice_reports_false() {
        let desk = FakeDesktop::new(USER);
        desk.plain_refusals.set(9);
        desk.nudged_refusals.set(9);
        assert!(!bring_forward_with(&desk, ROBLOX));
        assert_eq!(desk.foreground(), USER);
        assert_eq!(desk.calls.borrow().len(), 2);
    }

    #[test]
    fn nothing_is_done_when_the_focus_is_already_back_or_the_window_is_gone() {
        let desk = FakeDesktop::new(USER);
        assert_eq!(give_focus_back_with(&desk, USER, &[ROBLOX]), GiveBack::Restored);
        assert!(desk.calls.borrow().is_empty());
        let desk = FakeDesktop::new(ROBLOX);
        assert_eq!(give_focus_back_with(&desk, 0, &[ROBLOX]), GiveBack::Gone);
        assert!(desk.calls.borrow().is_empty());
    }
}

#[cfg(test)]
mod grid_overflow_real_tests {
    use super::*;

    // ---- Cenário de verdade (rodar à mão, sem Roblox aberto) -----------------
    //
    //   cargo test --all-features real_grid_scenario -- --ignored --test-threads=1
    //
    // Relato do Discord (25/10/2026): "they come on top of one another". Mais
    // janelas do que cabem no monitor no tamanho mínimo do Roblox (~816x639):
    // a grade dava a volta e empilhava. As janelas daqui são de verdade — uma
    // cópia do PowerShell com o nome do cliente abrindo um formulário com o
    // **mesmo tamanho mínimo** do Roblox (o Windows o impõe pelo mesmo
    // `WM_GETMINMAXINFO`) — e ficam registradas como abertas pelo app.

    const POWERSHELL: &str = r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe";
    // Janela Win32 crua (`NativeWindow`, sem a lógica do `Form`): o mínimo vem
    // só do `WM_GETMINMAXINFO`, como no Roblox. O `Form` do WinForms guardava a
    // própria posição de restauração e a reaplicava — ao contrário do Roblox,
    // que volta onde o `rcNormalPosition` manda.
    const FAKE_CLIENT: &str = r#"Add-Type -AssemblyName System.Windows.Forms
Add-Type -ReferencedAssemblies System.Windows.Forms -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Windows.Forms;
public class FakeRobloxWindow : NativeWindow {
    [StructLayout(LayoutKind.Sequential)] struct Pt { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] struct MinMax { public Pt Reserved, MaxSize, MaxPos, MinTrack, MaxTrack; }
    public FakeRobloxWindow() {
        CreateParams cp = new CreateParams();
        cp.Caption = "FakeRobloxGrid";
        cp.X = 200; cp.Y = 200; cp.Width = 816; cp.Height = 639;
        cp.Style = 0x10CF0000; // WS_OVERLAPPEDWINDOW | WS_VISIBLE
        CreateHandle(cp);
    }
    protected override void WndProc(ref Message m) {
        if (m.Msg == 0x0024) { // WM_GETMINMAXINFO
            MinMax mm = (MinMax)Marshal.PtrToStructure(m.LParam, typeof(MinMax));
            mm.MinTrack.X = 816; mm.MinTrack.Y = 639;
            Marshal.StructureToPtr(mm, m.LParam, false);
            m.Result = IntPtr.Zero;
            return;
        }
        base.WndProc(ref m);
    }
}
'@
$w = New-Object FakeRobloxWindow
[Windows.Forms.Application]::Run()
"#;

    struct FakeClients {
        children: Vec<std::process::Child>,
        dir: std::path::PathBuf,
        user_ids: Vec<i64>,
        exe: std::path::PathBuf,
        script: std::path::PathBuf,
        next_uid: i64,
    }

    impl FakeClients {
        fn start_child(&self) -> std::process::Child {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            std::process::Command::new(&self.exe)
                .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(&self.script)
                .creation_flags(CREATE_NO_WINDOW)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("spawn fake client")
        }

        fn track(&mut self, pid: u32) {
            let uid = self.next_uid;
            self.next_uid += 1;
            tracker().track(uid, pid, String::new());
            self.user_ids.push(uid);
        }

        /// Uma conta nova abrindo (o caminho do launch): devolve a janela.
        fn spawn_one(&mut self) -> HWND {
            let child = self.start_child();
            let pid = child.id();
            self.children.push(child);
            for _ in 0..200 {
                if let Some(hwnd) = find_main_window(pid) {
                    self.track(pid);
                    return hwnd;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            panic!("the fake client did not open");
        }

        /// O cliente `i` fecha (a conta saiu do jogo).
        fn close(&mut self, i: usize) {
            let mut child = self.children.remove(i);
            let _ = child.kill();
            let _ = child.wait();
            let uid = self.user_ids.remove(i);
            tracker().untrack(uid);
        }

        fn windows(&self) -> Vec<HWND> {
            self.children.iter().filter_map(|c| find_main_window(c.id())).collect()
        }
    }

    impl Drop for FakeClients {
        fn drop(&mut self) {
            for child in &mut self.children {
                let _ = child.kill();
                let _ = child.wait();
            }
            for uid in &self.user_ids {
                tracker().untrack(*uid);
            }
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn spawn_fake_clients(count: usize) -> FakeClients {
        assert!(get_roblox_pids().is_empty(), "close every Roblox client first");
        let dir = std::env::temp_dir().join(format!("ram-fake-grid-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let exe = dir.join("RobloxPlayerBeta.exe");
        std::fs::copy(POWERSHELL, &exe).expect("copy");
        let script = dir.join("fake-client.ps1");
        std::fs::write(&script, FAKE_CLIENT).expect("script");
        let mut clients = FakeClients {
            children: Vec::new(),
            dir,
            user_ids: Vec::new(),
            exe,
            script,
            next_uid: 900_000,
        };
        for _ in 0..count {
            let child = clients.start_child();
            clients.children.push(child);
        }
        for _ in 0..200 {
            if list_roblox_windows().len() >= count {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(list_roblox_windows().len(), count, "the fake clients did not open");
        let pids: Vec<u32> = clients.children.iter().map(|c| c.id()).collect();
        for pid in pids {
            clients.track(pid);
        }
        clients
    }

    fn primary_monitor_index() -> usize {
        list_monitors().into_iter().find(|m| m.primary).map(|m| m.index).expect("primary monitor")
    }

    /// Pares de janelas cujas áreas visíveis se cruzam.
    fn overlapping_pairs() -> Vec<(ScreenRect, ScreenRect)> {
        let rects: Vec<ScreenRect> = list_roblox_windows()
            .into_iter()
            .filter_map(|w| visible_bounds(w.hwnd as HWND))
            .collect();
        let mut pairs = Vec::new();
        for i in 0..rects.len() {
            for j in i + 1..rects.len() {
                if !rects[i].intersect(&rects[j]).is_empty() {
                    pairs.push((rects[i], rects[j]));
                }
            }
        }
        pairs
    }

    /// Janelas cuja área visível sai do monitor da grade.
    fn windows_outside(monitor: usize) -> usize {
        let m = selected_monitors(&list_monitors(), &[monitor])[0].clone();
        list_roblox_windows()
            .into_iter()
            .filter_map(|w| visible_bounds(w.hwnd as HWND))
            .filter(|r| r.left < m.x || r.top < m.y || r.right > m.x + m.width || r.bottom > m.y + m.height)
            .count()
    }

    fn slots_at_min_size(monitor: usize, gap: i32) -> usize {
        let monitors = selected_monitors(&list_monitors(), &[monitor]);
        grid_slots(&monitors, 816, 639, gap).len()
    }

    /// Versão lenta para gravar em vídeo: espera `RAM_GRID_DEMO_START_S`
    /// segundos (para começar a gravação), abre as janelas empilhadas, mostra
    /// assim por 5 s, arruma e deixa o resultado 10 s na tela.
    #[test]
    #[ignore]
    fn real_grid_demo_for_recording() {
        let start_s: u64 = std::env::var("RAM_GRID_DEMO_START_S").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        std::thread::sleep(Duration::from_secs(start_s));
        let monitor = primary_monitor_index();
        let gap = 8;
        let count = slots_at_min_size(monitor, gap) + 8;
        let _clients = spawn_fake_clients(count);
        std::thread::sleep(Duration::from_secs(5));
        let result = arrange_roblox_grid(
            &[monitor],
            gap,
            None,
            &std::collections::HashSet::new(),
            GridWindowStyle::default(),
        );
        std::thread::sleep(Duration::from_secs(10));
        eprintln!("demo: {count} windows -> {result:?}, overlapping pairs: {}", overlapping_pairs().len());
    }

    #[test]
    #[ignore]
    fn real_grid_scenario_arrange_with_more_windows_than_cells() {
        let monitor = primary_monitor_index();
        let gap = 8;
        for extra in [2usize, 8] {
        let count = slots_at_min_size(monitor, gap) + extra;
        let _clients = spawn_fake_clients(count);
        let result = arrange_roblox_grid(
            &[monitor],
            gap,
            None,
            &std::collections::HashSet::new(),
            GridWindowStyle::default(),
        );
        std::thread::sleep(Duration::from_millis(600));
        let overlaps = overlapping_pairs();
        eprintln!("arrange {count} windows -> {result:?}, overlapping pairs: {}", overlaps.len());
        assert!(overlaps.is_empty(), "windows on top of one another: {overlaps:?}");
        assert_eq!(windows_outside(monitor), 0, "a window left the monitor");
        }
    }

    /// Bateria de testes de 25/10/2026: minimizar tudo, "Organizar em grade" e
    /// restaurar. Com célula menor que o mínimo, a minimizada só ganhava o
    /// `rcNormalPosition`; ao restaurar, o Roblox impunha o mínimo e as janelas
    /// voltavam umas por cima das outras, saindo do monitor.
    #[test]
    #[ignore]
    fn real_grid_scenario_minimized_windows_keep_the_small_cell_when_restored() {
        let monitor = primary_monitor_index();
        let gap = 8;
        let count = slots_at_min_size(monitor, gap) + 2;
        // Os dois caminhos: minimizar antes da primeira organização (a célula
        // encolhe agora) e organizar → minimizar → organizar de novo (o
        // `rcNormalPosition` já é pequeno e a célula cabe sem encolher — o
        // caso da bateria com 4 contas de verdade).
        for arrange_first in [false, true] {
            let clients = spawn_fake_clients(count);
            let hwnds: Vec<HWND> = clients
                .children
                .iter()
                .map(|c| find_main_window(c.id()).expect("window"))
                .collect();
            // O caminho do launch, que deixa as minimizadas minimizadas (o
            // botão as traz de volta: teste abaixo).
            let arrange = || {
                arrange_roblox_grid_locked(
                    &[monitor],
                    gap,
                    None,
                    &std::collections::HashSet::new(),
                    GridWindowStyle::default(),
                    false,
                )
            };
            if arrange_first {
                let _ = arrange();
                std::thread::sleep(Duration::from_millis(400));
            }
            for &hwnd in &hwnds {
                unsafe { ShowWindow(hwnd, SW_MINIMIZE) };
            }
            std::thread::sleep(Duration::from_millis(400));
            let result = arrange();
            for &hwnd in &hwnds {
                unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
            }
            std::thread::sleep(Duration::from_millis(400));
            // A olhada de 1 s do app.
            snap_restored_small_windows();
            std::thread::sleep(Duration::from_millis(600));
            let overlaps = overlapping_pairs();
            eprintln!(
                "arrange_first={arrange_first}: minimized + arrange + restore of {count} windows -> {result:?}, overlapping pairs: {}",
                overlaps.len()
            );
            assert!(overlaps.is_empty(), "arrange_first={arrange_first}: windows on top of one another: {overlaps:?}");
            assert_eq!(windows_outside(monitor), 0, "arrange_first={arrange_first}: a window left the monitor");
        }
    }

    #[test]
    #[ignore]
    fn real_grid_scenario_a_website_client_shrinks_with_the_rest() {
        // Relato do Discord (30/10/2026): "Windows stopped shrinking". Com uma
        // janela aberta pelo site na grade, nenhuma encolhia — nem as do app.
        let monitor = primary_monitor_index();
        let mut clients = spawn_fake_clients(3);
        // A última vira "aberta pelo site": sai do rastreamento do app.
        let site_uid = clients.user_ids.pop().expect("uid");
        tracker().untrack(site_uid);
        let style = GridWindowStyle { allow_small: true, ..GridWindowStyle::default() };
        let result = arrange_roblox_grid_locked(
            &[monitor],
            8,
            Some((400, 300)),
            &std::collections::HashSet::new(),
            style,
            true,
        );
        std::thread::sleep(Duration::from_millis(600));
        let sizes: Vec<(i32, i32)> = list_roblox_windows().iter().map(|w| (w.w, w.h)).collect();
        eprintln!("arrange with a website client -> {result:?}, sizes: {sizes:?}");
        assert_eq!(sizes.len(), 3);
        for (w, h) in sizes {
            assert!(w < 816 && h < 639, "a window kept the Roblox minimum: {w}x{h}");
        }
        assert!(overlapping_pairs().is_empty());
    }

    #[test]
    #[ignore]
    fn real_grid_scenario_live_keeps_a_window_minimized_later_rendering() {
        // Relato do Discord (30/10/2026): conta minimizada com a Live aberta
        // congelava. Minimizar depois de a Live montar a grade também tem que
        // voltar (invisível), inclusive minimizar de novo a mesma janela.
        let clients = spawn_fake_clients(1);
        let uid = clients.user_ids[0];
        let hwnd = find_main_window(clients.children[0].id()).expect("window");
        let iconic = || unsafe { IsIconic(hwnd) } != 0;
        for round in 0..2 {
            unsafe { ShowWindow(hwnd, SW_MINIMIZE) };
            std::thread::sleep(Duration::from_millis(300));
            assert!(iconic(), "round {round}: not minimized");
            assert!(live_keep_rendering(uid, hwnd as isize), "round {round}");
            std::thread::sleep(Duration::from_millis(300));
            assert!(!iconic(), "round {round}: still minimized, the Live would freeze");
        }
        // A Live fecha: volta a minimizar, e a transparência sai.
        live_release_hidden();
        std::thread::sleep(Duration::from_millis(300));
        assert!(iconic());
    }

    /// Pedido do dono (26/10/2026): com as janelas minimizadas, o botão
    /// "Arrange in grid" só mudava onde elas voltariam, e ele tinha de abrir uma
    /// por uma. O botão agora as traz de volta, cada uma na sua célula.
    #[test]
    #[ignore]
    fn real_grid_scenario_arrange_button_brings_minimized_windows_back() {
        let monitor = primary_monitor_index();
        let gap = 8;
        let count = 4;
        let clients = spawn_fake_clients(count);
        let hwnds: Vec<HWND> = clients
            .children
            .iter()
            .map(|c| find_main_window(c.id()).expect("window"))
            .collect();
        // Metade minimizada, metade aberta — e uma minimizada depois de maximizada.
        unsafe { ShowWindow(hwnds[0], windows_sys::Win32::UI::WindowsAndMessaging::SW_MAXIMIZE) };
        std::thread::sleep(Duration::from_millis(300));
        for &hwnd in &hwnds[..2] {
            unsafe { ShowWindow(hwnd, SW_MINIMIZE) };
        }
        std::thread::sleep(Duration::from_millis(400));
        let result = arrange_roblox_grid(
            &[monitor],
            gap,
            None,
            &std::collections::HashSet::new(),
            GridWindowStyle::default(),
        );
        std::thread::sleep(Duration::from_millis(600));
        let still_minimized = hwnds.iter().filter(|&&h| unsafe { IsIconic(h) } != 0).count();
        let maximized = hwnds.iter().filter(|&&h| unsafe { IsZoomed(h) } != 0).count();
        let overlaps = overlapping_pairs();
        eprintln!(
            "arrange with {count} windows (2 minimized) -> {result:?}, still minimized: {still_minimized}, maximized: {maximized}, overlapping pairs: {}",
            overlaps.len()
        );
        assert_eq!(still_minimized, 0, "the button left windows minimized");
        assert_eq!(maximized, 0, "a window came back maximized over the grid");
        assert!(overlaps.is_empty(), "windows on top of one another: {overlaps:?}");
        assert_eq!(windows_outside(monitor), 0, "a window left the monitor");
    }

    /// "Best size for each monitor" com janelas de verdade: todas cabem, sem
    /// sobrepor nem sair do monitor, e na célula que a conta deu — inclusive
    /// abrindo uma por uma, como no launch.
    #[test]
    #[ignore]
    fn real_grid_scenario_best_size_fits_every_window() {
        let monitor = primary_monitor_index();
        let gap = 10;
        let style = GridWindowStyle { auto_fit: true, ..GridWindowStyle::default() };
        let monitors = selected_monitors(&list_monitors(), &[monitor]);
        for count in [3usize, 7] {
            let clients = spawn_fake_clients(count);
            let result = arrange_roblox_grid(&[monitor], gap, None, &std::collections::HashSet::new(), style);
            std::thread::sleep(Duration::from_millis(500));
            let best = best_fit_cell(&monitors, gap, count, &[]).expect("fits");
            let sizes: Vec<(i32, i32)> = list_roblox_windows().iter().map(|w| (w.w, w.h)).collect();
            eprintln!("best size, {count} windows -> {result:?}, cell {best:?}, sizes {sizes:?}");
            assert!(sizes.iter().all(|&s| s == best), "{count}: not every window got the best cell");
            assert!(overlapping_pairs().is_empty(), "{count}: windows on top of one another");
            assert_eq!(windows_outside(monitor), 0, "{count}: a window left the monitor");
            drop(clients);
            std::thread::sleep(Duration::from_millis(500));
        }
        // Uma por vez: cada janela nova rearruma no tamanho novo.
        let clients = spawn_fake_clients(5);
        let request = GridRequest {
            monitor_indices: vec![monitor],
            gap,
            excluded_pids: std::collections::HashSet::new(),
            style,
        };
        for child in &clients.children {
            let hwnd = find_main_window(child.id()).expect("window");
            let _ = place_in_grid(hwnd, child.id(), None, &request, true);
            std::thread::sleep(Duration::from_millis(300));
        }
        std::thread::sleep(Duration::from_millis(500));
        let best = best_fit_cell(&monitors, gap, 5, &[]).expect("fits");
        let sizes: Vec<(i32, i32)> = list_roblox_windows().iter().map(|w| (w.w, w.h)).collect();
        eprintln!("best size, 5 windows one by one -> cell {best:?}, sizes {sizes:?}");
        assert!(sizes.iter().all(|&s| s == best), "one by one: not every window got the best cell");
        assert!(overlapping_pairs().is_empty(), "one by one: windows on top of one another");
        assert_eq!(windows_outside(monitor), 0, "one by one: a window left the monitor");
    }

    /// Gerador com semente (xorshift): a rodada que falhar se repete com
    /// `RAM_GRID_FUZZ_SEED`.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n.max(1) as u64) as usize
        }
        fn chance(&mut self, percent: u64) -> bool {
            self.next() % 100 < percent
        }
    }

    fn env_number(name: &str, default: u64) -> u64 {
        std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
    }

    /// O monitor da rodada: o mais à direita (fora do principal, para não
    /// atrapalhar quem está usando o PC), ou `RAM_GRID_FUZZ_MONITOR`.
    fn fuzz_monitor() -> MonitorInfoDto {
        let all = list_monitors();
        let wanted = env_number("RAM_GRID_FUZZ_MONITOR", 0) as usize;
        all.iter()
            .find(|m| m.index == wanted)
            .or_else(|| all.iter().max_by_key(|m| m.x))
            .cloned()
            .expect("monitor")
    }

    /// Pedido do dono (25/10/2026): martelar a grade do jeito mais aleatório
    /// possível — minimizar, maximizar, tela cheia, arrastar, abrir e fechar
    /// contas — e conferir, a cada "Organizar em grade", que nada fica uma por
    /// cima da outra nem sai do monitor (inclusive depois de restaurar as
    /// minimizadas e de abrir conta nova).
    ///
    ///   cargo test --all-features real_grid_fuzz -- --ignored --test-threads=1 --nocapture
    ///
    /// `RAM_GRID_FUZZ_SEED`, `RAM_GRID_FUZZ_ROUNDS` (padrão 4), `RAM_GRID_FUZZ_STEPS` (padrão 40).
    #[test]
    #[ignore]
    fn real_grid_fuzz_random_user_actions() {
        let seed = env_number(
            "RAM_GRID_FUZZ_SEED",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(1)
                | 1,
        );
        let rounds = env_number("RAM_GRID_FUZZ_ROUNDS", 4);
        let steps = env_number("RAM_GRID_FUZZ_STEPS", 40);
        let monitor = fuzz_monitor();
        let gap = 8;
        let slots = grid_slots(std::slice::from_ref(&monitor), 816, 639, gap).len();
        let mut rng = Rng(seed);
        eprintln!("grid fuzz: seed={seed} rounds={rounds} steps={steps} monitor={} slots_at_min={slots}", monitor.index);
        let request = GridRequest {
            monitor_indices: vec![monitor.index],
            gap,
            excluded_pids: std::collections::HashSet::new(),
            style: GridWindowStyle::default(),
        };
        let arrange = || {
            arrange_roblox_grid(
                &[monitor.index],
                gap,
                None,
                &std::collections::HashSet::new(),
                GridWindowStyle::default(),
            )
        };
        let pause = |ms: u64| std::thread::sleep(Duration::from_millis(ms));
        let random_rect = |rng: &mut Rng| -> (i32, i32, i32, i32) {
            let w = 816 + rng.below(500) as i32;
            let h = 639 + rng.below(300) as i32;
            let x = monitor.x + rng.below((monitor.width - w).max(1) as usize) as i32;
            let y = monitor.y + rng.below((monitor.height - h).max(1) as usize) as i32;
            (x, y, w, h)
        };
        let check = |seed: u64, label: &str, log: &[String]| {
            let overlaps = overlapping_pairs();
            let outside = windows_outside(monitor.index);
            if !overlaps.is_empty() || outside > 0 {
                let windows: Vec<String> = list_roblox_windows()
                    .into_iter()
                    .map(|w| {
                        let h = w.hwnd as HWND;
                        let style = unsafe { GetWindowLongW(h, GWL_STYLE) } as u32;
                        format!(
                            "    pid={} at ({},{}) {}x{} zoomed={} popup={}",
                            w.pid,
                            w.x,
                            w.y,
                            w.w,
                            w.h,
                            unsafe { IsZoomed(h) } != 0,
                            style & WS_POPUP != 0
                        )
                    })
                    .collect();
                panic!(
                    "seed={seed} {label}: {} overlapping pairs, {outside} outside the monitor\n{}\n{}\n{overlaps:?}",
                    overlaps.len(),
                    log.join("\n"),
                    windows.join("\n")
                );
            }
        };

        for round in 0..rounds {
            let count = 2 + rng.below(slots + 5);
            let mut clients = spawn_fake_clients(count);
            let _ = arrange();
            pause(300);
            // Tela cheia simulada (estilo do Roblox): janela -> (estilo, retângulo de antes).
            let mut fullscreen: HashMap<isize, (u32, (i32, i32, i32, i32))> = HashMap::new();
            let mut log: Vec<String> = vec![format!("round {round}: {count} windows")];
            let mut checkpoint_in = 3 + rng.below(6);

            for step in 0..steps {
                let wins = clients.windows();
                if wins.is_empty() {
                    break;
                }
                let hwnd = wins[rng.below(wins.len())];
                let iconic = unsafe { IsIconic(hwnd) } != 0;
                let zoomed = unsafe { IsZoomed(hwnd) } != 0;
                let what = match rng.below(10) {
                    0 => {
                        unsafe { ShowWindow(hwnd, SW_SHOWMINNOACTIVE) };
                        "minimize"
                    }
                    1 => {
                        unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
                        "restore"
                    }
                    2 if !iconic => {
                        unsafe { ShowWindow(hwnd, windows_sys::Win32::UI::WindowsAndMessaging::SW_MAXIMIZE) };
                        "maximize"
                    }
                    3 if !iconic && !zoomed && !fullscreen.contains_key(&(hwnd as isize)) => {
                        let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
                        let before = get_window_position(hwnd).unwrap_or((monitor.x, monitor.y, 816, 639));
                        fullscreen.insert(hwnd as isize, (style, before));
                        unsafe {
                            SetWindowLongW(hwnd, GWL_STYLE, ((style & !WS_OVERLAPPEDWINDOW) | WS_POPUP) as i32);
                            SetWindowPos(
                                hwnd,
                                std::ptr::null_mut(),
                                monitor.x,
                                monitor.y,
                                monitor.width,
                                monitor.height,
                                SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                            );
                        }
                        "enter fullscreen"
                    }
                    4 if fullscreen.contains_key(&(hwnd as isize)) => {
                        let (style, before) = fullscreen.remove(&(hwnd as isize)).unwrap();
                        unsafe {
                            SetWindowLongW(hwnd, GWL_STYLE, style as i32);
                            SetWindowPos(
                                hwnd,
                                std::ptr::null_mut(),
                                before.0,
                                before.1,
                                before.2,
                                before.3,
                                SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                            );
                        }
                        "leave fullscreen"
                    }
                    5 if !iconic && !zoomed => {
                        let r = random_rect(&mut rng);
                        unsafe {
                            SetWindowPos(hwnd, std::ptr::null_mut(), r.0, r.1, r.2, r.3, SWP_NOZORDER | SWP_NOACTIVATE)
                        };
                        "drag/resize"
                    }
                    6 => {
                        let new = clients.spawn_one();
                        let pid = clients.children.last().map(|c| c.id()).unwrap_or(0);
                        let _ = place_in_grid(new, pid, None, &request, true);
                        "new account (launch placement)"
                    }
                    7 if wins.len() > 2 => {
                        let i = rng.below(clients.children.len());
                        clients.close(i);
                        "close a client"
                    }
                    8 => {
                        pause(200);
                        snap_restored_small_windows();
                        "one second passes (tick)"
                    }
                    _ => "nothing",
                };
                log.push(format!("  step {step}: {what}"));
                pause(120);
                // A olhada de 250 ms do app roda entre duas ações de gente.
                snap_restored_small_windows();

                checkpoint_in = checkpoint_in.saturating_sub(1);
                if checkpoint_in > 0 {
                    continue;
                }
                checkpoint_in = 3 + rng.below(6);

                // ── ponto de controle ─────────────────────────────────────
                let result = arrange();
                log.push(format!("  ARRANGE -> {result:?}"));
                pause(300);
                if std::env::var("RAM_GRID_FUZZ_TRACE").is_ok() {
                    for w in list_roblox_windows() {
                        log.push(format!("    after arrange: pid={} at ({},{}) {}x{}", w.pid, w.x, w.y, w.w, w.h));
                    }
                    for w in list_minimized_roblox_windows() {
                        log.push(format!("    after arrange (minimized): pid={} normal ({},{}) {}x{}", w.pid, w.x, w.y, w.w, w.h));
                    }
                }
                // Depois de organizar, a pessoa minimiza e restaura algumas
                // vezes (com a olhada de 1 s rodando no meio, ou não).
                for _ in 0..rng.below(5) {
                    let wins = clients.windows();
                    let h = wins[rng.below(wins.len())];
                    if unsafe { IsIconic(h) } != 0 {
                        unsafe { ShowWindow(h, SW_SHOWNOACTIVATE) };
                        log.push("    then restore one".into());
                    } else {
                        unsafe { ShowWindow(h, SW_SHOWMINNOACTIVE) };
                        log.push("    then minimize one".into());
                    }
                    pause(150);
                    snap_restored_small_windows();
                }
                // E no fim traz todas de volta.
                for h in clients.windows() {
                    if unsafe { IsIconic(h) } != 0 {
                        unsafe { ShowWindow(h, SW_SHOWNOACTIVATE) };
                    }
                }
                pause(300);
                snap_restored_small_windows();
                pause(400);
                check(seed, "after arrange + restore", &log);
                if rng.chance(40) {
                    let new = clients.spawn_one();
                    let pid = clients.children.last().map(|c| c.id()).unwrap_or(0);
                    let _ = place_in_grid(new, pid, None, &request, true);
                    pause(400);
                    log.push("  then a new account opens".into());
                    check(seed, "after a new account opened", &log);
                }
                log.push("  checkpoint ok".into());
                fullscreen.clear();
            }
            eprintln!("round {round}: ok ({} windows at the end)", clients.children.len());
        }
    }

    #[test]
    #[ignore]
    fn real_grid_scenario_launch_placement_with_more_windows_than_cells() {
        let monitor = primary_monitor_index();
        let gap = 8;
        for extra in [2usize, 8] {
        let count = slots_at_min_size(monitor, gap) + extra;
        let clients = spawn_fake_clients(count);
        let request = GridRequest {
            monitor_indices: vec![monitor],
            gap,
            excluded_pids: std::collections::HashSet::new(),
            style: GridWindowStyle::default(),
        };
        // Uma por vez, como no launch: cada janela nova vai para a grade.
        for child in &clients.children {
            let hwnd = find_main_window(child.id()).expect("window");
            let _ = place_in_grid(hwnd, child.id(), None, &request, true);
            std::thread::sleep(Duration::from_millis(300));
        }
        std::thread::sleep(Duration::from_millis(600));
        let overlaps = overlapping_pairs();
        eprintln!("launch placement of {count} windows, overlapping pairs: {}", overlaps.len());
        assert!(overlaps.is_empty(), "windows on top of one another: {overlaps:?}");
        assert_eq!(windows_outside(monitor), 0, "a window left the monitor");
        }
    }
}
