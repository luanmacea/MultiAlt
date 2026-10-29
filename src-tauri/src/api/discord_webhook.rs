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
#[allow(dead_code)]
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
    if section == "Webhook" && key.starts_with("Mention") && value.chars().count() > MAX_MENTION_CHARS {
        return Err(format!("The text before messages is too long (max {MAX_MENTION_CHARS} characters)."));
    }
    if section == "Webhook"
        && matches!(key, "QuietStart" | "QuietEnd" | "SummaryTime")
        && !value.trim().is_empty()
        && parse_clock(value).is_none()
    {
        return Err("Use the 24-hour format, like 07:30.".to_string());
    }
    Ok(())
}

/// "HH:MM" (24 h) → minuto do dia. Qualquer outra coisa: `None`.
pub fn parse_clock(value: &str) -> Option<u32> {
    let (h, m) = value.trim().split_once(':')?;
    if h.len() != 2 || m.len() != 2 {
        return None;
    }
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
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
    /// O cliente lançado para uma conta entrou logado em outra conta do
    /// Roblox (client_health, `roblox-client-wrong-account`).
    WrongAccount,
    /// O Roblox está bloqueando a conta (ban ou análise): a conta foi para o
    /// grupo das moderadas (`mark_account_moderated`).
    Moderated,
    /// Um lote de launch (2 contas ou mais) terminou. Sem conta: o motivo traz
    /// a contagem (`BatchResult`).
    BatchFinished,
    /// O app fechou um cliente por memória: acima do teto (Optimization) ou
    /// abaixo do mínimo do Watcher.
    MemoryClosed,
    /// O Windows está quase sem memória livre. Sem conta.
    SystemLowMemory,
    /// Mensagem de um script Python (`multialt.discord`, op `discord.send`).
    ScriptMessage,
}

impl WebhookEventKind {
    pub const ALL: [WebhookEventKind; 11] = [
        Self::Dropped,
        Self::ReconnectFailed,
        Self::NotResponding,
        Self::Reconnected,
        Self::LaunchFailed,
        Self::WrongAccount,
        Self::Moderated,
        Self::BatchFinished,
        Self::MemoryClosed,
        Self::SystemLowMemory,
        Self::ScriptMessage,
    ];

    /// Gravidade, para a cor da mensagem: a pior linha decide.
    fn severity(self) -> u8 {
        match self {
            Self::Reconnected | Self::BatchFinished | Self::ScriptMessage => 0,
            Self::NotResponding | Self::SystemLowMemory => 1,
            Self::LaunchFailed | Self::MemoryClosed => 2,
            Self::Dropped | Self::ReconnectFailed | Self::WrongAccount | Self::Moderated => 3,
        }
    }

    /// O nome do evento nas chaves da seção `[Webhook]`: `Notify<nome>` liga a
    /// caixa, `Mention<nome>` é o texto de menção só dele.
    pub fn setting_key(self) -> &'static str {
        match self {
            Self::Dropped => "Dropped",
            Self::Reconnected => "Reconnected",
            Self::ReconnectFailed => "ReconnectFailed",
            Self::NotResponding => "NotResponding",
            Self::LaunchFailed => "LaunchFailed",
            Self::WrongAccount => "WrongAccount",
            Self::Moderated => "Moderated",
            Self::BatchFinished => "BatchFinished",
            Self::MemoryClosed => "MemoryClosed",
            Self::SystemLowMemory => "SystemLowMemory",
            Self::ScriptMessage => "ScriptMessage",
        }
    }

    /// Caixa ligada quando a chave não existe.
    fn default_on(self) -> bool {
        !matches!(
            self,
            Self::Reconnected | Self::LaunchFailed | Self::BatchFinished | Self::SystemLowMemory
        )
    }

    /// Evento de uma conta (leva o campo Conta e a foto) ou do app inteiro.
    pub fn has_account(self) -> bool {
        !matches!(self, Self::BatchFinished | Self::SystemLowMemory)
    }
}

/// Por que o evento aconteceu, como **código**: a mensagem é traduzida na hora
/// de montar, no idioma do app (`WebhookLang`). O único texto livre é o do
/// kick, e ele só vem preenchido com os nomes à mostra (pode citar o nome real
/// de outro jogador).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebhookReason {
    ConnectionLost { code: Option<u32> },
    JoinedElsewhere { code: Option<u32> },
    Idle { code: Option<u32> },
    Disconnected { code: Option<u32> },
    Kicked { code: Option<u32>, message: Option<String> },
    ServerShutdown { code: Option<u32> },
    ClosedWithoutLeaving,
    GaveUp { attempts: u32 },
    Banned,
    SessionExpired,
    NoDestination,
    NotResponding,
    AuthTicketFailed,
    ClientDidNotOpen,
    OtherAccount,
    BatchResult { done: u32, failed: u32, cancelled: u32 },
    MemoryOverLimit { memory_mb: u64, limit_mb: u64 },
    MemoryBelowMinimum { memory_mb: u64, min_mb: u64 },
    SystemMemoryLow { free_mb: u64, total_mb: u64 },
    /// O texto já passou por `script_message_text` / `script_message_title`.
    ScriptMessage { script: String, title: Option<String>, text: String },
}

/// Teto do texto de uma mensagem de script (o Discord aceita 4096 na descrição).
pub const MAX_SCRIPT_TEXT_CHARS: usize = 1_500;
/// Teto do título de uma mensagem de script (o Discord aceita 256).
pub const MAX_SCRIPT_TITLE_CHARS: usize = 100;

/// O `@` nunca menciona: ganha um espaço invisível, como no resto da mensagem.
/// Negrito, itálico e link continuam (o canal é do próprio usuário).
fn no_pings(text: &str) -> String {
    // Idempotente: o texto já limpo pelo `discord.send` passa de novo pelo cartão.
    text.replace("@\u{200B}", "@").replace('@', "@\u{200B}")
}

/// O texto de uma mensagem de script: sem espaço nas pontas, cortado no teto e
/// sem menção.
pub fn script_message_text(text: &str) -> String {
    no_pings(&truncate_chars(text.trim(), MAX_SCRIPT_TEXT_CHARS))
}

pub fn script_message_title(title: &str) -> String {
    no_pings(&truncate_chars(title.trim(), MAX_SCRIPT_TITLE_CHARS))
}

