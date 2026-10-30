//! Página Discord: o status ao vivo (uma mensagem só, editada a cada N
//! minutos) e o resumo diário de tempo de jogo. A parte pura e o envio.
//!
//! Ver docs/features/webhook.md. A cola com o resto do app (clientes abertos,
//! settings, a task de fundo) mora em `commands/discord_status.rs`. Usa o mesmo
//! webhook dos avisos (`discord_webhook`): as mesmas travas da URL, do token e
//! do texto livre valem aqui.
//!
//! - **Status ao vivo:** a primeira vez é um POST com `?wait=true` (o Discord
//!   devolve o id da mensagem); depois, PATCH nessa mensagem. Apagaram a
//!   mensagem (404)? Uma nova é criada.
//! - **Resumo:** o tempo em jogo de cada conta e de cada jogo, somado a cada
//!   passada do laço, mandado uma vez por dia na hora escolhida.

use crate::api::discord_webhook::{escape_markdown, SendOutcome, WebhookLang, WebhookTarget};
use std::collections::{BTreeMap, HashMap};

/// Linhas de conta no status: passou disso, a última resume o resto.
pub const MAX_STATUS_LINES: usize = 30;
/// Linhas de conta e de jogo no resumo.
pub const MAX_SUMMARY_ACCOUNTS: usize = 15;
pub const MAX_SUMMARY_GAMES: usize = 10;
/// Intervalo do status ao vivo, em minutos.
pub const STATUS_INTERVAL_DEFAULT: i64 = 5;
pub const STATUS_INTERVAL_MIN: i64 = 1;
pub const STATUS_INTERVAL_MAX: i64 = 60;
/// Hora do resumo quando a chave não existe.
pub const SUMMARY_TIME_DEFAULT: &str = "23:00";
/// Uma passada do laço que demorou mais que isto (PC suspenso, app travado)
/// não conta como tempo de jogo inteiro: só até aqui.
pub const MAX_TICK_SECS: u64 = 120;

/// Um cliente aberto, como o status mostra. `account` já vem mascarado (ou
/// não), como nos avisos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    pub user_id: i64,
    pub account: String,
    pub masked: bool,
    pub place_id: Option<i64>,
    /// Está num jogo agora (o log do cliente disse que entrou).
    pub in_game: bool,
    /// Desde quando está em jogo (ms), se está.
    pub in_game_since_ms: Option<i64>,
    /// Aberto pelo site e reconhecido pelo app.
    pub from_site: bool,
}

/// "1h 05m", "12m", "<1m".
pub fn format_duration(secs: u64, lang: WebhookLang) -> String {
    let minutes = secs / 60;
    if minutes == 0 {
        return "<1m".to_string();
    }
    let (h, m) = (minutes / 60, minutes % 60);
    let _ = lang;
    if h == 0 {
        format!("{m}m")
    } else {
        format!("{h}h {m:02}m")
    }
}

fn game_label(place: i64, game_names: &HashMap<i64, String>) -> String {
    let label = match game_names.get(&place).map(|g| g.trim()).filter(|g| !g.is_empty()) {
        Some(game) => escape_markdown(game),
        None => format!("Place {place}"),
    };
    format!("[{label}](https://www.roblox.com/games/{place})")
}

fn status_line(entry: &StatusEntry, lang: WebhookLang, game_names: &HashMap<i64, String>, now_ms: i64) -> String {
    let name = escape_markdown(&entry.account);
    let mut parts = vec![format!("**{name}**")];
    if entry.in_game {
        if let Some(place) = entry.place_id.filter(|p| *p > 0) {
            parts.push(game_label(place, game_names));
        }
        if let Some(since) = entry.in_game_since_ms {
            let secs = (now_ms - since).max(0) as u64 / 1_000;
            parts.push(format_duration(secs, lang));
        }
    } else {
        parts.push(
            match lang {
                WebhookLang::En => "not in a game",
                WebhookLang::Pt => "fora de jogo",
                WebhookLang::Es => "fuera de juego",
            }
            .to_string(),
        );
    }
    if entry.from_site {
        parts.push(
            match lang {
                WebhookLang::En => "opened from the website",
                WebhookLang::Pt => "aberta pelo site",
                WebhookLang::Es => "abierta desde la web",
            }
            .to_string(),
        );
    }
    let dot = if entry.in_game { "🟢" } else { "🟡" };
    format!("{dot} {}", parts.join(" · "))
}

