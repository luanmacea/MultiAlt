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
/// Rota do clientsettings que diz a build do Windows de um canal.
const CLIENT_VERSION_ROUTE: &str = "/v2/client-version/WindowsPlayer";
const PRODUCTION_VERSION_CACHE_TTL: Duration = Duration::from_secs(60);
/// Plano da consulta de versão: (índice em `production_version_urls`, espera
/// antes). CDN, servidor de origem na hora, e o CDN de novo depois de uma
/// pausa curta. Pior caso (tudo em timeout) ~19 s — e só quando a rede está
/// ruim; antes, uma falha só mandava o launch para o instalador do Roblox.
const VERSION_LOOKUP_PLAN: &[(usize, Duration)] = &[
    (0, Duration::ZERO),
    (1, Duration::ZERO),
    (0, Duration::from_millis(750)),
];
/// Depois de uma consulta que falhou em todos os hosts, as próximas (o mesmo
/// launch chama a consulta mais de uma vez: pasta do ClientSettings, spawn)
/// não voltam à rede por este tempo — senão cada uma repetiria o plano inteiro.
const LOOKUP_FAILURE_BACKOFF: Duration = Duration::from_secs(20);
/// Última build de produção resolvida, gravada para sobreviver a um restart
/// (`app_data_root`, junto do catálogo de versões — é dado da máquina, não da
/// conta). É o que o launch usa quando a consulta falha logo depois de abrir o
/// app, com o cache em memória ainda vazio.
const PRODUCTION_BUILD_FILE: &str = "RobloxProductionBuild.txt";
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
/// Quando a consulta de cada canal falhou em todos os hosts pela última vez.
static CHANNEL_LOOKUP_FAILED_AT: LazyLock<Mutex<HashMap<String, std::time::Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

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
/// O handler do protocolo só entra como último recurso: nenhuma build de
/// produção instalada **e** o download falhou (ver `plan_production_build`).
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

/// O old join (`--app -t -j`) abre o exe direto e pula a checagem de update
/// que o Roblox faz no launch pelo protocolo: só pode usar a pasta da build
/// **atual** do canal do registro (31/10/2026, bans por "disallowed version of
/// Roblox"). Sem resposta da consulta de versão, mantém (não dá para afirmar
/// que é velha) — a regra nº 1 vale.
pub async fn old_join_folder_is_current(client_dir: &str) -> bool {
    match build_version_for_channel(&current_player_channel()).await {
        Some(current) => folder_is_build(client_dir, &current),
        None => true,
    }
}

fn folder_is_build(client_dir: &str, build: &str) -> bool {
    std::path::Path::new(client_dir.trim_end_matches(['\\', '/']))
        .file_name()
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(build))
}

/// `%LOCALAPPDATA%\Roblox\Versions\<build>\RobloxPlayerBeta.exe` da build
/// daquele canal, se estiver instalada.
async fn player_exe_for_channel(channel: &str) -> Option<PathBuf> {
    let version = build_version_for_channel(channel).await?;
    installed_player_exe(&version)
}

fn installed_player_exe(version: &str) -> Option<PathBuf> {
    let dir = roblox_versions_dir()?.join(version);
    build_folder_ready(&dir).then(|| dir.join("RobloxPlayerBeta.exe"))
}

/// A pasta tem o `RobloxPlayerBeta.exe` **e** as DLLs do Roblox que ele importa.
///
/// Relato do Discord (30/10/2026): "RobloxPlayerBeta.dll was not found" ao
/// abrir. O exe carrega o jogo de `RobloxPlayerBeta.dll`, na mesma pasta (vem
/// no `RobloxApp.zip`). Uma pasta com o exe e sem a DLL (apagada por outro
/// programa ou por instalação interrompida) passava por instalada e o cliente
/// morria nessa caixa de erro a cada launch. Agora ela conta como faltando: o
/// launch baixa a build de novo, ou cai na próxima instalada.
fn build_folder_ready(dir: &std::path::Path) -> bool {
    let exe = dir.join("RobloxPlayerBeta.exe");
    if !exe.exists() {
        return false;
    }
    match missing_roblox_import(&exe) {
        Some(dll) => {
            eprintln!(
                "Roblox build {} is incomplete: {dll} is missing; it will be installed again.",
                dir.display()
            );
            false
        }
        None => true,
    }
}

