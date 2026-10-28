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
// - O nome da conta sai **à mostra** por padrão (o canal é do próprio usuário;
//   pedido do dono, 15/10/2026). "Hide account names" (`Webhook.MaskNames`)
//   mascara como o "Names hidden" da tela e tira foto e link do perfil. A
//   mensagem nunca leva cookie, senha, Job ID nem link de servidor privado.
// - A mensagem sai no idioma do app (`General.Language`: en/pt/es) e ganha, na
//   hora de enviar, o nome do jogo e a foto da conta (`webhook_enrich`).

use api::discord_webhook::{
    build_message, build_test_message, parse_webhook_url, send as send_webhook, SendOutcome, WebhookConfig,
    WebhookEnrichment, WebhookEvent, WebhookEventKind, WebhookLang, WebhookQueue, WebhookReason,
};

const WEBHOOK_DISPATCH_TICK: std::time::Duration = std::time::Duration::from_secs(1);

/// Teto de cada busca do enriquecimento (nome do jogo, foto): a mensagem não
/// espera mais que isso — sem resposta, sai sem eles.
const WEBHOOK_ENRICH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);

/// Nome de cada jogo já buscado (Place ID → nome), para não pedir de novo.
static WEBHOOK_GAME_NAMES: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<i64, String>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

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

fn webhook_lang(app: &tauri::AppHandle) -> WebhookLang {
    let language = app
        .state::<SettingsStore>()
        .get("General", "Language")
        .ok()
        .flatten()
        .unwrap_or_default();
    WebhookLang::from_setting(&language)
}

/// Nome do jogo e foto da conta, buscados na hora de enviar. Só leitura: o
/// nome do jogo usa o cookie de uma conta do aviso **sem** renovar sessão; a
/// foto não usa cookie. Cada busca tem teto (`WEBHOOK_ENRICH_TIMEOUT`); falhou,
/// a mensagem sai sem aquilo.
async fn webhook_enrich(app: &tauri::AppHandle, events: &[WebhookEvent]) -> WebhookEnrichment {
    let mut out = WebhookEnrichment::default();

    let places: Vec<i64> = events.iter().filter_map(|e| e.place_id).collect();
    let users: Vec<i64> = events.iter().map(|e| e.user_id).collect();
    out.game_names = webhook_game_names(app, &places, &users).await;

    let mut users: Vec<i64> = Vec::new();
    for e in events.iter().filter(|e| !e.masked && e.user_id > 0) {
        if !users.contains(&e.user_id) {
            users.push(e.user_id);
        }
    }
    if !users.is_empty() {
        if let Ok(Ok(thumbs)) =
            tokio::time::timeout(WEBHOOK_ENRICH_TIMEOUT, api::roblox::get_avatar_headshots(&users, "150x150")).await
        {
            for t in thumbs {
                if let Some(url) = t.image_url.filter(|u| !u.trim().is_empty()) {
                    out.avatars.insert(t.target_id, url);
                }
            }
        }
    }
    out
}

/// O nome de cada jogo (Place ID → nome), do cache ou buscado de uma vez. O
/// cookie é o da primeira destas contas que tiver um (só leitura, sem renovar
/// sessão). Sem resposta no teto, o jogo fica de fora e a mensagem mostra o
/// Place ID.
async fn webhook_game_names(
    app: &tauri::AppHandle,
    places: &[i64],
    cookie_users: &[i64],
) -> std::collections::HashMap<i64, String> {
    let mut out = std::collections::HashMap::new();
    let mut missing: Vec<i64> = Vec::new();
    for place in places.iter().copied().filter(|p| *p > 0) {
        let cached = WEBHOOK_GAME_NAMES.lock().ok().and_then(|m| m.get(&place).cloned());
        match cached {
            Some(name) => {
                out.insert(place, name);
            }
            None if !missing.contains(&place) => missing.push(place),
            None => {}
        }
    }
    if missing.is_empty() {
        return out;
    }
    let store = app.state::<AccountStore>();
    let saved: Vec<i64> = store.get_all().unwrap_or_default().iter().map(|a| a.user_id).collect();
    let cookie = webhook_cookie_order(cookie_users, &saved)
        .into_iter()
        .find_map(|id| get_cookie(store.inner(), id).ok());
    if let Ok(Ok(details)) =
        tokio::time::timeout(WEBHOOK_ENRICH_TIMEOUT, api::roblox::get_place_details(&missing, cookie.as_deref())).await
    {
        for d in details {
            if !d.name.trim().is_empty() {
                if let Ok(mut cache) = WEBHOOK_GAME_NAMES.lock() {
                    cache.insert(d.place_id, d.name.clone());
                }
                out.insert(d.place_id, d.name);
            }
        }
    }
    out
}

