/// Onde o app procura o manifesto de atualizacao.
///
/// **Este repositorio, nao o original.** Ate aqui o app apontava para
/// `niccsprojects/Roblox-Account-Manager`, de onde este projeto foi bifurcado:
/// o banner "atualizacao disponivel" anunciava a versao **do outro projeto** e,
/// se instalada, substituiria este app pelo binario de la. Nao era so um botao
/// sobrando — era um caminho para perder o proprio app.
///
/// O manifesto fica no branch `update-manifests`, um arquivo por canal:
/// `update-manifests/<canal>/latest.json`. O canal vem de
/// [`resolve_manifest_channel`] (`stable`, `beta`, `stable-nexus-ws`,
/// `beta-nexus-ws`). Quem publica e o workflow `.github/workflows/release.yml`.
const UPDATER_MANIFEST_BASE: &str =
    "https://raw.githubusercontent.com/luanmacea/MultiAlt/update-manifests";

type PendingUpdate = (tauri_plugin_updater::Update, String, String);

#[derive(Default)]
struct UpdaterRuntimeState {
    pending_update: std::sync::Mutex<Option<PendingUpdate>>,
    downloaded_bytes: std::sync::Mutex<Option<(String, Vec<u8>)>>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdaterCheckResponse {
    version: String,
    current_version: String,
    date: String,
    body: String,
    release_channel: String,
    feature_channel: String,
}

fn normalize_updater_release_channel(raw: &str) -> &'static str {
    if raw.eq_ignore_ascii_case("stable") {
        "stable"
    } else {
        "beta"
    }
}

fn normalize_updater_feature_channel(raw: &str) -> &'static str {
    if raw.eq_ignore_ascii_case("nexus-ws")
        || raw.eq_ignore_ascii_case("nexus")
        || raw.eq_ignore_ascii_case("full")
    {
        "nexus-ws"
    } else {
        "standard"
    }
}

fn resolve_manifest_channel(release_channel: &str, feature_channel: &str) -> String {
    if feature_channel == "nexus-ws" {
        format!("{}-nexus-ws", release_channel)
    } else {
        release_channel.to_string()
    }
}

fn build_manifest_endpoint(release_channel: &str, feature_channel: &str) -> Result<reqwest::Url, String> {
    let channel = resolve_manifest_channel(release_channel, feature_channel);
    let endpoint = format!("{}/{}/latest.json", UPDATER_MANIFEST_BASE, channel);
    reqwest::Url::parse(&endpoint).map_err(|e| format!("Invalid updater endpoint: {}", e))
}

fn update_payload_key(update: Option<&PendingUpdate>) -> Option<String> {
    update.map(|(item, release_channel, feature_channel)| {
        format!(
            "{}|{}|{}|{}|{}|{}",
            item.version,
            item.target,
            item.download_url,
            item.signature,
            release_channel,
            feature_channel
        )
    })
}

fn parse_semver(raw: &str) -> Option<semver::Version> {
    semver::Version::parse(raw.trim().trim_start_matches('v')).ok()
}

fn select_preferred_update(
    primary: Option<PendingUpdate>,
    fallback: Option<PendingUpdate>,
) -> Option<PendingUpdate> {
    match (primary, fallback) {
        (None, None) => None,
        (Some(primary), None) => Some(primary),
        (None, Some(fallback)) => Some(fallback),
        (Some(primary), Some(fallback)) => {
            let primary_version = parse_semver(&primary.0.version);
            let fallback_version = parse_semver(&fallback.0.version);

            match (primary_version, fallback_version) {
                (Some(left), Some(right)) if right > left => Some(fallback),
                _ => Some(primary),
            }
        }
    }
}

/// Canal que ainda nao tem manifesto publicado (o endpoint responde 404).
///
/// O texto e o `Display` de `tauri_plugin_updater::Error::ReleaseNotFound`
/// (conferido no 2.10), que o plugin devolve tanto para 404 quanto para
/// resposta ilegivel.
fn channel_has_no_manifest(error: &str) -> bool {
    error.contains("Could not fetch a valid release JSON from the remote")
}

