use std::sync::atomic::AtomicU64;

#[derive(Debug, Clone, Serialize)]
pub struct TrackedProcess {
    pub pid: u32,
    pub user_id: i64,
    pub browser_tracker_id: String,
    #[serde(default)]
    pub version_id: Option<String>,
    /// Cliente aberto fora do app (pelo site) e reconhecido pelo log do
    /// Roblox ou identificado à mão — ver `external_clients.rs`.
    #[serde(default)]
    pub adopted: bool,
}

#[derive(Debug, Clone)]
pub struct PendingLaunch {
    pub id: u64,
    pub user_id: i64,
    pub version_id: Option<String>,
    pub deadline: std::time::Instant,
}

pub struct ProcessTracker {
    instances: Mutex<HashMap<i64, TrackedProcess>>,
    pending_launches: Mutex<Vec<PendingLaunch>>,
    next_pending_id: AtomicU64,
    job_handles: Mutex<HashMap<u32, SendHandle>>,
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
            pending_launches: Mutex::new(Vec::new()),
            next_pending_id: AtomicU64::new(1),
            job_handles: Mutex::new(HashMap::new()),
            watcher_active: AtomicBool::new(false),
            watcher_session: AtomicU64::new(0),
            watcher_state_lock: Mutex::new(()),
            launcher_cancelled: AtomicBool::new(false),
            next_account: AtomicBool::new(false),
        }
    }

    pub fn add_pending_launch(
        &self,
        user_id: i64,
        version_id: Option<String>,
        timeout: Duration,
    ) -> u64 {
        let id = self
            .next_pending_id
            .fetch_add(1, Ordering::Relaxed);
        if let Ok(mut pending) = self.pending_launches.lock() {
            pending.retain(|p| p.deadline > std::time::Instant::now());
            pending.push(PendingLaunch {
                id,
                user_id,
                version_id,
                deadline: std::time::Instant::now() + timeout,
            });
        }
        id
    }

    pub fn clear_pending_launch(&self, id: u64) {
        if let Ok(mut pending) = self.pending_launches.lock() {
            pending.retain(|p| p.id != id);
        }
    }

    fn prune_pending_locked(pending: &mut Vec<PendingLaunch>) {
        let now = std::time::Instant::now();
        pending.retain(|p| p.deadline > now);
    }

    pub fn track(&self, user_id: i64, pid: u32, browser_tracker_id: String) {
        self.track_with_version(user_id, pid, browser_tracker_id, None);
    }

    pub fn track_with_version(
        &self,
        user_id: i64,
        pid: u32,
        browser_tracker_id: String,
        version_id: Option<String>,
    ) {
        self.insert_tracked(TrackedProcess {
            pid,
            user_id,
            browser_tracker_id,
            version_id,
            adopted: false,
        });
    }

    /// Registra um cliente que o app não lançou (aberto pelo site). Daí em
    /// diante ele é igual a qualquer outro: Sessão, cliques AFK e Auto Rejoin
    /// o enxergam pelo mesmo `get_pid`.
    pub fn track_adopted(&self, user_id: i64, pid: u32, browser_tracker_id: String) {
        self.insert_tracked(TrackedProcess {
            pid,
            user_id,
            browser_tracker_id,
            version_id: None,
            adopted: true,
        });
    }

    fn insert_tracked(&self, process: TrackedProcess) {
        if let Ok(mut instances) = self.instances.lock() {
            if let Some(previous) = instances.insert(process.user_id, process) {
                self.clear_job_handle_for_pid(previous.pid);
            }
        }
    }

    pub fn running_version_keys(&self) -> std::collections::HashSet<Option<String>> {
        let mut set = std::collections::HashSet::new();
        if let Ok(instances) = self.instances.lock() {
            for tracked in instances.values() {
                set.insert(tracked.version_id.clone());
            }
        }
        if let Ok(mut pending) = self.pending_launches.lock() {
            Self::prune_pending_locked(&mut pending);
            for p in pending.iter() {
                set.insert(p.version_id.clone());
            }
        }
        set
    }

    pub fn untrack(&self, user_id: i64) {
        if let Ok(mut instances) = self.instances.lock() {
            if let Some(previous) = instances.remove(&user_id) {
                self.clear_job_handle_for_pid(previous.pid);
            }
        }
    }

    pub fn get_pid(&self, user_id: i64) -> Option<u32> {
        self.instances
            .lock()
            .ok()
            .and_then(|i| i.get(&user_id).map(|p| p.pid))
    }

    #[allow(dead_code)]
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

    pub fn set_job_handle(&self, pid: u32, handle: HANDLE) {
        if handle.is_null() {
            return;
        }
        self.clear_job_handle_for_pid(pid);
        if let Ok(mut jobs) = self.job_handles.lock() {
            jobs.insert(pid, SendHandle(handle));
        } else {
            unsafe {
                CloseHandle(handle);
            }
        }
    }

    pub fn clear_job_handle_for_pid(&self, pid: u32) {
        let handle = self
            .job_handles
            .lock()
            .ok()
            .and_then(|mut jobs| jobs.remove(&pid));
        if let Some(SendHandle(handle)) = handle {
            unsafe {
                CloseHandle(handle);
            }
        }
    }

    pub fn kill_for_user(&self, user_id: i64) -> bool {
        if let Some(pid) = self.get_pid(user_id) {
            // The tracked client may have exited long ago and Windows may have
            // reused its PID for an unrelated process: never kill that.
            if !is_roblox_pid_alive(pid) {
                self.untrack(user_id);
                return true;
            }
            if kill_process(pid).is_ok() {
                let exited = wait_for_process_exit(pid, Duration::from_millis(1200));
                if exited {
                    self.untrack(user_id);
                }
                exited
            } else if !is_roblox_pid_alive(pid) {
                self.untrack(user_id);
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn kill_for_user_graceful(&self, user_id: i64, timeout_ms: u64) -> bool {
        let Some(pid) = self.get_pid(user_id) else {
            return true;
        };
        if !is_roblox_pid_alive(pid) {
            self.untrack(user_id);
            return true;
        }

        let exited = if kill_process(pid).is_ok() {
            wait_for_process_exit(pid, Duration::from_millis(timeout_ms.max(250)))
        } else {
            !is_roblox_pid_alive(pid)
        };

        if exited {
            self.untrack(user_id);
        }

        exited
    }

    pub async fn kill_for_user_async(&'static self, user_id: i64) -> bool {
        tokio::task::spawn_blocking(move || self.kill_for_user(user_id))
            .await
            .unwrap_or(false)
    }

    pub async fn kill_for_user_graceful_async(&'static self, user_id: i64, timeout_ms: u64) -> bool {
        tokio::task::spawn_blocking(move || self.kill_for_user_graceful(user_id, timeout_ms))
            .await
            .unwrap_or(false)
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

        Some(
            self.watcher_session
                .fetch_add(1, Ordering::SeqCst)
                .wrapping_add(1),
        )
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

    pub fn cleanup_dead_processes(&self) -> Vec<i64> {
        let alive_pids = get_roblox_pids();
        let mut dead_user_ids = Vec::new();
        let mut dead_pids = Vec::new();

        if let Ok(mut instances) = self.instances.lock() {
            instances.retain(|user_id, process| {
                if alive_pids.contains(&process.pid) {
                    true
                } else {
                    dead_user_ids.push(*user_id);
                    dead_pids.push(process.pid);
                    false
                }
            });
        }

        for pid in dead_pids {
            self.clear_job_handle_for_pid(pid);
        }

        if let Ok(mut pending) = self.pending_launches.lock() {
            Self::prune_pending_locked(&mut pending);
        }

        dead_user_ids
    }
}

#[cfg(test)]
mod win_tracker_tests {
    use super::*;

    // Every test builds its own `ProcessTracker`, so nothing here touches the
    // process-wide tracker or any real Roblox client. `kill_for_user*` is not
    // covered: it calls OpenProcess/TerminateProcess.

    fn tracker_for_test() -> ProcessTracker {
        ProcessTracker::new()
    }

    // PIDs used purely as bookkeeping keys. They are never opened or killed.
    const FAKE_PID_A: u32 = 4_294_967_290;
    const FAKE_PID_B: u32 = 4_294_967_291;

    // ── track / untrack / lookup ───────────────────────────────────────────

    #[test]
    fn a_fresh_tracker_knows_nothing() {
        let tracker = tracker_for_test();
        assert!(tracker.get_pid(1).is_none());
        assert!(tracker.get_all().is_empty());
        assert!(tracker.get_tracked_pids().is_empty());
        assert!(!tracker.is_launch_cancelled());
        assert!(!tracker.is_next_account());
        assert!(!tracker.is_watcher_active());
    }

    #[test]
    fn track_records_the_pid_and_browser_tracker_id_per_account() {
        let tracker = tracker_for_test();
        tracker.track(11, FAKE_PID_A, "bt-11".into());
        tracker.track(22, FAKE_PID_B, "bt-22".into());

        assert_eq!(tracker.get_pid(11), Some(FAKE_PID_A));
        assert_eq!(tracker.get_pid(22), Some(FAKE_PID_B));
        assert_eq!(tracker.get_pid(33), None);

        let mut all = tracker.get_all();
        all.sort_by_key(|p| p.user_id);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].user_id, 11);
        assert_eq!(all[0].browser_tracker_id, "bt-11");
        assert_eq!(all[0].version_id, None);
        assert_eq!(all[1].user_id, 22);
    }

    #[test]
    fn track_with_version_stores_the_catalog_version() {
        let tracker = tracker_for_test();
        tracker.track_with_version(11, FAKE_PID_A, "bt".into(), Some("LIVE:version-aa".into()));
        let all = tracker.get_all();
        assert_eq!(all[0].version_id.as_deref(), Some("LIVE:version-aa"));
    }

    #[test]
    fn tracking_the_same_account_again_replaces_the_previous_entry() {
        let tracker = tracker_for_test();
        tracker.track_with_version(11, FAKE_PID_A, "old".into(), Some("LIVE:v1".into()));
        tracker.track_with_version(11, FAKE_PID_B, "new".into(), Some("LIVE:v2".into()));

        assert_eq!(tracker.get_all().len(), 1, "one entry per account");
        assert_eq!(tracker.get_pid(11), Some(FAKE_PID_B));
        assert_eq!(tracker.get_all()[0].browser_tracker_id, "new");
        assert_eq!(tracker.get_all()[0].version_id.as_deref(), Some("LIVE:v2"));
    }

    #[test]
    fn untrack_removes_only_the_named_account() {
        let tracker = tracker_for_test();
        tracker.track(11, FAKE_PID_A, "a".into());
        tracker.track(22, FAKE_PID_B, "b".into());

        tracker.untrack(11);
        assert!(tracker.get_pid(11).is_none());
        assert_eq!(tracker.get_pid(22), Some(FAKE_PID_B));

        tracker.untrack(11); // idempotent
        tracker.untrack(999); // unknown account is a no-op
        assert_eq!(tracker.get_all().len(), 1);
    }

    #[test]
    fn get_tracked_pids_lists_every_tracked_pid() {
        let tracker = tracker_for_test();
        tracker.track(11, FAKE_PID_A, "a".into());
        tracker.track(22, FAKE_PID_B, "b".into());
        let mut pids = tracker.get_tracked_pids();
        pids.sort_unstable();
        assert_eq!(pids, vec![FAKE_PID_A, FAKE_PID_B]);
    }

    #[test]
    fn tracked_process_serializes_for_the_frontend() {
        let tracker = tracker_for_test();
        tracker.track_with_version(11, FAKE_PID_A, "bt".into(), Some("LIVE:v1".into()));
        let json = serde_json::to_value(&tracker.get_all()[0]).unwrap();
        assert_eq!(json["pid"], FAKE_PID_A);
        assert_eq!(json["user_id"], 11);
        assert_eq!(json["browser_tracker_id"], "bt");
        assert_eq!(json["version_id"], "LIVE:v1");
    }

    #[test]
    fn a_client_adopted_from_outside_is_tracked_and_marked() {
        let tracker = tracker_for_test();
        tracker.track(11, FAKE_PID_A, "a".into());
        tracker.track_adopted(22, FAKE_PID_B, "b".into());

        assert_eq!(tracker.get_pid(22), Some(FAKE_PID_B));
        let mut all = tracker.get_all();
        all.sort_by_key(|p| p.user_id);
        assert!(!all[0].adopted, "a client the app launched is not adopted");
        assert!(all[1].adopted);
        assert_eq!(all[1].version_id, None);

        // Um launch do app depois troca o registro e tira a marca.
        tracker.track(22, FAKE_PID_A, "b".into());
        assert!(!tracker.get_all().iter().any(|p| p.adopted));
    }

    // ── pending launches ───────────────────────────────────────────────────

    #[test]
    fn pending_launch_ids_are_unique_and_increasing() {
        let tracker = tracker_for_test();
        let a = tracker.add_pending_launch(11, None, Duration::from_secs(30));
        let b = tracker.add_pending_launch(22, None, Duration::from_secs(30));
        let c = tracker.add_pending_launch(33, None, Duration::from_secs(30));
        assert!(a < b && b < c, "ids must increase: {} {} {}", a, b, c);
    }

    #[test]
    fn a_pending_launch_reserves_its_version_before_a_pid_exists() {
        let tracker = tracker_for_test();
        tracker.add_pending_launch(11, Some("LIVE:v1".into()), Duration::from_secs(30));

        let keys = tracker.running_version_keys();
        assert!(keys.contains(&Some("LIVE:v1".to_string())));
        assert_eq!(keys.len(), 1);
    }

    #[test]
    fn clear_pending_launch_drops_only_that_launch() {
        let tracker = tracker_for_test();
        let a = tracker.add_pending_launch(11, Some("LIVE:v1".into()), Duration::from_secs(30));
        tracker.add_pending_launch(22, Some("LIVE:v2".into()), Duration::from_secs(30));

        tracker.clear_pending_launch(a);
        let keys = tracker.running_version_keys();
        assert!(!keys.contains(&Some("LIVE:v1".to_string())));
        assert!(keys.contains(&Some("LIVE:v2".to_string())));

        tracker.clear_pending_launch(a); // clearing twice is harmless
        tracker.clear_pending_launch(u64::MAX); // unknown id is harmless
        assert_eq!(tracker.running_version_keys().len(), 1);
    }

    #[test]
    fn an_expired_pending_launch_stops_reserving_its_version() {
        let tracker = tracker_for_test();
        tracker.add_pending_launch(11, Some("LIVE:expired".into()), Duration::from_millis(1));
        std::thread::sleep(Duration::from_millis(15));

        let keys = tracker.running_version_keys();
        assert!(
            !keys.contains(&Some("LIVE:expired".to_string())),
            "an expired launch must not block a new one"
        );
        assert!(keys.is_empty());
    }

    #[test]
    fn adding_a_pending_launch_prunes_the_expired_ones() {
        let tracker = tracker_for_test();
        tracker.add_pending_launch(11, Some("LIVE:expired".into()), Duration::from_millis(1));
        std::thread::sleep(Duration::from_millis(15));
        tracker.add_pending_launch(22, Some("LIVE:fresh".into()), Duration::from_secs(30));

        let keys = tracker.running_version_keys();
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&Some("LIVE:fresh".to_string())));
    }

    // ── running_version_keys ───────────────────────────────────────────────

    #[test]
    fn running_version_keys_is_empty_for_an_idle_tracker() {
        assert!(tracker_for_test().running_version_keys().is_empty());
    }

    #[test]
    fn running_version_keys_merges_tracked_clients_and_pending_launches() {
        let tracker = tracker_for_test();
        tracker.track_with_version(11, FAKE_PID_A, "a".into(), Some("LIVE:v1".into()));
        tracker.track_with_version(22, FAKE_PID_B, "b".into(), None);
        tracker.add_pending_launch(33, Some("LIVE:v2".into()), Duration::from_secs(30));

        let keys = tracker.running_version_keys();
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&Some("LIVE:v1".to_string())));
        assert!(keys.contains(&Some("LIVE:v2".to_string())));
        assert!(keys.contains(&None), "the default install is a key of its own");
    }

    #[test]
    fn running_version_keys_deduplicates_the_same_version() {
        let tracker = tracker_for_test();
        tracker.track_with_version(11, FAKE_PID_A, "a".into(), Some("LIVE:v1".into()));
        tracker.track_with_version(22, FAKE_PID_B, "b".into(), Some("LIVE:v1".into()));
        tracker.add_pending_launch(33, Some("LIVE:v1".into()), Duration::from_secs(30));

        assert_eq!(tracker.running_version_keys().len(), 1);
    }

    // ── launch cancellation / next-account signal ──────────────────────────

    #[test]
    fn the_launch_cancelled_flag_is_set_and_reset_explicitly() {
        let tracker = tracker_for_test();
        assert!(!tracker.is_launch_cancelled());

        tracker.cancel_launch();
        assert!(tracker.is_launch_cancelled());
        tracker.cancel_launch(); // setting twice stays set
        assert!(tracker.is_launch_cancelled());

        tracker.reset_launch_cancelled();
        assert!(!tracker.is_launch_cancelled());
        tracker.reset_launch_cancelled(); // resetting twice stays clear
        assert!(!tracker.is_launch_cancelled());
    }

    #[test]
    fn the_next_account_signal_is_independent_of_the_cancel_flag() {
        let tracker = tracker_for_test();
        tracker.signal_next_account();
        assert!(tracker.is_next_account());
        assert!(!tracker.is_launch_cancelled());

        tracker.reset_next_account();
        assert!(!tracker.is_next_account());

        tracker.cancel_launch();
        assert!(!tracker.is_next_account());
    }

    #[test]
    fn the_cancel_flag_is_visible_across_threads() {
        let tracker = std::sync::Arc::new(tracker_for_test());
        let writer = tracker.clone();
        std::thread::spawn(move || writer.cancel_launch())
            .join()
            .unwrap();
        assert!(tracker.is_launch_cancelled());
    }

    // ── watcher session bookkeeping ────────────────────────────────────────

    #[test]
    fn try_start_watcher_only_succeeds_once_until_it_is_stopped() {
        let tracker = tracker_for_test();
        let session = tracker.try_start_watcher().expect("first start wins");
        assert!(tracker.is_watcher_active());
        assert!(
            tracker.try_start_watcher().is_none(),
            "a second watcher must not start"
        );

        tracker.stop_watcher();
        assert!(!tracker.is_watcher_active());
        let restarted = tracker.try_start_watcher().expect("restart after stop");
        assert_ne!(
            restarted, session,
            "a restart never reuses the old session id"
        );
    }

    #[test]
    fn is_watcher_session_active_only_matches_the_current_session() {
        let tracker = tracker_for_test();
        let first = tracker.try_start_watcher().unwrap();
        assert!(tracker.is_watcher_session_active(first));

        tracker.stop_watcher();
        assert!(!tracker.is_watcher_session_active(first));

        let second = tracker.try_start_watcher().unwrap();
        assert_ne!(first, second);
        assert!(tracker.is_watcher_session_active(second));
        assert!(
            !tracker.is_watcher_session_active(first),
            "a stale session must not keep running"
        );
    }

    #[test]
    fn set_watcher_active_invalidates_the_running_session() {
        let tracker = tracker_for_test();
        let session = tracker.try_start_watcher().unwrap();
        tracker.set_watcher_active(true);
        assert!(tracker.is_watcher_active());
        assert!(
            !tracker.is_watcher_session_active(session),
            "the session counter must advance"
        );

        tracker.set_watcher_active(false);
        assert!(!tracker.is_watcher_active());
    }

    // ── cleanup_dead_processes ─────────────────────────────────────────────

    #[test]
    fn cleanup_dead_processes_drops_accounts_whose_client_is_gone() {
        // FAKE_PID_* are near u32::MAX and are never real Roblox PIDs, so this
        // only reads the process list; nothing is killed.
        let tracker = tracker_for_test();
        tracker.track(11, FAKE_PID_A, "a".into());
        tracker.track(22, FAKE_PID_B, "b".into());

        let mut dead = tracker.cleanup_dead_processes();
        dead.sort_unstable();
        assert_eq!(dead, vec![11, 22]);
        assert!(tracker.get_all().is_empty());

        assert!(
            tracker.cleanup_dead_processes().is_empty(),
            "a second sweep has nothing left to do"
        );
    }

    #[test]
    fn cleanup_dead_processes_also_prunes_expired_pending_launches() {
        let tracker = tracker_for_test();
        tracker.add_pending_launch(11, Some("LIVE:expired".into()), Duration::from_millis(1));
        tracker.add_pending_launch(22, Some("LIVE:fresh".into()), Duration::from_secs(30));
        std::thread::sleep(Duration::from_millis(15));

        tracker.cleanup_dead_processes();
        let keys = tracker.running_version_keys();
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&Some("LIVE:fresh".to_string())));
    }

    #[test]
    fn clear_job_handle_for_pid_is_safe_when_nothing_is_stored() {
        let tracker = tracker_for_test();
        tracker.clear_job_handle_for_pid(FAKE_PID_A);
        tracker.untrack(11);
        assert!(tracker.get_all().is_empty());
    }

    #[test]
    fn set_job_handle_ignores_a_null_handle() {
        let tracker = tracker_for_test();
        tracker.set_job_handle(FAKE_PID_A, std::ptr::null_mut());
        // Nothing was stored, so clearing it must not try to close anything.
        tracker.clear_job_handle_for_pid(FAKE_PID_A);
    }
}
