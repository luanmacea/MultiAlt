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

static PRODUCTION_VERSION_CACHE: LazyLock<Mutex<Option<(std::time::Instant, String)>>> =
    LazyLock::new(|| Mutex::new(None));

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
/// To make every launch deterministic we pin the channel to production and
/// start the production build's `RobloxPlayerBeta.exe` directly with the
/// protocol URL (exactly what the official installer does), bypassing the
/// flip-flopping protocol handler. When Roblox ships a new production build we
/// download and install it ourselves (silently, without touching running
/// clients) instead of letting Roblox's installer do it; the protocol handler
/// is only a last resort if that install fails.
pub async fn launch_url(url: &str) -> Result<(), String> {
    pin_player_channel_to_production();

    if let Some(player_exe) = ensure_production_player_exe().await {
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

/// Install folder to use when no RAM-managed version was chosen: pins the
/// channel and returns the production build folder, falling back to
/// `fallback` (the registry-resolved install) when it is not installed.
/// See `launch_url` for why the registry handler must not be trusted.
pub async fn default_player_dir(fallback: &str) -> String {
    pin_player_channel_to_production();
    ensure_production_player_exe()
        .await
        .and_then(|exe| exe.parent().map(|p| p.to_string_lossy().into_owned()))
        .unwrap_or_else(|| fallback.to_string())
}

/// Writes `production` as the Roblox player channel so the client checks its
/// build against the production CDN entry instead of an account test channel.
fn pin_player_channel_to_production() {
    use windows_sys::Win32::System::Registry::{
        RegCreateKeyExW, RegSetValueExW, HKEY_CURRENT_USER, KEY_SET_VALUE,
        REG_OPTION_NON_VOLATILE,
    };

    let key_name = encode_wide(ROBLOX_PLAYER_CHANNEL_KEY);
    let value_name = encode_wide("www.roblox.com");
    let value = encode_wide(PRODUCTION_CHANNEL);
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

/// `%LOCALAPPDATA%\Roblox\Versions\<production build>\RobloxPlayerBeta.exe`,
/// if that build is installed.
async fn production_player_exe() -> Option<PathBuf> {
    let version = production_version_guid().await?;
    let exe = roblox_versions_dir()?
        .join(&version)
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

/// Production build's exe, installing the build first when Roblox has shipped
/// a new one. Returns `None` only if we could neither find nor install it.
async fn ensure_production_player_exe() -> Option<PathBuf> {
    if let Some(exe) = production_player_exe().await {
        return Some(exe);
    }

    let version = production_version_guid().await?;
    let _serialize = BUILD_INSTALL_LOCK.lock().await;
    // Another launch may have installed it while we waited for the lock.
    if let Some(exe) = production_player_exe().await {
        return Some(exe);
    }

    let target = roblox_versions_dir()?.join(&version);
    emit_build_install(&version, "starting", 0, 0, None);
    let progress = |stage: &str, _package: Option<String>, current: u64, total: u64| {
        emit_build_install(&version, stage, current, total, None);
    };
    match install_build_to_dir(PRODUCTION_CHANNEL_CDN, &version, &target, 4, &progress).await {
        Ok(()) => {
            emit_build_install(&version, "ready", 0, 0, None);
            let exe = target.join("RobloxPlayerBeta.exe");
            exe.exists().then_some(exe)
        }
        Err(e) => {
            emit_build_install(&version, "error", 0, 0, Some(e.clone()));
            eprintln!("Could not install Roblox production build {version}: {e}");
            None
        }
    }
}

fn roblox_versions_dir() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(local).join("Roblox").join("Versions"))
}

/// Resolves (and caches) the production build so sync helpers such as
/// `get_roblox_path` — used to write ClientSettings — target the same folder
/// that `launch_url` will start.
pub async fn refresh_production_version() {
    let _ = production_version_guid().await;
}

/// Production build folder from the cache filled by `production_version_guid`.
fn cached_production_player_dir() -> Option<String> {
    let (_, version) = PRODUCTION_VERSION_CACHE.lock().ok()?.clone()?;
    let local = std::env::var_os("LOCALAPPDATA")?;
    let dir = PathBuf::from(local).join("Roblox").join("Versions").join(version);
    dir.join("RobloxPlayerBeta.exe")
        .exists()
        .then(|| dir.to_string_lossy().into_owned())
}

async fn production_version_guid() -> Option<String> {
    let cached = PRODUCTION_VERSION_CACHE.lock().ok().and_then(|c| c.clone());
    if let Some((at, version)) = &cached {
        if at.elapsed() < PRODUCTION_VERSION_CACHE_TTL {
            return Some(version.clone());
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ClientVersion {
        client_version_upload: String,
    }

    let fetched = async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(6))
            .build()
            .ok()?;
        let resp = client.get(PRODUCTION_VERSION_URL).send().await.ok()?;
        let body: ClientVersion = resp.error_for_status().ok()?.json().await.ok()?;
        let version = body.client_version_upload.trim().to_string();
        version.starts_with("version-").then_some(version)
    }
    .await;

    match fetched {
        Some(version) => {
            if let Ok(mut cache) = PRODUCTION_VERSION_CACHE.lock() {
                *cache = Some((std::time::Instant::now(), version.clone()));
            }
            Some(version)
        }
        // Network hiccup: a stale answer is still far better than the protocol
        // handler, which may point at an account test-channel build.
        None => cached.map(|(_, version)| version),
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
