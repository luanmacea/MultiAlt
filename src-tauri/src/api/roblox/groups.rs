// Grupos (comunidades) do Roblox: busca, detalhes, entrada e conferência de
// participação. Ver docs/features/groups.md.
//
// Busca e detalhes são leitura pública (sem cookie). A entrada vai com o cookie
// da conta e **nunca** tenta resolver desafio: um 403 com `rblx-challenge-id`
// volta como `ChallengeRequired` e quem resolve é a pessoa, no navegador da
// conta.

/// Um grupo como a tela mostra: o que vem da busca e o que vem dos detalhes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupSummary {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub member_count: i64,
    /// `false` = o dono aprova cada pedido (a entrada vira pedido pendente).
    #[serde(default)]
    pub public_entry_allowed: bool,
    #[serde(default)]
    pub has_verified_badge: bool,
    /// Só vem nos detalhes; grupo trancado não aceita ninguém.
    #[serde(default)]
    pub is_locked: bool,
}

/// Uma página da busca. `next_cursor` vazio = acabou.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupSearchPage {
    pub groups: Vec<GroupSummary>,
    pub next_cursor: Option<String>,
}

/// O que aconteceu ao tentar entrar num grupo com uma conta.
///
/// Mapeamento (códigos confirmados na documentação oficial do
/// `POST groups.roblox.com/v1/groups/{groupId}/users`):
/// - 2xx → `Joined`, ou `Pending` se o grupo exige aprovação
///   (`publicEntryAllowed == false`: a resposta é a mesma, a participação fica
///   como pedido);
/// - 409 código 7 ("already requested to join") → `Pending`;
/// - 409 código 8 ("already a member") → `AlreadyMember`;
/// - 403 com o header `rblx-challenge-id` → `ChallengeRequired`, com o tipo
///   do header `rblx-challenge-type` (nem todo desafio é captcha: o dono viu
///   conta presa só no aviso de "Termos de Uso atualizados");
/// - qualquer outra coisa → `Failed` com a mensagem do Roblox (ex.: 403
///   código 6, limite de grupos; 429 código 10, tentativas demais).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupJoinOutcome {
    Joined,
    Pending,
    AlreadyMember,
    /// `challenge_type`: o `rblx-challenge-type` em minúsculas ("captcha",
    /// "twostepverification", "reauthentication", "chef", "proofofwork",
    /// "generic"...); ausente ou ilegível vira `"unknown"`.
    ChallengeRequired { challenge_type: String },
    Failed(String),
}

/// A tentativa inteira: o resultado e o que o Roblox respondeu, para o
/// diagnóstico (log e a linha da conta na tela). Nunca leva cookie nem token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupJoinAttempt {
    pub outcome: GroupJoinOutcome,
    /// `None` = nem chegou resposta (csrf ou rede falharam antes).
    pub http_status: Option<u16>,
    pub error_code: Option<i64>,
    /// A mensagem do Roblox (ou o corpo cru, se não era o JSON de erro padrão).
    pub error_message: Option<String>,
}

impl GroupJoinAttempt {
    fn without_response(message: String) -> Self {
        Self {
            outcome: GroupJoinOutcome::Failed(message),
            http_status: None,
            error_code: None,
            error_message: None,
        }
    }

