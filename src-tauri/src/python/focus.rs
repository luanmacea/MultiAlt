// A fila de foco (docs/features/python-automation.md, "Fila de foco").
//
// Mandar tecla ou clique exige a janela da conta na frente, e só uma janela
// fica na frente por vez. O Modo AFK e as Gravações já se revezam pelo
// `AFK_CYCLE_LOCK`; a automação Python entra **na mesma fila**: um *lease* é o
// guarda desse lock, preso a uma execução e a uma conta. Enquanto um script
// segura o lease, o ciclo do AFK espera — e vice-versa.
//
// Lease sem renovar por 30 s é solto pelo vigia (o script travou ou esqueceu
// de soltar), e soltar um lease solta as teclas que a execução deixou
// apertadas, na ordem inversa: tecla presa faz o personagem andar sozinho.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::errors::PyError;
use super::primitives::Desk;

/// Lease sem renovar por mais que isto é solto pelo vigia.
pub const LEASE_IDLE_MS: u64 = 30_000;

struct Lease {
    id: String,
    run_id: String,
    user_id: i64,
    _guard: tokio::sync::MutexGuard<'static, ()>,
    last_renew: Instant,
    held_keys: Vec<String>,
}

pub struct FocusQueue {
    lock: &'static tokio::sync::Mutex<()>,
    lease: Mutex<Option<Lease>>,
    seq: AtomicU64,
    /// Quantos pedidos de lease estão esperando a vez agora.
    waiting: AtomicUsize,
    idle: Duration,
}

/// Desconta a espera ao sair do `acquire` (inclusive cancelado no meio).
struct WaitingGuard<'a>(&'a AtomicUsize);

impl Drop for WaitingGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// O lease solto pelo vigia: de quem era.
#[derive(Debug, Clone, PartialEq)]
pub struct Expired {
    pub run_id: String,
    pub user_id: i64,
    pub released_keys: Vec<String>,
}

#[cfg(not(target_os = "windows"))]
static PORTABLE_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

impl FocusQueue {
    pub fn new(lock: &'static tokio::sync::Mutex<()>) -> Self {
        Self {
            lock,
            lease: Mutex::new(None),
            seq: AtomicU64::new(0),
            waiting: AtomicUsize::new(0),
            idle: Duration::from_millis(LEASE_IDLE_MS),
        }
    }

    /// A fila do app: o mesmo lock do Modo AFK e das Gravações.
    pub fn global() -> Arc<FocusQueue> {
        static GLOBAL: std::sync::LazyLock<Arc<FocusQueue>> = std::sync::LazyLock::new(|| {
            #[cfg(target_os = "windows")]
            let lock: &'static tokio::sync::Mutex<()> = &*crate::AFK_CYCLE_LOCK;
            #[cfg(not(target_os = "windows"))]
            let lock: &'static tokio::sync::Mutex<()> = &*PORTABLE_LOCK;
            Arc::new(FocusQueue::new(lock))
        });
        GLOBAL.clone()
    }

