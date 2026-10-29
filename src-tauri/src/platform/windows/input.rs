// Entrada sintética para a janela de um cliente Roblox (AFK mode).
//
// Por que `SendInput` e não `PostMessage`/`SendMessage`: o cliente do Roblox lê
// teclado pelo caminho de entrada do sistema, e mensagem postada na fila da
// janela não move o personagem. O preço do `SendInput` é o foco — ele entrega a
// tecla na janela que está em **primeiro plano**, então quem envia precisa
// trazer a janela do Roblox para frente, **confirmar que ela chegou lá** e
// devolver o foco depois (o ciclo mora em `commands/afk.rs`).
//
// Este módulo só **envia**, e por portas estreitas:
// - `tap_afk_key` recebe **nome** de tecla (ou par de setas) e o resolve pela
//   lista fechada (`send_key_flags` é privado): não existe chamador com virtual key cru;
// - `click_afk_point` recebe janela + **porcentagem** e clica com o botão
//   esquerdo dentro da área interna dela (`send_mouse` é privado): não existe
//   chamador com coordenada de tela crua. O cursor volta para onde estava;
// - as Gravações (docs/features/recordings.md) usam as mesmas duas formas:
//   `press_recording_key` recebe **nome** de tecla da lista fechada das
//   gravações (`RECORDING_KEYS`) e `click_recording_point` é o clique do AFK,
//   com o clique de foco opcional;
// - a automação Python usa `press_python_key`: a lista das gravações mais o
//   `Escape` (`PYTHON_ONLY_KEYS`), que as gravações e o AFK continuam recusando.
// Ler teclado ou botão do usuário é proibido aqui; a posição do cursor é lida só
// para devolvê-lo, para o Marcar e para a parada de emergência por chacoalhar
// (`commands/shake_stop.rs`), que também sabe, por `app_cursor_move`, quando
// quem move o cursor é o próprio app. A trava é o `afk_input_safety_tests` (em
// `commands/afk.rs`), que varre este arquivo e só aqui aceita injeção de mouse.

use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetCursorPos, GetSystemMetrics, IsWindow, SetCursorPos, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

/// Quantas vezes o "solta a tecla" é tentado. Tecla que fica logicamente
/// pressionada no cliente faz o personagem **andar** — exatamente o que o AFK
/// mode existe para evitar.
const KEY_UP_ATTEMPTS: u32 = 3;
/// Respiro entre duas tentativas de soltar a tecla.
const KEY_UP_RETRY_MS: u64 = 15;

/// `(virtual key, scan code, estendida)` de cada toque de uma tecla da lista
/// fechada do AFK mode — um para tecla simples, dois para um par de setas
/// (`AFK_KEY_PAIRS`) —, ou `None` para qualquer outro nome.
///
/// A lista fica no raiz do crate (`commands/afk.rs`) porque a tela também a
/// consulta; aqui só se acrescenta o scan code, que depende do layout de teclado
/// ativo. `MapVirtualKeyW` **traduz** um código de tecla pelo layout: não lê
/// tecla pressionada nem estado de teclado.
fn afk_taps_with_scan(key: &str) -> Option<Vec<(u16, u16, bool)>> {
    let taps = crate::afk_key_taps(key)?;
    Some(
        taps.into_iter()
            .map(|(vk, extended)| {
                let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16;
                (vk, scan, extended)
            })
            .collect(),
    )
}

/// A janela ainda existe? O cliente pode ter fechado entre um ciclo e o
/// seguinte, e aí não há para onde mandar tecla.
pub fn window_exists(hwnd: HWND) -> bool {
    if hwnd.is_null() {
        return false;
    }
    unsafe { IsWindow(hwnd) != 0 }
}

/// Um evento de tecla para a janela em primeiro plano: `up = false` pressiona,
/// `up = true` solta; `extended` marca tecla estendida (as setas). Devolve
/// `false` quando o Windows recusou o envio.
///
/// Privado: quem chama passa **nome** de tecla por `tap_afk_key`,
/// `press_recording_key` ou `press_python_key`, nunca um virtual key cru.
fn send_key_flags(vk: u16, scan: u16, up: bool, extended: bool) -> bool {
    let mut flags = if up { KEYEVENTF_KEYUP } else { 0 };
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) == 1 }
}

