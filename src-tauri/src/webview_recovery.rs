//! Recuperação de tela branca/preta do WebView2 (Windows).
//!
//! A interface deste app é desenhada pelo WebView2. Quando o WebView2 quebra
//! — driver de vídeo, runtime atualizado no meio, composição de janela — a
//! janela abre **em branco**: não há menu, não há configuração, não há como o
//! usuário desligar a aceleração de vídeo, porque a tela que teria esse botão é
//! justamente a que não apareceu. Sem este módulo o app fica inutilizável e a
//! única saída é reinstalar (que não reinstala o WebView2).
//!
//! A recuperação tem três peças:
//!
//! 1. **Safe mode de vídeo** (`--disable-gpu --disable-gpu-compositing`), que o
//!    usuário liga de fora (`--safe-mode`, `RAM_WEBVIEW_SAFE_MODE`, Shift no
//!    boot) ou que o próprio app liga ao ver que a janela não pintou.
//! 2. **Marcador em disco**, para o safe mode sobreviver ao reinício. Ele guarda
//!    a versão do runtime do WebView2 que falhou — runtime diferente, marcador
//!    inválido — senão o safe mode ficaria ligado para sempre, degradando o app
//!    muito depois de o problema ter sido corrigido.
//! 3. **Watchdog do sinal de pintura**: o frontend avisa quando pintou o
//!    primeiro quadro. Sem esse aviso dentro do prazo, o app grava o marcador,
//!    avisa o usuário e reabre em safe mode.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Wry};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_SHIFT};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, MB_ICONERROR, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MB_TOPMOST,
};

use crate::data::settings::get_settings_path;

/// O frontend confirmou que pintou. Só isso desarma o watchdog.
static FRONTEND_PAINTED: AtomicBool = AtomicBool::new(false);
/// Este boot está em safe mode de vídeo.
static SAFE_MODE: AtomicBool = AtomicBool::new(false);

/// Fica ao lado do `RAMSettings.ini`, na pasta de dados do usuário.
const MARKER_FILE: &str = "webview.safemode";
/// Prazo para o primeiro quadro. Generoso de propósito: máquina lenta com disco
/// ocupado no boot do Windows leva tempo, e reabrir o app por engano é pior que
/// esperar.
const READY_TIMEOUT_SECS: u64 = 25;
/// Recursos do WebView2 que o app nunca usa e que só trazem risco de travar a
/// primeira pintura (`CalculateNativeWinOcclusion` é o clássico da tela preta).
const DISABLED_FEATURES: [&str; 4] = [
    "msWebOOUI",
    "msPdfOOUI",
    "msSmartScreenProtection",
    "CalculateNativeWinOcclusion",
];
const SAFE_MODE_FLAGS: [&str; 2] = ["--disable-gpu", "--disable-gpu-compositing"];
const BROWSER_ARGUMENTS_ENV: &str = "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS";
const SAFE_MODE_ENV: &str = "RAM_WEBVIEW_SAFE_MODE";
const SAFE_MODE_ARG: &str = "--safe-mode";

/// O que se sabe no boot sobre o safe mode, já coletado de fora: linha de
/// comando, ambiente, teclado e disco. Existe separado para a **decisão** ser
/// testável sem Win32 e sem tocar o disco do usuário.
#[derive(Debug, Clone, Copy, Default)]
struct SafeModeSignals<'a> {
    /// `--safe-mode` na linha de comando (atalho do Windows, campo "Destino").
    cli: bool,
    /// Valor cru de `RAM_WEBVIEW_SAFE_MODE`, se a variável existir.
    env: Option<&'a str>,
    /// Shift segurado enquanto o app abria.
    shift: bool,
    /// Conteúdo do marcador em disco, se o arquivo existir.
    marker: Option<&'a str>,
    /// Versão do runtime do WebView2 deste boot.
    runtime_version: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SafeModePlan {
    safe_mode: bool,
    /// O marcador não vale mais (runtime diferente ou conteúdo vazio) e tem que
    /// sair do disco.
    discard_marker: bool,
}

