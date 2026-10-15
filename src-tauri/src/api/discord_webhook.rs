//! Aviso no Discord por webhook (ideia 5): a parte pura e o envio.
//!
//! Ver docs/features/webhook.md. A cola com o resto do app (settings, contas,
//! pontos onde os eventos nascem, a fila em segundo plano) mora em
//! `commands/webhook.rs`.
//!
//! - **A URL é segredo.** Quem a tem posta no canal. Nunca vai para log nem
//!   para mensagem de erro: o erro do reqwest sai sem a URL (`without_url`) e
//!   o token é trocado por `***` em qualquer texto que volte (`redact`).
//! - **Só Discord.** A URL colada é desmontada em (id, token) e o envio vai
//!   sempre para `https://discord.com/api/webhooks/<id>/<token>`, montado aqui
//!   (`endpoints::external_host("discord")`, que os testes apontam para o mock).
//!   Nada do que o usuário digitou vira host.
//! - **Fila com teto e lote.** O Discord aceita ~5 requisições a cada 2 s por
//!   webhook. Várias quedas em poucos segundos viram **uma** mensagem
//!   (`BATCH_WINDOW_MS`), e a fila respeita o limite e o `retry_after` do 429.

use crate::api::endpoints;
use std::collections::VecDeque;

/// Espera depois do primeiro evento antes de mandar, para juntar os vizinhos.
pub const BATCH_WINDOW_MS: i64 = 4_000;
/// Linhas por mensagem: mais que isto vai na mensagem seguinte.
pub const MAX_BATCH: usize = 10;
/// Teto da fila. Passou disso (Discord fora do ar por muito tempo), o mais
/// antigo sai — aviso velho não vale mais nada.
pub const MAX_QUEUE: usize = 50;
/// Aviso que esperou mais que isto na fila é descartado.
pub const MAX_EVENT_AGE_MS: i64 = 10 * 60_000;
/// Limite do Discord por webhook: 5 requisições a cada 2 s.
pub const RATE_LIMIT_REQUESTS: usize = 5;
pub const RATE_LIMIT_WINDOW_MS: i64 = 2_000;
/// Texto livre (motivo do kick, nome) corta aqui.
const MAX_TEXT_CHARS: usize = 200;
/// Texto antes da mensagem (menção), corta aqui.
pub const MAX_MENTION_CHARS: usize = 100;
/// O Discord aceita 4096 na descrição do embed; folga para o resto.
const MAX_DESCRIPTION_CHARS: usize = 3_900;

/// Hosts que o Discord usa nos links de webhook.
const DISCORD_HOSTS: &[&str] = &[
    "discord.com",
    "discordapp.com",
    "ptb.discord.com",
    "canary.discord.com",
    "ptb.discordapp.com",
    "canary.discordapp.com",
];

/// A frase (inglês; a tela traduz) para qualquer URL recusada.
pub const INVALID_URL_MESSAGE: &str =
    "That is not a Discord webhook URL. Copy it from Discord: Server Settings › Integrations › Webhooks › Copy Webhook URL.";

/// Um webhook do Discord, já desmontado. O envio nunca usa a URL colada.
#[derive(Clone, PartialEq, Eq)]
pub struct WebhookTarget {
    id: String,
    token: String,
}

// O token nunca aparece num `{:?}` (log de depuração, panic).
impl std::fmt::Debug for WebhookTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebhookTarget").field("id", &self.id).field("token", &"***").finish()
    }
}

impl WebhookTarget {
    /// Para onde a mensagem vai: sempre o host do Discord (ou o mock nos testes).
    pub fn post_url(&self) -> String {
        format!(
            "{}/api/webhooks/{}/{}",
            endpoints::external_host("discord"),
            self.id,
            self.token
        )
    }

    /// Tira o token de qualquer texto que vá para log ou para a tela.
    pub fn redact(&self, text: &str) -> String {
        if self.token.is_empty() {
            return text.to_string();
        }
        text.replace(&self.token, "***")
    }
}

