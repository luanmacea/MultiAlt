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
/// - 403 com o header `rblx-challenge-id` → `ChallengeRequired`;
/// - qualquer outra coisa → `Failed` com a mensagem do Roblox (ex.: 403
///   código 6, limite de grupos; 429 código 10, tentativas demais).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupJoinOutcome {
    Joined,
    Pending,
    AlreadyMember,
    ChallengeRequired,
    Failed(String),
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
    let csrf = match crate::api::auth::get_csrf_token(security_token).await {
        Ok(token) => token,
        Err(e) => return GroupJoinOutcome::Failed(e),
    };
    let client = http_client::client();
    let request = client
        .post(format!("{}/v1/groups/{}/users", endpoints::host("groups"), group_id))
        .header(COOKIE, cookie_header(security_token))
        .header("Content-Type", "application/json")
        .body("{}");
    let response = match crate::api::auth::send_with_csrf_retry(request, &csrf).await {
        Ok(response) => response,
        Err(e) => return GroupJoinOutcome::Failed(e),
    };

    let status = response.status();
    // Desafio antes de qualquer outra leitura do 403: não é falha de csrf.
    if status.as_u16() == 403 && response.headers().contains_key("rblx-challenge-id") {
        return GroupJoinOutcome::ChallengeRequired;
    }
    if status.is_success() {
        return if requires_approval {
            GroupJoinOutcome::Pending
        } else {
            GroupJoinOutcome::Joined
        };
    }

    let body = response.text().await.unwrap_or_default();
    match roblox_error(&body) {
        Some((GROUP_ERR_ALREADY_REQUESTED, _)) if status.as_u16() == 409 => GroupJoinOutcome::Pending,
        Some((GROUP_ERR_ALREADY_MEMBER, _)) if status.as_u16() == 409 => GroupJoinOutcome::AlreadyMember,
        Some((_, message)) if !message.is_empty() => GroupJoinOutcome::Failed(message),
        _ if !body.trim().is_empty() => GroupJoinOutcome::Failed(body),
        _ => GroupJoinOutcome::Failed(format!("status {}", status.as_u16())),
    }
}

/// Entrada simples (usada pelo painel de uma conta): só diz se deu certo.
/// Pedido pendente e "já é membro" contam como sucesso — a conta está (ou
/// estará) lá.
pub async fn join_group(security_token: &str, group_id: i64) -> Result<(), String> {
    match join_group_outcome(security_token, group_id, false).await {
        GroupJoinOutcome::Joined | GroupJoinOutcome::Pending | GroupJoinOutcome::AlreadyMember => Ok(()),
        GroupJoinOutcome::ChallengeRequired => {
            Err("Failed to join group: Roblox asked for a verification (captcha)".to_string())
        }
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