/// Um aviso pronto para a fila. `account` já vem mascarado (ou não) conforme
/// a opção (`masked`) — a fila nunca vê o nome real com "Hide account names"
/// ligado, e aí a mensagem também não leva foto nem link do perfil.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookEvent {
    pub kind: WebhookEventKind,
    pub user_id: i64,
    pub account: String,
    pub masked: bool,
    pub reason: Option<WebhookReason>,
    pub place_id: Option<i64>,
    pub at_ms: i64,
}

/// Idioma da mensagem: o do app (`General.Language`). Inglês para o resto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookLang {
    En,
    Pt,
    Es,
}

impl WebhookLang {
    pub fn from_setting(value: &str) -> Self {
        let v = value.trim().to_ascii_lowercase();
        if v.starts_with("pt") {
            Self::Pt
        } else if v.starts_with("es") {
            Self::Es
        } else {
            Self::En
        }
    }

    fn pick<'a>(self, en: &'a str, pt: &'a str, es: &'a str) -> &'a str {
        match self {
            Self::En => en,
            Self::Pt => pt,
            Self::Es => es,
        }
    }
}

/// O que a mensagem ganha além do que o evento traz, buscado na hora de enviar
/// (sem bloquear quem avisou): nome do jogo e foto da conta. Tudo opcional —
/// sem isto a mensagem sai igual, só sem o nome do jogo e sem a foto.
#[derive(Debug, Default, Clone)]
pub struct WebhookEnrichment {
    pub game_names: std::collections::HashMap<i64, String>,
    pub avatars: std::collections::HashMap<i64, String>,
}

/// As opções da seção `[Webhook]` do RAMSettings.ini.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookConfig {
    pub enabled: bool,
    pub target: Option<WebhookTarget>,
    /// A caixa de cada evento (`Notify<evento>`).
    boxes: std::collections::HashMap<WebhookEventKind, bool>,
    /// O texto antes de toda mensagem (`Mention`).
    pub mention: String,
    /// O texto só de um evento (`Mention<evento>`); vazio = o `mention`.
    mentions: std::collections::HashMap<WebhookEventKind, String>,
    /// "Hide account names in messages" (`MaskNames`, padrão **desligado**:
    /// o canal é do próprio usuário, e o nome é o que mais importa no aviso).
    pub mask_names: bool,
    /// Horário silencioso (`QuietHours` + `QuietStart`/`QuietEnd`, minuto do
    /// dia na hora local). `None` desligado, com horário inválido ou início = fim.
    quiet: Option<(u32, u32)>,
    /// No horário silencioso, segurar os avisos em vez de só tirar a menção
    /// (`QuietSkip`, padrão desligado).
    quiet_skip: bool,
}

fn clean_mention(text: &str) -> String {
    text.trim().chars().take(MAX_MENTION_CHARS).collect()
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
        let quiet = match (
            get("QuietStart").as_deref().and_then(parse_clock),
            get("QuietEnd").as_deref().and_then(parse_clock),
        ) {
            (Some(start), Some(end)) if flag("QuietHours", false) && start != end => Some((start, end)),
            _ => None,
        };
        Self {
            enabled: flag("Enabled", false),
            target: if url.trim().is_empty() { None } else { parse_webhook_url(&url).ok() },
            boxes: WebhookEventKind::ALL
                .into_iter()
                .map(|k| (k, flag(&format!("Notify{}", k.setting_key()), k.default_on())))
                .collect(),
            mention: clean_mention(&get("Mention").unwrap_or_default()),
            mentions: WebhookEventKind::ALL
                .into_iter()
                .filter_map(|k| {
                    let text = clean_mention(&get(&format!("Mention{}", k.setting_key())).unwrap_or_default());
                    (!text.is_empty()).then_some((k, text))
                })
                .collect(),
            mask_names: flag("MaskNames", false),
            quiet,
            quiet_skip: flag("QuietSkip", false),
        }
    }

    /// Este aviso sai? Só com a opção ligada, uma URL válida e a caixa marcada.
    pub fn wants(&self, kind: WebhookEventKind) -> bool {
        self.enabled && self.target.is_some() && self.wants_box(kind)
    }

    /// Só a caixa deste evento (a mensagem de teste lista as marcadas mesmo com
    /// a opção ainda desligada).
    pub fn wants_box(&self, kind: WebhookEventKind) -> bool {
        self.boxes.get(&kind).copied().unwrap_or_else(|| kind.default_on())
    }

    /// `minute` (minuto do dia, hora local) cai no horário silencioso? O
    /// horário pode atravessar a meia-noite (23:00 → 07:30).
    pub fn is_quiet(&self, minute: u32) -> bool {
        match self.quiet {
            Some((start, end)) if start < end => minute >= start && minute < end,
            Some((start, end)) => minute >= start || minute < end,
            None => false,
        }
    }

    /// No horário silencioso, com "não mandar nada" ligado, o aviso não sai.
    pub fn holds_messages(&self, minute: u32) -> bool {
        self.quiet_skip && self.is_quiet(minute)
    }

    /// O texto de menção de um lote: o de cada evento (ou o geral), cada um
    /// uma vez e na ordem. Nada no horário silencioso.
    pub fn mention_for(&self, events: &[WebhookEvent], minute: u32) -> String {
        if self.is_quiet(minute) {
            return String::new();
        }
        let mut out: Vec<&str> = Vec::new();
        for event in events {
            let text = self.mentions.get(&event.kind).map(String::as_str).unwrap_or(&self.mention);
            if !text.is_empty() && !out.contains(&text) {
                out.push(text);
            }
        }
        out.join(" ")
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

/// Discord: no máximo 10 embeds por mensagem.
const MAX_EMBEDS: usize = 10;

fn event_title(kind: WebhookEventKind, lang: WebhookLang) -> &'static str {
    match kind {
        WebhookEventKind::Dropped => lang.pick("🔴 Account dropped", "🔴 Conta caiu", "🔴 Cuenta caída"),
        WebhookEventKind::Reconnected => lang.pick("🟢 Back in the game", "🟢 Voltou para o jogo", "🟢 Volvió al juego"),
        WebhookEventKind::ReconnectFailed => {
            lang.pick("🟠 Could not reconnect", "🟠 Não conseguiu reconectar", "🟠 No pudo reconectar")
        }
        WebhookEventKind::NotResponding => {
            lang.pick("🟡 Window not responding", "🟡 Janela travada", "🟡 Ventana sin responder")
        }
        WebhookEventKind::LaunchFailed => lang.pick("⚠️ Launch failed", "⚠️ Falha ao abrir", "⚠️ No se pudo abrir"),
        WebhookEventKind::WrongAccount => lang.pick(
            "⚠️ Opened as another account",
            "⚠️ Abriu em outra conta",
            "⚠️ Se abrió con otra cuenta",
        ),
        WebhookEventKind::Moderated => lang.pick("⛔ Account banned", "⛔ Conta banida", "⛔ Cuenta baneada"),
        WebhookEventKind::BatchFinished => lang.pick(
            "📋 Launch batch finished",
            "📋 Lote de launch terminou",
            "📋 Terminó el lote de inicio",
        ),
        WebhookEventKind::MemoryClosed => lang.pick(
            "🧠 Client closed for memory",
            "🧠 Cliente fechado por memória",
            "🧠 Cliente cerrado por memoria",
        ),
        WebhookEventKind::SystemLowMemory => lang.pick(
            "🧠 Windows is almost out of memory",
            "🧠 O Windows está quase sem memória",
            "🧠 Windows casi no tiene memoria",
        ),
        WebhookEventKind::ScriptMessage => lang.pick(
            "📜 Message from a Python script",
            "📜 Mensagem de um script Python",
            "📜 Mensaje de un script de Python",
        ),
    }
}

