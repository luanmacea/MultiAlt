use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::data::accounts::{Account, AccountStore};
use crate::data::settings::SettingsStore;

use super::cdp::{persistent_cookie_expiry, spawn_chrome, CdpClient};
use super::download::{ensure_chromium, is_installed};
use super::manager::{ChromiumManager, LOGIN_KEY};

const ROBLOX_LOGIN_URL: &str = "https://www.roblox.com/login";
const ROBLOX_HOME_URL: &str = "https://www.roblox.com/home";

/// Quanto esperamos o browser de setup sair sozinho antes de matá-lo.
const BROWSER_EXIT_TIMEOUT: Duration = Duration::from_secs(10);

/// Strips the `.ROBLOSECURITY=` prefix, surrounding quotes and any cookie
/// attributes from a raw cookie string.
///
/// A ordem importa: o espaço em volta sai **antes** das aspas, senão
/// `.ROBLOSECURITY="token" ; Path=/` devolve `token"` e o cookie vai para o
/// Roblox com uma aspa grudada.
fn normalize_security_token(raw: &str) -> String {
    let trimmed = raw.trim();
    let no_name = trimmed.strip_prefix(".ROBLOSECURITY=").unwrap_or(trimmed);
    let no_attrs = no_name.split(';').next().unwrap_or(no_name);
    no_attrs.trim().trim_matches('"').trim().to_string()
}

/// Segundos desde a época, para a validade do cookie plantado.
fn unix_now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Página em que o browser de login abre.
///
/// Com perfil persistente é preciso apagar o cookie antigo antes de chegar ao
/// Roblox, e em stealth é preciso injetar o script antes do primeiro
/// documento; nos dois casos começamos em branco e navegamos via CDP.
fn login_start_url(persistent: bool, stealth: bool) -> &'static str {
    if persistent || stealth {
        "about:blank"
    } else {
        ROBLOX_LOGIN_URL
    }
}

