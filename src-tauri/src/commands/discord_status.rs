// Página Discord: status ao vivo e resumo diário — a cola com o resto do app.
// Ver docs/features/webhook.md; a parte pura e o envio estão em
// api/discord_status.rs.
//
// - Um laço só (`start_discord_status_loop`, a cada 30 s) faz as três coisas:
//   edita o status quando passou o intervalo, soma o tempo de jogo e manda o
//   resumo na hora marcada. Também olha a memória livre do Windows para o
//   aviso "quase sem memória".
// - Tudo **desligado por padrão** e atrás do mesmo interruptor dos avisos
//   (`Webhook.Enabled` + URL válida): as informações saem do PC.
// - O status nunca menciona ninguém (é editado a cada poucos minutos). O
//   resumo menciona só com o texto dele (`MentionSummary`), e não no horário
//   silencioso.

use api::discord_status::{
    build_status_message, build_summary_message, send_status, send_summary, summary_due, system_memory_step,
    PlayStats, StatusEntry, StatusOutcome, STATUS_INTERVAL_DEFAULT, STATUS_INTERVAL_MAX, STATUS_INTERVAL_MIN,
    SUMMARY_TIME_DEFAULT,
};

const DISCORD_STATUS_TICK: std::time::Duration = std::time::Duration::from_secs(30);

/// O que o laço lembra entre uma passada e outra.
#[derive(Default)]
struct DiscordStatusState {
    /// A soma do resumo (carregada do `RAMDiscordStats.json` na primeira vez).
    stats: Option<PlayStats>,
    /// Quando o status foi mandado pela última vez (ms).
    last_status_ms: i64,
    /// Quando o tempo de jogo foi somado pela última vez (ms).
    last_play_ms: i64,
    /// Desde quando cada conta está em jogo (para o "1h 05m" do status).
    in_game_since: std::collections::HashMap<i64, i64>,
    /// O aviso de memória do Windows pode disparar (ver `system_memory_step`).
    memory_armed: bool,
    /// Nada sai antes disto (429 do Discord).
    blocked_until_ms: i64,
}

static DISCORD_STATUS: std::sync::LazyLock<std::sync::Mutex<DiscordStatusState>> =
    std::sync::LazyLock::new(|| {
        std::sync::Mutex::new(DiscordStatusState { memory_armed: true, ..Default::default() })
    });

fn with_discord_status<R>(f: impl FnOnce(&mut DiscordStatusState) -> R) -> R {
    let mut state = match DISCORD_STATUS.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    f(&mut state)
}

/// As opções do status e do resumo (seção `[Webhook]`).
struct DiscordStatusConfig {
    status: bool,
    interval_min: i64,
    message_id: String,
    summary: bool,
    summary_minute: u32,
    summary_mention: String,
}

fn discord_status_config(app: &tauri::AppHandle) -> DiscordStatusConfig {
    let settings = app.state::<SettingsStore>();
    let summary_time = settings.get_string("Webhook", "SummaryTime");
    DiscordStatusConfig {
        status: settings.get_bool("Webhook", "StatusEnabled"),
        interval_min: settings
            .get_int("Webhook", "StatusInterval")
            .unwrap_or(STATUS_INTERVAL_DEFAULT)
            .clamp(STATUS_INTERVAL_MIN, STATUS_INTERVAL_MAX),
        message_id: settings.get_string("Webhook", "StatusMessageId"),
        summary: settings.get_bool("Webhook", "SummaryEnabled"),
        summary_minute: api::discord_webhook::parse_clock(&summary_time)
            .or_else(|| api::discord_webhook::parse_clock(SUMMARY_TIME_DEFAULT))
            .unwrap_or(23 * 60),
        summary_mention: settings.get_string("Webhook", "MentionSummary").trim().chars().take(100).collect(),
    }
}

fn discord_stats_path() -> std::path::PathBuf {
    data::settings::get_runtime_data_dir().join("RAMDiscordStats.json")
}