    /// Uma linha curta para a tela: "HTTP 403 · challenge proofofwork · code
    /// 0". A mensagem do Roblox vai à parte (`error_message`), para a tela não
    /// repeti-la. `None` quando não há nada a dizer (2xx, ou nem houve
    /// resposta).
    pub fn diagnostic(&self) -> Option<String> {
        let status = self.http_status?;
        if (200..300).contains(&status) {
            return None;
        }
        let mut parts = vec![format!("HTTP {}", status)];
        if let GroupJoinOutcome::ChallengeRequired { challenge_type } = &self.outcome {
            parts.push(format!("challenge {}", challenge_type));
        }
        if let Some(code) = self.error_code {
            parts.push(format!("code {}", code));
        }
        Some(parts.join(" · "))
    }
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

/// Tipo do desafio pelo header `rblx-challenge-type`: minúsculas, só letras,
/// números, `-` e `_`, até 40 caracteres. Vazio/ausente → `"unknown"`.
pub fn normalize_challenge_type(header: Option<&str>) -> String {
    let cleaned: String = header
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(40)
        .collect();
    if cleaned.is_empty() {
        "unknown".to_string()
    } else {
        cleaned
    }
}

/// Grupos grandes e conhecidos da lista "Grupos populares" (campo de busca
/// vazio). O Roblox não tem endpoint de "top grupos", então a lista é
/// **curada à mão**: montada em 08/10/2026 com a busca pública
/// (`/v1/groups/search?keyword=<palavra>&limit=100` para roblox, games,
/// simulator, studio, official, adopt, tycoon, obby, anime, brookhaven,
/// bloxburg, piggy, jailbreak, pet, doors, gaming...), pegando os de mais
/// membros, com selo de verificado, sem nome ofensivo, de piada ou com cara de
/// golpe. A ordem aqui não importa: a tela ordena pelos membros de agora.
///
/// Para atualizar: repita as buscas, troque os ids e confira cada um em
/// `groups.roblox.com/v1/groups/<id>` (verificado, nome decente). Ver
/// docs/features/groups.md.
pub const POPULAR_GROUP_IDS: [i64; 24] = [
    1074557114, // SecretVerse Studio
    3461453,    // Nosniy Games
    15102943,   // Catalog Avatar Creator
    4705120,    // Scriptbloxian Studios
    3959677,    // BIG Games Pets
    35864728,   // Megastar Studios
    3049798,    // LSPLASH (Doors)
    4372130,    // Gamer Robot Inc (Blox Fruits)
    2919215,    // Sonar Studios
    2782840,    // Chillz Studios
    9642354,    // Byteonix
    1112558765, // Pick a Door!
    985535428,  // Wonder Towers
    295182,     // Uplift Games (Adopt Me!)
    7,          // Roblox
    3996161,    // Osyriss Studios
    6042520,    // Grandma's Favourite Games
    11378976,   // ADV Gamers Team
    2703304,    // BIG Games™
    3333298,    // Rumble Studios
    17264167,   // Dress To Impress Group
    2801805,    // The Builder's Legion
    340538789,  // Speedy Fun Games
    2726951,    // Car Crushers Community
];

/// Detalhes de vários grupos, `concurrency` por vez, sem cookie. Quem falhar
/// fica de fora (o resto aparece); a ordem é a dos membros, maior primeiro.
///
/// Por que não o `/v2/groups?groupIds=` em lote: ele não traz `memberCount`
/// nem `publicEntryAllowed` (conferido em 08/10/2026), e a tela precisa dos
/// dois.
pub async fn get_groups_by_members(group_ids: &[i64], concurrency: usize) -> Vec<GroupSummary> {
    use futures_util::stream::{self, StreamExt};
    let mut seen = std::collections::HashSet::new();
    let ids: Vec<i64> = group_ids.iter().copied().filter(|id| *id > 0 && seen.insert(*id)).collect();
    let mut groups: Vec<GroupSummary> = stream::iter(ids)
        .map(|id| async move { get_group(None, id).await.ok() })
        .buffer_unordered(concurrency.max(1))
        .filter_map(|group| async move { group })
        .collect()
        .await;
    groups.sort_by(|a, b| b.member_count.cmp(&a.member_count).then_with(|| a.id.cmp(&b.id)));
    groups
}

/// Onde a conta está em relação ao grupo, conferido depois.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupMembership {
    Member,
    Pending,
    NotMember,
}

/// Código 7 do join: a conta já pediu para entrar.
const GROUP_ERR_ALREADY_REQUESTED: i64 = 7;
/// Código 8 do join: a conta já é membro.
const GROUP_ERR_ALREADY_MEMBER: i64 = 8;

