const DEFAULT_BLOXGEN_ENDPOINT: &str = "https://core.bloxgen.net";
const GENERATOR_TRANSIENT_BACKOFF_MS: i64 = 15_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GeneratorProvider {
    BloxGen,
}

impl GeneratorProvider {
    fn from_id(id: &str) -> Result<Self, String> {
        match id.trim().to_ascii_lowercase().as_str() {
            "bloxgen" => Ok(GeneratorProvider::BloxGen),
            other => Err(format!("Unknown generator provider: {}", other)),
        }
    }

    fn id(&self) -> &'static str {
        match self {
            GeneratorProvider::BloxGen => "bloxgen",
        }
    }

    fn default_endpoint(&self) -> &'static str {
        match self {
            GeneratorProvider::BloxGen => DEFAULT_BLOXGEN_ENDPOINT,
        }
    }

    fn default_account_type(&self) -> &'static str {
        match self {
            GeneratorProvider::BloxGen => "alt",
        }
    }
}

/// Falls back to the provider's own endpoint when the user left the field
/// blank, and drops a trailing slash so `{base}/api/...` never doubles up.
fn normalize_generator_endpoint(provider: GeneratorProvider, endpoint: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        provider.default_endpoint().to_string()
    } else {
        trimmed.to_string()
    }
}

/// Falls back to the provider's default account type when left blank.
fn normalize_generator_account_type(provider: GeneratorProvider, account_type: &str) -> String {
    let trimmed = account_type.trim();
    if trimmed.is_empty() {
        provider.default_account_type().to_string()
    } else {
        trimmed.to_string()
    }
}

/// Extra pause the user adds between generations, clamped to 0..=1h.
fn generator_extra_delay_ms(extra_delay_seconds: i64) -> i64 {
    extra_delay_seconds.clamp(0, 3600) * 1000
}

#[derive(Debug, Clone)]
struct GeneratorConfig {
    provider: GeneratorProvider,
    endpoint: String,
    api_key: String,
    account_type: String,
    extra_delay_ms: i64,
    target_group: String,
    max_accounts: i64,
    max_consecutive_failures: i64,
}

#[derive(Debug, Clone)]
struct GeneratorRuntime {
    active: bool,
    phase: String,
    total_generated: i64,
    next_attempt_at_ms: Option<i64>,
    last_username: Option<String>,
    last_user_id: Option<i64>,
    last_error: Option<String>,
    last_generated_at_ms: Option<i64>,
}

#[derive(Clone)]
struct GeneratorSession {
    id: u64,
    stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    stopped_notify: std::sync::Arc<tokio::sync::Notify>,
    started_at_ms: i64,
    config: std::sync::Arc<std::sync::Mutex<GeneratorConfig>>,
    runtime: std::sync::Arc<std::sync::Mutex<GeneratorRuntime>>,
}

struct GeneratorManager {
    session: std::sync::Mutex<Option<GeneratorSession>>,
    next_id: std::sync::atomic::AtomicU64,
}

impl GeneratorManager {
    fn new() -> Self {
        Self {
            session: std::sync::Mutex::new(None),
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    fn next_session_id(&self) -> u64 {
        self.next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    fn get_session(&self) -> Option<GeneratorSession> {
        self.session.lock().ok().and_then(|s| s.as_ref().cloned())
    }

    fn replace_session(&self, session: Option<GeneratorSession>) {
        if let Ok(mut guard) = self.session.lock() {
            *guard = session;
        }
    }
}

static GENERATOR_MANAGER: std::sync::LazyLock<GeneratorManager> =
    std::sync::LazyLock::new(GeneratorManager::new);

#[derive(Debug, Clone, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct GeneratorStatusPayload {
    active: bool,
    started_at_ms: Option<i64>,
    provider: String,
    endpoint: String,
    account_type: String,
    extra_delay_seconds: i64,
    target_group: String,
    max_accounts: i64,
    phase: String,
    next_attempt_at_ms: Option<i64>,
    total_generated: i64,
    last_username: Option<String>,
    last_user_id: Option<i64>,
    last_error: Option<String>,
    last_generated_at_ms: Option<i64>,
}

struct GeneratedAccount {
    cookie: String,
    password: String,
    username: String,
    user_id: Option<i64>,
}

enum GenerateOutcome {
    Account(Box<GeneratedAccount>),
    Cooldown(i64),
    Fatal(String),
    Transient(String),
}

fn generator_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

fn response_snippet(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "Empty response".to_string();
    }
    trimmed.chars().take(200).collect()
}

fn set_generator_runtime<F>(runtime: &std::sync::Mutex<GeneratorRuntime>, update: F)
where
    F: FnOnce(&mut GeneratorRuntime),
{
    if let Ok(mut guard) = runtime.lock() {
        update(&mut guard);
    }
}

async fn sleep_interruptible(stop_flag: &std::sync::atomic::AtomicBool, total_ms: i64) {
    let mut remaining = total_ms.max(0);
    while remaining > 0 {
        if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let chunk = remaining.min(250);
        tokio::time::sleep(std::time::Duration::from_millis(chunk as u64)).await;
        remaining -= chunk;
    }
}

async fn provider_generate(
    client: &reqwest::Client,
    config: &GeneratorConfig,
) -> GenerateOutcome {
    match config.provider {
        GeneratorProvider::BloxGen => bloxgen_generate(client, config).await,
    }
}

async fn provider_test_key(
    provider: GeneratorProvider,
    endpoint: &str,
    api_key: &str,
) -> Result<f64, String> {
    match provider {
        GeneratorProvider::BloxGen => bloxgen_test_key(endpoint, api_key).await,
    }
}

async fn add_generated_account(
    app: &tauri::AppHandle,
    account: &GeneratedAccount,
    target_group: &str,
    stop_flag: &std::sync::atomic::AtomicBool,
) -> Result<Option<(i64, String)>, String> {
    if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(None);
    }

