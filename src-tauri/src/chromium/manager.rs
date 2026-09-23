use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Child;
use std::sync::Mutex;

use tauri::AppHandle;

use super::download;

pub const LOGIN_KEY: i64 = i64::MIN;

#[derive(Default)]
pub struct ChromiumManager {
    children: Mutex<HashMap<i64, Child>>,
    login_cookie: Mutex<Option<String>>,
}

impl ChromiumManager {
    pub fn new() -> Self {
        Self::default()
    }

    fn profiles_root(app: &AppHandle) -> Result<PathBuf, String> {
        Ok(download::chromium_dir(app)?
            .parent()
            .ok_or("Could not resolve data directory")?
            .join("chromium-profiles"))
    }

    pub fn account_profile(app: &AppHandle, user_id: i64) -> Result<PathBuf, String> {
        Ok(Self::profiles_root(app)?.join(user_id.to_string()))
    }

    pub fn login_profile(app: &AppHandle) -> Result<PathBuf, String> {
        Ok(Self::profiles_root(app)?.join("_login"))
    }

    pub fn is_alive(&self, user_id: i64) -> bool {
        let mut children = self.children.lock().unwrap();
        match children.get_mut(&user_id) {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    pub fn track(&self, user_id: i64, child: Child) {
        let mut children = self.children.lock().unwrap();
        if let Some(mut old) = children.insert(user_id, child) {
            let _ = old.kill();
            let _ = old.wait();
        }
    }

    pub fn kill(&self, user_id: i64) {
        let child = self.children.lock().unwrap().remove(&user_id);
        if let Some(mut child) = child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    pub fn close_login_session(&self) {
        self.kill(LOGIN_KEY);
        *self.login_cookie.lock().unwrap() = None;
    }

    pub fn set_login_cookie(&self, cookie: Option<String>) {
        *self.login_cookie.lock().unwrap() = cookie;
    }

    pub fn login_cookie(&self) -> Option<String> {
        self.login_cookie.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod chromium_manager_tests {
    use super::*;

    // Profile keying and cookie state. Anything that owns a `Child` (track /
    // kill / is_alive) needs a real browser process and is left to manual
    // testing; only the states reachable without one are covered.

    #[test]
    fn a_new_manager_holds_no_cookie_and_no_children() {
        let manager = ChromiumManager::new();
        assert!(manager.login_cookie().is_none());
        assert!(!manager.is_alive(LOGIN_KEY));
        assert!(!manager.is_alive(1));
    }

    #[test]
    fn default_matches_new() {
        let manager = ChromiumManager::default();
        assert!(manager.login_cookie().is_none());
        assert!(!manager.is_alive(LOGIN_KEY));
    }

    #[test]
    fn the_login_cookie_can_be_set_replaced_and_cleared() {
        let manager = ChromiumManager::new();

        manager.set_login_cookie(Some("token-1".into()));
        assert_eq!(manager.login_cookie().as_deref(), Some("token-1"));

        manager.set_login_cookie(Some("token-2".into()));
        assert_eq!(manager.login_cookie().as_deref(), Some("token-2"));

        manager.set_login_cookie(None);
        assert!(manager.login_cookie().is_none());
    }

    #[test]
    fn closing_the_login_session_clears_the_captured_cookie() {
        // The cookie must never survive a session, otherwise the next login
        // could import the previous account.
        let manager = ChromiumManager::new();
        manager.set_login_cookie(Some("token".into()));
        manager.close_login_session();
        assert!(manager.login_cookie().is_none());
    }

    #[test]
    fn closing_a_session_that_never_started_is_harmless() {
        let manager = ChromiumManager::new();
        manager.close_login_session();
        manager.close_login_session();
        manager.kill(42);
        assert!(manager.login_cookie().is_none());
    }

    #[test]
    fn is_alive_is_false_for_an_account_that_was_never_tracked() {
        let manager = ChromiumManager::new();
        assert!(!manager.is_alive(0));
        assert!(!manager.is_alive(i64::MAX));
        assert!(!manager.is_alive(LOGIN_KEY));
    }

    #[test]
    fn the_login_key_can_never_collide_with_a_roblox_user_id() {
        // Roblox user ids are positive, so i64::MIN is a safe sentinel.
        assert_eq!(LOGIN_KEY, i64::MIN);
        assert!(LOGIN_KEY < 0);
    }

    #[test]
    fn the_manager_is_usable_from_several_threads() {
        let manager = std::sync::Arc::new(ChromiumManager::new());
        let writer = manager.clone();
        std::thread::spawn(move || writer.set_login_cookie(Some("from-thread".into())))
            .join()
            .unwrap();
        assert_eq!(manager.login_cookie().as_deref(), Some("from-thread"));
    }
}
