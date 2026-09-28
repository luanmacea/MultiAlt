// Amigos online de uma conta.
//
// `GET friends/v1/users/{userId}/friends/online`, **com o cookie da conta**, devolve os
// amigos online já com `userPresenceType`, `lastLocation`, `placeId` e
// `gameInstanceId` — esse último é o job id, o que permite entrar no servidor
// do amigo. Desde out/2024 o payload **não** traz mais `name`/`displayName`,
// então os nomes são completados pelo lote `POST users/v1/users`
// (`lookup_user_names`, 100 ids por requisição).
//
// Regra de custo: é **uma** chamada por conta. O rate limit da API de amigos é
// agressivo e consultar amigo a amigo derruba a conta em poucos segundos.

/// Um amigo online, já pronto para a UI (contrato compartilhado com o
/// frontend: `OnlineFriend` em `src/types.ts`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnlineFriend {
    pub user_id: i64,
    pub name: String,
    pub display_name: String,
    pub presence_type: i32,
    pub last_location: String,
    pub place_id: Option<i64>,
    pub root_place_id: Option<i64>,
    pub game_id: Option<String>,
}

/// Tipo de presença como as duas rotas mandam: número (`2`) na rota antiga e
/// texto (`"InGame"`) na `users/{id}/friends/online`. Guardar os dois evita que
/// um payload em texto derrube o amigo inteiro na desserialização.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum PresenceType {
    Code(i32),
    /// Guardado como texto livre, e não como enum: um nome novo que a Roblox
    /// invente vira presença desconhecida em vez de derrubar o amigo inteiro
    /// na desserialização.
    Name(String),
}

impl PresenceType {
    /// Código numérico que o resto do app (e o frontend) usa.
    fn code(&self) -> i32 {
        match self {
            PresenceType::Code(code) => *code,
            PresenceType::Name(name) => match name.as_str() {
                "Online" => 1,
                "InGame" => 2,
                "InStudio" => 3,
                "Invisible" => 4,
                // "Offline" e qualquer nome novo.
                _ => 0,
            },
        }
    }
}

/// Item cru do payload, tolerante a tudo que a Roblox já mandou nessa rota:
/// campos ausentes, `null`, `userId` ou `id`, `gameInstanceId` ou `gameId` e a
/// forma antiga com os campos de presença aninhados em `userPresence`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawOnlineFriend {
    #[serde(rename = "userId", alias = "id")]
    user_id: Option<i64>,
    #[serde(
        rename = "userPresenceType",
        alias = "presenceType",
        alias = "UserPresenceType"
    )]
    presence_type: Option<PresenceType>,
    #[serde(rename = "lastLocation", alias = "LastLocation")]
    last_location: Option<String>,
    #[serde(rename = "placeId", alias = "PlaceId")]
    place_id: Option<i64>,
    #[serde(rename = "rootPlaceId", alias = "RootPlaceId")]
    root_place_id: Option<i64>,
    #[serde(rename = "gameInstanceId", alias = "gameId", alias = "GameId")]
    game_id: Option<String>,
    name: Option<String>,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "userPresence")]
    nested: Option<Box<RawOnlineFriend>>,
}

/// String vazia/só espaços vale como ausente (a API manda `""` no lugar de
/// `null` em alguns campos).
fn blank_to_none(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.trim().is_empty())
}

impl RawOnlineFriend {
    /// `None` quando o item não tem id — sem id não dá para preencher nome nem
    /// entrar no servidor, então ele é descartado em vez de virar um amigo 0.
    fn into_friend(self) -> Option<OnlineFriend> {
        let user_id = self.user_id?;
        let nested = self.nested.map(|boxed| *boxed).unwrap_or_default();

        Some(OnlineFriend {
            user_id,
            name: self.name.unwrap_or_default(),
            display_name: self.display_name.unwrap_or_default(),
            presence_type: self
                .presence_type
                .or(nested.presence_type)
                .map(|value| value.code())
                .unwrap_or(0),
            last_location: self
                .last_location
                .or(nested.last_location)
                .unwrap_or_default(),
            place_id: self.place_id.or(nested.place_id),
            root_place_id: self.root_place_id.or(nested.root_place_id),
            game_id: blank_to_none(self.game_id.or(nested.game_id)),
        })
    }
}