    let cookie = account.cookie.clone();
    let (user_id, username) = match api::auth::validate_cookie(&cookie).await {
        Ok(info) => (info.user_id, info.name),
        Err(e) => match account.user_id {
            Some(id) if id > 0 && !account.username.is_empty() => (id, account.username.clone()),
            _ => return Err(format!("Generated cookie failed validation: {}", e)),
        },
    };

    if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(None);
    }

    let group = if target_group.trim().is_empty() {
        "Default".to_string()
    } else {
        target_group.trim().to_string()
    };

    let mut model = data::accounts::Account::new(cookie, username.clone(), user_id);
    model.password = account.password.clone();
    model.group = group;

    let state = app.state::<AccountStore>();
    state.add(model)?;

    Ok(Some((user_id, username)))
}

async fn run_generator_session(
    app: tauri::AppHandle,
    session_id: u64,
    stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    stopped_notify: std::sync::Arc<tokio::sync::Notify>,
    config: std::sync::Arc<std::sync::Mutex<GeneratorConfig>>,
    runtime: std::sync::Arc<std::sync::Mutex<GeneratorRuntime>>,
) {
    let cfg = match config.lock() {
        Ok(cfg) => cfg.clone(),
        Err(_) => return,
    };
    let client = generator_client();
    let mut consecutive_failures: i64 = 0;

    loop {
        if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }

        set_generator_runtime(&runtime, |r| {
            r.phase = "generating".to_string();
            r.next_attempt_at_ms = None;
        });
        emit_generator_status(&app);

        let outcome = provider_generate(&client, &cfg).await;
        if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }

        match outcome {
            GenerateOutcome::Account(account) => {
                set_generator_runtime(&runtime, |r| r.phase = "adding".to_string());
                emit_generator_status(&app);

                match add_generated_account(&app, &account, &cfg.target_group, &stop_flag).await {
                    Ok(Some((user_id, username))) => {
                        consecutive_failures = 0;
                        let total = {
                            let mut total = 0;
                            set_generator_runtime(&runtime, |r| {
                                r.total_generated += 1;
                                r.last_user_id = Some(user_id);
                                r.last_username = Some(username.clone());
                                r.last_error = None;
                                r.last_generated_at_ms = Some(now_ms());
                                r.phase = "cooldown".to_string();
                                total = r.total_generated;
                            });
                            total
                        };
                        let _ = app.emit(
                            "generator-account-added",
                            serde_json::json!({ "userId": user_id, "username": username }),
                        );
                        emit_generator_status(&app);

                        if cfg.max_accounts > 0 && total >= cfg.max_accounts {
                            set_generator_runtime(&runtime, |r| {
                                r.active = false;
                                r.phase = "completed".to_string();
                                r.next_attempt_at_ms = None;
                            });
                            break;
                        }
                    }
                    Ok(None) => {
                        break;
                    }
                    Err(e) => {
                        consecutive_failures += 1;
                        if cfg.max_consecutive_failures > 0
                            && consecutive_failures >= cfg.max_consecutive_failures
                        {
                            set_generator_runtime(&runtime, |r| {
                                r.active = false;
                                r.phase = "error".to_string();
                                r.last_error = Some(format!(
                                    "Stopped after {} consecutive failures: {}",
                                    consecutive_failures, e
                                ));
                                r.next_attempt_at_ms = None;
                            });
                            break;
                        }
                        let wait = GENERATOR_TRANSIENT_BACKOFF_MS + cfg.extra_delay_ms;
                        set_generator_runtime(&runtime, |r| {
                            r.phase = "waiting".to_string();
                            r.last_error = Some(e);
                            r.next_attempt_at_ms = Some(now_ms() + wait);
                        });
                        emit_generator_status(&app);
                        sleep_interruptible(&stop_flag, wait).await;
                    }
                }
            }
            GenerateOutcome::Cooldown(ms) => {
                let wait = ms + cfg.extra_delay_ms;
                set_generator_runtime(&runtime, |r| {
                    r.phase = "cooldown".to_string();
                    r.last_error = None;
                    r.next_attempt_at_ms = Some(now_ms() + wait);
                });
                emit_generator_status(&app);
                sleep_interruptible(&stop_flag, wait).await;
            }
            GenerateOutcome::Transient(message) => {
                let wait = GENERATOR_TRANSIENT_BACKOFF_MS + cfg.extra_delay_ms;
                set_generator_runtime(&runtime, |r| {
                    r.phase = "waiting".to_string();
                    r.last_error = Some(message);
                    r.next_attempt_at_ms = Some(now_ms() + wait);
                });
                emit_generator_status(&app);
                sleep_interruptible(&stop_flag, wait).await;
            }
            GenerateOutcome::Fatal(message) => {
                set_generator_runtime(&runtime, |r| {
                    r.active = false;
                    r.phase = "error".to_string();
                    r.last_error = Some(message);
                    r.next_attempt_at_ms = None;
                });
                break;
            }
        }
    }

    set_generator_runtime(&runtime, |r| {
        r.active = false;
        if r.phase != "completed" && r.phase != "error" {
            r.phase = "stopped".to_string();
        }
        r.next_attempt_at_ms = None;
    });

    let _ = session_id;
    stopped_notify.notify_waiters();
    let _ = app.emit("generator-stopped", serde_json::json!({}));
    emit_generator_status(&app);
}

