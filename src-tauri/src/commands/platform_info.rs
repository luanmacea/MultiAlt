// O que este SO consegue fazer, do ponto de vista do frontend.
//
// `src/store.tsx` chama `get_platform_capabilities` no boot (e de novo quando
// uma chave de `[Linux]` muda) e usa o resultado para bloquear multi launch e
// botting fora do Windows, além de `isWindowsPlatform` (`src/utils/platform.ts`),
// que sem o campo `os` caía num palpite pelo user agent. O formato é o tipo
// `PlatformCapabilities` em `src/types.ts` — camelCase, sem campos opcionais.

/// Runner nativo: o app lança `RobloxPlayerBeta.exe` (ou o Roblox.app) direto.
const RUNNER_NATIVE: &str = "native";
/// Nenhum runner utilizável nesta plataforma.
const RUNNER_NONE: &str = "none";

/// Espelho de `PlatformCapabilities` em `src/types.ts`.
///
/// Todo campo é obrigatório do lado do TypeScript, então nada aqui pode ser
/// `#[serde(skip_serializing_if = ...)]`: um campo ausente vira `undefined` e
/// quebra as guardas do frontend em silêncio.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PlatformCapabilities {
    os: String,
    session_type: String,
    preferred_runner: String,
    detected_runner: String,
    runner_path: Option<String>,
    supports_single_launch: bool,
    supports_multi_launch: bool,
    supports_watcher: bool,
    supports_watcher_memory: bool,
    supports_window_controls: bool,
    supports_botting: bool,
    supports_updater: bool,
    supports_client_settings: bool,
    /// Volume ao vivo por cliente: só no Windows e só com a feature
    /// `live-audio` no binário (nas duas edições, via `standard`).
    supports_live_audio: bool,
    /// Teto de memória que libera RAM antes de fechar: só no Windows e só com
    /// a feature `memory-trim` (nas duas edições).
    supports_memory_trim: bool,
    /// Páginas e seções que só existem onde há Win32 (DWM, SendInput, registro,
    /// mutex do Roblox...). O frontend ESCONDE o que vem `false` — não só
    /// desabilita. Windows e macOS mandam `true` (a UI deles não muda); só o
    /// Linux manda `false`. Ver "Tela no Linux" em docs/features/linux.md.
    ///
    /// Janela "MultiAlt Live" (miniaturas do DWM).
    supports_live_view: bool,
    /// Tecla/clique periódico e gravações do AFK Mode (SendInput).
    supports_afk_input: bool,
    /// Isolamento pré-launch (registro, MachineGuid, MAC).
    supports_isolation: bool,
    /// Travas do Roblox (mutex/evento) do Multi Roblox.
    supports_multi_roblox_mutex: bool,
    /// Catálogo e instalador de versões do Roblox.
    supports_versions: bool,
    /// Otimização do processo (prioridade, memória, FastFlags do cliente).
    supports_optimization: bool,
    /// Clientes abertos pelo site, adotados pela varredura do log.
    supports_external_clients: bool,
    /// Automação do Roblox Player em Python (lê pixels da janela).
    supports_python: bool,
    reasons: Vec<String>,
    warnings: Vec<String>,
}

