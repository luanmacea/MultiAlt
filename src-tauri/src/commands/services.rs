const WEBSERVER_DISABLED_ERR: &str = "Web server is disabled in this build";
const NEXUS_DISABLED_ERR: &str = "Nexus is disabled in this build";

#[derive(Debug, Clone, serde::Serialize)]
struct WebServerStatusResponse {
    running: bool,
    port: u16,
}

#[cfg(feature = "nexus")]
type NexusStatusResponse = nexus::websocket::NexusStatus;

#[cfg(feature = "nexus")]
type NexusAccountViewResponse = nexus::websocket::AccountView;

#[cfg(feature = "nexus")]
type NexusElementResponse = nexus::websocket::CustomElement;

#[cfg(not(feature = "nexus"))]
#[derive(Debug, Clone, serde::Serialize)]
struct NexusStatusResponse {
    running: bool,
    port: Option<u16>,
    connected_count: usize,
}

#[cfg(not(feature = "nexus"))]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct NexusAccountViewResponse {
    username: String,
    auto_execute: String,
    place_id: i64,
    job_id: String,
    relaunch_delay: f64,
    auto_relaunch: bool,
    is_checked: bool,
    status: String,
    in_game_job_id: String,
}

#[cfg(not(feature = "nexus"))]
#[derive(Debug, Clone, serde::Serialize)]
struct NexusElementResponse {
    name: String,
    element_type: String,
    content: String,
    size: Option<(i32, i32)>,
    margin: Option<(i32, i32, i32, i32)>,
    decimal_places: Option<i32>,
    increment: Option<String>,
    value: String,
    is_newline: bool,
}

#[cfg(feature = "webserver")]
#[tauri::command]
async fn start_web_server(app: tauri::AppHandle) -> Result<u16, String> {
    let accounts: &'static AccountStore =
        unsafe { &*(app.state::<AccountStore>().inner() as *const AccountStore) };
    let settings: &'static SettingsStore =
        unsafe { &*(app.state::<SettingsStore>().inner() as *const SettingsStore) };
    api::server::start(accounts, settings).await
}

#[cfg(not(feature = "webserver"))]
#[tauri::command]
async fn start_web_server(_app: tauri::AppHandle) -> Result<u16, String> {
    Err(WEBSERVER_DISABLED_ERR.into())
}

#[cfg(feature = "webserver")]
#[tauri::command]
fn stop_web_server() -> Result<(), String> {
    api::server::stop()
}

#[cfg(not(feature = "webserver"))]
#[tauri::command]
fn stop_web_server() -> Result<(), String> {
    Err(WEBSERVER_DISABLED_ERR.into())
}

#[cfg(feature = "webserver")]
#[tauri::command]
fn get_web_server_status() -> Result<WebServerStatusResponse, String> {
    Ok(WebServerStatusResponse {
        running: api::server::is_running(),
        port: api::server::get_port(),
    })
}

#[cfg(not(feature = "webserver"))]
#[tauri::command]
fn get_web_server_status() -> Result<WebServerStatusResponse, String> {
    Ok(WebServerStatusResponse {
        running: false,
        port: 0,
    })
}

#[cfg(feature = "nexus")]
#[tauri::command]
async fn start_nexus_server(
    app: tauri::AppHandle,
    settings: tauri::State<'_, SettingsStore>,
) -> Result<u16, String> {
    let port = settings
        .get_int("AccountControl", "NexusPort")
        .unwrap_or(5242) as u16;
    let allow_external = settings.get_bool("AccountControl", "AllowExternalConnections");
    nexus::websocket::nexus()
        .start(port, allow_external, app)
        .await
}

#[cfg(not(feature = "nexus"))]
#[tauri::command]
async fn start_nexus_server(
    _app: tauri::AppHandle,
    _settings: tauri::State<'_, SettingsStore>,
) -> Result<u16, String> {
    Err(NEXUS_DISABLED_ERR.into())
}

#[cfg(feature = "nexus")]
#[tauri::command]
fn stop_nexus_server() -> Result<(), String> {
    nexus::websocket::nexus().stop();
    Ok(())
}

#[cfg(not(feature = "nexus"))]
#[tauri::command]
fn stop_nexus_server() -> Result<(), String> {
    Err(NEXUS_DISABLED_ERR.into())
}

#[cfg(feature = "nexus")]
#[tauri::command]
fn get_nexus_status() -> Result<NexusStatusResponse, String> {
    Ok(nexus::websocket::nexus().get_status())
}

#[cfg(not(feature = "nexus"))]
#[tauri::command]
fn get_nexus_status() -> Result<NexusStatusResponse, String> {
    Ok(NexusStatusResponse {
        running: false,
        port: None,
        connected_count: 0,
    })
}

#[cfg(feature = "nexus")]
#[tauri::command]
fn get_nexus_accounts() -> Result<Vec<NexusAccountViewResponse>, String> {
    Ok(nexus::websocket::nexus().get_accounts())
}

#[cfg(not(feature = "nexus"))]
#[tauri::command]
fn get_nexus_accounts() -> Result<Vec<NexusAccountViewResponse>, String> {
    Ok(Vec::new())
}