fn generator_status_from_session(session: &GeneratorSession) -> GeneratorStatusPayload {
    let cfg = session.config.lock().ok();
    let runtime = session.runtime.lock().ok();
    GeneratorStatusPayload {
        active: runtime.as_ref().map(|r| r.active).unwrap_or(false),
        started_at_ms: Some(session.started_at_ms),
        provider: cfg
            .as_ref()
            .map(|c| c.provider.id().to_string())
            .unwrap_or_default(),
        endpoint: cfg.as_ref().map(|c| c.endpoint.clone()).unwrap_or_default(),
        account_type: cfg.as_ref().map(|c| c.account_type.clone()).unwrap_or_default(),
        extra_delay_seconds: cfg.as_ref().map(|c| c.extra_delay_ms / 1000).unwrap_or(0),
        target_group: cfg.as_ref().map(|c| c.target_group.clone()).unwrap_or_default(),
        max_accounts: cfg.as_ref().map(|c| c.max_accounts).unwrap_or(0),
        phase: runtime.as_ref().map(|r| r.phase.clone()).unwrap_or_default(),
        next_attempt_at_ms: runtime.as_ref().and_then(|r| r.next_attempt_at_ms),
        total_generated: runtime.as_ref().map(|r| r.total_generated).unwrap_or(0),
        last_username: runtime.as_ref().and_then(|r| r.last_username.clone()),
        last_user_id: runtime.as_ref().and_then(|r| r.last_user_id),
        last_error: runtime.as_ref().and_then(|r| r.last_error.clone()),
        last_generated_at_ms: runtime.as_ref().and_then(|r| r.last_generated_at_ms),
    }
}

fn current_generator_status() -> GeneratorStatusPayload {
    if let Some(session) = GENERATOR_MANAGER.get_session() {
        generator_status_from_session(&session)
    } else {
        GeneratorStatusPayload::default()
    }
}

fn emit_generator_status(app: &tauri::AppHandle) {
    let _ = app.emit("generator-status", current_generator_status());
}

#[tauri::command]
async fn start_generator(
    app: tauri::AppHandle,
    provider: String,
    endpoint: String,
    api_key: String,
    account_type: String,
    extra_delay_seconds: i64,
    target_group: String,
    max_accounts: i64,
) -> Result<GeneratorStatusPayload, String> {
    let provider = GeneratorProvider::from_id(&provider)?;

    let api_key = api_key.trim().to_string();
    if api_key.is_empty() {
        return Err("An API key is required".to_string());
    }

    let endpoint = normalize_generator_endpoint(provider, &endpoint);
    let account_type = normalize_generator_account_type(provider, &account_type);
    let extra_delay_ms = generator_extra_delay_ms(extra_delay_seconds);
    let target_group = target_group.trim().to_string();
    let max_accounts = max_accounts.max(0);
    let max_consecutive_failures = app
        .state::<SettingsStore>()
        .get_int("Generator", "MaxConsecutiveFailures")
        .unwrap_or(3)
        .max(0);

    if let Some(existing) = GENERATOR_MANAGER.get_session() {
        existing
            .stop_flag
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            existing.stopped_notify.notified(),
        )
        .await;
    }

    let cfg = GeneratorConfig {
        provider,
        endpoint,
        api_key,
        account_type,
        extra_delay_ms,
        target_group,
        max_accounts,
        max_consecutive_failures,
    };
    let runtime = GeneratorRuntime {
        active: true,
        phase: "starting".to_string(),
        total_generated: 0,
        next_attempt_at_ms: None,
        last_username: None,
        last_user_id: None,
        last_error: None,
        last_generated_at_ms: None,
    };

    let session_id = GENERATOR_MANAGER.next_session_id();
    let stop_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stopped_notify = std::sync::Arc::new(tokio::sync::Notify::new());
    let session = GeneratorSession {
        id: session_id,
        stop_flag: stop_flag.clone(),
        stopped_notify: stopped_notify.clone(),
        started_at_ms: now_ms(),
        config: std::sync::Arc::new(std::sync::Mutex::new(cfg)),
        runtime: std::sync::Arc::new(std::sync::Mutex::new(runtime)),
    };

    GENERATOR_MANAGER.replace_session(Some(session.clone()));
    emit_generator_status(&app);

    tokio::spawn(run_generator_session(
        app.clone(),
        session_id,
        stop_flag,
        stopped_notify,
        session.config.clone(),
        session.runtime.clone(),
    ));

    Ok(current_generator_status())
}

