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

struct WindowTitlesData {
    target_pid: u32,
    titles: Vec<String>,
}

unsafe extern "system" fn window_titles_callback(hwnd: HWND, lparam: isize) -> i32 {
    let data = &mut *(lparam as *mut WindowTitlesData);
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == data.target_pid && IsWindowVisible(hwnd) != 0 {
        data.titles.push(get_window_title(hwnd));
    }
    1
}

/// Os títulos das janelas visíveis do processo (a caixa de erro do Roblox é
/// outra janela, ao lado da principal).
pub fn window_titles_of(target_pid: u32) -> Vec<String> {
    let mut data = WindowTitlesData {
        target_pid,
        titles: Vec::new(),
    };
    unsafe {
        EnumWindows(Some(window_titles_callback), &mut data as *mut _ as isize);
    }
    data.titles
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

/// A borda invisível de redimensionar que o Windows 10/11 põe em volta de uma
/// janela com moldura: o retângulo da janela passa da parte visível por estes
/// pixels de cada lado (em 100% de escala, 7 dos lados e embaixo, 0 em cima).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct FrameMargins {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

/// O espaço entre as janelas da grade, medido entre as partes **visíveis**.
///
/// Relato do Discord (31/10/2026): com a moldura do Roblox, gap 0 ainda
/// deixava ~14 px entre as colunas e ~7 px entre as linhas — a grade encostava
/// os retângulos das janelas, que incluem a borda invisível. Sem moldura a
/// borda some e o gap já batia. Agora as células descontam `frame`: os
/// retângulos se cruzam só na parte invisível, e o que aparece encosta.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct GridGap {
    gap: i32,
    frame: FrameMargins,
}

impl From<i32> for GridGap {
    fn from(gap: i32) -> Self {
        GridGap { gap, frame: FrameMargins::default() }
    }
}

/// Mais que isto não é borda invisível (é medida errada): fica sem desconto.
const MAX_INVISIBLE_FRAME: i32 = 32;

/// A borda invisível de uma janela visível com moldura, ou `None` se não dá
/// para medir.
fn invisible_frame(hwnd: HWND) -> Option<FrameMargins> {
    let (x, y, w, h) = get_window_position(hwnd)?;
    let visible = visible_bounds(hwnd)?;
    let frame = FrameMargins {
        left: visible.left - x,
        top: visible.top - y,
        right: x + w - visible.right,
        bottom: y + h - visible.bottom,
    };
    let plausible = |v: i32| (0..=MAX_INVISIBLE_FRAME).contains(&v);
    (plausible(frame.left) && plausible(frame.top) && plausible(frame.right) && plausible(frame.bottom))
        .then_some(frame)
}

/// A borda das janelas da grade: medida em `probe` (uma janela com moldura
/// normal — maximizada ou em tela cheia diz outra coisa), nenhuma sem moldura.
fn grid_frame(style: GridWindowStyle, probe: Option<HWND>) -> FrameMargins {
    if style.borderless {
        return FrameMargins::default();
    }
    probe.and_then(invisible_frame).unwrap_or_default()
}

/// A primeira janela com moldura normal (nem minimizada, nem maximizada, nem
/// em tela cheia): é nela que a borda invisível da grade é medida.
fn framed_window(candidates: impl IntoIterator<Item = (HWND, (i32, i32, i32, i32))>) -> Option<HWND> {
    candidates
        .into_iter()
        .find(|&(hwnd, rect)| unsafe { IsIconic(hwnd) } == 0 && window_enforces_min_size(hwnd, rect))
        .map(|(hwnd, _)| hwnd)
}

/// O retângulo de uma janela na célula: a parte visível da célula (sem a
/// borda `grid`) mais a borda que a própria janela tem. Uma janela sem moldura
/// (a tela cheia do Roblox) no meio das com moldura fica só na parte visível —
/// do tamanho da célula, cobriria 7 px da vizinha.
fn rect_in_cell(cell: (i32, i32, i32, i32), grid: FrameMargins, own: FrameMargins) -> (i32, i32, i32, i32) {
    (
        cell.0 + grid.left - own.left,
        cell.1 + grid.top - own.top,
        cell.2 - grid.left - grid.right + own.left + own.right,
        cell.3 - grid.top - grid.bottom + own.top + own.bottom,
    )
}

