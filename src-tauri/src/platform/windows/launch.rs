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
/// URL de produção do CDN do clientsettings. Só os testes a usam direto; o
/// launch monta as URLs por `production_version_urls` (via `endpoints::host`).
#[cfg(test)]
const PRODUCTION_VERSION_URL: &str =
    "https://clientsettingscdn.roblox.com/v2/client-version/WindowsPlayer";
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

    // ---- canal de produção: o app lê o canal, nunca o fixa --------------------

    #[test]
    fn production_channel_constants_are_intact() {
        // `launch_url` abre a build de produção (`production`, `LIVE` no CDN) e
        // o old join lê o canal desta chave — o app não fixa canal nenhum no
        // registro. Mudar estes valores faz a build aberta divergir do canal
        // que o cliente consulta e traz de volta o instalador que fecha todos
        // os outros clientes (docs/features/launch.md).
        assert_eq!(PRODUCTION_CHANNEL, "production");
        assert_eq!(PRODUCTION_CHANNEL_CDN, "LIVE");
        assert_eq!(
            ROBLOX_PLAYER_CHANNEL_KEY,
            "Software\\ROBLOX Corporation\\Environments\\RobloxPlayer\\Channel"
        );
    }

    #[test]
    fn the_launch_url_still_ends_with_an_empty_channel() {
        // `+channel:` vazio = produção: o campo da URL vence o canal do
        // registro (provado nos logs do cliente), por isso `launch_url` abre a
        // build de produção sem ler nem escrever o registro.
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
        assert_eq!(
            channel_version_url(PRODUCTION_VERSION_URL, "production"),
            PRODUCTION_VERSION_URL
        );
        assert_eq!(
            channel_version_url(PRODUCTION_VERSION_URL, ""),
            PRODUCTION_VERSION_URL
        );
        assert_eq!(
            channel_version_url(PRODUCTION_VERSION_URL, "LIVE"),
            PRODUCTION_VERSION_URL
        );
    }

    #[test]
    fn a_test_channel_uses_its_own_version_endpoint_in_lowercase() {
        assert_eq!(
            channel_version_url(PRODUCTION_VERSION_URL, "ZTestLinkerSet"),
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
        remember_resolved_build(PRODUCTION_CHANNEL, "version-deadbeefdeadbeef");
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
        assert_eq!(
            channel_version_url(PRODUCTION_VERSION_URL, PRODUCTION_CHANNEL),
            PRODUCTION_VERSION_URL
        );
    }

    #[test]
    fn the_old_join_path_follows_the_registry_channel() {
        // Old join não passa URL: o cliente lê o canal do registro, então a
        // build tem que ser a daquele canal (o oposto do caminho do protocolo).
        let channel = current_player_channel();
        assert!(!channel.trim().is_empty());
        if channel_is_production(&channel) {
            assert_eq!(
                channel_version_url(PRODUCTION_VERSION_URL, &channel),
                PRODUCTION_VERSION_URL
            );
        } else {
            assert_eq!(
                channel_version_url(PRODUCTION_VERSION_URL, &channel),
                format!(
                    "{}/channel/{}",
                    PRODUCTION_VERSION_URL,
                    channel.to_ascii_lowercase()
                )
            );
        }
    }
}