    /// Espera a vez na fila (no máximo `timeout_ms`) e devolve o id do lease.
    /// Parar a execução no meio da espera devolve `stopped`.
    pub async fn acquire(
        &self,
        run_id: &str,
        user_id: i64,
        timeout_ms: u64,
        stop: &AtomicBool,
    ) -> Result<String, PyError> {
        let stop_watch = async {
            loop {
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        };
        self.waiting.fetch_add(1, Ordering::SeqCst);
        let waiting = WaitingGuard(&self.waiting);
        let guard = tokio::select! {
            guard = self.lock.lock() => guard,
            _ = tokio::time::sleep(Duration::from_millis(timeout_ms)) => {
                return Err(PyError::focus_timeout(timeout_ms));
            }
            _ = stop_watch => return Err(PyError::stopped()),
        };
        drop(waiting);
        let id = format!("L{}", self.seq.fetch_add(1, Ordering::SeqCst) + 1);
        *self.lease.lock().unwrap() = Some(Lease {
            id: id.clone(),
            run_id: run_id.to_string(),
            user_id,
            _guard: guard,
            last_renew: Instant::now(),
            held_keys: Vec::new(),
        });
        Ok(id)
    }

    pub fn renew(&self, run_id: &str, lease_id: &str) -> Result<(), PyError> {
        let mut slot = self.lease.lock().unwrap();
        match slot.as_mut() {
            Some(lease) if lease.id == lease_id && lease.run_id == run_id => {
                lease.last_renew = Instant::now();
                Ok(())
            }
            _ => Err(PyError::no_focus_lease()),
        }
    }

    /// Solta o lease (e as teclas que ficaram apertadas): devolve a conta e as
    /// teclas soltas. Lease que não é desta execução — ou que o vigia já
    /// soltou — não é erro: não há o que soltar (`None`).
    pub fn release(&self, run_id: &str, lease_id: &str, desk: &dyn Desk) -> Option<(i64, Vec<String>)> {
        let taken = {
            let mut slot = self.lease.lock().unwrap();
            match slot.as_ref() {
                Some(lease) if lease.id == lease_id && lease.run_id == run_id => slot.take(),
                _ => None,
            }
        };
        taken.map(|lease| {
            let user_id = lease.user_id;
            (user_id, release_keys(lease, desk))
        })
    }

    /// A fila livre de verdade — sem lease, sem ciclo do AFK ou gravação e sem
    /// ninguém esperando a vez — devolve o lock, para a volta do foco rodar
    /// segurando-o (ninguém entra no meio). Ocupada, `None`: devolver o foco
    /// agora roubaria a janela de quem vem a seguir (outra execução, outra
    /// thread desta mesma execução, ou o Modo AFK, que não usa lease).
    pub fn hold_if_free(&self) -> Option<tokio::sync::MutexGuard<'static, ()>> {
        if self.waiting.load(Ordering::SeqCst) > 0 || self.lease.lock().unwrap().is_some() {
            return None;
        }
        let guard = self.lock.try_lock().ok()?;
        // Alguém pode ter entrado na fila entre a conferência e o lock.
        if self.waiting.load(Ordering::SeqCst) > 0 {
            return None;
        }
        Some(guard)
    }

    /// Esta execução segura o lease desta conta?
    pub fn holds(&self, run_id: &str, user_id: i64) -> bool {
        matches!(self.lease.lock().unwrap().as_ref(), Some(l) if l.run_id == run_id && l.user_id == user_id)
    }

    pub fn note_key_down(&self, run_id: &str, key: &str) {
        if let Some(lease) = self.lease.lock().unwrap().as_mut() {
            if lease.run_id == run_id && !lease.held_keys.iter().any(|k| k.eq_ignore_ascii_case(key)) {
                lease.held_keys.push(key.to_string());
            }
        }
    }

    pub fn note_key_up(&self, run_id: &str, key: &str) {
        if let Some(lease) = self.lease.lock().unwrap().as_mut() {
            if lease.run_id == run_id {
                lease.held_keys.retain(|k| !k.eq_ignore_ascii_case(key));
            }
        }
    }

    /// O vigia: solta o lease de `run_id` sem renovar há mais de 30 s.
    pub fn expire_idle(&self, run_id: &str, now: Instant, desk: &dyn Desk) -> Option<Expired> {
        let taken = {
            let mut slot = self.lease.lock().unwrap();
            match slot.as_ref() {
                Some(l) if l.run_id == run_id && now.saturating_duration_since(l.last_renew) > self.idle => slot.take(),
                _ => None,
            }
        };
        taken.map(|lease| {
            let (run_id, user_id) = (lease.run_id.clone(), lease.user_id);
            let released_keys = release_keys(lease, desk);
            Expired {
                run_id,
                user_id,
                released_keys,
            }
        })
    }

    /// O app está fechando: solta o lease de quem for, com as teclas.
    pub fn release_current(&self, desk: &dyn Desk) -> Vec<String> {
        let taken = self.lease.lock().unwrap().take();
        taken.map(|lease| release_keys(lease, desk)).unwrap_or_default()
    }

    /// Fim da execução: o lease dela (se houver) é solto, com as teclas.
    pub fn release_all_for_run(&self, run_id: &str, desk: &dyn Desk) -> Vec<String> {
        let taken = {
            let mut slot = self.lease.lock().unwrap();
            match slot.as_ref() {
                Some(l) if l.run_id == run_id => slot.take(),
                _ => None,
            }
        };
        taken.map(|lease| release_keys(lease, desk)).unwrap_or_default()
    }
}

/// Solta as teclas na ordem inversa e só então o lock (o guarda cai no fim).
fn release_keys(lease: Lease, desk: &dyn Desk) -> Vec<String> {
    let mut released = Vec::new();
    for key in lease.held_keys.iter().rev() {
        desk.key(key, true);
        released.push(key.clone());
    }
    drop(lease);
    released
}
