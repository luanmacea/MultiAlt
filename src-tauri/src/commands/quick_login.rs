// Adicionar conta por Quick Login (ideia 12): o app mostra um código, a pessoa
// aprova num aparelho já logado e a conta entra sem colar cookie nem digitar
// senha aqui. O fluxo HTTP mora em `api::auth` (`quick_login_create`,
// `quick_login_status`, `quick_login_redeem`); aqui ficam o estado do login em
// andamento (a chave privada nunca vai para o frontend) e o "adicionar" pelo
// mesmo caminho do Quick Add: `validate_cookie` → `AccountStore::add`.

/// O Quick Login em andamento. Um por vez: começar outro descarta este.
static PENDING_QUICK_LOGIN: std::sync::Mutex<Option<api::auth::QuickLoginSession>> =
    std::sync::Mutex::new(None);

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct QuickLoginStarted {
    code: String,
    expires_at: Option<String>,
}

/// O que a tela mostra a cada consulta.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum QuickLoginPoll {
    Pending,
    #[serde(rename_all = "camelCase")]
    Linked { account_name: Option<String> },
    Cancelled,
    Expired,
    #[serde(rename_all = "camelCase")]
    Added { user_id: i64, username: String, already_saved: bool },
}

/// O que fazer com o estado que o Roblox devolveu.
#[derive(Debug, PartialEq, Eq)]
enum QuickLoginStep {
    /// Mostrar e continuar esperando.
    Report(QuickLoginPoll),
    /// Acabou sem conta: mostrar e esquecer o login.
    Finish(QuickLoginPoll),
    /// Confirmado: trocar pela sessão e adicionar a conta.
    Redeem,
}

fn quick_login_step(status: api::auth::QuickLoginStatus) -> QuickLoginStep {
    use api::auth::QuickLoginStatus as S;
    match status {
        S::Pending => QuickLoginStep::Report(QuickLoginPoll::Pending),
        S::Linked { account_name } => QuickLoginStep::Report(QuickLoginPoll::Linked { account_name }),
        S::Cancelled => QuickLoginStep::Finish(QuickLoginPoll::Cancelled),
        S::Expired => QuickLoginStep::Finish(QuickLoginPoll::Expired),
        S::Validated => QuickLoginStep::Redeem,
    }
}

fn pending_quick_login() -> Result<api::auth::QuickLoginSession, String> {
    PENDING_QUICK_LOGIN
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "No Quick Login in progress. Get a new code.".to_string())
}

/// Esquece o login só se ainda for o mesmo (outro pode ter começado no meio).
fn forget_quick_login(code: &str) {
    if let Ok(mut slot) = PENDING_QUICK_LOGIN.lock() {
        if slot.as_ref().is_some_and(|s| s.code == code) {
            *slot = None;
        }
    }
}

#[tauri::command]
async fn add_by_quick_login_start() -> Result<QuickLoginStarted, String> {
    let session = api::auth::quick_login_create().await?;
    let started = QuickLoginStarted {
        code: session.code.clone(),
        expires_at: session.expires_at.clone(),
    };
    *PENDING_QUICK_LOGIN.lock().map_err(|e| e.to_string())? = Some(session);
    Ok(started)
}

#[tauri::command]
async fn add_by_quick_login_poll(
    state: tauri::State<'_, AccountStore>,
) -> Result<QuickLoginPoll, String> {
    let session = pending_quick_login()?;
    let status = api::auth::quick_login_status(&session).await?;
    match quick_login_step(status) {
        QuickLoginStep::Report(poll) => Ok(poll),
        QuickLoginStep::Finish(poll) => {
            forget_quick_login(&session.code);
            Ok(poll)
        }
        QuickLoginStep::Redeem => {
            // Código de uso único: dando certo ou não, este login acabou.
            forget_quick_login(&session.code);
            let cookie = api::auth::quick_login_redeem(&session).await?;
            let info = api::auth::validate_cookie(&cookie).await?;
            let already_saved = state
                .get_all()?
                .iter()
                .any(|account| account.user_id == info.user_id);
            state.add(data::accounts::Account::new(cookie, info.name.clone(), info.user_id))?;
            Ok(QuickLoginPoll::Added {
                user_id: info.user_id,
                username: info.name,
                already_saved,
            })
        }
    }
}

#[tauri::command]
fn add_by_quick_login_cancel() -> Result<(), String> {
    *PENDING_QUICK_LOGIN.lock().map_err(|e| e.to_string())? = None;
    Ok(())
}