/// O corpo do status ao vivo. Sem menção nunca: a mensagem é editada a cada
/// poucos minutos e não pode chamar ninguém.
pub fn build_status_message(
    entries: &[StatusEntry],
    lang: WebhookLang,
    game_names: &HashMap<i64, String>,
    now_ms: i64,
    interval_min: i64,
) -> serde_json::Value {
    let in_game = entries.iter().filter(|e| e.in_game).count();
    let description = if entries.is_empty() {
        match lang {
            WebhookLang::En => "No Roblox client open.",
            WebhookLang::Pt => "Nenhum cliente do Roblox aberto.",
            WebhookLang::Es => "Ningún cliente de Roblox abierto.",
        }
        .to_string()
    } else {
        // Em jogo primeiro, depois pelo nome.
        let mut sorted: Vec<&StatusEntry> = entries.iter().collect();
        sorted.sort_by(|a, b| b.in_game.cmp(&a.in_game).then_with(|| a.account.cmp(&b.account)));
        let mut lines: Vec<String> = sorted
            .iter()
            .take(MAX_STATUS_LINES)
            .map(|e| status_line(e, lang, game_names, now_ms))
            .collect();
        if sorted.len() > MAX_STATUS_LINES {
            let rest = sorted.len() - MAX_STATUS_LINES;
            lines.push(match lang {
                WebhookLang::En => format!("+{rest} more"),
                WebhookLang::Pt => format!("+{rest} a mais"),
                WebhookLang::Es => format!("+{rest} más"),
            });
        }
        lines.join("\n")
    };
    let secs = now_ms.div_euclid(1_000);
    let footer = match lang {
        WebhookLang::En => format!("MultiAlt · updates every {interval_min} min"),
        WebhookLang::Pt => format!("MultiAlt · atualiza a cada {interval_min} min"),
        WebhookLang::Es => format!("MultiAlt · se actualiza cada {interval_min} min"),
    };
    let mut embed = serde_json::json!({
        "title": match lang {
            WebhookLang::En => "📊 Live status",
            WebhookLang::Pt => "📊 Status ao vivo",
            WebhookLang::Es => "📊 Estado en vivo",
        },
        "description": description,
        "color": if in_game > 0 { 0x2e_cc_71 } else { 0x95_a5_a6 },
        "fields": [
            {
                "name": match lang { WebhookLang::En => "In game", WebhookLang::Pt => "Em jogo", WebhookLang::Es => "En juego" },
                "value": in_game.to_string(),
                "inline": true,
            },
            {
                "name": match lang { WebhookLang::En => "Open", WebhookLang::Pt => "Abertas", WebhookLang::Es => "Abiertas" },
                "value": entries.len().to_string(),
                "inline": true,
            },
            {
                "name": match lang { WebhookLang::En => "Updated", WebhookLang::Pt => "Atualizado", WebhookLang::Es => "Actualizado" },
                "value": format!("<t:{secs}:R>"),
                "inline": true,
            },
        ],
        "footer": { "text": footer },
    });
    if let Some(ts) = chrono::DateTime::from_timestamp_millis(now_ms) {
        embed["timestamp"] = serde_json::Value::String(ts.to_rfc3339());
    }
    serde_json::json!({
        "username": "MultiAlt",
        "content": "",
        "allowed_mentions": { "parse": [] },
        "embeds": [embed],
    })
}

/// Tempo de jogo e quedas de uma conta no período do resumo.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AccountPlay {
    pub secs: u64,
    #[serde(default)]
    pub drops: u32,
}

/// O que o resumo diário soma. Mora em `RAMDiscordStats.json` para não zerar
/// quando o app fecha no meio do dia.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayStats {
    /// Desde quando soma (ms). Zero = ainda não começou.
    #[serde(default)]
    pub since_ms: i64,
    /// O dia (hora local, `AAAA-MM-DD`) do último resumo mandado.
    #[serde(default)]
    pub last_summary_day: Option<String>,
    #[serde(default)]
    pub accounts: BTreeMap<i64, AccountPlay>,
    #[serde(default)]
    pub games: BTreeMap<i64, u64>,
}

