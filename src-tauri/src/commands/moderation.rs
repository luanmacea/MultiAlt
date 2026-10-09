// Moderação da conta (banida / advertida / encerrada): consulta sob demanda,
// "conferir contas" e checagem logo antes do launch. Ver
// `docs/features/accounts.md` ("Moderação antes do launch").
//
// Regras:
// - **Leitura sem refresh** (`read_without_refresh`): consultar nunca desloga.
// - Resultado guardado por conta durante `MODERATION_TTL`, só em memória.
// - 429 / rede = "tente depois": nunca marca a conta como banida.
// - Banida (ban ainda valendo) ou encerrada → a conta vai para o grupo
//   `moderadas` (o mesmo de `mark_account_moderated`) e o launch dela é pulado.

use api::roblox::{ModerationFailure, ModerationState, ModerationStatus};

/// Quanto tempo um resultado vale. Curto: um ban de 1 dia acaba, e a pessoa
/// pode ter reativado a conta no site.
const MODERATION_TTL: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// Teto da consulta feita **antes do launch**: ela está no caminho de abrir o
/// jogo, e um endpoint lento não pode segurar a fila (sem resposta = não pula).
const MODERATION_PRELAUNCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

static MODERATION_CACHE: LazyLock<Mutex<HashMap<i64, (std::time::Instant, ModerationStatus)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn cached_moderation(user_id: i64, now: std::time::Instant) -> Option<ModerationStatus> {
    let cache = MODERATION_CACHE.lock().ok()?;
    let (at, status) = cache.get(&user_id)?;
    (now.saturating_duration_since(*at) < MODERATION_TTL).then(|| status.clone())
}

fn remember_moderation(user_id: i64, status: &ModerationStatus, now: std::time::Instant) {
    if let Ok(mut cache) = MODERATION_CACHE.lock() {
        cache.insert(user_id, (now, status.clone()));
    }
}

fn forget_moderation(user_id: i64) {
    if let Ok(mut cache) = MODERATION_CACHE.lock() {
        cache.remove(&user_id);
    }
}

/// O ban ainda vale? Sem data de fim, vale.
fn ban_is_active(status: &ModerationStatus, now: chrono::DateTime<chrono::Utc>) -> bool {
    match status.state {
        ModerationState::Terminated => true,
        ModerationState::Banned => status
            .until
            .as_deref()
            .and_then(|d| chrono::DateTime::parse_from_rfc3339(d).ok())
            .map(|end| end.with_timezone(&chrono::Utc) > now)
            .unwrap_or(true),
        ModerationState::Warned | ModerationState::Clean => false,
    }
}

fn moderator_note_suffix(status: &ModerationStatus) -> String {
    match status.note.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        Some(note) => {
            let short: String = note.chars().take(200).collect();
            format!(" Moderator note: \"{short}\"")
        }
        None => String::new(),
    }
}