/// Respiro entre os dois toques de um par de setas: o jogo lê a primeira
/// tecla solta antes de a segunda descer.
const AFK_PAIR_GAP_MS: u64 = 40;

/// Um toque na tecla `key` (da lista fechada) na janela que está em primeiro
/// plano: pressiona, espera `hold_ms` e solta. Um par de setas (`Right+Left`,
/// `Up+Down`) é a primeira tocada assim e, depois de um respiro, a segunda, do
/// mesmo tamanho — para o personagem terminar onde começou.
///
/// Antes de **cada** toque depois do primeiro, `ok_to_continue` confere que
/// ainda dá para enviar (a janela do alvo continua na frente e ninguém mandou
/// parar): entre os dois toques há um respiro, e no meio dele o usuário pode
/// clicar em outra janela — a segunda tecla cairia fora. Se não der, o par para
/// ali, e nenhuma tecla fica apertada (cada toque solta a sua antes do respiro).
///
/// O "solta" é tentado até três vezes: se ele falhar, a tecla fica logicamente
/// pressionada dentro do cliente e o personagem sai andando. Se o Windows
/// recusa um toque, o par para ali: a segunda tecla sozinha deixaria a câmera
/// (ou o personagem) fora do lugar.
pub fn tap_afk_key(
    key: &str,
    hold_ms: u64,
    mut ok_to_continue: impl FnMut() -> bool,
) -> Result<(), String> {
    let taps =
        afk_taps_with_scan(key).ok_or_else(|| format!("Key not allowed in AFK mode: {}", key))?;

    for (index, (vk, scan, extended)) in taps.into_iter().enumerate() {
        if index > 0 {
            std::thread::sleep(std::time::Duration::from_millis(AFK_PAIR_GAP_MS));
            // A janela saiu da frente ou pararam durante o respiro: o segundo
            // toque cairia na janela do usuário. Para aqui — o primeiro já
            // reiniciou o relógio de inatividade, e nada ficou apertado.
            if !ok_to_continue() {
                return Ok(());
            }
        }
        let down = send_key_flags(vk, scan, false, extended);
        std::thread::sleep(std::time::Duration::from_millis(hold_ms));

        let mut up = false;
        for attempt in 0..KEY_UP_ATTEMPTS {
            if send_key_flags(vk, scan, true, extended) {
                up = true;
                break;
            }
            if attempt + 1 < KEY_UP_ATTEMPTS {
                std::thread::sleep(std::time::Duration::from_millis(KEY_UP_RETRY_MS));
            }
        }

        if !down {
            return Err("Windows refused the synthetic key".into());
        }
        if !up {
            return Err("Windows refused to release the key".into());
        }
    }
    Ok(())
}

/// `(virtual key, scan code, estendida)` de uma tecla da lista fechada das
/// Gravações, ou `None` para qualquer outro nome.
fn recording_vk_and_scan(key: &str) -> Option<(u16, u16, bool)> {
    let (vk, extended) = crate::data::recordings::recording_key(key)?;
    let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16;
    Some((vk, scan, extended))
}

/// Aperta (`up = false`) ou solta (`up = true`) uma tecla da lista das
/// Gravações na janela em primeiro plano. Tecla fora da lista: `false`, sem
/// enviar nada. O "solta" é tentado até três vezes, como no AFK: tecla presa
/// faz o personagem andar sozinho.
pub fn press_recording_key(key: &str, up: bool) -> bool {
    let Some((vk, scan, extended)) = recording_vk_and_scan(key) else {
        return false;
    };
    press_named_key(vk, scan, extended, up)
}

/// `(virtual key, scan code, estendida)` de uma tecla que a automação Python
/// pode mandar (a lista das gravações mais o `Escape`), ou `None`.
fn python_vk_and_scan(key: &str) -> Option<(u16, u16, bool)> {
    let (_, vk, extended) = crate::data::recordings::python_key(key)?;
    let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16;
    Some((vk, scan, extended))
}