impl PlayStats {
    /// Soma `elapsed_secs` (com teto: PC suspenso não vira horas de jogo) a
    /// cada conta em jogo e ao jogo dela.
    pub fn add_play(&mut self, entries: &[StatusEntry], elapsed_secs: u64, now_ms: i64) {
        if self.since_ms == 0 {
            self.since_ms = now_ms;
        }
        let secs = elapsed_secs.min(MAX_TICK_SECS);
        if secs == 0 {
            return;
        }
        for entry in entries.iter().filter(|e| e.in_game) {
            self.accounts.entry(entry.user_id).or_default().secs += secs;
            if let Some(place) = entry.place_id.filter(|p| *p > 0) {
                *self.games.entry(place).or_default() += secs;
            }
        }
    }

    pub fn add_drop(&mut self, user_id: i64, now_ms: i64) {
        if self.since_ms == 0 {
            self.since_ms = now_ms;
        }
        self.accounts.entry(user_id).or_default().drops += 1;
    }

    pub fn is_empty(&self) -> bool {
        self.accounts.values().all(|a| a.secs == 0 && a.drops == 0)
    }

    /// Mandou o resumo: zera a soma e guarda o dia.
    pub fn reset_after_summary(&mut self, day: &str, now_ms: i64) {
        self.accounts.clear();
        self.games.clear();
        self.since_ms = now_ms;
        self.last_summary_day = Some(day.to_string());
    }
}

/// Já é hora do resumo de hoje? Depois da hora escolhida e só uma vez por dia.
/// Com o PC desligado na hora, sai na próxima vez que passar da hora.
pub fn summary_due(today: &str, now_minute: u32, summary_minute: u32, last_day: Option<&str>) -> bool {
    now_minute >= summary_minute && last_day != Some(today)
}

/// O corpo do resumo. `names` é o nome de cada conta (já mascarado ou não);
/// conta que saiu do app vira "Unknown account".
pub fn build_summary_message(
    stats: &PlayStats,
    names: &HashMap<i64, String>,
    masked: bool,
    game_names: &HashMap<i64, String>,
    lang: WebhookLang,
    mention: &str,
    now_ms: i64,
) -> serde_json::Value {
    let total: u64 = stats.accounts.values().map(|a| a.secs).sum();
    let drops: u32 = stats.accounts.values().map(|a| a.drops).sum();
    let unknown = match lang {
        WebhookLang::En => "Unknown account",
        WebhookLang::Pt => "Conta desconhecida",
        WebhookLang::Es => "Cuenta desconocida",
    };

    let mut accounts: Vec<(&i64, &AccountPlay)> = stats.accounts.iter().filter(|(_, a)| a.secs > 0 || a.drops > 0).collect();
    accounts.sort_by(|a, b| b.1.secs.cmp(&a.1.secs).then_with(|| a.0.cmp(b.0)));
    let mut account_lines: Vec<String> = accounts
        .iter()
        .take(MAX_SUMMARY_ACCOUNTS)
        .map(|(id, play)| {
            let name = escape_markdown(names.get(id).map(String::as_str).unwrap_or(unknown));
            let name = if masked { format!("**{name}**") } else { format!("**[{name}](https://www.roblox.com/users/{id}/profile)**") };
            let mut line = format!("{name} — {}", format_duration(play.secs, lang));
            if play.drops > 0 {
                line.push_str(&match lang {
                    WebhookLang::En => format!(" · {} drop(s)", play.drops),
                    WebhookLang::Pt => format!(" · {} queda(s)", play.drops),
                    WebhookLang::Es => format!(" · {} caída(s)", play.drops),
                });
            }
            line
        })
        .collect();
    if accounts.len() > MAX_SUMMARY_ACCOUNTS {
        account_lines.push(format!("+{}", accounts.len() - MAX_SUMMARY_ACCOUNTS));
    }

    let mut games: Vec<(&i64, &u64)> = stats.games.iter().filter(|(_, s)| **s > 0).collect();
    games.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    let game_lines: Vec<String> = games
        .iter()
        .take(MAX_SUMMARY_GAMES)
        .map(|(place, secs)| format!("{} — {}", game_label(**place, game_names), format_duration(**secs, lang)))
        .collect();

    let none = match lang {
        WebhookLang::En => "No play time.",
        WebhookLang::Pt => "Nenhum tempo de jogo.",
        WebhookLang::Es => "Sin tiempo de juego.",
    };
    let since = stats.since_ms.div_euclid(1_000);
    let mut fields = vec![
        serde_json::json!({
            "name": match lang { WebhookLang::En => "Total play time", WebhookLang::Pt => "Tempo total em jogo", WebhookLang::Es => "Tiempo total en juego" },
            "value": format_duration(total, lang),
            "inline": true,
        }),
        serde_json::json!({
            "name": match lang { WebhookLang::En => "Drops", WebhookLang::Pt => "Quedas", WebhookLang::Es => "Caídas" },
            "value": drops.to_string(),
            "inline": true,
        }),
        serde_json::json!({
            "name": match lang { WebhookLang::En => "Accounts", WebhookLang::Pt => "Contas", WebhookLang::Es => "Cuentas" },
            "value": if account_lines.is_empty() { none.to_string() } else { account_lines.join("\n") },
            "inline": false,
        }),
    ];
    if !game_lines.is_empty() {
        fields.push(serde_json::json!({
            "name": match lang { WebhookLang::En => "Games", WebhookLang::Pt => "Jogos", WebhookLang::Es => "Juegos" },
            "value": game_lines.join("\n"),
            "inline": false,
        }));
    }
    let mut embed = serde_json::json!({
        "title": match lang {
            WebhookLang::En => "🗓️ Daily summary",
            WebhookLang::Pt => "🗓️ Resumo do dia",
            WebhookLang::Es => "🗓️ Resumen del día",
        },
        "description": match lang {
            WebhookLang::En => format!("Since <t:{since}:f>"),
            WebhookLang::Pt => format!("Desde <t:{since}:f>"),
            WebhookLang::Es => format!("Desde <t:{since}:f>"),
        },
        "color": 0x58_65_f2,
        "fields": fields,
        "footer": { "text": "MultiAlt" },
    });
    if let Some(ts) = chrono::DateTime::from_timestamp_millis(now_ms) {
        embed["timestamp"] = serde_json::Value::String(ts.to_rfc3339());
    }
    let mention: String = mention.trim().chars().take(1_000).collect();
    let allowed = if mention.is_empty() {
        serde_json::json!({ "parse": [] })
    } else {
        serde_json::json!({ "parse": ["everyone", "roles", "users"] })
    };
    serde_json::json!({
        "username": "MultiAlt",
        "content": mention,
        "allowed_mentions": allowed,
        "embeds": [embed],
    })
}

