static SERVER_STATE: std::sync::LazyLock<std::sync::Mutex<Option<ServerHandle>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(None));

struct ServerHandle {
    shutdown_tx: watch::Sender<bool>,
    port: u16,
}

#[derive(Clone)]
struct AppState {
    accounts: &'static AccountStore,
    settings: &'static SettingsStore,
}

/// `AppState` is cloned into every request by the `Extension` layer. It must
/// keep pointing at the one process-wide store pair, never at a copy, or a
/// request would read a stale account list.
#[cfg(test)]
mod server_state_tests {
    use super::*;

    #[test]
    fn cloning_the_state_keeps_the_same_stores() {
        let settings_path = super::server_helpers_tests::unique_path("state-clone", "ini");
        let accounts_path = super::server_helpers_tests::unique_path("state-clone", "json");

        let settings: &'static SettingsStore =
            Box::leak(Box::new(SettingsStore::new(settings_path.clone())));
        let accounts: &'static AccountStore =
            Box::leak(Box::new(AccountStore::new(accounts_path.clone())));

        let state = AppState { accounts, settings };
        let cloned = state.clone();

        assert!(std::ptr::eq(state.accounts, cloned.accounts));
        assert!(std::ptr::eq(state.settings, cloned.settings));

        // A write through one handle is visible through the other.
        state
            .accounts
            .add(crate::data::accounts::Account::new(
                "token".to_string(),
                "alt_one".to_string(),
                1,
            ))
            .unwrap();
        assert_eq!(cloned.accounts.get_all().unwrap().len(), 1);

        let _ = std::fs::remove_file(&accounts_path);
        let _ = std::fs::remove_file(&settings_path);
    }
}
