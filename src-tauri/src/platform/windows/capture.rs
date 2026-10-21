// Captura da área interna de uma janela, para a automação Python (pixel e
// achar imagem) e o inspetor (docs/features/python-automation.md, "Captura").
//
// `PrintWindow` com `PW_RENDERFULLCONTENT` pede à janela que se desenhe num
// bitmap nosso: funciona com ela **atrás** de outras (não minimizada), e é o
// único jeito de pegar conteúdo DirectX/Chromium sem trazê-la para frente.
// Quando ela está na frente e o resultado vem todo preto (alguns caminhos de
// composição não respondem ao PrintWindow), copia da tela.
//
// Só **lê** pixels de uma janela: não envia nada, não lê teclado nem mouse.
// Fragmento do módulo `windows` (include! em platform/windows.rs), que é varrido
// pelo `afk_input_safety_tests`.

use windows_sys::Win32::Graphics::Gdi as cap_gdi;
use windows_sys::Win32::Storage::Xps as cap_xps;
use windows_sys::Win32::System::Memory as cap_mem;

/// `PW_CLIENTONLY | PW_RENDERFULLCONTENT`.
const CAPTURE_FLAGS: u32 = 1 | 2;

/// Tamanho da área interna (cliente) da janela.
fn capture_client_size(hwnd: HWND) -> Option<(i32, i32)> {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if hwnd.is_null()
        || unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect) } == 0
    {
        return None;
    }
    let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
    (w > 0 && h > 0).then_some((w, h))
}

/// Os pixels BGRA da área interna (`largura`, `altura`, `largura*altura*4`
/// bytes, de cima para baixo). O alfa sai sempre 255.
pub fn capture_client_bgra(hwnd: HWND) -> Result<(u32, u32, Vec<u8>), String> {
    let exists = window_exists(hwnd);
    let minimized = exists && window_is_minimized(hwnd);
    // `PrintWindow` espera a janela se desenhar: travada, ele trava junto.
    let hung = exists && !minimized && is_window_hung(hwnd);
    if let Some(reason) = capture_blocker(exists, minimized, hung) {
        return Err(reason.into());
    }
    let (w, h) = capture_client_size(hwnd).ok_or("the window has no game area")?;
    let mut pixels = unsafe { render_with(hwnd, w, h, false)? };
    if all_black(&pixels) && get_foreground_hwnd() == hwnd {
        pixels = unsafe { render_with(hwnd, w, h, true)? };
    }
    for px in pixels.chunks_exact_mut(4) {
        px[3] = 255;
    }
    Ok((w as u32, h as u32, pixels))
}

/// Por que a janela não pode ser capturada agora (`None`: pode).
fn capture_blocker(exists: bool, minimized: bool, hung: bool) -> Option<&'static str> {
    if !exists {
        Some("the window is gone")
    } else if minimized {
        Some("the window is minimized")
    } else if hung {
        Some("window is not responding")
    } else {
        None
    }
}

fn all_black(pixels: &[u8]) -> bool {
    pixels.chunks_exact(4).all(|px| px[0] == 0 && px[1] == 0 && px[2] == 0)
}

/// Desenha a janela num bitmap de memória (`from_screen = false`: PrintWindow;
/// `true`: cópia da tela na posição da área interna) e lê os bytes.
unsafe fn render_with(hwnd: HWND, w: i32, h: i32, from_screen: bool) -> Result<Vec<u8>, String> {
    let screen = cap_gdi::GetDC(std::ptr::null_mut());
    if screen.is_null() {
        return Err("no screen device context".into());
    }
    let memory = cap_gdi::CreateCompatibleDC(screen);
    let bitmap = cap_gdi::CreateCompatibleBitmap(screen, w, h);
    let result = (|| {
        if memory.is_null() || bitmap.is_null() {
            return Err("could not allocate the capture bitmap".to_string());
        }
        let old = cap_gdi::SelectObject(memory, bitmap);
        let drawn = if from_screen {
            let mut origin = POINT { x: 0, y: 0 };
            cap_gdi::ClientToScreen(hwnd, &mut origin);
            cap_gdi::BitBlt(memory, 0, 0, w, h, screen, origin.x, origin.y, cap_gdi::SRCCOPY) != 0
        } else {
            cap_xps::PrintWindow(hwnd, memory, CAPTURE_FLAGS) != 0
        };
        cap_gdi::SelectObject(memory, old);
        if !drawn {
            return Err(if from_screen { "screen copy failed" } else { "PrintWindow failed" }.to_string());
        }
        let mut info: cap_gdi::BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<cap_gdi::BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = w;
        // Altura negativa: linhas de cima para baixo.
        info.bmiHeader.biHeight = -h;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = cap_gdi::BI_RGB;
        let mut pixels = vec![0u8; (w as usize) * (h as usize) * 4];
        let lines = cap_gdi::GetDIBits(
            memory,
            bitmap,
            0,
            h as u32,
            pixels.as_mut_ptr() as *mut core::ffi::c_void,
            &mut info,
            cap_gdi::DIB_RGB_COLORS,
        );
        if lines != h {
            return Err("could not read the capture bitmap".to_string());
        }
        Ok(pixels)
    })();
    if !bitmap.is_null() {
        cap_gdi::DeleteObject(bitmap);
    }
    if !memory.is_null() {
        cap_gdi::DeleteDC(memory);
    }
    cap_gdi::ReleaseDC(std::ptr::null_mut(), screen);
    result
}

