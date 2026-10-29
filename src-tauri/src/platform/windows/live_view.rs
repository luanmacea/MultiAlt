// Live (pedido do dono, 28/10/2026): as janelas do Roblox das contas, ao vivo,
// numa janela só do MultiAlt — quem transmite captura só ela no OBS, e nada
// mais da tela aparece. Ver docs/features/live.md.
//
// Quem desenha é o DWM (miniatura ao vivo, a mesma do Alt+Tab): o app não copia
// imagem nenhuma, então pesa quase nada mesmo com muitas contas. Medido num
// cliente real em 28/10/2026: a miniatura segue ao vivo com a janela atrás de
// outras, fora da tela e transparente (alfa 0 ou 1); **minimizada ela congela**
// no último quadro. Por isso a conta minimizada é restaurada invisível (alfa 0,
// não recebe clique) enquanto a Live estiver aberta, e volta a ser minimizada
// quando ela fecha (`live_release_hidden`).

/// Onde a miniatura fica dentro da célula: o maior retângulo com a proporção da
/// janela de origem, centrado. `(x, y, largura, altura)` em pixels.
pub fn live_fit_rect(cell: (i32, i32, i32, i32), source: (i32, i32)) -> (i32, i32, i32, i32) {
    let (x, y, w, h) = cell;
    let (sw, sh) = source;
    if w <= 0 || h <= 0 || sw <= 0 || sh <= 0 {
        return (x, y, w.max(0), h.max(0));
    }
    // Compara as proporções sem dividir: w/h contra sw/sh.
    let (fw, fh) = if (w as i64) * (sh as i64) > (h as i64) * (sw as i64) {
        // A célula é mais larga que a janela: a altura manda.
        (((h as i64) * (sw as i64) / (sh as i64)) as i32, h)
    } else {
        (w, ((w as i64) * (sh as i64) / (sw as i64)) as i32)
    };
    (x + (w - fw) / 2, y + (h - fh) / 2, fw, fh)
}

/// Uma miniatura registrada: a janela de origem e o id do DWM.
struct LiveThumb {
    hwnd: isize,
    thumb: isize,
}

static LIVE_THUMBS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<i64, LiveThumb>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// As contas que a Live tirou do minimizado (invisíveis): voltam a minimizar
/// quando ela fecha.
static LIVE_HIDDEN: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<i64, HiddenWindow>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

fn live_lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn live_unregister(thumb: isize) {
    unsafe {
        let _ = windows_sys::Win32::Graphics::Dwm::DwmUnregisterThumbnail(thumb);
    }
}

/// Mostra as miniaturas na janela `host`, uma por `(conta, janela do Roblox,
/// célula)`. A que saiu da lista (ou trocou de janela) é desfeita. Devolve
/// quantas estão de pé.
pub fn live_show_tiles(host: isize, tiles: &[(i64, isize, (i32, i32, i32, i32))]) -> usize {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmQueryThumbnailSourceSize, DwmRegisterThumbnail, DwmUpdateThumbnailProperties, DWM_THUMBNAIL_PROPERTIES,
        DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION, DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE,
    };
    let mut thumbs = live_lock(&LIVE_THUMBS);
    thumbs.retain(|user_id, t| {
        let keep = tiles.iter().any(|(u, hwnd, _)| u == user_id && *hwnd == t.hwnd);
        if !keep {
            live_unregister(t.thumb);
        }
        keep
    });
    for (user_id, hwnd, cell) in tiles {
        if !thumbs.contains_key(user_id) {
            let mut thumb: isize = 0;
            let hr = unsafe { DwmRegisterThumbnail(host as HWND, *hwnd as HWND, &mut thumb) };
            if hr < 0 || thumb == 0 {
                continue;
            }
            thumbs.insert(*user_id, LiveThumb { hwnd: *hwnd, thumb });
        }
        let Some(t) = thumbs.get(user_id) else { continue };
        let mut size = windows_sys::Win32::Foundation::SIZE { cx: 0, cy: 0 };
        unsafe {
            let _ = DwmQueryThumbnailSourceSize(t.thumb, &mut size);
        }
        // A origem com a moldura: a área interna é a que importa, mas o DWM
        // só dá o tamanho da janela inteira. A diferença é pequena (a moldura
        // do Roblox é fina) e a imagem continua inteira, com faixa nas bordas.
        let (x, y, w, h) = live_fit_rect(*cell, (size.cx.max(1), size.cy.max(1)));
        let props = DWM_THUMBNAIL_PROPERTIES {
            dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY | DWM_TNP_SOURCECLIENTAREAONLY,
            rcDestination: windows_sys::Win32::Foundation::RECT { left: x, top: y, right: x + w, bottom: y + h },
            rcSource: windows_sys::Win32::Foundation::RECT { left: 0, top: 0, right: 0, bottom: 0 },
            opacity: 255,
            fVisible: 1,
            fSourceClientAreaOnly: 1,
        };
        unsafe {
            let _ = DwmUpdateThumbnailProperties(t.thumb, &props);
        }
    }
    thumbs.len()
}