/// Monta as capacidades para um SO qualquer — função pura, é o que os testes
/// exercitam (o comando só coleta o ambiente e delega para cá).
///
/// - `os`: `std::env::consts::OS` (`windows`, `macos`, `linux`, ...), que é
///   exatamente o vocabulário que o frontend compara.
/// - `session_type`: `desktop` no Windows/macOS, `$XDG_SESSION_TYPE` no resto.
/// - `preferred_runner`: `[Linux] PreferredRunner` do INI (default `sober`);
///   ignorado onde o launch é nativo.
///
/// `reasons` explica por que algo está desligado (o frontend mostra
/// `reasons[0]` no toast de bloqueio); `warnings` guarda o que funciona só
/// parcialmente. No Windows ambos ficam vazios.
fn build_platform_capabilities(
    os: &str,
    session_type: &str,
    preferred_runner: &str,
) -> PlatformCapabilities {
    match os {
        // Plataforma primária: tudo suportado.
        "windows" => PlatformCapabilities {
            os: os.to_string(),
            session_type: session_type.to_string(),
            preferred_runner: RUNNER_NATIVE.to_string(),
            detected_runner: RUNNER_NATIVE.to_string(),
            runner_path: None,
            supports_single_launch: true,
            supports_multi_launch: true,
            supports_watcher: true,
            supports_watcher_memory: true,
            supports_window_controls: true,
            supports_botting: true,
            supports_updater: true,
            supports_client_settings: true,
            supports_live_audio: cfg!(feature = "live-audio"),
            supports_memory_trim: cfg!(feature = "memory-trim"),
            supports_live_view: true,
            supports_afk_input: true,
            supports_isolation: true,
            supports_multi_roblox_mutex: true,
            supports_versions: true,
            supports_optimization: true,
            supports_external_clients: true,
            supports_python: true,
            reasons: Vec::new(),
            warnings: Vec::new(),
        },
        // Suporte parcial: lança e vigia, mas sem memória do processo, sem
        // grid de janelas e sem botting (tudo isso é Win32).
        "macos" => PlatformCapabilities {
            os: os.to_string(),
            session_type: session_type.to_string(),
            preferred_runner: RUNNER_NATIVE.to_string(),
            detected_runner: RUNNER_NATIVE.to_string(),
            runner_path: None,
            supports_single_launch: true,
            supports_multi_launch: true,
            supports_watcher: true,
            supports_watcher_memory: false,
            supports_window_controls: false,
            supports_botting: false,
            supports_updater: true,
            supports_client_settings: true,
            supports_live_audio: false,
            supports_memory_trim: false,
            // A UI do macOS não muda neste porte: segue mostrando tudo.
            supports_live_view: true,
            supports_afk_input: true,
            supports_isolation: true,
            supports_multi_roblox_mutex: true,
            supports_versions: true,
            supports_optimization: true,
            supports_external_clients: true,
            supports_python: true,
            reasons: vec!["Auto Rejoin is only supported on Windows".to_string()],
            warnings: vec![
                "macOS support is partial: no per-client memory watch, no window grid and no pre-launch isolation".to_string(),
            ],
        },
        // Linux: o Roblox roda pelo Sober (Flatpak), que o app abre com
        // `unshare` + `pasta`. Lança uma ou várias contas; tudo o que depende de
        // Win32 fica desligado — e a UI esconde (não só desabilita).
        // `detected_runner` repete o configurado: quem sonda o Sober de verdade
        // preenche `runner_path` e troca isto pelo achado.
        "linux" => PlatformCapabilities {
            os: os.to_string(),
            session_type: session_type.to_string(),
            preferred_runner: preferred_runner.to_string(),
            detected_runner: preferred_runner.to_string(),
            runner_path: None,
            supports_single_launch: true,
            supports_multi_launch: true,
            supports_watcher: false,
            supports_watcher_memory: false,
            supports_window_controls: false,
            supports_botting: false,
            supports_updater: true,
            supports_client_settings: false,
            supports_live_audio: false,
            supports_memory_trim: false,
            supports_live_view: false,
            supports_afk_input: false,
            supports_isolation: false,
            supports_multi_roblox_mutex: false,
            supports_versions: false,
            supports_optimization: false,
            supports_external_clients: false,
            supports_python: false,
            reasons: vec!["Auto Rejoin is not available on Linux yet".to_string()],
            warnings: vec![
                "Linux runs Roblox through Sober: there is no window grid, Live view, AFK keys, isolation, Roblox versions or process tuning".to_string(),
            ],
        },
        // Qualquer outro alvo: não há backend de launch.
        _ => PlatformCapabilities {
            os: os.to_string(),
            session_type: session_type.to_string(),
            preferred_runner: preferred_runner.to_string(),
            detected_runner: RUNNER_NONE.to_string(),
            runner_path: None,
            supports_single_launch: false,
            supports_multi_launch: false,
            supports_watcher: false,
            supports_watcher_memory: false,
            supports_window_controls: false,
            supports_botting: false,
            supports_updater: true,
            supports_client_settings: false,
            supports_live_audio: false,
            supports_memory_trim: false,
            supports_live_view: false,
            supports_afk_input: false,
            supports_isolation: false,
            supports_multi_roblox_mutex: false,
            supports_versions: false,
            supports_optimization: false,
            supports_external_clients: false,
            supports_python: false,
            reasons: vec!["Launching Roblox is only supported on Windows, macOS and Linux".to_string()],
            warnings: vec![
                "Account management works, but launching, the watcher and Auto Rejoin are unavailable on this platform".to_string(),
            ],
        },
    }
}

/// Tipo de sessão gráfica. No Windows/macOS é sempre `desktop`; no resto vem de
/// `XDG_SESSION_TYPE` (`wayland`, `x11`, `tty`) e cai em `unknown` sem a var.
fn detect_session_type(os: &str) -> String {
    match os {
        "windows" | "macos" => "desktop".to_string(),
        _ => std::env::var("XDG_SESSION_TYPE")
            .ok()
            .map(|v| v.trim().to_lowercase())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "unknown".to_string()),
    }
}