/// `https://discord.com/api/webhooks/<id>/<token>` (e `discordapp.com`,
/// `ptb.`, `canary.`, `/api/v10/webhooks/...`) → (id, token). Todo o resto é
/// recusado: outro esquema, outro host, porta, usuário/senha, query, fragmento.
pub fn parse_webhook_url(url: &str) -> Result<WebhookTarget, String> {
    let invalid = || INVALID_URL_MESSAGE.to_string();
    let parsed = reqwest::Url::parse(url.trim()).map_err(|_| invalid())?;
    if parsed.scheme() != "https"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(invalid());
    }
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    if !DISCORD_HOSTS.contains(&host.as_str()) {
        return Err(invalid());
    }
    let mut segments: Vec<&str> = parsed.path_segments().map(|s| s.collect()).unwrap_or_default();
    if segments.last() == Some(&"") {
        segments.pop();
    }
    let rest: &[&str] = match segments.as_slice() {
        ["api", "webhooks", rest @ ..] => rest,
        ["api", version, "webhooks", rest @ ..]
            if version.len() > 1 && version.starts_with('v') && version[1..].chars().all(|c| c.is_ascii_digit()) =>
        {
            rest
        }
        _ => return Err(invalid()),
    };
    let [id, token] = rest else {
        return Err(invalid());
    };
    let id_ok = (15..=25).contains(&id.len()) && id.chars().all(|c| c.is_ascii_digit());
    let token_ok = (20..=200).contains(&token.len())
        && token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !id_ok || !token_ok {
        return Err(invalid());
    }
    Ok(WebhookTarget {
        id: id.to_string(),
        token: token.to_string(),
    })
}

/// `update_setting` passa por aqui: `Webhook.Url` só grava vazio ou um
/// webhook do Discord válido (a tela já confere; isto é a segunda trava).
pub fn validate_setting(section: &str, key: &str, value: &str) -> Result<(), String> {
    if section == "Webhook" && key == "Url" && !value.trim().is_empty() {
        parse_webhook_url(value)?;
    }
    if section == "Webhook" && key == "Mention" && value.chars().count() > MAX_MENTION_CHARS {
        return Err(format!("The text before messages is too long (max {MAX_MENTION_CHARS} characters)."));
    }
    Ok(())
}

/// O que pode virar aviso. Cada um tem a sua caixa na tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WebhookEventKind {
    /// A conta caiu / o cliente fechou sem sair do jogo.
    Dropped,
    /// A reconexão automática devolveu a conta ao jogo.
    Reconnected,
    /// A reconexão desistiu (ou não pôde continuar: banida, sessão expirada…).
    ReconnectFailed,
    /// A janela ficou "Não respondendo" por 30 s.
    NotResponding,
    /// O launch de uma conta falhou.
    LaunchFailed,
}

impl WebhookEventKind {
    fn title(self) -> &'static str {
        match self {
            Self::Dropped => "Dropped",
            Self::Reconnected => "Reconnected",
            Self::ReconnectFailed => "Reconnect failed",
            Self::NotResponding => "Not responding",
            Self::LaunchFailed => "Launch failed",
        }
    }

    /// Gravidade, para a cor da mensagem: a pior linha decide.
    fn severity(self) -> u8 {
        match self {
            Self::Reconnected => 0,
            Self::NotResponding => 1,
            Self::LaunchFailed => 2,
            Self::Dropped | Self::ReconnectFailed => 3,
        }
    }
}

/// Um aviso pronto para a fila. `account` já vem mascarado (ou não) conforme
/// a opção — a fila nunca vê o nome real com "Hide account names" ligado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookEvent {
    pub kind: WebhookEventKind,
    pub account: String,
    pub reason: Option<String>,
    pub place_id: Option<i64>,
    pub at_ms: i64,
}

/// As opções da seção `[Webhook]` do RAMSettings.ini.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookConfig {
    pub enabled: bool,
    pub target: Option<WebhookTarget>,
    pub dropped: bool,
    pub reconnected: bool,
    pub reconnect_failed: bool,
    pub not_responding: bool,
    pub launch_failed: bool,
    pub mention: String,
    pub hide_names: bool,
}

impl WebhookConfig {
    /// Lê as chaves (`get("Enabled")` …). Chave ausente = o padrão documentado.
    pub fn from_settings(get: impl Fn(&str) -> Option<String>) -> Self {
        let flag = |key: &str, default: bool| match get(key).as_deref().map(str::trim) {
            Some("true") => true,
            Some("false") => false,
            _ => default,
        };
        let url = get("Url").unwrap_or_default();
        Self {
            enabled: flag("Enabled", false),
            target: if url.trim().is_empty() { None } else { parse_webhook_url(&url).ok() },
            dropped: flag("NotifyDropped", true),
            reconnected: flag("NotifyReconnected", false),
            reconnect_failed: flag("NotifyReconnectFailed", true),
            not_responding: flag("NotifyNotResponding", true),
            launch_failed: flag("NotifyLaunchFailed", false),
            mention: get("Mention")
                .unwrap_or_default()
                .trim()
                .chars()
                .take(MAX_MENTION_CHARS)
                .collect(),
            hide_names: flag("HideNames", true),
        }
    }