/// De quem pegar o cookie para o nome do jogo: as contas do aviso primeiro e,
/// depois, qualquer conta salva. Aviso sem conta (lote de launch, memória do
/// Windows) saía com "Place 123" porque não achava cookie (teste real,
/// 28/10/2026).
fn webhook_cookie_order(event_users: &[i64], saved: &[i64]) -> Vec<i64> {
    let mut out: Vec<i64> = Vec::new();
    for id in event_users.iter().chain(saved.iter()).copied().filter(|id| *id > 0) {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// Nome da conta na mensagem: Alias || Username; com "Hide account names"
/// ligado, a mesma máscara da tela com os nomes ocultos.
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
        Some(a) => webhook_account_name(&a.alias, &a.username, config.mask_names, letters),
        None => "Unknown account".to_string(),
    }
}

/// O minuto do dia na hora local (o horário silencioso e o resumo são na hora
/// de quem usa).
fn local_minute_of_day() -> u32 {
    use chrono::Timelike;
    let now = chrono::Local::now();
    now.hour() * 60 + now.minute()
}

/// Põe um aviso na fila, se a opção e a caixa deste evento estiverem ligadas.
/// Barato e sem rede: pode ser chamado do caminho de launch/reconexão.
pub(crate) fn notify_webhook(
    app: &tauri::AppHandle,
    kind: WebhookEventKind,
    user_id: i64,
    reason: Option<WebhookReason>,
    place_id: Option<i64>,
) {
    let config = webhook_config(app);
    if !config.wants(kind) {
        return;
    }
    let now_ms = chrono::Utc::now().timestamp_millis();
    let event = WebhookEvent {
        kind,
        user_id,
        // Evento do app inteiro (lote, memória do Windows): sem conta.
        account: if kind.has_account() { webhook_account_label(app, &config, user_id) } else { String::new() },
        masked: config.mask_names,
        reason,
        place_id,
        at_ms: now_ms,
    };
    with_webhook_queue(|queue| queue.push(event, now_ms));
}

/// Motivo da queda, como código (a mensagem traduz). O texto cru do kick do
/// Roblox pode citar o nome real de outro jogador ("kicked by <username>"): com
/// "Hide account names" ligado, só a CATEGORIA; o texto só com os nomes à mostra.
fn webhook_drop_reason(drop: &ClientDrop, mask_names: bool) -> WebhookReason {
    let code = drop.code;
    match drop.kind {
        DropKind::Disconnected => match drop.reason {
            Some(DropReason::ConnectionLost) => WebhookReason::ConnectionLost { code },
            Some(DropReason::JoinedElsewhere) => WebhookReason::JoinedElsewhere { code },
            Some(DropReason::Idle) => WebhookReason::Idle { code },
            _ => WebhookReason::Disconnected { code },
        },
        DropKind::Kicked => WebhookReason::Kicked {
            code,
            message: if mask_names { None } else { drop.message.clone() },
        },
        DropKind::ServerShutdown => WebhookReason::ServerShutdown { code },
        DropKind::Crashed => WebhookReason::ClosedWithoutLeaving,
    }
}

