pub fn build_launch_url(
    ticket: &str,
    place_id: i64,
    job_id: &str,
    browser_tracker_id: &str,
    launch_data: &str,
    follow_user: bool,
    join_vip: bool,
    access_code: &str,
    link_code: &str,
    is_teleport: bool,
) -> String {
    let launch_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let ld_param = if launch_data.is_empty() {
        String::new()
    } else {
        format!("&launchData={}", urlencoding::encode(launch_data))
    };

    let place_launcher_url = if join_vip {
        let access_param = if access_code.is_empty() {
            String::new()
        } else {
            format!("&accessCode={}", urlencoding::encode(access_code))
        };
        let link_param = if link_code.is_empty() {
            String::new()
        } else {
            format!("&linkCode={}", urlencoding::encode(link_code))
        };
        format!(
            "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request=RequestPrivateGame&placeId={}{}{}{}",
            place_id, access_param, link_param, ld_param
        )
    } else if follow_user {
        format!(
            "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request=RequestFollowUser&userId={}{}",
            place_id, ld_param
        )
    } else {
        let req = if job_id.is_empty() {
            "RequestGame"
        } else {
            "RequestGameJob"
        };
        let gid = if job_id.is_empty() {
            String::new()
        } else {
            format!("&gameId={}", job_id)
        };
        let tp = if is_teleport { "&isTeleport=true" } else { "" };
        format!(
            "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request={}&browserTrackerId={}&placeId={}{}&isPlayTogetherGame=false{}{}",
            req, browser_tracker_id, place_id, gid, tp, ld_param
        )
    };

    format!(
        "roblox-player:1+launchmode:play+gameinfo:{}+launchtime:{}+placelauncherurl:{}+browsertrackerid:{}+robloxLocale:en_us+gameLocale:en_us+channel:+LaunchExp:InApp",
        ticket,
        launch_time,
        urlencoding::encode(&place_launcher_url),
        browser_tracker_id
    )
}

/// Registry key where the Roblox client reads its deployment channel at startup
/// (`www.roblox.com` value). Confirmed via client logs: "RobloxChannel has been
/// set to <value>" followed by a version query against that channel.
const ROBLOX_PLAYER_CHANNEL_KEY: &str =
    "Software\\ROBLOX Corporation\\Environments\\RobloxPlayer\\Channel";
const PRODUCTION_CHANNEL: &str = "production";
/// The CDN uses "LIVE" for what the client calls the production channel.
const PRODUCTION_CHANNEL_CDN: &str = "LIVE";
const PRODUCTION_VERSION_URL: &str =
    "https://clientsettingscdn.roblox.com/v2/client-version/WindowsPlayer";
const PRODUCTION_VERSION_CACHE_TTL: Duration = Duration::from_secs(60);
/// Teto da consulta "qual build este canal exige". Fica **abaixo** do teto das
/// chamadas de API de propósito: esta consulta tem fallback (o cache vencido, ou
/// o canal de produção), então esperar mais tempo só atrasaria o launch sem
/// mudar o resultado. Era o valor que este ponto já praticava.
const CHANNEL_LOOKUP_TIMEOUT: Duration = Duration::from_secs(6);

/// Cache of "channel -> (fetched at, build)".
static CHANNEL_VERSION_CACHE: LazyLock<Mutex<HashMap<String, (std::time::Instant, String)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// Build resolved by the last lookup, for sync callers (`get_roblox_path`).
static LAST_RESOLVED_BUILD: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

