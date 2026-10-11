// Manter o PC acordado (commands/keep_awake.rs). A única API nativa daqui é
// `SetThreadExecutionState` (kernel32), e o pedido vale para a thread que o
// fez — por isso ele sai sempre desta thread dedicada, que vive enquanto o
// app vive. Se ela morrer, o Windows solta o pedido sozinho.
//
// Só o sistema fica acordado; a tela continua apagando no tempo do Windows.

use windows_sys::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED};

/// Os bits do pedido: acordado = sistema exigido, de forma contínua; solto =
/// só `ES_CONTINUOUS`, que limpa o que a thread pediu antes.
pub fn keep_awake_flags(awake: bool) -> u32 {
    if awake {
        ES_CONTINUOUS | ES_SYSTEM_REQUIRED
    } else {
        ES_CONTINUOUS
    }
}

pub struct KeepAwakeThread {
    sender: Mutex<Option<std::sync::mpsc::Sender<(bool, std::sync::mpsc::Sender<bool>)>>>,
}

impl KeepAwakeThread {
    fn start() -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<(bool, std::sync::mpsc::Sender<bool>)>();
        let spawned = std::thread::Builder::new()
            .name("keep-awake".into())
            .spawn(move || {
                for (awake, reply) in rx {
                    let ok = unsafe { SetThreadExecutionState(keep_awake_flags(awake)) } != 0;
                    let _ = reply.send(ok);
                }
            });
        Self {
            sender: Mutex::new(spawned.ok().map(|_| tx)),
        }
    }

    /// Pede (ou solta) e espera a resposta por até 1 s.
    pub fn set(&self, awake: bool) -> bool {
        let Some(sender) = self.sender.lock().ok().and_then(|s| s.clone()) else {
            return false;
        };
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        if sender.send((awake, reply_tx)).is_err() {
            return false;
        }
        reply_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap_or(false)
    }
}

static KEEP_AWAKE_THREAD: LazyLock<KeepAwakeThread> = LazyLock::new(KeepAwakeThread::start);

pub fn keep_awake_thread() -> &'static KeepAwakeThread {
    &KEEP_AWAKE_THREAD
}

#[cfg(test)]
mod win_power_tests {
    use super::*;

    #[test]
    fn awake_is_continuous_system_required_and_release_is_continuous_only() {
        assert_eq!(keep_awake_flags(true), 0x8000_0001);
        assert_eq!(keep_awake_flags(false), 0x8000_0000);
    }
}
