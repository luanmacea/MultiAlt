// Foco sem aparecer (AFK mode, 24/10/2026).
//
// O Roblox só lê teclado da janela em primeiro plano, então o ciclo do AFK tem
// de dar o foco a cada cliente. Pedido de usuários (ver um filme enquanto o AFK
// roda): que as janelas não pulem na tela. Três peças, a mesma técnica do Nudge
// Nest (MIT, só a ideia):
//
// 1. A janela que **não estava à vista** (minimizada, ou inteira coberta) fica
//    100% transparente enquanto tem o foco — `make_window_invisible`.
//    Janela que aparece na tela não fica transparente: ela sumiria e voltaria
//    (o piscar que o Nudge Nest anota).
// 2. No mesmo instante em que ganha o foco, ela volta para a profundidade de
//    antes (`tuck_window_after_focus`): o foco **fica** com ela mesmo atrás das
//    outras, e a tecla chega.
// 3. Sem animação de restaurar/minimizar nesse meio-tempo (DWM).
//
// No fim do ciclo, o caminho de sempre (`minimize_window_quietly`,
// `put_window_below`) devolve tudo, e só então a transparência sai.

use windows_sys::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DwmSetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
    DWMWA_TRANSITIONS_FORCEDISABLED,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetLayeredWindowAttributes, SetLayeredWindowAttributes, HWND_BOTTOM, LWA_ALPHA,
    SWP_ASYNCWINDOWPOS, WS_EX_LAYERED, WS_EX_TRANSPARENT,
};

/// Retângulo de tela, em pixels (`right`/`bottom` exclusivos).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl ScreenRect {
    fn is_empty(&self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }

    fn intersect(&self, other: &ScreenRect) -> ScreenRect {
        ScreenRect {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        }
    }
}

/// O que sobra de `a` tirando `b`: até quatro pedaços.
fn subtract_rect(a: ScreenRect, b: ScreenRect) -> Vec<ScreenRect> {
    let cut = a.intersect(&b);
    if cut.is_empty() {
        return vec![a];
    }
    let pieces = [
        ScreenRect { left: a.left, top: a.top, right: a.right, bottom: cut.top },
        ScreenRect { left: a.left, top: cut.bottom, right: a.right, bottom: a.bottom },
        ScreenRect { left: a.left, top: cut.top, right: cut.left, bottom: cut.bottom },
        ScreenRect { left: cut.right, top: cut.top, right: a.right, bottom: cut.bottom },
    ];
    pieces.into_iter().filter(|p| !p.is_empty()).collect()
}

/// Sobra alguma parte de `target` na tela (`screen`) depois das janelas que
/// estão por cima (`covers`)?
fn rect_partly_visible(target: ScreenRect, screen: ScreenRect, covers: &[ScreenRect]) -> bool {
    let mut left = vec![target.intersect(&screen)];
    left.retain(|r| !r.is_empty());
    for cover in covers {
        if left.is_empty() {
            break;
        }
        left = left.into_iter().flat_map(|piece| subtract_rect(piece, *cover)).collect();
    }
    !left.is_empty()
}

/// Esconder esta janela enquanto ela tem o foco? Só com a opção ligada e se ela
/// **não** estava à vista: minimizada, ou sem nenhuma parte aparecendo.
pub fn should_hide_during_focus(enabled: bool, was_minimized: bool, partly_visible: bool) -> bool {
    enabled && (was_minimized || !partly_visible)
}

fn dwm_cloaked(hwnd: HWND) -> bool {
    let mut cloaked: u32 = 0;
    let ok = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED as u32,
            &mut cloaked as *mut u32 as *mut std::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        )
    };
    ok == 0 && cloaked != 0
}

/// O retângulo que aparece na tela, sem a borda invisível de redimensionar.
fn visible_bounds(hwnd: HWND) -> Option<ScreenRect> {
    let mut rect: RECT = unsafe { std::mem::zeroed() };
    let dwm = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            &mut rect as *mut RECT as *mut std::ffi::c_void,
            std::mem::size_of::<RECT>() as u32,
        )
    };
    if dwm != 0 && unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return None;
    }
    let r = ScreenRect { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom };
    (!r.is_empty()).then_some(r)
}

/// Alguma parte de `hwnd` aparece na tela? Minimizada, não. Senão, tira do
/// retângulo dela o de cada janela visível por cima (fora as transparentes, as
/// minimizadas e as "fantasma" do DWM — apps suspensos, outras áreas de
/// trabalho virtuais).
pub fn window_partly_visible(hwnd: HWND) -> bool {
    if window_is_minimized(hwnd) {
        return false;
    }
    let Some(target) = visible_bounds(hwnd) else {
        return false;
    };
    let Some(desktop) = virtual_desktop() else {
        return true;
    };
    let screen = ScreenRect {
        left: desktop.left,
        top: desktop.top,
        right: desktop.left + desktop.width,
        bottom: desktop.top + desktop.height,
    };
    let covers: Vec<ScreenRect> = std::iter::successors(Some(hwnd), |current| {
        let previous = unsafe { GetWindow(*current, GW_HWNDPREV) };
        (!previous.is_null()).then_some(previous)
    })
    .skip(1)
    .take(WINDOW_ABOVE_MAX_STEPS)
    .filter(|&w| unsafe { IsWindowVisible(w) } != 0 && !window_is_minimized(w) && !dwm_cloaked(w))
    .filter(|&w| {
        let ex = unsafe { GetWindowLongW(w, GWL_EXSTYLE) } as u32;
        ex & (WS_EX_LAYERED | WS_EX_TRANSPARENT) == 0
    })
    .filter_map(visible_bounds)
    .collect();
    rect_partly_visible(target, screen, &covers)
}