/// Id de grupo a partir do que a pessoa colou: número puro ou link
/// `roblox.com/groups/<id>/...` / `roblox.com/communities/<id>/...`.
/// `None` = é palavra de busca.
pub fn parse_group_reference(input: &str) -> Option<i64> {
    let text = input.trim();
    if text.is_empty() {
        return None;
    }
    if text.chars().all(|c| c.is_ascii_digit()) {
        return text.parse::<i64>().ok().filter(|id| *id > 0);
    }
    let lower = text.to_ascii_lowercase();
    let without_scheme = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .unwrap_or(&lower);
    let (host, path) = without_scheme.split_once('/')?;
    if host != "roblox.com" && !host.ends_with(".roblox.com") {
        return None;
    }
    let mut segments = path.split(['/', '?', '#']).filter(|s| !s.is_empty());
    while let Some(segment) = segments.next() {
        if segment == "groups" || segment == "communities" {
            return segments
                .next()
                .filter(|id| id.chars().all(|c| c.is_ascii_digit()))
                .and_then(|id| id.parse::<i64>().ok())
                .filter(|id| *id > 0);
        }
    }
    None
}

/// Primeiro `{ code, message }` do corpo de erro padrão do Roblox.
fn roblox_error(body: &str) -> Option<(i64, String)> {
    let parsed: serde_json::Value = serde_json::from_str(body).ok()?;
    let first = parsed.get("errors")?.as_array()?.first()?;
    let code = first.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    let message = first
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or_default()
        .to_string();
    Some((code, message))
}

fn with_optional_cookie(request: reqwest::RequestBuilder, cookie: Option<&str>) -> reqwest::RequestBuilder {
    match cookie {
        Some(token) if !token.trim().is_empty() => request.header(COOKIE, cookie_header(token)),
        _ => request,
    }
}

/// Busca de grupos por palavra (`/v1/groups/search`), 25 por página.
pub async fn search_groups(
    cookie: Option<&str>,
    keyword: &str,
    cursor: Option<&str>,
) -> Result<GroupSearchPage, String> {
    let client = http_client::client();
    let mut query: Vec<(&str, String)> = vec![
        ("keyword", keyword.trim().to_string()),
        ("limit", "25".to_string()),
        ("prioritizeExactMatch", "true".to_string()),
    ];
    if let Some(cursor) = cursor.filter(|c| !c.is_empty()) {
        query.push(("cursor", cursor.to_string()));
    }
    let url = format!("{}/v1/groups/search", endpoints::host("groups"));
    let response = send_with_retry(|| with_optional_cookie(client.get(&url).query(&query), cookie)).await?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(match roblox_error(&body) {
            Some((_, message)) if !message.is_empty() => message,
            _ => format!("Group search failed (status {})", status.as_u16()),
        });
    }
    let parsed: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Failed to parse group search: {}", e))?;
    let groups = parsed["data"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value::<GroupSummary>(item.clone()).ok())
                .collect()
        })
        .unwrap_or_default();
    let next_cursor = parsed["nextPageCursor"]
        .as_str()
        .filter(|c| !c.is_empty())
        .map(|c| c.to_string());
    Ok(GroupSearchPage { groups, next_cursor })
}

/// Detalhes de um grupo (`/v1/groups/<id>`).
pub async fn get_group(cookie: Option<&str>, group_id: i64) -> Result<GroupSummary, String> {
    let client = http_client::client();
    let url = format!("{}/v1/groups/{}", endpoints::host("groups"), group_id);
    let response = send_with_retry(|| with_optional_cookie(client.get(&url), cookie)).await?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(match roblox_error(&body) {
            Some((_, message)) if !message.is_empty() => message,
            _ => format!("Failed to read the group (status {})", status.as_u16()),
        });
    }
    serde_json::from_str::<GroupSummary>(&body).map_err(|e| format!("Failed to parse the group: {}", e))
}

/// Entra no grupo com a conta. Uma chamada só (mais o csrf): nada de refresh
/// de sessão, nada de repetir sozinho, nada de resolver desafio.
///
/// `requires_approval` vem dos detalhes do grupo (`!publicEntryAllowed`): o
/// Roblox responde 200 também quando a entrada vira pedido.
pub async fn join_group_outcome(security_token: &str, group_id: i64, requires_approval: bool) -> GroupJoinOutcome {
    join_group_attempt(security_token, group_id, requires_approval).await.outcome
}