/// Formas de "sim" aceitas em `RAM_WEBVIEW_SAFE_MODE`.
fn env_asks_for_safe_mode(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// A decisão inteira do safe mode, sem efeito colateral.
fn decide_safe_mode(signals: SafeModeSignals) -> SafeModePlan {
    let forced =
        signals.cli || signals.env.map(env_asks_for_safe_mode).unwrap_or(false) || signals.shift;

    // Marcador só vale para o runtime que falhou. Runtime diferente (o WebView2
    // se atualiza sozinho) ou conteúdo vazio significa que a informação é
    // velha: some com ela, senão o safe mode fica ligado para sempre.
    let (marker_asks, discard_marker) = match signals.marker.map(str::trim) {
        Some(recorded) if !recorded.is_empty() && recorded == signals.runtime_version => {
            (true, false)
        }
        Some(_) => (false, true),
        None => (false, false),
    };

    SafeModePlan {
        safe_mode: forced || marker_asks,
        discard_marker,
    }
}

/// Mescla os argumentos do WebView2 com o que já estava na variável de ambiente.
///
/// Sobrescrever a variável apagaria o que o usuário (ou um lançador externo)
/// pôs ali, então o que já existe é preservado e as nossas opções só são
/// acrescentadas quando faltam.
fn merge_browser_arguments(existing: &str, safe_mode: bool) -> String {
    let mut features: Vec<&str> = Vec::new();
    let mut args: Vec<&str> = Vec::new();

    for part in existing.split_whitespace() {
        // Vários `--disable-features=` na mesma variável: o WebView2 usa o
        // último, então juntar todos numa lista é o que preserva a intenção.
        if let Some(list) = part.strip_prefix("--disable-features=") {
            for feature in list.split(',').filter(|f| !f.is_empty()) {
                if !features.contains(&feature) {
                    features.push(feature);
                }
            }
        } else {
            args.push(part);
        }
    }

    for feature in DISABLED_FEATURES {
        if !features.contains(&feature) {
            features.push(feature);
        }
    }
    if safe_mode {
        for flag in SAFE_MODE_FLAGS {
            if !args.contains(&flag) {
                args.push(flag);
            }
        }
    }

    let mut merged = args.join(" ");
    if !merged.is_empty() {
        merged.push(' ');
    }
    merged.push_str("--disable-features=");
    merged.push_str(&features.join(","));
    merged
}

/// O que o watchdog faz quando o prazo do primeiro quadro estoura.
///
/// Existe separado da thread porque **errar a mensagem aqui manda o usuário
/// consertar a coisa errada**: dizer "nem o safe mode funcionou" quando o safe
/// mode nunca foi tentado faz a pessoa reparar o WebView2 e trocar driver de
/// vídeo, enquanto o problema de verdade é a pasta de dados não aceitar escrita
/// — que também vai derrubar o `RAMSettings.ini` e o `AccountData.json` dela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WatchdogAction {
    /// O frontend pintou dentro do prazo: nada a fazer.
    Nothing,
    /// Já era a tentativa em safe mode e ela também não pintou. Reabrir seria
    /// um laço, então só resta o reparo do runtime.
    WarnRepairRuntime,
    /// O marcador não pôde ser gravado. Sem ele o safe mode não sobrevive ao
    /// reinício, e a causa é a pasta de dados, não o vídeo.
    WarnDataFolderNotWritable,
    /// Marcador gravado: avisa e reabre com a aceleração de vídeo desligada.
    RestartInSafeMode,
}

/// `write_marker` é preguiçoso de propósito: nos dois primeiros ramos o
/// marcador **não** deve ser tocado.
fn decide_watchdog_action(
    painted: bool,
    safe_mode: bool,
    write_marker: impl FnOnce() -> bool,
) -> WatchdogAction {
    if painted {
        return WatchdogAction::Nothing;
    }
    if safe_mode {
        return WatchdogAction::WarnRepairRuntime;
    }
    if write_marker() {
        WatchdogAction::RestartInSafeMode
    } else {
        WatchdogAction::WarnDataFolderNotWritable
    }
}