/// A porta da automação Python: a das gravações mais o `Escape`
/// (`data::recordings::PYTHON_ONLY_KEYS`). Tecla fora da lista: `false`, sem
/// enviar nada.
#[cfg_attr(not(feature = "python-scripts"), allow(dead_code))]
pub fn press_python_key(key: &str, up: bool) -> bool {
    let Some((vk, scan, extended)) = python_vk_and_scan(key) else {
        return false;
    };
    press_named_key(vk, scan, extended, up)
}

/// Aperta ou solta uma tecla já resolvida por uma das listas fechadas. O
/// "solta" é tentado até três vezes.
fn press_named_key(vk: u16, scan: u16, extended: bool, up: bool) -> bool {
    if !up {
        return send_key_flags(vk, scan, false, extended);
    }
    for attempt in 0..KEY_UP_ATTEMPTS {
        if send_key_flags(vk, scan, true, extended) {
            return true;
        }
        if attempt + 1 < KEY_UP_ATTEMPTS {
            std::thread::sleep(std::time::Duration::from_millis(KEY_UP_RETRY_MS));
        }
    }
    false
}

/// Área interna (cliente) da janela, em coordenadas de tela. `None` para janela
/// nula, fechada ou sem área.
pub fn client_rect_on_screen(hwnd: HWND) -> Option<crate::AfkClientRect> {
    if !window_exists(hwnd) {
        return None;
    }
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if unsafe { GetClientRect(hwnd, &mut rect) } == 0 {
        return None;
    }
    let mut origin = POINT { x: 0, y: 0 };
    if unsafe { ClientToScreen(hwnd, &mut origin) } == 0 {
        return None;
    }
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    if width <= 0 || height <= 0 {
        return None;
    }
    Some(crate::AfkClientRect {
        left: origin.x,
        top: origin.y,
        width,
        height,
    })
}

/// Posição do cursor. **Só a posição**: nunca botão, nunca tecla.
pub fn cursor_position() -> Option<(i32, i32)> {
    let mut point = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut point) } == 0 {
        None
    } else {
        Some((point.x, point.y))
    }
}

/// Um evento de mouse cru: movimento (`dx`, `dy`) e/ou botão, conforme `flags`.
/// Privado: as portas são `click_afk_point`, que decide tudo a partir da
/// janela, da porcentagem e da receita do clique, e `nudge_for_focus_back`.
fn send_mouse(dx: i32, dy: i32, flags: u32) -> bool {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) == 1 }
}

/// Um movimento de mouse de **zero** pixel, para o AFK mode poder trazer a janela
/// da conta e, no fim do ciclo, devolver o foco (issue #23).
///
/// O Windows só aceita o `SetForegroundWindow` de quem gerou a última entrada.
/// No ciclo normal essa entrada é a tecla ou o clique do AFK; se o usuário
/// mexe em outra janela depois dela (o outro monitor), a volta do foco é
/// recusada e o Roblox fica na frente. Este evento não move o cursor, não
/// aperta nada e não lê nada: só faz deste processo, de novo, o da última
/// entrada. Medido em 10/10/2026: recusado sem ele, aceito com ele (4 de 4).
pub fn nudge_for_focus_back() -> bool {
    // Movimento do app: a parada de emergência (commands/shake_stop.rs) não o
    // conta como chacoalhar do usuário.
    let _app_move = crate::app_cursor_move();
    send_mouse(0, 0, MOUSEEVENTF_MOVE)
}

/// A área de trabalho virtual (todos os monitores), em pixels de tela.
fn virtual_desktop() -> Option<crate::AfkClientRect> {
    let desktop = unsafe {
        crate::AfkClientRect {
            left: GetSystemMetrics(SM_XVIRTUALSCREEN),
            top: GetSystemMetrics(SM_YVIRTUALSCREEN),
            width: GetSystemMetrics(SM_CXVIRTUALSCREEN),
            height: GetSystemMetrics(SM_CYVIRTUALSCREEN),
        }
    };
    (desktop.width > 0 && desktop.height > 0).then_some(desktop)
}