#[tauri::command]
fn stop_generator(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(session) = GENERATOR_MANAGER.get_session() {
        session
            .stop_flag
            .store(true, std::sync::atomic::Ordering::Relaxed);
        set_generator_runtime(&session.runtime, |r| {
            r.active = false;
            if r.phase != "completed" && r.phase != "error" {
                r.phase = "stopped".to_string();
            }
            r.next_attempt_at_ms = None;
        });
    }
    let _ = app.emit("generator-stopped", serde_json::json!({}));
    emit_generator_status(&app);
    Ok(())
}

#[tauri::command]
fn get_generator_status() -> Result<GeneratorStatusPayload, String> {
    Ok(current_generator_status())
}

#[tauri::command]
async fn generator_test_key(
    provider: String,
    endpoint: String,
    api_key: String,
) -> Result<f64, String> {
    let provider = GeneratorProvider::from_id(&provider)?;
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err("An API key is required".to_string());
    }
    provider_test_key(provider, &endpoint, api_key).await
}

// ---------------------------------------------------------------------------
// Provider: BloxGen (https://docs.bloxgen.net)
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct BloxGenData {
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    cookie: String,
    #[serde(default)]
    id: Option<i64>,
}

#[derive(serde::Deserialize)]
struct BloxGenResponse {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    data: Option<BloxGenData>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(rename = "timeRemaining", default)]
    time_remaining: Option<i64>,
}

#[derive(serde::Deserialize)]
struct BloxGenBalanceData {
    #[serde(default)]
    balance: f64,
}

#[derive(serde::Deserialize)]
struct BloxGenBalanceResponse {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    data: Option<BloxGenBalanceData>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

fn bloxgen_base(endpoint: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    let base = if trimmed.is_empty() {
        DEFAULT_BLOXGEN_ENDPOINT
    } else {
        trimmed
    };
    base.trim_end_matches("/api/generate")
        .trim_end_matches('/')
        .to_string()
}

async fn bloxgen_generate(client: &reqwest::Client, config: &GeneratorConfig) -> GenerateOutcome {
    let url = format!("{}/api/generate", bloxgen_base(&config.endpoint));
    let body = serde_json::json!({ "apiKey": config.api_key, "type": config.account_type });
    let response = match client.post(&url).json(&body).send().await {
        Ok(response) => response,
        Err(e) => return GenerateOutcome::Transient(format!("Request failed: {}", e)),
    };

    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let parsed: Option<BloxGenResponse> = serde_json::from_str(&text).ok();

    if status.as_u16() == 200 {
        if let Some(payload) = parsed {
            if payload.success {
                if let Some(data) = payload.data {
                    if data.cookie.is_empty() {
                        return GenerateOutcome::Transient(
                            "Generated account did not include a cookie".to_string(),
                        );
                    }
                    return GenerateOutcome::Account(Box::new(GeneratedAccount {
                        cookie: data.cookie,
                        password: data.password,
                        username: data.username,
                        user_id: data.id,
                    }));
                }
                return GenerateOutcome::Transient(
                    "API returned success but no account data".to_string(),
                );
            }
            let message = payload
                .message
                .or(payload.error)
                .unwrap_or_else(|| "Account generation failed".to_string());
            return GenerateOutcome::Transient(message);
        }
        return GenerateOutcome::Transient(response_snippet(&text));
    }

    let message = parsed
        .as_ref()
        .and_then(|p| p.message.clone().or_else(|| p.error.clone()))
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| response_snippet(&text));

    match status.as_u16() {
        429 => match parsed.as_ref().and_then(|p| p.time_remaining) {
            Some(ms) if ms > 0 => GenerateOutcome::Cooldown(ms),
            _ => GenerateOutcome::Transient(message),
        },
        404 | 500 | 502 | 503 | 504 => GenerateOutcome::Transient(message),
        _ => GenerateOutcome::Fatal(message),
    }
}

async fn bloxgen_test_key(endpoint: &str, api_key: &str) -> Result<f64, String> {
    let url = format!("{}/api/balance", bloxgen_base(endpoint));
    let client = generator_client();
    let response = client
        .get(&url)
        .header("x-api-key", api_key)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let parsed: Option<BloxGenBalanceResponse> = serde_json::from_str(&text).ok();

    if status.is_success() {
        if let Some(payload) = parsed {
            if payload.success {
                if let Some(data) = payload.data {
                    return Ok(data.balance);
                }
            }
            return Err(payload
                .message
                .or(payload.error)
                .unwrap_or_else(|| "Failed to read balance".to_string()));
        }
        return Err(response_snippet(&text));
    }

    Err(parsed
        .and_then(|p| p.message.or(p.error))
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| format!("Balance check failed (status {})", status.as_u16())))
}

#[cfg(test)]
mod generator_command_tests {
    use super::*;

    // ---- GeneratorProvider --------------------------------------------------

    #[test]
    fn provider_from_id_accepts_the_known_provider_in_any_casing() {
        assert_eq!(
            GeneratorProvider::from_id("bloxgen").unwrap(),
            GeneratorProvider::BloxGen
        );
        assert_eq!(
            GeneratorProvider::from_id("  BloxGen  ").unwrap(),
            GeneratorProvider::BloxGen
        );
        assert_eq!(
            GeneratorProvider::from_id("BLOXGEN").unwrap(),
            GeneratorProvider::BloxGen
        );
    }

