// Laço da janela em uso: uma olhada por segundo em qual janela está em
// primeiro plano, para a otimização que segue o foco
// (`platform/windows/focus_follow.rs`) e o volume ao vivo (`live_audio.rs`), e
// para devolver a moldura quando a grade sem moldura desliga
// (`windowing.rs`). Desligadas as opções, a olhada só lê o INI e volta. Ver
// docs/features/performance.md.

/// Intervalo entre duas olhadas.
#[cfg(target_os = "windows")]
const FOCUS_FOLLOW_TICK: std::time::Duration = std::time::Duration::from_secs(1);

#[cfg(target_os = "windows")]
pub(crate) fn start_focus_follow_loop(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let handle = app.clone();
            let _ = tokio::task::spawn_blocking(move || {
                let settings = handle.state::<SettingsStore>();
                platform::windows::focus_follow_tick(settings.inner());
                // Grade sem moldura desligada: as molduras voltam.
                platform::windows::grid_borders_tick(settings.inner());
            })
            .await;
            tokio::time::sleep(FOCUS_FOLLOW_TICK).await;
        }
    });
}

/// Ao fechar o app: cada cliente volta ao que o launch deu, o que o app mutou
/// volta a tocar e as janelas da grade sem moldura recebem a moldura de volta.
#[cfg(target_os = "windows")]
fn release_focus_follow_on_exit(app: &AppHandle<Wry>) {
    platform::windows::release_focus_follow(app.state::<SettingsStore>().inner());
    platform::windows::release_borderless_windows();
}