/// A frase do que aconteceu, com o nome em negrito.
fn event_sentence(kind: WebhookEventKind, name: &str, lang: WebhookLang) -> String {
    let n = format!("**{name}**");
    match kind {
        WebhookEventKind::Dropped => match lang {
            WebhookLang::En => format!("{n} left the game."),
            WebhookLang::Pt => format!("{n} saiu do jogo."),
            WebhookLang::Es => format!("{n} salió del juego."),
        },
        WebhookEventKind::Reconnected => match lang {
            WebhookLang::En => format!("{n} is back in the game."),
            WebhookLang::Pt => format!("{n} voltou para o jogo."),
            WebhookLang::Es => format!("{n} volvió al juego."),
        },
        WebhookEventKind::ReconnectFailed => match lang {
            WebhookLang::En => format!("Auto-reconnect could not bring {n} back."),
            WebhookLang::Pt => format!("A reconexão automática não conseguiu trazer {n} de volta."),
            WebhookLang::Es => format!("La reconexión automática no pudo traer de vuelta a {n}."),
        },
        WebhookEventKind::NotResponding => match lang {
            WebhookLang::En => format!("The Roblox window of {n} stopped responding."),
            WebhookLang::Pt => format!("A janela do Roblox de {n} parou de responder."),
            WebhookLang::Es => format!("La ventana de Roblox de {n} dejó de responder."),
        },
        WebhookEventKind::LaunchFailed => match lang {
            WebhookLang::En => format!("{n} could not be opened."),
            WebhookLang::Pt => format!("Não deu para abrir {n}."),
            WebhookLang::Es => format!("No se pudo abrir {n}."),
        },
        WebhookEventKind::WrongAccount => match lang {
            WebhookLang::En => format!(
                "The client opened for {n} is logged into a different Roblox account. The launch queue was paused and the client was left open."
            ),
            WebhookLang::Pt => format!(
                "O cliente aberto para {n} está logado em outra conta do Roblox. A fila de launch foi pausada e o cliente ficou aberto."
            ),
            WebhookLang::Es => format!(
                "El cliente abierto para {n} tiene sesión en otra cuenta de Roblox. La cola de inicio se pausó y el cliente quedó abierto."
            ),
        },
        WebhookEventKind::Moderated => match lang {
            WebhookLang::En => format!("Roblox is blocking {n}."),
            WebhookLang::Pt => format!("O Roblox está bloqueando {n}."),
            WebhookLang::Es => format!("Roblox está bloqueando a {n}."),
        },
        WebhookEventKind::MemoryClosed => match lang {
            WebhookLang::En => format!("MultiAlt closed the Roblox client of {n}."),
            WebhookLang::Pt => format!("O MultiAlt fechou o cliente do Roblox de {n}."),
            WebhookLang::Es => format!("MultiAlt cerró el cliente de Roblox de {n}."),
        },
        // Do app inteiro: a frase sai do motivo (`event_embed`); a do script é o texto dele.
        WebhookEventKind::BatchFinished | WebhookEventKind::SystemLowMemory | WebhookEventKind::ScriptMessage => {
            String::new()
        }
    }
}

fn with_code(text: &str, code: Option<u32>, lang: WebhookLang) -> String {
    match code {
        Some(c) => format!("{text} ({} {c})", lang.pick("code", "código", "código")),
        None => text.to_string(),
    }
}

