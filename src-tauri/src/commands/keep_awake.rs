// Manter o PC acordado enquanto o Modo AFK, o Auto Rejoin ou a reconexão
// automática rodam (`General.KeepPcAwake`, padrão ligado). Ver
// docs/features/afk-mode.md ("PC acordado").
//
// Só o **sistema** fica acordado (`ES_SYSTEM_REQUIRED`): a tela pode apagar
// normalmente (`ES_DISPLAY_REQUIRED` nunca é pedido). O pedido é solto quando
// tudo para e ao fechar o app.
//
// Um dono só: `KeepAwake` junta os motivos e só fala com o Windows quando o
// "precisa ficar acordado" muda. O pedido do Windows (`SetThreadExecutionState`)
// vale para a **thread** que o fez, então ele sai sempre da mesma thread
// (`platform::windows::KeepAwakeThread`), nunca de uma thread do tokio.

/// Por que o PC precisa ficar acordado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AwakeReason {
    AfkMode,
    AutoRejoin,
    AutoReconnect,
}

impl AwakeReason {
    fn label(self) -> &'static str {
        match self {
            Self::AfkMode => "Modo AFK",
            Self::AutoRejoin => "Auto Rejoin",
            Self::AutoReconnect => "reconexão automática",
        }
    }
}

/// O pedido ao sistema (dublê nos testes). Devolve se deu certo.
pub trait PowerRequest {
    fn set_awake(&mut self, awake: bool) -> bool;
}

/// O dono único do pedido de "ficar acordado".
pub struct KeepAwake<P: PowerRequest> {
    power: P,
    enabled: bool,
    holders: std::collections::BTreeSet<AwakeReason>,
    applied: bool,
}

impl<P: PowerRequest> KeepAwake<P> {
    pub fn new(power: P) -> Self {
        Self {
            power,
            enabled: true,
            holders: Default::default(),
            applied: false,
        }
    }

    /// Os motivos de agora (substitui os de antes) e se a opção está ligada.
    /// Devolve `Some(acordado)` quando o pedido ao sistema mudou.
    pub fn update(&mut self, enabled: bool, reasons: &[AwakeReason]) -> Option<bool> {
        self.enabled = enabled;
        self.holders = reasons.iter().copied().collect();
        self.apply()
    }

    /// Ao fechar o app: solta o pedido, seja qual for o motivo.
    pub fn release_all(&mut self) -> Option<bool> {
        self.holders.clear();
        self.apply()
    }

    fn apply(&mut self) -> Option<bool> {
        let want = self.enabled && !self.holders.is_empty();
        if want == self.applied {
            return None;
        }
        // Se o sistema recusar, tenta de novo na próxima passada.
        if self.power.set_awake(want) {
            self.applied = want;
            Some(want)
        } else {
            None
        }
    }

    pub fn is_awake(&self) -> bool {
        self.applied
    }

    pub fn reasons(&self) -> Vec<AwakeReason> {
        self.holders.iter().copied().collect()
    }
}

/// Linha do Console quando o pedido muda.
fn keep_awake_console_line(awake: bool, reasons: &[AwakeReason]) -> String {
    if awake {
        let names: Vec<&str> = reasons.iter().map(|r| r.label()).collect();
        format!("PC mantido acordado enquanto roda: {} (a tela pode apagar)", names.join(", "))
    } else {
        String::from("PC liberado para dormir")
    }
}

#[cfg(target_os = "windows")]
struct WindowsPower;

#[cfg(target_os = "windows")]
impl PowerRequest for WindowsPower {
    fn set_awake(&mut self, awake: bool) -> bool {
        platform::windows::keep_awake_thread().set(awake)
    }
}

#[cfg(target_os = "windows")]
static KEEP_AWAKE: std::sync::LazyLock<std::sync::Mutex<KeepAwake<WindowsPower>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(KeepAwake::new(WindowsPower)));

/// Uma passada (no laço do monitor de quedas, a cada 2 s): quem precisa do PC
/// acordado agora.
#[cfg(target_os = "windows")]
pub(crate) fn keep_awake_tick(app: &tauri::AppHandle) {
    let enabled = app.state::<SettingsStore>().get_string("General", "KeepPcAwake") != "false";
    let mut reasons = Vec::new();
    if AFK_MANAGER.get_session().is_some() {
        reasons.push(AwakeReason::AfkMode);
    }
    if current_botting_status().active {
        reasons.push(AwakeReason::AutoRejoin);
    }
    if reconnect_is_guarding() {
        reasons.push(AwakeReason::AutoReconnect);
    }
    let changed = match KEEP_AWAKE.lock() {
        Ok(mut keep) => keep.update(enabled, &reasons),
        Err(_) => None,
    };
    if let Some(awake) = changed {
        emit_session_log(app, "info", "power", keep_awake_console_line(awake, &reasons));
    }
}