/// Aceita `{ "data": [...] }` (forma atual) e a lista crua, que a rota já
/// devolveu em versões antigas. Item que não parseia é pulado, nunca derruba a
/// lista inteira.
fn parse_online_friends(body: &serde_json::Value) -> Vec<OnlineFriend> {
    let items = body
        .get("data")
        .and_then(|value| value.as_array())
        .or_else(|| body.as_array());

    items
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value::<RawOnlineFriend>(item.clone()).ok())
                .filter_map(RawOnlineFriend::into_friend)
                .collect()
        })
        .unwrap_or_default()
}

/// Posição do amigo na ordem de exibição: quem dá para entrar primeiro.
///
/// `0` tem job id (entra direto), `1` está online no site (`presenceType == 1`)
/// e `2` é o resto (em jogo sem job id, no Studio, invisível...).
fn joinability_rank(friend: &OnlineFriend) -> u8 {
    if friend.game_id.is_some() {
        0
    } else if friend.presence_type == 1 {
        1
    } else {
        2
    }
}

/// Nome usado na ordenação: o de exibição quando existe, senão o usuário.
fn display_key(friend: &OnlineFriend) -> String {
    let base = if friend.display_name.trim().is_empty() {
        &friend.name
    } else {
        &friend.display_name
    };
    base.to_lowercase()
}

/// Ordena por grupo de "entrabilidade" e, dentro de cada grupo, por nome (sem
/// diferenciar maiúsculas). O id desempata para a ordem ser determinística.
fn sort_online_friends(friends: &mut [OnlineFriend]) {
    friends.sort_by(|a, b| {
        joinability_rank(a)
            .cmp(&joinability_rank(b))
            .then_with(|| display_key(a).cmp(&display_key(b)))
            .then_with(|| a.user_id.cmp(&b.user_id))
    });
}

/// Completa `name`/`displayName` de quem veio sem eles pelo lote de usuários.
///
/// Best-effort: se o lote falhar, o amigo fica só com o id e a lista continua
/// utilizável — não vale derrubar a tela inteira por causa dos nomes.
async fn fill_missing_names(friends: &mut [OnlineFriend]) {
    let missing: Vec<i64> = friends
        .iter()
        .filter(|friend| friend.name.trim().is_empty() || friend.display_name.trim().is_empty())
        .map(|friend| friend.user_id)
        .collect();
    if missing.is_empty() {
        return;
    }

    let Ok(infos) = lookup_user_names(&missing).await else {
        return;
    };

    for friend in friends.iter_mut() {
        let Some(info) = infos.iter().find(|info| info.id == friend.user_id) else {
            continue;
        };
        if friend.name.trim().is_empty() {
            friend.name = info.name.clone();
        }
        if friend.display_name.trim().is_empty() {
            friend.display_name = if info.display_name.trim().is_empty() {
                info.name.clone()
            } else {
                info.display_name.clone()
            };
        }
    }
}

/// Resgate para quem está em jogo mas veio sem `gameInstanceId`: a presença
/// **com cookie** ainda devolve o `gameId` (sem cookie ele vem sempre nulo).
///
/// Uma chamada extra por conta e só quando falta job id — sem isso o amigo
/// apareceria como "não dá para entrar". Best-effort, igual aos nomes.
async fn fill_missing_game_ids(security_token: &str, friends: &mut [OnlineFriend]) {
    let missing: Vec<i64> = friends
        .iter()
        .filter(|friend| friend.game_id.is_none() && friend.place_id.is_some())
        .map(|friend| friend.user_id)
        .collect();
    if missing.is_empty() {
        return;
    }

    let Ok(presences) = get_presence_as(Some(security_token), &missing).await else {
        return;
    };

    for friend in friends.iter_mut() {
        let Some(presence) = presences
            .iter()
            .find(|presence| presence.user_id == friend.user_id)
        else {
            continue;
        };
        if friend.game_id.is_none() {
            friend.game_id = blank_to_none(presence.game_id.clone());
        }
        if friend.root_place_id.is_none() {
            friend.root_place_id = presence.root_place_id;
        }
    }
}