/// Um clique esquerdo no ponto relativo `point` da área interna da janela
/// `hwnd`, que o chamador **já confirmou** estar em primeiro plano.
///
/// Executa a receita de `afk_click_plan`: o movimento sai pelo `SendInput` (o
/// Roblox lê o mouse por entrada crua e não enxerga só o cursor posto no
/// lugar), com tremor e dois cliques — o primeiro pode só focar o jogo. No fim o
/// cursor do usuário volta para onde estava, inclusive quando o clique falha.
///
/// O "solta" é tentado até três vezes: botão que fica pressionado vira arrastar
/// dentro do jogo.
pub fn click_afk_point(hwnd: HWND, point: crate::AfkPoint, hold_ms: u64) -> Result<(), String> {
    click_point_with_plan(hwnd, point, hold_ms, true)
}

/// O clique de uma Gravação: a mesma receita do AFK (`click_afk_point`), e o
/// clique de foco só quando `focus_click` — o primeiro clique depois de trazer
/// a janela. Os seguintes da mesma gravação já acham o jogo focado, e um clique
/// a mais ali apertaria o botão do jogo duas vezes.
pub fn click_recording_point(
    hwnd: HWND,
    point: crate::AfkPoint,
    hold_ms: u64,
    focus_click: bool,
) -> Result<(), String> {
    click_point_with_plan(hwnd, point, hold_ms, focus_click)
}

fn click_point_with_plan(
    hwnd: HWND,
    point: crate::AfkPoint,
    hold_ms: u64,
    focus_click: bool,
) -> Result<(), String> {
    // Daqui até o fim da função (inclusive a volta do cursor) o movimento é do
    // app: a parada de emergência por chacoalhar (commands/shake_stop.rs)
    // descarta o que ler neste meio-tempo.
    let _app_move = crate::app_cursor_move();
    let rect = client_rect_on_screen(hwnd).ok_or("The window has no game area")?;
    let steps = crate::afk_click_plan_with(rect, point, hold_ms, focus_click)
        .ok_or("The window has no game area")?;
    let desktop = virtual_desktop().ok_or("Could not read the screen size")?;
    let back = cursor_position();

    let result = (|| {
        for step in steps {
            match step {
                crate::AfkMouseStep::MoveTo(x, y) => {
                    let (nx, ny) = crate::afk_absolute_input(x, y, desktop);
                    if !send_mouse(
                        nx,
                        ny,
                        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                    ) {
                        return Err("Windows refused to move the mouse".to_string());
                    }
                }
                crate::AfkMouseStep::Nudge(dx, dy) => {
                    if !send_mouse(dx, dy, MOUSEEVENTF_MOVE) {
                        return Err("Windows refused to move the mouse".to_string());
                    }
                }
                crate::AfkMouseStep::Wait(ms) => {
                    std::thread::sleep(std::time::Duration::from_millis(ms));
                }
                crate::AfkMouseStep::Press => {
                    if !send_mouse(0, 0, MOUSEEVENTF_LEFTDOWN) {
                        return Err("Windows refused the synthetic click".to_string());
                    }
                }
                crate::AfkMouseStep::Release => {
                    let mut released = false;
                    for attempt in 0..KEY_UP_ATTEMPTS {
                        if send_mouse(0, 0, MOUSEEVENTF_LEFTUP) {
                            released = true;
                            break;
                        }
                        if attempt + 1 < KEY_UP_ATTEMPTS {
                            std::thread::sleep(std::time::Duration::from_millis(KEY_UP_RETRY_MS));
                        }
                    }
                    if !released {
                        return Err("Windows refused to release the click".to_string());
                    }
                }
            }
        }
        Ok(())
    })();

    // Devolver o cursor só reposiciona o ponteiro: não gera movimento que o
    // jogo leia, então não mexe em nada dentro dele.
    if let Some((bx, by)) = back {
        unsafe { SetCursorPos(bx, by) };
    }
    result
}

#[cfg(test)]
mod win_input_tests {
    use super::*;

    // Nada aqui envia entrada: `send_key_flags`/`tap_afk_key` e `send_mouse`/
    // `click_afk_point` só são chamados com janela nula, que é recusada antes de
    // qualquer envio. `afk_taps_with_scan` só traduz um código de tecla pelo layout
    // ativo.

