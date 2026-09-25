// Amigos online de uma conta.
//
// `GET friends/v1/my/friends/online`, **com o cookie da conta**, devolve os
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

/// Item cru do payload, tolerante a tudo que a Roblox já mandou nessa rota:
/// campos ausentes, `null`, `userId` ou `id`, `gameInstanceId` ou `gameId` e a
/// forma antiga com os campos de presença aninhados em `userPresence`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawOnlineFriend {
    #[serde(rename = "userId", alias = "id")]
    user_id: Option<i64>,
    #[serde(rename = "userPresenceType", alias = "presenceType")]
    presence_type: Option<i32>,
    #[serde(rename = "lastLocation")]
    last_location: Option<String>,
    #[serde(rename = "placeId")]
    place_id: Option<i64>,
    #[serde(rename = "rootPlaceId")]
    root_place_id: Option<i64>,
    #[serde(rename = "gameInstanceId", alias = "gameId")]
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
            presence_type: self.presence_type.or(nested.presence_type).unwrap_or(0),
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

/// Amigos online da conta dona do cookie, já com nomes e ordenados.
pub async fn get_online_friends(security_token: &str) -> Result<Vec<OnlineFriend>, String> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/my/friends/online", endpoints::host("friends"));

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