/// Estado do safe mode como o frontend o vê.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebviewSafeModeState {
    /// Este boot está com a aceleração de vídeo desligada.
    pub active: bool,
    /// Existe marcador em disco, então a próxima abertura também vem em safe
    /// mode. É o que decide se vale oferecer "voltar ao modo normal": safe mode
    /// que veio só do Shift acaba sozinho no próximo boot.
    pub sticky: bool,
}

fn safe_mode_state(active: bool, marker_exists: bool) -> WebviewSafeModeState {
    WebviewSafeModeState {
        active,
        sticky: marker_exists,
    }
}

/// Apaga o marcador do safe mode — é a saída do modo degradado pelas mãos do
/// usuário, sem depender de o runtime do WebView2 mudar.
///
/// Recebe o caminho para ser testável sem tocar a pasta do usuário.
fn remove_safe_mode_marker(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        // Já não existe: o app já está no modo normal. Erro aqui seria assustar
        // o usuário por causa de um clique a mais.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        // O caminho vai na mensagem porque, se o app não consegue apagar, a
        // única saída que sobra é o usuário apagar o arquivo na mão.
        Err(e) => Err(format!(
            "Could not delete {}: {}. Delete this file manually to leave graphics safe mode.",
            path.display(),
            e
        )),
    }
}

fn marker_path() -> PathBuf {
    get_settings_path().with_file_name(MARKER_FILE)
}