#[cfg(test)]
mod channel_repair_tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // Regressão: o reparo do canal — a única escrita no registro do Roblox —
    // disparava em qualquer falha da consulta (timeout, 429, 5xx) sempre que a
    // produção respondia. Só um canal aposentado (401/404) justifica abrir a
    // build de produção e gravar `production`; erro passageiro não diz nada
    // sobre o canal, e gravar ali faz o launch pelo site divergir.

    const CHANNEL: &str = "zcanaldeteste";
    const ROUTE: &str = "/v2/client-version/WindowsPlayer";

    /// Resposta real do clientsettings, com todos os campos.
    fn build_answer(upload: &str) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "version": "0.700.0.7000839",
            "clientVersionUpload": upload,
            "bootstrapperVersion": "1, 6, 0, 7000839"
        }))
    }

    /// Servidor próprio (a URL entra por parâmetro, não por `endpoints::host`): o
    /// endpoint do canal responde `channel`, o de produção responde
    /// `production`. Devolve o servidor (vivo enquanto o teste roda) e a base.
    async fn serve(
        channel: ResponseTemplate,
        production: ResponseTemplate,
    ) -> (MockServer, String) {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("{ROUTE}/channel/{CHANNEL}")))
            .respond_with(channel)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(ROUTE))
            .respond_with(production)
            .mount(&server)
            .await;
        let base = format!("{}{ROUTE}", server.uri());
        (server, base)
    }

    fn client() -> reqwest::Client {
        crate::api::http_client::builder_with(CHANNEL_LOOKUP_TIMEOUT, CHANNEL_LOOKUP_TIMEOUT)
            .build()
            .unwrap()
    }

    /// O host CDN é autoritativo para aposentadoria (`cdn = true`).
    const CDN: bool = true;
    /// O servidor de origem é apenas fallback (`cdn = false`).
    const ORIGIN: bool = false;

    #[tokio::test]
    async fn a_transient_failure_never_falls_back_to_production() {
        for status in [408, 429, 500, 502, 503, 504] {
            let (_server, base) = serve(
                ResponseTemplate::new(status),
                build_answer("version-producao"),
            )
            .await;
            assert_eq!(
                fetch_channel_build(&client(), &base, CHANNEL, CDN).await,
                None,
                "HTTP {status} no canal levou à build de produção (e ao reparo)"
            );
        }
    }

    #[tokio::test]
    async fn an_unreadable_answer_never_falls_back_to_production() {
        let (_server, base) = serve(
            ResponseTemplate::new(200).set_body_string("<html>manutenção</html>"),
            build_answer("version-producao"),
        )
        .await;
        assert_eq!(fetch_channel_build(&client(), &base, CHANNEL, CDN).await, None);
    }

    #[tokio::test]
    async fn a_retired_channel_falls_back_to_production_and_asks_for_the_repair() {
        for status in [401, 404] {
            let (_server, base) = serve(
                ResponseTemplate::new(status),
                build_answer("version-producao"),
            )
            .await;
            assert_eq!(
                fetch_channel_build(&client(), &base, CHANNEL, CDN).await,
                Some(ChannelBuild::RetiredUseProduction(
                    "version-producao".into()
                )),
                "HTTP {status} no canal"
            );
        }
    }

    /// Regressão (crítico): só o CDN pode sinalizar um canal aposentado. Um
    /// 401/404 do servidor de **origem** para um canal que o CDN ainda serve
    /// não pode virar `RetiredUseProduction` — senão gravaria `production` no
    /// registro e o launch pelo site divergiria. Resultado esperado: `None`
    /// (sem reparo).
    #[tokio::test]
    async fn the_origin_server_never_signals_a_retired_channel() {
        for status in [401, 404] {
            let (_server, base) = serve(
                ResponseTemplate::new(status),
                build_answer("version-producao"),
            )
            .await;
            assert_eq!(
                fetch_channel_build(&client(), &base, CHANNEL, ORIGIN).await,
                None,
                "HTTP {status} no servidor de origem não pode pedir o reparo"
            );
        }
    }

    #[tokio::test]
    async fn a_retired_channel_without_a_production_answer_asks_for_nothing() {
        let (_server, base) = serve(ResponseTemplate::new(404), ResponseTemplate::new(503)).await;
        assert_eq!(fetch_channel_build(&client(), &base, CHANNEL, CDN).await, None);
    }

    #[tokio::test]
    async fn a_live_channel_uses_its_own_build() {
        let (_server, base) = serve(
            build_answer("version-docanal"),
            build_answer("version-producao"),
        )
        .await;
        assert_eq!(
            fetch_channel_build(&client(), &base, CHANNEL, CDN).await,
            Some(ChannelBuild::Channel("version-docanal".into()))
        );
    }
}

