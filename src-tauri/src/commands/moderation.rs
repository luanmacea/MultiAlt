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

/// A nota do moderador (texto do Roblox), no tamanho que cabe numa linha.
fn moderator_note_of(status: &ModerationStatus) -> Option<String> {
    status
        .note
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(|note| note.chars().take(200).collect())
}

/// A frase do launch pulado, ou `None` quando a conta pode abrir. Advertência
/// não bloqueia; ban que já acabou também não (o Roblox pode pedir para
/// reativar a conta no site, e aí o próprio launch diz). Esta é a frase do
/// Console, traduzível; `moderation_block_message` é a mesma em inglês montada.
fn moderation_block_log(
    status: &ModerationStatus,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<LogMsg> {
    if !ban_is_active(status, now) {
        return None;
    }
    let note = moderator_note_of(status);
    let terminated = matches!(status.state, ModerationState::Terminated);
    let until = status
        .until
        .as_deref()
        .and_then(|d| chrono::DateTime::parse_from_rfc3339(d).ok())
        .map(|end| end.with_timezone(&chrono::Local).format("%d/%m/%Y %H:%M").to_string());
    Some(match (terminated, until, note) {
        (true, _, None) => log_msg!("Skipped: Roblox terminated this account."),
        (true, _, Some(note)) => log_msg!(
            "Skipped: Roblox terminated this account. Moderator note: \"{{note}}\"",
            note = note
        ),
        (false, Some(date), None) => log_msg!("Skipped: this account is banned until {{date}}.", date = date),
        (false, Some(date), Some(note)) => log_msg!(
            "Skipped: this account is banned until {{date}}. Moderator note: \"{{note}}\"",
            date = date,
            note = note
        ),
        (false, None, None) => log_msg!("Skipped: this account is banned."),
        (false, None, Some(note)) => log_msg!(
            "Skipped: this account is banned. Moderator note: \"{{note}}\"",
            note = note
        ),
    })
}

fn moderation_block_message(
    status: &ModerationStatus,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<String> {
    moderation_block_log(status, now).map(|msg| msg.render())
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
) -> Option<LogMsg> {
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
    let message = moderation_block_log(&status, chrono::Utc::now())?;
    // O cache pode ter vindo de antes; garante o grupo também nesse caso.
    mark_account_moderated(state, app, user_id);
    Some(message)
}
