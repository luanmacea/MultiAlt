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
            return AccountCheckOutcome::Invalid;
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
            return AccountCheckOutcome::Invalid;
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

#[cfg(test)]
mod account_check_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    fn temp_store(tag: &str) -> AccountStore {
        crypto::init();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        AccountStore::new(std::env::temp_dir().join(format!("ram-check-{tag}-{nanos}.json")))
    }

    async fn mount_moderation(token: &str, response: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path(mock_path("usermoderation", "/v1/not-approved")))
            .and(header("cookie", cookie_of(token)))
            .respond_with(response)
            .mount(mock_server().await)
            .await;
    }

    async fn mount_session(token: &str, response: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path(mock_path("users", "/v1/users/authenticated")))
            .and(header("cookie", cookie_of(token)))
            .respond_with(response)
            .mount(mock_server().await)
            .await;
    }

    fn clean() -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({}))
    }

    fn me(id: i64) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({ "id": id, "name": "x", "displayName": "x" }))
    }

    fn valid_of(store: &AccountStore, user_id: i64) -> bool {
        store.get_all().unwrap().into_iter().find(|a| a.user_id == user_id).unwrap().valid
    }

    #[tokio::test]
    async fn a_mixed_batch_is_summed_up_and_401_marks_invalid() {
        // ok
        mount_moderation("chk-ok", clean()).await;
        mount_session("chk-ok", me(8101)).await;
        // cookie morto
        mount_moderation("chk-dead", ResponseTemplate::new(401)).await;
        mount_session("chk-dead", ResponseTemplate::new(401)).await;
        // banida
        mount_moderation(
            "chk-ban",
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "punishmentTypeDescription": "Ban 7 Days",
                "endDate": "2099-01-01T00:00:00Z"
            })),
        )
        .await;
        // Roblox limitando
        mount_moderation("chk-429", clean()).await;
        mount_session("chk-429", ResponseTemplate::new(429)).await;
        // advertida
        mount_moderation(
            "chk-warn",
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "punishmentTypeDescription": "Warn" })),
        )
        .await;
        mount_session("chk-warn", me(8105)).await;

        let store = temp_store("mixed");
        for (id, token) in [
            (8101, "chk-ok"),
            (8102, "chk-dead"),
            (8103, "chk-ban"),
            (8104, "chk-429"),
            (8105, "chk-warn"),
        ] {
            store.add(crate::data::accounts::Account::new(token.into(), format!("u{id}"), id)).unwrap();
        }
        store.add(crate::data::accounts::Account::new(String::new(), "nosession".into(), 8106)).unwrap();

        let mut progress = Vec::new();
        let summary = run_account_checks(
            &store,
            None,
            vec![8101, 8102, 8103, 8104, 8105, 8106],
            api::http_client::client(),
            std::time::Duration::ZERO,
            std::time::Duration::ZERO,
            |done, total| progress.push((done, total)),
        )
        .await;

        assert_eq!(summary.total, 6);
        assert_eq!(summary.ok, 1);
        assert_eq!(summary.warned, 1);
        assert_eq!(summary.invalid, 2, "dead cookie + no cookie");
        assert_eq!(summary.banned, 1);
        assert_eq!(summary.unknown, 1, "429 is try-later");
        assert_eq!(progress.first(), Some(&(0, 6)));
        assert_eq!(progress.last(), Some(&(6, 6)));

        assert!(valid_of(&store, 8101));
        assert!(!valid_of(&store, 8102), "401 marks the account invalid");
        assert!(valid_of(&store, 8104), "a 429 never marks the account invalid");
        assert!(valid_of(&store, 8103), "a ban is not a dead cookie");
    }

    /// Conta marcada inválida que volta a responder 200 perde o ponto vermelho.
    #[tokio::test]
    async fn a_live_cookie_clears_the_invalid_mark() {
        mount_moderation("chk-revived", clean()).await;
        mount_session("chk-revived", me(8201)).await;
        let store = temp_store("revived");
        let mut account = crate::data::accounts::Account::new("chk-revived".into(), "u".into(), 8201);
        account.valid = false;
        store.add(account).unwrap();

        let summary = run_account_checks(
            &store,
            None,
            vec![8201],
            api::http_client::client(),
            std::time::Duration::ZERO,
            std::time::Duration::ZERO,
            |_, _| {},
        )
        .await;
        assert_eq!(summary.ok, 1);
        assert!(valid_of(&store, 8201));
    }

    /// Nunca chama o sign-out (`signoutfromallsessionsandreauthenticate`): é leitura.
    #[tokio::test]
    async fn the_check_never_refreshes_the_session() {
        mount_moderation("chk-norefresh", ResponseTemplate::new(401)).await;
        mount_session("chk-norefresh", ResponseTemplate::new(401)).await;
        let store = temp_store("norefresh");
        store
            .add(crate::data::accounts::Account::new("chk-norefresh".into(), "u".into(), 8301))
            .unwrap();
        run_account_checks(
            &store,
            None,
            vec![8301],
            api::http_client::client(),
            std::time::Duration::ZERO,
            std::time::Duration::ZERO,
            |_, _| {},
        )
        .await;
        let signouts = mock_server()
            .await
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|r| {
                r.url.path().contains("signoutfromallsessions")
                    && r.headers
                        .get("cookie")
                        .map(|c| c.as_bytes() == cookie_of("chk-norefresh").as_bytes())
                        .unwrap_or(false)
            })
            .count();
        assert_eq!(signouts, 0);
        assert_eq!(store.get_all().unwrap()[0].security_token, "chk-norefresh");
    }

    #[test]
    fn only_one_check_runs_at_a_time() {
        let first = try_begin_account_check().expect("first run");
        assert!(try_begin_account_check().is_err());
        drop(first);
        assert!(try_begin_account_check().is_ok());
    }
}