/// Igual a `join_group_outcome`, com o que o Roblox respondeu (status, código
/// e mensagem) para o diagnóstico.
pub async fn join_group_attempt(security_token: &str, group_id: i64, requires_approval: bool) -> GroupJoinAttempt {
    let csrf = match crate::api::auth::get_csrf_token(security_token).await {
        Ok(token) => token,
        Err(e) => return GroupJoinAttempt::without_response(e),
    };
    let client = http_client::client();
    let request = client
        .post(format!("{}/v1/groups/{}/users", endpoints::host("groups"), group_id))
        .header(COOKIE, cookie_header(security_token))
        .header("Content-Type", "application/json")
        .body("{}");
    let response = match crate::api::auth::send_with_csrf_retry(request, &csrf).await {
        Ok(response) => response,
        Err(e) => return GroupJoinAttempt::without_response(e),
    };

    let status = response.status();
    let code = status.as_u16();
    // Desafio antes de qualquer outra leitura do 403: não é falha de csrf.
    let challenge = (code == 403 && response.headers().contains_key("rblx-challenge-id")).then(|| {
        normalize_challenge_type(
            response
                .headers()
                .get("rblx-challenge-type")
                .and_then(|value| value.to_str().ok()),
        )
    });
    if status.is_success() {
        let outcome = if requires_approval {
            GroupJoinOutcome::Pending
        } else {
            GroupJoinOutcome::Joined
        };
        return GroupJoinAttempt { outcome, http_status: Some(code), error_code: None, error_message: None };
    }

    let body = response.text().await.unwrap_or_default();
    let parsed = roblox_error(&body);
    let error_code = parsed.as_ref().map(|(c, _)| *c);
    let error_message = match &parsed {
        Some((_, message)) if !message.is_empty() => Some(message.clone()),
        _ if !body.trim().is_empty() => Some(truncate_chars(body.trim(), 500)),
        _ => None,
    };
    let outcome = if let Some(challenge_type) = challenge {
        GroupJoinOutcome::ChallengeRequired { challenge_type }
    } else {
        match error_code {
            Some(GROUP_ERR_ALREADY_REQUESTED) if code == 409 => GroupJoinOutcome::Pending,
            Some(GROUP_ERR_ALREADY_MEMBER) if code == 409 => GroupJoinOutcome::AlreadyMember,
            _ => GroupJoinOutcome::Failed(error_message.clone().unwrap_or_else(|| format!("status {}", code))),
        }
    };
    GroupJoinAttempt { outcome, http_status: Some(code), error_code, error_message }
}

/// Entrada simples (usada pelo painel de uma conta): só diz se deu certo.
/// Pedido pendente e "já é membro" contam como sucesso — a conta está (ou
/// estará) lá.
pub async fn join_group(security_token: &str, group_id: i64) -> Result<(), String> {
    match join_group_outcome(security_token, group_id, false).await {
        GroupJoinOutcome::Joined | GroupJoinOutcome::Pending | GroupJoinOutcome::AlreadyMember => Ok(()),
        GroupJoinOutcome::ChallengeRequired { challenge_type } if challenge_type == "captcha" => Err(
            "Failed to join group: Roblox asked for a captcha. Open the account in the browser and join there"
                .to_string(),
        ),
        GroupJoinOutcome::ChallengeRequired { challenge_type } => Err(format!(
            "Failed to join group: Roblox asked this account to confirm something ({}). Open it in the browser and accept what Roblox shows",
            challenge_type
        )),
        GroupJoinOutcome::Failed(message) => Err(format!("Failed to join group: {}", message)),
    }
}

/// A conta já está no grupo? Primeiro a lista pública de grupos da conta; se
/// não estiver lá, os pedidos pendentes (essa leitura precisa do cookie).
pub async fn group_membership(security_token: &str, user_id: i64, group_id: i64) -> Result<GroupMembership, String> {
    let client = http_client::client();

    let roles_url = format!("{}/v1/users/{}/groups/roles", endpoints::host("groups"), user_id);
    let response = send_with_retry(|| client.get(&roles_url)).await?;
    if !response.status().is_success() {
        return Err(format!("Failed to read the account's groups (status {})", response.status().as_u16()));
    }
    let roles: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse the account's groups: {}", e))?;
    let is_member = roles["data"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item["group"]["id"].as_i64() == Some(group_id)));
    if is_member {
        return Ok(GroupMembership::Member);
    }

    let pending_url = format!("{}/v1/user/groups/pending", endpoints::host("groups"));
    let response = send_with_retry(|| client.get(&pending_url).header(COOKIE, cookie_header(security_token))).await?;
    if !response.status().is_success() {
        return Err(format!("Failed to read the pending requests (status {})", response.status().as_u16()));
    }
    let pending: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse the pending requests: {}", e))?;
    let is_pending = pending["data"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item["id"].as_i64() == Some(group_id)));
    Ok(if is_pending {
        GroupMembership::Pending
    } else {
        GroupMembership::NotMember
    })
}