/// A versão do runtime do WebView2 deste boot. `unknown` quando o Tauri não
/// consegue responder: um valor estável é melhor que vazio, porque o marcador
/// compara strings e vazio invalidaria o marcador em todo boot.
fn runtime_version() -> String {
    tauri::webview_version()
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn shift_held() -> bool {
    // O bit alto de `GetAsyncKeyState` diz "está pressionada agora".
    unsafe { GetAsyncKeyState(VK_SHIFT as i32) < 0 }
}

fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Aviso nativo do Windows.
///
/// É `MessageBoxW` do `windows-sys` (já dependência do projeto) e não um
/// diálogo do Tauri de propósito: quando este aviso é necessário, o WebView2 é
/// exatamente a peça que não está funcionando — qualquer aviso desenhado pelo
/// frontend (toast da store, faixa, modal) apareceria na mesma tela em branco
/// que se está tentando consertar.
fn message_box(title: &str, text: &str, icon: u32) {
    let title = wide(title);
    let text = wide(text);
    unsafe {
        let no_owner: HWND = std::ptr::null_mut();
        MessageBoxW(
            no_owner,
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | icon | MB_SETFOREGROUND | MB_TOPMOST,
        );
    }
}

/// Aviso do ramo em que o safe mode **nunca foi tentado**: o que falhou foi
/// gravar na pasta de dados. Mandar reparar o WebView2 aqui seria diagnóstico
/// errado — e a pasta sem escrita vai derrubar as contas e as settings também.
fn warn_data_folder_not_writable(folder: &str) {
    message_box(
        "Roblox Account Manager",
        &format!(
            "Roblox Account Manager could not draw its interface, and it could not write to its data folder to remember that:\n\n{folder}\n\nThat folder has to be writable — the account file and the settings file live there too. Check the folder's permissions and free disk space, or run RAM from a folder you own.\n\nMeanwhile you can start RAM with graphics safe mode by holding Shift while it opens, or by adding --safe-mode to the shortcut's Target field."
        ),
        MB_ICONERROR,
    );
}

fn warn_restarting_in_safe_mode() {
    message_box(
        "Roblox Account Manager",
        "Roblox Account Manager could not draw its interface, so it will reopen with graphics safe mode (GPU acceleration off).\n\nIf the window works after the restart, keep using it: safe mode stays on until the Microsoft Edge WebView2 Runtime changes.\n\nYou can force this mode at any time by holding Shift while RAM starts, or by adding --safe-mode to the shortcut's Target field.",
        MB_ICONWARNING,
    );
}

fn warn_repair_runtime() {
    message_box(
        "Roblox Account Manager",
        "Roblox Account Manager could not draw its interface, not even with graphics safe mode.\n\nThe interface is drawn by the Microsoft Edge WebView2 Runtime, so the fix is on that side:\n\n1. Windows Settings > Apps > Installed apps > Microsoft Edge WebView2 Runtime > Modify > Repair, then restart Windows.\n2. If it is missing, install Microsoft's WebView2 Evergreen Standalone Installer.\n3. Update the graphics driver.\n\nYour accounts and settings are untouched.",
        MB_ICONERROR,
    );
}

/// Roda **antes** de o Tauri criar a janela: é a última hora em que dá para
/// mexer nos argumentos que o WebView2 vai receber.
pub fn prepare_environment() {
    let version = runtime_version();
    let marker = marker_path();
    let recorded = std::fs::read_to_string(&marker).ok();
    let env = std::env::var(SAFE_MODE_ENV).ok();
    let cli = std::env::args_os()
        .skip(1)
        .any(|arg| arg.to_string_lossy().eq_ignore_ascii_case(SAFE_MODE_ARG));

    let plan = decide_safe_mode(SafeModeSignals {
        cli,
        env: env.as_deref(),
        shift: shift_held(),
        marker: recorded.as_deref(),
        runtime_version: &version,
    });

    if plan.discard_marker {
        let _ = std::fs::remove_file(&marker);
    }
    SAFE_MODE.store(plan.safe_mode, Ordering::SeqCst);

    let existing = std::env::var(BROWSER_ARGUMENTS_ENV).unwrap_or_default();
    std::env::set_var(
        BROWSER_ARGUMENTS_ENV,
        merge_browser_arguments(&existing, plan.safe_mode),
    );
}

/// O frontend pintou o primeiro quadro.
pub fn mark_painted() {
    FRONTEND_PAINTED.store(true, Ordering::SeqCst);
    // Pintou **sem** safe mode: o modo normal volta a funcionar, então o
    // marcador não tem mais motivo para existir. Em safe mode o marcador fica:
    // ter pintado com a GPU desligada não é prova de que ligá-la de novo
    // funciona, e a prova errada custa outra tela em branco.
    if !SAFE_MODE.load(Ordering::SeqCst) {
        let _ = std::fs::remove_file(marker_path());
    }
}

pub fn start_watchdog(app: AppHandle<Wry>) {
    // Em dev o frontend vem do servidor do Vite e pode demorar (ou estar
    // caído) por motivos que não têm nada a ver com o WebView2 — reabrir o app
    // no meio de uma sessão de desenvolvimento só atrapalha.
    if cfg!(debug_assertions) {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(READY_TIMEOUT_SECS));
        let marker = marker_path();
        let action = decide_watchdog_action(
            FRONTEND_PAINTED.load(Ordering::SeqCst),
            SAFE_MODE.load(Ordering::SeqCst),
            || std::fs::write(&marker, runtime_version()).is_ok(),
        );
        match action {
            WatchdogAction::Nothing => {}
            WatchdogAction::WarnRepairRuntime => warn_repair_runtime(),
            WatchdogAction::WarnDataFolderNotWritable => {
                let folder = marker
                    .parent()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| marker.display().to_string());
                warn_data_folder_not_writable(&folder);
            }
            WatchdogAction::RestartInSafeMode => {
                warn_restarting_in_safe_mode();
                app.restart();
            }
        }
    });
}

/// Estado do safe mode para o frontend poder dizer que está ligado.
pub fn current_state() -> WebviewSafeModeState {
    safe_mode_state(SAFE_MODE.load(Ordering::SeqCst), marker_path().exists())
}

/// Saída do safe mode pelas mãos do usuário: apaga o marcador para a próxima
/// abertura voltar ao normal, sem depender de o runtime do WebView2 mudar.
///
/// Não desliga o safe mode **deste** boot: as flags já foram entregues ao
/// WebView2 na criação da janela, então só reabrindo o app a GPU volta. Quem
/// chama é que reinicia.
pub fn leave_safe_mode() -> Result<(), String> {
    remove_safe_mode_marker(&marker_path())
}