/// DPI da tela onde a janela está (96 = 100%).
pub fn window_dpi(hwnd: HWND) -> u32 {
    unsafe {
        let dc = cap_gdi::GetDC(hwnd);
        if dc.is_null() {
            return 96;
        }
        let dpi = cap_gdi::GetDeviceCaps(dc, cap_gdi::LOGPIXELSX as i32);
        cap_gdi::ReleaseDC(hwnd, dc);
        if dpi > 0 {
            dpi as u32
        } else {
            96
        }
    }
}

/// Um quadro numa memória compartilhada nomeada (`Local\multialt-cap-...`),
/// que o Python abre com `mmap.mmap(-1, n, tagname=nome)` e lê sem passar os
/// bytes por JSON. A memória vive enquanto este valor viver.
pub struct SharedFrame {
    handle: HANDLE,
    pub name: String,
    pub len: usize,
}

unsafe impl Send for SharedFrame {}
unsafe impl Sync for SharedFrame {}

impl SharedFrame {
    pub fn create(name: &str, bytes: &[u8]) -> Result<Self, String> {
        let wide = encode_wide(name);
        let len = bytes.len().max(1);
        unsafe {
            let handle = cap_mem::CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                std::ptr::null(),
                cap_mem::PAGE_READWRITE,
                ((len as u64) >> 32) as u32,
                (len as u64 & 0xFFFF_FFFF) as u32,
                wide.as_ptr(),
            );
            if handle.is_null() {
                return Err("could not create the shared frame".into());
            }
            let view = cap_mem::MapViewOfFile(handle, cap_mem::FILE_MAP_ALL_ACCESS, 0, 0, len);
            if view.Value.is_null() {
                CloseHandle(handle);
                return Err("could not map the shared frame".into());
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), view.Value as *mut u8, bytes.len());
            cap_mem::UnmapViewOfFile(view);
            Ok(Self {
                handle,
                name: name.to_string(),
                len,
            })
        }
    }

    /// Lê de volta pelo nome, como o Python faria (para os testes).
    pub fn read_by_name(name: &str, len: usize) -> Option<Vec<u8>> {
        let wide = encode_wide(name);
        unsafe {
            let handle = cap_mem::OpenFileMappingW(cap_mem::FILE_MAP_READ, 0, wide.as_ptr());
            if handle.is_null() {
                return None;
            }
            let view = cap_mem::MapViewOfFile(handle, cap_mem::FILE_MAP_READ, 0, 0, len);
            let out = if view.Value.is_null() {
                None
            } else {
                let mut buf = vec![0u8; len];
                std::ptr::copy_nonoverlapping(view.Value as *const u8, buf.as_mut_ptr(), len);
                cap_mem::UnmapViewOfFile(view);
                Some(buf)
            };
            CloseHandle(handle);
            out
        }
    }
}

impl Drop for SharedFrame {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod python_capture_tests {
    use super::*;

    #[test]
    fn a_null_window_cannot_be_captured() {
        assert!(capture_client_bgra(std::ptr::null_mut()).is_err());
        assert_eq!(capture_client_size(std::ptr::null_mut()), None);
    }