/// Amigos online de uma conta, já com nomes e ordenados.
///
/// A rota é `users/{userId}/friends/online`. A antiga (`my/friends/online`) foi
/// **removida** pelo Roblox e passou a responder 404 para todo mundo — era o
/// motivo de a aba Friends mostrar "status 404" em todas as contas.
pub async fn get_online_friends(
    security_token: &str,
    user_id: i64,
) -> Result<Vec<OnlineFriend>, String> {
    if user_id <= 0 {
        return Err("Conta sem UserID: refaça o login dela".to_string());
    }
    let client = http_client::client();
    let url = format!(
        "{}/v1/users/{}/friends/online",
        endpoints::host("friends"),
        user_id
    );

    let response = send_with_retry(|| {
        client
            .get(&url)
            .header(COOKIE, cookie_header(security_token))
    })
    .await?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to get online friends (status {})",
            response.status().as_u16()
        ));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse online friends: {}", e))?;

    let mut friends = parse_online_friends(&body);
    fill_missing_game_ids(security_token, &mut friends).await;
    fill_missing_names(&mut friends).await;
    sort_online_friends(&mut friends);
    Ok(friends)
}

/// Parsing e ordenação, sem rede.
#[cfg(test)]
mod friends_online_tests {
    use super::*;

    fn friend(user_id: i64, display_name: &str, presence_type: i32, game_id: Option<&str>) -> OnlineFriend {
        OnlineFriend {
            user_id,
            name: format!("user{user_id}"),
            display_name: display_name.to_string(),
            presence_type,
            last_location: String::new(),
            place_id: None,
            root_place_id: None,
            game_id: game_id.map(|id| id.to_string()),
        }
    }

    #[test]
    fn a_full_entry_is_parsed_field_by_field() {
        let body = serde_json::json!({
            "data": [{
                "userId": 42,
                "userPresenceType": 2,
                "lastLocation": "Jailbreak",
                "placeId": 606849621,
                "rootPlaceId": 606849620,
                "gameInstanceId": "job-abc",
                "universeId": 606
            }]
        });

        let friends = parse_online_friends(&body);
        assert_eq!(friends.len(), 1);
        assert_eq!(friends[0].user_id, 42);
        assert_eq!(friends[0].presence_type, 2);
        assert_eq!(friends[0].last_location, "Jailbreak");
        assert_eq!(friends[0].place_id, Some(606849621));
        assert_eq!(friends[0].root_place_id, Some(606849620));
        assert_eq!(friends[0].game_id.as_deref(), Some("job-abc"));
        // A rota parou de mandar nome: fica vazio até o lote completar.
        assert!(friends[0].name.is_empty());
        assert!(friends[0].display_name.is_empty());
    }

    #[test]
    fn missing_and_null_fields_fall_back_to_defaults() {
        let body = serde_json::json!({
            "data": [
                { "userId": 1 },
                { "userId": 2, "lastLocation": null, "placeId": null, "gameInstanceId": null }
            ]
        });

        let friends = parse_online_friends(&body);
        assert_eq!(friends.len(), 2);
        for friend in &friends {
            assert_eq!(friend.presence_type, 0);
            assert!(friend.last_location.is_empty());
            assert!(friend.place_id.is_none());
            assert!(friend.root_place_id.is_none());
            assert!(friend.game_id.is_none());
        }
    }

    #[test]
    fn an_entry_without_a_game_instance_id_has_a_null_game_id() {
        let body = serde_json::json!({
            "data": [
                { "userId": 3, "userPresenceType": 1, "lastLocation": "Website" },
                { "userId": 4, "userPresenceType": 2, "gameInstanceId": "   " }
            ]
        });

        let friends = parse_online_friends(&body);
        assert!(friends[0].game_id.is_none());
        // String vazia/em branco conta como ausente, não como job id.
        assert!(friends[1].game_id.is_none());
    }

    #[test]
    fn an_entry_without_an_id_is_skipped() {
        let body = serde_json::json!({
            "data": [{ "userPresenceType": 2 }, { "userId": 9, "userPresenceType": 2 }]
        });

        let friends = parse_online_friends(&body);
        assert_eq!(friends.len(), 1);
        assert_eq!(friends[0].user_id, 9);
    }