/// Why launches used to "update Roblox" and close the other clients:
///
/// Roblox enrolls each *account* in a deployment channel (e.g. `ztestlinkerset`,
/// `zswocc-500-c`) that may point at a different client build than production.
/// After an account joins a game, its client spawns a background installer
/// (`-channel <account channel>`) that installs that build and rewrites both the
/// `roblox-player:` protocol handler and the channel stored in the registry.
/// The next `roblox-player:` launch then starts a client whose build does not
/// match the channel it reads, which reports `updateRequired TRUE` and runs the
/// foreground installer — and that installer closes every running
/// `RobloxPlayerBeta.exe`, killing the other accounts.
///
/// **A build aberta tem que casar com o `channel:` que vai dentro da URL.**
/// Provado nos logs do cliente (24/09/2026): com o registro em
/// `ztestlinkerset` e a URL com `channel:` vazio, o cliente consultou o
/// endpoint de *produção* (`versionQueryUrl: .../client-version/WindowsPlayer`,
/// `channel: ""`) e pediu update — o campo da URL **vence o registro**. A mesma
/// URL com a build de produção dá `updateRequired FALSE`.
///
/// Como `build_launch_url` sempre emite `channel:` vazio (igual ao site e ao
/// launcher oficial), aqui a build é sempre a de **produção**, instalada em
/// silêncio quando faltar. O registro não é lido nem escrito neste caminho —
/// quem joga pelo site continua com o que o Roblox configurou.
///
/// O handler do protocolo só entra como último recurso, se o download falhar.
pub async fn launch_url(url: &str) -> Result<(), String> {
    if let Some(player_exe) = ensure_player_exe_for_channel(PRODUCTION_CHANNEL).await {
        std::process::Command::new(&player_exe)
            .arg(url)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to launch {}: {}", player_exe.display(), e))?;
        return Ok(());
    }

    std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("Failed to launch: {}", e))?;
    Ok(())
}

/// Pasta usada pelo *old join* (`RobloxPlayerBeta.exe --app -t -j`), que não
/// passa URL nenhuma: sem `channel:` na linha de comando o cliente cai no canal
/// do **registro**, então aqui a build tem que ser a daquele canal — o oposto
/// do `launch_url`. Instala se faltar; em último caso devolve `fallback`.
pub async fn default_player_dir(fallback: &str) -> String {
    ensure_player_exe_for_channel(&current_player_channel())
        .await
        .and_then(|exe| exe.parent().map(|p| p.to_string_lossy().into_owned()))
        .unwrap_or_else(|| fallback.to_string())
}

/// Deployment channel the Roblox client will use, read from the registry value
/// that Roblox itself maintains. Empty/missing means production.
fn current_player_channel() -> String {
    use windows_sys::Win32::System::Registry::HKEY_CURRENT_USER;

    let value = read_string_value(
        HKEY_CURRENT_USER as isize,
        ROBLOX_PLAYER_CHANNEL_KEY,
        "www.roblox.com",
    )
    .unwrap_or_default();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        PRODUCTION_CHANNEL.to_string()
    } else {
        trimmed.to_string()
    }
}

fn channel_is_production(channel: &str) -> bool {
    channel.trim().is_empty()
        || channel.eq_ignore_ascii_case(PRODUCTION_CHANNEL)
        || channel.eq_ignore_ascii_case("live")
}

/// clientsettings endpoint for a channel. The literal "production" has no
/// `/channel/` route (it answers 401), so it uses the default URL.
fn channel_version_url(channel: &str) -> String {
    if channel_is_production(channel) {
        PRODUCTION_VERSION_URL.to_string()
    } else {
        format!(
            "{}/channel/{}",
            PRODUCTION_VERSION_URL,
            channel.to_ascii_lowercase()
        )
    }
}

/// Channel name understood by the setup CDN (`install_build_to_dir`), which
/// calls production "LIVE" and serves test-channel builds from
/// `/channel/common/` (its own fallback handles that).
fn channel_for_cdn(channel: &str) -> String {
    if channel_is_production(channel) {
        PRODUCTION_CHANNEL_CDN.to_string()
    } else {
        channel.to_string()
    }
}

/// Writes the channel Roblox should use. Only called to repair an inconsistent
/// state (a channel whose version endpoint no longer answers): without it we
/// would start a production build while the client still reads the dead
/// channel, which is exactly the mismatch that makes Roblox's installer run.
fn set_player_channel(channel: &str) {
    use windows_sys::Win32::System::Registry::{
        RegCreateKeyExW, RegSetValueExW, HKEY_CURRENT_USER, KEY_SET_VALUE,
        REG_OPTION_NON_VOLATILE,
    };

    let key_name = encode_wide(ROBLOX_PLAYER_CHANNEL_KEY);
    let value_name = encode_wide("www.roblox.com");
    let value = encode_wide(channel);
    unsafe {
        let mut hkey: windows_sys::Win32::System::Registry::HKEY = std::ptr::null_mut();
        let created = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            key_name.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut hkey,
            std::ptr::null_mut(),
        );
        if created != 0 {
            return;
        }
        RegSetValueExW(
            hkey,
            value_name.as_ptr(),
            0,
            REG_SZ,
            value.as_ptr() as *const u8,
            (value.len() * 2) as u32,
        );
        RegCloseKey(hkey);
    }
}