/// A DLL do Roblox (nome começando por "Roblox") que o exe importa e que não
/// está ao lado dele. Lê só os cabeçalhos PE e a tabela de importação, não o
/// exe inteiro (passa de 100 MB). Qualquer coisa ilegível dá `None`: na dúvida
/// a build vale, nunca é motivo para deixar de abrir.
fn missing_roblox_import(exe: &std::path::Path) -> Option<String> {
    let dir = exe.parent()?;
    pe_imported_dlls(exe)?
        .into_iter()
        .filter(|name| name.to_ascii_lowercase().starts_with("roblox"))
        .find(|name| !dir.join(name).exists())
}

/// Os nomes das DLLs na tabela de importação de um PE (32 ou 64 bits).
fn pe_imported_dlls(exe: &std::path::Path) -> Option<Vec<String>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(exe).ok()?;
    let mut read_at = |offset: u64, len: usize| -> Option<Vec<u8>> {
        let mut buf = vec![0u8; len];
        file.seek(SeekFrom::Start(offset)).ok()?;
        file.read_exact(&mut buf).ok()?;
        Some(buf)
    };
    let u16_at = |b: &[u8], i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let u32_at = |b: &[u8], i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);

    let dos = read_at(0, 64)?;
    if &dos[0..2] != b"MZ" {
        return None;
    }
    let pe = u32_at(&dos, 0x3C) as u64;
    let headers = read_at(pe, 24)?;
    if &headers[0..4] != b"PE\0\0" {
        return None;
    }
    let sections = (u16_at(&headers, 6) as usize).min(96);
    let optional_size = u16_at(&headers, 20) as usize;
    let optional = read_at(pe + 24, optional_size)?;
    if optional.len() < 2 {
        return None;
    }
    let directories = match u16_at(&optional, 0) {
        0x20B => 112, // PE32+
        0x10B => 96,  // PE32
        _ => return None,
    };
    if optional.len() < directories + 16 {
        return None;
    }
    let import_rva = u32_at(&optional, directories + 8);
    if import_rva == 0 {
        return Some(Vec::new());
    }
    let table = read_at(pe + 24 + optional_size as u64, sections * 40)?;
    let to_offset = |rva: u32| -> Option<u64> {
        (0..sections).find_map(|i| {
            let s = &table[i * 40..i * 40 + 40];
            let (size, va, raw_size, raw) = (u32_at(s, 8), u32_at(s, 12), u32_at(s, 16), u32_at(s, 20));
            (rva >= va && rva < va.saturating_add(size.max(raw_size))).then(|| (rva - va) as u64 + raw as u64)
        })
    };
    let mut names = Vec::new();
    let mut descriptor = to_offset(import_rva)?;
    for _ in 0..256 {
        let entry = read_at(descriptor, 20)?;
        if entry.iter().all(|b| *b == 0) {
            break;
        }
        let raw_name = to_offset(u32_at(&entry, 12))
            .and_then(|at| read_at(at, 64))
            .unwrap_or_default();
        let name: String = raw_name
            .iter()
            .take_while(|b| **b != 0)
            .map(|b| *b as char)
            .collect();
        if !name.is_empty() {
            names.push(name);
        }
        descriptor += 20;
    }
    Some(names)
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
    if channel_is_production(channel) {
        return ensure_production_player_exe().await;
    }
    // Canal de teste (old join, que lê o canal do registro): sem os fallbacks
    // de produção — uma build de outro canal faria o cliente pedir update. Quem
    // chama cai na própria pasta de fallback, nunca no handler cru.
    if let Some(exe) = player_exe_for_channel(channel).await {
        return Some(exe);
    }
    let version = build_version_for_channel(channel).await?;
    install_player_build(channel, &version).await
}

/// O que fazer para ter uma build de **produção** para abrir.
#[derive(Debug, PartialEq, Eq)]
enum ProductionBuildPlan {
    /// Abrir esta build, já instalada.
    Use(String),
    /// Baixar `version`; se o download falhar, abrir `fallback` (instalada).
    Install {
        version: String,
        fallback: Option<String>,
    },
    /// Nada instalado e nada para baixar: só resta o último recurso.
    Nothing,
}

