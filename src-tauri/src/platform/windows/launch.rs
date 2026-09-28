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
/// `production_url` is [`PRODUCTION_VERSION_URL`] outside the tests.
fn channel_version_url(production_url: &str, channel: &str) -> String {
    if channel_is_production(channel) {
        production_url.to_string()
    } else {
        format!(
            "{}/channel/{}",
            production_url,
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

/// Guarda a build de **produção** para os helpers síncronos
/// (`get_roblox_path`). Build de outro canal (a do registro, no old join) não
/// entra: quem pede a pasta de produção receberia a do canal.
fn remember_resolved_build(channel: &str, version: &str) {
    if !channel_is_production(channel) {
        return;
    }
    if let Ok(mut guard) = LAST_RESOLVED_BUILD.lock() {
        *guard = Some(version.to_string());
    }
}

/// De onde o cliente deste launch abre — e, portanto, onde o
/// `ClientAppSettings.json` (FPS, fast flags) tem que ser gravado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientSource {
    /// Old join numa versão do catálogo: a pasta dela.
    Catalog,
    /// Old join sem versão do catálogo: a build do canal do **registro**.
    RegistryChannel,
    /// Protocolo (`launch_url`): sempre a build de produção.
    Production,
}

pub fn client_source(use_old_join: bool, has_catalog_version: bool) -> ClientSource {
    match (use_old_join, has_catalog_version) {
        (true, true) => ClientSource::Catalog,
        (true, false) => ClientSource::RegistryChannel,
        (false, _) => ClientSource::Production,
    }
}

/// Pasta de onde o cliente vai abrir. É a **mesma** que o spawn usa: o patch
/// e o old join têm que receber este valor, não um calculado antes.
/// `resolved_base_path` é a pasta de `resolve_roblox_install_path` — a da
/// versão do catálogo, ou o fallback quando a build não pôde ser achada nem
/// instalada.
pub async fn client_dir(source: ClientSource, resolved_base_path: &str) -> String {
    match source {
        ClientSource::Catalog => resolved_base_path.to_string(),
        ClientSource::RegistryChannel => default_player_dir(resolved_base_path).await,
        ClientSource::Production => ensure_player_exe_for_channel(PRODUCTION_CHANNEL)
            .await
            .and_then(|exe| exe.parent().map(|p| p.to_string_lossy().into_owned()))
            .unwrap_or_else(|| resolved_base_path.to_string()),
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

/// O que a consulta de build de um canal decidiu.
#[derive(Debug, PartialEq)]
enum ChannelBuild {
    /// O endpoint do próprio canal respondeu.
    Channel(String),
    /// Build de produção no lugar da do canal. Só é seguro abri-la gravando
    /// `production` no registro (`set_player_channel`): senão o cliente
    /// consulta o canal antigo e o instalador do Roblox roda.
    RetiredUseProduction(String),
}

/// Consulta, em `production_url` (a URL de produção do clientsettings), a
/// build que o cliente vai exigir para `channel`.
///
/// Só um canal **aposentado** — o endpoint dele responde 401/404 — cai na
/// build de produção (e no reparo do registro). Timeout, 429, 5xx ou resposta
/// ilegível não dizem nada sobre o canal: dá `None`, e quem chama usa o cache
/// vencido sem tocar no registro.
async fn fetch_channel_build(
    client: &reqwest::Client,
    production_url: &str,
    channel: &str,
) -> Option<ChannelBuild> {
    let resp = client
        .get(channel_version_url(production_url, channel))
        .send()
        .await
        .ok()?;
    let retired = matches!(
        resp.status(),
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::NOT_FOUND
    );
    if retired && !channel_is_production(channel) {
        let production = client.get(production_url).send().await.ok()?;
        return read_build(production)
            .await
            .map(ChannelBuild::RetiredUseProduction);
    }
    read_build(resp).await.map(ChannelBuild::Channel)
}

/// A build (`version-...`) de uma resposta do clientsettings.
async fn read_build(resp: reqwest::Response) -> Option<String> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ClientVersion {
        client_version_upload: String,
    }

    let body = resp
        .error_for_status()
        .ok()?
        .json::<ClientVersion>()
        .await
        .ok()?;
    let version = body.client_version_upload.trim().to_string();
    version.starts_with("version-").then_some(version)
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
            remember_resolved_build(&channel, version);
            return Some(version.clone());
        }
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
        match fetch_channel_build(&client, PRODUCTION_VERSION_URL, &channel).await? {
            ChannelBuild::Channel(version) => Some(version),
            ChannelBuild::RetiredUseProduction(version) => {
                set_player_channel(PRODUCTION_CHANNEL);
                Some(version)
            }
        }
    }
    .await;

    match fetched {
        Some(version) => {
            remember_resolved_build(&channel, &version);
            if let Ok(mut cache) = CHANNEL_VERSION_CACHE.lock() {
                cache.insert(channel, (std::time::Instant::now(), version.clone()));
            }
            Some(version)
        }
        // Network hiccup: a stale answer is still far better than the protocol
        // handler, whose build may not match the channel.
        None => {
            let stale = cached.map(|(_, version)| version);
            if let Some(version) = &stale {
                remember_resolved_build(&channel, version);
            }
            stale
        }
    }
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
