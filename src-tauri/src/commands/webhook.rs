// Aviso no Discord por webhook (ideia 5): a cola com o resto do app.
// Ver docs/features/webhook.md; a parte pura e o envio estão em
// api/discord_webhook.rs.
//
// - **Desligado por padrão** (`Webhook.Enabled`): os avisos saem do PC.
// - Quem avisa é o **backend**, nos pontos onde os eventos já nascem: o monitor
//   de quedas (`webhook_after_health_tick`), a reconexão (`publish_reconnect`)
//   e as falhas de launch (launch.rs). A tela não faz rede.
// - `notify_webhook` só lê as settings e põe na fila — nunca espera rede, então
//   não atrasa launch nem reconexão. Quem manda é a task de
//   `start_webhook_dispatcher`, uma passada por segundo.
// - O nome da conta sai mascarado por padrão (`Webhook.HideNames`, mesma
//   máscara do "Names hidden" da tela). A mensagem nunca leva cookie, senha,
//   Job ID nem link de servidor privado.

use api::discord_webhook::{
    build_message, parse_webhook_url, send as send_webhook, SendOutcome, WebhookConfig, WebhookEvent,
    WebhookEventKind, WebhookQueue,
};

const WEBHOOK_DISPATCH_TICK: std::time::Duration = std::time::Duration::from_secs(1);

/// Motivos da falha de launch na mensagem. Só a etapa — o texto do erro pode
/// trazer URL de launch, ticket ou código de servidor privado.
const WEBHOOK_AUTH_FAILED: &str = "could not get the authentication ticket";
const WEBHOOK_SPAWN_FAILED: &str = "could not open the Roblox client";

static WEBHOOK_QUEUE: std::sync::LazyLock<std::sync::Mutex<WebhookQueue>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(WebhookQueue::new()));

fn with_webhook_queue<R>(f: impl FnOnce(&mut WebhookQueue) -> R) -> R {
    let mut queue = match WEBHOOK_QUEUE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    f(&mut queue)
}

fn webhook_config(app: &tauri::AppHandle) -> WebhookConfig {
    let settings = app.state::<SettingsStore>();
    WebhookConfig::from_settings(|key| settings.get("Webhook", key).ok().flatten())
}

/// Nome da conta na mensagem: Alias || Username; com "Hide account names"
/// (padrão), a mesma máscara da tela com os nomes ocultos.
fn webhook_account_name(alias: &str, username: &str, hide: bool, preview_letters: i64) -> String {
    if alias.trim().is_empty() && username.trim().is_empty() {
        return "Unknown account".to_string();
    }
    window_account_label(alias, username, hide, preview_letters)
}

fn webhook_account_label(app: &tauri::AppHandle, config: &WebhookConfig, user_id: i64) -> String {
    let letters = app
        .state::<SettingsStore>()
        .get_int("General", "HiddenNameLetters")
        .unwrap_or(0);
    let account = app
        .state::<AccountStore>()
        .get_all()
        .unwrap_or_default()
        .into_iter()
        .find(|a| a.user_id == user_id);
    match account {
        Some(a) => webhook_account_name(&a.alias, &a.username, config.hide_names, letters),
        None => "Unknown account".to_string(),
    }
}

/// Põe um aviso na fila, se a opção e a caixa deste evento estiverem ligadas.
/// Barato e sem rede: pode ser chamado do caminho de launch/reconexão.
pub(crate) fn notify_webhook(
    app: &tauri::AppHandle,
    kind: WebhookEventKind,
    user_id: i64,
    reason: Option<String>,
    place_id: Option<i64>,
) {
    let config = webhook_config(app);
    if !config.wants(kind) {
        return;
    }
    let now_ms = chrono::Utc::now().timestamp_millis();
    let event = WebhookEvent {
        kind,
        account: webhook_account_label(app, &config, user_id),
        reason,
        place_id,
        at_ms: now_ms,
    };
    with_webhook_queue(|queue| queue.push(event, now_ms));
}

/// Motivo da queda em inglês (a mensagem não passa pelo i18n da tela).
///
/// O texto cru do kick do Roblox pode citar o nome real de outro jogador
/// ("kicked by <username>", "<username> banned you", ...). Com "Hide account
/// names" ligado (padrão) sai só a CATEGORIA ("kicked from the game"); o texto
/// completo só quando a pessoa desligou o mascaramento.
fn webhook_drop_reason(drop: &ClientDrop, hide_names: bool) -> String {
    let code = drop.code.map(|c| format!(" (code {c})")).unwrap_or_default();
    match drop.kind {
        DropKind::Disconnected => match drop.reason {
            Some(DropReason::ConnectionLost) => format!("lost the connection{code}"),
            Some(DropReason::JoinedElsewhere) => format!("the account joined somewhere else{code}"),
            Some(DropReason::Idle) => format!("idle for too long{code}"),
            _ => format!("disconnected{code}"),
        },
        DropKind::Kicked => match &drop.message {
            Some(message) if !hide_names => format!("kicked: {message}"),
            _ => format!("kicked from the game{code}"),
        },
        DropKind::ServerShutdown => format!("the server shut down{code}"),
        DropKind::Crashed => "the client closed without leaving the game".to_string(),
    }
}

