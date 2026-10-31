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

#[tauri::command]
fn get_platform_capabilities(
    settings: tauri::State<'_, SettingsStore>,
) -> Result<PlatformCapabilities, String> {
    let os = std::env::consts::OS;
    Ok(build_platform_capabilities(
        os,
        &detect_session_type(os),
        &preferred_runner_setting(settings.inner()),
    ))
}
