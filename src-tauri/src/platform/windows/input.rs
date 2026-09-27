// Entrada sintética para a janela de um cliente Roblox (AFK mode).
//
// Por que `SendInput` e não `PostMessage`/`SendMessage`: o cliente do Roblox lê
// teclado pelo caminho de entrada do sistema, e mensagem postada na fila da
// janela não move o personagem. O preço do `SendInput` é o foco — ele entrega a
// tecla na janela que está em **primeiro plano**, então quem envia precisa
// trazer a janela do Roblox para frente por um instante e devolver o foco
// depois (o ciclo mora em `commands/afk.rs`).
//
// Este módulo só **envia**. Ler o teclado do usuário é proibido aqui, e a trava
// é o teste `afk_input_safety_tests` (em `commands/afk.rs`), que varre o corpo
// deste arquivo.

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    MAPVK_VK_TO_VSC,
};
use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;

/// `(virtual key, scan code)` da tecla escolhida, ou `None` para qualquer nome
/// fora da lista fechada do AFK mode.
///
/// A lista fica no raiz do crate (`commands/afk.rs`) porque a tela também a
/// consulta; aqui só se acrescenta o scan code, que depende do layout de teclado
/// ativo. `MapVirtualKeyW` **traduz** um código de tecla pelo layout: não lê
/// tecla pressionada nem estado de teclado.
pub fn vk_and_scan(key: &str) -> Option<(u16, u16)> {
    let vk = crate::afk_virtual_key(key)?;
    let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16;
    Some((vk, scan))
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
/// `up = true` solta. Devolve `false` quando o Windows recusou o envio (por
/// exemplo, se a janela em foco for de um processo de integridade maior).
pub fn send_key(vk: u16, scan: u16, up: bool) -> bool {
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) == 1 }
}

#[cfg(test)]
mod win_input_tests {
    use super::*;

    // Nada aqui envia entrada: `send_key` é o único caminho de envio e não é
    // chamado em teste nenhum. `vk_and_scan` só traduz um código de tecla.

    #[test]
    fn a_listed_key_gets_a_virtual_key_and_a_scan_code() {
        let (vk, scan) = vk_and_scan("Space").expect("Space está na lista");
        assert_eq!(vk, 0x20);
        assert_ne!(scan, 0, "o layout tem de dar um scan code para o espaço");

        let (vk_w, _) = vk_and_scan("w").expect("o nome não é sensível a caixa");
        assert_eq!(vk_w, 0x57);
    }

    #[test]
    fn a_key_outside_the_list_has_no_codes() {
        for outside in ["Enter", "F4", "Tab", "LWin", "", "Z"] {
            assert!(
                vk_and_scan(outside).is_none(),
                "tecla fora da lista foi traduzida: {outside:?}"
            );
        }
    }

    #[test]
    fn a_null_window_never_counts_as_existing() {
        assert!(!window_exists(std::ptr::null_mut()));
    }
}
