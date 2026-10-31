// Live: a janela "MultiAlt Live" com as contas em jogo, ao vivo, numa grade —
// para transmitir só ela. A cola com o resto do app; quem desenha é o DWM
// (platform/windows/live_view.rs). Ver docs/features/live.md.
//
// - A janela (rótulo `live`) é a tela `?view=live` do mesmo frontend: ela mede o
//   próprio tamanho, calcula a grade (src/live/liveGrid.ts) e manda as células
//   para cá (`live_set_layout`). As miniaturas ficam por cima dela, nas células.
// - As contas: as que têm cliente aberto (pelo app ou pelo site, reconhecidas)
//   e foram marcadas na página Live (`Live.Accounts`; vazio = todas).
// - Só Windows; nas outras plataformas a lista vem vazia.

const LIVE_WINDOW: &str = "live";

/// Uma conta que pode aparecer na Live.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveAccount {
    user_id: i64,
    /// O nome como a tela mostra (mascarado com "Names hidden"); vazio com
    /// "Show account names" desligado na Live.
    label: String,
    in_game: bool,
    selected: bool,
}

/// A célula de uma conta, em pixels da janela da Live.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveCell {
    user_id: i64,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

/// `Live.Accounts`: ids separados por vírgula; vazio = todas as que estão
/// abertas (inclusive as que abrirem depois); `none` = nenhuma.
fn live_selection(raw: &str) -> Option<Vec<i64>> {
    if raw.trim() == LIVE_NONE {
        return Some(Vec::new());
    }
    let ids: Vec<i64> = raw.split(',').filter_map(|s| s.trim().parse().ok()).filter(|id| *id > 0).collect();
    (!ids.is_empty()).then_some(ids)
}

const LIVE_NONE: &str = "none";

/// O que grava: `all` = todas (vazio); lista vazia sem `all` = nenhuma.
fn live_selection_text(ids: &[i64], all: bool) -> String {
    if all {
        return String::new();
    }
    if ids.is_empty() {
        return LIVE_NONE.to_string();
    }
    ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
}

/// As contas com cliente aberto, na ordem da lista do app, e quais estão
/// marcadas. `only_selected` = só as que vão para a janela.
fn live_accounts_list(app: &tauri::AppHandle, only_selected: bool) -> Vec<LiveAccount> {
    #[cfg(target_os = "windows")]
    {
        let settings = app.state::<SettingsStore>();
        let selection = live_selection(&settings.get_string("Live", "Accounts"));
        let show_names = settings.get_string("Live", "ShowNames") != "false";
        let hidden = settings.get_bool("General", "HideUsernames");
        let letters = settings.get_int("General", "HiddenNameLetters").unwrap_or(0);
        let open: Vec<i64> = platform::windows::tracker().get_all().into_iter().map(|p| p.user_id).collect();
        let views = CLIENT_HEALTH_VIEWS.lock().map(|v| v.clone()).unwrap_or_default();
        return app
            .state::<AccountStore>()
            .get_all()
            .unwrap_or_default()
            .into_iter()
            .filter(|a| open.contains(&a.user_id))
            .map(|a| {
                let selected = selection.as_ref().is_none_or(|ids| ids.contains(&a.user_id));
                LiveAccount {
                    user_id: a.user_id,
                    label: if show_names { window_account_label(&a.alias, &a.username, hidden, letters) } else { String::new() },
                    in_game: views.get(&a.user_id).is_some_and(|v| v.in_game),
                    selected,
                }
            })
            .filter(|a| !only_selected || a.selected)
            .collect();
    }
    #[allow(unreachable_code)]
    {
        let _ = (app, only_selected);
        Vec::new()
    }
}

/// Página Live: todas as contas com cliente aberto e quais estão marcadas.
#[tauri::command]
fn live_accounts(app: tauri::AppHandle) -> Vec<LiveAccount> {
    live_accounts_list(&app, false)
}

/// Janela da Live: só as marcadas.
#[tauri::command]
fn live_tiles(app: tauri::AppHandle) -> Vec<LiveAccount> {
    live_accounts_list(&app, true)
}

/// Marca quais contas aparecem (`all` = todas, inclusive as que abrirem depois).
#[tauri::command]
fn set_live_accounts(app: tauri::AppHandle, user_ids: Vec<i64>, all: Option<bool>) -> Result<(), String> {
    app.state::<SettingsStore>().set("Live", "Accounts", &live_selection_text(&user_ids, all.unwrap_or(false)))?;
    let _ = app.emit("live-changed", ());
    Ok(())
}