    #[test]
    fn provider_from_id_rejects_anything_else_and_names_it() {
        assert_eq!(
            GeneratorProvider::from_id("other").unwrap_err(),
            "Unknown generator provider: other"
        );
        assert_eq!(
            GeneratorProvider::from_id("").unwrap_err(),
            "Unknown generator provider: "
        );
        assert!(GeneratorProvider::from_id("блоксген").is_err());
    }

    #[test]
    fn provider_id_round_trips_through_from_id() {
        let provider = GeneratorProvider::BloxGen;
        assert_eq!(provider.id(), "bloxgen");
        assert_eq!(GeneratorProvider::from_id(provider.id()).unwrap(), provider);
    }

    #[test]
    fn provider_defaults_are_the_documented_bloxgen_values() {
        assert_eq!(
            GeneratorProvider::BloxGen.default_endpoint(),
            "https://core.bloxgen.net"
        );
        assert_eq!(GeneratorProvider::BloxGen.default_account_type(), "alt");
        assert_eq!(DEFAULT_BLOXGEN_ENDPOINT, "https://core.bloxgen.net");
    }

    // ---- config normalization ------------------------------------------------

    #[test]
    fn normalize_generator_endpoint_falls_back_to_the_provider_default() {
        let p = GeneratorProvider::BloxGen;
        assert_eq!(
            normalize_generator_endpoint(p, ""),
            "https://core.bloxgen.net"
        );
        assert_eq!(
            normalize_generator_endpoint(p, "   "),
            "https://core.bloxgen.net"
        );
        assert_eq!(
            normalize_generator_endpoint(p, "///"),
            "https://core.bloxgen.net"
        );
    }

    #[test]
    fn normalize_generator_endpoint_trims_whitespace_and_trailing_slashes() {
        let p = GeneratorProvider::BloxGen;
        assert_eq!(
            normalize_generator_endpoint(p, "  https://example.test/  "),
            "https://example.test"
        );
        assert_eq!(
            normalize_generator_endpoint(p, "https://example.test///"),
            "https://example.test"
        );
        assert_eq!(
            normalize_generator_endpoint(p, "https://example.test"),
            "https://example.test"
        );
    }

    #[test]
    fn normalize_generator_account_type_falls_back_to_the_provider_default() {
        let p = GeneratorProvider::BloxGen;
        assert_eq!(normalize_generator_account_type(p, ""), "alt");
        assert_eq!(normalize_generator_account_type(p, "  \t "), "alt");
        assert_eq!(normalize_generator_account_type(p, "  premium "), "premium");
    }

    #[test]
    fn generator_extra_delay_ms_clamps_to_zero_and_one_hour() {
        assert_eq!(generator_extra_delay_ms(0), 0);
        assert_eq!(generator_extra_delay_ms(1), 1_000);
        assert_eq!(generator_extra_delay_ms(-50), 0);
        assert_eq!(generator_extra_delay_ms(3_600), 3_600_000);
        assert_eq!(generator_extra_delay_ms(999_999), 3_600_000);
        assert_eq!(generator_extra_delay_ms(i64::MIN), 0);
        assert_eq!(generator_extra_delay_ms(i64::MAX), 3_600_000);
    }

    // ---- response_snippet ----------------------------------------------------

    #[test]
    fn response_snippet_labels_an_empty_body() {
        assert_eq!(response_snippet(""), "Empty response");
        assert_eq!(response_snippet("   \n\t "), "Empty response");
    }

    #[test]
    fn response_snippet_trims_and_caps_at_200_characters() {
        assert_eq!(response_snippet("  boom  "), "boom");
        let long = "x".repeat(500);
        assert_eq!(response_snippet(&long).chars().count(), 200);
    }

    #[test]
    fn response_snippet_counts_characters_not_bytes_for_unicode() {
        // Slicing by byte index would panic or split a character in half.
        let long = "é".repeat(500);
        let snippet = response_snippet(&long);
        assert_eq!(snippet.chars().count(), 200);
        assert!(snippet.chars().all(|c| c == 'é'));
    }

    // ---- bloxgen_base --------------------------------------------------------

    #[test]
    fn bloxgen_base_falls_back_to_the_default_endpoint() {
        assert_eq!(bloxgen_base(""), "https://core.bloxgen.net");
        assert_eq!(bloxgen_base("   "), "https://core.bloxgen.net");
        assert_eq!(bloxgen_base("/"), "https://core.bloxgen.net");
    }

    #[test]
    fn bloxgen_base_strips_a_pasted_api_generate_suffix() {
        // Users paste the full documented URL; appending /api/generate to it
        // again would 404.
        assert_eq!(
            bloxgen_base("https://core.bloxgen.net/api/generate"),
            "https://core.bloxgen.net"
        );
        assert_eq!(
            bloxgen_base("https://core.bloxgen.net/api/generate/"),
            "https://core.bloxgen.net"
        );
        assert_eq!(
            bloxgen_base("  https://core.bloxgen.net/api/generate  "),
            "https://core.bloxgen.net"
        );
    }

    #[test]
    fn bloxgen_base_keeps_a_custom_host_and_path_prefix() {
        assert_eq!(bloxgen_base("http://127.0.0.1:1234"), "http://127.0.0.1:1234");
        assert_eq!(
            bloxgen_base("http://127.0.0.1:1234/proxy/"),
            "http://127.0.0.1:1234/proxy"
        );
    }

