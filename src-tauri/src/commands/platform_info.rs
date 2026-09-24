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
            reasons: vec!["Botting Mode is only supported on Windows".to_string()],
            warnings: vec![
                "macOS support is partial: no per-client memory watch, no window grid and no pre-launch isolation".to_string(),
            ],
        },
        // Linux e qualquer outro alvo: não há backend de launch.
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
            reasons: vec!["Launching Roblox is only supported on Windows and macOS".to_string()],
            warnings: vec![
                "Account management works, but launching, the watcher and Botting Mode are unavailable on this platform".to_string(),
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