/// Decide qual build de produção abrir. `resolved` é o que a consulta de
/// versão respondeu (ou o cache vencido dela), `persisted` a última build de
/// produção conhecida (gravada em disco), `installed` as builds instaladas,
/// da mais nova para a mais antiga.
///
/// Por que uma build instalada vale mais que o instalador: o `launch_url`
/// manda `channel:` vazio = produção, então a build certa é a de produção.
/// Se a consulta falhou, a última de produção conhecida é a melhor aposta; na
/// falta dela, a mais nova da pasta (pode ser de um canal de teste que o
/// Roblox instalou — ainda assim é o que o handler cru abriria de pior). Uma
/// build um pouco velha pode fazer o cliente se atualizar sozinho; o handler
/// cru é **sempre** o instalador, que fecha todos os outros clientes.
fn plan_production_build(
    resolved: Option<&str>,
    persisted: Option<&str>,
    installed: &[String],
) -> ProductionBuildPlan {
    let is_installed = |v: &str| installed.iter().any(|i| i == v);
    let installed_persisted = persisted.filter(|p| is_installed(p));
    match resolved {
        Some(version) if is_installed(version) => ProductionBuildPlan::Use(version.to_string()),
        Some(version) => ProductionBuildPlan::Install {
            version: version.to_string(),
            fallback: installed_persisted
                .filter(|p| *p != version)
                .or_else(|| installed.iter().map(String::as_str).find(|i| *i != version))
                .map(str::to_string),
        },
        None => match (installed_persisted, installed.first(), persisted) {
            (Some(p), _, _) => ProductionBuildPlan::Use(p.to_string()),
            (None, Some(newest), _) => ProductionBuildPlan::Use(newest.clone()),
            (None, None, Some(p)) => ProductionBuildPlan::Install {
                version: p.to_string(),
                fallback: None,
            },
            (None, None, None) => ProductionBuildPlan::Nothing,
        },
    }
}

/// Builds instaladas em `versions_dir` (`version-<hex>` com o
/// `RobloxPlayerBeta.exe`), da pasta modificada por último para a mais antiga.
/// Pastas de staging/backup do instalador do app (`.staging-…`, `.old-…`)
/// ficam de fora pelo nome.
fn installed_builds_newest_first(versions_dir: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(versions_dir) else {
        return Vec::new();
    };
    let found = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !is_valid_version_hash(&name) || !build_folder_ready(&entry.path()) {
                return None;
            }
            let modified = entry.metadata().and_then(|m| m.modified()).ok()?;
            Some((name, modified))
        })
        .collect();
    order_builds_newest_first(found)
}

fn order_builds_newest_first(mut builds: Vec<(String, SystemTime)>) -> Vec<String> {
    builds.sort_by(|a, b| b.1.cmp(&a.1));
    builds.into_iter().map(|(name, _)| name).collect()
}

fn persisted_build_file() -> Option<PathBuf> {
    crate::data::settings::app_data_root().map(|root| root.join(PRODUCTION_BUILD_FILE))
}

/// Lê a build gravada; qualquer coisa que não seja `version-<hex>` é ignorada
/// (o nome vira caminho de pasta).
fn read_persisted_build(file: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(file).ok()?;
    let version = text.trim();
    is_valid_version_hash(version).then(|| version.to_string())
}

/// Grava a build (só quando mudou). Falha de disco não é erro de launch.
fn write_persisted_build(file: &std::path::Path, version: &str) {
    if !is_valid_version_hash(version) || read_persisted_build(file).as_deref() == Some(version) {
        return;
    }
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(file, version) {
        eprintln!("Could not save the last Roblox production build: {e}");
    }
}

/// Build de produção já resolvida para o launch em curso. O `client_dir` (que
/// recebe o patch do ClientSettings — FPS, fast flags) e o `launch_url` (o spawn)
/// pedem a **mesma** build com milissegundos de diferença; sem o memo cada um
/// refazia a resolução e, pior, um download que falhava na 1ª e passava na 2ª
/// deixava o ClientSettings na pasta de fallback em vez da build aberta. A janela
/// é curta (cobre o intervalo interno de um launch: resolução + auth + join) e
/// menor que o espaçamento típico entre contas de uma leva (AccountJoinDelay),
/// quando a build volta a ser reavaliada.
static PRODUCTION_EXE_MEMO: LazyLock<Mutex<Option<(std::time::Instant, Option<PathBuf>)>>> =
    LazyLock::new(|| Mutex::new(None));
