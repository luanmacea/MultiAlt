use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Child;
use std::sync::{Mutex, MutexGuard};

use tauri::AppHandle;

use super::download;

pub const LOGIN_KEY: i64 = i64::MIN;

/// Pega o lock tolerando envenenamento.
///
/// Um panic enquanto o guard estava vivo envenena o `Mutex` para sempre; com
/// `.lock().unwrap()` toda chamada seguinte entraria em panic e o manager
/// pararia de matar browsers e de limpar o cookie de login — exatamente as
/// duas coisas que não podem falhar por motivos de segurança. Os dados
/// protegidos continuam estruturalmente válidos, então recuperamos o guard
/// (`into_inner`) em vez de propagar o panic.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

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
        let mut children = lock(&self.children);
        match children.get_mut(&user_id) {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    pub fn track(&self, user_id: i64, child: Child) {
        let mut children = lock(&self.children);
        if let Some(mut old) = children.insert(user_id, child) {
            let _ = old.kill();
            let _ = old.wait();
        }
    }

    pub fn kill(&self, user_id: i64) {
        let child = lock(&self.children).remove(&user_id);
        if let Some(mut child) = child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Fecha só a janela de login, mantendo o cookie já capturado em memória.
    ///
    /// É o que permite derrubar a porta de debug no instante em que o cookie
    /// chega, sem perder o valor que o frontend ainda vai buscar com
    /// `extract_browser_cookie`.
    pub fn close_login_window(&self) {
        self.kill(LOGIN_KEY);
    }

    pub fn close_login_session(&self) {
        self.kill(LOGIN_KEY);
        *lock(&self.login_cookie) = None;
    }

    pub fn set_login_cookie(&self, cookie: Option<String>) {
        *lock(&self.login_cookie) = cookie;
    }

    pub fn login_cookie(&self) -> Option<String> {
        lock(&self.login_cookie).clone()
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

    /// Envenena um mutex do manager fazendo um panic com o guard vivo.
    fn poison<T: Send + Sync + 'static>(mutex: &'static Mutex<T>) {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let _ = std::panic::catch_unwind(|| {
            let _guard = mutex.lock().unwrap();
            panic!("envenena o lock");
        });
        std::panic::set_hook(previous);
        assert!(mutex.is_poisoned());
    }

    #[test]
    fn a_poisoned_cookie_lock_still_lets_the_cookie_be_stored_and_cleared() {
        // Regressão: com `.lock().unwrap()`, um panic segurando o lock fazia
        // TODA chamada seguinte entrar em panic — inclusive a que limpa o
        // cookie de sessão da memória.
        let manager: &'static ChromiumManager = Box::leak(Box::new(ChromiumManager::new()));
        poison(&manager.login_cookie);

        manager.set_login_cookie(Some("token".into()));
        assert_eq!(manager.login_cookie().as_deref(), Some("token"));

        manager.close_login_session();
        assert!(manager.login_cookie().is_none());
    }

    #[test]
    fn a_poisoned_children_lock_still_lets_windows_be_queried_and_killed() {
        let manager: &'static ChromiumManager = Box::leak(Box::new(ChromiumManager::new()));
        poison(&manager.children);

        assert!(!manager.is_alive(LOGIN_KEY));
        manager.kill(7);
        manager.close_login_window();
        manager.close_login_session();
    }

    #[test]
    fn closing_the_login_window_keeps_the_captured_cookie() {
        // A janela (e com ela a porta de debug) fecha assim que o cookie
        // aparece, mas o valor precisa sobreviver até o frontend buscá-lo.
        let manager = ChromiumManager::new();
        manager.set_login_cookie(Some("token".into()));
        manager.close_login_window();
        assert_eq!(manager.login_cookie().as_deref(), Some("token"));

        manager.close_login_session();
        assert!(manager.login_cookie().is_none());
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