    #[test]
    fn a_listed_key_gets_a_virtual_key_and_a_scan_code() {
        let taps = afk_taps_with_scan("Space").expect("Space está na lista");
        assert_eq!(taps.len(), 1);
        let (vk, scan, extended) = taps[0];
        assert_eq!(vk, 0x20);
        assert_ne!(scan, 0, "o layout tem de dar um scan code para o espaço");
        assert!(!extended);

        let taps_w = afk_taps_with_scan("w").expect("o nome não é sensível a caixa");
        assert_eq!(taps_w[0].0, 0x57);
    }

    /// O par manda as duas setas, nessa ordem, como teclas estendidas (sem a
    /// marca, o Windows as trata como as do teclado numérico).
    #[test]
    fn an_arrow_pair_is_two_extended_taps_in_order() {
        let taps = afk_taps_with_scan("Right+Left").expect("o par está na lista");
        let codes: Vec<(u16, bool)> = taps.iter().map(|(vk, _, ext)| (*vk, *ext)).collect();
        assert_eq!(codes, vec![(0x27, true), (0x25, true)]);
        assert!(taps.iter().all(|(_, scan, _)| *scan != 0));

        let up_down = afk_taps_with_scan("up+down").expect("sem caixa");
        let codes: Vec<u16> = up_down.iter().map(|(vk, _, _)| *vk).collect();
        assert_eq!(codes, vec![0x26, 0x28]);
    }

    #[test]
    fn a_key_outside_the_list_has_no_codes() {
        for outside in ["Enter", "F4", "Tab", "LWin", "", "Z", "Right", "Left", "Left+Right"] {
            assert!(
                afk_taps_with_scan(outside).is_none(),
                "tecla fora da lista foi traduzida: {outside:?}"
            );
        }
    }

    #[test]
    fn a_null_window_never_counts_as_existing() {
        assert!(!window_exists(std::ptr::null_mut()));
    }

    #[test]
    fn a_null_window_has_no_game_area() {
        assert_eq!(client_rect_on_screen(std::ptr::null_mut()), None);
    }

    #[test]
    fn a_recording_key_gets_codes_and_arrows_are_extended() {
        let (vk, scan, extended) = recording_vk_and_scan("Up").expect("Up está na lista");
        assert_eq!(vk, 0x26);
        assert!(extended);
        assert_ne!(scan, 0);
        let (vk_w, _, ext_w) = recording_vk_and_scan("w").expect("W está na lista");
        assert_eq!(vk_w, 0x57);
        assert!(!ext_w);
    }

    #[test]
    fn a_key_outside_the_recording_list_is_never_sent() {
        for outside in ["Enter", "Escape", "F4", "Tab", "LWin", ""] {
            assert!(recording_vk_and_scan(outside).is_none(), "{outside:?}");
            // Recusado antes de qualquer envio.
            assert!(!press_recording_key(outside, true), "{outside:?}");
        }
    }

    #[test]
    fn python_keys_add_escape_but_recordings_still_refuse_it() {
        for name in ["Escape", "escape", "Esc", "ESC"] {
            let (vk, scan, extended) = python_vk_and_scan(name).expect("Escape is a Python key");
            assert_eq!(vk, 0x1B);
            assert!(!extended);
            assert_ne!(scan, 0);
            assert!(recording_vk_and_scan(name).is_none(), "{name}: recordings never send it");
        }
        assert_eq!(python_vk_and_scan("Up").map(|k| k.0), Some(0x26));
        for outside in ["Enter", "F4", "Tab", "LWin", "Alt", ""] {
            assert!(python_vk_and_scan(outside).is_none(), "{outside:?}");
            assert!(!press_python_key(outside, true), "{outside:?}");
        }
    }

    #[test]
    fn a_null_window_never_gets_a_recording_click() {
        assert!(click_recording_point(std::ptr::null_mut(), crate::AFK_DEFAULT_POINT, 1, false).is_err());
    }

    #[test]
    fn a_null_window_is_never_clicked() {
        // Recusado antes de mover o cursor: nada é enviado.
        assert!(click_afk_point(std::ptr::null_mut(), crate::AFK_DEFAULT_POINT, 1).is_err());
    }
}
