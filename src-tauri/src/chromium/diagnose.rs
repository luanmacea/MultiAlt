// "O navegador de login não faz nada" — o mesmo checklist do launch
// (commands/diagnostics.rs), para o Chromium do login e da criação de contas.
// Relato de um usuário: o navegador não fazia nada ao criar contas, e voltou a
// funcionar depois de desinstalar e instalar de novo o Google Chrome — era o
// Chrome do PC que o app usava (o download do Chromium próprio tinha falhado,
// `resolve_browser_binary` caiu no navegador do sistema). Ver
// docs/features/chromium.md ("Checagem do navegador").
//
// Como no launch, o backend devolve só `id` + `reason` (+ `count`): nenhum
// caminho nem nome de programa chega à tela, e a frase sai traduzida de
// src/utils/diagnostics.ts. Nunca baixa nada (o download fica para o botão
// Reinstall) e nunca toca no perfil de login nem nos perfis das contas: o teste
// de abertura usa um perfil próprio, apagado no fim.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tauri::{AppHandle, State};

use super::cdp::{spawn_chrome, CdpClient};
use super::download::{cached_binary, find_system_chromium, has_damaged_download, resolve_manual_path};
use super::manager::ChromiumManager;
use crate::data::settings::SettingsStore;
use crate::{CheckStatus, DiagnosticCheck, FolderProbe};

/// Qual navegador o login usaria agora.
#[derive(Debug, Clone, PartialEq, Eq)]
enum BinaryState {
    /// `Login.ManualBinaryPath` preenchido; `valid` = é um arquivo de verdade.
    Manual { valid: bool },
    /// A cópia própria baixada, inteira.
    Downloaded,
    /// A cópia própria existe, mas falta arquivo (antivírus, extração cortada).
    Damaged { system: bool },
    /// Não baixada ainda; `system` = há um Chrome/Edge/Brave no PC para cair.
    NotDownloaded { system: bool },
}

fn binary_check(state: &BinaryState) -> DiagnosticCheck {
    match state {
        BinaryState::Manual { valid: true } => DiagnosticCheck::new("browserBinary", CheckStatus::Ok, "manual"),
        BinaryState::Manual { valid: false } => {
            DiagnosticCheck::new("browserBinary", CheckStatus::Problem, "manualInvalid")
        }
        BinaryState::Downloaded => DiagnosticCheck::new("browserBinary", CheckStatus::Ok, "downloaded"),
        BinaryState::Damaged { .. } => DiagnosticCheck::new("browserBinary", CheckStatus::Problem, "damaged"),
        // O caso do relato: o login abre o navegador do PC, que pode estar
        // quebrado — e reinstalá-lo era o que resolvia.
        BinaryState::NotDownloaded { system: true } => {
            DiagnosticCheck::new("browserBinary", CheckStatus::Warn, "systemFallback")
        }
        BinaryState::NotDownloaded { system: false } => {
            DiagnosticCheck::new("browserBinary", CheckStatus::Warn, "notDownloaded")
        }
    }
}

/// Como foi a abertura de teste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartOutcome {
    /// Abriu e respondeu ao controle; segundos até responder.
    Opened { secs: u32 },
    /// O Windows nem deixou o processo começar (bloqueado, arquivo inválido).
    SpawnFailed,
    /// O processo subiu, mas não respondeu em 30 s (antivírus segurando,
    /// navegador quebrado — o "não faz nada").
    DidNotOpen,
    /// Não havia navegador para testar.
    Skipped,
}

fn start_check(outcome: StartOutcome) -> DiagnosticCheck {
    match outcome {
        StartOutcome::Opened { secs } => {
            DiagnosticCheck::new("browserStart", CheckStatus::Ok, "opened").with_count(secs)
        }
        StartOutcome::SpawnFailed => DiagnosticCheck::new("browserStart", CheckStatus::Problem, "spawnFailed"),
        StartOutcome::DidNotOpen => DiagnosticCheck::new("browserStart", CheckStatus::Problem, "didNotOpen"),
        StartOutcome::Skipped => DiagnosticCheck::new("browserStart", CheckStatus::Warn, "skipped"),
    }
}

/// A página de login do Roblox carregou dentro do navegador? `None` = o teste
/// não chegou lá (o navegador não abriu): a linha não aparece.
fn roblox_check(loaded: Option<bool>) -> Option<DiagnosticCheck> {
    loaded.map(|ok| {
        if ok {
            DiagnosticCheck::new("browserRoblox", CheckStatus::Ok, "reachable")
        } else {
            DiagnosticCheck::new("browserRoblox", CheckStatus::Problem, "unreachable")
        }
    })
}