    /// Este aviso sai? Só com a opção ligada, uma URL válida e a caixa marcada.
    pub fn wants(&self, kind: WebhookEventKind) -> bool {
        if !self.enabled || self.target.is_none() {
            return false;
        }
        match kind {
            WebhookEventKind::Dropped => self.dropped,
            WebhookEventKind::Reconnected => self.reconnected,
            WebhookEventKind::ReconnectFailed => self.reconnect_failed,
            WebhookEventKind::NotResponding => self.not_responding,
            WebhookEventKind::LaunchFailed => self.launch_failed,
        }
    }
}

fn truncate_chars(text: &str, max: usize) -> String {
    let mut out: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        out.push('…');
    }
    out
}

/// Texto livre (nome, motivo do kick) vira texto puro no Discord: sem
/// negrito/link/menção acidental. Quebra de linha vira espaço.
pub fn escape_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in truncate_chars(text.trim(), MAX_TEXT_CHARS).chars() {
        match c {
            '\\' | '*' | '_' | '~' | '`' | '|' | '>' | '#' | '[' | ']' | '(' | ')' | '<' | ':' | '-' => {
                out.push('\\');
                out.push(c);
            }
            '@' => out.push_str("@\u{200B}"),
            '\n' | '\r' | '\t' => out.push(' '),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Uma linha da mensagem: `**conta** — Dropped: motivo · Place 123 · <t:…:T>`.
pub fn format_event_line(event: &WebhookEvent) -> String {
    let mut line = format!("**{}** — {}", escape_markdown(&event.account), event.kind.title());
    if let Some(reason) = event.reason.as_deref().filter(|r| !r.trim().is_empty()) {
        line.push_str(": ");
        line.push_str(&escape_markdown(reason));
    }
    if let Some(place) = event.place_id.filter(|p| *p > 0) {
        line.push_str(&format!(" · Place {place}"));
    }
    // O Discord mostra `<t:unix:T>` na hora local de quem lê.
    line.push_str(&format!(" · <t:{}:T>", event.at_ms.div_euclid(1_000)));
    line
}

/// O corpo do POST. Uma mensagem com um embed; a menção (texto antes) vai no
/// `content` e é a **única** coisa que pode mencionar alguém.
pub fn build_message(events: &[WebhookEvent], mention: &str) -> serde_json::Value {
    let title = match events {
        [single] => single.kind.title().to_string(),
        many => format!("{} updates", many.len()),
    };
    let mut description = String::new();
    for event in events {
        let line = format_event_line(event);
        if description.chars().count() + line.chars().count() + 1 > MAX_DESCRIPTION_CHARS {
            description.push_str("\n…");
            break;
        }
        if !description.is_empty() {
            description.push('\n');
        }
        description.push_str(&line);
    }
    let color = match events.iter().map(|e| e.kind.severity()).max().unwrap_or(0) {
        0 => 0x2e_cc_71, // verde
        1 => 0xf1_c4_0f, // amarelo
        2 => 0xe6_7e_22, // laranja
        _ => 0xe7_4c_3c, // vermelho
    };
    let mention: String = mention.trim().chars().take(MAX_MENTION_CHARS).collect();
    let allowed = if mention.is_empty() {
        serde_json::json!({ "parse": [] })
    } else {
        serde_json::json!({ "parse": ["everyone", "roles", "users"] })
    };
    serde_json::json!({
        "username": "MultiAlt",
        "content": mention,
        "allowed_mentions": allowed,
        "embeds": [{
            "title": title,
            "description": description,
            "color": color,
            "footer": { "text": "MultiAlt" },
        }],
    })
}

/// A fila: junta avisos próximos e respeita o limite do Discord. Pura (o
/// relógio entra por parâmetro), testada sem rede.
#[derive(Debug, Default)]
pub struct WebhookQueue {
    /// (quando entrou, aviso)
    pending: VecDeque<(i64, WebhookEvent)>,
    /// Quando saíram os últimos envios (janela do limite).
    sent_at: VecDeque<i64>,
    /// 429 do Discord: nada sai antes disto.
    blocked_until: i64,
    /// Quantos avisos saíram da fila sem ir (teto, velhice).
    pub discarded: usize,
}

impl WebhookQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn push(&mut self, event: WebhookEvent, now_ms: i64) {
        self.pending.push_back((now_ms, event));
        while self.pending.len() > MAX_QUEUE {
            self.pending.pop_front();
            self.discarded += 1;
        }
    }

    /// Cabe mais um envio agora? (≤ 5 nos últimos 2 s e sem 429 valendo.)
    pub fn can_send(&mut self, now_ms: i64) -> bool {
        if now_ms < self.blocked_until {
            return false;
        }
        while self.sent_at.front().is_some_and(|t| now_ms - *t >= RATE_LIMIT_WINDOW_MS) {
            self.sent_at.pop_front();
        }
        self.sent_at.len() < RATE_LIMIT_REQUESTS
    }

    /// O próximo lote, se já é hora: o primeiro aviso esperou a janela (ou o
    /// lote encheu) e o limite deixa. Marca o envio.
    pub fn take_batch(&mut self, now_ms: i64) -> Option<Vec<WebhookEvent>> {
        let before = self.pending.len();
        self.pending.retain(|(queued, _)| now_ms - *queued <= MAX_EVENT_AGE_MS);
        self.discarded += before - self.pending.len();

        let (first_at, _) = self.pending.front()?;
        let ready = self.pending.len() >= MAX_BATCH || now_ms - *first_at >= BATCH_WINDOW_MS;
        if !ready || !self.can_send(now_ms) {
            return None;
        }
        let n = self.pending.len().min(MAX_BATCH);
        let batch: Vec<WebhookEvent> = self.pending.drain(..n).map(|(_, e)| e).collect();
        self.sent_at.push_back(now_ms);
        Some(batch)
    }

    /// O Discord respondeu 429: o lote volta para a frente da fila e nada sai
    /// antes de `retry_after`.
    pub fn rate_limited(&mut self, batch: Vec<WebhookEvent>, now_ms: i64, retry_after_ms: i64) {
        self.blocked_until = now_ms + retry_after_ms.clamp(500, 60_000);
        for event in batch.into_iter().rev() {
            // Volta com "entrou agora menos a janela": sai assim que puder.
            self.pending.push_front((now_ms - BATCH_WINDOW_MS, event));
        }
        while self.pending.len() > MAX_QUEUE {
            self.pending.pop_back();
            self.discarded += 1;
        }
    }
}

/// Resultado de um POST que o Discord aceitou ou mandou esperar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendOutcome {
    Sent,
    RateLimited { retry_after_ms: i64 },
}