/// A janela da Live pergunta a cada passada (2 s): conta que a pessoa
/// minimizou **depois** de a Live abrir também volta a desenhar, invisível.
/// Antes só a que já estava minimizada quando a grade foi montada (relato do
/// Discord, 30/10/2026: "i had them minimized and they stopped loading").
#[tauri::command]
fn live_keep_minimized(app: tauri::AppHandle) -> usize {
    #[cfg(target_os = "windows")]
    {
        if app.get_webview_window(LIVE_WINDOW).is_none()
            || app.state::<SettingsStore>().get_string("Live", "KeepMinimized") == "false"
        {
            return 0;
        }
        let tracker = platform::windows::tracker();
        let mut kept = 0;
        for account in live_accounts_list(&app, true) {
            let Some(pid) = tracker.get_pid(account.user_id) else { continue };
            let Some(hwnd) = platform::windows::find_main_window(pid) else { continue };
            if platform::windows::live_keep_rendering(account.user_id, hwnd as isize) {
                kept += 1;
            }
        }
        return kept;
    }
    #[allow(unreachable_code)]
    {
        let _ = app;
        0
    }
}

/// A janela da Live mandou a grade: põe (ou move) a miniatura de cada conta na
/// célula dela. Devolve quantas estão de pé.
#[tauri::command]
fn live_set_layout(app: tauri::AppHandle, cells: Vec<LiveCell>) -> Result<usize, String> {
    #[cfg(target_os = "windows")]
    {
        let window = app.get_webview_window(LIVE_WINDOW).ok_or("The Live window is not open.")?;
        let host = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
        let keep_minimized = app.state::<SettingsStore>().get_string("Live", "KeepMinimized") != "false";
        let tracker = platform::windows::tracker();
        let mut tiles: Vec<(i64, isize, (i32, i32, i32, i32))> = Vec::new();
        for cell in cells {
            let Some(pid) = tracker.get_pid(cell.user_id) else { continue };
            let Some(hwnd) = platform::windows::find_main_window(pid) else { continue };
            let hwnd = hwnd as isize;
            if keep_minimized {
                platform::windows::live_keep_rendering(cell.user_id, hwnd);
            }
            tiles.push((cell.user_id, hwnd, (cell.x, cell.y, cell.width, cell.height)));
        }
        return Ok(platform::windows::live_show_tiles(host, &tiles));
    }
    #[allow(unreachable_code)]
    {
        let _ = (app, cells);
        Ok(0)
    }
}

/// Fechou a Live (ou o app): desfaz as miniaturas e devolve as janelas que ela
/// tirou do minimizado.
fn live_cleanup() {
    #[cfg(target_os = "windows")]
    {
        platform::windows::live_clear_tiles();
        platform::windows::live_release_hidden();
    }
}

/// Abre a janela da Live (ou a traz para a frente, se já está aberta).
#[tauri::command]
async fn open_live_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(LIVE_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    let window = tauri::WebviewWindowBuilder::new(&app, LIVE_WINDOW, tauri::WebviewUrl::App("index.html?view=live".into()))
        .title("MultiAlt Live")
        .inner_size(1280.0, 720.0)
        .min_inner_size(480.0, 270.0)
        .build()
        .map_err(|e| e.to_string())?;
    window.on_window_event(|event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            live_cleanup();
        }
    });
    let _ = app.emit("live-changed", ());
    Ok(())
}

#[tauri::command]
fn close_live_window(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(LIVE_WINDOW) {
        let _ = window.close();
    }
    live_cleanup();
}

#[tauri::command]
fn live_window_open(app: tauri::AppHandle) -> bool {
    app.get_webview_window(LIVE_WINDOW).is_some()
}

/// Clicou numa conta da Live para jogar: ela volta a aparecer (se a Live a
/// deixou invisível) e vem para a frente.
#[tauri::command]
fn live_focus_account(user_id: i64) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    platform::windows::live_unhide(user_id);
    focus_roblox_window(user_id)
}

#[cfg(test)]
mod live_view_command_tests {
    use super::*;

    #[test]
    fn the_selection_is_a_comma_list_and_empty_means_every_open_account() {
        assert_eq!(live_selection(""), None);
        assert_eq!(live_selection(" 12, 7 ,x,0"), Some(vec![12, 7]));
        assert_eq!(live_selection_text(&[12, 7], false), "12,7");
        assert_eq!(live_selection(&live_selection_text(&[], true)), None, "every open account");
        // Desmarcou a última: nenhuma, não "todas".
        assert_eq!(live_selection(&live_selection_text(&[], false)), Some(vec![]));
    }
}
