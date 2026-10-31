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