/// Manda uma mensagem. Erro volta **sem** a URL e sem o token.
pub async fn send(client: &reqwest::Client, target: &WebhookTarget, body: &serde_json::Value) -> Result<SendOutcome, String> {
    let response = client
        .post(target.post_url())
        .json(body)
        .send()
        .await
        .map_err(|e| target.redact(&format!("Could not reach Discord: {}", e.without_url())))?;
    let status = response.status();
    if status.is_success() {
        return Ok(SendOutcome::Sent);
    }
    if status.as_u16() == 429 {
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
        let secs = body_secs.or(header_secs).unwrap_or(2.0);
        return Ok(SendOutcome::RateLimited {
            retry_after_ms: (secs * 1_000.0).ceil() as i64,
        });
    }
    Err(match status.as_u16() {
        401 | 403 | 404 => format!(
            "Discord answered {}: the webhook was deleted or the URL is wrong.",
            status.as_u16()
        ),
        code => format!("Discord answered {code}."),
    })
}

#[cfg(test)]
mod discord_webhook_url_tests {
    use super::*;

    const ID: &str = "123456789012345678";
    const TOKEN: &str = "AbCdEf_ghIJ-klmnopQRSTuvwxyz0123456789";

    fn url(prefix: &str) -> String {
        format!("{prefix}/{ID}/{TOKEN}")
    }