    /// `PrintWindow` manda a janela se desenhar e espera: com ela "Não
    /// respondendo" a captura travaria a thread. Pula com erro claro.
    #[test]
    fn a_hung_window_is_not_captured() {
        assert_eq!(capture_blocker(false, false, false), Some("the window is gone"));
        assert_eq!(capture_blocker(true, true, false), Some("the window is minimized"));
        assert_eq!(capture_blocker(true, false, true), Some("window is not responding"));
        assert_eq!(capture_blocker(true, false, false), None);
    }

    #[test]
    fn black_detection_ignores_alpha() {
        assert!(all_black(&[0, 0, 0, 255, 0, 0, 0, 0]));
        assert!(!all_black(&[0, 0, 1, 255]));
    }

    #[test]
    fn a_shared_frame_is_readable_by_name_until_dropped() {
        let name = format!("Local\\multialt-cap-test-{}", std::process::id());
        let bytes: Vec<u8> = (0..4096u32).map(|i| (i % 251) as u8).collect();
        let frame = SharedFrame::create(&name, &bytes).unwrap();
        assert_eq!(SharedFrame::read_by_name(&name, bytes.len()).as_deref(), Some(bytes.as_slice()));
        drop(frame);
        assert!(SharedFrame::read_by_name(&name, bytes.len()).is_none());
    }

    /// P0 do spike, metade "memória compartilhada": o `mmap` do Python abre o
    /// quadro pelo nome. Pula (passa com aviso) sem `python` no PATH.
    #[test]
    fn python_mmap_reads_the_shared_frame() {
        use std::os::windows::process::CommandExt;
        let name = format!("Local\\multialt-cap-pytest-{}", std::process::id());
        let bytes: Vec<u8> = (0..1024u32).map(|i| (i * 7 % 256) as u8).collect();
        let _frame = SharedFrame::create(&name, &bytes).unwrap();
        let code = format!(
            "import mmap,hashlib;m=mmap.mmap(-1,{},tagname={:?});print(hashlib.sha256(m[:]).hexdigest())",
            bytes.len(),
            name
        );
        let output = std::process::Command::new("python")
            .args(["-I", "-c", &code])
            .creation_flags(0x0800_0000)
            .output();
        let Ok(output) = output else {
            eprintln!("python not on PATH: skipping the mmap check");
            return;
        };
        if !output.status.success() {
            eprintln!("python could not run: {}", String::from_utf8_lossy(&output.stderr));
            return;
        }
        use sha2::Digest;
        let expected: String = sha2::Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), expected);
    }

    /// P0 do spike, metade "captura": PrintWindow numa janela Chromium/DirectX
    /// que **não** está na frente devolve pixels de verdade. Manual (lê a tela
    /// do usuário): `cargo test --all-features --lib python_capture_tests -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn spike_printwindow_on_a_background_gpu_window() {
        use windows_sys::Win32::UI::WindowsAndMessaging::GetClassNameW;
        struct Found(Vec<(isize, String)>);
        unsafe extern "system" fn each(hwnd: HWND, lparam: isize) -> i32 {
            let found = &mut *(lparam as *mut Found);
            if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
                return 1;
            }
            let mut class = [0u16; 128];
            let n = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
            let class = String::from_utf16_lossy(&class[..n.max(0) as usize]);
            if class == "Chrome_WidgetWin_1" || class == "WINDOWSCLIENT" || class.contains("Roblox") {
                found.0.push((hwnd as isize, class));
            }
            1
        }
        let mut found = Found(Vec::new());
        unsafe { EnumWindows(Some(each), &mut found as *mut _ as isize) };
        let front = get_foreground_hwnd() as isize;
        let mut proved = false;
        for (hwnd, class) in found.0.iter().filter(|(h, _)| *h != front) {
            let Ok((w, h, pixels)) = capture_client_bgra(*hwnd as HWND) else {
                continue;
            };
            let lit = pixels.chunks_exact(4).filter(|p| p[0] > 8 || p[1] > 8 || p[2] > 8).count();
            let ratio = lit as f64 / (w as f64 * h as f64);
            // Só a classe e a contagem: nada do conteúdo da tela do usuário.
            eprintln!("{class} {w}x{h} lit={:.1}%", ratio * 100.0);
            if ratio > 0.05 {
                proved = true;
            }
        }
        assert!(proved, "no background GPU window gave non-black pixels");
    }
}
