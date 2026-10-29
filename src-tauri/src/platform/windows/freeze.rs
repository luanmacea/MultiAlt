// Freeze (pedido do dono, 29/10/2026, ideia da issue #40): congela o cliente de
// uma conta sem fechá-lo — o jogo para de desenhar e de gastar CPU/GPU, e volta
// de onde estava. Ver docs/features/freeze.md.
//
// É o mesmo efeito de segurar o botão direito na barra de título do Roblox: ali
// o Windows prende a thread da janela num laço próprio até o botão soltar, e o
// jogo (que roda nessa thread) para. Esse laço não dá para provocar por
// mensagem: medido em 29/10/2026 numa janela de teste, o `WM_NCRBUTTONDOWN`
// postado volta na hora (o Windows confere o botão de verdade). Então o app
// pausa essa mesma thread (`SuspendThread`) e a solta depois. As outras threads
// do cliente (rede) seguem rodando, como no truque do clique.
//
// Só a thread da janela, nunca o processo inteiro: com tudo pausado a conexão
// cai. E cada pausa do app é desfeita uma vez só (`ResumeThread`), para nunca
// soltar uma pausa que não foi o app que pôs.

/// Pausa a thread dona da janela. Devolve `(pid, tid)` do cliente.
pub fn freeze_window_thread(hwnd: HWND) -> Result<(u32, u32), String> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::Threading::{OpenThread, SuspendThread, THREAD_SUSPEND_RESUME};
    let mut pid = 0u32;
    let tid = unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if tid == 0 || pid == 0 {
        return Err("The Roblox window is gone.".to_string());
    }
    let handle = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, tid) };
    if handle.is_null() {
        let code = unsafe { GetLastError() };
        return Err(format!("Windows did not let MultiAlt pause this client (error {code})."));
    }
    let previous = unsafe { SuspendThread(handle) };
    unsafe { CloseHandle(handle) };
    if previous == u32::MAX {
        return Err("Windows did not pause this client.".to_string());
    }
    Ok((pid, tid))
}

/// Solta a pausa que o app pôs na thread `tid` do processo `pid`. Confere que a
/// thread ainda é desse processo (o id pode ter sido reaproveitado depois que o
/// cliente fechou). `false` quando não havia o que soltar.
pub fn thaw_thread(pid: u32, tid: u32) -> bool {
    use windows_sys::Win32::System::Threading::{
        GetProcessIdOfThread, OpenThread, ResumeThread, THREAD_QUERY_LIMITED_INFORMATION,
        THREAD_SUSPEND_RESUME,
    };
    let handle = unsafe { OpenThread(THREAD_SUSPEND_RESUME | THREAD_QUERY_LIMITED_INFORMATION, 0, tid) };
    if handle.is_null() {
        return false;
    }
    let owner = unsafe { GetProcessIdOfThread(handle) };
    // `ResumeThread` devolve a contagem de antes: 0 = não estava pausada (nada
    // a desfazer); falha = u32::MAX.
    let previous = if owner == pid { unsafe { ResumeThread(handle) } } else { u32::MAX };
    unsafe { CloseHandle(handle) };
    previous != u32::MAX && previous > 0
}

#[cfg(test)]
mod freeze_thread_tests {
    use super::*;
    use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering as AtomicOrdering};
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostMessageW,
        RegisterClassW, TranslateMessage, MSG, WM_APP, WM_CLOSE, WNDCLASSW, WS_EX_TOOLWINDOW, WS_OVERLAPPEDWINDOW,
    };

    static TICKS: AtomicU32 = AtomicU32::new(0);
    static WINDOW: AtomicIsize = AtomicIsize::new(0);

    unsafe extern "system" fn count_ticks(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        if m == WM_APP {
            TICKS.fetch_add(1, AtomicOrdering::SeqCst);
            return 0;
        }
        DefWindowProcW(h, m, w, l)
    }

    /// Janela de teste que nunca aparece (sem `ShowWindow`), numa thread com
    /// laço de mensagens, como a do Roblox.
    fn hidden_window() -> HWND {
        std::thread::spawn(|| unsafe {
            let class: Vec<u16> = "MultiAltFreezeTest".encode_utf16().chain(Some(0)).collect();
            let mut wc: WNDCLASSW = std::mem::zeroed();
            wc.lpfnWndProc = Some(count_ticks);
            wc.hInstance = GetModuleHandleW(std::ptr::null());
            wc.lpszClassName = class.as_ptr();
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW, class.as_ptr(), class.as_ptr(), WS_OVERLAPPEDWINDOW,
                -30000, -30000, 200, 200, std::ptr::null_mut(), std::ptr::null_mut(), wc.hInstance, std::ptr::null(),
            );
            WINDOW.store(hwnd as isize, AtomicOrdering::SeqCst);
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
        for _ in 0..100 {
            let hwnd = WINDOW.load(AtomicOrdering::SeqCst);
            if hwnd != 0 {
                return hwnd as HWND;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("test window never came up");
    }

    fn ticks_after_post(hwnd: HWND) -> u32 {
        unsafe { PostMessageW(hwnd, WM_APP, 0, 0) };
        std::thread::sleep(Duration::from_millis(250));
        TICKS.load(AtomicOrdering::SeqCst)
    }

    #[test]
    fn a_frozen_window_thread_stops_and_thawing_it_picks_up_where_it_was() {
        let hwnd = hidden_window();
        assert_eq!(ticks_after_post(hwnd), 1);

        let (pid, tid) = freeze_window_thread(hwnd).expect("freeze");
        assert_eq!(pid, std::process::id());
        assert_eq!(ticks_after_post(hwnd), 1, "a frozen thread handles nothing");

        assert!(thaw_thread(pid, tid));
        std::thread::sleep(Duration::from_millis(250));
        assert_eq!(TICKS.load(AtomicOrdering::SeqCst), 2, "the message posted while frozen runs after");
        assert!(!thaw_thread(pid, tid), "a second thaw has nothing of ours to undo");
        assert!(!thaw_thread(pid.wrapping_add(4), tid), "never thaws a thread of another process");
        unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) };
    }

    #[test]
    fn a_window_that_is_gone_is_not_frozen() {
        assert!(freeze_window_thread(std::ptr::null_mut()).is_err());
    }
}