    // ---- set_generator_runtime -----------------------------------------------

    fn test_runtime() -> GeneratorRuntime {
        GeneratorRuntime {
            active: true,
            phase: "starting".to_string(),
            total_generated: 0,
            next_attempt_at_ms: None,
            last_username: None,
            last_user_id: None,
            last_error: None,
            last_generated_at_ms: None,
        }
    }

    #[test]
    fn set_generator_runtime_applies_the_mutation_in_place() {
        let runtime = std::sync::Mutex::new(test_runtime());
        set_generator_runtime(&runtime, |r| {
            r.phase = "cooldown".to_string();
            r.total_generated += 3;
        });
        let guard = runtime.lock().unwrap();
        assert_eq!(guard.phase, "cooldown");
        assert_eq!(guard.total_generated, 3);
    }

    // ---- sleep_interruptible --------------------------------------------------

    #[tokio::test]
    async fn sleep_interruptible_returns_at_once_for_zero_or_negative_durations() {
        let flag = std::sync::atomic::AtomicBool::new(false);
        let started = std::time::Instant::now();
        sleep_interruptible(&flag, 0).await;
        sleep_interruptible(&flag, -5_000).await;
        assert!(started.elapsed() < std::time::Duration::from_millis(200));
    }

    #[tokio::test]
    async fn sleep_interruptible_gives_up_as_soon_as_the_stop_flag_is_set() {
        // A stopped generator must not keep the app waiting for a long cooldown.
        let flag = std::sync::atomic::AtomicBool::new(true);
        let started = std::time::Instant::now();
        sleep_interruptible(&flag, 60_000).await;
        assert!(started.elapsed() < std::time::Duration::from_millis(500));
    }

    #[tokio::test]
    async fn sleep_interruptible_waits_out_a_short_delay() {
        let flag = std::sync::atomic::AtomicBool::new(false);
        let started = std::time::Instant::now();
        sleep_interruptible(&flag, 300).await;
        assert!(started.elapsed() >= std::time::Duration::from_millis(250));
    }

    // ---- status payload --------------------------------------------------------

    fn test_session(config: GeneratorConfig, runtime: GeneratorRuntime) -> GeneratorSession {
        GeneratorSession {
            id: 1,
            stop_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            stopped_notify: std::sync::Arc::new(tokio::sync::Notify::new()),
            started_at_ms: 1_700_000_000_000,
            config: std::sync::Arc::new(std::sync::Mutex::new(config)),
            runtime: std::sync::Arc::new(std::sync::Mutex::new(runtime)),
        }
    }

    fn test_config() -> GeneratorConfig {
        GeneratorConfig {
            provider: GeneratorProvider::BloxGen,
            endpoint: "https://example.test".to_string(),
            api_key: "secret".to_string(),
            account_type: "alt".to_string(),
            extra_delay_ms: 5_000,
            target_group: "BloxGen".to_string(),
            max_accounts: 10,
            max_consecutive_failures: 3,
        }
    }

    #[test]
    fn generator_status_from_session_projects_config_and_runtime() {
        let mut runtime = test_runtime();
        runtime.phase = "cooldown".to_string();
        runtime.total_generated = 4;
        runtime.last_username = Some("alt42".to_string());
        runtime.last_user_id = Some(99);
        runtime.next_attempt_at_ms = Some(1_700_000_010_000);

        let status = generator_status_from_session(&test_session(test_config(), runtime));

        assert!(status.active);
        assert_eq!(status.started_at_ms, Some(1_700_000_000_000));
        assert_eq!(status.provider, "bloxgen");
        assert_eq!(status.endpoint, "https://example.test");
        assert_eq!(status.account_type, "alt");
        assert_eq!(status.extra_delay_seconds, 5);
        assert_eq!(status.target_group, "BloxGen");
        assert_eq!(status.max_accounts, 10);
        assert_eq!(status.phase, "cooldown");
        assert_eq!(status.total_generated, 4);
        assert_eq!(status.last_username.as_deref(), Some("alt42"));
        assert_eq!(status.last_user_id, Some(99));
        assert_eq!(status.next_attempt_at_ms, Some(1_700_000_010_000));
    }

    #[test]
    fn generator_status_never_exposes_the_api_key() {
        // Regression guard: the status payload is emitted to the frontend.
        let status = generator_status_from_session(&test_session(test_config(), test_runtime()));
        let json = serde_json::to_string(&status).unwrap();
        assert!(!json.contains("secret"), "api key leaked into the status: {json}");
    }

    #[test]
    fn generator_status_rounds_the_extra_delay_down_to_whole_seconds() {
        let mut cfg = test_config();
        cfg.extra_delay_ms = 1_999;
        let status = generator_status_from_session(&test_session(cfg, test_runtime()));
        assert_eq!(status.extra_delay_seconds, 1);
    }

    #[test]
    fn get_generator_status_reports_no_session_by_default() {
        let status = get_generator_status().expect("status should be readable");
        assert!(!status.active);
        assert_eq!(status.total_generated, 0);
        assert_eq!(status.started_at_ms, None);
        assert_eq!(status.provider, "");
    }