/// O que cada aviso de reconexão vira (ou nada).
fn webhook_reconnect_event(notice: &ReconnectNotice) -> Option<(WebhookEventKind, i64, Option<String>)> {
    match notice {
        ReconnectNotice::Reconnected { user_id } => Some((WebhookEventKind::Reconnected, *user_id, None)),
        ReconnectNotice::GaveUp { user_id, attempts } => Some((
            WebhookEventKind::ReconnectFailed,
            *user_id,
            Some(format!("gave up after {attempts} attempts")),
        )),
        ReconnectNotice::Stopped { user_id, reason } => {
            let text = match reason {
                ReconnectStopReason::Banned => "the account is banned",
                ReconnectStopReason::SessionExpired => "the session expired; log in to the account again",
                ReconnectStopReason::NoDestination => "no game to return to",
                // A pessoa fechou, ou a conta está em outro lugar: não é falha.
                ReconnectStopReason::JoinedElsewhere
                | ReconnectStopReason::ClosedByUser
                | ReconnectStopReason::OpenedOutsideApp => return None,
            };
            Some((WebhookEventKind::ReconnectFailed, *user_id, Some(text.to_string())))
        }
        _ => None,
    }
}

pub(crate) fn webhook_after_reconnect(app: &tauri::AppHandle, notices: &[ReconnectNotice]) {
    for notice in notices {
        if let Some((kind, user_id, reason)) = webhook_reconnect_event(notice) {
            notify_webhook(app, kind, user_id, reason, webhook_place_of(user_id));
        }
    }
}

/// Onde a conta estava, se o monitor de quedas sabe (só o Place ID; o Job ID
/// nunca vai na mensagem).
fn webhook_place_of(user_id: i64) -> Option<i64> {
    #[cfg(target_os = "windows")]
    {
        return CLIENT_HEALTH_VIEWS
            .lock()
            .ok()?
            .get(&user_id)
            .and_then(|view| view.destination.as_ref())
            .map(|d| d.place_id);
    }
    #[allow(unreachable_code)]
    {
        let _ = user_id;
        None
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn webhook_after_health_tick(app: &tauri::AppHandle, notices: &[HealthNotice]) {
    let hide_names = webhook_config(app).hide_names;
    for notice in notices {
        match notice {
            HealthNotice::Dropped { user_id, drop, .. } => notify_webhook(
                app,
                WebhookEventKind::Dropped,
                *user_id,
                Some(webhook_drop_reason(drop, hide_names)),
                webhook_place_of(*user_id),
            ),
            HealthNotice::NotResponding {
                user_id,
                not_responding: true,
            } => notify_webhook(
                app,
                WebhookEventKind::NotResponding,
                *user_id,
                Some("the window has not responded for 30 s".to_string()),
                webhook_place_of(*user_id),
            ),
            // O cliente nunca é fechado (pode ser a conta principal); o aviso
            // leva só que entrou em outra conta, sem o id dela.
            HealthNotice::WrongAccount { user_id, .. } => notify_webhook(
                app,
                WebhookEventKind::WrongAccount,
                *user_id,
                Some("the client opened logged into a different Roblox account".to_string()),
                webhook_place_of(*user_id),
            ),
            _ => {}
        }
    }
}

/// Liga a fila em segundo plano. Sem aviso na fila, cada passada só olha a fila.
pub(crate) fn start_webhook_dispatcher(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let client = match api::http_client::builder().build() {
            Ok(client) => client,
            Err(e) => {
                eprintln!("Webhook client failed to start: {e}");
                return;
            }
        };
        loop {
            tokio::time::sleep(WEBHOOK_DISPATCH_TICK).await;
            let now_ms = chrono::Utc::now().timestamp_millis();
            let Some(batch) = with_webhook_queue(|queue| queue.take_batch(now_ms)) else {
                continue;
            };
            // Desligou (ou trocou a URL por uma inválida) com aviso na fila: some.
            let config = webhook_config(&app);
            let Some(target) = config.target.clone().filter(|_| config.enabled) else {
                continue;
            };
            let body = build_message(&batch, &config.mention);
            match send_webhook(&client, &target, &body).await {
                Ok(SendOutcome::Sent) => {}
                Ok(SendOutcome::RateLimited { retry_after_ms }) => {
                    let now_ms = chrono::Utc::now().timestamp_millis();
                    with_webhook_queue(|queue| queue.rate_limited(batch, now_ms, retry_after_ms));
                }
                // O erro já vem sem URL e sem token.
                Err(e) => eprintln!("Discord webhook: {e}"),
            }
        }
    });
}

