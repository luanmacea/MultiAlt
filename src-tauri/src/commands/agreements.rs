// "Acordos atualizados" (Termos de Uso / Política de Privacidade): confere quais
// contas têm acordo pendente e, por pedido, aceita em nome da conta. Ver
// `docs/features/accounts.md` ("Termos pendentes").
//
// Regras:
// - **Conferir é leitura sem refresh** (`read_without_refresh`): nunca desloga.
// - Em lote: no máximo `TERMS_CONCURRENCY` contas ao mesmo tempo, com respiro
//   entre elas. Nada aqui segura launch, fila ou Auto Rejoin (regra nº 1).
// - **Aceitar só roda por pedido** (`accept_account_terms`, depois da
//   confirmação da tela). Devolve a resposta exata do Roblox; se a API recusar,
//   a tela oferece abrir o navegador da conta para a pessoa clicar "Concordo".
// - Resposta que não reconhecemos vai inteira para o Console (aviso); nunca
//   vira "nada pendente".

use api::roblox::{AcceptAttempt, PendingAgreement};

const TERMS_CONCURRENCY: usize = 2;
/// Respiro antes de cada conta depois das primeiras.
const TERMS_PACE: std::time::Duration = std::time::Duration::from_millis(500);

static TERMS_CHECK_RUNNING: AtomicBool = AtomicBool::new(false);

struct TermsCheckGuard;

impl Drop for TermsCheckGuard {
    fn drop(&mut self) {
        TERMS_CHECK_RUNNING.store(false, Ordering::SeqCst);
    }
}

fn try_begin_terms_check() -> Result<TermsCheckGuard, String> {
    if TERMS_CHECK_RUNNING.swap(true, Ordering::SeqCst) {
        return Err("A terms check is already running. Wait for it to finish.".to_string());
    }
    Ok(TermsCheckGuard)
}

/// O que a conta ainda precisa aceitar (lista vazia = nada pendente).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TermsStatus {
    pending: Vec<PendingAgreement>,
}

/// Uma conta do lote: o estado, ou o motivo de não ter dado para conferir.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TermsCheckEntry {
    user_id: i64,
    status: Option<TermsStatus>,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TermsAcceptResult {
    /// Aceite confirmado: o Roblox respondeu 2xx **e** a conferência seguinte
    /// não achou mais nada pendente.
    accepted: bool,
    /// O que sobrou pendente na conferência depois do aceite; `None` quando
    /// essa conferência falhou (não dá para afirmar nada).
    remaining: Option<Vec<PendingAgreement>>,
    /// A resposta exata do Roblox a cada tentativa de aceite.
    attempts: Vec<AcceptAttempt>,
}

/// Lê os acordos pendentes sem refresh; o JSON que não reconhecemos vai ao
/// Console como aviso.
async fn fetch_terms(
    state: &AccountStore,
    app: Option<&tauri::AppHandle>,
    user_id: i64,
    client: reqwest::Client,
) -> Result<TermsStatus, String> {
    let check = read_without_refresh(state, user_id, |cookie| async move {
        api::roblox::get_pending_agreements(&cookie, &client).await
    })
    .await?;
    if let (Some(app), Some(raw)) = (app, &check.unrecognized) {
        emit_launch_log(
            app,
            user_id,
            "warn",
            "terms",
            log_msg!(
                "Terms check: Roblox answered in a shape this version doesn't fully recognize: {{raw}}",
                raw = raw.clone()
            ),
        );
    }
    Ok(TermsStatus { pending: check.pending })
}

fn terms_entry(user_id: i64, result: Result<TermsStatus, String>) -> TermsCheckEntry {
    match result {
        Ok(status) => TermsCheckEntry { user_id, status: Some(status), error: None },
        Err(error) => TermsCheckEntry { user_id, status: None, error: Some(error) },
    }
}

#[tauri::command]
async fn check_account_terms(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
) -> Result<TermsStatus, String> {
    fetch_terms(state.inner(), Some(&app), user_id, api::http_client::client()).await
}