    #[test]
    fn accepts_every_discord_webhook_host() {
        for prefix in [
            "https://discord.com/api/webhooks",
            "https://discordapp.com/api/webhooks",
            "https://ptb.discord.com/api/webhooks",
            "https://canary.discord.com/api/webhooks",
            "https://ptb.discordapp.com/api/webhooks",
            "https://canary.discordapp.com/api/webhooks",
            "https://DISCORD.com/api/webhooks",
            "https://discord.com/api/v10/webhooks",
        ] {
            assert!(parse_webhook_url(&url(prefix)).is_ok(), "{prefix}");
        }
        assert!(parse_webhook_url(&format!("  {}/  ", url("https://discord.com/api/webhooks"))).is_ok());
    }

    #[test]
    fn refuses_anything_else() {
        for bad in [
            String::new(),
            "not a url".into(),
            url("http://discord.com/api/webhooks"),
            url("https://discord.com.evil.com/api/webhooks"),
            url("https://evil.com/api/webhooks"),
            url("https://discord.com:8443/api/webhooks"),
            url("https://user:pw@discord.com/api/webhooks"),
            url("https://discord.com/api/channels"),
            url("https://discord.com/webhooks"),
            format!("{}?x=1", url("https://discord.com/api/webhooks")),
            format!("{}#x", url("https://discord.com/api/webhooks")),
            format!("https://discord.com/api/webhooks/{ID}"),
            format!("https://discord.com/api/webhooks/{ID}/{TOKEN}/extra"),
            format!("https://discord.com/api/webhooks/abc/{TOKEN}"),
            format!("https://discord.com/api/webhooks/{ID}/short"),
            format!("https://discord.com/api/webhooks/{ID}/tok%20en{TOKEN}"),
        ] {
            assert_eq!(parse_webhook_url(&bad).unwrap_err(), INVALID_URL_MESSAGE, "{bad}");
        }
    }

    #[test]
    fn always_posts_to_discord_whatever_host_was_pasted() {
        let target = parse_webhook_url(&url("https://canary.discordapp.com/api/v10/webhooks")).unwrap();
        // Sem override nos testes desta suíte, o host é o do Discord (ou o mock,
        // se outro teste do binário já o ligou) — nunca o host colado.
        let post = target.post_url();
        assert!(post.ends_with(&format!("/api/webhooks/{ID}/{TOKEN}")), "{post}");
        assert!(!post.contains("canary"), "{post}");
    }

    #[test]
    fn the_token_never_shows_in_debug_or_redacted_text() {
        let target = parse_webhook_url(&url("https://discord.com/api/webhooks")).unwrap();
        assert!(!format!("{target:?}").contains(TOKEN));
        let text = format!("error for {}", target.post_url());
        assert!(!target.redact(&text).contains(TOKEN));
    }

    #[test]
    fn the_setting_only_stores_an_empty_or_valid_url() {
        assert!(validate_setting("Webhook", "Url", "").is_ok());
        assert!(validate_setting("Webhook", "Url", &url("https://discord.com/api/webhooks")).is_ok());
        assert!(validate_setting("Webhook", "Url", "https://example.com/x").is_err());
        assert!(validate_setting("Webhook", "Mention", &"x".repeat(MAX_MENTION_CHARS + 1)).is_err());
        // Outras chaves passam reto.
        assert!(validate_setting("General", "Url", "anything").is_ok());
    }
}

#[cfg(test)]
mod discord_webhook_message_tests {
    use super::*;

    fn settings(pairs: &[(&str, &str)]) -> WebhookConfig {
        let map: std::collections::HashMap<String, String> =
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        WebhookConfig::from_settings(|k| map.get(k).cloned())
    }

    const URL: &str = "https://discord.com/api/webhooks/123456789012345678/AbCdEf_ghIJ-klmnopQRSTuvwxyz0123456789";

    fn event(kind: WebhookEventKind, account: &str) -> WebhookEvent {
        WebhookEvent {
            kind,
            account: account.into(),
            reason: None,
            place_id: None,
            at_ms: 1_760_000_000_000,
        }
    }

    #[test]
    fn off_by_default_and_needs_a_valid_url() {
        let c = settings(&[]);
        assert!(!c.enabled);
        assert!(c.hide_names, "names are hidden by default");
        assert!(!c.wants(WebhookEventKind::Dropped));
        assert!(!settings(&[("Enabled", "true")]).wants(WebhookEventKind::Dropped), "no URL");
        assert!(!settings(&[("Enabled", "true"), ("Url", "https://evil.com/x")]).wants(WebhookEventKind::Dropped));
        assert!(settings(&[("Enabled", "true"), ("Url", URL)]).wants(WebhookEventKind::Dropped));
    }