/// Espera o processo sair sozinho; `true` se saiu dentro do limite.
async fn wait_for_exit(child: &mut std::process::Child, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Derruba um processo que ainda não está sob o [`ChromiumManager`].
fn kill_untracked(mut child: std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[derive(Serialize)]
pub struct ImportedAccount {
    pub user_id: i64,
    pub name: String,
}

#[tauri::command]
pub fn is_browser_ready(app: AppHandle) -> bool {
    is_installed(&app)
}

#[tauri::command]
pub async fn ensure_browser(app: AppHandle) -> Result<(), String> {
    ensure_chromium(&app).await.map(|_| ())
}

#[tauri::command]
pub async fn open_account_browser(
    app: AppHandle,
    state: State<'_, AccountStore>,
    chromium: State<'_, ChromiumManager>,
    settings: State<'_, SettingsStore>,
    user_id: i64,
) -> Result<(), String> {
    let token = {
        let accounts = state.get_all()?;
        let account = accounts
            .iter()
            .find(|a| a.user_id == user_id)
            .ok_or("Account not found")?;
        normalize_security_token(&account.security_token)
    };
    if token.is_empty() {
        return Err("Account has no .ROBLOSECURITY token".into());
    }

    let stealth = settings.get_bool("Login", "StealthMode");
    let binary = ensure_chromium(&app).await?;
    let profile = ChromiumManager::account_profile(&app, user_id)?;

    // Duas instâncias no mesmo perfil brigam pelo lock do Chromium: a janela
    // antiga desta conta tem que sair antes de plantarmos o cookie de novo.
    chromium.kill(user_id);

    // Fase 1 — browser de setup. É o único momento em que a porta de debug
    // existe: ele fica em `about:blank`, sem nenhuma página do Roblox aberta,
    // e vive só o tempo de gravar o cookie no perfil.
    let (mut setup, port) = spawn_chrome(&binary, &profile, "about:blank", true, stealth).await?;

    let Some(port) = port else {
        // Sem porta não há como plantar o cookie — e um browser de debug sem
        // ninguém do nosso lado é exatamente a exposição que queremos evitar.
        kill_untracked(setup);
        return Err("Could not attach to the browser".into());
    };

    let mut cdp = match CdpClient::connect(port).await {
        Ok(cdp) => cdp,
        Err(e) => {
            kill_untracked(setup);
            return Err(e);
        }
    };

    if let Err(e) = plant_account_cookie(&mut cdp, &token, stealth).await {
        let _ = cdp.close_browser().await;
        kill_untracked(setup);
        return Err(e);
    }

    // Fecha o browser de setup graciosamente (é o shutdown do Chromium que
    // grava o cookie persistente no perfil) e, com ele, a porta de debug.
    let _ = cdp.close_browser().await;
    drop(cdp);
    if !wait_for_exit(&mut setup, BROWSER_EXIT_TIMEOUT).await {
        let _ = setup.kill();
    }
    let _ = setup.wait();

    // Fase 2 — a janela que o usuário realmente usa, já autenticada pelo
    // cookie do perfil e **sem** porta de debug. Sem CDP anexado o
    // `navigator.webdriver` já nasce indefinido, então o stealth continua
    // valendo sem precisarmos injetar nada.
    let (child, _) = spawn_chrome(&binary, &profile, ROBLOX_HOME_URL, false, stealth).await?;
    chromium.track(user_id, child);
    Ok(())
}

/// Grava o cookie da conta no perfil, com validade, para que ele sobreviva ao
/// restart entre a fase 1 e a fase 2.
async fn plant_account_cookie(
    cdp: &mut CdpClient,
    token: &str,
    stealth: bool,
) -> Result<(), String> {
    if stealth {
        let _ = cdp.inject_stealth().await;
    }
    let expires = Some(persistent_cookie_expiry(unix_now_secs()));
    cdp.set_roblosecurity(token, ".roblox.com", expires).await?;
    cdp.set_roblosecurity(token, "www.roblox.com", expires).await?;
    Ok(())
}

pub(super) fn wipe_profile_dir(profile: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(profile) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!(
            "Could not clear login profile at {}: {}",
            profile.display(),
            e
        )),
    }
}