/// Uma janela de login aberta agora: o teste usa outro perfil, mas a pessoa
/// pode estar esperando aquela janela — a linha diz que ela existe.
fn login_window_check(open: bool) -> Option<DiagnosticCheck> {
    open.then(|| DiagnosticCheck::new("browserLoginOpen", CheckStatus::Warn, "open"))
}

fn binary_state(app: &AppHandle, manual: &str) -> (BinaryState, Option<PathBuf>) {
    match resolve_manual_path(Some(manual)) {
        Ok(Some(path)) => return (BinaryState::Manual { valid: true }, Some(path)),
        Err(_) => return (BinaryState::Manual { valid: false }, None),
        Ok(None) => {}
    }
    if let Some(path) = cached_binary(app) {
        return (BinaryState::Downloaded, Some(path));
    }
    let system = find_system_chromium().map(|(path, _)| path);
    let state = if has_damaged_download(app) {
        BinaryState::Damaged { system: system.is_some() }
    } else {
        BinaryState::NotDownloaded { system: system.is_some() }
    };
    (state, system)
}

/// Prazo para a página de login do Roblox responder dentro do teste.
const ROBLOX_PAGE_TIMEOUT: Duration = Duration::from_secs(20);

/// Abre o navegador num perfil só do teste, espera ele responder e pede a
/// página de login do Roblox. Fecha e apaga o perfil no fim, dê certo ou não.
async fn try_open(binary: &std::path::Path, profile: &std::path::Path) -> (StartOutcome, Option<bool>) {
    let _ = std::fs::remove_dir_all(profile);
    let started = Instant::now();
    let outcome = match spawn_chrome(binary, profile, "about:blank", true, false).await {
        Err(_) => (StartOutcome::SpawnFailed, None),
        Ok((mut child, None)) => {
            let _ = child.kill();
            (StartOutcome::DidNotOpen, None)
        }
        Ok((mut child, Some(port))) => {
            let secs = started.elapsed().as_secs().min(u32::MAX as u64) as u32;
            let loaded = match CdpClient::connect(port).await {
                Ok(mut cdp) => {
                    let loaded = tokio::time::timeout(ROBLOX_PAGE_TIMEOUT, cdp.navigate(super::commands::ROBLOX_LOGIN_URL))
                        .await
                        .map(|r| r.is_ok())
                        .unwrap_or(false);
                    let _ = cdp.close_browser().await;
                    Some(loaded)
                }
                Err(_) => None,
            };
            for _ in 0..25 {
                if matches!(child.try_wait(), Ok(Some(_))) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            let _ = child.kill();
            match loaded {
                Some(loaded) => (StartOutcome::Opened { secs }, Some(loaded)),
                // Abriu a porta, mas o controle não conectou: para quem usa,
                // é o mesmo "não faz nada".
                None => (StartOutcome::DidNotOpen, None),
            }
        }
    };
    tokio::time::sleep(Duration::from_millis(500)).await;
    let _ = std::fs::remove_dir_all(profile);
    outcome
}

/// Roda as checagens do navegador de login. Não baixa nada e não mexe no perfil
/// de login nem nos das contas.
#[tauri::command]
pub async fn run_browser_diagnostics(
    app: AppHandle,
    chromium: State<'_, ChromiumManager>,
    settings: State<'_, SettingsStore>,
) -> Result<Vec<DiagnosticCheck>, String> {
    let mut checks = Vec::new();
    let manual = super::commands::manual_binary_path(&settings);
    let (state, binary) = binary_state(&app, &manual);
    checks.push(binary_check(&state));

    let profiles_root = ChromiumManager::login_profile(&app)?
        .parent()
        .map(std::path::Path::to_path_buf)
        .ok_or("Could not resolve data directory")?;
    let probe = if profiles_root.as_os_str().is_empty() {
        FolderProbe::Unknown
    } else {
        crate::probe_folder_writable(&profiles_root)
    };
    checks.push(crate::folder_check("browserFolder", probe));

    if let Some(check) = login_window_check(chromium.is_alive(super::manager::LOGIN_KEY)) {
        checks.push(check);
    }

    let (outcome, loaded) = match binary {
        Some(binary) => try_open(&binary, &profiles_root.join("_diagnostic")).await,
        None => (StartOutcome::Skipped, None),
    };
    checks.push(start_check(outcome));
    if let Some(check) = roblox_check(loaded) {
        checks.push(check);
    }
    Ok(checks)
}