    #[test]
    fn each_event_follows_its_own_box_with_the_documented_defaults() {
        let on = settings(&[("Enabled", "true"), ("Url", URL)]);
        assert!(on.wants(WebhookEventKind::Dropped));
        assert!(!on.wants(WebhookEventKind::Reconnected));
        assert!(on.wants(WebhookEventKind::ReconnectFailed));
        assert!(on.wants(WebhookEventKind::NotResponding));
        assert!(!on.wants(WebhookEventKind::LaunchFailed));

        let custom = settings(&[
            ("Enabled", "true"),
            ("Url", URL),
            ("NotifyDropped", "false"),
            ("NotifyReconnected", "true"),
            ("NotifyLaunchFailed", "true"),
        ]);
        assert!(!custom.wants(WebhookEventKind::Dropped));
        assert!(custom.wants(WebhookEventKind::Reconnected));
        assert!(custom.wants(WebhookEventKind::LaunchFailed));
    }

    #[test]
    fn a_line_has_the_account_the_event_the_reason_the_place_and_the_time() {
        let e = WebhookEvent {
            reason: Some("lost the connection (code 277)".into()),
            place_id: Some(920587237),
            ..event(WebhookEventKind::Dropped, "Farm 1")
        };
        assert_eq!(
            format_event_line(&e),
            "**Farm 1** — Dropped: lost the connection \\(code 277\\) · Place 920587237 · <t:1760000000:T>"
        );
    }

    #[test]
    fn free_text_cannot_format_link_or_ping() {
        let line = format_event_line(&WebhookEvent {
            reason: Some("@everyone **x** [a](http://x)\nnext".into()),
            ..event(WebhookEventKind::Dropped, "<@&123>")
        });
        assert!(!line.contains("@everyone"), "{line}");
        assert!(!line.contains("<@&"), "{line}");
        assert!(!line.contains("**x**"), "{line}");
        assert!(!line.contains('\n'), "{line}");
    }

    #[test]
    fn a_masked_name_goes_out_as_it_came() {
        // O nome chega mascarado da cola (commands/webhook.rs): a mensagem não
        // tem como saber o real.
        let body = build_message(&[event(WebhookEventKind::Dropped, "Fa********")], "");
        let text = body.to_string();
        assert!(text.contains("Fa\\\\*\\\\*"), "{text}");
    }

    #[test]
    fn several_events_become_one_message_and_only_the_prefix_can_mention() {
        let events = vec![
            event(WebhookEventKind::Dropped, "A"),
            event(WebhookEventKind::Reconnected, "B"),
        ];
        let body = build_message(&events, "");
        assert_eq!(body["embeds"][0]["title"], "2 updates");
        assert_eq!(body["embeds"][0]["description"].as_str().unwrap().lines().count(), 2);
        assert_eq!(body["allowed_mentions"]["parse"], serde_json::json!([]));
        assert_eq!(body["embeds"][0]["color"], 0xe7_4c_3c, "the worst line sets the color");

        let pinged = build_message(&events[..1], "<@&42> ");
        assert_eq!(pinged["content"], "<@&42>");
        assert_eq!(pinged["embeds"][0]["title"], "Dropped");
        assert_ne!(pinged["allowed_mentions"]["parse"], serde_json::json!([]));
    }

    #[test]
    fn a_huge_batch_stays_under_discords_limit() {
        let events: Vec<WebhookEvent> = (0..MAX_BATCH)
            .map(|_| WebhookEvent {
                reason: Some("x".repeat(500)),
                ..event(WebhookEventKind::Dropped, &"n".repeat(500))
            })
            .collect();
        let body = build_message(&events, "");
        assert!(body["embeds"][0]["description"].as_str().unwrap().chars().count() <= 4096);
    }
}

#[cfg(test)]
mod discord_webhook_queue_tests {
    use super::*;

    fn event(n: usize) -> WebhookEvent {
        WebhookEvent {
            kind: WebhookEventKind::Dropped,
            account: format!("acc{n}"),
            reason: None,
            place_id: None,
            at_ms: 0,
        }
    }

    #[test]
    fn drops_close_together_go_out_as_one_batch_after_the_window() {
        let mut q = WebhookQueue::new();
        q.push(event(1), 0);
        q.push(event(2), 1_000);
        q.push(event(3), 2_500);
        assert_eq!(q.take_batch(BATCH_WINDOW_MS - 1), None, "still waiting for neighbours");
        let batch = q.take_batch(BATCH_WINDOW_MS).unwrap();
        assert_eq!(batch.len(), 3);
        assert_eq!(q.len(), 0);
        assert_eq!(q.take_batch(BATCH_WINDOW_MS + 10_000), None);
    }