    /// Formas alternativas que a rota já devolveu: lista crua, `id` em vez de
    /// `userId`, `gameId` em vez de `gameInstanceId` e presença aninhada.
    /// Forma da rota `users/{id}/friends/online`: presença aninhada em
    /// `userPresence`, com as chaves em maiúscula e o tipo como **texto**.
    /// Foi o que quebrou quando a rota `my/friends/online` saiu do ar.
    #[test]
    fn the_current_route_payload_with_a_named_presence_is_understood() {
        let body = serde_json::json!({
            "data": [{
                "id": 7401,
                "name": "amigo",
                "displayName": "Amigo",
                "userPresence": {
                    "UserPresenceType": "InGame",
                    "lastLocation": "Jailbreak",
                    "placeId": 606,
                    "rootPlaceId": 606849621,
                    "gameId": "job-7401"
                }
            }]
        });
        let friends = parse_online_friends(&body);
        assert_eq!(friends.len(), 1);
        assert_eq!(friends[0].user_id, 7401);
        assert_eq!(friends[0].presence_type, 2, "InGame tem que virar 2");
        assert_eq!(friends[0].game_id.as_deref(), Some("job-7401"));
        assert_eq!(friends[0].root_place_id, Some(606849621));
        assert_eq!(friends[0].last_location, "Jailbreak");
    }

    #[test]
    fn every_named_presence_maps_to_its_code() {
        for (name, code) in [
            ("Offline", 0),
            ("Online", 1),
            ("InGame", 2),
            ("InStudio", 3),
            ("Invisible", 4),
        ] {
            let body = serde_json::json!({
                "data": [{ "id": 1, "userPresence": { "UserPresenceType": name } }]
            });
            assert_eq!(parse_online_friends(&body)[0].presence_type, code, "{}", name);
        }
    }

    /// Um tipo de presença desconhecido não pode derrubar o amigo inteiro.
    #[test]
    fn an_unknown_presence_name_leaves_the_friend_in_the_list() {
        let body = serde_json::json!({
            "data": [{ "id": 9, "userPresence": { "UserPresenceType": "Teleporting" } }]
        });
        let friends = parse_online_friends(&body);
        assert_eq!(friends.len(), 1);
        assert_eq!(friends[0].presence_type, 0);
    }

    #[test]
    fn legacy_payload_shapes_are_still_understood() {
        let body = serde_json::json!([{
            "id": 77,
            "name": "legacy",
            "displayName": "Legacy",
            "userPresence": {
                "userPresenceType": 2,
                "lastLocation": "Old Place",
                "placeId": 10,
                "rootPlaceId": 11,
                "gameId": "job-legacy"
            }
        }]);

        let friends = parse_online_friends(&body);
        assert_eq!(friends.len(), 1);
        assert_eq!(friends[0].user_id, 77);
        assert_eq!(friends[0].name, "legacy");
        assert_eq!(friends[0].presence_type, 2);
        assert_eq!(friends[0].last_location, "Old Place");
        assert_eq!(friends[0].place_id, Some(10));
        assert_eq!(friends[0].root_place_id, Some(11));
        assert_eq!(friends[0].game_id.as_deref(), Some("job-legacy"));
    }

    #[test]
    fn a_body_without_a_list_is_empty() {
        assert!(parse_online_friends(&serde_json::json!({})).is_empty());
        assert!(parse_online_friends(&serde_json::json!({ "data": null })).is_empty());
    }

    #[test]
    fn joinable_friends_come_first_then_website_then_the_rest() {
        let mut friends = vec![
            friend(1, "Zed", 0, None),           // resto
            friend(2, "Bob", 1, None),           // online no site
            friend(3, "Yan", 2, Some("job-3")),  // dá para entrar
            friend(4, "Ana", 2, None),           // em jogo sem job id → resto
            friend(5, "Cleo", 1, Some("job-5")), // dá para entrar
        ];

        sort_online_friends(&mut friends);

        let order: Vec<i64> = friends.iter().map(|f| f.user_id).collect();
        // (job id: Cleo, Yan) → (site: Bob) → (resto: Ana, Zed)
        assert_eq!(order, vec![5, 3, 2, 4, 1]);
    }