fn load_discord_stats() -> PlayStats {
    std::fs::read_to_string(discord_stats_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Grava em arquivo temporário e troca, para um crash não deixar o arquivo
/// pela metade. Falhar ao gravar só perde a soma desde a última gravação.
fn save_discord_stats(stats: &PlayStats) {
    let path = discord_stats_path();
    let tmp = path.with_extension("json.tmp");
    if let Ok(text) = serde_json::to_string(stats) {
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

fn with_discord_stats<R>(f: impl FnOnce(&mut PlayStats) -> R) -> R {
    with_discord_status(|state| f(state.stats.get_or_insert_with(load_discord_stats)))
}

/// Uma queda entra na conta do resumo (só com o resumo ligado).
pub(crate) fn discord_stats_note_drop(app: &tauri::AppHandle, user_id: i64) {
    if !discord_status_config(app).summary || !webhook_config(app).enabled {
        return;
    }
    let now_ms = chrono::Utc::now().timestamp_millis();
    with_discord_stats(|stats| {
        stats.add_drop(user_id, now_ms);
        save_discord_stats(stats);
    });
}

/// Os clientes do Roblox abertos agora, como o status mostra. Só Windows (o
/// tracker e o monitor de quedas são de lá).
fn discord_status_entries(app: &tauri::AppHandle, config: &WebhookConfig, now_ms: i64) -> Vec<StatusEntry> {
    #[cfg(target_os = "windows")]
    {
        let processes = platform::windows::tracker().get_all();
        let views = CLIENT_HEALTH_VIEWS.lock().map(|v| v.clone()).unwrap_or_default();
        let mut entries: Vec<StatusEntry> = Vec::new();
        for process in processes {
            if entries.iter().any(|e| e.user_id == process.user_id) {
                continue;
            }
            let view = views.get(&process.user_id).filter(|v| v.pid == process.pid);
            let in_game = view.is_some_and(|v| v.in_game);
            let place_id = view.and_then(|v| v.destination.as_ref()).map(|d| d.place_id);
            entries.push(StatusEntry {
                user_id: process.user_id,
                account: webhook_account_label(app, config, process.user_id),
                masked: config.mask_names,
                place_id,
                in_game,
                in_game_since_ms: None,
                from_site: process.adopted,
            });
        }
        // Desde quando está em jogo: lembrado aqui (o monitor só diz "em jogo").
        with_discord_status(|state| {
            state.in_game_since.retain(|id, _| entries.iter().any(|e| e.user_id == *id && e.in_game));
            for entry in entries.iter_mut().filter(|e| e.in_game) {
                entry.in_game_since_ms = Some(*state.in_game_since.entry(entry.user_id).or_insert(now_ms));
            }
        });
        return entries;
    }
    #[allow(unreachable_code)]
    {
        let _ = (app, config, now_ms);
        Vec::new()
    }
}

fn local_day() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Manda (ou edita) o status agora. Guarda o id da mensagem nova em
/// `Webhook.StatusMessageId`, para as próximas edições irem na mesma.
async fn push_discord_status(
    app: &tauri::AppHandle,
    client: &reqwest::Client,
    config: &WebhookConfig,
    status: &DiscordStatusConfig,
) -> Result<(), String> {
    let target = config.target.clone().ok_or_else(|| "Paste the webhook URL first.".to_string())?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let entries = discord_status_entries(app, config, now_ms);
    let places: Vec<i64> = entries.iter().filter_map(|e| e.place_id).collect();
    let users: Vec<i64> = entries.iter().map(|e| e.user_id).collect();
    let games = webhook_game_names(app, &places, &users).await;
    let body = build_status_message(&entries, webhook_lang(app), &games, now_ms, status.interval_min);

    let mut message_id = Some(status.message_id.as_str()).filter(|id| !id.trim().is_empty());
    loop {
        match send_status(client, &target, message_id, &body).await? {
            StatusOutcome::Edited => return Ok(()),
            StatusOutcome::Posted(id) => {
                let _ = app.state::<SettingsStore>().set("Webhook", "StatusMessageId", &id);
                return Ok(());
            }
            // Apagaram a mensagem no canal: cria outra.
            StatusOutcome::Gone if message_id.is_some() => message_id = None,
            StatusOutcome::Gone => return Err("Discord answered 404: the webhook was deleted or the URL is wrong.".into()),
            StatusOutcome::RateLimited { retry_after_ms } => {
                with_discord_status(|state| state.blocked_until_ms = now_ms + retry_after_ms.clamp(500, 60_000));
                return Err("Discord asked to slow down. Try again in a few seconds.".into());
            }
        }
    }
}

/// O nome de cada conta do resumo, mascarado ou não como os avisos.
/// Conta que saiu do app fica de fora (o resumo a chama de "conta desconhecida").
fn discord_summary_names(app: &tauri::AppHandle, config: &WebhookConfig, stats: &PlayStats) -> std::collections::HashMap<i64, String> {
    let letters = app.state::<SettingsStore>().get_int("General", "HiddenNameLetters").unwrap_or(0);
    app.state::<AccountStore>()
        .get_all()
        .unwrap_or_default()
        .into_iter()
        .filter(|a| stats.accounts.contains_key(&a.user_id))
        .map(|a| (a.user_id, webhook_account_name(&a.alias, &a.username, config.mask_names, letters)))
        .collect()
}

/// Manda o resumo de `stats`. Não zera nada (quem zera é o laço, depois).
async fn push_discord_summary(
    app: &tauri::AppHandle,
    client: &reqwest::Client,
    config: &WebhookConfig,
    status: &DiscordStatusConfig,
    stats: &PlayStats,
) -> Result<(), String> {
    let target = config.target.clone().ok_or_else(|| "Paste the webhook URL first.".to_string())?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let places: Vec<i64> = stats.games.keys().copied().collect();
    let users: Vec<i64> = stats.accounts.keys().copied().collect();
    let games = webhook_game_names(app, &places, &users).await;
    let names = discord_summary_names(app, config, stats);
    let mention = if config.is_quiet(local_minute_of_day()) { "" } else { status.summary_mention.as_str() };
    let body = build_summary_message(stats, &names, config.mask_names, &games, webhook_lang(app), mention, now_ms);
    match send_summary(client, &target, &body).await? {
        api::discord_webhook::SendOutcome::Sent => Ok(()),
        api::discord_webhook::SendOutcome::RateLimited { .. } => {
            Err("Discord asked to slow down. Try again in a few seconds.".into())
        }
    }
}

/// Uma passada do laço.
async fn discord_status_tick(app: &tauri::AppHandle, client: &reqwest::Client) {
    let config = webhook_config(app);
    if !config.enabled || config.target.is_none() {
        return;
    }
    let status = discord_status_config(app);
    let now_ms = chrono::Utc::now().timestamp_millis();
    if with_discord_status(|state| now_ms < state.blocked_until_ms) {
        return;
    }

    // Windows quase sem memória (aviso comum, pela fila dos avisos).
    #[cfg(target_os = "windows")]
    if config.wants(WebhookEventKind::SystemLowMemory) {
        if let Some((free_mb, total_mb)) = platform::windows::system_memory_mb() {
            let fire = with_discord_status(|state| {
                let (fire, armed) = system_memory_step(state.memory_armed, free_mb, total_mb);
                state.memory_armed = armed;
                fire
            });
            if fire {
                notify_webhook(
                    app,
                    WebhookEventKind::SystemLowMemory,
                    0,
                    Some(WebhookReason::SystemMemoryLow { free_mb, total_mb }),
                    None,
                );
            }
        }
    }

    // Tempo de jogo e resumo.
    if status.summary {
        let entries = discord_status_entries(app, &config, now_ms);
        let due = with_discord_status(|state| {
            let elapsed = if state.last_play_ms == 0 { 0 } else { ((now_ms - state.last_play_ms).max(0) / 1_000) as u64 };
            state.last_play_ms = now_ms;
            let stats = state.stats.get_or_insert_with(load_discord_stats);
            stats.add_play(&entries, elapsed, now_ms);
            let today = local_day();
            let due = summary_due(&today, local_minute_of_day(), status.summary_minute, stats.last_summary_day.as_deref());
            if due && stats.is_empty() {
                // Nada para contar: só marca o dia.
                stats.reset_after_summary(&today, now_ms);
                save_discord_stats(stats);
                return None;
            }
            save_discord_stats(stats);
            due.then(|| (today, stats.clone()))
        });
        if let Some((today, stats)) = due {
            match push_discord_summary(app, client, &config, &status, &stats).await {
                Ok(()) => with_discord_stats(|s| {
                    s.reset_after_summary(&today, now_ms);
                    save_discord_stats(s);
                }),
                Err(e) => eprintln!("Discord summary: {e}"),
            }
        }
    }

    // Status ao vivo.
    if status.status {
        let due = with_discord_status(|state| now_ms - state.last_status_ms >= status.interval_min * 60_000);
        if due {
            with_discord_status(|state| state.last_status_ms = now_ms);
            if let Err(e) = push_discord_status(app, client, &config, &status).await {
                eprintln!("Discord status: {e}");
            }
        }
    }
}

pub(crate) fn start_discord_status_loop(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let client = match api::http_client::builder().build() {
            Ok(client) => client,
            Err(e) => {
                eprintln!("Discord status client failed to start: {e}");
                return;
            }
        };
        loop {
            tokio::time::sleep(DISCORD_STATUS_TICK).await;
            discord_status_tick(&app, &client).await;
        }
    });
}

fn discord_ready(app: &tauri::AppHandle) -> Result<WebhookConfig, String> {
    let config = webhook_config(app);
    if config.target.is_none() {
        return Err("Paste the webhook URL first.".to_string());
    }
    Ok(config)
}

/// "Send now" do status: manda (ou edita) já, sem esperar o intervalo.
#[tauri::command]
async fn send_discord_status_now(app: tauri::AppHandle) -> Result<(), String> {
    let config = discord_ready(&app)?;
    let status = discord_status_config(&app);
    let client = api::http_client::client();
    push_discord_status(&app, &client, &config, &status).await?;
    with_discord_status(|state| state.last_status_ms = chrono::Utc::now().timestamp_millis());
    Ok(())
}

/// "Send a preview" do resumo: manda o que somou até agora, sem zerar.
#[tauri::command]
async fn send_discord_summary_now(app: tauri::AppHandle) -> Result<(), String> {
    let config = discord_ready(&app)?;
    let status = discord_status_config(&app);
    let stats = with_discord_stats(|s| s.clone());
    let client = api::http_client::client();
    push_discord_summary(&app, &client, &config, &status, &stats).await
}
