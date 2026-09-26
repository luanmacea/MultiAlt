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
    "https://raw.githubusercontent.com/luanmacea/roblox-account-manager/update-manifests";

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

    let primary_update = check_update_for_channel(&app, normalized_release, normalized_feature).await?;
    let fallback_update = if normalized_release == "beta" {
        check_update_for_channel(&app, "stable", normalized_feature).await?
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

#[tauri::command]
async fn download_selected_update(
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

    let bytes = update
        .download(|_, _| {}, || {})
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

#[cfg(test)]
mod updater_tests {
    use super::*;

    // ---- channel normalization ----------------------------------------------

    #[test]
    fn release_channel_normalizes_to_stable_only_for_stable() {
        assert_eq!(normalize_updater_release_channel("stable"), "stable");
        assert_eq!(normalize_updater_release_channel("STABLE"), "stable");
        assert_eq!(normalize_updater_release_channel("Stable"), "stable");
    }

    #[test]
    fn release_channel_defaults_to_beta_for_anything_else() {
        assert_eq!(normalize_updater_release_channel("beta"), "beta");
        assert_eq!(normalize_updater_release_channel(""), "beta");
        assert_eq!(normalize_updater_release_channel("nightly"), "beta");
        assert_eq!(normalize_updater_release_channel("  stable  "), "beta");
    }

    #[test]
    fn feature_channel_accepts_the_three_nexus_aliases() {
        assert_eq!(normalize_updater_feature_channel("nexus-ws"), "nexus-ws");
        assert_eq!(normalize_updater_feature_channel("NEXUS"), "nexus-ws");
        assert_eq!(normalize_updater_feature_channel("Full"), "nexus-ws");
    }

    #[test]
    fn feature_channel_defaults_to_standard() {
        assert_eq!(normalize_updater_feature_channel("standard"), "standard");
        assert_eq!(normalize_updater_feature_channel(""), "standard");
        assert_eq!(normalize_updater_feature_channel("whatever"), "standard");
    }

    // ---- manifest channel / endpoint ----------------------------------------

    #[test]
    fn resolve_manifest_channel_appends_the_nexus_suffix() {
        assert_eq!(resolve_manifest_channel("beta", "nexus-ws"), "beta-nexus-ws");
        assert_eq!(
            resolve_manifest_channel("stable", "nexus-ws"),
            "stable-nexus-ws"
        );
        assert_eq!(resolve_manifest_channel("beta", "standard"), "beta");
        assert_eq!(resolve_manifest_channel("stable", "standard"), "stable");
    }

    #[test]
    fn build_manifest_endpoint_points_at_the_channel_manifest() {
        let url = build_manifest_endpoint("stable", "standard").unwrap();
        assert_eq!(
            url.as_str(),
            format!("{}/stable/latest.json", UPDATER_MANIFEST_BASE)
        );

        let nexus = build_manifest_endpoint("beta", "nexus-ws").unwrap();
        assert_eq!(
            nexus.as_str(),
            format!("{}/beta-nexus-ws/latest.json", UPDATER_MANIFEST_BASE)
        );
        assert_eq!(nexus.scheme(), "https");
        assert_eq!(nexus.host_str(), Some("raw.githubusercontent.com"));
    }

    /// O app ja apontou para o repositorio de onde este projeto foi bifurcado.
    /// O banner anunciava a versao do outro projeto e instalar substituiria
    /// este app pelo binario de la — com assinatura valida, porque a chave
    /// publica tambem era de la. Este teste existe para isso nao voltar por
    /// descuido num merge.
    #[test]
    fn the_updater_never_points_at_the_upstream_project() {
        let url = build_manifest_endpoint("stable", "standard").unwrap();
        let alvo = url.as_str().to_ascii_lowercase();
        assert!(
            !alvo.contains("niccsprojects"),
            "o updater voltou a apontar para o projeto original: {alvo}"
        );
        assert!(
            alvo.contains("/luanmacea/"),
            "o manifesto tem que vir deste repositorio: {alvo}"
        );
    }

    // ---- parse_semver --------------------------------------------------------

    #[test]
    fn parse_semver_strips_the_v_prefix_and_whitespace() {
        assert_eq!(
            parse_semver("  v4.1.0  "),
            Some(semver::Version::parse("4.1.0").unwrap())
        );
        assert_eq!(
            parse_semver("4.1.0"),
            Some(semver::Version::parse("4.1.0").unwrap())
        );
    }

    #[test]
    fn parse_semver_returns_none_for_invalid_input() {
        assert_eq!(parse_semver(""), None);
        assert_eq!(parse_semver("not-a-version"), None);
        assert_eq!(parse_semver("4.1"), None);
    }

    #[test]
    fn parse_semver_orders_numerically_not_lexicographically() {
        // This ordering is what select_preferred_update relies on to pick the
        // newer of the primary/fallback manifests.
        let older = parse_semver("v4.9.9").unwrap();
        let newer = parse_semver("v4.10.0").unwrap();
        assert!(newer > older);

        // Pre-releases sort below the matching release.
        assert!(parse_semver("4.1.0").unwrap() > parse_semver("4.1.0-beta.1").unwrap());
    }

    // ---- select_preferred_update / update_payload_key ------------------------

    #[test]
    fn select_preferred_update_returns_none_when_no_channel_has_an_update() {
        // `tauri_plugin_updater::Update` has private fields and cannot be built
        // in a unit test, so only the empty case is exercised here; the
        // higher-semver preference is covered by parse_semver's ordering test.
        assert!(select_preferred_update(None, None).is_none());
    }

    #[test]
    fn update_payload_key_is_none_without_a_pending_update() {
        assert_eq!(update_payload_key(None), None);
    }
}
