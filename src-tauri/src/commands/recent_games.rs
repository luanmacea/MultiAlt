// Aba Recentes da Escolher jogo: os jogos em que **cada conta** entrou por
// último (o "Continuar" da página inicial do Roblox), além da lista dos jogos
// abertos pelo MultiAlt. Só leitura, uma conta por vez, sem refresh de sessão:
// cookie vencido só falha aquela conta. Ver docs/features/server-list.md.

/// Quantos jogos por conta (o "Continuar" do Roblox traz uns 10-20).
const RECENT_GAMES_PER_ACCOUNT: usize = 10;

/// Pausa entre uma conta e a próxima.
const RECENT_GAMES_DELAY: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountRecentGame {
    place_id: i64,
    universe_id: i64,
    name: String,
    icon_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountRecentGames {
    user_id: i64,
    games: Vec<AccountRecentGame>,
    error: Option<String>,
}

/// Junta o "Continuar" de cada conta (`universes`) com os nomes e ícones. Jogo
/// sem nome/lugar conhecido fica de fora; a ordem é a do Roblox.
fn build_recent_games(
    universes: Vec<(i64, Result<Vec<i64>, String>)>,
    summaries: &[api::roblox::UniverseSummary],
    icons: &std::collections::HashMap<i64, String>,
) -> Vec<AccountRecentGames> {
    let by_universe: std::collections::HashMap<i64, &api::roblox::UniverseSummary> =
        summaries.iter().map(|s| (s.universe_id, s)).collect();
    universes
        .into_iter()
        .map(|(user_id, result)| match result {
            Ok(ids) => AccountRecentGames {
                user_id,
                games: ids
                    .iter()
                    .filter_map(|id| by_universe.get(id))
                    .take(RECENT_GAMES_PER_ACCOUNT)
                    .map(|game| AccountRecentGame {
                        place_id: game.root_place_id,
                        universe_id: game.universe_id,
                        name: game.name.clone(),
                        icon_url: icons.get(&game.universe_id).cloned(),
                    })
                    .collect(),
                error: None,
            },
            Err(error) => AccountRecentGames { user_id, games: Vec::new(), error: Some(error) },
        })
        .collect()
}

#[tauri::command]
async fn get_recent_games_for_accounts(
    state: tauri::State<'_, AccountStore>,
    image_cache: tauri::State<'_, ImageCache>,
    user_ids: Vec<i64>,
) -> Result<Vec<AccountRecentGames>, String> {
    let ids = dedupe_friend_ids(user_ids);
    let mut universes: Vec<(i64, Result<Vec<i64>, String>)> = Vec::with_capacity(ids.len());
    for (index, &user_id) in ids.iter().enumerate() {
        let result = match get_cookie(state.inner(), user_id) {
            Ok(cookie) => api::roblox::get_continue_universes(&cookie).await,
            Err(e) => Err(e),
        };
        universes.push((user_id, result));
        if index + 1 < ids.len() {
            tokio::time::sleep(RECENT_GAMES_DELAY).await;
        }
    }

    let mut wanted: Vec<i64> = Vec::new();
    for (_, result) in &universes {
        if let Ok(list) = result {
            for id in list.iter().take(RECENT_GAMES_PER_ACCOUNT * 2) {
                if !wanted.contains(id) {
                    wanted.push(*id);
                }
            }
        }
    }
    // Sem nomes não há o que mostrar: o erro vai para cada conta que tinha jogos.
    let summaries = match api::roblox::get_universe_summaries(&wanted).await {
        Ok(summaries) => summaries,
        Err(e) if !wanted.is_empty() => {
            return Ok(universes
                .into_iter()
                .map(|(user_id, result)| AccountRecentGames {
                    user_id,
                    games: Vec::new(),
                    error: Some(result.err().unwrap_or_else(|| e.clone())),
                })
                .collect())
        }
        Err(_) => Vec::new(),
    };
    let requests = summaries
        .iter()
        .map(|s| (s.universe_id, "GameIcon".to_string(), "150x150".to_string(), "png".to_string()))
        .collect();
    let icons: std::collections::HashMap<i64, String> = image_cache
        .get_images_batch(requests)
        .await
        .into_iter()
        .filter_map(|(id, url)| url.map(|u| (id, u)))
        .collect();
    Ok(build_recent_games(universes, &summaries, &icons))
}