#[cfg(test)]
mod group_reference_tests {
    use super::*;

    #[test]
    fn a_bare_number_is_a_group_id() {
        assert_eq!(parse_group_reference("  7654321 "), Some(7654321));
        assert_eq!(parse_group_reference("0"), None);
    }

    #[test]
    fn group_and_community_links_give_the_id() {
        for link in [
            "https://www.roblox.com/groups/4199740/Some-Group#!/about",
            "https://www.roblox.com/communities/4199740/some-community",
            "www.roblox.com/communities/4199740",
            "http://roblox.com/groups/4199740",
            "https://web.roblox.com/groups/4199740/x?tab=about",
            "https://www.roblox.com/pt/communities/4199740/x",
        ] {
            assert_eq!(parse_group_reference(link), Some(4199740), "{link}");
        }
    }

    #[test]
    fn words_and_foreign_links_are_a_search() {
        assert_eq!(parse_group_reference("pet simulator"), None);
        assert_eq!(parse_group_reference("https://example.com/groups/123"), None);
        assert_eq!(parse_group_reference("https://www.roblox.com/games/123/x"), None);
        assert_eq!(parse_group_reference("https://www.roblox.com/groups/abc"), None);
        assert_eq!(parse_group_reference(""), None);
    }

    #[test]
    fn the_standard_error_body_gives_code_and_message() {
        assert_eq!(
            roblox_error(r#"{"errors":[{"code":8,"message":"You are already a member of this group."}]}"#),
            Some((8, "You are already a member of this group.".to_string()))
        );
        assert_eq!(roblox_error("group is locked"), None);
    }
}

#[cfg(test)]
mod group_join_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server, mount_csrf};
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, ResponseTemplate};

    async fn mount_join(token: &str, group_id: i64, response: ResponseTemplate) {
        Mock::given(method("POST"))
            .and(path(mock_path("groups", &format!("/v1/groups/{group_id}/users"))))
            .and(header("cookie", cookie_of(token)))
            .respond_with(response)
            .mount(mock_server().await)
            .await;
    }

    fn error_body(code: i64, message: &str) -> serde_json::Value {
        serde_json::json!({ "errors": [{ "code": code, "message": message }] })
    }

    #[tokio::test]
    async fn a_2xx_on_an_open_group_is_joined() {
        mount_csrf("grp-join-ok", "csrf-grp-join-ok").await;
        mount_join("grp-join-ok", 880_001, ResponseTemplate::new(200).set_body_json(serde_json::json!({}))).await;
        assert_eq!(join_group_outcome("grp-join-ok", 880_001, false).await, GroupJoinOutcome::Joined);
    }

    #[tokio::test]
    async fn a_2xx_on_a_group_that_needs_approval_is_pending() {
        mount_csrf("grp-join-approval", "csrf-grp-join-approval").await;
        mount_join("grp-join-approval", 880_002, ResponseTemplate::new(200)).await;
        assert_eq!(
            join_group_outcome("grp-join-approval", 880_002, true).await,
            GroupJoinOutcome::Pending
        );
    }

    #[tokio::test]
    async fn conflict_code_7_is_an_existing_request() {
        mount_csrf("grp-join-requested", "csrf-grp-join-requested").await;
        mount_join(
            "grp-join-requested",
            880_003,
            ResponseTemplate::new(409).set_body_json(error_body(7, "You have already requested to join this group.")),
        )
        .await;
        assert_eq!(
            join_group_outcome("grp-join-requested", 880_003, false).await,
            GroupJoinOutcome::Pending
        );
    }

    #[tokio::test]
    async fn conflict_code_8_is_already_a_member() {
        mount_csrf("grp-join-member", "csrf-grp-join-member").await;
        mount_join(
            "grp-join-member",
            880_004,
            ResponseTemplate::new(409).set_body_json(error_body(8, "You are already a member of this group.")),
        )
        .await;
        assert_eq!(
            join_group_outcome("grp-join-member", 880_004, false).await,
            GroupJoinOutcome::AlreadyMember
        );
    }

    #[tokio::test]
    async fn a_403_with_a_challenge_header_asks_for_a_captcha() {
        mount_csrf("grp-join-challenge", "csrf-grp-join-challenge").await;
        mount_join(
            "grp-join-challenge",
            880_005,
            ResponseTemplate::new(403)
                .insert_header("rblx-challenge-id", "x")
                .insert_header("rblx-challenge-type", "captcha"),
        )
        .await;
        assert_eq!(
            join_group_outcome("grp-join-challenge", 880_005, false).await,
            GroupJoinOutcome::ChallengeRequired { challenge_type: "captcha".to_string() }
        );
    }

    /// Nem todo desafio é captcha: o tipo vem do header, e o corpo do Roblox
    /// vai junto para o diagnóstico.
    #[tokio::test]
    async fn a_challenge_keeps_its_type_and_roblox_message() {
        mount_csrf("grp-join-pow", "csrf-grp-join-pow").await;
        mount_join(
            "grp-join-pow",
            880_011,
            ResponseTemplate::new(403)
                .insert_header("rblx-challenge-id", "x")
                .insert_header("rblx-challenge-type", "ProofOfWork")
                .set_body_json(error_body(0, "Challenge is required to authorize the request")),
        )
        .await;
        let attempt = join_group_attempt("grp-join-pow", 880_011, false).await;
        assert_eq!(
            attempt.outcome,
            GroupJoinOutcome::ChallengeRequired { challenge_type: "proofofwork".to_string() }
        );
        assert_eq!(attempt.http_status, Some(403));
        assert_eq!(attempt.error_code, Some(0));
        assert_eq!(
            attempt.error_message.as_deref(),
            Some("Challenge is required to authorize the request")
        );
        assert_eq!(
            attempt.diagnostic().as_deref(),
            Some("HTTP 403 · challenge proofofwork · code 0")
        );
    }

    #[tokio::test]
    async fn a_challenge_without_a_type_is_unknown() {
        mount_csrf("grp-join-notype", "csrf-grp-join-notype").await;
        mount_join(
            "grp-join-notype",
            880_012,
            ResponseTemplate::new(403).insert_header("rblx-challenge-id", "x"),
        )
        .await;
        assert_eq!(
            join_group_outcome("grp-join-notype", 880_012, false).await,
            GroupJoinOutcome::ChallengeRequired { challenge_type: "unknown".to_string() }
        );
    }

    #[test]
    fn challenge_types_are_normalized() {
        assert_eq!(normalize_challenge_type(Some(" Captcha ")), "captcha");
        assert_eq!(normalize_challenge_type(Some("twostepverification")), "twostepverification");
        assert_eq!(normalize_challenge_type(Some("<script>")), "script");
        assert_eq!(normalize_challenge_type(Some("")), "unknown");
        assert_eq!(normalize_challenge_type(None), "unknown");
        assert_eq!(normalize_challenge_type(Some(&"a".repeat(90))).len(), 40);
    }

    #[test]
    fn a_success_has_no_diagnostic_and_a_network_error_has_none_either() {
        let ok = GroupJoinAttempt {
            outcome: GroupJoinOutcome::Joined,
            http_status: Some(200),
            error_code: None,
            error_message: None,
        };
        assert_eq!(ok.diagnostic(), None);
        assert_eq!(GroupJoinAttempt::without_response("offline".into()).diagnostic(), None);
    }

    #[tokio::test]
    async fn any_other_refusal_fails_with_roblox_message() {
        mount_csrf("grp-join-max", "csrf-grp-join-max").await;
        mount_join(
            "grp-join-max",
            880_006,
            ResponseTemplate::new(403).set_body_json(error_body(6, "You are already in the maximum number of groups.")),
        )
        .await;
        let attempt = join_group_attempt("grp-join-max", 880_006, false).await;
        assert_eq!(
            attempt.outcome,
            GroupJoinOutcome::Failed("You are already in the maximum number of groups.".to_string())
        );
        assert_eq!(
            attempt.diagnostic().as_deref(),
            Some("HTTP 403 · code 6")
        );
    }

    /// Um 403 sem o header de desafio não é captcha (e um 409 de código
    /// desconhecido não é "já membro").
    #[tokio::test]
    async fn a_403_without_the_header_or_an_unknown_409_is_a_failure() {
        mount_csrf("grp-join-plain403", "csrf-grp-join-plain403").await;
        mount_join("grp-join-plain403", 880_007, ResponseTemplate::new(403)).await;
        assert_eq!(
            join_group_outcome("grp-join-plain403", 880_007, false).await,
            GroupJoinOutcome::Failed("status 403".to_string())
        );

        mount_csrf("grp-join-409", "csrf-grp-join-409").await;
        mount_join(
            "grp-join-409",
            880_008,
            ResponseTemplate::new(409).set_body_json(error_body(99, "Something else.")),
        )
        .await;
        assert_eq!(
            join_group_outcome("grp-join-409", 880_008, false).await,
            GroupJoinOutcome::Failed("Something else.".to_string())
        );
    }

    #[tokio::test]
    async fn the_simple_join_treats_pending_and_member_as_success() {
        mount_csrf("grp-simple-member", "csrf-grp-simple-member").await;
        mount_join(
            "grp-simple-member",
            880_009,
            ResponseTemplate::new(409).set_body_json(error_body(8, "member")),
        )
        .await;
        assert!(join_group("grp-simple-member", 880_009).await.is_ok());

        mount_csrf("grp-simple-challenge", "csrf-grp-simple-challenge").await;
        mount_join(
            "grp-simple-challenge",
            880_010,
            ResponseTemplate::new(403).insert_header("rblx-challenge-id", "x"),
        )
        .await;
        assert!(join_group("grp-simple-challenge", 880_010)
            .await
            .unwrap_err()
            .contains("confirm something (unknown)"));
    }

    /// Os populares: detalhes de cada id, quem falha fica de fora, maior
    /// primeiro, sem repetir.
    #[tokio::test]
    async fn popular_groups_are_sorted_by_members_and_skip_failures() {
        for (id, members) in [(880_201_i64, 50_i64), (880_202, 900), (880_203, 300)] {
            Mock::given(method("GET"))
                .and(path(mock_path("groups", &format!("/v1/groups/{id}"))))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "id": id, "name": format!("G{id}"), "memberCount": members,
                    "publicEntryAllowed": true, "hasVerifiedBadge": true
                })))
                .mount(mock_server().await)
                .await;
        }
        Mock::given(method("GET"))
            .and(path(mock_path("groups", "/v1/groups/880204")))
            .respond_with(ResponseTemplate::new(400).set_body_json(error_body(1, "Group is invalid or does not exist.")))
            .mount(mock_server().await)
            .await;

        let groups = get_groups_by_members(&[880_201, 880_204, 880_202, 880_203, 880_202], 2).await;
        let ids: Vec<i64> = groups.iter().map(|g| g.id).collect();
        assert_eq!(ids, vec![880_202, 880_203, 880_201]);
        assert!(groups.iter().all(|g| g.has_verified_badge));
    }

    #[test]
    fn the_curated_popular_list_has_no_repeats() {
        let unique: std::collections::HashSet<i64> = POPULAR_GROUP_IDS.iter().copied().collect();
        assert_eq!(unique.len(), POPULAR_GROUP_IDS.len());
        assert!(POPULAR_GROUP_IDS.iter().all(|id| *id > 0));
    }

    #[tokio::test]
    async fn search_reads_the_page_and_the_cursor() {
        Mock::given(method("GET"))
            .and(path(mock_path("groups", "/v1/groups/search")))
            .and(query_param("keyword", "grp-search-kw"))
            .and(query_param("limit", "25"))
            .and(query_param("prioritizeExactMatch", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "keyword": "grp-search-kw",
                "nextPageCursor": "next-1",
                "data": [
                    { "id": 11, "name": "Alpha", "description": "d", "memberCount": 120,
                      "publicEntryAllowed": true, "hasVerifiedBadge": true },
                    { "id": 12, "name": "Beta", "memberCount": 5, "publicEntryAllowed": false }
                ]
            })))
            .mount(mock_server().await)
            .await;

        let page = search_groups(None, " grp-search-kw ", None).await.expect("page");
        assert_eq!(page.next_cursor.as_deref(), Some("next-1"));
        assert_eq!(page.groups.len(), 2);
        assert_eq!(page.groups[0].name, "Alpha");
        assert!(page.groups[0].has_verified_badge && page.groups[0].public_entry_allowed);
        assert_eq!(page.groups[1].member_count, 5);
        assert!(!page.groups[1].public_entry_allowed);
    }

    #[tokio::test]
    async fn search_reports_roblox_message_on_a_refusal() {
        Mock::given(method("GET"))
            .and(path(mock_path("groups", "/v1/groups/search")))
            .and(query_param("keyword", "grp-search-bad"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(error_body(2, "Search term not appropriate for Roblox.")),
            )
            .mount(mock_server().await)
            .await;

        assert_eq!(
            search_groups(None, "grp-search-bad", None).await.unwrap_err(),
            "Search term not appropriate for Roblox."
        );
    }

    #[tokio::test]
    async fn group_details_are_read_by_id() {
        Mock::given(method("GET"))
            .and(path(mock_path("groups", "/v1/groups/880100")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 880100, "name": "Locked one", "memberCount": 9,
                "publicEntryAllowed": true, "isLocked": true, "hasVerifiedBadge": false,
                "owner": { "userId": 1 }
            })))
            .mount(mock_server().await)
            .await;

        let group = get_group(None, 880_100).await.expect("group");
        assert_eq!(group.name, "Locked one");
        assert!(group.is_locked);
    }
}