    #[test]
    fn names_sort_case_insensitively_inside_a_group() {
        let mut friends = vec![
            friend(1, "banana", 1, None),
            friend(2, "Apple", 1, None),
            friend(3, "Cherry", 1, None),
        ];

        sort_online_friends(&mut friends);

        let order: Vec<&str> = friends.iter().map(|f| f.display_name.as_str()).collect();
        assert_eq!(order, vec!["Apple", "banana", "Cherry"]);
    }

    /// Sem nome de exibição a ordenação cai no nome de usuário, e ids iguais em
    /// nome desempatam pelo id para a ordem não oscilar entre chamadas.
    #[test]
    fn the_username_is_the_fallback_sort_key() {
        let mut friends = vec![
            OnlineFriend { display_name: String::new(), ..friend(30, "", 1, None) },
            OnlineFriend { display_name: String::new(), ..friend(20, "", 1, None) },
        ];

        sort_online_friends(&mut friends);

        // "user20" < "user30"
        assert_eq!(friends[0].user_id, 20);
        assert_eq!(friends[1].user_id, 30);
    }
}

/// A rota de amigos online com HTTP mockado.
#[cfg(test)]
mod friends_online_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server};
    use wiremock::matchers::{body_string_contains, header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    /// Conta dona da lista nos testes desta suíte.
    const ACCOUNT_ID: i64 = 1001;

    /// Rota atual. A antiga (`my/friends/online`) foi removida pelo Roblox.
    fn route(user_id: i64) -> String {
        format!("/v1/users/{}/friends/online", user_id)
    }

    #[tokio::test]
    async fn the_call_carries_the_account_cookie_and_parses_the_list() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("friends", &route(ACCOUNT_ID))))
            .and(header("cookie", cookie_of("friends-ok")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{
                    "userId": 7201,
                    "userPresenceType": 2,
                    "lastLocation": "Jailbreak",
                    "placeId": 606,
                    "rootPlaceId": 606,
                    "gameInstanceId": "job-7201"
                }]
            })))
            .mount(server)
            .await;

        Mock::given(method("POST"))
            .and(path(mock_path("users", "/v1/users")))
            .and(body_string_contains("7201"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "id": 7201, "name": "jailer", "displayName": "Jailer" }]
            })))
            .mount(server)
            .await;

        let friends = get_online_friends("friends-ok", ACCOUNT_ID).await.expect("friends");
        assert_eq!(friends.len(), 1);
        assert_eq!(friends[0].user_id, 7201);
        assert_eq!(friends[0].name, "jailer");
        assert_eq!(friends[0].display_name, "Jailer");
        assert_eq!(friends[0].game_id.as_deref(), Some("job-7201"));
    }

    /// O endpoint de amigos não manda mais nome; o lote de usuários preenche
    /// todo mundo de uma vez (uma requisição, não uma por amigo).
    #[tokio::test]
    async fn the_batch_lookup_fills_every_missing_name_at_once() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("friends", &route(ACCOUNT_ID))))
            .and(header("cookie", cookie_of("friends-names")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    { "userId": 7301, "userPresenceType": 1 },
                    { "userId": 7302, "userPresenceType": 1 }
                ]
            })))
            .mount(server)
            .await;

        let batch = Mock::given(method("POST"))
            .and(path(mock_path("users", "/v1/users")))
            .and(body_string_contains("7301"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    { "id": 7301, "name": "alpha", "displayName": "Alpha" },
                    // Sem displayName: cai no nome de usuário.
                    { "id": 7302, "name": "beta", "displayName": "" }
                ]
            })))
            .expect(1)
            .named("one batch lookup for both friends")
            .mount_as_scoped(server)
            .await;

        let friends = get_online_friends("friends-names", ACCOUNT_ID).await.expect("friends");
        assert_eq!(friends.len(), 2);
        assert_eq!(friends[0].name, "alpha");
        assert_eq!(friends[0].display_name, "Alpha");
        assert_eq!(friends[1].name, "beta");
        assert_eq!(friends[1].display_name, "beta");

        drop(batch);
    }

    /// Sem `gameInstanceId` e sem `placeId` não há o que resgatar: `gameId`
    /// fica nulo e nenhuma chamada de presença é feita.
    #[tokio::test]
    async fn a_friend_without_a_game_instance_id_keeps_a_null_game_id() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("friends", &route(ACCOUNT_ID))))
            .and(header("cookie", cookie_of("friends-nojob")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "userId": 7401, "userPresenceType": 1, "lastLocation": "Website" }]
            })))
            .mount(server)
            .await;

        Mock::given(method("POST"))
            .and(path(mock_path("users", "/v1/users")))
            .and(body_string_contains("7401"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "id": 7401, "name": "lonely", "displayName": "Lonely" }]
            })))
            .mount(server)
            .await;

        let friends = get_online_friends("friends-nojob", ACCOUNT_ID).await.expect("friends");
        assert_eq!(friends.len(), 1);
        assert!(friends[0].game_id.is_none());
        assert!(friends[0].root_place_id.is_none());
    }

    /// Em jogo sem job id: a presença **com cookie** devolve o `gameId` e o
    /// amigo passa a ser entrável.
    #[tokio::test]
    async fn presence_rescues_a_missing_game_id_for_a_friend_in_game() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("friends", &route(ACCOUNT_ID))))
            .and(header("cookie", cookie_of("friends-rescue")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{
                    "userId": 7501,
                    "userPresenceType": 2,
                    "lastLocation": "Some Game",
                    "placeId": 5150
                }]
            })))
            .mount(server)
            .await;

        Mock::given(method("POST"))
            .and(path(mock_path("presence", "/v1/presence/users")))
            .and(header("cookie", cookie_of("friends-rescue")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "userPresences": [{
                    "userPresenceType": 2,
                    "userId": 7501,
                    "placeId": 5150,
                    "rootPlaceId": 5151,
                    "gameId": "job-rescued"
                }]
            })))
            .mount(server)
            .await;

        Mock::given(method("POST"))
            .and(path(mock_path("users", "/v1/users")))
            .and(body_string_contains("7501"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "id": 7501, "name": "rescued", "displayName": "Rescued" }]
            })))
            .mount(server)
            .await;

        let friends = get_online_friends("friends-rescue", ACCOUNT_ID).await.expect("friends");
        assert_eq!(friends[0].game_id.as_deref(), Some("job-rescued"));
        assert_eq!(friends[0].root_place_id, Some(5151));
    }

    #[tokio::test]
    async fn an_http_failure_reports_the_status() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("friends", &route(ACCOUNT_ID))))
            .and(header("cookie", cookie_of("friends-401")))
            .respond_with(ResponseTemplate::new(401))
            .mount(server)
            .await;

        assert_eq!(
            get_online_friends("friends-401", ACCOUNT_ID).await.unwrap_err(),
            "Failed to get online friends (status 401)"
        );
    }

    #[tokio::test]
    async fn a_malformed_body_is_a_parse_error() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("friends", &route(ACCOUNT_ID))))
            .and(header("cookie", cookie_of("friends-garbage")))
            .respond_with(ResponseTemplate::new(200).set_body_raw("nope", "text/plain"))
            .mount(server)
            .await;

        let err = get_online_friends("friends-garbage", ACCOUNT_ID).await.unwrap_err();
        assert!(
            err.starts_with("Failed to parse online friends: "),
            "unexpected: {}",
            err
        );
    }

    /// Falha no lote de nomes não derruba a lista: o amigo fica sem nome, mas
    /// com id e job id — o suficiente para entrar no servidor.
    #[tokio::test]
    async fn a_failing_name_lookup_still_returns_the_friends() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("friends", &route(ACCOUNT_ID))))
            .and(header("cookie", cookie_of("friends-noname")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "userId": 7601, "userPresenceType": 2, "gameInstanceId": "job-7601" }]
            })))
            .mount(server)
            .await;

        Mock::given(method("POST"))
            .and(path(mock_path("users", "/v1/users")))
            .and(body_string_contains("7601"))
            .respond_with(ResponseTemplate::new(500))
            .mount(server)
            .await;

        let friends = get_online_friends("friends-noname", ACCOUNT_ID).await.expect("friends");
        assert_eq!(friends.len(), 1);
        assert!(friends[0].name.is_empty());
        assert_eq!(friends[0].game_id.as_deref(), Some("job-7601"));
    }
}
