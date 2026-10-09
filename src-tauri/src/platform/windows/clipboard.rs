// Área de transferência para credencial (cookie, senha) copiada pelo app.
//
// Este módulo só **escreve** e **apaga**. Ler o conteúdo da área de
// transferência é proibido aqui: é o padrão de programa que rouba senha, e os
// modelos de ML dos antivírus o associam a malware. Para saber se o que está lá
// ainda é nosso, compara-se o **número de sequência** do Windows
// (`GetClipboardSequenceNumber`, que só conta mudanças) — nunca o texto. A
// trava é o `clipboard_write_only_tests`, que varre este arquivo.
//
// Junto com o texto vão três formatos registrados que o Windows reconhece:
// - `ExcludeClipboardContentFromMonitorProcessing`: monitores de clipboard
//   (o histórico do Win+V, por exemplo) devem ignorar o conteúdo;
// - `CanIncludeInClipboardHistory` = 0: fora do histórico do Win+V;
// - `CanUploadToCloudClipboard` = 0: não vai para a nuvem (sincronização entre
//   aparelhos).

use windows_sys::Win32::Foundation::GlobalFree;
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardSequenceNumber, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

/// `CF_UNICODETEXT`: texto UTF-16 terminado em zero.
const CF_UNICODETEXT: u32 = 13;

/// Outro programa pode estar com a área de transferência aberta; o Windows não
/// enfileira, quem chega depois tenta de novo.
const OPEN_ATTEMPTS: u32 = 10;
const OPEN_RETRY_MS: u64 = 20;

/// Fecha a área de transferência ao sair do escopo, inclusive no caminho de erro.
struct OpenedClipboard;

impl OpenedClipboard {
    fn open() -> Result<Self, String> {
        for attempt in 0..OPEN_ATTEMPTS {
            if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
                return Ok(Self);
            }
            if attempt + 1 < OPEN_ATTEMPTS {
                std::thread::sleep(std::time::Duration::from_millis(OPEN_RETRY_MS));
            }
        }
        Err("The clipboard is busy (another program is using it). Try again.".to_string())
    }
}

impl Drop for OpenedClipboard {
    fn drop(&mut self) {
        unsafe {
            CloseClipboard();
        }
    }
}

/// Copia `bytes` para um bloco global e o entrega à área de transferência no
/// formato `format`. Depois do `SetClipboardData` o bloco é do Windows; se ele
/// recusar, o bloco é nosso e é liberado aqui.
fn put_bytes(format: u32, bytes: &[u8]) -> Result<(), String> {
    unsafe {
        let mem = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1));
        if mem.is_null() {
            return Err("Could not allocate memory for the clipboard.".to_string());
        }
        let ptr = GlobalLock(mem) as *mut u8;
        if ptr.is_null() {
            GlobalFree(mem);
            return Err("Could not allocate memory for the clipboard.".to_string());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        GlobalUnlock(mem);
        if SetClipboardData(format, mem).is_null() {
            GlobalFree(mem);
            return Err("Could not put the text on the clipboard.".to_string());
        }
    }
    Ok(())
}

fn registered_format(name: &str) -> u32 {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe { RegisterClipboardFormatW(wide.as_ptr()) }
}

/// O texto em UTF-16 terminado em zero, como `CF_UNICODETEXT` pede.
fn utf16_bytes(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(|unit| unit.to_le_bytes())
        .collect()
}

/// Põe `text` na área de transferência marcado como fora do histórico do Win+V
/// e da nuvem. Devolve o número de sequência **depois** da cópia: é com ele que
/// [`clear_secret_if_unchanged`] sabe, sem ler nada, se o conteúdo ainda é este.
pub fn copy_secret_text(text: &str) -> Result<u32, String> {
    {
        let _open = OpenedClipboard::open()?;
        if unsafe { EmptyClipboard() } == 0 {
            return Err("Could not clear the clipboard before copying.".to_string());
        }
        put_bytes(CF_UNICODETEXT, &utf16_bytes(text))?;
        // Melhor esforço: um Windows sem o histórico (versões antigas) só não
        // conhece o formato, e o texto já está lá.
        let zero = 0u32.to_le_bytes();
        for (name, data) in [
            ("ExcludeClipboardContentFromMonitorProcessing", &zero[..]),
            ("CanIncludeInClipboardHistory", &zero[..]),
            ("CanUploadToCloudClipboard", &zero[..]),
        ] {
            let format = registered_format(name);
            if format != 0 {
                let _ = put_bytes(format, data);
            }
        }
    }
    Ok(unsafe { GetClipboardSequenceNumber() })
}

/// Apaga a área de transferência **só** se ninguém mexeu nela desde a nossa
/// cópia (mesmo número de sequência). Se a pessoa já copiou outra coisa, não
/// toca. Devolve `true` quando apagou.
pub fn clear_secret_if_unchanged(sequence: u32) -> bool {
    if unsafe { GetClipboardSequenceNumber() } != sequence {
        return false;
    }
    let Ok(_open) = OpenedClipboard::open() else {
        return false;
    };
    // Segunda conferência já com a área de transferência aberta: entre a
    // primeira e o `OpenClipboard` alguém pode ter copiado outra coisa.
    if unsafe { GetClipboardSequenceNumber() } != sequence {
        return false;
    }
    unsafe { EmptyClipboard() != 0 }
}

#[cfg(test)]
mod win_clipboard_tests {
    use super::*;

    #[test]
    fn text_is_utf16_with_a_terminating_zero() {
        assert_eq!(utf16_bytes("ab"), vec![b'a', 0, b'b', 0, 0, 0]);
        assert_eq!(utf16_bytes(""), vec![0, 0]);
        // `_|WARNING` e acento passam inteiros.
        let bytes = utf16_bytes("é");
        assert_eq!(bytes, vec![0xE9, 0x00, 0, 0]);
    }

    /// Os três nomes são registrados pelo Windows (o registro não mexe na área
    /// de transferência; só devolve o id do formato).
    #[test]
    fn the_privacy_formats_register() {
        for name in [
            "ExcludeClipboardContentFromMonitorProcessing",
            "CanIncludeInClipboardHistory",
            "CanUploadToCloudClipboard",
        ] {
            assert_ne!(registered_format(name), 0, "{name}");
        }
    }
}

/// Trava estrutural: este arquivo nunca **lê** a área de transferência.
#[cfg(test)]
mod clipboard_write_only_tests {
    #[test]
    fn the_clipboard_module_never_reads_contents() {
        let source = include_str!("clipboard.rs");
        // Monta os nomes em pedaços para o próprio teste não casar consigo.
        for forbidden in [
            ["Get", "ClipboardData"].concat(),
            ["IsClipboard", "FormatAvailable"].concat(),
            ["EnumClipboard", "Formats"].concat(),
            ["GetPriority", "ClipboardFormat"].concat(),
            ["AddClipboard", "FormatListener"].concat(),
            ["SetClipboard", "Viewer"].concat(),
        ] {
            assert!(
                !source.contains(&forbidden),
                "clipboard.rs must stay write-only; found {forbidden}"
            );
        }
    }
}