#[cfg(test)]
mod group_membership_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    async fn mount_roles(user_id: i64, group_ids: &[i64]) {
        let data: Vec<serde_json::Value> = group_ids
            .iter()
            .map(|id| serde_json::json!({ "group": { "id": id, "name": "g" }, "role": { "id": 1 } }))
            .collect();
        Mock::given(method("GET"))
            .and(path(mock_path("groups", &format!("/v1/users/{user_id}/groups/roles"))))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "data": data })))
            .mount(mock_server().await)
            .await;
    }

    async fn mount_pending(token: &str, group_ids: &[i64]) {
        let data: Vec<serde_json::Value> = group_ids.iter().map(|id| serde_json::json!({ "id": id })).collect();
        Mock::given(method("GET"))
            .and(path(mock_path("groups", "/v1/user/groups/pending")))
            .and(header("cookie", cookie_of(token)))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "data": data })))
            .mount(mock_server().await)
            .await;
    }

    #[tokio::test]
    async fn a_group_in_the_roles_list_is_a_membership() {
        mount_roles(770_001, &[5, 881_001]).await;
        assert_eq!(
            group_membership("grp-member-yes", 770_001, 881_001).await,
            Ok(GroupMembership::Member)
        );
    }

    #[tokio::test]
    async fn a_pending_request_is_reported_as_pending() {
        mount_roles(770_002, &[5]).await;
        mount_pending("grp-member-pending", &[881_002]).await;
        assert_eq!(
            group_membership("grp-member-pending", 770_002, 881_002).await,
            Ok(GroupMembership::Pending)
        );
    }

    #[tokio::test]
    async fn neither_list_means_not_a_member() {
        mount_roles(770_003, &[]).await;
        mount_pending("grp-member-no", &[]).await;
        assert_eq!(
            group_membership("grp-member-no", 770_003, 881_003).await,
            Ok(GroupMembership::NotMember)
        );
    }

    #[tokio::test]
    async fn a_failed_read_is_an_error_not_a_verdict() {
        Mock::given(method("GET"))
            .and(path(mock_path("groups", "/v1/users/770004/groups/roles")))
            .respond_with(ResponseTemplate::new(500))
            .mount(mock_server().await)
            .await;
        assert!(group_membership("grp-member-err", 770_004, 881_004).await.is_err());
    }
}