/// O id de uma mensagem do Discord (o que fica em `Webhook.StatusMessageId`):
/// só dígitos, para nada do arquivo de settings virar caminho da URL.
pub fn valid_message_id(id: &str) -> Option<&str> {
    let id = id.trim();
    ((15..=25).contains(&id.len()) && id.chars().all(|c| c.is_ascii_digit())).then_some(id)
}

/// Resultado de mandar ou editar o status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusOutcome {
    /// Mensagem nova: o id dela (para as próximas edições).
    Posted(String),
    Edited,
    /// A mensagem foi apagada no Discord: criar outra.
    Gone,
    RateLimited { retry_after_ms: i64 },
}

async fn retry_after_ms(response: reqwest::Response) -> i64 {
    let header_secs = response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<f64>().ok());
    let body_secs = response
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|v| v.get("retry_after").and_then(|r| r.as_f64()));
    (body_secs.or(header_secs).unwrap_or(2.0) * 1_000.0).ceil() as i64
}

fn status_error(target: &WebhookTarget, status: u16) -> String {
    target.redact(&match status {
        401 | 403 | 404 => format!("Discord answered {status}: the webhook was deleted or the URL is wrong."),
        code => format!("Discord answered {code}."),
    })
}

/// Manda (sem `message_id`) ou edita a mensagem do status. Erro volta sem a
/// URL e sem o token.
pub async fn send_status(
    client: &reqwest::Client,
    target: &WebhookTarget,
    message_id: Option<&str>,
    body: &serde_json::Value,
) -> Result<StatusOutcome, String> {
    let unreachable = |e: reqwest::Error| target.redact(&format!("Could not reach Discord: {}", e.without_url()));
    match message_id.and_then(valid_message_id) {
        Some(id) => {
            let response = client
                .patch(format!("{}/messages/{id}", target.post_url()))
                .json(body)
                .send()
                .await
                .map_err(unreachable)?;
            match response.status().as_u16() {
                200..=299 => Ok(StatusOutcome::Edited),
                // A mensagem sumiu (apagada no canal); o webhook ainda existe.
                404 => Ok(StatusOutcome::Gone),
                429 => Ok(StatusOutcome::RateLimited { retry_after_ms: retry_after_ms(response).await }),
                code => Err(status_error(target, code)),
            }
        }
        None => {
            let response = client
                .post(format!("{}?wait=true", target.post_url()))
                .json(body)
                .send()
                .await
                .map_err(unreachable)?;
            match response.status().as_u16() {
                200..=299 => {
                    let id = response
                        .json::<serde_json::Value>()
                        .await
                        .ok()
                        .and_then(|v| v.get("id").and_then(|i| i.as_str()).map(String::from))
                        .filter(|id| valid_message_id(id).is_some())
                        .ok_or_else(|| "Discord did not say which message it created.".to_string())?;
                    Ok(StatusOutcome::Posted(id))
                }
                429 => Ok(StatusOutcome::RateLimited { retry_after_ms: retry_after_ms(response).await }),
                code => Err(status_error(target, code)),
            }
        }
    }
}