/// Ao fechar o app: devolve o PC ao normal.
#[cfg(target_os = "windows")]
pub(crate) fn keep_awake_release_on_exit() {
    if let Ok(mut keep) = KEEP_AWAKE.lock() {
        keep.release_all();
    }
}

#[cfg(test)]
mod keep_awake_tests {
    use super::*;

    /// Dublê: anota cada pedido ao sistema.
    #[derive(Default)]
    struct FakePower {
        calls: Vec<bool>,
        refuse: bool,
    }

    impl PowerRequest for FakePower {
        fn set_awake(&mut self, awake: bool) -> bool {
            self.calls.push(awake);
            !self.refuse
        }
    }

    #[test]
    fn it_asks_once_when_something_starts_and_releases_when_all_stop() {
        let mut keep = KeepAwake::new(FakePower::default());
        assert_eq!(keep.update(true, &[]), None, "nothing running: no call at all");
        assert_eq!(keep.update(true, &[AwakeReason::AfkMode]), Some(true));
        assert_eq!(keep.update(true, &[AwakeReason::AfkMode]), None, "same state: no new call");
        assert_eq!(
            keep.update(true, &[AwakeReason::AfkMode, AwakeReason::AutoRejoin]),
            None,
            "a second reason does not ask again"
        );
        assert_eq!(keep.update(true, &[AwakeReason::AutoRejoin]), None, "AFK stopped, Auto Rejoin still runs");
        assert_eq!(keep.update(true, &[]), Some(false));
        assert_eq!(keep.power.calls, vec![true, false]);
        assert!(!keep.is_awake());
    }

    #[test]
    fn turning_the_option_off_releases_and_on_asks_again() {
        let mut keep = KeepAwake::new(FakePower::default());
        keep.update(true, &[AwakeReason::AutoReconnect]);
        assert_eq!(keep.update(false, &[AwakeReason::AutoReconnect]), Some(false));
        assert_eq!(keep.update(false, &[AwakeReason::AutoReconnect]), None);
        assert_eq!(keep.update(true, &[AwakeReason::AutoReconnect]), Some(true));
        assert_eq!(keep.power.calls, vec![true, false, true]);
    }

    #[test]
    fn closing_the_app_releases_whatever_was_holding_it() {
        let mut keep = KeepAwake::new(FakePower::default());
        keep.update(true, &[AwakeReason::AfkMode, AwakeReason::AutoRejoin]);
        assert_eq!(keep.release_all(), Some(false));
        assert_eq!(keep.release_all(), None, "already released");
        assert_eq!(keep.power.calls, vec![true, false]);
    }

    #[test]
    fn a_refused_request_is_tried_again_on_the_next_pass() {
        let mut keep = KeepAwake::new(FakePower {
            refuse: true,
            ..Default::default()
        });
        assert_eq!(keep.update(true, &[AwakeReason::AfkMode]), None);
        assert!(!keep.is_awake());
        keep.power.refuse = false;
        assert_eq!(keep.update(true, &[AwakeReason::AfkMode]), Some(true));
        assert_eq!(keep.power.calls, vec![true, true]);
    }

    #[test]
    fn the_console_line_names_who_keeps_it_awake() {
        assert_eq!(
            keep_awake_console_line(true, &[AwakeReason::AfkMode, AwakeReason::AutoReconnect]),
            "PC mantido acordado enquanto roda: Modo AFK, reconexão automática (a tela pode apagar)"
        );
        assert_eq!(keep_awake_console_line(false, &[]), "PC liberado para dormir");
    }

    /// O pedido ao Windows é só "sistema acordado": nunca a tela.
    #[test]
    fn the_windows_request_never_keeps_the_screen_on() {
        let source = include_str!("../platform/windows/power.rs");
        let code = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(code.contains("ES_SYSTEM_REQUIRED"));
        assert!(code.contains("ES_CONTINUOUS"));
        assert!(!code.contains("ES_DISPLAY_REQUIRED"), "the screen may turn off");
    }
}
