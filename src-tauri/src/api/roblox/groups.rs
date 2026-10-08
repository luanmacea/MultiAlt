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
            GroupJoinOutcome::ChallengeRequired
        );
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
        assert_eq!(
            join_group_outcome("grp-join-max", 880_006, false).await,
            GroupJoinOutcome::Failed("You are already in the maximum number of groups.".to_string())
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
            .contains("captcha"));
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