/// O resumo é uma mensagem comum (POST): mesmo envio dos avisos.
pub async fn send_summary(
    client: &reqwest::Client,
    target: &WebhookTarget,
    body: &serde_json::Value,
) -> Result<SendOutcome, String> {
    crate::api::discord_webhook::send(client, target, body).await
}

/// Aviso "Windows quase sem memória": dispara quando a memória livre cai
/// abaixo de 10% e só volta a disparar depois de passar de 15% (para não
/// repetir a cada passada enquanto fica na beira).
/// Devolve (avisar agora, armado depois desta passada).
pub fn system_memory_step(armed: bool, free_mb: u64, total_mb: u64) -> (bool, bool) {
    if total_mb == 0 {
        return (false, armed);
    }
    let low = free_mb * 100 < total_mb * 10;
    let recovered = free_mb * 100 > total_mb * 15;
    match (armed, low, recovered) {
        (true, true, _) => (true, false),
        (false, _, true) => (false, true),
        _ => (false, armed),
    }
}

#[cfg(test)]
mod discord_status_tests {
    use super::*;

    fn entry(user_id: i64, name: &str, in_game: bool, place: Option<i64>, since: Option<i64>) -> StatusEntry {
        StatusEntry {
            user_id,
            account: name.into(),
            masked: false,
            place_id: place,
            in_game,
            in_game_since_ms: since,
            from_site: false,
        }
    }

    const NOW: i64 = 1_760_000_000_000;

    #[test]
    fn durations_read_like_a_clock() {
        assert_eq!(format_duration(30, WebhookLang::En), "<1m");
        assert_eq!(format_duration(12 * 60, WebhookLang::En), "12m");
        assert_eq!(format_duration(3_900, WebhookLang::En), "1h 05m");
    }

    #[test]
    fn the_status_lists_each_open_account_in_game_first_and_never_pings() {
        let mut games = HashMap::new();
        games.insert(42, "Huss Valley".to_string());
        let entries = [
            entry(1, "Zed", false, None, None),
            entry(2, "Alpha", true, Some(42), Some(NOW - 3_900_000)),
        ];
        let body = build_status_message(&entries, WebhookLang::Pt, &games, NOW, 5);
        let card = &body["embeds"][0];
        assert_eq!(card["title"], "📊 Status ao vivo");
        let lines: Vec<&str> = card["description"].as_str().unwrap().lines().collect();
        assert_eq!(lines[0], "🟢 **Alpha** · [Huss Valley](https://www.roblox.com/games/42) · 1h 05m");
        assert_eq!(lines[1], "🟡 **Zed** · fora de jogo");
        assert_eq!(card["fields"][0]["value"], "1");
        assert_eq!(card["fields"][1]["value"], "2");
        assert_eq!(card["footer"]["text"], "MultiAlt · atualiza a cada 5 min");
        assert_eq!(body["content"], "");
        assert_eq!(body["allowed_mentions"]["parse"], serde_json::json!([]));
    }

    #[test]
    fn the_status_says_when_nothing_is_open_and_caps_long_lists() {
        let empty = build_status_message(&[], WebhookLang::En, &HashMap::new(), NOW, 5);
        assert_eq!(empty["embeds"][0]["description"], "No Roblox client open.");
        let many: Vec<StatusEntry> = (0..40).map(|i| entry(i, &format!("acc{i:02}"), true, None, None)).collect();
        let body = build_status_message(&many, WebhookLang::En, &HashMap::new(), NOW, 5);
        let text = body["embeds"][0]["description"].as_str().unwrap();
        assert_eq!(text.lines().count(), MAX_STATUS_LINES + 1);
        assert!(text.ends_with("+10 more"));
    }