#[cfg(test)]
mod webview_recovery_tests {
    use super::*;
    use std::cell::Cell;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Caminho próprio em `%TEMP%`, no padrão que o resto dos testes já usa.
    fn unique_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("ram-webview-{name}-{nanos}.safemode"))
    }

    fn signals<'a>(runtime_version: &'a str) -> SafeModeSignals<'a> {
        SafeModeSignals {
            runtime_version,
            ..Default::default()
        }
    }

    #[test]
    fn a_quiet_boot_does_not_enable_safe_mode_and_keeps_the_disk_untouched() {
        let plan = decide_safe_mode(signals("120.0.0.1"));
        assert_eq!(
            plan,
            SafeModePlan {
                safe_mode: false,
                discard_marker: false
            }
        );
    }

    #[test]
    fn the_command_line_flag_enables_safe_mode() {
        let plan = decide_safe_mode(SafeModeSignals {
            cli: true,
            ..signals("120.0.0.1")
        });
        assert!(plan.safe_mode);
    }

    #[test]
    fn the_environment_variable_accepts_the_usual_spellings_of_yes() {
        for value in ["1", "true", "TRUE", "yes", "on", " On "] {
            assert!(
                env_asks_for_safe_mode(value),
                "{value:?} deveria ligar o safe mode"
            );
        }
        for value in ["", "0", "false", "no", "off", "maybe"] {
            assert!(
                !env_asks_for_safe_mode(value),
                "{value:?} não deveria ligar o safe mode"
            );
        }
    }

    #[test]
    fn the_environment_variable_reaches_the_decision() {
        let plan = decide_safe_mode(SafeModeSignals {
            env: Some("1"),
            ..signals("120.0.0.1")
        });
        assert!(plan.safe_mode);

        let plan = decide_safe_mode(SafeModeSignals {
            env: Some("0"),
            ..signals("120.0.0.1")
        });
        assert!(!plan.safe_mode);
    }

    #[test]
    fn holding_shift_enables_safe_mode() {
        let plan = decide_safe_mode(SafeModeSignals {
            shift: true,
            ..signals("120.0.0.1")
        });
        assert!(plan.safe_mode);
    }

    #[test]
    fn a_marker_from_this_runtime_enables_safe_mode_and_stays_on_disk() {
        let plan = decide_safe_mode(SafeModeSignals {
            marker: Some("120.0.0.1"),
            ..signals("120.0.0.1")
        });
        assert_eq!(
            plan,
            SafeModePlan {
                safe_mode: true,
                discard_marker: false
            }
        );
    }

    #[test]
    fn a_marker_with_trailing_whitespace_still_matches_the_runtime() {
        let plan = decide_safe_mode(SafeModeSignals {
            marker: Some("120.0.0.1\r\n"),
            ..signals("120.0.0.1")
        });
        assert!(plan.safe_mode);
        assert!(!plan.discard_marker);
    }

    #[test]
    fn a_marker_from_another_runtime_is_ignored_and_discarded() {
        let plan = decide_safe_mode(SafeModeSignals {
            marker: Some("119.0.0.9"),
            ..signals("120.0.0.1")
        });
        assert_eq!(
            plan,
            SafeModePlan {
                safe_mode: false,
                discard_marker: true
            },
            "runtime novo tem que zerar o safe mode, senão ele fica ligado para sempre"
        );
    }

    #[test]
    fn an_empty_marker_is_discarded_and_enables_nothing() {
        let plan = decide_safe_mode(SafeModeSignals {
            marker: Some("   "),
            ..signals("120.0.0.1")
        });
        assert_eq!(
            plan,
            SafeModePlan {
                safe_mode: false,
                discard_marker: true
            }
        );
    }

    #[test]
    fn a_stale_marker_is_discarded_even_when_safe_mode_was_forced() {
        let plan = decide_safe_mode(SafeModeSignals {
            cli: true,
            marker: Some("119.0.0.9"),
            ..signals("120.0.0.1")
        });
        assert_eq!(
            plan,
            SafeModePlan {
                safe_mode: true,
                discard_marker: true
            }
        );
    }

    /// Argumentos do resultado, na ordem.
    fn args(merged: &str) -> Vec<&str> {
        merged.split_whitespace().collect()
    }

    /// Os nomes dentro do único `--disable-features=` do resultado.
    fn features(merged: &str) -> Vec<&str> {
        let lists: Vec<&str> = args(merged)
            .into_iter()
            .filter_map(|a| a.strip_prefix("--disable-features="))
            .collect();
        assert_eq!(
            lists.len(),
            1,
            "o resultado tem que ter exatamente um --disable-features=: {merged:?}"
        );
        lists[0].split(',').filter(|f| !f.is_empty()).collect()
    }

    #[test]
    fn merging_keeps_what_was_already_in_the_variable() {
        let merged = merge_browser_arguments("--algo-do-usuario --outro=1", false);
        let args = args(&merged);
        assert!(args.contains(&"--algo-do-usuario"), "{merged:?}");
        assert!(args.contains(&"--outro=1"), "{merged:?}");
    }

    #[test]
    fn merging_keeps_features_the_user_had_disabled() {
        let merged = merge_browser_arguments("--disable-features=MinhaFeature,Outra", false);
        let features = features(&merged);
        assert!(features.contains(&"MinhaFeature"), "{merged:?}");
        assert!(features.contains(&"Outra"), "{merged:?}");
        for ours in DISABLED_FEATURES {
            assert!(features.contains(&ours), "{ours} faltou em {merged:?}");
        }
    }

    #[test]
    fn merging_does_not_duplicate_a_feature_that_was_already_there() {
        let merged = merge_browser_arguments("--disable-features=msWebOOUI", false);
        let features = features(&merged);
        assert_eq!(
            features.iter().filter(|f| **f == "msWebOOUI").count(),
            1,
            "{merged:?}"
        );
    }

    #[test]
    fn merging_an_empty_variable_yields_only_our_features() {
        let merged = merge_browser_arguments("", false);
        assert_eq!(features(&merged), DISABLED_FEATURES.to_vec());
        assert_eq!(args(&merged).len(), 1, "{merged:?}");
    }

    #[test]
    fn safe_mode_adds_the_gpu_flags() {
        let merged = merge_browser_arguments("", true);
        let args = args(&merged);
        for flag in SAFE_MODE_FLAGS {
            assert!(args.contains(&flag), "{flag} faltou em {merged:?}");
        }
    }

    #[test]
    fn without_safe_mode_the_gpu_flags_stay_out() {
        let merged = merge_browser_arguments("", false);
        let args = args(&merged);
        for flag in SAFE_MODE_FLAGS {
            assert!(!args.contains(&flag), "{flag} sobrou em {merged:?}");
        }
    }

    #[test]
    fn safe_mode_does_not_duplicate_a_gpu_flag_that_was_already_there() {
        let merged = merge_browser_arguments("--disable-gpu", true);
        let args = args(&merged);
        assert_eq!(
            args.iter().filter(|a| **a == "--disable-gpu").count(),
            1,
            "{merged:?}"
        );
        assert!(args.contains(&"--disable-gpu-compositing"), "{merged:?}");
    }

    // --- o que o watchdog faz quando o prazo estoura ---

    /// Conta as tentativas de gravar o marcador: nos ramos em que o marcador
    /// não deve ser tocado, tocá-lo já é bug.
    struct Writer {
        result: bool,
        tries: Cell<u32>,
    }

    impl Writer {
        fn new(result: bool) -> Self {
            Writer {
                result,
                tries: Cell::new(0),
            }
        }

        fn write(&self) -> bool {
            self.tries.set(self.tries.get() + 1);
            self.result
        }
    }

    #[test]
    fn a_frontend_that_painted_stops_the_watchdog_without_touching_the_marker() {
        let writer = Writer::new(true);
        let action = decide_watchdog_action(true, false, || writer.write());
        assert_eq!(action, WatchdogAction::Nothing);
        assert_eq!(writer.tries.get(), 0);
    }

    #[test]
    fn a_safe_mode_boot_that_did_not_paint_asks_for_a_runtime_repair() {
        let writer = Writer::new(true);
        let action = decide_watchdog_action(false, true, || writer.write());
        assert_eq!(action, WatchdogAction::WarnRepairRuntime);
        assert_eq!(
            writer.tries.get(),
            0,
            "já está em safe mode: regravar o marcador não muda nada"
        );
    }

    #[test]
    fn a_normal_boot_that_did_not_paint_restarts_in_safe_mode() {
        let writer = Writer::new(true);
        let action = decide_watchdog_action(false, false, || writer.write());
        assert_eq!(action, WatchdogAction::RestartInSafeMode);
        assert_eq!(writer.tries.get(), 1);
    }

    #[test]
    fn a_marker_that_cannot_be_written_blames_the_data_folder_not_the_safe_mode() {
        let writer = Writer::new(false);
        let action = decide_watchdog_action(false, false, || writer.write());
        assert_eq!(
            action,
            WatchdogAction::WarnDataFolderNotWritable,
            "o safe mode nunca foi tentado neste ramo: culpar vídeo manda o usuário consertar a coisa errada"
        );
    }

    // --- estado exposto ao frontend e a saída do safe mode ---

    #[test]
    fn the_exposed_state_says_whether_the_next_boot_is_safe_mode_too() {
        assert_eq!(
            safe_mode_state(true, true),
            WebviewSafeModeState {
                active: true,
                sticky: true
            }
        );
        // Safe mode forçado por Shift/argumento/variável: não há marcador, então
        // a próxima abertura já volta ao normal sozinha.
        assert_eq!(
            safe_mode_state(true, false),
            WebviewSafeModeState {
                active: true,
                sticky: false
            }
        );
        assert_eq!(
            safe_mode_state(false, false),
            WebviewSafeModeState {
                active: false,
                sticky: false
            }
        );
    }

    /// Um marcador gravado por uma versão anterior fica no disco até alguém
    /// apagá-lo, então o estado tem que reportar `sticky` mesmo quando este boot
    /// não está em safe mode — é o que permite a faixa oferecer a saída.
    #[test]
    fn a_marker_left_behind_is_reported_even_when_this_boot_is_normal() {
        assert_eq!(
            safe_mode_state(false, true),
            WebviewSafeModeState {
                active: false,
                sticky: true
            }
        );
    }

    #[test]
    fn leaving_safe_mode_deletes_the_marker() {
        let path = unique_path("leave");
        std::fs::write(&path, "120.0.0.1").expect("marcador de teste");
        assert!(remove_safe_mode_marker(&path).is_ok());
        assert!(!path.exists(), "o marcador tinha que ter sido apagado");
    }

    #[test]
    fn leaving_safe_mode_twice_is_not_an_error() {
        let path = unique_path("missing");
        assert!(!path.exists());
        assert!(
            remove_safe_mode_marker(&path).is_ok(),
            "já estar no modo normal não é erro para mostrar ao usuário"
        );
    }

    #[test]
    fn a_marker_that_cannot_be_deleted_reports_the_path_so_the_user_can_do_it() {
        // Uma pasta no lugar do arquivo: `remove_file` falha, e é o mais perto
        // que dá para chegar de "sem permissão" sem mexer em ACL.
        let path = unique_path("as-dir");
        std::fs::create_dir_all(&path).expect("pasta de teste");
        let error = remove_safe_mode_marker(&path).expect_err("tinha que falhar");
        assert!(
            error.contains(&path.display().to_string()),
            "a mensagem precisa dizer qual arquivo apagar na mão: {error:?}"
        );
        let _ = std::fs::remove_dir_all(&path);
    }
}