/// A borda da própria janela. Sem moldura (pelo estilo, que vale até
/// minimizada): nenhuma. Minimizada, maximizada ou sem medida: a da grade.
fn own_frame(hwnd: HWND, grid: FrameMargins) -> FrameMargins {
    if grid == FrameMargins::default() {
        return grid;
    }
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    if !frame_enforces_min_size(style) {
        return FrameMargins::default();
    }
    if unsafe { IsIconic(hwnd) } != 0 || unsafe { IsZoomed(hwnd) } != 0 {
        return grid;
    }
    invisible_frame(hwnd).unwrap_or(grid)
}

/// A janela vista pela célula em que está (o inverso de `rect_in_cell`): a sem
/// moldura conta com o tamanho da célula inteira. Sem isso, três janelas sem
/// moldura viravam o "tamanho mais comum" da grade e a próxima célula saía
/// menor, por cima da vizinha (fuzz de 31/10/2026).
fn cell_of_window(w: RobloxWin, grid: FrameMargins) -> RobloxWin {
    let own = own_frame(w.hwnd as HWND, grid);
    RobloxWin {
        x: w.x - grid.left + own.left,
        y: w.y - grid.top + own.top,
        w: w.w + grid.left + grid.right - own.left - own.right,
        h: w.h + grid.top + grid.bottom - own.top - own.bottom,
        ..w
    }
}

/// Os retângulos das janelas de fora, vistos pelas partes visíveis contra a
/// célula: encostada (gap 0), a vizinha não bloqueia a célula só porque as
/// bordas invisíveis se cruzam.
fn visible_blockers(blockers: &[(i32, i32, i32, i32)], frame: FrameMargins) -> Vec<(i32, i32, i32, i32)> {
    let (dx, dy) = (frame.left + frame.right, frame.top + frame.bottom);
    blockers
        .iter()
        .map(|&(x, y, w, h)| (x + dx, y + dy, (w - 2 * dx).max(0), (h - 2 * dy).max(0)))
        .collect()
}

