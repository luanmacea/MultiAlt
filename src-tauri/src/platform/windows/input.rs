// Entrada sintética para a janela de um cliente Roblox (AFK mode).
//
// Por que `SendInput` e não `PostMessage`/`SendMessage`: o cliente do Roblox lê
// teclado pelo caminho de entrada do sistema, e mensagem postada na fila da
// janela não move o personagem. O preço do `SendInput` é o foco — ele entrega a
// tecla na janela que está em **primeiro plano**, então quem envia precisa
// trazer a janela do Roblox para frente, **confirmar que ela chegou lá** e
// devolver o foco depois (o ciclo mora em `commands/afk.rs`).
//
// Este módulo só **envia**, e envia só tecla da lista fechada: `send_key` é
// privado de propósito, e a única porta é `tap_afk_key`, que recebe **nome** de
// tecla e o resolve pela lista. Assim não existe chamador com virtual key cru.
// Ler o teclado do usuário é proibido aqui, e a trava é o
// `afk_input_safety_tests` (em `commands/afk.rs`), que varre este arquivo.

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    MAPVK_VK_TO_VSC,
};
use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;

/// Quantas vezes o "solta a tecla" é tentado. Tecla que fica logicamente
/// pressionada no cliente faz o personagem **andar** — exatamente o que o AFK
/// mode existe para evitar.
const KEY_UP_ATTEMPTS: u32 = 3;
/// Respiro entre duas tentativas de soltar a tecla.
const KEY_UP_RETRY_MS: u64 = 15;

/// `(virtual key, scan code)` de uma tecla da lista fechada do AFK mode, ou
/// `None` para qualquer outro nome.
///
/// A lista fica no raiz do crate (`commands/afk.rs`) porque a tela também a
/// consulta; aqui só se acrescenta o scan code, que depende do layout de teclado
/// ativo. `MapVirtualKeyW` **traduz** um código de tecla pelo layout: não lê
/// tecla pressionada nem estado de teclado.
fn vk_and_scan(key: &str) -> Option<(u16, u16)> {
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
/// `up = true` solta. Devolve `false` quando o Windows recusou o envio.
///
/// Privado: quem chama passa **nome** de tecla por `tap_afk_key`, nunca um
/// virtual key cru.
fn send_key(vk: u16, scan: u16, up: bool) -> bool {
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

/// Um toque na tecla `key` (da lista fechada) na janela que está em primeiro
/// plano: pressiona, espera `hold_ms` e solta.
///
/// O "solta" é tentado até três vezes: se ele falhar, a tecla fica logicamente
/// pressionada dentro do cliente e o personagem sai andando.
pub fn tap_afk_key(key: &str, hold_ms: u64) -> Result<(), String> {
    let (vk, scan) =
        vk_and_scan(key).ok_or_else(|| format!("Key not allowed in AFK mode: {}", key))?;

    let down = send_key(vk, scan, false);
    std::thread::sleep(std::time::Duration::from_millis(hold_ms));

    let mut up = false;
    for attempt in 0..KEY_UP_ATTEMPTS {
        if send_key(vk, scan, true) {
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
    Ok(())
}

#[cfg(test)]
mod win_input_tests {
    use super::*;

    // Nada aqui envia entrada: `send_key`/`tap_afk_key` são o único caminho de
    // envio e não são chamados em teste nenhum. `vk_and_scan` só traduz um
    // código de tecla pelo layout ativo.

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