/// Validade do memo da build de produção. Igual ao backoff da consulta de
/// versão (`LOOKUP_FAILURE_BACKOFF`): a mesma ideia de "não repetir a rede no
/// mesmo launch". Guardar um `None` (falha) por essa janela é de propósito — a
/// degradação graciosa é abrir a build instalada/fallback, nunca o instalador.
const PRODUCTION_EXE_MEMO_TTL: Duration = LOOKUP_FAILURE_BACKOFF;

/// Decide o que fazer com o memo da build de produção. `None` = resolver de
/// novo; `Some(v)` = reusar `v` (inclusive um `None` guardado: "já tentei e
/// falhou neste launch", para não baixar de novo). Uma build guardada que sumiu
/// do disco força nova resolução.
fn production_exe_memo_decision(
    entry: Option<&(std::time::Instant, Option<PathBuf>)>,
    now: std::time::Instant,
    ttl: Duration,
) -> Option<Option<PathBuf>> {
    let (at, value) = entry?;
    if now.duration_since(*at) >= ttl {
        return None;
    }
    if value.as_ref().is_some_and(|exe| !exe.exists()) {
        return None;
    }
    Some(value.clone())
}

/// Exe da build de produção, resolvida **uma vez por launch** (ver
/// [`PRODUCTION_EXE_MEMO`]) e reusada pelo patch e pelo spawn.
async fn ensure_production_player_exe() -> Option<PathBuf> {
    let now = std::time::Instant::now();
    if let Some(hit) = PRODUCTION_EXE_MEMO
        .lock()
        .ok()
        .and_then(|guard| production_exe_memo_decision(guard.as_ref(), now, PRODUCTION_EXE_MEMO_TTL))
    {
        return hit;
    }
    let resolved = resolve_production_player_exe().await;
    if let Ok(mut guard) = PRODUCTION_EXE_MEMO.lock() {
        *guard = Some((now, resolved.clone()));
    }
    resolved
}

/// Exe da build de produção, com os fallbacks de `plan_production_build`.
/// Sem memo: o invólucro [`ensure_production_player_exe`] é quem resolve uma vez
/// por launch.
async fn resolve_production_player_exe() -> Option<PathBuf> {
    let resolved = build_version_for_channel(PRODUCTION_CHANNEL).await;
    let installed = roblox_versions_dir()
        .map(|dir| installed_builds_newest_first(&dir))
        .unwrap_or_default();
    let persisted = persisted_build_file().and_then(|file| read_persisted_build(&file));
    match plan_production_build(resolved.as_deref(), persisted.as_deref(), &installed) {
        ProductionBuildPlan::Use(version) => {
            if resolved.as_deref() != Some(version.as_str()) {
                eprintln!(
                    "Roblox version lookup failed; opening the installed build {version} instead of Roblox's installer"
                );
            }
            installed_player_exe(&version)
        }
        ProductionBuildPlan::Install { version, fallback } => {
            match install_player_build(PRODUCTION_CHANNEL, &version).await {
                Some(exe) => Some(exe),
                None => fallback.and_then(|old| {
                    eprintln!(
                        "Could not install Roblox build {version}; opening the installed build {old} instead of Roblox's installer"
                    );
                    installed_player_exe(&old)
                }),
            }
        }
        ProductionBuildPlan::Nothing => None,
    }
}

