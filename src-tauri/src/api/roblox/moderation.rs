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

#[cfg(test)]
mod moderation_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    async fn mount(token: &str, response: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path(mock_path("usermoderation", "/v1/not-approved")))
            .and(header("cookie", cookie_of(token)))
            .respond_with(response)
            .mount(mock_server().await)
            .await;
    }

    async fn check(token: &str) -> Result<ModerationStatus, ModerationFailure> {
        get_moderation_status(token, &http_client::client()).await
    }

    #[tokio::test]
    async fn an_empty_object_is_a_clean_account() {
        mount("mod-clean", ResponseTemplate::new(200).set_body_json(serde_json::json!({}))).await;
        assert_eq!(check("mod-clean").await.unwrap(), ModerationStatus::clean());
    }

    #[tokio::test]
    async fn a_ban_with_an_end_date() {
        mount(
            "mod-ban",
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "punishmentTypeDescription": "Ban 3 Days",
                "beginDate": "2026-10-09T12:00:00.000Z",
                "endDate": "2026-10-12T12:00:00.000Z",
                "messageToUser": "Harassment",
                "punishedUserId": 1
            })),
        )
        .await;
        let status = check("mod-ban").await.unwrap();
        assert_eq!(status.state, ModerationState::Banned);
        assert_eq!(status.until.as_deref(), Some("2026-10-12T12:00:00Z"));
        assert_eq!(status.note.as_deref(), Some("Harassment"));
        assert_eq!(status.punishment.as_deref(), Some("Ban 3 Days"));
    }

    #[tokio::test]
    async fn a_warning() {
        mount(
            "mod-warn",
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "punishmentTypeDescription": "Warn",
                "endDate": null,
                "messageToUser": "Be nice"
            })),
        )
        .await;
        let status = check("mod-warn").await.unwrap();
        assert_eq!(status.state, ModerationState::Warned);
        assert_eq!(status.until, None);
    }

    #[tokio::test]
    async fn a_deleted_account_is_terminated() {
        mount(
            "mod-delete",
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "punishmentTypeDescription": "Delete",
                "messageToUser": ""
            })),
        )
        .await;
        let status = check("mod-delete").await.unwrap();
        assert_eq!(status.state, ModerationState::Terminated);
        assert_eq!(status.note, None, "an empty note is no note");
    }

    /// 429 é "tente depois": nunca pode virar banida (nem inválida).
    #[tokio::test]
    async fn a_429_is_rate_limited_not_banned() {
        mount("mod-429", ResponseTemplate::new(429)).await;
        assert_eq!(check("mod-429").await.unwrap_err(), ModerationFailure::RateLimited);
    }

    #[tokio::test]
    async fn a_401_is_a_dead_cookie_not_a_ban() {
        mount("mod-401", ResponseTemplate::new(401)).await;
        assert_eq!(check("mod-401").await.unwrap_err(), ModerationFailure::Unauthorized);
    }

    #[tokio::test]
    async fn a_server_error_is_reported_as_is() {
        mount("mod-503", ResponseTemplate::new(503)).await;
        assert!(matches!(check("mod-503").await.unwrap_err(), ModerationFailure::Other(_)));
    }

    #[test]
    fn an_unknown_type_without_an_end_date_is_only_a_warning() {
        let status = classify(NotApproved {
            punishment_type_description: Some("Something New".into()),
            end_date: None,
            message_to_user: None,
        });
        assert_eq!(status.state, ModerationState::Warned);
    }

    #[test]
    fn a_date_without_a_time_zone_is_read_as_utc() {
        let status = classify(NotApproved {
            punishment_type_description: Some("Ban 1 Day".into()),
            end_date: Some("2026-10-10T08:30:00.123".into()),
            message_to_user: None,
        });
        assert_eq!(status.until.as_deref(), Some("2026-10-10T08:30:00Z"));
    }

    #[test]
    fn a_garbage_end_date_is_dropped() {
        let status = classify(NotApproved {
            punishment_type_description: Some("Ban 7 Days".into()),
            end_date: Some("soon".into()),
            message_to_user: None,
        });
        assert_eq!(status.state, ModerationState::Banned);
        assert_eq!(status.until, None);
    }
}