#[cfg(feature = "nexus")]
#[tauri::command]
fn add_nexus_account(username: String) -> Result<(), String> {
    nexus::websocket::nexus().add_account(&username)
}

#[cfg(not(feature = "nexus"))]
#[tauri::command]
fn add_nexus_account(_username: String) -> Result<(), String> {
    Err(NEXUS_DISABLED_ERR.into())
}

#[cfg(feature = "nexus")]
#[tauri::command]
fn remove_nexus_accounts(usernames: Vec<String>) -> Result<(), String> {
    nexus::websocket::nexus().remove_accounts(&usernames);
    Ok(())
}

#[cfg(not(feature = "nexus"))]
#[tauri::command]
fn remove_nexus_accounts(_usernames: Vec<String>) -> Result<(), String> {
    Err(NEXUS_DISABLED_ERR.into())
}

#[cfg(feature = "nexus")]
#[tauri::command]
fn update_nexus_account(account: NexusAccountViewResponse) -> Result<(), String> {
    nexus::websocket::nexus().update_account(account);
    Ok(())
}

#[cfg(not(feature = "nexus"))]
#[tauri::command]
fn update_nexus_account(_account: NexusAccountViewResponse) -> Result<(), String> {
    Err(NEXUS_DISABLED_ERR.into())
}

#[cfg(feature = "nexus")]
#[tauri::command]
fn nexus_send_command(message: String) -> Result<(), String> {
    nexus::websocket::nexus().send_command(&message);
    Ok(())
}

#[cfg(not(feature = "nexus"))]
#[tauri::command]
fn nexus_send_command(_message: String) -> Result<(), String> {
    Err(NEXUS_DISABLED_ERR.into())
}

#[cfg(feature = "nexus")]
#[tauri::command]
fn nexus_send_to_all(message: String) -> Result<(), String> {
    nexus::websocket::nexus().send_to_all(&message);
    Ok(())
}

#[cfg(not(feature = "nexus"))]
#[tauri::command]
fn nexus_send_to_all(_message: String) -> Result<(), String> {
    Err(NEXUS_DISABLED_ERR.into())
}

#[cfg(feature = "nexus")]
#[tauri::command]
fn get_nexus_log() -> Result<Vec<String>, String> {
    Ok(nexus::websocket::nexus().get_log())
}

#[cfg(not(feature = "nexus"))]
#[tauri::command]
fn get_nexus_log() -> Result<Vec<String>, String> {
    Ok(Vec::new())
}

#[cfg(feature = "nexus")]
#[tauri::command]
fn clear_nexus_log() -> Result<(), String> {
    nexus::websocket::nexus().clear_log();
    Ok(())
}

#[cfg(not(feature = "nexus"))]
#[tauri::command]
fn clear_nexus_log() -> Result<(), String> {
    Err(NEXUS_DISABLED_ERR.into())
}

#[cfg(feature = "nexus")]
#[tauri::command]
fn get_nexus_elements() -> Result<Vec<NexusElementResponse>, String> {
    Ok(nexus::websocket::nexus().get_elements())
}

#[cfg(not(feature = "nexus"))]
#[tauri::command]
fn get_nexus_elements() -> Result<Vec<NexusElementResponse>, String> {
    Ok(Vec::new())
}

#[cfg(feature = "nexus")]
#[tauri::command]
fn set_nexus_element_value(name: String, value: String) -> Result<(), String> {
    nexus::websocket::nexus().set_element_value(&name, &value);
    Ok(())
}

#[cfg(not(feature = "nexus"))]
#[tauri::command]
fn set_nexus_element_value(_name: String, _value: String) -> Result<(), String> {
    Err(NEXUS_DISABLED_ERR.into())
}

#[cfg(feature = "nexus")]
#[tauri::command]
fn export_nexus_lua() -> Result<String, String> {
    let out_path = std::env::current_dir()
        .map_err(|e| format!("Failed to read current directory: {}", e))?
        .join("Nexus.lua");
    let content = include_str!("../../assets/Nexus.lua");
    std::fs::write(&out_path, content).map_err(|e| format!("Failed to write Nexus.lua: {}", e))?;
    Ok(out_path.to_string_lossy().into_owned())
}

#[cfg(not(feature = "nexus"))]
#[tauri::command]
fn export_nexus_lua() -> Result<String, String> {
    Err(NEXUS_DISABLED_ERR.into())
}

#[tauri::command]
fn open_repo_url() -> Result<(), String> {
    let url = "https://github.com/niccsprojects/Roblox-Account-Manager";

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err("Opening URL is not supported on this platform".into())
}

#[tauri::command]
fn sync_windows_navbar_theme(
    app: tauri::AppHandle,
    settings: tauri::State<'_, SettingsStore>,
    theme_store: tauri::State<'_, ThemeStore>,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let enable_theme_sync = settings.get_bool("General", "ThemeWindowsNavbar");
        let target_theme = if enable_theme_sync {
            let theme = theme_store.get()?;
            Some(if theme.dark_top_bar {
                tauri::Theme::Dark
            } else {
                tauri::Theme::Light
            })
        } else {
            None
        };

        for window in app.webview_windows().values() {
            let _ = window.set_theme(target_theme.clone());
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        let _ = settings;
        let _ = theme_store;
    }

    Ok(())
}