    #[test]
    fn a_name_in_the_status_cannot_ping_or_format() {
        let body = build_status_message(&[entry(1, "@everyone **x**", true, None, None)], WebhookLang::En, &HashMap::new(), NOW, 5);
        let text = body.to_string();
        assert!(!text.contains("@everyone"), "{text}");
        assert!(!text.contains("**x**"), "{text}");
    }

    #[test]
    fn play_time_counts_only_accounts_in_game_and_caps_a_long_gap() {
        let mut stats = PlayStats::default();
        let entries = [entry(1, "A", true, Some(42), None), entry(2, "B", false, None, None)];
        stats.add_play(&entries, 60, NOW);
        stats.add_play(&entries, 60, NOW + 60_000);
        assert_eq!(stats.since_ms, NOW);
        assert_eq!(stats.accounts[&1].secs, 120);
        assert!(!stats.accounts.contains_key(&2));
        assert_eq!(stats.games[&42], 120);
        // O PC dormiu 8 horas: não vira 8 horas de jogo.
        stats.add_play(&entries, 8 * 3_600, NOW + 120_000);
        assert_eq!(stats.accounts[&1].secs, 120 + MAX_TICK_SECS);
    }

    #[test]
    fn the_summary_goes_once_a_day_after_the_chosen_time() {
        let at = 23 * 60;
        assert!(!summary_due("2026-10-27", 22 * 60, at, None));
        assert!(summary_due("2026-10-27", 23 * 60, at, None));
        assert!(!summary_due("2026-10-27", 23 * 60 + 30, at, Some("2026-10-27")));
        assert!(summary_due("2026-10-28", 23 * 60 + 1, at, Some("2026-10-27")));

        let mut stats = PlayStats::default();
        stats.add_drop(5, NOW);
        assert!(!stats.is_empty());
        stats.reset_after_summary("2026-10-27", NOW + 1);
        assert!(stats.is_empty());
        assert_eq!(stats.since_ms, NOW + 1);
        assert_eq!(stats.last_summary_day.as_deref(), Some("2026-10-27"));
    }

    #[test]
    fn the_summary_ranks_accounts_and_games_by_play_time() {
        let mut stats = PlayStats { since_ms: NOW - 86_400_000, ..Default::default() };
        stats.accounts.insert(1, AccountPlay { secs: 600, drops: 0 });
        stats.accounts.insert(2, AccountPlay { secs: 7_200, drops: 2 });
        stats.accounts.insert(3, AccountPlay { secs: 60, drops: 0 });
        stats.games.insert(42, 7_800);
        let mut names = HashMap::new();
        names.insert(1, "Small".to_string());
        names.insert(2, "Big".to_string());
        let mut games = HashMap::new();
        games.insert(42, "Huss Valley".to_string());
        let body = build_summary_message(&stats, &names, false, &games, WebhookLang::En, "", NOW);
        let card = &body["embeds"][0];
        assert_eq!(card["title"], "🗓️ Daily summary");
        assert_eq!(card["fields"][0]["value"], "2h 11m");
        assert_eq!(card["fields"][1]["value"], "2");
        let accounts: Vec<&str> = card["fields"][2]["value"].as_str().unwrap().lines().collect();
        assert_eq!(accounts[0], "**[Big](https://www.roblox.com/users/2/profile)** — 2h 00m · 2 drop(s)");
        assert_eq!(accounts[1], "**[Small](https://www.roblox.com/users/1/profile)** — 10m");
        assert_eq!(accounts[2], "**[Unknown account](https://www.roblox.com/users/3/profile)** — 1m");
        assert_eq!(card["fields"][3]["value"], "[Huss Valley](https://www.roblox.com/games/42) — 2h 10m");
        assert_eq!(body["allowed_mentions"]["parse"], serde_json::json!([]));

        // Nomes ocultos: sem link do perfil.
        let masked = build_summary_message(&stats, &names, true, &games, WebhookLang::En, "<@&1>", NOW);
        assert!(!masked.to_string().contains("/profile"));
        assert_eq!(masked["content"], "<@&1>");
    }