/// Confere `user_ids` com ritmo limitado, chamando `on_entry` a cada conta.
async fn run_terms_checks(
    state: &AccountStore,
    app: Option<&tauri::AppHandle>,
    user_ids: Vec<i64>,
    client: reqwest::Client,
    pace: std::time::Duration,
    mut on_entry: impl FnMut(&TermsCheckEntry),
) -> Vec<TermsCheckEntry> {
    use futures_util::StreamExt;

    let mut entries = Vec::new();
    let mut checks = futures_util::stream::iter(user_ids.into_iter().enumerate())
        .map(|(index, user_id)| {
            let client = client.clone();
            async move {
                if index >= TERMS_CONCURRENCY {
                    tokio::time::sleep(pace).await;
                }
                terms_entry(user_id, fetch_terms(state, app, user_id, client).await)
            }
        })
        .buffer_unordered(TERMS_CONCURRENCY);
    while let Some(entry) = checks.next().await {
        on_entry(&entry);
        entries.push(entry);
    }
    entries
}

#[tauri::command]
async fn check_accounts_terms(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    user_ids: Vec<i64>,
) -> Result<Vec<TermsCheckEntry>, String> {
    let _guard = try_begin_terms_check()?;
    let mut ids = Vec::new();
    for id in user_ids {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    let event_app = app.clone();
    Ok(run_terms_checks(
        state.inner(),
        Some(&app),
        ids,
        api::http_client::client(),
        TERMS_PACE,
        move |entry| {
            let _ = event_app.emit("account-terms", entry);
        },
    )
    .await)
}

/// Aceita os acordos pendentes de **uma** conta. Confere de novo antes (o que
/// vale é o que o Roblox pede agora), envia e confere depois. Cada resposta do
/// Roblox vai para o Console e volta para a tela como veio.
async fn accept_terms_of(
    state: &AccountStore,
    app: Option<&tauri::AppHandle>,
    user_id: i64,
) -> Result<TermsAcceptResult, String> {
    let before = fetch_terms(state, app, user_id, api::http_client::client()).await?;
    if before.pending.is_empty() {
        return Ok(TermsAcceptResult { accepted: true, remaining: Some(Vec::new()), attempts: Vec::new() });
    }
    let ids: Vec<String> = before.pending.iter().map(|p| p.id.clone()).collect();
    let attempts = read_without_refresh(state, user_id, |cookie| async move {
        api::roblox::accept_agreements(&cookie, &ids).await
    })
    .await?;

    if let Some(app) = app {
        for attempt in &attempts {
            let message = log_msg!(
                "Accept terms ({{shape}}): Roblox answered {{status}}: {{body}}",
                shape = attempt.shape.clone(),
                status = attempt.status,
                body = attempt.body.clone()
            );
            let level = if (200..300).contains(&attempt.status) { "info" } else { "warn" };
            emit_launch_log(app, user_id, level, "terms", message);
        }
    }

    let sent_ok = attempts.last().map(|a| (200..300).contains(&a.status)).unwrap_or(false);
    let remaining = fetch_terms(state, app, user_id, api::http_client::client())
        .await
        .ok()
        .map(|status| status.pending);
    let accepted = sent_ok && remaining.as_ref().map(Vec::is_empty).unwrap_or(false);
    if let Some(app) = app {
        match (&remaining, accepted) {
            (_, true) => emit_launch_log(
                app,
                user_id,
                "success",
                "terms",
                log_msg!("Accept terms: done, nothing is pending anymore."),
            ),
            (Some(left), false) => emit_launch_log(
                app,
                user_id,
                "warn",
                "terms",
                log_msg!("Accept terms: {{count}} agreement(s) still pending afterwards.", count = left.len()),
            ),
            (None, false) => emit_launch_log(
                app,
                user_id,
                "warn",
                "terms",
                log_msg!("Accept terms: could not confirm afterwards. Check the terms again."),
            ),
        }
    }
    Ok(TermsAcceptResult { accepted, remaining, attempts })
}

/// Só por pedido explícito da pessoa (botão "Accept terms" depois da confirmação).
#[tauri::command]
async fn accept_account_terms(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
) -> Result<TermsAcceptResult, String> {
    accept_terms_of(state.inner(), Some(&app), user_id).await
}

#[cfg(test)]
mod terms_command_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server, mount_csrf};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    fn temp_store(tag: &str) -> AccountStore {
        crypto::init();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        AccountStore::new(std::env::temp_dir().join(format!("ram-terms-{tag}-{nanos}.json")))
    }

    async fn mount_pending(token: &str, body: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path(mock_path("apis", "/user-agreements/v1/agreements-resolution/Web")))
            .and(header("cookie", cookie_of(token)))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(mock_server().await)
            .await;
    }

    fn add(store: &AccountStore, token: &str, user_id: i64) {
        store
            .add(crate::data::accounts::Account::new(token.into(), format!("u{user_id}"), user_id))
            .unwrap();
    }

    #[tokio::test]
    async fn a_batch_reports_pending_none_and_error_per_account() {
        mount_pending("trm-p", serde_json::json!([{ "agreementId": "a-1", "title": "Terms of Use" }])).await;
        mount_pending("trm-n", serde_json::json!([])).await;
        Mock::given(method("GET"))
            .and(path(mock_path("apis", "/user-agreements/v1/agreements-resolution/Web")))
            .and(header("cookie", cookie_of("trm-e")))
            .respond_with(ResponseTemplate::new(429))
            .mount(mock_server().await)
            .await;
        let store = temp_store("batch");
        add(&store, "trm-p", 9101);
        add(&store, "trm-n", 9102);
        add(&store, "trm-e", 9103);

        let mut seen = 0;
        let mut entries = run_terms_checks(
            &store,
            None,
            vec![9101, 9102, 9103],
            api::http_client::client(),
            std::time::Duration::ZERO,
            |_| seen += 1,
        )
        .await;
        entries.sort_by_key(|e| e.user_id);
        assert_eq!(seen, 3);
        assert_eq!(entries[0].status.as_ref().unwrap().pending[0].title, "Terms of Use");
        assert_eq!(entries[1].status, Some(TermsStatus { pending: Vec::new() }));
        assert!(entries[2].status.is_none());
        assert!(entries[2].error.as_deref().unwrap().contains("Try again"));
    }

    /// Conferir é leitura: nenhum sign-out, nenhum POST, o cookie fica como estava.
    #[tokio::test]
    async fn checking_never_signs_out_or_posts() {
        Mock::given(method("GET"))
            .and(path(mock_path("apis", "/user-agreements/v1/agreements-resolution/Web")))
            .and(header("cookie", cookie_of("trm-401")))
            .respond_with(ResponseTemplate::new(401))
            .mount(mock_server().await)
            .await;
        let store = temp_store("401");
        add(&store, "trm-401", 9201);
        let entries = run_terms_checks(
            &store,
            None,
            vec![9201],
            api::http_client::client(),
            std::time::Duration::ZERO,
            |_| {},
        )
        .await;
        assert!(entries[0].error.as_deref().unwrap().contains("status 401"));
        let mine = |r: &wiremock::Request| {
            r.headers.get("cookie").map(|c| c.as_bytes() == cookie_of("trm-401").as_bytes()).unwrap_or(false)
        };
        let received = mock_server().await.received_requests().await.unwrap_or_default();
        assert_eq!(received.iter().filter(|r| mine(r) && r.url.path().contains("signoutfromallsessions")).count(), 0);
        assert_eq!(received.iter().filter(|r| mine(r) && r.method.as_str() == "POST").count(), 0);
        assert_eq!(store.get_all().unwrap()[0].security_token, "trm-401");
    }

    #[tokio::test]
    async fn accepting_sends_the_pending_ids_and_confirms_with_a_second_read() {
        use std::sync::atomic::AtomicUsize;
        // 1ª leitura: pendente; as seguintes: nada (depois do aceite).
        let reads = std::sync::Arc::new(AtomicUsize::new(0));
        let counter = reads.clone();
        Mock::given(method("GET"))
            .and(path(mock_path("apis", "/user-agreements/v1/agreements-resolution/Web")))
            .and(header("cookie", cookie_of("trm-acc")))
            .respond_with(move |_: &wiremock::Request| {
                if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!([{ "agreementId": "a-7", "title": "Terms of Use" }]))
                } else {
                    ResponseTemplate::new(200).set_body_json(serde_json::json!([]))
                }
            })
            .mount(mock_server().await)
            .await;
        mount_csrf("trm-acc", "csrf-acc").await;
        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/user-agreements/v1/acceptances")))
            .and(header("cookie", cookie_of("trm-acc")))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(mock_server().await)
            .await;
        let store = temp_store("accept");
        add(&store, "trm-acc", 9301);

        let result = accept_terms_of(&store, None, 9301).await.unwrap();
        assert!(result.accepted, "{result:?}");
        assert_eq!(result.remaining, Some(Vec::new()));
        assert_eq!(result.attempts.len(), 1);
        assert_eq!(result.attempts[0].status, 200);
        assert_eq!(reads.load(Ordering::SeqCst), 2);
    }

    /// Roblox disse 200, mas a conta continua pendente: não é "aceito".
    #[tokio::test]
    async fn a_200_that_changes_nothing_is_not_reported_as_accepted() {
        mount_pending("trm-stuck", serde_json::json!([{ "agreementId": "a-8", "title": "Privacy Policy" }])).await;
        mount_csrf("trm-stuck", "csrf-stuck").await;
        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/user-agreements/v1/acceptances")))
            .and(header("cookie", cookie_of("trm-stuck")))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(mock_server().await)
            .await;
        let store = temp_store("stuck");
        add(&store, "trm-stuck", 9401);
        let result = accept_terms_of(&store, None, 9401).await.unwrap();
        assert!(!result.accepted);
        assert_eq!(result.remaining.as_ref().map(Vec::len), Some(1));
    }

    #[tokio::test]
    async fn a_refusal_comes_back_with_the_exact_answer() {
        mount_pending("trm-no", serde_json::json!([{ "agreementId": "a-5", "title": "Terms of Use" }])).await;
        mount_csrf("trm-no", "csrf-no").await;
        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/user-agreements/v1/acceptances")))
            .and(header("cookie", cookie_of("trm-no")))
            .respond_with(ResponseTemplate::new(403).set_body_string("{\"message\":\"Forbidden\"}"))
            .mount(mock_server().await)
            .await;
        let store = temp_store("refused");
        add(&store, "trm-no", 9501);
        let result = accept_terms_of(&store, None, 9501).await.unwrap();
        assert!(!result.accepted);
        assert_eq!(result.attempts.last().map(|a| a.status), Some(403));
        assert_eq!(result.attempts.last().map(|a| a.body.as_str()), Some("{\"message\":\"Forbidden\"}"));
    }

    /// Sem id reconhecido, a API nem é chamada: o navegador resolve.
    #[tokio::test]
    async fn an_agreement_without_an_id_is_not_sent_to_the_api() {
        mount_pending("trm-noid", serde_json::json!([{ "mystery": true }])).await;
        let store = temp_store("noid");
        add(&store, "trm-noid", 9601);
        let err = accept_terms_of(&store, None, 9601).await.unwrap_err();
        assert!(err.contains("did not give an id"), "{err}");
    }

    #[test]
    fn only_one_terms_check_runs_at_a_time() {
        let first = try_begin_terms_check().expect("first run");
        assert!(try_begin_terms_check().is_err());
        drop(first);
        assert!(try_begin_terms_check().is_ok());
    }
}
