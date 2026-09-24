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