fn remember_resolved_build(version: &str) {
    if let Ok(mut guard) = LAST_RESOLVED_BUILD.lock() {
        *guard = Some(version.to_string());
    }
}

/// `%LOCALAPPDATA%\Roblox\Versions\<build>\RobloxPlayerBeta.exe` da build
/// daquele canal, se estiver instalada.
async fn player_exe_for_channel(channel: &str) -> Option<PathBuf> {
    let version = build_version_for_channel(channel).await?;
    installed_player_exe(&version)
}

fn installed_player_exe(version: &str) -> Option<PathBuf> {
    let exe = roblox_versions_dir()?
        .join(version)
        .join("RobloxPlayerBeta.exe");
    exe.exists().then_some(exe)
}

/// Progress of a launcher-initiated production build install, mirrored to the
/// frontend as the `roblox-build-install` event.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildInstallProgress {
    pub version: String,
    pub stage: String,
    pub current: u64,
    pub total: u64,
    pub message: Option<String>,
}

static BUILD_INSTALL_APP: LazyLock<Mutex<Option<tauri::AppHandle>>> =
    LazyLock::new(|| Mutex::new(None));
/// Serializes installs so a multi-account launch downloads a new build once.
static BUILD_INSTALL_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

/// Lets the launcher report build-install progress to the UI.
pub fn set_build_install_app_handle(app: tauri::AppHandle) {
    if let Ok(mut guard) = BUILD_INSTALL_APP.lock() {
        *guard = Some(app);
    }
}

fn emit_build_install(version: &str, stage: &str, current: u64, total: u64, message: Option<String>) {
    let app = BUILD_INSTALL_APP.lock().ok().and_then(|g| g.clone());
    if let Some(app) = app {
        let _ = app.emit(
            "roblox-build-install",
            BuildInstallProgress {
                version: version.to_string(),
                stage: stage.to_string(),
                current,
                total,
                message,
            },
        );
    }
}

/// Exe da build de `channel`, instalando-a antes quando faltar (versão nova do
/// Roblox, ou canal para o qual o Roblox acabou de trocar). `None` só quando
/// não foi possível nem achar nem instalar.
async fn ensure_player_exe_for_channel(channel: &str) -> Option<PathBuf> {
    if let Some(exe) = player_exe_for_channel(channel).await {
        return Some(exe);
    }

    let channel = channel.to_string();
    let version = build_version_for_channel(&channel).await?;
    let _serialize = BUILD_INSTALL_LOCK.lock().await;
    // Another launch may have installed it while we waited for the lock.
    if let Some(exe) = installed_player_exe(&version) {
        return Some(exe);
    }

    let target = roblox_versions_dir()?.join(&version);
    emit_build_install(&version, "starting", 0, 0, None);
    let progress = |stage: &str, _package: Option<String>, current: u64, total: u64| {
        emit_build_install(&version, stage, current, total, None);
    };
    match install_build_to_dir(&channel_for_cdn(&channel), &version, &target, 4, &progress).await {
        Ok(()) => {
            emit_build_install(&version, "ready", 0, 0, None);
            installed_player_exe(&version)
        }
        Err(e) => {
            emit_build_install(&version, "error", 0, 0, Some(e.clone()));
            eprintln!("Could not install Roblox build {version} (channel {channel}): {e}");
            None
        }
    }
}

fn roblox_versions_dir() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(local).join("Roblox").join("Versions"))
}

/// Resolve (e guarda em cache) a build de **produção** — a mesma que
/// `launch_url` abre — para os helpers síncronos como `get_roblox_path`, usado
/// ao gravar o ClientSettings, apontarem para a pasta certa.
pub async fn refresh_production_version() {
    let _ = build_version_for_channel(PRODUCTION_CHANNEL).await;
}

