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
