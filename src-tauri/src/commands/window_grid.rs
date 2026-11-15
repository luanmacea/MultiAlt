// Laços da grade de janelas (`platform/windows/windowing.rs`): devolver a
// moldura quando a grade sem moldura desliga e pôr de volta na célula a janela
// que volta de minimizada.

/// Intervalo entre duas olhadas da moldura.
#[cfg(target_os = "windows")]
const GRID_BORDERS_TICK: std::time::Duration = std::time::Duration::from_secs(1);

/// Grade sem moldura desligada: as molduras voltam. Desligada a opção, a
/// olhada só lê o INI e volta.
#[cfg(target_os = "windows")]
pub(crate) fn start_grid_borders_loop(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let handle = app.clone();
            let _ = tokio::task::spawn_blocking(move || {
                let settings = handle.state::<SettingsStore>();
                platform::windows::grid_borders_tick(settings.inner());
            })
            .await;
            tokio::time::sleep(GRID_BORDERS_TICK).await;
        }
    });
}

/// Intervalo da olhada da grade: rápido o bastante para pegar quem minimiza e
/// restaura logo em seguida (a de 1 s perdia a volta), e quase de graça com a
/// lista vazia (uma trava e volta).
#[cfg(target_os = "windows")]
const GRID_SNAP_TICK: std::time::Duration = std::time::Duration::from_millis(250);

/// Janela da grade que volta de minimizada vai para a célula dela
/// (`platform/windows/windowing.rs`, `snap_restored_small_windows`).
#[cfg(target_os = "windows")]
pub(crate) fn start_grid_snap_loop() {
    tauri::async_runtime::spawn(async move {
        loop {
            let _ = tokio::task::spawn_blocking(platform::windows::snap_restored_small_windows).await;
            tokio::time::sleep(GRID_SNAP_TICK).await;
        }
    });
}

/// Ao fechar o app: as janelas da grade sem moldura recebem a moldura de volta.
#[cfg(target_os = "windows")]
fn release_grid_windows_on_exit() {
    platform::windows::release_borderless_windows();
}