/// Build folder resolved by the last lookup, if it is installed.
fn cached_production_player_dir() -> Option<String> {
    let version = LAST_RESOLVED_BUILD.lock().ok()?.clone()?;
    let dir = roblox_versions_dir()?.join(version);
    dir.join("RobloxPlayerBeta.exe")
        .exists()
        .then(|| dir.to_string_lossy().into_owned())
}

/// Build que o cliente vai exigir para `channel`: o que o endpoint
/// clientsettings reporta para ele.
async fn build_version_for_channel(channel: &str) -> Option<String> {
    let channel = channel.to_string();
    let cached = CHANNEL_VERSION_CACHE
        .lock()
        .ok()
        .and_then(|c| c.get(&channel).cloned());
    if let Some((at, version)) = &cached {
        if at.elapsed() < PRODUCTION_VERSION_CACHE_TTL {
            remember_resolved_build(version);
            return Some(version.clone());
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ClientVersion {
        client_version_upload: String,
    }

    let fetched = async {
        // Teto próprio e curto: esta consulta tem fallback (o cache vencido),
        // então esperar mais que isso só atrasa o launch. `connect_timeout` vem
        // do builder compartilhado; o total continua sendo os 6 s de sempre.
        let client = crate::api::http_client::builder_with(
            CHANNEL_LOOKUP_TIMEOUT,
            CHANNEL_LOOKUP_TIMEOUT,
        )
        .build()
        .ok()?;
        let mut urls = vec![(channel_version_url(&channel), false)];
        if !channel_is_production(&channel) {
            // A channel Roblox retired answers 401/404. Falling back to the
            // production build is only safe if the client reads the same
            // channel, so that fallback also repairs the registry value.
            urls.push((PRODUCTION_VERSION_URL.to_string(), true));
        }
        for (url, repair_channel) in urls {
            let Ok(resp) = client.get(&url).send().await else {
                continue;
            };
            let Ok(resp) = resp.error_for_status() else {
                continue;
            };
            let Ok(body) = resp.json::<ClientVersion>().await else {
                continue;
            };
            let version = body.client_version_upload.trim().to_string();
            if version.starts_with("version-") {
                if repair_channel {
                    set_player_channel(PRODUCTION_CHANNEL);
                }
                return Some(version);
            }
        }
        None
    }
    .await;

    match fetched {
        Some(version) => {
            if let Ok(mut cache) = CHANNEL_VERSION_CACHE.lock() {
                cache.insert(channel, (std::time::Instant::now(), version.clone()));
            }
            remember_resolved_build(&version);
            Some(version)
        }
        // Network hiccup: a stale answer is still far better than the protocol
        // handler, whose build may not match the channel.
        None => {
            let stale = cached.map(|(_, version)| version);
            if let Some(version) = &stale {
                remember_resolved_build(version);
            }
            stale
        }
    }
}

pub async fn launch_old_join(
    ticket: &str,
    place_id: i64,
    job_id: &str,
    launch_data: &str,
    follow_user: bool,
    join_vip: bool,
    access_code: &str,
    link_code: &str,
    is_teleport: bool,
) -> Result<(), String> {
    let version_folder = default_player_dir(&get_roblox_path()?).await;
    launch_old_join_from(
        &version_folder,
        ticket,
        place_id,
        job_id,
        launch_data,
        follow_user,
        join_vip,
        access_code,
        link_code,
        is_teleport,
    )
}

pub fn launch_old_join_from(
    base_path: &str,
    ticket: &str,
    place_id: i64,
    job_id: &str,
    launch_data: &str,
    follow_user: bool,
    join_vip: bool,
    access_code: &str,
    link_code: &str,
    is_teleport: bool,
) -> Result<(), String> {
    let version_folder = base_path;
    let exe = std::path::Path::new(version_folder).join("RobloxPlayerBeta.exe");
    if !exe.exists() {
        return Err("RobloxPlayerBeta.exe not found in Roblox version folder".into());
    }

    let ld_param = if launch_data.is_empty() {
        String::new()
    } else {
        format!("&launchData={}", urlencoding::encode(launch_data))
    };

    let join_url = if join_vip {
        let access_param = if access_code.is_empty() {
            String::new()
        } else {
            format!("&accessCode={}", urlencoding::encode(access_code))
        };
        let link_param = if link_code.is_empty() {
            String::new()
        } else {
            format!("&linkCode={}", urlencoding::encode(link_code))
        };
        format!(
            "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request=RequestPrivateGame&placeId={}{}{}{}",
            place_id, access_param, link_param, ld_param
        )
    } else if follow_user {
        format!(
            "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request=RequestFollowUser&userId={}{}",
            place_id, ld_param
        )
    } else {
        let req = if job_id.is_empty() {
            "RequestGame"
        } else {
            "RequestGameJob"
        };
        let gid = if job_id.is_empty() {
            String::new()
        } else {
            format!("&gameId={}", job_id)
        };
        let tp = if is_teleport { "&isTeleport=true" } else { "" };
        format!(
            "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request={}&placeId={}{}&isPlayTogetherGame=false{}{}",
            req, place_id, gid, tp, ld_param
        )
    };

    std::process::Command::new(exe)
        .arg("--app")
        .arg("-t")
        .arg(ticket)
        .arg("-j")
        .arg(join_url)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("Failed to launch old join: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod launch_url_tests {
    use super::*;

    const PLACE_LAUNCHER_PREFIX: &str = "+placelauncherurl:";
    const TRACKER_SUFFIX: &str = "+browsertrackerid:";

    /// The PlaceLauncher URL is url-encoded inside the `roblox-player:` string;
    /// decode one layer so assertions can read it back.
    fn place_launcher_url(url: &str) -> String {
        let start = url
            .find(PLACE_LAUNCHER_PREFIX)
            .expect("placelauncherurl segment")
            + PLACE_LAUNCHER_PREFIX.len();
        let rest = &url[start..];
        let end = rest.find(TRACKER_SUFFIX).expect("browsertrackerid segment");
        urlencoding::decode(&rest[..end])
            .expect("decodable placelauncherurl")
            .into_owned()
    }

    fn build(
        job_id: &str,
        launch_data: &str,
        follow_user: bool,
        join_vip: bool,
        access_code: &str,
        link_code: &str,
        is_teleport: bool,
    ) -> String {
        build_launch_url(
            "TICKET123",
            606849621,
            job_id,
            "123456789012",
            launch_data,
            follow_user,
            join_vip,
            access_code,
            link_code,
            is_teleport,
        )
    }

    // ---- channel pinning guards ---------------------------------------------

    #[test]
    fn production_channel_constants_are_intact() {
        // These pin the Roblox deployment channel to production before launch.
        // Changing them brings back the foreground installer that closes every
        // other open client (docs/features/launch.md).
        assert_eq!(PRODUCTION_CHANNEL, "production");
        assert_eq!(PRODUCTION_CHANNEL_CDN, "LIVE");
        assert_eq!(
            ROBLOX_PLAYER_CHANNEL_KEY,
            "Software\\ROBLOX Corporation\\Environments\\RobloxPlayer\\Channel"
        );
    }

    #[test]
    fn the_launch_url_still_ends_with_an_empty_channel() {
        // `+channel:` with no value tells the client to use the registry
        // channel we just pinned to production.
        let url = build("", "", false, false, "", "", false);
        assert!(
            url.ends_with("+channel:+LaunchExp:InApp"),
            "unexpected tail: {url}"
        );
    }

    #[test]
    fn the_launch_url_carries_the_ticket_and_the_tracker_id() {
        let url = build("", "", false, false, "", "", false);
        assert!(url.starts_with("roblox-player:1+launchmode:play+gameinfo:TICKET123+launchtime:"));
        assert!(url.contains("+browsertrackerid:123456789012+robloxLocale:en_us"));
    }

    // ---- public games --------------------------------------------------------

    #[test]
    fn a_job_id_produces_request_game_job_with_game_id() {
        let url = build("job-abc", "", false, false, "", "", false);
        let launcher = place_launcher_url(&url);
        assert!(launcher.contains("request=RequestGameJob"), "{launcher}");
        assert!(launcher.contains("&gameId=job-abc"), "{launcher}");
        assert!(launcher.contains("&placeId=606849621"), "{launcher}");
        assert!(launcher.contains("browserTrackerId=123456789012"), "{launcher}");
        assert!(launcher.contains("&isPlayTogetherGame=false"), "{launcher}");
    }

    #[test]
    fn an_empty_job_id_produces_request_game_without_game_id() {
        let url = build("", "", false, false, "", "", false);
        let launcher = place_launcher_url(&url);
        assert!(launcher.contains("request=RequestGame&"), "{launcher}");
        assert!(!launcher.contains("RequestGameJob"), "{launcher}");
        assert!(!launcher.contains("gameId"), "{launcher}");
    }

    #[test]
    fn is_teleport_is_only_added_when_requested() {
        let without = place_launcher_url(&build("job-abc", "", false, false, "", "", false));
        assert!(!without.contains("isTeleport"), "{without}");

        let with = place_launcher_url(&build("job-abc", "", false, false, "", "", true));
        assert!(with.contains("&isTeleport=true"), "{with}");
    }

    // ---- private servers -----------------------------------------------------

    #[test]
    fn join_vip_produces_request_private_game_with_encoded_codes() {
        let url = build("", "", false, true, "acc code/1", "link code/2", false);
        let launcher = place_launcher_url(&url);
        assert!(launcher.contains("request=RequestPrivateGame"), "{launcher}");
        assert!(launcher.contains("placeId=606849621"), "{launcher}");
        assert!(launcher.contains("&accessCode=acc%20code%2F1"), "{launcher}");
        assert!(launcher.contains("&linkCode=link%20code%2F2"), "{launcher}");
    }

    #[test]
    fn join_vip_omits_the_codes_that_are_empty() {
        let only_link = place_launcher_url(&build("", "", false, true, "", "abc123", false));
        assert!(!only_link.contains("accessCode"), "{only_link}");
        assert!(only_link.contains("&linkCode=abc123"), "{only_link}");

        let only_access = place_launcher_url(&build("", "", false, true, "a-b-c-d-e", "", false));
        assert!(only_access.contains("&accessCode=a-b-c-d-e"), "{only_access}");
        assert!(!only_access.contains("linkCode"), "{only_access}");
    }

    #[test]
    fn join_vip_wins_over_follow_user() {
        let launcher = place_launcher_url(&build("", "", true, true, "", "abc123", false));
        assert!(launcher.contains("request=RequestPrivateGame"), "{launcher}");
        assert!(!launcher.contains("RequestFollowUser"), "{launcher}");
    }

    // ---- follow user ---------------------------------------------------------

    #[test]
    fn follow_user_produces_request_follow_user_with_the_user_id() {
        let launcher = place_launcher_url(&build("job-ignored", "", true, false, "", "", false));
        assert!(launcher.contains("request=RequestFollowUser"), "{launcher}");
        // The place_id argument doubles as the followed user id.
        assert!(launcher.contains("&userId=606849621"), "{launcher}");
        assert!(!launcher.contains("gameId"), "{launcher}");
        assert!(!launcher.contains("isTeleport"), "{launcher}");
    }

    // ---- launch data ---------------------------------------------------------

    #[test]
    fn launch_data_is_omitted_when_empty() {
        for launcher in [
            place_launcher_url(&build("job-abc", "", false, false, "", "", false)),
            place_launcher_url(&build("", "", true, false, "", "", false)),
            place_launcher_url(&build("", "", false, true, "", "abc", false)),
        ] {
            assert!(!launcher.contains("launchData"), "{launcher}");
        }
    }

    #[test]
    fn launch_data_is_appended_url_encoded_on_every_request_shape() {
        let public = place_launcher_url(&build("job-abc", "a b&c", false, false, "", "", false));
        assert!(public.contains("&launchData=a%20b%26c"), "{public}");

        let follow = place_launcher_url(&build("", "a b&c", true, false, "", "", false));
        assert!(follow.contains("&launchData=a%20b%26c"), "{follow}");

        let vip = place_launcher_url(&build("", "a b&c", false, true, "", "abc", false));
        assert!(vip.contains("&launchData=a%20b%26c"), "{vip}");
    }
}

#[cfg(test)]
mod channel_follow_tests {
    use super::*;

    // The app follows the channel Roblox set instead of pinning its own; these
    // guard the mapping from that channel to the version endpoint and the CDN.

    #[test]
    fn production_is_recognised_by_every_spelling() {
        assert!(channel_is_production(""));
        assert!(channel_is_production("   "));
        assert!(channel_is_production("production"));
        assert!(channel_is_production("PRODUCTION"));
        assert!(channel_is_production("live"));
        assert!(channel_is_production("LIVE"));
        assert!(!channel_is_production("ztestlinkerset"));
        assert!(!channel_is_production("zswocc-500-c"));
    }

    #[test]
    fn production_uses_the_default_version_endpoint() {
        // clientsettings answers 401 for /channel/production, so it must not
        // be used for the production channel.
        assert_eq!(channel_version_url("production"), PRODUCTION_VERSION_URL);
        assert_eq!(channel_version_url(""), PRODUCTION_VERSION_URL);
        assert_eq!(channel_version_url("LIVE"), PRODUCTION_VERSION_URL);
    }

    #[test]
    fn a_test_channel_uses_its_own_version_endpoint_in_lowercase() {
        assert_eq!(
            channel_version_url("ZTestLinkerSet"),
            format!("{}/channel/ztestlinkerset", PRODUCTION_VERSION_URL)
        );
    }

    #[test]
    fn the_cdn_calls_production_live_and_keeps_other_channel_names() {
        assert_eq!(channel_for_cdn("production"), "LIVE");
        assert_eq!(channel_for_cdn(""), "LIVE");
        assert_eq!(channel_for_cdn("ztestlinkerset"), "ztestlinkerset");
    }

    #[test]
    fn the_current_channel_is_never_empty() {
        // Missing/blank registry value means production.
        let channel = current_player_channel();
        assert!(!channel.trim().is_empty());
    }

    #[test]
    fn a_resolved_build_is_remembered_for_sync_callers() {
        remember_resolved_build("version-deadbeefdeadbeef");
        let remembered = LAST_RESOLVED_BUILD.lock().unwrap().clone();
        assert_eq!(remembered.as_deref(), Some("version-deadbeefdeadbeef"));
        // Not installed, so the sync helper must not hand out a bogus folder.
        assert!(cached_production_player_dir().is_none());
    }

    #[test]
    fn an_installed_build_is_found_and_a_missing_one_is_not() {
        assert!(installed_player_exe("version-does-not-exist-0000").is_none());
    }
}

#[cfg(test)]
mod channel_build_pairing_tests {
    use super::*;

    // Regressão do bug que voltou duas vezes: a build aberta tem que casar com
    // o canal que o cliente vai consultar. Quem manda nisso é o campo
    // `channel:` DENTRO da URL de launch — provado nos logs do cliente:
    // build 0.739 (canal de teste) + `channel:` vazio => updateRequired TRUE
    // (instalador em primeiro plano, fecha os outros clientes);
    // build 0.740 (produção) + `channel:` vazio => updateRequired FALSE.

    #[test]
    fn the_launch_url_always_declares_the_production_channel() {
        // `channel:` vazio = produção, igual ao site e ao launcher oficial.
        // Se algum dia isso mudar, `launch_url` precisa mudar junto.
        let url = build_launch_url("t", 1, "", "btid", "", false, false, "", "", false);
        assert!(
            url.contains("+channel:+"),
            "a URL deixou de mandar channel vazio: {url}"
        );
    }

    #[test]
    fn the_protocol_path_resolves_the_production_build() {
        // `launch_url` usa PRODUCTION_CHANNEL, que tem que bater com o campo
        // vazio da URL acima.
        assert!(channel_is_production(PRODUCTION_CHANNEL));
        assert_eq!(channel_version_url(PRODUCTION_CHANNEL), PRODUCTION_VERSION_URL);
    }

    #[test]
    fn the_old_join_path_follows_the_registry_channel() {
        // Old join não passa URL: o cliente lê o canal do registro, então a
        // build tem que ser a daquele canal (o oposto do caminho do protocolo).
        let channel = current_player_channel();
        assert!(!channel.trim().is_empty());
        if channel_is_production(&channel) {
            assert_eq!(channel_version_url(&channel), PRODUCTION_VERSION_URL);
        } else {
            assert_eq!(
                channel_version_url(&channel),
                format!("{}/channel/{}", PRODUCTION_VERSION_URL, channel.to_ascii_lowercase())
            );
        }
    }
}
