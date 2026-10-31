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
    /// O nome do motivo na linha do Console (traduzível).
    fn label(self) -> LogMsg {
        match self {
            Self::AfkMode => log_msg!("AFK mode"),
            Self::AutoRejoin => log_msg!("Auto Rejoin"),
            Self::AutoReconnect => log_msg!("automatic reconnection"),
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
fn keep_awake_console_line(awake: bool, reasons: &[AwakeReason]) -> LogMsg {
    if awake {
        let names: Vec<LogMsg> = reasons.iter().map(|r| r.label()).collect();
        log_msg!("PC kept awake while it runs: {{reasons}} (the screen may turn off)", reasons = names)
    } else {
        log_msg!("PC released to sleep")
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