/// Runner preferido do INI, com fallback para o default de `[Linux]`.
fn preferred_runner_setting(settings: &SettingsStore) -> String {
    let configured = settings.get_string("Linux", "PreferredRunner");
    let trimmed = configured.trim();
    if trimmed.is_empty() {
        "sober".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Linux: o que o ambiente tem de fato, por cima das capacidades fixas.
///
/// - runner `custom`: o programa é do usuário, então nada do Sober é cobrado;
/// - Sober sem Flatpak ou sem o Sober instalado: **bloqueia** o launch (vira
///   motivo em `reasons`, depois do motivo do Auto Rejoin, que o botting lê em
///   `reasons[0]`);
/// - sem `pasta`/`unshare`: só **aviso** — uma conta ainda abre sem rede
///   própria; a segunda ao mesmo tempo é que precisa do `passt`.
///
/// Pura (recebe o diagnóstico pronto) para os testes rodarem em qualquer SO.
fn apply_linux_setup(
    mut caps: PlatformCapabilities,
    custom_runner: bool,
    diagnostics: &platform::sober::Diagnostics,
) -> PlatformCapabilities {
    use platform::sober::SetupIssue;

    if custom_runner {
        caps.detected_runner = platform::sober::RUNNER_CUSTOM.to_string();
        caps.runner_path = None;
        return caps;
    }

    let mut blocked = false;
    for issue in diagnostics.issues() {
        match issue {
            SetupIssue::FlatpakMissing | SetupIssue::SoberMissing => {
                blocked = true;
                caps.reasons.push(issue.render());
            }
            SetupIssue::UnshareMissing | SetupIssue::PastaMissing => {
                caps.warnings.push(issue.render());
            }
        }
    }
    if blocked {
        caps.detected_runner = RUNNER_NONE.to_string();
        caps.runner_path = None;
        caps.supports_single_launch = false;
        caps.supports_multi_launch = false;
    } else {
        caps.detected_runner = platform::sober::RUNNER_SOBER.to_string();
        caps.runner_path = diagnostics
            .flatpak
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned());
    }
    caps
}

#[tauri::command]
fn get_platform_capabilities(
    settings: tauri::State<'_, SettingsStore>,
) -> Result<PlatformCapabilities, String> {
    let os = std::env::consts::OS;
    let caps = build_platform_capabilities(
        os,
        &detect_session_type(os),
        &preferred_runner_setting(settings.inner()),
    );
    #[cfg(target_os = "linux")]
    let caps = {
        let runner = platform::sober::plan_runner(&linux_runner_settings(settings.inner())).runner;
        apply_linux_setup(
            caps,
            matches!(runner, platform::sober::Runner::Custom(_)),
            &platform::linux::diagnose(),
        )
    };
    Ok(caps)
}

#[cfg(test)]
mod platform_info_tests {
    use super::*;

    /// Os campos que o TypeScript declara como obrigatórios.
    const REQUIRED_KEYS: &[&str] = &[
        "os",
        "sessionType",
        "preferredRunner",
        "detectedRunner",
        "runnerPath",
        "supportsSingleLaunch",
        "supportsMultiLaunch",
        "supportsWatcher",
        "supportsWatcherMemory",
        "supportsWindowControls",
        "supportsBotting",
        "supportsUpdater",
        "supportsClientSettings",
        "supportsLiveAudio",
        "supportsMemoryTrim",
        "supportsLiveView",
        "supportsAfkInput",
        "supportsIsolation",
        "supportsMultiRobloxMutex",
        "supportsVersions",
        "supportsOptimization",
        "supportsExternalClients",
        "supportsPython",
        "reasons",
        "warnings",
    ];

    #[test]
    fn windows_supports_everything_without_reasons_or_warnings() {
        let caps = build_platform_capabilities("windows", "desktop", "sober");
        assert!(caps.supports_single_launch);
        assert!(caps.supports_multi_launch);
        assert!(caps.supports_watcher);
        assert!(caps.supports_watcher_memory);
        assert!(caps.supports_window_controls);
        assert!(caps.supports_botting);
        assert!(caps.supports_updater);
        assert!(caps.supports_client_settings);
        assert!(caps.reasons.is_empty());
        assert!(caps.warnings.is_empty());
        // `PreferredRunner` é uma chave de `[Linux]`; no Windows o launch é
        // sempre nativo e a configuração não deve vazar para a UI.
        assert_eq!(caps.preferred_runner, RUNNER_NATIVE);
        assert_eq!(caps.detected_runner, RUNNER_NATIVE);
        assert_eq!(caps.runner_path, None);
    }

    #[test]
    fn live_audio_follows_the_build_feature_and_is_windows_only() {
        let windows = build_platform_capabilities("windows", "desktop", "sober");
        assert_eq!(windows.supports_live_audio, cfg!(feature = "live-audio"));
        // Nas duas edições: o build da padrão (`--features standard`) também tem.
        if cfg!(feature = "standard") {
            assert!(windows.supports_live_audio);
        }
        assert!(!build_platform_capabilities("macos", "desktop", "sober").supports_live_audio);
        assert!(!build_platform_capabilities("linux", "x11", "sober").supports_live_audio);
    }

    #[test]
    fn the_memory_ceiling_follows_the_build_feature_and_is_windows_only() {
        let windows = build_platform_capabilities("windows", "desktop", "sober");
        assert_eq!(windows.supports_memory_trim, cfg!(feature = "memory-trim"));
        // Só na edição completa: a padrão (`--features standard`) não leva.
        assert!(!build_platform_capabilities("macos", "desktop", "sober").supports_memory_trim);
        assert!(!build_platform_capabilities("linux", "x11", "sober").supports_memory_trim);
    }

    #[test]
    fn macos_launches_but_has_no_botting() {
        let caps = build_platform_capabilities("macos", "desktop", "sober");
        assert!(caps.supports_single_launch);
        assert!(caps.supports_multi_launch);
        assert!(caps.supports_client_settings);
        assert!(!caps.supports_botting);
        assert!(!caps.supports_watcher_memory);
        assert!(!caps.supports_window_controls);
        // O frontend mostra `reasons[0]` quando bloqueia o botting.
        assert!(!caps.reasons.is_empty());
        assert!(!caps.warnings.is_empty());
    }

    #[test]
    fn linux_launches_through_sober_but_not_the_win32_features() {
        let caps = build_platform_capabilities("linux", "wayland", "sober");
        assert_eq!(caps.os, "linux");
        assert_eq!(caps.session_type, "wayland");
        assert!(caps.supports_single_launch);
        assert!(caps.supports_multi_launch);
        assert!(caps.supports_updater);
        // Sem backend de vigia/Auto Rejoin por enquanto: as guardas de
        // `startBottingMode` leem `reasons[0]`; uma lista vazia faria o
        // frontend mostrar um texto genérico.
        assert!(!caps.supports_botting);
        assert!(!caps.supports_watcher);
        assert!(!caps.reasons[0].is_empty());
        assert!(!caps.warnings.is_empty());
        // O runner configurado é ecoado de volta para a UI poder mostrá-lo.
        assert_eq!(caps.preferred_runner, "sober");
        assert_eq!(caps.detected_runner, "sober");
    }

    fn complete_setup() -> platform::sober::Diagnostics {
        platform::sober::Diagnostics {
            flatpak: Some("/usr/bin/flatpak".into()),
            sober_installed: true,
            unshare: Some("/usr/bin/unshare".into()),
            pasta: Some("/usr/bin/pasta".into()),
        }
    }

    #[test]
    fn linux_with_everything_installed_reports_sober_and_its_path() {
        let base = build_platform_capabilities("linux", "x11", "sober");
        let caps = apply_linux_setup(base.clone(), false, &complete_setup());
        assert_eq!(caps.detected_runner, "sober");
        assert_eq!(caps.runner_path.as_deref(), Some("/usr/bin/flatpak"));
        assert!(caps.supports_single_launch);
        assert!(caps.supports_multi_launch);
        assert_eq!(caps.reasons, base.reasons);
        assert_eq!(caps.warnings, base.warnings);
    }

    #[test]
    fn linux_without_flatpak_or_sober_blocks_the_launch_with_a_reason() {
        let base = build_platform_capabilities("linux", "x11", "sober");
        for diagnostics in [
            platform::sober::Diagnostics { flatpak: None, ..complete_setup() },
            platform::sober::Diagnostics { sober_installed: false, ..complete_setup() },
        ] {
            let caps = apply_linux_setup(base.clone(), false, &diagnostics);
            assert!(!caps.supports_single_launch);
            assert!(!caps.supports_multi_launch);
            assert_eq!(caps.detected_runner, RUNNER_NONE);
            assert_eq!(caps.runner_path, None);
            // O motivo do Auto Rejoin segue em `reasons[0]`.
            assert_eq!(caps.reasons[0], base.reasons[0]);
            assert_eq!(caps.reasons.len(), base.reasons.len() + 1);
        }
    }

    #[test]
    fn linux_without_passt_only_warns_and_still_launches() {
        let base = build_platform_capabilities("linux", "x11", "sober");
        let diagnostics = platform::sober::Diagnostics { pasta: None, ..complete_setup() };
        let caps = apply_linux_setup(base.clone(), false, &diagnostics);
        assert!(caps.supports_single_launch);
        assert!(caps.supports_multi_launch);
        assert_eq!(caps.detected_runner, "sober");
        assert_eq!(caps.reasons, base.reasons);
        assert_eq!(caps.warnings.len(), base.warnings.len() + 1);
        assert!(caps.warnings.last().unwrap().contains("passt"));
    }

    #[test]
    fn the_custom_runner_is_not_asked_for_sober() {
        let base = build_platform_capabilities("linux", "x11", "custom");
        let caps = apply_linux_setup(base.clone(), true, &platform::sober::Diagnostics::default());
        assert_eq!(caps.detected_runner, "custom");
        assert_eq!(caps.runner_path, None);
        assert!(caps.supports_single_launch);
        assert_eq!(caps.reasons, base.reasons);
        assert_eq!(caps.warnings, base.warnings);
    }

    #[test]
    fn linux_hides_every_win32_only_feature() {
        let caps = build_platform_capabilities("linux", "x11", "sober");
        assert!(!caps.supports_window_controls);
        assert!(!caps.supports_live_view);
        assert!(!caps.supports_afk_input);
        assert!(!caps.supports_isolation);
        assert!(!caps.supports_multi_roblox_mutex);
        assert!(!caps.supports_versions);
        assert!(!caps.supports_optimization);
        assert!(!caps.supports_client_settings);
        assert!(!caps.supports_live_audio);
        assert!(!caps.supports_memory_trim);
        assert!(!caps.supports_external_clients);
        assert!(!caps.supports_python);
    }

    #[test]
    fn windows_and_macos_keep_showing_every_page() {
        for os in ["windows", "macos"] {
            let caps = build_platform_capabilities(os, "desktop", "sober");
            assert!(caps.supports_live_view, "{os}");
            assert!(caps.supports_afk_input, "{os}");
            assert!(caps.supports_isolation, "{os}");
            assert!(caps.supports_multi_roblox_mutex, "{os}");
            assert!(caps.supports_versions, "{os}");
            assert!(caps.supports_optimization, "{os}");
            assert!(caps.supports_external_clients, "{os}");
            assert!(caps.supports_python, "{os}");
        }
    }

    #[test]
    fn an_unknown_os_cannot_launch() {
        let caps = build_platform_capabilities("freebsd", "x11", "custom");
        assert_eq!(caps.os, "freebsd");
        assert!(!caps.supports_single_launch);
        assert!(!caps.supports_multi_launch);
        assert!(!caps.supports_live_view);
        assert_eq!(caps.preferred_runner, "custom");
        assert_eq!(caps.detected_runner, RUNNER_NONE);
        assert!(!caps.reasons[0].is_empty());
    }

    #[test]
    fn the_payload_carries_every_key_the_frontend_type_declares() {
        for os in ["windows", "macos", "linux", "freebsd"] {
            let json = serde_json::to_value(build_platform_capabilities(os, "desktop", "sober"))
                .expect("capabilities should serialize");
            let object = json.as_object().expect("capabilities should be an object");
            for key in REQUIRED_KEYS {
                assert!(object.contains_key(*key), "{os} is missing {key}");
            }
            assert_eq!(object.len(), REQUIRED_KEYS.len(), "{os} has extra keys");
            // `runnerPath` é `string | null` no TypeScript: nunca pode sumir.
            assert!(object["runnerPath"].is_null() || object["runnerPath"].is_string());
            assert!(object["reasons"].is_array());
            assert!(object["warnings"].is_array());
        }
    }

    #[test]
    fn session_type_is_desktop_on_the_native_platforms() {
        assert_eq!(detect_session_type("windows"), "desktop");
        assert_eq!(detect_session_type("macos"), "desktop");
    }

    #[test]
    fn the_command_describes_the_machine_it_runs_on() {
        let caps = build_platform_capabilities(
            std::env::consts::OS,
            &detect_session_type(std::env::consts::OS),
            "sober",
        );
        assert_eq!(caps.os, std::env::consts::OS);
        assert!(!caps.session_type.is_empty());
        // Suporte de launch e de botting nunca podem discordar para mais:
        // botting lança clientes.
        assert!(caps.supports_single_launch || !caps.supports_botting);
        assert!(caps.supports_single_launch || !caps.supports_multi_launch);
    }
}