#[cfg(test)]
mod client_dir_tests {
    use super::*;

    // O `ClientAppSettings.json` (FPS e fast flags) é por pasta de build: tem
    // que ir para a pasta de onde o cliente vai abrir. O patch usava a pasta
    // resolvida antes do launch e o spawn usava outra — no old join sem versão
    // do catálogo (build do canal do registro) e no protocolo (build de
    // produção, que pode ter acabado de ser baixada).

    #[test]
    fn old_join_with_a_catalog_version_opens_that_version() {
        assert_eq!(client_source(true, true), ClientSource::Catalog);
    }

    #[test]
    fn old_join_without_a_catalog_version_opens_the_registry_channel_build() {
        assert_eq!(client_source(true, false), ClientSource::RegistryChannel);
    }

    #[test]
    fn the_protocol_always_opens_the_production_build() {
        assert_eq!(client_source(false, false), ClientSource::Production);
        // Versão do catálogo não vale pelo protocolo: o `channel:` vazio da URL
        // manda abrir produção (é o caso do isolamento que apaga a instalação).
        assert_eq!(client_source(false, true), ClientSource::Production);
    }

    /// `get_roblox_path` diz "a build de produção", mas o cache guardava a
    /// última build resolvida de **qualquer** canal: depois de um old join num
    /// canal de teste, quem pedia a pasta de produção recebia a do canal.
    #[test]
    fn a_test_channel_build_is_not_remembered_as_the_production_build() {
        remember_resolved_build("zcanaldeteste", "version-docanaldeteste");
        let remembered = LAST_RESOLVED_BUILD.lock().unwrap().clone();
        assert_ne!(remembered.as_deref(), Some("version-docanaldeteste"));
    }
}