/// O motivo, traduzido. Texto livre (o do kick) passa por `escape_markdown`.
pub fn reason_text(reason: &WebhookReason, lang: WebhookLang) -> String {
    match reason {
        WebhookReason::ConnectionLost { code } => {
            with_code(lang.pick("Lost the connection", "Perdeu a conexão", "Perdió la conexión"), *code, lang)
        }
        WebhookReason::JoinedElsewhere { code } => with_code(
            lang.pick(
                "The account joined from somewhere else",
                "A conta entrou por outro lugar",
                "La cuenta entró desde otro lugar",
            ),
            *code,
            lang,
        ),
        WebhookReason::Idle { code } => with_code(
            lang.pick("Idle for too long", "Ficou parada tempo demais", "Inactiva demasiado tiempo"),
            *code,
            lang,
        ),
        WebhookReason::Disconnected { code } => {
            with_code(lang.pick("Disconnected", "Desconectada", "Desconectada"), *code, lang)
        }
        WebhookReason::Kicked { code, message } => match message.as_deref().filter(|m| !m.trim().is_empty()) {
            Some(m) => format!(
                "{}: {}",
                lang.pick("Kicked", "Expulsa", "Expulsada"),
                escape_markdown(m)
            ),
            None => with_code(
                lang.pick("Kicked from the game", "Expulsa do jogo", "Expulsada del juego"),
                *code,
                lang,
            ),
        },
        WebhookReason::ServerShutdown { code } => with_code(
            lang.pick("The server shut down", "O servidor fechou", "El servidor se cerró"),
            *code,
            lang,
        ),
        WebhookReason::ClosedWithoutLeaving => lang
            .pick(
                "The Roblox client closed without leaving the game",
                "O Roblox fechou sem sair do jogo",
                "Roblox se cerró sin salir del juego",
            )
            .to_string(),
        WebhookReason::GaveUp { attempts } => match lang {
            WebhookLang::En => format!("Gave up after {attempts} attempts"),
            WebhookLang::Pt => format!("Desistiu depois de {attempts} tentativas"),
            WebhookLang::Es => format!("Se rindió tras {attempts} intentos"),
        },
        WebhookReason::Banned => lang.pick("The account is banned", "A conta está banida", "La cuenta está baneada").to_string(),
        WebhookReason::SessionExpired => lang
            .pick(
                "The session expired: log in to the account again",
                "A sessão expirou: entre na conta de novo",
                "La sesión expiró: vuelve a iniciar sesión en la cuenta",
            )
            .to_string(),
        WebhookReason::NoDestination => lang
            .pick("No game to return to", "Não havia jogo para voltar", "No había juego al que volver")
            .to_string(),
        WebhookReason::NotResponding => lang
            .pick("No response for 30 seconds", "Sem resposta há 30 segundos", "Sin respuesta durante 30 segundos")
            .to_string(),
        WebhookReason::AuthTicketFailed => lang
            .pick(
                "Could not get the authentication ticket",
                "Não deu para obter o ticket de autenticação",
                "No se pudo obtener el ticket de autenticación",
            )
            .to_string(),
        WebhookReason::ClientDidNotOpen => lang
            .pick(
                "The Roblox client did not open",
                "O cliente do Roblox não abriu",
                "El cliente de Roblox no se abrió",
            )
            .to_string(),
        WebhookReason::OtherAccount => lang
            .pick(
                "Logged into a different Roblox account",
                "Logado em outra conta do Roblox",
                "Con sesión en otra cuenta de Roblox",
            )
            .to_string(),
        WebhookReason::BatchResult { done, failed, cancelled } => match lang {
            WebhookLang::En => format!("{done} opened, {failed} failed, {cancelled} cancelled."),
            WebhookLang::Pt => format!("{done} abertas, {failed} falharam, {cancelled} canceladas."),
            WebhookLang::Es => format!("{done} abiertas, {failed} fallaron, {cancelled} canceladas."),
        },
        WebhookReason::MemoryOverLimit { memory_mb, limit_mb } => match lang {
            WebhookLang::En => format!("Using {memory_mb} MB, over the {limit_mb} MB limit"),
            WebhookLang::Pt => format!("Usando {memory_mb} MB, acima do limite de {limit_mb} MB"),
            WebhookLang::Es => format!("Usando {memory_mb} MB, por encima del límite de {limit_mb} MB"),
        },
        WebhookReason::MemoryBelowMinimum { memory_mb, min_mb } => match lang {
            WebhookLang::En => format!("Memory dropped to {memory_mb} MB, under the {min_mb} MB minimum (client frozen)"),
            WebhookLang::Pt => format!("A memória caiu para {memory_mb} MB, abaixo do mínimo de {min_mb} MB (cliente travado)"),
            WebhookLang::Es => format!("La memoria bajó a {memory_mb} MB, por debajo del mínimo de {min_mb} MB (cliente congelado)"),
        },
        WebhookReason::ScriptMessage { text, .. } => text.clone(),
        WebhookReason::SystemMemoryLow { free_mb, total_mb } => match lang {
            WebhookLang::En => format!("{free_mb} MB free of {total_mb} MB."),
            WebhookLang::Pt => format!("{free_mb} MB livres de {total_mb} MB."),
            WebhookLang::Es => format!("{free_mb} MB libres de {total_mb} MB."),
        },
    }
}

fn severity_color(severity: u8) -> u32 {
    match severity {
        0 => 0x2e_cc_71, // verde
        1 => 0xf1_c4_0f, // amarelo
        2 => 0xe6_7e_22, // laranja
        _ => 0xe7_4c_3c, // vermelho
    }
}

fn field(name: &str, value: String, inline: bool) -> serde_json::Value {
    serde_json::json!({ "name": name, "value": truncate_chars(&value, 1_000), "inline": inline })
}

/// Um cartão (embed) para um aviso: título com ícone, a frase do que
/// aconteceu e os campos Conta / Jogo / Motivo / Quando. Com o nome à mostra,
/// o nome vira link do perfil e a foto da conta aparece; mascarado, nenhum dos
/// dois (a foto e o perfil identificariam a conta).
pub fn event_embed(event: &WebhookEvent, lang: WebhookLang, extra: &WebhookEnrichment) -> serde_json::Value {
    if let Some(WebhookReason::ScriptMessage { script, title, text }) = &event.reason {
        return script_embed(event, script, title.as_deref(), text, lang, extra);
    }
    let name = escape_markdown(&event.account);
    let mut fields = Vec::new();

    let with_account = event.kind.has_account();
    if with_account {
        let account_value = if event.masked || event.user_id <= 0 {
            name.clone()
        } else {
            format!("[{name}](https://www.roblox.com/users/{}/profile)", event.user_id)
        };
        fields.push(field(lang.pick("Account", "Conta", "Cuenta"), account_value, true));
    }

    if let Some(place) = event.place_id.filter(|p| *p > 0) {
        let label = match extra.game_names.get(&place).map(|g| g.trim()).filter(|g| !g.is_empty()) {
            Some(game) => escape_markdown(game),
            None => format!("Place {place}"),
        };
        fields.push(field(
            lang.pick("Game", "Jogo", "Juego"),
            format!("[{label}](https://www.roblox.com/games/{place})"),
            true,
        ));
    }

    // Evento do app inteiro (lote, memória do Windows): o motivo é a frase.
    let description = match (&event.reason, with_account) {
        (Some(reason), false) => reason_text(reason, lang),
        (reason, _) => {
            if let Some(reason) = reason {
                fields.push(field(lang.pick("Reason", "Motivo", "Motivo"), reason_text(reason, lang), false));
            }
            event_sentence(event.kind, &name, lang)
        }
    };

    // O Discord mostra `<t:…>` na hora local de quem lê: relativo ("há 2 min")
    // e a hora certa.
    let secs = event.at_ms.div_euclid(1_000);
    fields.push(field(lang.pick("When", "Quando", "Cuándo"), format!("<t:{secs}:R> · <t:{secs}:T>"), true));

    let mut embed = serde_json::json!({
        "title": event_title(event.kind, lang),
        "description": description,
        "color": severity_color(event.kind.severity()),
        "fields": fields,
        "footer": { "text": "MultiAlt" },
    });
    if let Some(ts) = chrono::DateTime::from_timestamp_millis(event.at_ms) {
        embed["timestamp"] = serde_json::Value::String(ts.to_rfc3339());
    }
    if !event.masked && with_account {
        if let Some(url) = extra.avatars.get(&event.user_id) {
            embed["thumbnail"] = serde_json::json!({ "url": url });
        }
    }
    embed
}

