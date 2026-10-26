// "Check accounts": confere de uma vez se o cookie de cada conta ainda abre
// sessão e se ela está banida/advertida (ideia 9). Ver
// `docs/features/accounts.md` ("Conferir contas").
//
// Regras:
// - **Só leitura e sem refresh** (`read_without_refresh`): conferir nunca
//   desloga a conta nem derruba cliente aberto.
// - 401 em `users/v1/users/authenticated` = cookie morto → `Valid = false` (o
//   ponto vermelho de sempre). 200 devolve `Valid = true`.
// - 429, rede e 5xx = "não deu para conferir": não muda nada na conta.
// - Ritmo limitado: no máximo `CHECK_CONCURRENCY` contas ao mesmo tempo, um
//   respiro entre elas e um freio maior depois de um 429.

use api::roblox::SessionCheck;

const CHECK_CONCURRENCY: usize = 2;
/// Respiro antes de cada conta depois das primeiras.
const CHECK_PACE: std::time::Duration = std::time::Duration::from_millis(600);
/// Freio depois que o Roblox limitou (429) ou a rede falhou.
const CHECK_BACKOFF: std::time::Duration = std::time::Duration::from_secs(3);

static ACCOUNT_CHECK_RUNNING: AtomicBool = AtomicBool::new(false);

struct AccountCheckGuard;

impl Drop for AccountCheckGuard {
    fn drop(&mut self) {
        ACCOUNT_CHECK_RUNNING.store(false, Ordering::SeqCst);
    }
}

fn try_begin_account_check() -> Result<AccountCheckGuard, String> {
    if ACCOUNT_CHECK_RUNNING.swap(true, Ordering::SeqCst) {
        return Err("A check is already running. Wait for it to finish.".to_string());
    }
    Ok(AccountCheckGuard)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
enum AccountCheckOutcome {
    Ok,
    Warned,
    Invalid,
    Banned,
    /// Não deu para conferir agora (429, rede): tentar depois.
    Unknown,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountCheckResult {
    user_id: i64,
    outcome: AccountCheckOutcome,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountCheckSummary {
    total: usize,
    ok: usize,
    warned: usize,
    invalid: usize,
    banned: usize,
    unknown: usize,
    results: Vec<AccountCheckResult>,
}

impl AccountCheckSummary {
    fn add(&mut self, user_id: i64, outcome: AccountCheckOutcome) {
        self.total += 1;
        match outcome {
            AccountCheckOutcome::Ok => self.ok += 1,
            AccountCheckOutcome::Warned => self.warned += 1,
            AccountCheckOutcome::Invalid => self.invalid += 1,
            AccountCheckOutcome::Banned => self.banned += 1,
            AccountCheckOutcome::Unknown => self.unknown += 1,
        }
        self.results.push(AccountCheckResult { user_id, outcome });
    }
}

/// Sessão morta: sem ela a moderação não responde, e a conta banida de vez
/// aparecia só como "inválida" (relato do dono, 26/10/2026). A página pública
/// do usuário (sem cookie) diz `isBanned`; qualquer falha nela deixa "inválida".
async fn invalid_or_terminated(user_id: i64) -> AccountCheckOutcome {
    match api::roblox::get_user_info(None, user_id).await {
        Ok(info) if info.is_banned => AccountCheckOutcome::Banned,
        _ => AccountCheckOutcome::Invalid,
    }
}

/// Uma conta: moderação primeiro (conta banida pode ter a sessão recusada em
/// `users/authenticated` sem que o cookie esteja morto), depois a sessão.
async fn check_one_account(
    state: &AccountStore,
    app: Option<&tauri::AppHandle>,
    user_id: i64,
    client: &reqwest::Client,
) -> AccountCheckOutcome {
    match get_cookie(state, user_id) {
        Ok(token) if token.trim().is_empty() => {
            // Sem cookie não há sessão: o mesmo ponto vermelho da linha.
            let _ = state.set_valid(user_id, false);
            return invalid_or_terminated(user_id).await;
        }
        Ok(_) => {}
        Err(_) => return AccountCheckOutcome::Unknown,
    }

    let moderation = fetch_moderation(state, app, user_id, client.clone()).await;
    if let Ok(status) = &moderation {
        if ban_is_active(status, chrono::Utc::now()) {
            return AccountCheckOutcome::Banned;
        }
    }

    let session_client = client.clone();
    let session = read_without_refresh(state, user_id, |cookie| async move {
        Ok(api::roblox::check_session(&cookie, &session_client).await)
    })
    .await;
    match session {
        Ok(SessionCheck::Valid(_)) => {
            let _ = state.set_valid(user_id, true);
        }
        Ok(SessionCheck::Invalid) => {
            let _ = state.set_valid(user_id, false);
            return invalid_or_terminated(user_id).await;
        }
        _ => return AccountCheckOutcome::Unknown,
    }

    match moderation {
        Ok(status) if status.state == ModerationState::Clean => AccountCheckOutcome::Ok,
        Ok(_) => AccountCheckOutcome::Warned,
        // Sessão viva, moderação sem resposta: não dá para dizer "ok".
        Err(_) => AccountCheckOutcome::Unknown,
    }
}

/// Confere `user_ids` com ritmo limitado, chamando `on_progress(feitas, total)`
/// a cada conta.
async fn run_account_checks(
    state: &AccountStore,
    app: Option<&tauri::AppHandle>,
    user_ids: Vec<i64>,
    client: reqwest::Client,
    pace: std::time::Duration,
    backoff: std::time::Duration,
    mut on_progress: impl FnMut(usize, usize),
) -> AccountCheckSummary {
    use futures_util::StreamExt;

    let total = user_ids.len();
    let slow_down = std::sync::Arc::new(AtomicBool::new(false));
    let mut summary = AccountCheckSummary::default();
    on_progress(0, total);

    let mut checks = futures_util::stream::iter(user_ids.into_iter().enumerate())
        .map(|(index, user_id)| {
            let client = client.clone();
            let slow_down = slow_down.clone();
            async move {
                if index >= CHECK_CONCURRENCY {
                    tokio::time::sleep(pace).await;
                }
                if slow_down.load(Ordering::SeqCst) {
                    tokio::time::sleep(backoff).await;
                }
                let outcome = check_one_account(state, app, user_id, &client).await;
                if outcome == AccountCheckOutcome::Unknown {
                    slow_down.store(true, Ordering::SeqCst);
                }
                (user_id, outcome)
            }
        })
        .buffer_unordered(CHECK_CONCURRENCY);

    while let Some((user_id, outcome)) = checks.next().await {
        summary.add(user_id, outcome);
        on_progress(summary.total, total);
    }
    summary
}

#[tauri::command]
async fn check_accounts(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    user_ids: Vec<i64>,
) -> Result<AccountCheckSummary, String> {
    let _guard = try_begin_account_check()?;
    let mut ids = Vec::new();
    for id in user_ids {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    let progress_app = app.clone();
    let summary = run_account_checks(
        state.inner(),
        Some(&app),
        ids,
        api::http_client::client(),
        CHECK_PACE,
        CHECK_BACKOFF,
        move |done, total| {
            let _ = progress_app.emit(
                "account-check-progress",
                serde_json::json!({ "done": done, "total": total }),
            );
        },
    )
    .await;
    Ok(summary)
}