/// O que cada aviso de reconexão vira (ou nada).
fn webhook_reconnect_event(notice: &ReconnectNotice) -> Option<(WebhookEventKind, i64, Option<WebhookReason>)> {
    match notice {
        ReconnectNotice::Reconnected { user_id } => Some((WebhookEventKind::Reconnected, *user_id, None)),
        ReconnectNotice::GaveUp { user_id, attempts } => Some((
            WebhookEventKind::ReconnectFailed,
            *user_id,
            Some(WebhookReason::GaveUp { attempts: *attempts }),
        )),
        ReconnectNotice::Stopped { user_id, reason } => {
            let reason = match reason {
                ReconnectStopReason::Banned => WebhookReason::Banned,
                ReconnectStopReason::SessionExpired => WebhookReason::SessionExpired,
                ReconnectStopReason::NoDestination => WebhookReason::NoDestination,
                // A pessoa fechou, ou a conta está em outro lugar: não é falha.
                ReconnectStopReason::JoinedElsewhere
                | ReconnectStopReason::ClosedByUser
                | ReconnectStopReason::OpenedOutsideApp => return None,
            };
            Some((WebhookEventKind::ReconnectFailed, *user_id, Some(reason)))
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
    let mask_names = webhook_config(app).mask_names;
    for notice in notices {
        match notice {
            HealthNotice::Dropped { user_id, drop, .. } => {
                notify_webhook(
                    app,
                    WebhookEventKind::Dropped,
                    *user_id,
                    Some(webhook_drop_reason(drop, mask_names)),
                    webhook_place_of(*user_id),
                );
                discord_stats_note_drop(app, *user_id);
            }
            HealthNotice::NotResponding {
                user_id,
                not_responding: true,
            } => notify_webhook(
                app,
                WebhookEventKind::NotResponding,
                *user_id,
                Some(WebhookReason::NotResponding),
                webhook_place_of(*user_id),
            ),
            // O cliente nunca é fechado (pode ser a conta principal); o aviso
            // leva só que entrou em outra conta, sem o id dela.
            HealthNotice::WrongAccount { user_id, .. } => notify_webhook(
                app,
                WebhookEventKind::WrongAccount,
                *user_id,
                Some(WebhookReason::OtherAccount),
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
            // Horário silencioso com "não mandar nada": o lote some.
            let minute = local_minute_of_day();
            if config.holds_messages(minute) {
                continue;
            }
            let extra = webhook_enrich(&app, &batch).await;
            let mention = config.mention_for(&batch, minute);
            let body = build_message(&batch, &mention, webhook_lang(&app), &extra);
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

/// "Send test message": manda já, para a URL do campo (salva ou não). Um cartão
/// "Webhook conectado" com os avisos marcados e um exemplo de queda com a
/// primeira conta do app (nome à mostra ou mascarado, como a opção manda).
#[tauri::command]
async fn send_test_webhook(app: tauri::AppHandle, url: String, mention: Option<String>) -> Result<(), String> {
    let target = parse_webhook_url(&url)?;
    let config = webhook_config(&app);
    let lang = webhook_lang(&app);
    let enabled: Vec<WebhookEventKind> =
        WebhookEventKind::ALL.into_iter().filter(|k| config.wants_box(*k)).collect();

    let first = app.state::<AccountStore>().get_all().unwrap_or_default().into_iter().next();
    let sample = first.map(|a| WebhookEvent {
        kind: WebhookEventKind::Dropped,
        user_id: a.user_id,
        account: webhook_account_label(&app, &config, a.user_id),
        masked: config.mask_names,
        reason: Some(WebhookReason::ClosedWithoutLeaving),
        place_id: None,
        at_ms: chrono::Utc::now().timestamp_millis(),
    });
    let extra = match &sample {
        Some(e) => webhook_enrich(&app, std::slice::from_ref(e)).await,
        None => WebhookEnrichment::default(),
    };
    let body = build_test_message(
        mention.as_deref().unwrap_or(&config.mention),
        lang,
        &enabled,
        sample.as_ref().map(|e| (e, &extra)),
    );
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
    fn an_event_without_an_account_still_finds_a_cookie_for_the_game_name() {
        // Lote de launch: user_id 0, então vale qualquer conta salva.
        assert_eq!(webhook_cookie_order(&[0], &[7, 8]), vec![7, 8]);
        // Com conta no aviso, ela vem primeiro, sem repetir.
        assert_eq!(webhook_cookie_order(&[8, 0, 8], &[7, 8]), vec![8, 7]);
        assert!(webhook_cookie_order(&[0], &[]).is_empty());
    }

    #[test]
    fn names_are_masked_like_the_hidden_names_mode() {
        assert_eq!(webhook_account_name("Farm 1", "farm_one", false, 0), "Farm 1");
        assert_eq!(webhook_account_name("", "farm_one", false, 0), "farm_one");
        assert_eq!(webhook_account_name("Farm 1", "farm_one", true, 0), "************");
        assert_eq!(webhook_account_name("Farm 1", "farm_one", true, 2), "Fa********");
        assert_eq!(webhook_account_name("", "", true, 0), "Unknown account");
    }

    #[test]
    fn drop_reasons_become_codes_with_the_roblox_code() {
        let drop = |kind, reason, code, message: Option<&str>| ClientDrop {
            kind,
            reason,
            code,
            message: message.map(String::from),
            since_ms: 0,
        };
        assert_eq!(
            webhook_drop_reason(&drop(DropKind::Disconnected, Some(DropReason::ConnectionLost), Some(277), None), false),
            WebhookReason::ConnectionLost { code: Some(277) }
        );
        assert_eq!(
            webhook_drop_reason(&drop(DropKind::Crashed, None, None, None), false),
            WebhookReason::ClosedWithoutLeaving
        );
    }

    #[test]
    fn a_kick_message_only_goes_out_with_the_names_showing() {
        let drop = |message: Option<&str>, code| ClientDrop {
            kind: DropKind::Kicked,
            reason: None,
            code,
            message: message.map(String::from),
            since_ms: 0,
        };
        // Mascarado: o texto cru do kick (pode trazer o nome real de um
        // jogador) não sai; só a categoria, com o código.
        assert_eq!(
            webhook_drop_reason(&drop(Some("Kicked by SomeRealPlayer42"), Some(267)), true),
            WebhookReason::Kicked { code: Some(267), message: None }
        );
        // Nomes à mostra: a mensagem completa sai.
        assert_eq!(
            webhook_drop_reason(&drop(Some("Kicked by SomeRealPlayer42"), Some(267)), false),
            WebhookReason::Kicked { code: Some(267), message: Some("Kicked by SomeRealPlayer42".into()) }
        );
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
            include_str!("../../../src/components/discord/DiscordSettings.tsx"),
            include_str!("../../../src/utils/discordWebhook.ts"),
        ] {
            assert!(!source.contains("fetch("), "frontend must not call fetch for the webhook");
        }
    }
}