/// "Send test message": manda já, para a URL do campo (salva ou não).
#[tauri::command]
async fn send_test_webhook(app: tauri::AppHandle, url: String, mention: Option<String>) -> Result<(), String> {
    let target = parse_webhook_url(&url)?;
    let config = webhook_config(&app);
    let letters = app
        .state::<SettingsStore>()
        .get_int("General", "HiddenNameLetters")
        .unwrap_or(0);
    let sample = WebhookEvent {
        kind: WebhookEventKind::Dropped,
        account: webhook_account_name("", "Example account", config.hide_names, letters),
        reason: Some("test message from MultiAlt; real notifications look like this".to_string()),
        place_id: None,
        at_ms: chrono::Utc::now().timestamp_millis(),
    };
    let body = build_message(&[sample], mention.as_deref().unwrap_or(&config.mention));
    let client = api::http_client::client();
    match send_webhook(&client, &target, &body).await? {
        SendOutcome::Sent => Ok(()),
        SendOutcome::RateLimited { .. } => Err("Discord asked to slow down. Try again in a few seconds.".to_string()),
    }
}

#[cfg(test)]
mod webhook_glue_tests {
    use super::*;

    #[test]
    fn names_are_masked_like_the_hidden_names_mode() {
        assert_eq!(webhook_account_name("Farm 1", "farm_one", false, 0), "Farm 1");
        assert_eq!(webhook_account_name("", "farm_one", false, 0), "farm_one");
        assert_eq!(webhook_account_name("Farm 1", "farm_one", true, 0), "************");
        assert_eq!(webhook_account_name("Farm 1", "farm_one", true, 2), "Fa********");
        assert_eq!(webhook_account_name("", "", true, 0), "Unknown account");
    }

    #[test]
    fn drop_reasons_read_in_english_with_the_code() {
        let drop = |kind, reason, code, message: Option<&str>| ClientDrop {
            kind,
            reason,
            code,
            message: message.map(String::from),
            since_ms: 0,
        };
        assert_eq!(
            webhook_drop_reason(
                &drop(DropKind::Disconnected, Some(DropReason::ConnectionLost), Some(277), None),
                false
            ),
            "lost the connection (code 277)"
        );
        assert_eq!(
            webhook_drop_reason(&drop(DropKind::Kicked, None, Some(267), Some("AFK")), false),
            "kicked: AFK"
        );
        assert_eq!(
            webhook_drop_reason(&drop(DropKind::Crashed, None, None, None), false),
            "the client closed without leaving the game"
        );
    }

    #[test]
    fn a_kick_reason_is_hidden_when_names_are_hidden() {
        let drop = |message: Option<&str>, code| ClientDrop {
            kind: DropKind::Kicked,
            reason: None,
            code,
            message: message.map(String::from),
            since_ms: 0,
        };
        // Com HideNames ligado (padrão), o texto cru do kick — que pode trazer o
        // nome real de um jogador — não sai; só a categoria, com o código.
        assert_eq!(
            webhook_drop_reason(&drop(Some("Kicked by SomeRealPlayer42"), Some(267)), true),
            "kicked from the game (code 267)"
        );
        // Com HideNames desligado, a mensagem completa sai.
        assert_eq!(
            webhook_drop_reason(&drop(Some("Kicked by SomeRealPlayer42"), Some(267)), false),
            "kicked: Kicked by SomeRealPlayer42"
        );
        // Sem mensagem, os dois caminhos dão a categoria.
        assert_eq!(webhook_drop_reason(&drop(None, Some(267)), true), "kicked from the game (code 267)");
        assert_eq!(webhook_drop_reason(&drop(None, Some(267)), false), "kicked from the game (code 267)");
    }

    #[test]
    fn only_real_reconnect_outcomes_become_notifications() {
        assert_eq!(
            webhook_reconnect_event(&ReconnectNotice::Reconnected { user_id: 1 }),
            Some((WebhookEventKind::Reconnected, 1, None))
        );
        assert_eq!(
            webhook_reconnect_event(&ReconnectNotice::GaveUp { user_id: 2, attempts: 5 }).map(|e| e.0),
            Some(WebhookEventKind::ReconnectFailed)
        );
        assert_eq!(
            webhook_reconnect_event(&ReconnectNotice::Stopped { user_id: 3, reason: ReconnectStopReason::Banned })
                .map(|e| e.0),
            Some(WebhookEventKind::ReconnectFailed)
        );
        // Fechou de propósito / está em outro lugar / tentativas no meio: não.
        for notice in [
            ReconnectNotice::Stopped { user_id: 4, reason: ReconnectStopReason::ClosedByUser },
            ReconnectNotice::Stopped { user_id: 4, reason: ReconnectStopReason::JoinedElsewhere },
            ReconnectNotice::Scheduled { user_id: 4, attempt: 1, in_ms: 5_000 },
            ReconnectNotice::Relaunched { user_id: 4, attempt: 1 },
        ] {
            assert_eq!(webhook_reconnect_event(&notice), None, "{notice:?}");
        }
    }

    #[test]
    fn the_frontend_never_posts_to_the_webhook_itself() {
        // A rede é do backend (CLAUDE.md): nenhum arquivo da tela fala com o
        // Discord diretamente.
        for source in [
            include_str!("../../../src/components/settings/NotificationsTab.tsx"),
            include_str!("../../../src/utils/discordWebhook.ts"),
        ] {
            assert!(!source.contains("fetch("), "frontend must not call fetch for the webhook");
        }
    }
}