async fn setup_login_session(
    cdp: &mut CdpClient,
    stealth: bool,
    persistent: bool,
    start_url: &str,
) -> Result<(), String> {
    if stealth {
        let _ = cdp.inject_stealth().await;
    }
    if persistent {
        cdp.delete_roblosecurity(".roblox.com").await?;
        cdp.delete_roblosecurity("www.roblox.com").await?;
    }
    if start_url != ROBLOX_LOGIN_URL {
        cdp.navigate(ROBLOX_LOGIN_URL).await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn open_login_browser(
    app: AppHandle,
    chromium: State<'_, ChromiumManager>,
    settings: State<'_, SettingsStore>,
) -> Result<(), String> {
    let binary = ensure_chromium(&app).await?;
    chromium.close_login_session();

    let persistent = settings.get_bool("Login", "PersistentProfile");
    let stealth = settings.get_bool("Login", "StealthMode");

    let profile = ChromiumManager::login_profile(&app)?;
    if !persistent {
        wipe_profile_dir(&profile)?;
    }

    let start_url = login_start_url(persistent, stealth);
    let (child, port) = spawn_chrome(&binary, &profile, start_url, true, stealth).await?;
    chromium.track(LOGIN_KEY, child);

    let port = port.ok_or("Could not start the login browser")?;

    let mut cdp = match CdpClient::connect(port).await {
        Ok(cdp) => cdp,
        Err(e) => {
            chromium.close_login_session();
            return Err(e);
        }
    };
    if let Err(e) = setup_login_session(&mut cdp, stealth, persistent, start_url).await {
        chromium.close_login_session();
        return Err(e);
    }

    let app_task = app.clone();
    tauri::async_runtime::spawn(async move {
        let chromium = app_task.state::<ChromiumManager>();
        for _ in 0..480 {
            if !chromium.is_alive(LOGIN_KEY) {
                return;
            }
            if let Ok(Some(cookie)) = cdp.get_roblosecurity().await {
                chromium.set_login_cookie(Some(cookie));
                // A janela (e com ela a porta de debug) fecha no instante em
                // que o cookie aparece: é o único ponto do fluxo em que uma
                // sessão autenticada e um endpoint CDP aberto coexistem.
                chromium.close_login_window();
                if !persistent {
                    if let Ok(profile) = ChromiumManager::login_profile(&app_task) {
                        let _ = wipe_profile_dir(&profile);
                    }
                }
                let _ = app_task.emit("browser-login-detected", ());
                return;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn extract_browser_cookie(chromium: State<'_, ChromiumManager>) -> Result<String, String> {
    chromium
        .login_cookie()
        .filter(|c| !c.trim().is_empty())
        .ok_or_else(|| "No .ROBLOSECURITY cookie found. Make sure you completed the login.".into())
}

#[tauri::command]
pub async fn close_login_browser(
    app: AppHandle,
    chromium: State<'_, ChromiumManager>,
    settings: State<'_, SettingsStore>,
) -> Result<(), String> {
    chromium.close_login_session();
    if !settings.get_bool("Login", "PersistentProfile") {
        if let Ok(profile) = ChromiumManager::login_profile(&app) {
            let _ = wipe_profile_dir(&profile);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn import_userpass(
    app: AppHandle,
    state: State<'_, AccountStore>,
    chromium: State<'_, ChromiumManager>,
    settings: State<'_, SettingsStore>,
    username: String,
    password: String,
) -> Result<ImportedAccount, String> {
    if username.trim().is_empty() || password.is_empty() {
        return Err("Enter a username and password".into());
    }

    let binary = ensure_chromium(&app).await?;
    chromium.close_login_session();

    let persistent = settings.get_bool("Login", "PersistentProfile");
    let stealth = settings.get_bool("Login", "StealthMode");

    let profile = ChromiumManager::login_profile(&app)?;
    if !persistent {
        wipe_profile_dir(&profile)?;
    }

    let start_url = login_start_url(persistent, stealth);
    let (child, port) = spawn_chrome(&binary, &profile, start_url, true, stealth).await?;
    chromium.track(LOGIN_KEY, child);

    let port = port.ok_or("Could not start the login browser")?;
    let mut cdp = match CdpClient::connect(port).await {
        Ok(cdp) => cdp,
        Err(e) => {
            chromium.close_login_session();
            if !persistent {
                if let Err(cleanup) = wipe_profile_dir(&profile) {
                    return Err(format!("{}; {}", e, cleanup));
                }
            }
            return Err(e);
        }
    };

    if let Err(e) = setup_login_session(&mut cdp, stealth, persistent, start_url).await {
        chromium.close_login_session();
        if !persistent {
            if let Err(cleanup) = wipe_profile_dir(&profile) {
                return Err(format!("{}; {}", e, cleanup));
            }
        }
        return Err(e);
    }

    if cdp.wait_for_selector("#login-username", 40).await {
        let _ = cdp.fill_login(&username, &password).await;
    }

    let mut captured = None;
    for _ in 0..480 {
        if !chromium.is_alive(LOGIN_KEY) {
            return Err("Login window was closed before sign-in completed".into());
        }
        if let Ok(Some(cookie)) = cdp.get_roblosecurity().await {
            captured = Some(cookie);
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    chromium.close_login_session();
    if !persistent {
        let _ = wipe_profile_dir(&profile);
    }

    let cookie = captured.ok_or("Timed out waiting for sign-in")?;
    let info = crate::api::auth::validate_cookie(&cookie).await?;

    let mut account = Account::new(cookie, info.name.clone(), info.user_id);
    account.password = password;
    state.add(account)?;

    Ok(ImportedAccount {
        user_id: info.user_id,
        name: info.name,
    })
}
