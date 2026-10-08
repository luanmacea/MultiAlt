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

/// Edição deste binário, no vocabulário do canal de recursos do updater.
///
/// Sai das features de compilação, não da setting: a setting
/// `General.UpdaterFeatureChannel` diz a edição que a pessoa **quer**; isto diz
/// a que está rodando. A completa é a que o release compila com
/// `--features nexus,webserver,avatar-batch`.
pub(crate) const RUNNING_FEATURE_CHANNEL: &str =
    if cfg!(all(feature = "nexus", feature = "webserver", feature = "avatar-batch")) {
        "nexus-ws"
    } else {
        "standard"
    };

/// A versão do manifesto conta como atualização?
///
/// Normalmente só uma versão maior. A exceção é a troca de edição pedida pela
/// pessoa (`allow_edition_switch`, só nas checagens manuais): se o canal pedido
/// é de outra edição, a **mesma** versão também vale — o que se baixa é o
/// instalador da outra edição. Versão menor nunca. A assinatura do pacote
/// continua sendo conferida no download/instalação, como em qualquer update.
fn update_is_offered(
    current: &semver::Version,
    remote: &semver::Version,
    running_feature_channel: &str,
    wanted_feature_channel: &str,
    allow_edition_switch: bool,
) -> bool {
    if allow_edition_switch && running_feature_channel != wanted_feature_channel {
        remote >= current
    } else {
        remote > current
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
    allow_edition_switch: bool,
) -> Result<Option<PendingUpdate>, String> {
    use tauri_plugin_updater::UpdaterExt;

    let endpoint = build_manifest_endpoint(release_channel, feature_channel)?;
    let wanted_feature_channel = feature_channel.to_string();
    let updater = app
        .updater_builder()
        .version_comparator(move |current, release| {
            update_is_offered(
                &current,
                &release.version,
                RUNNING_FEATURE_CHANNEL,
                &wanted_feature_channel,
                allow_edition_switch,
            )
        })
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
    // Só a checagem manual manda `true` (ver `update_is_offered`).
    allow_edition_switch: Option<bool>,
) -> Result<Option<UpdaterCheckResponse>, String> {
    let allow_edition_switch = allow_edition_switch.unwrap_or(false);
    let normalized_release = normalize_updater_release_channel(
        release_channel.as_deref().unwrap_or("beta"),
    );
    let normalized_feature = normalize_updater_feature_channel(
        feature_channel.as_deref().unwrap_or("standard"),
    );

    let primary_update = primary_channel_result(
        check_update_for_channel(&app, normalized_release, normalized_feature, allow_edition_switch).await,
    )?;
    // Quem esta no beta tambem olha o stable: uma estavel mais nova vence a
    // beta. Mas o stable so nasce na primeira release estavel, e ate la ele
    // responde 404 — por isso a falha dele nao pode derrubar a checagem.
    let fallback_update = if normalized_release == "beta" {
        fallback_channel_result(
            check_update_for_channel(&app, "stable", normalized_feature, allow_edition_switch).await,
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

    // ---- troca de edição -----------------------------------------------------

    fn v(raw: &str) -> semver::Version {
        semver::Version::parse(raw).unwrap()
    }

    #[test]
    fn switching_to_the_complete_edition_offers_the_same_version() {
        // O manifesto `stable-nexus-ws` sai com a mesma versão do `stable`: sem
        // isto, quem pede a edição completa ouviria "nada para atualizar".
        assert!(update_is_offered(&v("1.4.0"), &v("1.4.0"), "standard", "nexus-ws", true));
    }

    #[test]
    fn switching_back_to_the_standard_edition_offers_the_same_version() {
        assert!(update_is_offered(&v("1.4.0"), &v("1.4.0"), "nexus-ws", "standard", true));
    }

    #[test]
    fn the_same_edition_at_the_same_version_stays_up_to_date() {
        assert!(!update_is_offered(&v("1.4.0"), &v("1.4.0"), "standard", "standard", true));
        assert!(!update_is_offered(&v("1.4.0"), &v("1.4.0"), "nexus-ws", "nexus-ws", true));
    }

    #[test]
    fn the_automatic_check_never_switches_edition_at_the_same_version() {
        // A checagem do boot não pede troca: uma setting antiga apontando para a
        // outra edição não vira aviso de atualização a cada abertura.
        assert!(!update_is_offered(&v("1.4.0"), &v("1.4.0"), "standard", "nexus-ws", false));
        assert!(update_is_offered(&v("1.4.0"), &v("1.5.0"), "standard", "nexus-ws", false));
    }

    #[test]
    fn an_edition_switch_never_offers_an_older_version() {
        assert!(!update_is_offered(&v("1.4.0"), &v("1.3.9"), "standard", "nexus-ws", true));
        assert!(update_is_offered(&v("1.4.0"), &v("1.4.1"), "standard", "nexus-ws", true));
    }

    #[test]
    fn the_running_edition_comes_from_the_compiled_features() {
        let complete = cfg!(all(feature = "nexus", feature = "webserver", feature = "avatar-batch"));
        assert_eq!(RUNNING_FEATURE_CHANNEL, if complete { "nexus-ws" } else { "standard" });
        // É um dos dois canais que o updater conhece.
        assert_eq!(normalize_updater_feature_channel(RUNNING_FEATURE_CHANNEL), RUNNING_FEATURE_CHANNEL);
    }

    // ---- falha de canal ------------------------------------------------------

    /// O que o `check_update_for_channel` devolve quando o canal responde 404.
    const SEM_MANIFESTO: &str =
        "Failed to check for updates: Could not fetch a valid release JSON from the remote";

    #[test]
    fn a_channel_without_a_published_manifest_is_not_a_failure() {
        // O canal `stable` so nasce na primeira release estavel. Ate la ele
        // responde 404, e isso e "nada para atualizar", nao "a checagem falhou".
        assert_eq!(primary_channel_result::<u8>(Err(SEM_MANIFESTO.into())), Ok(None));
    }

    #[test]
    fn a_real_failure_in_the_primary_channel_still_surfaces() {
        let erro = "Failed to check for updates: error sending request".to_string();
        assert_eq!(primary_channel_result::<u8>(Err(erro.clone())), Err(erro));
    }

    #[test]
    fn the_fallback_channel_never_breaks_the_check() {
        // Bug vivido (28/09/2026): no canal beta o app consultava tambem o
        // `stable`, que nunca teve release, e subia o 404 — entao "Procurar
        // agora" falhava sempre, mesmo sem nada para atualizar.
        assert_eq!(fallback_channel_result::<u8>(Err(SEM_MANIFESTO.into())), Ok(None));
        assert_eq!(
            fallback_channel_result::<u8>(Err("Failed to check for updates: timed out".into())),
            Ok(None)
        );
    }

    #[test]
    fn both_channels_pass_their_update_through() {
        assert_eq!(primary_channel_result(Ok(Some(7u8))), Ok(Some(7)));
        assert_eq!(fallback_channel_result(Ok(Some(7u8))), Ok(Some(7)));
        assert_eq!(fallback_channel_result::<u8>(Ok(None)), Ok(None));
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

    // ---- progresso do download ----------------------------------------------

    #[test]
    fn download_progress_emits_the_first_chunk_then_waits_for_the_interval() {
        let start = std::time::Instant::now();
        let mut throttle = DownloadProgressThrottle::default();
        assert!(throttle.should_emit(start, false));
        assert!(!throttle.should_emit(start + std::time::Duration::from_millis(50), false));
        assert!(throttle.should_emit(start + DOWNLOAD_PROGRESS_INTERVAL, false));
    }

    #[test]
    fn download_progress_always_emits_the_last_chunk() {
        let start = std::time::Instant::now();
        let mut throttle = DownloadProgressThrottle::default();
        assert!(throttle.should_emit(start, false));
        assert!(throttle.should_emit(start + std::time::Duration::from_millis(1), true));
    }

    // ---- instalacao silenciosa ----------------------------------------------

    #[test]
    fn the_msi_install_writes_a_verbose_log_to_the_given_path() {
        let path = std::path::Path::new(r"C:\Users\a b\RAM\RAMUpdateInstall.log");
        let args = update_installer_args(Some(tauri::utils::config::BundleType::Msi), path);
        assert_eq!(
            args,
            vec![
                "/l*v".to_string(),
                r#""C:\Users\a b\RAM\RAMUpdateInstall.log""#.to_string(),
            ]
        );
    }

    #[test]
    fn only_the_msi_install_gets_the_log_arguments() {
        let path = std::path::Path::new(r"C:\log.txt");
        assert!(update_installer_args(Some(tauri::utils::config::BundleType::Nsis), path).is_empty());
        assert!(update_installer_args(None, path).is_empty());
    }
}