/// Distribute `count` windows of `win_w`×`win_h` into cells across `monitors`.
fn compute_grid(
    monitors: &[MonitorInfoDto],
    win_w: i32,
    win_h: i32,
    count: usize,
    gap: impl Into<GridGap>,
) -> Vec<GridCell> {
    let GridGap { gap, frame: f } = gap.into();
    let gap = gap.max(0);
    // De uma célula à próxima: a janela, o gap, menos as duas bordas
    // invisíveis que se cruzam.
    let (step_x, step_y) = (win_w + gap - f.left - f.right, win_h + gap - f.top - f.bottom);
    if step_x <= 0 || step_y <= 0 {
        return Vec::new();
    }
    let mut cells: Vec<GridCell> = Vec::new();
    for mon in monitors {
        let mut row = 0;
        while cells.len() < count {
            let cell_y = mon.y + gap - f.top + row * step_y;
            if cell_y + win_h - f.bottom > mon.y + mon.height {
                break; // row doesn't fit vertically -> monitor exhausted
            }
            let mut col = 0;
            let mut placed_in_row = false;
            while cells.len() < count {
                let cell_x = mon.x + gap - f.left + col * step_x;
                if cell_x + win_w - f.right > mon.x + mon.width {
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
fn grid_slots(monitors: &[MonitorInfoDto], cell_w: i32, cell_h: i32, gap: impl Into<GridGap>) -> Vec<GridCell> {
    if cell_w <= 0 || cell_h <= 0 {
        return Vec::new();
    }
    compute_grid(monitors, cell_w, cell_h, MAX_GRID_SLOTS, gap)
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
    gap: impl Into<GridGap>,
    count: usize,
    blockers: &[(i32, i32, i32, i32)],
) -> Option<(i32, i32)> {
    let gap: GridGap = gap.into();
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
    gap: impl Into<GridGap>,
    count: usize,
    blockers: &[(i32, i32, i32, i32)],
) -> Option<(i32, i32)> {
    if count == 0 || monitors.is_empty() {
        return None;
    }
    let gap: GridGap = gap.into();
    let (g, f) = (gap.gap.max(0), gap.frame);
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
                // `n` colunas: gap + n*w + (n-1)*gap <= largura, com `w` a
                // parte visível (a janela mais a borda invisível).
                widths.push((m.width - n * g) / n + f.left + f.right);
                // `n` linhas, convertidas em largura pelo formato.
                let h = (m.height - n * g) / n + f.top + f.bottom;
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
    // O gap conta da parte visível: a borda invisível da moldura sai da conta,
    // medida nesta janela ou numa das outras (esta pode estar sem moldura).
    let probe = framed_window(
        std::iter::once((hwnd, current)).chain(occupant_wins.iter().map(|w| (w.hwnd as HWND, (w.x, w.y, w.w, w.h)))),
    );
    let gap = GridGap { gap: request.gap, frame: grid_frame(request.style, probe) };
    let own = own_frame(hwnd, gap.frame);
    let occupant_wins: Vec<RobloxWin> = occupant_wins.into_iter().map(|w| cell_of_window(w, gap.frame)).collect();
    if request.style.auto_fit {
        let monitors = selected_monitors(&list_monitors(), &request.monitor_indices);
        let rect = |w: &RobloxWin| (w.x, w.y, w.w, w.h);
        let occupants: Vec<(i32, i32, i32, i32)> = occupant_wins.iter().map(rect).collect();
        let blockers = visible_blockers(&blocker_wins.iter().map(rect).collect::<Vec<_>>(), gap.frame);
        if let Some(best) = best_fit_cell(&monitors, gap, occupants.len() + 1, &blockers) {
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
            let slots = grid_slots(&monitors, best.0, best.1, gap);
            let target = match pick_grid_slot(&slots, best, &occupants, &blockers) {
                Some(i) => rect_in_cell((slots[i].x, slots[i].y, best.0, best.1), gap.frame, own),
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
    let slots = grid_slots(&monitors, cell_w, cell_h, gap);
    let rect = |w: &RobloxWin| (w.x, w.y, w.w, w.h);
    let occupants: Vec<(i32, i32, i32, i32)> = occupant_wins.iter().map(rect).collect();
    let blockers = visible_blockers(&blocker_wins.iter().map(rect).collect::<Vec<_>>(), gap.frame);
    // Grade lotada: em vez de pôr esta por cima de outra, rearruma todas num
    // tamanho em que caibam.
    if grid_is_full(&slots, (cell_w, cell_h), &occupants, &blockers)
        && cell_that_fits(&monitors, (cell_w, cell_h), gap, occupants.len() + 1, &blockers).is_some()
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
        Some(i) => rect_in_cell((slots[i].x, slots[i].y, cell_w, cell_h), gap.frame, own),
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
    // O gap conta da parte visível (ver `GridGap`). A borda é medida só numa
    // janela com moldura normal: maximizada, a conta sai errada.
    let framed = framed_window(visible.iter().map(|w| (w.hwnd as HWND, (w.x, w.y, w.w, w.h))));
    let gap = GridGap { gap, frame: grid_frame(style, framed) };
    let wins: Vec<RobloxWin> = wins.into_iter().map(|w| cell_of_window(w, gap.frame)).collect();
    let blockers = visible_blockers(&blockers, gap.frame);
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
            let rect = rect_in_cell((cell.x, cell.y, cell_w, cell_h), gap.frame, own_frame(hwnd, gap.frame));
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
            let rect = rect_in_cell((cell.x, cell.y, cell_w, cell_h), gap.frame, own_frame(hwnd, gap.frame));
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
        let rect = rect_in_cell((cell.x, cell.y, cell_w, cell_h), gap.frame, own_frame(hwnd, gap.frame));
        if place_window_with(hwnd, rect, grid_swp_flags(small)) {
            arranged += 1;
        }
        // A pessoa pode minimizá-la depois: ao voltar, ela volta para cá.
        remember_grid_cell(w.hwnd, (small || own.contains(&w.pid)).then_some(rect), false);
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

/// Memória do Windows em MB: `((física livre, total), (confirmada livre, limite))`.
/// A confirmada é RAM + arquivo de paginação; é ela que, ao acabar, faz os
/// programas quebrarem. Só leitura.
pub fn system_memory_detail() -> Option<((u64, u64), (u64, u64))> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    const MB: u64 = 1024 * 1024;
    // SAFETY: MEMORYSTATUSEX é uma struct C simples; zerada e com o
    // `dwLength` certo é o que a função pede.
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
        return None;
    }
    Some((
        (status.ullAvailPhys / MB, status.ullTotalPhys / MB),
        (status.ullAvailPageFile / MB, status.ullTotalPageFile / MB),
    ))
}
