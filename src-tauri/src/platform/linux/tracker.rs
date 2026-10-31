// Quem é quem: conta -> processo raiz do cliente. Mesmo desenho do tracker do
// macOS (o resto do app usa a mesma forma), mais o netns da conta, que é como
// o fechamento acha tudo o que é dela sem tocar nos clientes das outras.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use serde::Serialize;

use super::process::{is_pid_alive, kill_account_tree, netns_id};

static TRACKER: LazyLock<ProcessTracker> = LazyLock::new(ProcessTracker::new);

pub fn tracker() -> &'static ProcessTracker {
    &TRACKER
}

#[derive(Debug, Clone, Serialize)]
pub struct TrackedProcess {
    pub pid: u32,
    pub user_id: i64,
    pub browser_tracker_id: String,
    /// netns próprio da conta; `None` no runner `custom`.
    #[serde(skip)]
    pub netns_id: Option<u64>,
}

pub struct ProcessTracker {
    instances: Mutex<HashMap<i64, TrackedProcess>>,
    watcher_active: AtomicBool,
    watcher_session: AtomicU64,
    watcher_state_lock: Mutex<()>,
    launcher_cancelled: AtomicBool,
    next_account: AtomicBool,
}

impl ProcessTracker {
    pub fn new() -> Self {
        Self {
            instances: Mutex::new(HashMap::new()),
            watcher_active: AtomicBool::new(false),
            watcher_session: AtomicU64::new(0),
            watcher_state_lock: Mutex::new(()),
            launcher_cancelled: AtomicBool::new(false),
            next_account: AtomicBool::new(false),
        }
    }

    /// Guarda o cliente da conta. Lê o netns agora: é só depois da rede isolada
    /// pronta que o processo já está no netns próprio.
    pub fn track(&self, user_id: i64, pid: u32, browser_tracker_id: String) {
        self.track_in_netns(user_id, pid, browser_tracker_id, netns_id(pid));
    }

    pub fn track_in_netns(
        &self,
        user_id: i64,
        pid: u32,
        browser_tracker_id: String,
        netns_id: Option<u64>,
    ) {
        if let Ok(mut instances) = self.instances.lock() {
            instances.insert(
                user_id,
                TrackedProcess { pid, user_id, browser_tracker_id, netns_id },
            );
        }
    }

    pub fn untrack(&self, user_id: i64) {
        if let Ok(mut instances) = self.instances.lock() {
            instances.remove(&user_id);
        }
    }

    pub fn get_pid(&self, user_id: i64) -> Option<u32> {
        self.instances
            .lock()
            .ok()
            .and_then(|i| i.get(&user_id).map(|p| p.pid))
    }

    pub fn get_tracked_pids(&self) -> Vec<u32> {
        self.instances
            .lock()
            .ok()
            .map(|i| i.values().map(|p| p.pid).collect())
            .unwrap_or_default()
    }

    pub fn get_all(&self) -> Vec<TrackedProcess> {
        self.instances
            .lock()
            .ok()
            .map(|i| i.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Fecha o cliente desta conta — só a árvore dela.
    pub fn kill_for_user(&self, user_id: i64) -> bool {
        let tracked = self
            .instances
            .lock()
            .ok()
            .and_then(|i| i.get(&user_id).cloned());
        match tracked {
            Some(process) => {
                let result = kill_account_tree(process.pid, process.netns_id).is_ok();
                self.untrack(user_id);
                result
            }
            None => false,
        }
    }

    pub fn is_watcher_active(&self) -> bool {
        self.watcher_active.load(Ordering::SeqCst)
    }

    fn lock_watcher_state(&self) -> std::sync::MutexGuard<'_, ()> {
        match self.watcher_state_lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub fn set_watcher_active(&self, active: bool) {
        let _guard = self.lock_watcher_state();
        self.watcher_active.store(active, Ordering::SeqCst);
        self.watcher_session.fetch_add(1, Ordering::SeqCst);
    }

    pub fn try_start_watcher(&self) -> Option<u64> {
        let _guard = self.lock_watcher_state();
        if self.watcher_active.load(Ordering::SeqCst) {
            return None;
        }
        self.watcher_active.store(true, Ordering::SeqCst);
        Some(self.watcher_session.fetch_add(1, Ordering::SeqCst).wrapping_add(1))
    }

    pub fn stop_watcher(&self) {
        let _guard = self.lock_watcher_state();
        self.watcher_active.store(false, Ordering::SeqCst);
        self.watcher_session.fetch_add(1, Ordering::SeqCst);
    }

    pub fn is_watcher_session_active(&self, session: u64) -> bool {
        self.watcher_active.load(Ordering::SeqCst)
            && self.watcher_session.load(Ordering::SeqCst) == session
    }

    pub fn cancel_launch(&self) {
        self.launcher_cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_launch_cancelled(&self) -> bool {
        self.launcher_cancelled.load(Ordering::Relaxed)
    }

    pub fn reset_launch_cancelled(&self) {
        self.launcher_cancelled.store(false, Ordering::Relaxed);
    }

    pub fn signal_next_account(&self) {
        self.next_account.store(true, Ordering::Relaxed);
    }

    pub fn is_next_account(&self) -> bool {
        self.next_account.load(Ordering::Relaxed)
    }

    pub fn reset_next_account(&self) {
        self.next_account.store(false, Ordering::Relaxed);
    }

    /// Tira do rastreio quem já fechou e devolve as contas correspondentes.
    pub fn cleanup_dead_processes(&self) -> Vec<i64> {
        let mut dead_user_ids = Vec::new();
        if let Ok(mut instances) = self.instances.lock() {
            instances.retain(|user_id, process| {
                if is_pid_alive(process.pid) {
                    true
                } else {
                    dead_user_ids.push(*user_id);
                    false
                }
            });
        }
        dead_user_ids
    }
}