    #[test]
    fn the_stats_file_survives_a_round_trip() {
        let mut stats = PlayStats { since_ms: NOW, last_summary_day: Some("2026-10-27".into()), ..Default::default() };
        stats.accounts.insert(12345678901, AccountPlay { secs: 90, drops: 1 });
        stats.games.insert(42, 90);
        let text = serde_json::to_string(&stats).unwrap();
        assert_eq!(serde_json::from_str::<PlayStats>(&text).unwrap(), stats);
        // Arquivo velho ou pela metade: começa do zero em vez de falhar.
        assert_eq!(serde_json::from_str::<PlayStats>("{}").unwrap(), PlayStats::default());
    }

    #[test]
    fn only_a_discord_message_id_is_used_in_the_url() {
        assert_eq!(valid_message_id(" 1234567890123456789 "), Some("1234567890123456789"));
        assert_eq!(valid_message_id("123/../../x"), None);
        assert_eq!(valid_message_id(""), None);
    }

    #[test]
    fn low_windows_memory_warns_once_until_it_recovers() {
        // Armado, caiu abaixo de 10%: avisa e desarma.
        assert_eq!(system_memory_step(true, 900, 10_000), (true, false));
        // Continua baixo: não repete.
        assert_eq!(system_memory_step(false, 800, 10_000), (false, false));
        // Entre 10% e 15%: continua desarmado.
        assert_eq!(system_memory_step(false, 1_200, 10_000), (false, false));
        // Passou de 15%: rearma.
        assert_eq!(system_memory_step(false, 1_600, 10_000), (false, true));
        assert_eq!(system_memory_step(true, 5_000, 10_000), (false, true));
    }
}

#[cfg(test)]
mod discord_status_http_tests {
    use super::*;
    use crate::api::discord_webhook::parse_webhook_url;
    use crate::api::endpoints::test_support::{mock_path, mock_server};
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, ResponseTemplate};

    const TOKEN: &str = "SeCrEtToken_abcdefghijklmnopqrstuvwxyz";

    fn target(id: &str) -> WebhookTarget {
        parse_webhook_url(&format!("https://discord.com/api/webhooks/{id}/{TOKEN}")).unwrap()
    }

    fn client() -> reqwest::Client {
        crate::api::http_client::builder().build().unwrap()
    }

    fn body() -> serde_json::Value {
        build_status_message(&[], WebhookLang::En, &HashMap::new(), 0, 5)
    }

    #[tokio::test]
    async fn the_first_status_is_posted_and_returns_the_message_id() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("discord", &format!("/api/webhooks/200000000000000001/{TOKEN}"))))
            .and(query_param("wait", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "id": "1300000000000000001" })))
            .expect(1)
            .mount(server)
            .await;
        let outcome = send_status(&client(), &target("200000000000000001"), None, &body()).await;
        assert_eq!(outcome, Ok(StatusOutcome::Posted("1300000000000000001".into())));
    }

    #[tokio::test]
    async fn later_statuses_edit_the_same_message_and_a_deleted_one_is_gone() {
        let server = mock_server().await;
        Mock::given(method("PATCH"))
            .and(path(mock_path(
                "discord",
                &format!("/api/webhooks/200000000000000002/{TOKEN}/messages/1300000000000000002"),
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "id": "1300000000000000002" })))
            .expect(1)
            .mount(server)
            .await;
        Mock::given(method("PATCH"))
            .and(path(mock_path(
                "discord",
                &format!("/api/webhooks/200000000000000002/{TOKEN}/messages/1300000000000000009"),
            )))
            .respond_with(ResponseTemplate::new(404))
            .mount(server)
            .await;
        let t = target("200000000000000002");
        assert_eq!(send_status(&client(), &t, Some("1300000000000000002"), &body()).await, Ok(StatusOutcome::Edited));
        assert_eq!(send_status(&client(), &t, Some("1300000000000000009"), &body()).await, Ok(StatusOutcome::Gone));
    }

    #[tokio::test]
    async fn a_status_error_never_carries_the_token() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("discord", &format!("/api/webhooks/200000000000000003/{TOKEN}"))))
            .respond_with(ResponseTemplate::new(401))
            .mount(server)
            .await;
        let err = send_status(&client(), &target("200000000000000003"), None, &body()).await.unwrap_err();
        assert!(err.contains("401"), "{err}");
        assert!(!err.contains(TOKEN), "{err}");
    }
}