/// Teto do texto de menção de uma mensagem (o de vários eventos junto). O
/// Discord aceita 2000 no `content`.
const MAX_CONTENT_MENTION_CHARS: usize = 1_000;

/// O cartão de uma mensagem de script: o título que o script deu (ou "Mensagem
/// de <script>"), o texto dele como descrição e os campos Script / Conta /
/// Quando. Conta e foto só quando a mensagem é de uma conta e o nome está à mostra.
fn script_embed(
    event: &WebhookEvent,
    script: &str,
    title: Option<&str>,
    text: &str,
    lang: WebhookLang,
    extra: &WebhookEnrichment,
) -> serde_json::Value {
    let script_name = escape_markdown(script);
    let title = match title.filter(|t| !t.trim().is_empty()) {
        Some(t) => format!("📜 {t}"),
        None => match lang {
            WebhookLang::En => format!("📜 Message from {script_name}"),
            WebhookLang::Pt => format!("📜 Mensagem de {script_name}"),
            WebhookLang::Es => format!("📜 Mensaje de {script_name}"),
        },
    };
    let mut fields = vec![field("Script", script_name, true)];
    if event.user_id > 0 {
        let name = escape_markdown(&event.account);
        let value = if event.masked {
            name
        } else {
            format!("[{name}](https://www.roblox.com/users/{}/profile)", event.user_id)
        };
        fields.push(field(lang.pick("Account", "Conta", "Cuenta"), value, true));
    }
    let secs = event.at_ms.div_euclid(1_000);
    fields.push(field(lang.pick("When", "Quando", "Cuándo"), format!("<t:{secs}:R> · <t:{secs}:T>"), true));
    let mut embed = serde_json::json!({
        "title": no_pings(&title),
        "description": no_pings(text),
        "color": 0x58_65_f2,
        "fields": fields,
        "footer": { "text": "MultiAlt" },
    });
    if let Some(ts) = chrono::DateTime::from_timestamp_millis(event.at_ms) {
        embed["timestamp"] = serde_json::Value::String(ts.to_rfc3339());
    }
    if !event.masked && event.user_id > 0 {
        if let Some(url) = extra.avatars.get(&event.user_id) {
            embed["thumbnail"] = serde_json::json!({ "url": url });
        }
    }
    embed
}

fn mention_and_allowed(mention: &str) -> (String, serde_json::Value) {
    let mention: String = mention.trim().chars().take(MAX_CONTENT_MENTION_CHARS).collect();
    let allowed = if mention.is_empty() {
        serde_json::json!({ "parse": [] })
    } else {
        serde_json::json!({ "parse": ["everyone", "roles", "users"] })
    };
    (mention, allowed)
}

/// O corpo do POST: um cartão por aviso (até 10; passou disso, o 10º resume o
/// resto). A menção (texto antes) vai no `content` e é a **única** coisa que
/// pode mencionar alguém.
pub fn build_message(
    events: &[WebhookEvent],
    mention: &str,
    lang: WebhookLang,
    extra: &WebhookEnrichment,
) -> serde_json::Value {
    let mut embeds: Vec<serde_json::Value> = Vec::new();
    if events.len() <= MAX_EMBEDS {
        embeds.extend(events.iter().map(|e| event_embed(e, lang, extra)));
    } else {
        embeds.extend(events[..MAX_EMBEDS - 1].iter().map(|e| event_embed(e, lang, extra)));
        let rest = events.len() - (MAX_EMBEDS - 1);
        let title = match lang {
            WebhookLang::En => format!("+{rest} more notifications"),
            WebhookLang::Pt => format!("+{rest} avisos a mais"),
            WebhookLang::Es => format!("+{rest} avisos más"),
        };
        let worst = events[MAX_EMBEDS - 1..].iter().map(|e| e.kind.severity()).max().unwrap_or(0);
        embeds.push(serde_json::json!({ "title": title, "color": severity_color(worst) }));
    }
    let (mention, allowed) = mention_and_allowed(mention);
    serde_json::json!({
        "username": "MultiAlt",
        "content": mention,
        "allowed_mentions": allowed,
        "embeds": embeds,
    })
}

