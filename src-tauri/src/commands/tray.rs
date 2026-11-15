// Menu do ícone da bandeja: Show e Quit. Ver docs/features/ui-layout.md
// ("Bandeja").

const TRAY_ID: &str = "main";
const TRAY_SHOW_ID: &str = "show";
const TRAY_QUIT_ID: &str = "quit";

/// A tela está trancada por inatividade (ideia 27)? A sobreposição de bloqueio
/// é só do frontend (deixa a janela `inert`); o frontend liga/desliga via
/// `set_app_locked`. Começa destrancado; reinicia destrancado a cada processo,
/// que é o certo.
static APP_LOCKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[allow(dead_code)]
pub(crate) fn app_is_locked() -> bool {
    APP_LOCKED.load(std::sync::atomic::Ordering::SeqCst)
}

/// O frontend liga ao trancar por inatividade e desliga ao destrancar.
#[tauri::command]
fn set_app_locked(locked: bool) {
    APP_LOCKED.store(locked, std::sync::atomic::Ordering::SeqCst);
}

fn build_tray_menu(app: &AppHandle<Wry>) -> tauri::Result<tauri::menu::Menu<Wry>> {
    let show = MenuItemBuilder::with_id(TRAY_SHOW_ID, "Show").build(app)?;
    let quit = MenuItemBuilder::with_id(TRAY_QUIT_ID, "Quit").build(app)?;
    MenuBuilder::new(app).item(&show).separator().item(&quit).build()
}

fn show_main_window(app: &AppHandle<Wry>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn handle_tray_menu_event(app: &AppHandle<Wry>, menu_id: &str) {
    match menu_id {
        TRAY_SHOW_ID => show_main_window(app),
        TRAY_QUIT_ID => app.exit(0),
        _ => {}
    }
}