    #[test]
    fn a_full_batch_goes_at_once_and_the_rest_waits_for_the_next() {
        let mut q = WebhookQueue::new();
        for n in 0..MAX_BATCH + 3 {
            q.push(event(n), 0);
        }
        assert_eq!(q.take_batch(0).unwrap().len(), MAX_BATCH);
        assert_eq!(q.len(), 3);
    }

    #[test]
    fn never_more_than_five_sends_in_two_seconds() {
        let mut q = WebhookQueue::new();
        let mut sent = 0;
        for round in 0..RATE_LIMIT_REQUESTS + 2 {
            for n in 0..MAX_BATCH {
                q.push(event(round * MAX_BATCH + n), 0);
            }
            if q.take_batch(100).is_some() {
                sent += 1;
            }
        }
        assert_eq!(sent, RATE_LIMIT_REQUESTS);
        assert_eq!(q.len(), 2 * MAX_BATCH, "the rest waits in the queue");
        // Passou a janela: volta a sair.
        assert!(q.take_batch(100 + RATE_LIMIT_WINDOW_MS).is_some());
    }

    #[test]
    fn a_429_puts_the_batch_back_and_waits_retry_after() {
        let mut q = WebhookQueue::new();
        q.push(event(1), 0);
        let batch = q.take_batch(BATCH_WINDOW_MS).unwrap();
        q.rate_limited(batch, 5_000, 3_000);
        assert_eq!(q.len(), 1);
        assert_eq!(q.take_batch(7_999), None);
        assert_eq!(q.take_batch(8_000).unwrap()[0].account, "acc1");
    }

    #[test]
    fn the_queue_is_capped_and_old_events_expire() {
        let mut q = WebhookQueue::new();
        for n in 0..MAX_QUEUE + 5 {
            q.push(event(n), 0);
        }
        assert_eq!(q.len(), MAX_QUEUE);
        assert_eq!(q.discarded, 5);
        // Discord fora do ar por mais que o teto de idade: tudo sai sem ir.
        assert_eq!(q.take_batch(MAX_EVENT_AGE_MS + 1), None);
        assert_eq!(q.len(), 0);
    }
}

#[cfg(test)]
mod discord_webhook_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{mock_path, mock_server};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};

    fn target(id: &str) -> WebhookTarget {
        parse_webhook_url(&format!(
            "https://discord.com/api/webhooks/{id}/SeCrEtToken_abcdefghijklmnopqrstuvwxyz"
        ))
        .unwrap()
    }

    fn client() -> reqwest::Client {
        crate::api::http_client::builder().build().unwrap()
    }

    #[tokio::test]
    async fn posts_the_message_to_the_discord_path() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path(
                "discord",
                "/api/webhooks/100000000000000001/SeCrEtToken_abcdefghijklmnopqrstuvwxyz",
            )))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(server)
            .await;
        let body = build_message(&[], "");
        assert_eq!(send(&client(), &target("100000000000000001"), &body).await, Ok(SendOutcome::Sent));
    }

    #[tokio::test]
    async fn a_429_reports_how_long_to_wait() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path(
                "discord",
                "/api/webhooks/100000000000000002/SeCrEtToken_abcdefghijklmnopqrstuvwxyz",
            )))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({ "retry_after": 1.5 })))
            .mount(server)
            .await;
        let outcome = send(&client(), &target("100000000000000002"), &build_message(&[], "")).await;
        assert_eq!(outcome, Ok(SendOutcome::RateLimited { retry_after_ms: 1_500 }));
    }

    #[tokio::test]
    async fn an_error_never_carries_the_url_or_the_token() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path(
                "discord",
                "/api/webhooks/100000000000000003/SeCrEtToken_abcdefghijklmnopqrstuvwxyz",
            )))
            .respond_with(ResponseTemplate::new(404))
            .mount(server)
            .await;
        let err = send(&client(), &target("100000000000000003"), &build_message(&[], ""))
            .await
            .unwrap_err();
        assert!(err.contains("404"), "{err}");
        assert!(!err.contains("SeCrEtToken"), "{err}");
        assert!(!err.contains("webhooks"), "{err}");
    }
}
