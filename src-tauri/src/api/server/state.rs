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