/// Canal principal: canal ainda sem release publicada e "nada para atualizar",
/// nao falha. Qualquer outro erro sobe.
fn primary_channel_result<T>(result: Result<Option<T>, String>) -> Result<Option<T>, String> {
    match result {
        Err(error) if channel_has_no_manifest(&error) => Ok(None),
        other => other,
    }
}

/// Canal de reserva (o `stable` consultado de dentro do beta): um extra que
/// nunca pode derrubar a checagem do canal principal.
fn fallback_channel_result<T>(result: Result<Option<T>, String>) -> Result<Option<T>, String> {
    match result {
        Ok(update) => Ok(update),
        Err(error) => {
            eprintln!("Updater: canal de reserva ignorado ({})", error);
            Ok(None)
        }
    }
}

async fn check_update_for_channel(
    app: &tauri::AppHandle,
    release_channel: &str,
    feature_channel: &str,
) -> Result<Option<PendingUpdate>, String> {
    use tauri_plugin_updater::UpdaterExt;

    let endpoint = build_manifest_endpoint(release_channel, feature_channel)?;
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|e| format!("Failed to configure updater endpoint: {}", e))?
        .installer_args(update_installer_args(
            tauri::utils::platform::bundle_type(),
            &update_install_log_path(),
        ))
        .build()
        .map_err(|e| format!("Failed to initialize updater: {}", e))?;

    let update = updater
        .check()
        .await
        .map_err(|e| format!("Failed to check for updates: {}", e))?;

    Ok(update.map(|item| {
        (
            item,
            release_channel.to_string(),
            feature_channel.to_string(),
        )
    }))
}

#[tauri::command]
async fn check_for_updates_with_channels(
    app: tauri::AppHandle,
    updater_state: tauri::State<'_, UpdaterRuntimeState>,
    release_channel: Option<String>,
    feature_channel: Option<String>,
) -> Result<Option<UpdaterCheckResponse>, String> {
    let normalized_release = normalize_updater_release_channel(
        release_channel.as_deref().unwrap_or("beta"),
    );
    let normalized_feature = normalize_updater_feature_channel(
        feature_channel.as_deref().unwrap_or("standard"),
    );

    let primary_update = primary_channel_result(
        check_update_for_channel(&app, normalized_release, normalized_feature).await,
    )?;
    // Quem esta no beta tambem olha o stable: uma estavel mais nova vence a
    // beta. Mas o stable so nasce na primeira release estavel, e ate la ele
    // responde 404 — por isso a falha dele nao pode derrubar a checagem.
    let fallback_update = if normalized_release == "beta" {
        fallback_channel_result(
            check_update_for_channel(&app, "stable", normalized_feature).await,
        )?
    } else {
        None
    };
    let update = select_preferred_update(primary_update, fallback_update);

    let previous_payload_key = {
        let pending = updater_state
            .pending_update
            .lock()
            .map_err(|e| e.to_string())?;
        update_payload_key(pending.as_ref())
    };

    let next_payload_key = update_payload_key(update.as_ref());

    if previous_payload_key != next_payload_key {
        let mut downloaded = updater_state
            .downloaded_bytes
            .lock()
            .map_err(|e| e.to_string())?;
        *downloaded = None;
    }

    {
        let mut pending = updater_state
            .pending_update
            .lock()
            .map_err(|e| e.to_string())?;
        *pending = update.clone();
    }

    Ok(update.map(|(item, resolved_release_channel, resolved_feature_channel)| UpdaterCheckResponse {
        version: item.version,
        current_version: item.current_version,
        date: item.date.map(|d| d.to_string()).unwrap_or_default(),
        body: item.body.unwrap_or_default(),
        release_channel: resolved_release_channel,
        feature_channel: resolved_feature_channel,
    }))
}

/// De quanto em quanto tempo o download avisa a tela. O plugin chama o callback
/// a cada pedaco recebido (centenas por segundo); a barra so precisa de alguns.
const DOWNLOAD_PROGRESS_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

