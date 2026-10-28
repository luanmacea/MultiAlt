// Situação de moderação da conta (banida, advertida, encerrada), antes do launch.
//
// Uma leitura só: `GET usermoderation.roblox.com/v1/not-approved` com o cookie
// da conta. Conta sem punição responde `{}`; punida responde o "aviso" que o
// site mostra, com:
// - `punishmentTypeDescription`: "Warn", "Ban 1 Day", "Ban 3 Days", "Ban 7
//   Days", "Ban 14 Days", "Delete" (encerrada)…;
// - `endDate`: fim do ban (ISO 8601), `null` quando não há;
// - `messageToUser`: a nota do moderador.
// Nenhum outro campo é lido. Quem chama usa o caminho sem refresh
// (`read_without_refresh`): consultar moderação nunca pode deslogar a conta.

/// Estado de moderação resumido para a tela e para o launch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModerationState {
    Clean,
    Warned,
    Banned,
    Terminated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModerationStatus {
    pub state: ModerationState,
    /// Fim do ban (RFC 3339), quando o Roblox informa.
    pub until: Option<String>,
    /// Nota do moderador (`messageToUser`), quando há.
    pub note: Option<String>,
    /// O tipo como o Roblox escreve (`punishmentTypeDescription`).
    pub punishment: Option<String>,
}

impl ModerationStatus {
    pub fn clean() -> Self {
        Self { state: ModerationState::Clean, until: None, note: None, punishment: None }
    }
}

/// Por que a consulta não respondeu. Só [`ModerationFailure::Unauthorized`]
/// diz algo sobre a conta (cookie morto); limite e rede são "tente depois" e
/// **nunca** viram "banida".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModerationFailure {
    Unauthorized,
    RateLimited,
    Other(String),
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct NotApproved {
    #[serde(default)]
    punishment_type_description: Option<String>,
    #[serde(default)]
    end_date: Option<String>,
    #[serde(default)]
    message_to_user: Option<String>,
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Resume o corpo do `not-approved`. Tipo desconhecido **sem** data de fim vira
/// "advertida", não "banida": bloquear o launch por um rótulo que não
/// entendemos é pior que deixar o Roblox recusar.
fn classify(body: NotApproved) -> ModerationStatus {
    let Some(punishment) = non_empty(body.punishment_type_description) else {
        return ModerationStatus::clean();
    };
    let until = non_empty(body.end_date)
        .filter(|d| chrono::DateTime::parse_from_rfc3339(d).is_ok() || parse_roblox_date(d).is_some())
        .map(|d| normalize_date(&d));
    let note = non_empty(body.message_to_user);
    let kind = punishment.to_ascii_lowercase();
    let state = if kind.contains("delete") || kind.contains("terminat") {
        ModerationState::Terminated
    } else if kind.starts_with("ban") || until.is_some() {
        ModerationState::Banned
    } else {
        ModerationState::Warned
    };
    ModerationStatus { state, until, note, punishment: Some(punishment) }
}

/// O Roblox às vezes manda a data sem fuso (`2026-10-12T18:00:00.123`), que é UTC.
fn parse_roblox_date(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(value) {
        return Some(d.with_timezone(&chrono::Utc));
    }
    chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
        .ok()
        .map(|n| n.and_utc())
}

fn normalize_date(value: &str) -> String {
    parse_roblox_date(value)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_else(|| value.to_string())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PunishedUser {
    #[serde(default)]
    punished_user_id: Option<i64>,
}

/// De qual conta é a sessão, para conta moderada: as consultas de sempre
/// (`my/account/json`, `users/authenticated`) recusam, e a moderação diz o ID da
/// conta punida. `None` quando ela não diz (conta sem moderação, ou resposta
/// sem o campo). Leitura pura, como [`get_moderation_status`].
pub async fn punished_user_id(security_token: &str, client: &reqwest::Client) -> Result<Option<i64>, String> {
    let response = client
        .get(format!("{}/v1/not-approved", endpoints::host("usermoderation")))
        .header(COOKIE, cookie_header(security_token))
        .header("Accept", "application/json")
        .send_noting()
        .await
        .map_err(|e| http_client::describe_error(&e))?;
    if !response.status().is_success() {
        return Err(format!("Moderation check failed (status {})", response.status().as_u16()));
    }
    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read the moderation status: {e}"))?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    let body: PunishedUser =
        serde_json::from_str(&text).map_err(|_| "Unexpected moderation response".to_string())?;
    Ok(body.punished_user_id.filter(|id| *id > 0))
}

/// Lê a moderação da conta. Leitura pura: não renova sessão, não aceita termos,
/// não reativa conta.
pub async fn get_moderation_status(
    security_token: &str,
    client: &reqwest::Client,
) -> Result<ModerationStatus, ModerationFailure> {
    let response = client
        .get(format!("{}/v1/not-approved", endpoints::host("usermoderation")))
        .header(COOKIE, cookie_header(security_token))
        .header("Accept", "application/json")
        .send_noting()
        .await
        .map_err(|e| ModerationFailure::Other(http_client::describe_error(&e)))?;

    match response.status().as_u16() {
        200..=299 => {}
        401 => return Err(ModerationFailure::Unauthorized),
        429 => return Err(ModerationFailure::RateLimited),
        status => return Err(ModerationFailure::Other(format!("Moderation check failed (status {status})"))),
    }

    let text = response
        .text()
        .await
        .map_err(|e| ModerationFailure::Other(format!("Failed to read the moderation status: {e}")))?;
    if text.trim().is_empty() {
        return Ok(ModerationStatus::clean());
    }
    let body: NotApproved = serde_json::from_str(&text)
        .map_err(|_| ModerationFailure::Other("Unexpected moderation response".to_string()))?;
    Ok(classify(body))
}