    #[test]
    fn generator_status_payload_serializes_with_camel_case_keys() {
        let status = generator_status_from_session(&test_session(test_config(), test_runtime()));
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["startedAtMs"], 1_700_000_000_000_i64);
        assert_eq!(json["extraDelaySeconds"], 5);
        assert_eq!(json["targetGroup"], "BloxGen");
        assert_eq!(json["maxAccounts"], 10);
        assert_eq!(json["totalGenerated"], 0);
    }

    // ---- generator_test_key argument validation --------------------------------

    #[tokio::test]
    async fn generator_test_key_rejects_an_unknown_provider_before_any_request() {
        let err = generator_test_key("nope".into(), "http://127.0.0.1:1".into(), "k".into())
            .await
            .unwrap_err();
        assert_eq!(err, "Unknown generator provider: nope");
    }

    #[tokio::test]
    async fn generator_test_key_requires_a_non_blank_api_key() {
        for key in ["", "   "] {
            let err = generator_test_key(
                "bloxgen".into(),
                "http://127.0.0.1:1".into(),
                key.into(),
            )
            .await
            .unwrap_err();
            assert_eq!(err, "An API key is required");
        }
    }
}

#[cfg(test)]
mod generator_http_tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn config_for(server: &MockServer) -> GeneratorConfig {
        GeneratorConfig {
            provider: GeneratorProvider::BloxGen,
            endpoint: server.uri(),
            api_key: "test-key".to_string(),
            account_type: "alt".to_string(),
            extra_delay_ms: 0,
            target_group: "BloxGen".to_string(),
            max_accounts: 0,
            max_consecutive_failures: 3,
        }
    }

    async fn mount_generate(server: &MockServer, response: ResponseTemplate) {
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(response)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn bloxgen_generate_returns_the_account_on_a_successful_response() {
        let server = MockServer::start().await;
        mount_generate(
            &server,
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "data": {
                    "username": "alt_42",
                    "password": "hunter2",
                    "cookie": "_|WARNING:-DO-NOT-SHARE|_TOKEN",
                    "id": 12345
                }
            })),
        )
        .await;

        match provider_generate(&generator_client(), &config_for(&server)).await {
            GenerateOutcome::Account(account) => {
                assert_eq!(account.username, "alt_42");
                assert_eq!(account.password, "hunter2");
                assert_eq!(account.cookie, "_|WARNING:-DO-NOT-SHARE|_TOKEN");
                assert_eq!(account.user_id, Some(12345));
            }
            other => panic!("expected an account, got {}", outcome_name(&other)),
        }
    }

    fn outcome_name(outcome: &GenerateOutcome) -> String {
        match outcome {
            GenerateOutcome::Account(_) => "Account".to_string(),
            GenerateOutcome::Cooldown(ms) => format!("Cooldown({ms})"),
            GenerateOutcome::Transient(m) => format!("Transient({m})"),
            GenerateOutcome::Fatal(m) => format!("Fatal({m})"),
        }
    }

    #[tokio::test]
    async fn bloxgen_generate_treats_a_missing_cookie_as_transient() {
        let server = MockServer::start().await;
        mount_generate(
            &server,
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "data": { "username": "alt_42", "password": "p", "cookie": "" }
            })),
        )
        .await;

        let outcome = bloxgen_generate(&generator_client(), &config_for(&server)).await;
        match outcome {
            GenerateOutcome::Transient(message) => {
                assert!(message.contains("cookie"), "unexpected message: {message}");
            }
            other => panic!("expected Transient, got {}", outcome_name(&other)),
        }
    }

    #[tokio::test]
    async fn bloxgen_generate_treats_success_without_data_as_transient() {
        let server = MockServer::start().await;
        mount_generate(
            &server,
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "success": true })),
        )
        .await;

        assert!(matches!(
            bloxgen_generate(&generator_client(), &config_for(&server)).await,
            GenerateOutcome::Transient(_)
        ));
    }

    #[tokio::test]
    async fn bloxgen_generate_surfaces_the_api_message_on_success_false() {
        let server = MockServer::start().await;
        mount_generate(
            &server,
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": false,
                "message": "No stock available"
            })),
        )
        .await;

        match bloxgen_generate(&generator_client(), &config_for(&server)).await {
            GenerateOutcome::Transient(message) => assert_eq!(message, "No stock available"),
            other => panic!("expected Transient, got {}", outcome_name(&other)),
        }
    }

    #[tokio::test]
    async fn bloxgen_generate_falls_back_to_a_body_snippet_for_non_json_200() {
        let server = MockServer::start().await;
        mount_generate(&server, ResponseTemplate::new(200).set_body_string("<html>nope</html>"))
            .await;

        match bloxgen_generate(&generator_client(), &config_for(&server)).await {
            GenerateOutcome::Transient(message) => assert_eq!(message, "<html>nope</html>"),
            other => panic!("expected Transient, got {}", outcome_name(&other)),
        }
    }

    #[tokio::test]
    async fn bloxgen_generate_maps_429_with_time_remaining_to_a_cooldown() {
        let server = MockServer::start().await;
        mount_generate(
            &server,
            ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "success": false,
                "message": "Rate limited",
                "timeRemaining": 42_000
            })),
        )
        .await;

        match bloxgen_generate(&generator_client(), &config_for(&server)).await {
            GenerateOutcome::Cooldown(ms) => assert_eq!(ms, 42_000),
            other => panic!("expected Cooldown, got {}", outcome_name(&other)),
        }
    }

    #[tokio::test]
    async fn bloxgen_generate_maps_429_without_a_usable_cooldown_to_transient() {
        let server = MockServer::start().await;
        mount_generate(
            &server,
            ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "success": false,
                "message": "Slow down",
                "timeRemaining": 0
            })),
        )
        .await;

        match bloxgen_generate(&generator_client(), &config_for(&server)).await {
            GenerateOutcome::Transient(message) => assert_eq!(message, "Slow down"),
            other => panic!("expected Transient, got {}", outcome_name(&other)),
        }
    }

    #[tokio::test]
    async fn bloxgen_generate_treats_server_errors_as_transient() {
        for status in [404_u16, 500, 502, 503, 504] {
            let server = MockServer::start().await;
            mount_generate(
                &server,
                ResponseTemplate::new(status)
                    .set_body_json(serde_json::json!({ "error": "boom" })),
            )
            .await;

            match bloxgen_generate(&generator_client(), &config_for(&server)).await {
                GenerateOutcome::Transient(message) => assert_eq!(message, "boom"),
                other => panic!("status {status}: expected Transient, got {}", outcome_name(&other)),
            }
        }
    }

    #[tokio::test]
    async fn bloxgen_generate_treats_a_rejected_key_as_fatal() {
        // A bad API key must stop the loop instead of retrying forever.
        let server = MockServer::start().await;
        mount_generate(
            &server,
            ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Invalid API key"
            })),
        )
        .await;

        match bloxgen_generate(&generator_client(), &config_for(&server)).await {
            GenerateOutcome::Fatal(message) => assert_eq!(message, "Invalid API key"),
            other => panic!("expected Fatal, got {}", outcome_name(&other)),
        }
    }

    #[tokio::test]
    async fn bloxgen_generate_reports_a_connection_failure_as_transient() {
        // Port 1 is reserved and refuses connections.
        let mut config = GeneratorConfig {
            provider: GeneratorProvider::BloxGen,
            endpoint: "http://127.0.0.1:1".to_string(),
            api_key: "k".to_string(),
            account_type: "alt".to_string(),
            extra_delay_ms: 0,
            target_group: String::new(),
            max_accounts: 0,
            max_consecutive_failures: 3,
        };
        config.account_type = "alt".to_string();

        match bloxgen_generate(&generator_client(), &config).await {
            GenerateOutcome::Transient(message) => {
                assert!(message.starts_with("Request failed"), "{message}");
            }
            other => panic!("expected Transient, got {}", outcome_name(&other)),
        }
    }

    #[tokio::test]
    async fn bloxgen_generate_posts_the_api_key_and_account_type() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "apiKey": "test-key",
                "type": "alt"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "data": { "username": "u", "password": "p", "cookie": "c", "id": 1 }
            })))
            .mount(&server)
            .await;

        assert!(matches!(
            bloxgen_generate(&generator_client(), &config_for(&server)).await,
            GenerateOutcome::Account(_)
        ));
    }

    // ---- balance / key test ---------------------------------------------------

    #[tokio::test]
    async fn bloxgen_test_key_returns_the_balance_and_sends_the_key_header() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/balance"))
            .and(header("x-api-key", "test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "data": { "balance": 12.5 }
            })))
            .mount(&server)
            .await;

        assert_eq!(bloxgen_test_key(&server.uri(), "test-key").await.unwrap(), 12.5);
    }

    #[tokio::test]
    async fn bloxgen_test_key_surfaces_the_api_message_on_success_false() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/balance"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": false,
                "error": "Unknown key"
            })))
            .mount(&server)
            .await;

        assert_eq!(
            bloxgen_test_key(&server.uri(), "bad").await.unwrap_err(),
            "Unknown key"
        );
    }

    #[tokio::test]
    async fn bloxgen_test_key_reports_the_status_when_the_body_is_not_json() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/balance"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
            .mount(&server)
            .await;

        assert_eq!(
            bloxgen_test_key(&server.uri(), "bad").await.unwrap_err(),
            "Balance check failed (status 403)"
        );
    }

    #[tokio::test]
    async fn bloxgen_test_key_returns_a_body_snippet_for_a_non_json_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/balance"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html>hi</html>"))
            .mount(&server)
            .await;

        assert_eq!(
            bloxgen_test_key(&server.uri(), "k").await.unwrap_err(),
            "<html>hi</html>"
        );
    }

    #[tokio::test]
    async fn bloxgen_test_key_accepts_an_endpoint_pasted_with_the_generate_path() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/balance"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "data": { "balance": 1.0 }
            })))
            .mount(&server)
            .await;

        let pasted = format!("{}/api/generate", server.uri());
        assert_eq!(bloxgen_test_key(&pasted, "k").await.unwrap(), 1.0);
    }

    #[tokio::test]
    async fn generator_test_key_command_reaches_the_provider() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/balance"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "data": { "balance": 7.25 }
            })))
            .mount(&server)
            .await;

        let balance = generator_test_key("bloxgen".into(), server.uri(), "  k  ".into())
            .await
            .unwrap();
        assert_eq!(balance, 7.25);
    }
}