/// "Send test message": um cartão dizendo que o canal está ligado (com os
/// avisos marcados) e, se houver conta, um exemplo real de como chega.
pub fn build_test_message(
    mention: &str,
    lang: WebhookLang,
    enabled: &[WebhookEventKind],
    sample: Option<(&WebhookEvent, &WebhookEnrichment)>,
) -> serde_json::Value {
    let list = if enabled.is_empty() {
        lang.pick("None", "Nenhum", "Ninguno").to_string()
    } else {
        enabled.iter().map(|k| event_title(*k, lang)).collect::<Vec<_>>().join("\n")
    };
    let mut embeds = vec![serde_json::json!({
        "title": lang.pick("✅ Webhook connected", "✅ Webhook conectado", "✅ Webhook conectado"),
        "description": lang.pick(
            "MultiAlt notifications will arrive in this channel.",
            "Os avisos do MultiAlt vão chegar neste canal.",
            "Los avisos de MultiAlt llegarán a este canal.",
        ),
        "color": 0x58_65_f2,
        "fields": [field(lang.pick("Notifications on", "Avisos ligados", "Avisos activos"), list, false)],
        "footer": { "text": "MultiAlt" },
    })];
    if let Some((event, extra)) = sample {
        let mut example = event_embed(event, lang, extra);
        let prefix = lang.pick("Example", "Exemplo", "Ejemplo");
        let title = example["title"].as_str().unwrap_or_default().to_string();
        example["title"] = serde_json::Value::String(format!("{prefix} — {title}"));
        embeds.push(example);
    }
    let (mention, allowed) = mention_and_allowed(mention);
    serde_json::json!({
        "username": "MultiAlt",
        "content": mention,
        "allowed_mentions": allowed,
        "embeds": embeds,
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
            user_id: 55,
            account: account.into(),
            masked: false,
            reason: None,
            place_id: None,
            at_ms: 1_760_000_000_000,
        }
    }

    #[test]
    fn off_by_default_and_needs_a_valid_url() {
        let c = settings(&[]);
        assert!(!c.enabled);
        assert!(!c.mask_names, "names show by default (15/10/2026)");
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
        assert!(on.wants(WebhookEventKind::WrongAccount));

        let custom = settings(&[
            ("Enabled", "true"),
            ("Url", URL),
            ("NotifyDropped", "false"),
            ("NotifyReconnected", "true"),
            ("NotifyLaunchFailed", "true"),
            ("NotifyWrongAccount", "false"),
        ]);
        assert!(!custom.wants(WebhookEventKind::Dropped));
        assert!(custom.wants(WebhookEventKind::Reconnected));
        assert!(custom.wants(WebhookEventKind::LaunchFailed));
        assert!(!custom.wants(WebhookEventKind::WrongAccount));
    }

    #[test]
    fn the_new_events_have_their_own_boxes_and_defaults() {
        let on = settings(&[("Enabled", "true"), ("Url", URL)]);
        assert!(on.wants(WebhookEventKind::Moderated), "a ban is on by default");
        assert!(on.wants(WebhookEventKind::MemoryClosed), "a client closed for memory is on by default");
        assert!(!on.wants(WebhookEventKind::BatchFinished));
        assert!(!on.wants(WebhookEventKind::SystemLowMemory));
        let custom = settings(&[
            ("Enabled", "true"),
            ("Url", URL),
            ("NotifyModerated", "false"),
            ("NotifyBatchFinished", "true"),
            ("NotifySystemLowMemory", "true"),
        ]);
        assert!(!custom.wants(WebhookEventKind::Moderated));
        assert!(custom.wants(WebhookEventKind::BatchFinished));
        assert!(custom.wants(WebhookEventKind::SystemLowMemory));
    }

    /// Pedido do dono (27/10/2026): cada evento pode chamar alguém diferente.
    #[test]
    fn each_event_can_ping_its_own_people_and_falls_back_to_the_global_text() {
        let c = settings(&[
            ("Enabled", "true"),
            ("Url", URL),
            ("Mention", "<@&1>"),
            ("MentionModerated", "@everyone"),
            ("MentionDropped", " <@2> "),
        ]);
        let noon = 12 * 60;
        assert_eq!(c.mention_for(&[event(WebhookEventKind::Moderated, "A")], noon), "@everyone");
        assert_eq!(c.mention_for(&[event(WebhookEventKind::Dropped, "A")], noon), "<@2>");
        assert_eq!(c.mention_for(&[event(WebhookEventKind::NotResponding, "A")], noon), "<@&1>");
        // Um lote com vários: cada um uma vez, na ordem.
        let batch = [
            event(WebhookEventKind::Dropped, "A"),
            event(WebhookEventKind::Dropped, "B"),
            event(WebhookEventKind::Moderated, "C"),
            event(WebhookEventKind::NotResponding, "D"),
        ];
        assert_eq!(c.mention_for(&batch, noon), "<@2> @everyone <@&1>");
    }

    #[test]
    fn quiet_hours_take_the_pings_out_and_can_hold_the_messages() {
        let c = settings(&[
            ("Enabled", "true"),
            ("Url", URL),
            ("Mention", "@everyone"),
            ("QuietHours", "true"),
            ("QuietStart", "23:00"),
            ("QuietEnd", "07:30"),
        ]);
        let drop = [event(WebhookEventKind::Dropped, "A")];
        // Atravessa a meia-noite.
        assert!(c.is_quiet(23 * 60));
        assert!(c.is_quiet(2 * 60));
        assert!(c.is_quiet(7 * 60 + 29));
        assert!(!c.is_quiet(7 * 60 + 30));
        assert!(!c.is_quiet(12 * 60));
        assert_eq!(c.mention_for(&drop, 2 * 60), "");
        assert_eq!(c.mention_for(&drop, 12 * 60), "@everyone");
        // Por padrão a mensagem sai, só sem menção.
        assert!(!c.holds_messages(2 * 60));
        let hold = settings(&[
            ("Enabled", "true"),
            ("Url", URL),
            ("QuietHours", "true"),
            ("QuietStart", "09:00"),
            ("QuietEnd", "17:00"),
            ("QuietSkip", "true"),
        ]);
        assert!(hold.holds_messages(10 * 60));
        assert!(!hold.holds_messages(18 * 60));
        // Desligado, horário inválido ou início = fim: nunca silencioso.
        assert!(!settings(&[("QuietStart", "23:00"), ("QuietEnd", "07:00")]).is_quiet(2 * 60));
        assert!(!settings(&[("QuietHours", "true"), ("QuietStart", "25:00"), ("QuietEnd", "07:00")]).is_quiet(2 * 60));
        assert!(!settings(&[("QuietHours", "true"), ("QuietStart", "08:00"), ("QuietEnd", "08:00")]).is_quiet(8 * 60));
    }

    #[test]
    fn times_and_per_event_mentions_are_checked_before_saving() {
        assert!(validate_setting("Webhook", "QuietStart", "07:30").is_ok());
        assert!(validate_setting("Webhook", "QuietStart", "7:3").is_err());
        assert!(validate_setting("Webhook", "SummaryTime", "24:00").is_err());
        assert!(validate_setting("Webhook", "SummaryTime", "").is_ok());
        assert!(validate_setting("Webhook", "MentionDropped", &"x".repeat(MAX_MENTION_CHARS + 1)).is_err());
        assert!(validate_setting("Webhook", "MentionDropped", "<@&1>").is_ok());
    }

    #[test]
    fn a_ban_a_batch_and_memory_read_like_the_other_cards() {
        let ban = WebhookEvent { reason: Some(WebhookReason::Banned), ..event(WebhookEventKind::Moderated, "Farm 1") };
        let card = &build_message(&[ban], "", WebhookLang::Pt, &WebhookEnrichment::default())["embeds"][0];
        assert_eq!(card["title"], "⛔ Conta banida");
        assert_eq!(card["description"], "O Roblox está bloqueando **Farm 1**.");
        assert_eq!(card["color"], 0xe7_4c_3c);

        let batch = WebhookEvent {
            user_id: 0,
            account: String::new(),
            reason: Some(WebhookReason::BatchResult { done: 8, failed: 1, cancelled: 0 }),
            place_id: Some(42),
            ..event(WebhookEventKind::BatchFinished, "")
        };
        let card = &build_message(&[batch], "", WebhookLang::En, &WebhookEnrichment::default())["embeds"][0];
        assert_eq!(card["title"], "📋 Launch batch finished");
        assert_eq!(card["description"], "8 opened, 1 failed, 0 cancelled.");
        let names: Vec<&str> = card["fields"].as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap()).collect();
        assert!(!names.contains(&"Account"), "a batch has no single account: {names:?}");
        assert!(names.contains(&"Game"));

        let memory = WebhookEvent {
            reason: Some(WebhookReason::MemoryOverLimit { memory_mb: 3_100, limit_mb: 2_048 }),
            ..event(WebhookEventKind::MemoryClosed, "Farm 2")
        };
        let card = &build_message(&[memory], "", WebhookLang::En, &WebhookEnrichment::default())["embeds"][0];
        assert_eq!(card["title"], "🧠 Client closed for memory");
        assert_eq!(card["fields"][1]["value"], "Using 3100 MB, over the 2048 MB limit");

        let low = WebhookEvent {
            user_id: 0,
            account: String::new(),
            reason: Some(WebhookReason::SystemMemoryLow { free_mb: 700, total_mb: 16_000 }),
            ..event(WebhookEventKind::SystemLowMemory, "")
        };
        let card = &build_message(&[low], "", WebhookLang::En, &WebhookEnrichment::default())["embeds"][0];
        assert_eq!(card["title"], "🧠 Windows is almost out of memory");
        assert_eq!(card["description"], "700 MB free of 16000 MB.");
    }

    /// Pedido do dono (28/10/2026): função padrão para mandar mensagem pelo
    /// webhook a partir dos scripts Python.
    #[test]
    fn a_script_message_is_its_own_card_and_cannot_ping() {
        let on = settings(&[("Enabled", "true"), ("Url", URL)]);
        assert!(on.wants(WebhookEventKind::ScriptMessage), "on by default: the script asked for it");
        let e = WebhookEvent {
            reason: Some(WebhookReason::ScriptMessage {
                script: "farm".into(),
                title: Some("Night run".into()),
                text: "**12** loops @everyone <@&99>".into(),
            }),
            ..event(WebhookEventKind::ScriptMessage, "Farm 1")
        };
        let card = &build_message(&[e.clone()], "", WebhookLang::En, &extra())["embeds"][0];
        assert_eq!(card["title"], "📜 Night run");
        let text = card["description"].as_str().unwrap();
        assert!(text.starts_with("**12** loops"), "formatting stays: {text}");
        assert!(!text.contains("@everyone") && !text.contains("<@&"), "{text}");
        let names: Vec<&str> = card["fields"].as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"Script"), "{names:?}");
        assert!(names.contains(&"Account"));
        // Sem conta e sem título: o nome do script no título, sem campo Conta.
        let bare = WebhookEvent {
            user_id: 0,
            account: String::new(),
            reason: Some(WebhookReason::ScriptMessage { script: "farm".into(), title: None, text: "done".into() }),
            ..event(WebhookEventKind::ScriptMessage, "")
        };
        let card = &build_message(&[bare], "", WebhookLang::Pt, &WebhookEnrichment::default())["embeds"][0];
        assert_eq!(card["title"], "📜 Mensagem de farm");
        assert!(!card.to_string().contains("\"Conta\""));
    }

    #[test]
    fn a_script_message_is_cut_to_the_limit() {
        let long = "x".repeat(MAX_SCRIPT_TEXT_CHARS + 50);
        assert_eq!(script_message_text(&long).chars().count(), MAX_SCRIPT_TEXT_CHARS + 1, "cut with an ellipsis");
        assert_eq!(script_message_text("  hi  "), "hi");
        assert_eq!(script_message_title("t".repeat(300).as_str()).chars().count(), MAX_SCRIPT_TITLE_CHARS + 1);
    }

    fn extra() -> WebhookEnrichment {
        let mut e = WebhookEnrichment::default();
        e.game_names.insert(107535308163741, "Huss Valley".into());
        e.avatars.insert(55, "https://tr.rbxcdn.com/avatar.png".into());
        e
    }

    /// Pedido do dono (15/10/2026): título, o que aconteceu, o nome à mostra e
    /// o jogo pelo nome — a linha única em inglês com o nome mascarado era feia.
    #[test]
    fn a_drop_reads_like_a_card_with_title_sentence_and_fields() {
        let e = WebhookEvent {
            reason: Some(WebhookReason::ClosedWithoutLeaving),
            place_id: Some(107535308163741),
            ..event(WebhookEventKind::Dropped, "dasdawedaw2e1231")
        };
        let body = build_message(&[e], "", WebhookLang::Pt, &extra());
        let card = &body["embeds"][0];
        assert_eq!(card["title"], "🔴 Conta caiu");
        assert_eq!(card["description"], "**dasdawedaw2e1231** saiu do jogo.");
        let fields = card["fields"].as_array().unwrap();
        assert_eq!(fields[0]["name"], "Conta");
        assert_eq!(fields[0]["value"], "[dasdawedaw2e1231](https://www.roblox.com/users/55/profile)");
        assert_eq!(fields[1]["name"], "Jogo");
        assert_eq!(fields[1]["value"], "[Huss Valley](https://www.roblox.com/games/107535308163741)");
        assert_eq!(fields[2]["value"], "O Roblox fechou sem sair do jogo");
        assert!(fields[3]["value"].as_str().unwrap().starts_with("<t:1760000000:R>"));
        assert_eq!(card["thumbnail"]["url"], "https://tr.rbxcdn.com/avatar.png");
        assert_eq!(card["color"], 0xe7_4c_3c);
        assert!(card["timestamp"].is_string());
    }

    #[test]
    fn english_and_spanish_follow_the_app_language() {
        let e = WebhookEvent {
            reason: Some(WebhookReason::ConnectionLost { code: Some(277) }),
            ..event(WebhookEventKind::Dropped, "Farm 1")
        };
        let en = build_message(&[e.clone()], "", WebhookLang::En, &WebhookEnrichment::default());
        assert_eq!(en["embeds"][0]["title"], "🔴 Account dropped");
        assert_eq!(en["embeds"][0]["fields"][1]["value"], "Lost the connection (code 277)");
        let es = build_message(&[e], "", WebhookLang::Es, &WebhookEnrichment::default());
        assert_eq!(es["embeds"][0]["title"], "🔴 Cuenta caída");
        assert_eq!(es["embeds"][0]["fields"][1]["value"], "Perdió la conexión (código 277)");
        assert_eq!(WebhookLang::from_setting("pt-BR"), WebhookLang::Pt);
        assert_eq!(WebhookLang::from_setting("de"), WebhookLang::En);
    }

    /// Mascarado: nada de foto nem link do perfil — identificariam a conta.
    #[test]
    fn a_masked_name_has_no_photo_and_no_profile_link() {
        let e = WebhookEvent { masked: true, ..event(WebhookEventKind::Dropped, "Fa********") };
        let card = &build_message(&[e], "", WebhookLang::En, &extra())["embeds"][0];
        assert!(card.get("thumbnail").is_none());
        assert!(!card.to_string().contains("/profile"));
    }

    #[test]
    fn without_a_known_name_the_game_is_a_place_link() {
        let e = WebhookEvent { place_id: Some(42), ..event(WebhookEventKind::Dropped, "A") };
        let card = &build_message(&[e], "", WebhookLang::En, &WebhookEnrichment::default())["embeds"][0];
        assert_eq!(card["fields"][1]["value"], "[Place 42](https://www.roblox.com/games/42)");
    }

    #[test]
    fn free_text_cannot_format_link_or_ping() {
        let e = WebhookEvent {
            reason: Some(WebhookReason::Kicked { code: None, message: Some("@everyone **x** [a](http://x)\nnext".into()) }),
            ..event(WebhookEventKind::Dropped, "<@&123>")
        };
        let text = build_message(&[e], "", WebhookLang::En, &WebhookEnrichment::default()).to_string();
        assert!(!text.contains("@everyone"), "{text}");
        assert!(!text.contains("<@&"), "{text}");
        assert!(!text.contains("**x**"), "{text}");
    }

    #[test]
    fn several_events_are_one_message_with_a_card_each_and_only_the_prefix_can_mention() {
        let events = vec![event(WebhookEventKind::Dropped, "A"), event(WebhookEventKind::Reconnected, "B")];
        let body = build_message(&events, "", WebhookLang::En, &WebhookEnrichment::default());
        assert_eq!(body["embeds"].as_array().unwrap().len(), 2);
        assert_eq!(body["embeds"][1]["title"], "🟢 Back in the game");
        assert_eq!(body["allowed_mentions"]["parse"], serde_json::json!([]));

        let pinged = build_message(&events[..1], "<@&42> ", WebhookLang::En, &WebhookEnrichment::default());
        assert_eq!(pinged["content"], "<@&42>");
        assert_ne!(pinged["allowed_mentions"]["parse"], serde_json::json!([]));
    }

    #[test]
    fn more_than_ten_events_fit_discords_ten_cards() {
        let events: Vec<WebhookEvent> = (0..15).map(|i| event(WebhookEventKind::Dropped, &format!("acc{i}"))).collect();
        let body = build_message(&events, "", WebhookLang::Pt, &WebhookEnrichment::default());
        let cards = body["embeds"].as_array().unwrap();
        assert_eq!(cards.len(), 10);
        assert_eq!(cards[9]["title"], "+6 avisos a mais");
    }

    #[test]
    fn the_test_message_says_it_is_connected_lists_the_events_and_shows_an_example() {
        let sample = WebhookEvent {
            reason: Some(WebhookReason::ClosedWithoutLeaving),
            ..event(WebhookEventKind::Dropped, "dasdawedaw2e1231")
        };
        let body = build_test_message(
            "",
            WebhookLang::Pt,
            &[WebhookEventKind::Dropped, WebhookEventKind::NotResponding],
            Some((&sample, &extra())),
        );
        let cards = body["embeds"].as_array().unwrap();
        assert_eq!(cards[0]["title"], "✅ Webhook conectado");
        assert_eq!(cards[0]["fields"][0]["value"], "🔴 Conta caiu\n🟡 Janela travada");
        assert_eq!(cards[1]["title"], "Exemplo — 🔴 Conta caiu");
        assert_eq!(cards[1]["description"], "**dasdawedaw2e1231** saiu do jogo.");
    }
}

#[cfg(test)]
mod discord_webhook_queue_tests {
    use super::*;

    fn event(n: usize) -> WebhookEvent {
        WebhookEvent {
            kind: WebhookEventKind::Dropped,
            user_id: n as i64,
            account: format!("acc{n}"),
            masked: false,
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
        let body = build_message(&[], "", WebhookLang::En, &WebhookEnrichment::default());
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
        let outcome = send(&client(), &target("100000000000000002"), &build_message(&[], "", WebhookLang::En, &WebhookEnrichment::default())).await;
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
        let err = send(&client(), &target("100000000000000003"), &build_message(&[], "", WebhookLang::En, &WebhookEnrichment::default()))
            .await
            .unwrap_err();
        assert!(err.contains("404"), "{err}");
        assert!(!err.contains("SeCrEtToken"), "{err}");
        assert!(!err.contains("webhooks"), "{err}");
    }
}