/// Desfaz todas as miniaturas (a Live fechou).
pub fn live_clear_tiles() {
    let mut thumbs = live_lock(&LIVE_THUMBS);
    for (_, t) in thumbs.drain() {
        live_unregister(t.thumb);
    }
}

/// Janela minimizada não se desenha: a Live a restaura **invisível** (alfa 0,
/// não recebe clique), atrás de tudo e sem pegar o foco. Só mexe em janela
/// minimizada; as outras ficam como estão.
pub fn live_keep_rendering(user_id: i64, hwnd: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsIconic, SetWindowPos, ShowWindow, HWND_BOTTOM, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_SHOWNOACTIVATE,
    };
    let h = hwnd as HWND;
    if unsafe { IsIconic(h) } == 0 {
        return;
    }
    let mut hidden = live_lock(&LIVE_HIDDEN);
    if hidden.contains_key(&user_id) {
        return;
    }
    let Some(saved) = make_window_invisible(h, 0) else { return };
    unsafe {
        ShowWindow(h, SW_SHOWNOACTIVATE);
        SetWindowPos(h, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
    hidden.insert(user_id, saved);
}

/// A Live fechou: as janelas que ela tirou do minimizado voltam a ser
/// minimizadas (ainda invisíveis) e só então voltam a aparecer.
pub fn live_release_hidden() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWMINNOACTIVE};
    let mut hidden = live_lock(&LIVE_HIDDEN);
    for (_, saved) in hidden.drain() {
        unsafe {
            ShowWindow(saved.hwnd as HWND, SW_SHOWMINNOACTIVE);
        }
        restore_window_visibility(saved);
    }
}

/// A pessoa clicou na conta para jogar: se a Live a deixou invisível, ela volta
/// a aparecer (e não é minimizada de novo quando a Live fechar).
pub fn live_unhide(user_id: i64) {
    if let Some(saved) = live_lock(&LIVE_HIDDEN).remove(&user_id) {
        restore_window_visibility(saved);
    }
}

#[cfg(test)]
mod live_view_tests {
    use super::*;

    #[test]
    fn a_thumbnail_keeps_the_window_shape_and_is_centered_in_its_cell() {
        // Janela 4:3 numa célula larga: a altura manda, faixas dos lados.
        assert_eq!(live_fit_rect((0, 0, 1000, 300), (800, 600)), (300, 0, 400, 300));
        // Janela larga numa célula alta: a largura manda, faixas em cima e embaixo.
        assert_eq!(live_fit_rect((10, 20, 400, 600), (1600, 900)), (10, 207, 400, 225));
        // Mesma proporção: ocupa a célula inteira.
        assert_eq!(live_fit_rect((5, 5, 800, 600), (400, 300)), (5, 5, 800, 600));
        // Tamanho que o DWM ainda não deu: a célula inteira, sem dividir por zero.
        assert_eq!(live_fit_rect((0, 0, 200, 100), (0, 0)), (0, 0, 200, 100));
    }
}