/// Baixa e instala `version` (do `channel`) em `%LOCALAPPDATA%\Roblox\Versions`.
async fn install_player_build(channel: &str, version: &str) -> Option<PathBuf> {
    let channel = channel.to_string();
    let version = version.to_string();
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
    build_folder_ready(&dir).then(|| dir.to_string_lossy().into_owned())
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
///
/// E só quando a consulta veio do **CDN** (`cdn = true`), o host que o próprio
/// cliente do Roblox consulta. Um 401/404 do servidor de **origem**
/// (`clientsettings.roblox.com`, o fallback) para um canal que o CDN ainda
/// serve não prova aposentadoria: gravar `production` aí faria o launch pelo
/// site divergir. Nesse caso o 4xx cai para `read_build`, que devolve `None`, e
/// o registro fica intocado.
async fn fetch_channel_build(
    client: &reqwest::Client,
    production_url: &str,
    channel: &str,
    cdn: bool,
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
    if retired && cdn && !channel_is_production(channel) {
        let production = client.get(production_url).send().await.ok()?;
        return read_build(production)
            .await
            .map(ChannelBuild::RetiredUseProduction);
    }
    read_build(resp).await.map(ChannelBuild::Channel)
}

/// URLs de produção do clientsettings, na ordem do plano: o CDN (o mesmo que
/// o cliente consulta) e o servidor de origem, que responde a mesma rota.
fn production_version_urls() -> Vec<String> {
    ["clientsettingscdn", "clientsettings"]
        .iter()
        .map(|sub| format!("{}{}", crate::api::endpoints::host(sub), CLIENT_VERSION_ROUTE))
        .collect()
}

/// `fetch_channel_build` seguindo `plan`: cada passo espera o seu tempo e
/// consulta o host indicado; o primeiro que responde vence.
async fn fetch_channel_build_with_retry(
    client: &reqwest::Client,
    production_urls: &[String],
    channel: &str,
    plan: &[(usize, Duration)],
) -> Option<ChannelBuild> {
    for (host, delay) in plan {
        if !delay.is_zero() {
            tokio::time::sleep(*delay).await;
        }
        let Some(url) = production_urls.get(*host) else {
            continue;
        };
        // Só o CDN (índice 0, o host que o cliente consulta) é autoritativo para
        // dizer que um canal foi aposentado; ver `fetch_channel_build`.
        if let Some(found) = fetch_channel_build(client, url, channel, *host == 0).await {
            return Some(found);
        }
    }
    None
}

/// A consulta falhou há pouco (`elapsed` desde a falha)? Então não volta à rede.
fn lookup_recently_failed(elapsed: Option<Duration>) -> bool {
    elapsed.is_some_and(|e| e < LOOKUP_FAILURE_BACKOFF)
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

    // Acabou de falhar em todos os hosts: não repete o plano inteiro a cada
    // chamada do mesmo launch — quem chama decide com o que tem.
    let failed_recently = CHANNEL_LOOKUP_FAILED_AT
        .lock()
        .ok()
        .and_then(|failed| failed.get(&channel).map(|at| at.elapsed()));
    let fetched = if lookup_recently_failed(failed_recently) {
        None
    } else {
        async {
            // Teto próprio e curto por tentativa: esta consulta tem fallback (o
            // cache vencido, a build gravada, as instaladas), então esperar
            // mais só atrasa o launch. `connect_timeout` vem do builder
            // compartilhado.
            let client = crate::api::http_client::builder_with(
                CHANNEL_LOOKUP_TIMEOUT,
                CHANNEL_LOOKUP_TIMEOUT,
            )
            .build()
            .ok()?;
            let urls = production_version_urls();
            match fetch_channel_build_with_retry(&client, &urls, &channel, VERSION_LOOKUP_PLAN)
                .await?
            {
                ChannelBuild::Channel(version) => Some(version),
                ChannelBuild::RetiredUseProduction(version) => {
                    set_player_channel(PRODUCTION_CHANNEL);
                    Some(version)
                }
            }
        }
        .await
    };

    match fetched {
        Some(version) => {
            remember_resolved_build(&channel, &version);
            if channel_is_production(&channel) {
                if let Some(file) = persisted_build_file() {
                    write_persisted_build(&file, &version);
                }
            }
            if let Ok(mut cache) = CHANNEL_VERSION_CACHE.lock() {
                cache.insert(channel.clone(), (std::time::Instant::now(), version.clone()));
            }
            if let Ok(mut failed) = CHANNEL_LOOKUP_FAILED_AT.lock() {
                failed.remove(&channel);
            }
            Some(version)
        }
        // Network hiccup: a stale answer is still far better than the protocol
        // handler, whose build may not match the channel.
        None => {
            if !lookup_recently_failed(failed_recently) {
                if let Ok(mut failed) = CHANNEL_LOOKUP_FAILED_AT.lock() {
                    failed.insert(channel.clone(), std::time::Instant::now());
                }
            }
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