/// A frase do launch pulado, ou `None` quando a conta pode abrir. Advertência
/// não bloqueia; ban que já acabou também não (o Roblox pode pedir para
/// reativar a conta no site, e aí o próprio launch diz).
fn moderation_block_message(
    status: &ModerationStatus,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<String> {
    if !ban_is_active(status, now) {
        return None;
    }
    let note = moderator_note_suffix(status);
    Some(match status.state {
        ModerationState::Terminated => {
            format!("Skipped: Roblox terminated this account.{note}")
        }
        _ => match status
            .until
            .as_deref()
            .and_then(|d| chrono::DateTime::parse_from_rfc3339(d).ok())
        {
            Some(end) => format!(
                "Skipped: this account is banned until {}.{note}",
                end.with_timezone(&chrono::Local).format("%d/%m/%Y %H:%M")
            ),
            None => format!("Skipped: this account is banned.{note}"),
        },
    })
}

/// Lê a moderação sem refresh, guarda no cache e avisa a tela (`account-moderation`).
async fn fetch_moderation(
    state: &AccountStore,
    app: Option<&tauri::AppHandle>,
    user_id: i64,
    client: reqwest::Client,
) -> Result<ModerationStatus, ModerationFailure> {
    let result = read_without_refresh(state, user_id, |cookie| async move {
        Ok(api::roblox::get_moderation_status(&cookie, &client).await)
    })
    .await
    .map_err(ModerationFailure::Other)?;
    if let Ok(status) = &result {
        remember_moderation(user_id, status, std::time::Instant::now());
        if let Some(app) = app {
            let _ = app.emit(
                "account-moderation",
                serde_json::json!({ "userId": user_id, "status": status }),
            );
            if ban_is_active(status, chrono::Utc::now()) {
                mark_account_moderated(state, app, user_id);
            }
        }
    }
    result
}

/// Texto do erro para a tela. 429 e rede dizem "tente depois"; nunca "banida".
fn moderation_failure_message(failure: &ModerationFailure) -> String {
    match failure {
        ModerationFailure::RateLimited => {
            "Roblox is limiting requests right now. Try again in a minute.".to_string()
        }
        ModerationFailure::Unauthorized => {
            "This account's session is no longer valid. Sign in again to check it.".to_string()
        }
        ModerationFailure::Other(e) => format!("Couldn't check right now: {e}"),
    }
}

/// Consulta sob demanda (painel da conta). `force` ignora o cache.
#[tauri::command]
async fn check_account_moderation(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    force: Option<bool>,
) -> Result<ModerationStatus, String> {
    if !force.unwrap_or(false) {
        if let Some(status) = cached_moderation(user_id, std::time::Instant::now()) {
            return Ok(status);
        }
    } else {
        forget_moderation(user_id);
    }
    fetch_moderation(state.inner(), Some(&app), user_id, api::http_client::client())
        .await
        .map_err(|f| moderation_failure_message(&f))
}

/// Logo antes do launch: `Some(frase)` quando a conta deve ser pulada. Qualquer
/// falha da consulta (429, rede, cookie) devolve `None` — na dúvida, abre, e o
/// launch segue o caminho de sempre. Desligável em
/// `General.CheckModerationBeforeLaunch`.
async fn moderation_launch_block(
    state: &AccountStore,
    app: &tauri::AppHandle,
    settings: &SettingsStore,
    user_id: i64,
) -> Option<String> {
    if !settings.get_bool("General", "CheckModerationBeforeLaunch") {
        return None;
    }
    let status = match cached_moderation(user_id, std::time::Instant::now()) {
        Some(status) => status,
        None => {
            let client = api::http_client::builder_with(
                api::http_client::CONNECT_TIMEOUT,
                MODERATION_PRELAUNCH_TIMEOUT,
            )
            .build()
            .ok()?;
            fetch_moderation(state, Some(app), user_id, client).await.ok()?
        }
    };
    let message = moderation_block_message(&status, chrono::Utc::now())?;
    // O cache pode ter vindo de antes; garante o grupo também nesse caso.
    mark_account_moderated(state, app, user_id);
    Some(message)
}

#[cfg(test)]
mod moderation_command_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    fn status(state: ModerationState, until: Option<&str>, note: Option<&str>) -> ModerationStatus {
        ModerationStatus {
            state,
            until: until.map(str::to_string),
            note: note.map(str::to_string),
            punishment: None,
        }
    }

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-10-09T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    #[test]
    fn an_active_ban_blocks_the_launch_with_its_end_date() {
        let msg = moderation_block_message(
            &status(ModerationState::Banned, Some("2026-10-12T12:00:00Z"), Some("Spam")),
            now(),
        )
        .expect("blocked");
        assert!(msg.starts_with("Skipped: this account is banned until 12/10/2026"), "{msg}");
        assert!(msg.contains("Moderator note: \"Spam\""), "{msg}");
    }

    #[test]
    fn a_ban_without_an_end_date_blocks() {
        let msg = moderation_block_message(&status(ModerationState::Banned, None, None), now()).unwrap();
        assert_eq!(msg, "Skipped: this account is banned.");
    }

    #[test]
    fn a_terminated_account_blocks() {
        let msg =
            moderation_block_message(&status(ModerationState::Terminated, None, None), now()).unwrap();
        assert_eq!(msg, "Skipped: Roblox terminated this account.");
    }

    #[test]
    fn warnings_finished_bans_and_clean_accounts_launch() {
        for s in [
            status(ModerationState::Warned, None, Some("Be nice")),
            status(ModerationState::Clean, None, None),
            status(ModerationState::Banned, Some("2026-10-08T12:00:00Z"), None),
        ] {
            assert_eq!(moderation_block_message(&s, now()), None, "{s:?}");
        }
    }

    /// A frase do launch pulado não pode disparar os classificadores do launch:
    /// sessão (renovaria e deslogaria a conta) — e a nota do moderador fica curta.
    #[test]
    fn the_block_message_is_not_a_session_error_and_the_note_is_capped() {
        let long = "x".repeat(500);
        let msg = moderation_block_message(
            &status(ModerationState::Terminated, None, Some(&long)),
            now(),
        )
        .unwrap();
        assert!(!is_auth_session_error(&msg), "{msg}");
        assert!(msg.len() < 300, "{}", msg.len());
    }

    #[test]
    fn the_cache_expires_after_the_ttl() {
        let t0 = std::time::Instant::now();
        let s = status(ModerationState::Warned, None, None);
        remember_moderation(-7001, &s, t0);
        assert_eq!(cached_moderation(-7001, t0), Some(s));
        assert_eq!(cached_moderation(-7001, t0 + MODERATION_TTL), None);
        forget_moderation(-7001);
        assert_eq!(cached_moderation(-7001, t0), None);
    }

    #[test]
    fn rate_limits_say_try_later_and_never_banned() {
        let msg = moderation_failure_message(&ModerationFailure::RateLimited);
        assert!(msg.contains("Try again"), "{msg}");
        assert!(!msg.to_lowercase().contains("banned"), "{msg}");
    }

    fn temp_store(tag: &str) -> AccountStore {
        crypto::init();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        AccountStore::new(std::env::temp_dir().join(format!("ram-moderation-{tag}-{nanos}.json")))
    }

    /// A consulta vai pelo caminho sem refresh: um 401 vira "sessão inválida"
    /// e **nenhum** sign-out é chamado.
    #[tokio::test]
    async fn the_check_never_signs_the_account_out() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("usermoderation", "/v1/not-approved")))
            .and(header("cookie", cookie_of("mod-cmd-401")))
            .respond_with(ResponseTemplate::new(401))
            .mount(server)
            .await;
        let store = temp_store("401");
        store
            .add(crate::data::accounts::Account::new("mod-cmd-401".into(), "u".into(), 7201))
            .unwrap();

        let err = fetch_moderation(&store, None, 7201, api::http_client::client())
            .await
            .unwrap_err();
        assert_eq!(err, ModerationFailure::Unauthorized);
        let signouts = server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|r| r.url.path().contains("signoutfromallsessions") && r.headers.get("cookie").map(|c| c.as_bytes() == cookie_of("mod-cmd-401").as_bytes()).unwrap_or(false))
            .count();
        assert_eq!(signouts, 0);
        assert_eq!(store.get_all().unwrap()[0].security_token, "mod-cmd-401");
    }

    #[tokio::test]
    async fn a_fresh_status_is_cached() {
        Mock::given(method("GET"))
            .and(path(mock_path("usermoderation", "/v1/not-approved")))
            .and(header("cookie", cookie_of("mod-cmd-ban")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "punishmentTypeDescription": "Ban 1 Day",
                "endDate": "2099-01-01T00:00:00Z"
            })))
            .mount(mock_server().await)
            .await;
        let store = temp_store("ban");
        store
            .add(crate::data::accounts::Account::new("mod-cmd-ban".into(), "u".into(), 7202))
            .unwrap();

        let fetched = fetch_moderation(&store, None, 7202, api::http_client::client())
            .await
            .unwrap();
        assert_eq!(fetched.state, ModerationState::Banned);
        assert_eq!(cached_moderation(7202, std::time::Instant::now()), Some(fetched));
    }
}