#[derive(Default)]
struct DownloadProgressThrottle {
    last_emit: Option<std::time::Instant>,
}

impl DownloadProgressThrottle {
    /// O primeiro pedaco e o ultimo sempre passam; no meio, um a cada intervalo.
    fn should_emit(&mut self, now: std::time::Instant, finished: bool) -> bool {
        let due = match self.last_emit {
            None => true,
            Some(last) => now.duration_since(last) >= DOWNLOAD_PROGRESS_INTERVAL,
        };
        if due || finished {
            self.last_emit = Some(now);
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateDownloadProgress {
    downloaded: u64,
    total: Option<u64>,
}

/// Argumentos extras do instalador da atualizacao.
///
/// A instalacao roda em silencio (`installMode: quiet` no tauri.conf.json): sem a
/// janela nativa do MSI, que parecia "baixar de novo" depois do download do app.
/// Sem janela, um erro tambem nao aparece — por isso o MSI grava um log detalhado
/// na pasta de dados. O NSIS nao entende `/l*v`, entao so o MSI o recebe.
fn update_installer_args(
    bundle: Option<tauri::utils::config::BundleType>,
    log_path: &std::path::Path,
) -> Vec<String> {
    match bundle {
        Some(tauri::utils::config::BundleType::Msi) => vec![
            "/l*v".to_string(),
            format!("\"{}\"", log_path.display()),
        ],
        _ => Vec::new(),
    }
}

fn update_install_log_path() -> std::path::PathBuf {
    crate::data::settings::get_runtime_data_dir().join("RAMUpdateInstall.log")
}

#[tauri::command]
async fn download_selected_update(
    app: tauri::AppHandle,
    updater_state: tauri::State<'_, UpdaterRuntimeState>,
) -> Result<(), String> {
    let pending_update = {
        let pending = updater_state
            .pending_update
            .lock()
            .map_err(|e| e.to_string())?;
        pending
            .clone()
            .ok_or_else(|| "No pending update selected".to_string())?
    };
    let (update, _, _) = pending_update.clone();

    let payload_key =
        update_payload_key(Some(&pending_update)).ok_or_else(|| "No pending update selected".to_string())?;

    let mut downloaded_so_far: u64 = 0;
    let mut throttle = DownloadProgressThrottle::default();
    let bytes = update
        .download(
            |chunk, total| {
                downloaded_so_far += chunk as u64;
                let finished = total.is_some_and(|t| downloaded_so_far >= t);
                if throttle.should_emit(std::time::Instant::now(), finished) {
                    let _ = app.emit(
                        "update-download-progress",
                        UpdateDownloadProgress {
                            downloaded: downloaded_so_far,
                            total,
                        },
                    );
                }
            },
            || {},
        )
        .await
        .map_err(|e| format!("Failed to download update: {}", e))?;

    let mut downloaded = updater_state
        .downloaded_bytes
        .lock()
        .map_err(|e| e.to_string())?;
    *downloaded = Some((payload_key, bytes));

    Ok(())
}

#[tauri::command]
fn install_selected_update(updater_state: tauri::State<'_, UpdaterRuntimeState>) -> Result<(), String> {
    let pending_update = {
        let pending = updater_state
            .pending_update
            .lock()
            .map_err(|e| e.to_string())?;
        pending
            .clone()
            .ok_or_else(|| "No pending update selected".to_string())?
    };
    let (update, _, _) = pending_update.clone();

    let pending_payload_key =
        update_payload_key(Some(&pending_update)).ok_or_else(|| "No pending update selected".to_string())?;

    let (downloaded_payload_key, bytes) = {
        let downloaded = updater_state
            .downloaded_bytes
            .lock()
            .map_err(|e| e.to_string())?;
        downloaded
            .clone()
            .ok_or_else(|| "No downloaded update found".to_string())?
    };

    if downloaded_payload_key != pending_payload_key {
        return Err(
            "Downloaded update no longer matches selected channel/version. Download again.".into(),
        );
    }

    update
        .install(bytes)
        .map_err(|e| format!("Failed to install update: {}", e))
}