/// Como devolver a janela ao normal depois de `make_window_invisible`.
#[derive(Debug, Clone, Copy)]
pub struct HiddenWindow {
    hwnd: isize,
    was_layered: bool,
    key: u32,
    alpha: u8,
    flags: u32,
}

fn set_transitions(hwnd: HWND, enabled: bool) {
    let disabled: i32 = if enabled { 0 } else { 1 };
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_TRANSITIONS_FORCEDISABLED as u32,
            &disabled as *const i32 as *const std::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        );
    }
}

/// Deixa a janela transparente (`alpha` de 0 a 255; 0 = some, mas não recebe
/// clique — ver `afk_hidden_alpha`) e sem animação até `restore_window_visibility`.
pub fn make_window_invisible(hwnd: HWND, alpha_while_hidden: u8) -> Option<HiddenWindow> {
    if !window_exists(hwnd) {
        return None;
    }
    let ex = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
    let was_layered = ex & WS_EX_LAYERED != 0;
    let (mut key, mut alpha, mut flags) = (0u32, 255u8, 0u32);
    unsafe {
        if was_layered {
            let _ = GetLayeredWindowAttributes(hwnd, &mut key, &mut alpha, &mut flags);
        } else {
            SetWindowLongW(hwnd, GWL_EXSTYLE, (ex | WS_EX_LAYERED) as i32);
        }
        if SetLayeredWindowAttributes(hwnd, 0, alpha_while_hidden, LWA_ALPHA) == 0 {
            if !was_layered {
                SetWindowLongW(hwnd, GWL_EXSTYLE, ex as i32);
            }
            return None;
        }
    }
    set_transitions(hwnd, false);
    Some(HiddenWindow { hwnd: hwnd as isize, was_layered, key, alpha, flags })
}

/// Desfaz `make_window_invisible`: a janela volta a aparecer como era.
pub fn restore_window_visibility(saved: HiddenWindow) {
    let hwnd = saved.hwnd as HWND;
    if !window_exists(hwnd) {
        return;
    }
    unsafe {
        if saved.was_layered {
            let _ = SetLayeredWindowAttributes(hwnd, saved.key, saved.alpha, saved.flags);
        } else {
            let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            SetWindowLongW(hwnd, GWL_EXSTYLE, (ex & !WS_EX_LAYERED) as i32);
        }
    }
    set_transitions(hwnd, true);
}

/// Logo depois de a janela ganhar o foco: volta para a profundidade de antes —
/// embaixo de tudo se estava minimizada, ou sob a janela que estava acima dela.
/// O foco continua com ela. Não ativa, move nem redimensiona nada.
pub fn tuck_window_after_focus(hwnd: HWND, was_minimized: bool, above: HWND) {
    const FLAGS: u32 = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS;
    let insert_after = if was_minimized {
        HWND_BOTTOM
    } else if !above.is_null() && window_exists(above) {
        above
    } else {
        return;
    };
    unsafe {
        let _ = SetWindowPos(hwnd, insert_after, 0, 0, 0, 0, FLAGS);
    }
}

#[cfg(test)]
mod hidden_focus_tests {
    use super::*;

    fn r(left: i32, top: i32, right: i32, bottom: i32) -> ScreenRect {
        ScreenRect { left, top, right, bottom }
    }

    const SCREEN: ScreenRect = ScreenRect { left: 0, top: 0, right: 1920, bottom: 1080 };

    #[test]
    fn a_window_under_a_fullscreen_movie_is_not_visible() {
        assert!(!rect_partly_visible(r(100, 100, 900, 700), SCREEN, &[SCREEN]));
    }

    #[test]
    fn a_window_with_a_corner_showing_is_visible() {
        let cover = r(0, 0, 800, 1080);
        assert!(rect_partly_visible(r(700, 100, 1000, 400), SCREEN, &[cover]));
    }

    #[test]
    fn several_windows_together_can_cover_it() {
        let left_half = r(0, 0, 960, 1080);
        let right_half = r(960, 0, 1920, 1080);
        assert!(!rect_partly_visible(r(500, 100, 1500, 700), SCREEN, &[left_half, right_half]));
        assert!(rect_partly_visible(r(500, 100, 1500, 700), SCREEN, &[left_half]));
    }

    #[test]
    fn a_window_off_every_screen_is_not_visible() {
        assert!(!rect_partly_visible(r(-5000, 0, -4000, 600), SCREEN, &[]));
    }

    #[test]
    fn subtracting_a_rect_leaves_the_frame_around_it() {
        let pieces = subtract_rect(r(0, 0, 100, 100), r(25, 25, 75, 75));
        let area: i32 = pieces.iter().map(|p| (p.right - p.left) * (p.bottom - p.top)).sum();
        assert_eq!(area, 100 * 100 - 50 * 50);
        assert_eq!(subtract_rect(r(0, 0, 10, 10), r(20, 20, 30, 30)), vec![r(0, 0, 10, 10)]);
        assert!(subtract_rect(r(0, 0, 10, 10), r(-5, -5, 50, 50)).is_empty());
    }

    /// A janela que aparece na tela nunca fica transparente (sumiria e
    /// voltaria); a que não aparece, sim — com a opção ligada.
    #[test]
    fn only_a_window_that_was_not_on_screen_is_hidden() {
        assert!(should_hide_during_focus(true, true, false));
        assert!(should_hide_during_focus(true, false, false));
        assert!(!should_hide_during_focus(true, false, true));
        assert!(!should_hide_during_focus(false, true, false));
    }
}