#[cfg(test)]
mod production_build_fallback_tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // Regressão (ideias 40/29): uma única consulta de versão lenta ou falha
    // logo depois de abrir o app (cache em memória vazio) mandava o launch
    // para o handler `roblox-player:` cru — o instalador do Roblox, que fecha
    // todos os clientes — mesmo com a build de produção instalada no disco.

    fn builds(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn a_resolved_build_that_is_installed_is_used() {
        assert_eq!(
            plan_production_build(Some("version-aa"), None, &builds(&["version-aa"])),
            ProductionBuildPlan::Use("version-aa".into())
        );
    }

    #[test]
    fn a_resolved_build_that_is_missing_is_installed_with_an_installed_fallback() {
        assert_eq!(
            plan_production_build(
                Some("version-new"),
                Some("version-old"),
                &builds(&["version-other", "version-old"])
            ),
            ProductionBuildPlan::Install {
                version: "version-new".into(),
                fallback: Some("version-old".into()),
            },
            "o último build de produção conhecido vem antes do mais recente da pasta"
        );
        assert_eq!(
            plan_production_build(Some("version-new"), None, &builds(&["version-x", "version-y"])),
            ProductionBuildPlan::Install {
                version: "version-new".into(),
                fallback: Some("version-x".into()),
            }
        );
        assert_eq!(
            plan_production_build(Some("version-new"), None, &[]),
            ProductionBuildPlan::Install {
                version: "version-new".into(),
                fallback: None,
            }
        );
    }

    #[test]
    fn a_failed_lookup_uses_the_persisted_build_when_it_is_installed() {
        assert_eq!(
            plan_production_build(None, Some("version-old"), &builds(&["version-x", "version-old"])),
            ProductionBuildPlan::Use("version-old".into())
        );
    }

    #[test]
    fn a_failed_lookup_uses_the_newest_installed_build_before_the_installer() {
        assert_eq!(
            plan_production_build(None, Some("version-gone"), &builds(&["version-x", "version-y"])),
            ProductionBuildPlan::Use("version-x".into())
        );
        assert_eq!(
            plan_production_build(None, None, &builds(&["version-x"])),
            ProductionBuildPlan::Use("version-x".into())
        );
    }

    #[test]
    fn with_nothing_installed_the_persisted_build_is_downloaded() {
        assert_eq!(
            plan_production_build(None, Some("version-old"), &[]),
            ProductionBuildPlan::Install {
                version: "version-old".into(),
                fallback: None,
            }
        );
    }

    #[test]
    fn only_with_no_build_at_all_is_there_nothing_to_open() {
        assert_eq!(plan_production_build(None, None, &[]), ProductionBuildPlan::Nothing);
    }

    #[test]
    fn installed_builds_are_listed_newest_first() {
        let t0 = SystemTime::UNIX_EPOCH;
        let ordered = order_builds_newest_first(vec![
            ("version-a".into(), t0 + Duration::from_secs(10)),
            ("version-b".into(), t0 + Duration::from_secs(30)),
            ("version-c".into(), t0 + Duration::from_secs(20)),
        ]);
        assert_eq!(ordered, builds(&["version-b", "version-c", "version-a"]));
    }

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("ram4-prodbuild-{}-{}-{}", tag, nanos, n));
            std::fs::create_dir_all(&dir).expect("temp dir");
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_scan_skips_staging_folders_and_folders_without_the_client() {
        let temp = TempDir::new("scan");
        for (name, with_exe) in [
            ("version-0123abcd", true),
            ("version-4567ef01", false),
            ("version-89ab.staging-1700000000000", true),
            ("version-cdef.old-1700000000000", true),
            ("notaversion", true),
        ] {
            let dir = temp.0.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            if with_exe {
                std::fs::write(dir.join("RobloxPlayerBeta.exe"), b"MZ").unwrap();
            }
        }
        assert_eq!(installed_builds_newest_first(&temp.0), builds(&["version-0123abcd"]));
        assert!(installed_builds_newest_first(&temp.0.join("missing")).is_empty());
    }

    /// Um PE64 mínimo que importa `dlls` (tabela de importação na única seção).
    fn fake_pe_importing(dlls: &[&str]) -> Vec<u8> {
        let mut b = vec![0u8; 0x600];
        b[0..2].copy_from_slice(b"MZ");
        b[0x3C..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        b[0x40..0x44].copy_from_slice(b"PE\0\0");
        b[0x46..0x48].copy_from_slice(&1u16.to_le_bytes()); // seções
        b[0x54..0x56].copy_from_slice(&240u16.to_le_bytes()); // tamanho do opcional
        let opt = 0x58;
        b[opt..opt + 2].copy_from_slice(&0x20Bu16.to_le_bytes());
        b[opt + 120..opt + 124].copy_from_slice(&0x1000u32.to_le_bytes()); // import RVA
        let sec = opt + 240;
        b[sec + 8..sec + 12].copy_from_slice(&0x200u32.to_le_bytes());
        b[sec + 12..sec + 16].copy_from_slice(&0x1000u32.to_le_bytes());
        b[sec + 16..sec + 20].copy_from_slice(&0x200u32.to_le_bytes());
        b[sec + 20..sec + 24].copy_from_slice(&0x400u32.to_le_bytes());
        for (i, dll) in dlls.iter().enumerate() {
            let name_rva = 0x1100 + (i as u32) * 0x20;
            let desc = 0x400 + i * 20;
            b[desc + 12..desc + 16].copy_from_slice(&name_rva.to_le_bytes());
            let at = 0x500 + i * 0x20;
            b[at..at + dll.len()].copy_from_slice(dll.as_bytes());
        }
        b
    }

    /// Relato do Discord (30/10/2026): "RobloxPlayerBeta.dll was not found".
    /// A pasta com o exe e sem a DLL que ele importa não é build instalada.
    #[test]
    fn a_build_without_the_dll_its_exe_imports_is_not_installed() {
        let temp = TempDir::new("dll");
        let dir = temp.0.join("version-0123abcd");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("RobloxPlayerBeta.exe"),
            fake_pe_importing(&["RobloxPlayerBeta.dll", "KERNEL32.dll"]),
        )
        .unwrap();
        assert_eq!(
            pe_imported_dlls(&dir.join("RobloxPlayerBeta.exe")).unwrap(),
            vec!["RobloxPlayerBeta.dll".to_string(), "KERNEL32.dll".to_string()]
        );
        assert!(!build_folder_ready(&dir));
        assert!(installed_builds_newest_first(&temp.0).is_empty());

        // Com a DLL de volta a build vale; a do sistema (KERNEL32) não conta.
        std::fs::write(dir.join("RobloxPlayerBeta.dll"), b"MZ").unwrap();
        assert!(build_folder_ready(&dir));
        assert_eq!(installed_builds_newest_first(&temp.0), builds(&["version-0123abcd"]));
    }

    /// Exe que não dá para ler como PE não impede de abrir: na dúvida, vale.
    #[test]
    fn an_unreadable_exe_still_counts_as_installed() {
        let temp = TempDir::new("unreadable");
        for junk in [b"MZ".as_slice(), b"", b"not a pe at all"] {
            std::fs::write(temp.0.join("RobloxPlayerBeta.exe"), junk).unwrap();
            assert!(build_folder_ready(&temp.0));
        }
    }

    /// O exe de verdade instalado nesta máquina (quando há um) é lido: a
    /// tabela dele traz a `RobloxPlayerBeta.dll` (medido em 30/10/2026).
    #[test]
    fn the_real_roblox_exe_imports_its_dll() {
        let Some(entries) = roblox_versions_dir().and_then(|d| std::fs::read_dir(d).ok()) else {
            return;
        };
        for entry in entries.flatten() {
            let exe = entry.path().join("RobloxPlayerBeta.exe");
            if let Some(dlls) = pe_imported_dlls(&exe) {
                assert!(
                    dlls.iter().any(|d| d.eq_ignore_ascii_case("RobloxPlayerBeta.dll")),
                    "{}: {dlls:?}",
                    exe.display()
                );
            }
        }
    }

    #[test]
    fn the_last_production_build_survives_a_restart() {
        let temp = TempDir::new("persist");
        let file = temp.0.join("sub").join(PRODUCTION_BUILD_FILE);
        assert_eq!(read_persisted_build(&file), None);
        write_persisted_build(&file, "version-0123abcd");
        assert_eq!(read_persisted_build(&file).as_deref(), Some("version-0123abcd"));
        write_persisted_build(&file, "version-4567ef01");
        assert_eq!(read_persisted_build(&file).as_deref(), Some("version-4567ef01"));
    }

    #[test]
    fn a_garbled_persisted_build_is_ignored() {
        let temp = TempDir::new("garbled");
        let file = temp.0.join(PRODUCTION_BUILD_FILE);
        for junk in ["", "lixo", "version-../../x", "version-"] {
            std::fs::write(&file, junk).unwrap();
            assert_eq!(read_persisted_build(&file), None, "{junk:?}");
        }
        // Uma build inválida nunca é gravada.
        write_persisted_build(&file, "version-../evil");
        assert_ne!(
            std::fs::read_to_string(&file).unwrap_or_default(),
            "version-../evil"
        );
    }

    #[test]
    fn the_lookup_tries_the_cdn_then_the_origin_server() {
        let urls = production_version_urls();
        assert_eq!(urls.len(), 2);
        assert!(urls[0].contains("clientsettingscdn"), "{}", urls[0]);
        assert!(
            urls[1].contains("clientsettings") && !urls[1].contains("clientsettingscdn"),
            "{}",
            urls[1]
        );
        for url in &urls {
            assert!(url.ends_with(CLIENT_VERSION_ROUTE), "{url}");
        }
        // O plano começa no CDN, cai no servidor de origem e tenta de novo.
        assert!(VERSION_LOOKUP_PLAN.len() >= 2 && VERSION_LOOKUP_PLAN.len() <= 3);
        assert_eq!(VERSION_LOOKUP_PLAN[0], (0, Duration::ZERO));
        assert_eq!(VERSION_LOOKUP_PLAN[1].0, 1);
        assert!(VERSION_LOOKUP_PLAN
            .iter()
            .all(|(host, delay)| *host < urls.len() && *delay <= Duration::from_secs(2)));
    }

    #[test]
    fn a_recent_failure_skips_the_network_for_a_while() {
        assert!(!lookup_recently_failed(None));
        assert!(lookup_recently_failed(Some(Duration::from_secs(1))));
        assert!(!lookup_recently_failed(Some(
            LOOKUP_FAILURE_BACKOFF + Duration::from_secs(1)
        )));
    }

    /// Fix 4: a build de produção é resolvida uma vez por launch. O `client_dir`
    /// (patch do ClientSettings) e o `launch_url` (spawn) reusam a mesma decisão
    /// — inclusive uma falha —, então não há dois downloads nem ClientSettings
    /// na pasta errada.
    #[test]
    fn the_production_build_is_resolved_once_per_launch() {
        use std::time::Instant;
        // Base fixa (somar a um Instant nunca estoura; subtrair poderia, se o
        // processo tivesse acabado de subir).
        let base = Instant::now();
        let now = base + Duration::from_secs(60);
        let ttl = Duration::from_secs(20);

        // Sem entrada: resolve.
        assert_eq!(production_exe_memo_decision(None, now, ttl), None);

        // Sucesso fresco (build que existe): reusa a MESMA — é o que faz o patch
        // e o spawn concordarem.
        let exe = std::env::current_exe().expect("exe do teste existe");
        let ok = (now, Some(exe.clone()));
        assert_eq!(
            production_exe_memo_decision(Some(&ok), now, ttl),
            Some(Some(exe.clone()))
        );

        // Falha fresca (None): reusa a falha — não baixa de novo no mesmo launch.
        let failed = (now, None);
        assert_eq!(production_exe_memo_decision(Some(&failed), now, ttl), Some(None));

        // Entrada vencida: resolve de novo (próximo launch da leva).
        let stale = (base, None);
        assert_eq!(production_exe_memo_decision(Some(&stale), now, ttl), None);

        // Build guardada que sumiu do disco: resolve de novo.
        let gone = (now, Some(PathBuf::from(r"C:\nao\existe\RobloxPlayerBeta.exe")));
        assert_eq!(production_exe_memo_decision(Some(&gone), now, ttl), None);
    }

    const ROUTE: &str = "/v2/client-version/WindowsPlayer";

    fn build_answer(upload: &str) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "version": "0.700.0.7000839",
            "clientVersionUpload": upload,
            "bootstrapperVersion": "1, 6, 0, 7000839"
        }))
    }

    fn client() -> reqwest::Client {
        crate::api::http_client::builder_with(CHANNEL_LOOKUP_TIMEOUT, CHANNEL_LOOKUP_TIMEOUT)
            .build()
            .unwrap()
    }

    const FAST_PLAN: &[(usize, Duration)] = &[
        (0, Duration::ZERO),
        (1, Duration::ZERO),
        (0, Duration::from_millis(5)),
    ];

    #[tokio::test]
    async fn the_origin_server_answers_when_the_cdn_fails() {
        let cdn = MockServer::start().await;
        let origin = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(ROUTE))
            .respond_with(ResponseTemplate::new(503))
            .mount(&cdn)
            .await;
        Mock::given(method("GET"))
            .and(path(ROUTE))
            .respond_with(build_answer("version-origem"))
            .mount(&origin)
            .await;
        let urls = vec![format!("{}{ROUTE}", cdn.uri()), format!("{}{ROUTE}", origin.uri())];
        assert_eq!(
            fetch_channel_build_with_retry(&client(), &urls, PRODUCTION_CHANNEL, FAST_PLAN).await,
            Some(ChannelBuild::Channel("version-origem".into()))
        );
    }

    #[tokio::test]
    async fn a_second_try_on_the_cdn_rescues_a_transient_failure() {
        let cdn = MockServer::start().await;
        let origin = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(ROUTE))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&cdn)
            .await;
        Mock::given(method("GET"))
            .and(path(ROUTE))
            .respond_with(build_answer("version-cdn"))
            .mount(&cdn)
            .await;
        Mock::given(method("GET"))
            .and(path(ROUTE))
            .respond_with(ResponseTemplate::new(500))
            .mount(&origin)
            .await;
        let urls = vec![format!("{}{ROUTE}", cdn.uri()), format!("{}{ROUTE}", origin.uri())];
        assert_eq!(
            fetch_channel_build_with_retry(&client(), &urls, PRODUCTION_CHANNEL, FAST_PLAN).await,
            Some(ChannelBuild::Channel("version-cdn".into()))
        );
    }

    #[tokio::test]
    async fn every_host_failing_gives_nothing() {
        let cdn = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(ROUTE))
            .respond_with(ResponseTemplate::new(503))
            .mount(&cdn)
            .await;
        let urls = vec![format!("{}{ROUTE}", cdn.uri()), format!("{}{ROUTE}", cdn.uri())];
        assert_eq!(
            fetch_channel_build_with_retry(&client(), &urls, PRODUCTION_CHANNEL, FAST_PLAN).await,
            None
        );
        assert_eq!(cdn.received_requests().await.unwrap().len(), FAST_PLAN.len());
    }

    #[tokio::test]
    async fn an_origin_404_with_a_live_cdn_never_triggers_a_channel_repair() {
        // Crítico: canal de teste (old join). O CDN cai uma vez (transitório), o
        // servidor de origem responde 404 para esse canal — mas só o CDN é
        // autoritativo, então a origem NÃO pode pedir `RetiredUseProduction` (=
        // gravar `production` no registro). O CDN se recupera na 2ª tentativa e a
        // build dele vence. Com o bug, o 404 da origem virava o reparo.
        const CH: &str = "ztestchannel";
        let cdn = MockServer::start().await;
        let origin = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("{ROUTE}/channel/{CH}")))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&cdn)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("{ROUTE}/channel/{CH}")))
            .respond_with(build_answer("version-cdn"))
            .mount(&cdn)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("{ROUTE}/channel/{CH}")))
            .respond_with(ResponseTemplate::new(404))
            .mount(&origin)
            .await;
        Mock::given(method("GET"))
            .and(path(ROUTE))
            .respond_with(build_answer("version-producao"))
            .mount(&origin)
            .await;
        let urls = vec![
            format!("{}{ROUTE}", cdn.uri()),
            format!("{}{ROUTE}", origin.uri()),
        ];
        assert_eq!(
            fetch_channel_build_with_retry(&client(), &urls, CH, FAST_PLAN).await,
            Some(ChannelBuild::Channel("version-cdn".into())),
            "a origem 404 não pode pedir o reparo; a build do CDN vence"
        );
    }

    /// O último recurso cru (`cmd /C start roblox-player:...`) continua um só,
    /// dentro de `launch_url`.
    #[test]
    fn the_raw_handler_stays_the_single_last_resort() {
        let source = include_str!("launch.rs");
        let production = source.split("mod launch_url_tests").next().unwrap();
        assert_eq!(production.matches("\"start\"").count(), 1);
    }
}
