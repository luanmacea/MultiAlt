use std::future::Future;

fn session_relogin_error() -> String {
    "Roblox invalidated this session. Re-login required.".to_string()
}

fn is_auth_session_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("status 401")
        || lower.contains("[401")
        || lower.contains("unauthorized")
        || lower.contains("not authenticated")
        || lower.contains("user is not authenticated")
        || lower.contains("invalid cookie")
        || lower.contains("authorization has been denied")
        || lower.contains("\"code\":9002")
        || lower.contains("code 9002")
}

fn get_account(state: &AccountStore, user_id: i64) -> Result<crate::data::accounts::Account, String> {
    state
        .get_all()?
        .into_iter()
        .find(|a| a.user_id == user_id)
        .ok_or_else(|| format!("Account {} not found", user_id))
}

fn mark_refresh_attempt(
    state: &AccountStore,
    user_id: i64,
) -> Result<crate::data::accounts::Account, String> {
    let mut account = get_account(state, user_id)?;
    account.last_attempted_refresh = chrono::Utc::now();
    state.update(account.clone())?;
    Ok(account)
}

fn persist_cookie_update(state: &AccountStore, user_id: i64, new_cookie: &str) -> Result<(), String> {
    let mut account = get_account(state, user_id)?;
    account.security_token = new_cookie.to_string();
    account.valid = true;
    state.update(account)?;
    Ok(())
}

/// Grava a senha nova depois de uma troca bem-sucedida. Sem isto a conta ficava
/// com a senha ANTIGA guardada: a próxima troca (ou o lote do menu Ações) usava
/// essa como "senha atual" e o Roblox respondia que estava errada.
fn persist_password_update(state: &AccountStore, user_id: i64, new_password: &str) -> Result<(), String> {
    let mut account = get_account(state, user_id)?;
    account.replace_password(new_password);
    state.update(account)?;
    Ok(())
}

async fn refresh_account_session(state: &AccountStore, user_id: i64) -> Result<String, String> {
    let account = mark_refresh_attempt(state, user_id)?;
    let result = api::auth::log_out_other_sessions(&account.security_token).await?;
    if !result.success {
        return Err(session_relogin_error());
    }
    let new_cookie = result
        .new_cookie
        .filter(|cookie| !cookie.trim().is_empty())
        .ok_or_else(session_relogin_error)?;
    persist_cookie_update(state, user_id, &new_cookie)?;
    Ok(new_cookie)
}

/// Grava o `.ROBLOSECURITY` novo que o Roblox devolveu numa resposta à chamada
/// feita com `sent` (registrado em `api::cookie_rotation`). Só troca se a conta
/// ainda estiver com `sent`, e pelo caminho normal criptografado. Falha de
/// gravação não derruba a chamada; a mensagem não leva o cookie.
fn adopt_rotated_cookie(state: &AccountStore, user_id: i64, sent: &str) {
    if let Some(new_cookie) = api::cookie_rotation::take_rotated(sent) {
        if let Err(e) = state.replace_token_if(user_id, sent, &new_cookie) {
            eprintln!("[cookie-rotation] could not save the new cookie of account {user_id}: {e}");
        }
    }
}

async fn run_with_session_retry<T, F, Fut>(
    state: &AccountStore,
    user_id: i64,
    mut operation: F,
) -> Result<T, String>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let cookie = get_cookie(state, user_id)?;
    let first = operation(cookie.clone()).await;
    adopt_rotated_cookie(state, user_id, &cookie);
    match first {
        Ok(value) => Ok(value),
        Err(error) => {
            if !is_auth_session_error(&error) {
                return Err(error);
            }

            let refreshed_cookie = refresh_account_session(state, user_id)
                .await
                .map_err(|refresh_error| {
                    if is_auth_session_error(&refresh_error) {
                        session_relogin_error()
                    } else {
                        refresh_error
                    }
                })?;

            let retried = operation(refreshed_cookie.clone()).await;
            adopt_rotated_cookie(state, user_id, &refreshed_cookie);
            match retried {
                Ok(value) => Ok(value),
                Err(retry_error) => {
                    if is_auth_session_error(&retry_error) {
                        Err(session_relogin_error())
                    } else {
                        Err(retry_error)
                    }
                }
            }
        }
    }
}

/// Turns any pasted Roblox link (experience invite, VIP/private server, plain
/// game or deep link) into a launch target for the launch commands.
/// Uses the account's cookie only for share codes, which need a resolve call.
#[tauri::command]
async fn resolve_join_link(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    link: String,
) -> Result<api::roblox::JoinTarget, String> {
    // Links with everything inline (place/job/link code) need no account.
    let parsed = api::roblox::parse_join_link(&link);
    if parsed.share_code.is_none() {
        return api::roblox::resolve_join_link("", &link).await;
    }
    // Resolver o share link e leitura: se o cookie estiver velho, isto falha e a
    // pessoa reloga. Renovar sessao aqui derrubaria os clientes abertos da conta
    // para resolver um link — preco alto demais para uma consulta.
    read_without_refresh(state.inner(), user_id, move |cookie| async move {
        api::roblox::resolve_join_link(&cookie, &link).await
    })
    .await
}

#[tauri::command]
async fn validate_cookie(cookie: String) -> Result<api::auth::AccountInfo, String> {
    api::auth::validate_cookie(&cookie).await
}

#[tauri::command]
async fn get_csrf_token(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
) -> Result<String, String> {
    read_without_refresh(state.inner(), user_id, |cookie| async move {
        api::auth::get_csrf_token(&cookie).await
    })
    .await
}

/// Leitura que **não** pode renovar sessão: pega o cookie e chama a API direto.
///
/// O refresh (`run_with_session_retry`) chama
/// `signoutfromallsessionsandreauthenticate`, que desloga a conta de todas as
/// sessões e pode derrubar clientes Roblox abertos. Para leitura, um cookie velho
/// tem de virar erro na tela — quem lê o saldo não aceita esse preço.
async fn read_without_refresh<T, F, Fut>(
    state: &AccountStore,
    user_id: i64,
    operation: F,
) -> Result<T, String>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let cookie = get_cookie(state, user_id)?;
    let result = operation(cookie.clone()).await;
    adopt_rotated_cookie(state, user_id, &cookie);
    result
}

/// Ticket de auth para os links que o menu de contexto copia (`roblox-player://`
/// e o app link). É leitura não crítica, então **não** passa por
/// `run_with_session_retry`: o refresh chama
/// `signoutfromallsessionsandreauthenticate` e derrubaria as sessões abertas da
/// conta só porque o cookie estava velho. O launch tem o seu próprio caminho e
/// continua com retry.
async fn auth_ticket_without_refresh(state: &AccountStore, user_id: i64) -> Result<String, String> {
    read_without_refresh(state, user_id, |cookie| async move {
        api::auth::get_auth_ticket(&cookie).await
    })
    .await
}

#[tauri::command]
async fn get_auth_ticket(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
) -> Result<String, String> {
    auth_ticket_without_refresh(state.inner(), user_id).await
}

#[tauri::command]
async fn check_pin(state: tauri::State<'_, AccountStore>, user_id: i64) -> Result<bool, String> {
    read_without_refresh(state.inner(), user_id, |cookie| async move {
        api::auth::check_pin(&cookie).await
    })
    .await
}

#[tauri::command]
async fn unlock_pin(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    pin: String,
) -> Result<bool, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| {
        let pin = pin.clone();
        async move { api::auth::unlock_pin(&cookie, &pin).await }
    })
    .await
}

#[tauri::command]
async fn refresh_cookie(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
) -> Result<bool, String> {
    refresh_account_session(state.inner(), user_id).await?;
    Ok(true)
}

#[tauri::command]
async fn get_robux(state: tauri::State<'_, AccountStore>, user_id: i64) -> Result<i64, String> {
    read_without_refresh(state.inner(), user_id, |cookie| async move {
        api::roblox::get_robux(&cookie).await
    })
    .await
}

#[tauri::command]
async fn get_user_info(user_id: i64) -> Result<api::roblox::UserInfo, String> {
    api::roblox::get_user_info(None, user_id).await
}

#[tauri::command]
async fn lookup_user(username: String) -> Result<api::roblox::UserLookupResult, String> {
    api::roblox::get_user_id(None, &username).await
}

#[tauri::command]
async fn test_auth(cookie: String) -> Result<String, String> {
    let mut results = Vec::new();

    results.push(format!("Cookie length: {}", cookie.len()));

    match api::auth::validate_cookie(&cookie).await {
        Ok(info) => results.push(format!(
            "Validate: OK - {} (ID: {})",
            info.name, info.user_id
        )),
        Err(e) => results.push(format!("Validate: FAILED - {}", e)),
    }

    match api::auth::get_csrf_token(&cookie).await {
        Ok(token) => results.push(format!("CSRF: OK - {}...", token.chars().take(16).collect::<String>())),
        Err(e) => results.push(format!("CSRF: FAILED - {}", e)),
    }

    match api::auth::get_auth_ticket(&cookie).await {
        Ok(ticket) => results.push(format!(
            "Ticket: OK - {}...",
            ticket.chars().take(20).collect::<String>()
        )),
        Err(e) => results.push(format!("Ticket: FAILED - {}", e)),
    }

    Ok(results.join("\n"))
}

#[tauri::command]
async fn send_friend_request(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    target_user_id: i64,
) -> Result<(), String> {
    run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::roblox::send_friend_request(&cookie, target_user_id).await
    })
    .await
}

/// Drops non-positive and repeated ids while preserving the selection order.
fn dedupe_friend_ids(user_ids: Vec<i64>) -> Vec<i64> {
    let mut ids: Vec<i64> = Vec::new();
    for id in user_ids {
        if id > 0 && !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

/// "Join Group" do painel de uma conta. Sem refresh de sessão: um 401 vira
/// erro na tela em vez de deslogar a conta de todas as sessões.
#[tauri::command]
async fn join_group(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    group_id: i64,
) -> Result<(), String> {
    if group_id <= 0 {
        return Err("Invalid group ID".to_string());
    }
    read_without_refresh(state.inner(), user_id, |cookie| async move {
        api::roblox::join_group(&cookie, group_id).await
    })
    .await
}

#[tauri::command]
async fn block_user(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    target_user_id: i64,
) -> Result<(), String> {
    run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::roblox::block_user(&cookie, target_user_id).await
    })
    .await
}

#[tauri::command]
async fn unblock_user(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    target_user_id: i64,
) -> Result<(), String> {
    run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::roblox::unblock_user(&cookie, target_user_id).await
    })
    .await
}

#[tauri::command]
async fn get_blocked_users(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
) -> Result<Vec<api::roblox::BlockedUser>, String> {
    read_without_refresh(state.inner(), user_id, |cookie| async move {
        api::roblox::get_blocked_users(&cookie).await
    })
    .await
}

#[tauri::command]
async fn unblock_all_users(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
) -> Result<i32, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::roblox::unblock_all_users(&cookie).await
    })
    .await
}

#[tauri::command]
async fn set_follow_privacy(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    privacy: String,
) -> Result<(), String> {
    run_with_session_retry(state.inner(), user_id, |cookie| {
        let privacy = privacy.clone();
        async move { api::roblox::set_follow_privacy(&cookie, &privacy).await }
    })
    .await
}

#[tauri::command]
async fn get_private_server_invite_privacy(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
) -> Result<String, String> {
    read_without_refresh(state.inner(), user_id, |cookie| async move {
        api::roblox::get_private_server_invite_privacy(&cookie).await
    })
    .await
}

#[tauri::command]
async fn set_private_server_invite_privacy(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    privacy: String,
) -> Result<(), String> {
    run_with_session_retry(state.inner(), user_id, |cookie| {
        let privacy = privacy.clone();
        async move { api::roblox::set_private_server_invite_privacy(&cookie, &privacy).await }
    })
    .await
}

#[tauri::command]
async fn set_avatar(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    avatar_json: serde_json::Value,
) -> Result<Vec<i64>, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| {
        let avatar_json = avatar_json.clone();
        async move { api::roblox::set_avatar(&cookie, avatar_json).await }
    })
    .await
}

#[tauri::command]
async fn get_outfits(target_user_id: i64) -> Result<Vec<api::roblox::OutfitInfo>, String> {
    api::roblox::get_outfits(target_user_id).await
}

#[tauri::command]
async fn get_outfit_details(outfit_id: i64) -> Result<serde_json::Value, String> {
    api::roblox::get_outfit_details(outfit_id).await
}

#[tauri::command]
async fn get_place_details(
    state: tauri::State<'_, AccountStore>,
    place_ids: Vec<i64>,
    user_id: Option<i64>,
) -> Result<Vec<api::roblox::PlaceDetails>, String> {
    let cookie = user_id.and_then(|id| get_cookie(&state, id).ok());
    api::roblox::get_place_details(&place_ids, cookie.as_deref()).await
}

#[tauri::command]
async fn get_servers(
    state: tauri::State<'_, AccountStore>,
    place_id: i64,
    server_type: String,
    cursor: Option<String>,
    user_id: Option<i64>,
    sort_order: Option<String>,
    exclude_full: Option<bool>,
) -> Result<api::roblox::ServersResponse, String> {
    let cookie = user_id.and_then(|id| get_cookie(&state, id).ok());
    api::roblox::get_servers_page(
        place_id,
        &server_type,
        cursor.as_deref(),
        cookie.as_deref(),
        sort_order.as_deref().unwrap_or("Asc"),
        exclude_full.unwrap_or(false),
    )
    .await
}

/// Os servidores privados deste place que **a conta** pode acessar (uma página),
/// lidos com o cookie dela — a seção "Private servers" da aba Servers.
///
/// **Sem `run_with_session_retry`**: é leitura, e o refresh chama
/// `signoutfromallsessionsandreauthenticate`, que derrubaria as sessões abertas
/// da conta. Cookie vencido vira o erro desta conta, e só dela.
#[tauri::command]
async fn get_private_servers(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    place_id: i64,
    cursor: Option<String>,
) -> Result<api::roblox::PrivateServersResponse, String> {
    get_private_servers_inner(state.inner(), user_id, place_id, cursor).await
}

async fn get_private_servers_inner(
    state: &AccountStore,
    user_id: i64,
    place_id: i64,
    cursor: Option<String>,
) -> Result<api::roblox::PrivateServersResponse, String> {
    if place_id <= 0 {
        return Err("Enter a valid Place ID".to_string());
    }
    read_without_refresh(state, user_id, |cookie| async move {
        api::roblox::get_private_servers(place_id, cursor.as_deref(), &cookie).await
    })
    .await
}

/// A lista de servidores **na ordem da preferência**, já sem os que não cabem
/// o lote.
///
/// Existe para a aba Servers mostrar exatamente o que o launch escolheria: a
/// ordenação e a paginação do "melhor encaixe" moram no backend, e refazer isso
/// no frontend com uma página de 100 servidores dava resultado diferente.
#[tauri::command]
async fn list_servers_ranked(
    state: tauri::State<'_, AccountStore>,
    place_id: i64,
    user_id: Option<i64>,
    preference: String,
    accounts: Option<usize>,
) -> Result<Vec<api::roblox::ServerData>, String> {
    let cookie = user_id.and_then(|id| get_cookie(&state, id).ok());
    let preference = api::roblox::parse_server_preference(&preference);
    let accounts = accounts.unwrap_or(1).max(1);

    let servers = api::roblox::collect_servers_for(
        place_id,
        cookie.as_deref(),
        preference,
        accounts,
        api::roblox::BEST_FIT_MAX_PAGES,
    )
    .await?;

    Ok(api::roblox::rank_servers(&servers, preference, accounts)
        .into_iter()
        .cloned()
        .collect())
}

// ---------------------------------------------------------------------------
// Varredura de servidores
// ---------------------------------------------------------------------------
//
// Um jogo grande tem milhares de servidores e a API devolve 100 por página. Com
// um lote de 6 contas, as primeiras páginas podem não ter **nenhum** servidor
// que caiba todo mundo — e esperar a varredura inteira antes de mostrar
// qualquer coisa deixaria a tela vazia por vários segundos.
//
// A varredura roda em background e publica um `server-scan` a cada página, já
// com a lista reordenada: o usuário vê o que existe agora e a lista vai
// melhorando enquanto mais páginas chegam.

/// Páginas por varredura quando o usuário não escolheu nada (100 servidores
/// cada). Cobre a maioria dos jogos sem virar uma enxurrada de requisições.
pub const SCAN_DEFAULT_PAGES: usize = 30;

/// Teto absoluto, mesmo que o usuário peça mais: 50 mil servidores já é mais do
/// que qualquer jogo tem, e uma varredura infinita martelaria a API do Roblox.
pub const SCAN_HARD_MAX_PAGES: usize = 500;

/// Quantas páginas esta varredura pode percorrer.
///
/// `None` (a UI não mandou nada) usa o padrão; zero é erro de chamada e vira o
/// padrão também; acima do teto, o teto.
pub fn scan_page_budget(requested: Option<usize>) -> usize {
    match requested {
        Some(0) | None => SCAN_DEFAULT_PAGES,
        Some(pages) => pages.min(SCAN_HARD_MAX_PAGES),
    }
}
/// Pausa entre páginas, para não tomar 429 da API de servidores.
const SCAN_PAGE_DELAY_MS: u64 = 250;
/// Quantos servidores que cabem o lote bastam para parar de procurar.
const SCAN_ENOUGH_FITTING: usize = 12;
/// Quantos servidores a UI recebe por atualização.
const SCAN_VISIBLE_LIMIT: usize = 150;

/// Geração da varredura em curso. Começar outra (ou parar) invalida a anterior,
/// que percebe a mudança na próxima página e se encerra.
static SERVER_SCAN_GENERATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Uma atualização da varredura (evento `server-scan`).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerScanUpdate {
    pub scan_id: u64,
    pub place_id: i64,
    /// Os melhores servidores encontrados até agora, já ordenados.
    pub servers: Vec<api::roblox::ServerData>,
    /// Quantos servidores foram examinados no total.
    pub scanned: usize,
    /// Quantos deles cabem o lote inteiro.
    pub fitting: usize,
    pub done: bool,
    /// Parou por ter batido o limite de páginas, não por falta de servidores.
    /// A UI oferece aumentar o limite em vez de dizer que acabou.
    pub stopped_at_limit: bool,
    pub error: Option<String>,
}

fn scan_cancelled(scan_id: u64) -> bool {
    SERVER_SCAN_GENERATION.load(std::sync::atomic::Ordering::Relaxed) != scan_id
}

/// Encerra a varredura em curso (troca de jogo, de preferência, ou a aba saiu
/// da tela).
#[tauri::command]
fn stop_server_scan() {
    SERVER_SCAN_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Começa a varrer os servidores de um place em background.
///
/// Devolve o `scanId` da varredura; a UI ignora eventos de outro id (o de uma
/// varredura antiga que ainda estava no ar).
#[tauri::command]
async fn start_server_scan(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    place_id: i64,
    user_id: Option<i64>,
    preference: String,
    accounts: Option<usize>,
    max_pages: Option<usize>,
) -> Result<u64, String> {
    if place_id <= 0 {
        return Err("Place ID inválido".to_string());
    }
    let budget = scan_page_budget(max_pages);
    let cookie = user_id.and_then(|id| get_cookie(&state, id).ok());
    let preference = api::roblox::parse_server_preference(&preference);
    let accounts = accounts.unwrap_or(1).max(1);

    let scan_id = SERVER_SCAN_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;

    tauri::async_runtime::spawn(async move {
        let mut all: Vec<api::roblox::ServerData> = Vec::new();
        let mut cursor: Option<String> = None;

        for page in 0..budget {
            if scan_cancelled(scan_id) {
                return;
            }

            let fetched = api::roblox::get_servers_page(
                place_id,
                "Public",
                cursor.as_deref(),
                cookie.as_deref(),
                preference.sort_order(),
                true,
            )
            .await;

            let (batch, next) = match fetched {
                Ok(page) => (page.data, page.next_page_cursor),
                Err(error) => {
                    // Erro na primeira página é falha; no meio, a varredura
                    // para com o que já achou — melhor uma lista parcial que
                    // uma tela de erro.
                    let _ = app.emit(
                        "server-scan",
                        scan_update(
                            scan_id, place_id, &all, preference, accounts, true, false,
                            Some(error),
                        ),
                    );
                    return;
                }
            };

            extend_unique(&mut all, batch);
            let fitting = count_fitting(&all, accounts);
            let exhausted = next.as_deref().map(|c| c.trim().is_empty()).unwrap_or(true);
            let enough = fitting >= SCAN_ENOUGH_FITTING;
            let hit_limit = page + 1 == budget && !exhausted && !enough;
            let done = exhausted || enough || hit_limit;

            if scan_cancelled(scan_id) {
                return;
            }
            let _ = app.emit(
                "server-scan",
                scan_update(
                    scan_id, place_id, &all, preference, accounts, done, hit_limit, None,
                ),
            );

            if done {
                return;
            }
            cursor = next;
            tokio::time::sleep(std::time::Duration::from_millis(SCAN_PAGE_DELAY_MS)).await;
        }
    });

    Ok(scan_id)
}

/// Junta a página nova **sem repetir servidor**.
///
/// A lista do Roblox se mexe entre uma página e outra (jogador entra, jogador
/// sai), então o mesmo Job ID volta em páginas diferentes — em dados reais, 50
/// repetidos em 400. Isso inflava a contagem de "servidores examinados" e,
/// principalmente, mandava ids repetidos para a UI, onde viravam chaves de
/// lista duplicadas e travavam a reordenação do React.
fn extend_unique(all: &mut Vec<api::roblox::ServerData>, batch: Vec<api::roblox::ServerData>) {
    let seen: std::collections::HashSet<String> = all.iter().map(|s| s.id.clone()).collect();
    let mut seen = seen;
    for server in batch {
        if server.id.trim().is_empty() || !seen.insert(server.id.clone()) {
            continue;
        }
        all.push(server);
    }
}

/// Quantos servidores da lista cabem o lote inteiro.
fn count_fitting(servers: &[api::roblox::ServerData], accounts: usize) -> usize {
    let needed = accounts.max(1) as i32;
    servers
        .iter()
        .filter(|s| s.max_players > 0 && s.playing + needed <= s.max_players)
        .count()
}

/// Monta a atualização com a lista já ordenada e cortada no que a UI mostra.
fn scan_update(
    scan_id: u64,
    place_id: i64,
    all: &[api::roblox::ServerData],
    preference: api::roblox::ServerPreference,
    accounts: usize,
    done: bool,
    stopped_at_limit: bool,
    error: Option<String>,
) -> ServerScanUpdate {
    // Os que cabem o lote vão na frente **antes** do corte: a varredura pode
    // achar milhares de servidores e o recorte não pode ser o que esconde os
    // poucos que servem.
    let ranked = api::roblox::rank_servers(all, preference, accounts);
    let needed = accounts.max(1) as i32;
    let (fits, rest): (Vec<_>, Vec<_>) = ranked
        .into_iter()
        .partition(|s| s.max_players > 0 && s.playing + needed <= s.max_players);
    let servers: Vec<api::roblox::ServerData> = fits
        .into_iter()
        .chain(rest)
        .take(SCAN_VISIBLE_LIMIT)
        .cloned()
        .collect();

    ServerScanUpdate {
        scan_id,
        place_id,
        servers,
        scanned: all.len(),
        fitting: count_fitting(all, accounts),
        done,
        stopped_at_limit,
        error,
    }
}

#[tauri::command]
async fn join_game_instance(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    place_id: i64,
    game_id: String,
    is_teleport: bool,
) -> Result<serde_json::Value, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| {
        let game_id = game_id.clone();
        async move { api::roblox::join_game_instance(&cookie, place_id, &game_id, is_teleport).await }
    })
    .await
}

#[tauri::command]
async fn join_game(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    place_id: i64,
) -> Result<serde_json::Value, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::roblox::join_game(&cookie, place_id).await
    })
    .await
}

#[tauri::command]
async fn search_games(
    security_token: Option<String>,
    keyword: String,
    start: i32,
) -> Result<serde_json::Value, String> {
    api::roblox::search_games(security_token.as_deref(), &keyword, start).await
}

#[tauri::command]
async fn get_universe_places(
    state: tauri::State<'_, AccountStore>,
    universe_id: i64,
    user_id: Option<i64>,
) -> Result<Vec<api::roblox::UniversePlace>, String> {
    let cookie = user_id.and_then(|id| get_cookie(&state, id).ok());
    api::roblox::get_universe_places(universe_id, cookie.as_deref()).await
}

#[tauri::command]
async fn parse_private_server_link_code(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    place_id: i64,
    link_code: String,
) -> Result<String, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| {
        let link_code = link_code.clone();
        async move { api::roblox::parse_private_server_link_code(&cookie, place_id, &link_code).await }
    })
    .await
}

/// Escolhe o cookie de "quem está olhando" para uma chamada de presença sem
/// `viewer_user_id` explícito: prefere uma conta válida com cookie não vazio e
/// cai para qualquer conta com cookie não vazio se nenhuma válida sobrar.
///
/// Ressalva: o `gameId` (job id) que a Roblox devolve para as outras contas
/// passa a depender da privacidade/configuração de amigos de quem foi
/// escolhida como "viewer" — é assim que a API funciona, não uma falha daqui.
fn pick_viewer_cookie(accounts: &[data::accounts::Account]) -> Option<String> {
    accounts
        .iter()
        .find(|account| account.valid && !account.security_token.trim().is_empty())
        .or_else(|| accounts.iter().find(|account| !account.security_token.trim().is_empty()))
        .map(|account| account.security_token.clone())
}

/// Presença dos usuários pedidos, com o cookie de uma conta quando possível —
/// sem cookie a Roblox devolve a versão degradada, sem `gameId`, e a lista
/// perde as bolinhas In Game/Online precisas (`src/store.tsx`).
///
/// **Nunca** usa `run_with_session_retry` aqui: é leitura, e o refresh chama
/// `signoutfromallsessionsandreauthenticate`, que derrubaria as sessões
/// abertas da conta escolhida só para colorir bolinhas de presença.
///
/// **Fallback obrigatório:** se a chamada autenticada falhar (cookie morto,
/// rede, o que for), repete sem cookie em vez de propagar o erro — uma única
/// conta com sessão inválida não pode apagar a presença de todo mundo.
///
/// **Exceção: rate limit (429).** Um 429 na chamada autenticada é propagado
/// **com o `Retry-After`** em vez de cair no anônimo: o limite é por IP, o
/// anônimo tomaria 429 igual, e um sucesso anônimo aqui zeraria o backoff da
/// espera da aba Seguir ("Wait until they're in a game") — ela nunca
/// desaceleraria sob rate limiting. Continua só leitura, sem refresh de sessão.
async fn presence_with_viewer_cookie(
    state: &AccountStore,
    user_ids: &[i64],
    viewer_user_id: Option<i64>,
) -> Result<Vec<api::roblox::UserPresence>, String> {
    let cookie = viewer_user_id
        .and_then(|id| get_cookie(state, id).ok())
        .filter(|token| !token.trim().is_empty())
        .or_else(|| state.get_all().ok().and_then(|accounts| pick_viewer_cookie(&accounts)));

    if let Some(token) = cookie {
        match api::roblox::get_presence_as(Some(&token), user_ids).await {
            Ok(presences) => return Ok(presences),
            // 429: propaga (leva o Retry-After); não mascara com o anônimo.
            Err(err) if is_presence_rate_limited(&err) => return Err(err),
            // Cookie morto, rede, 5xx, ilegível: cai para o anônimo.
            Err(_) => {}
        }
    }

    api::roblox::get_presence(user_ids).await
}

/// Presença "em jogo" (`userPresenceType` 2): a conta está jogando, onde
/// quer que seja — pelo site, no celular, em outro PC.
fn presence_means_in_use(presence_type: i32) -> bool {
    presence_type == 2
}

/// Conta em jogo fora do app também está em uso: o "usado há" anda (de 10 em
/// 10 min no máximo). Pedido do dono, 25/10/2026.
fn mark_in_game_accounts_used(state: &AccountStore, presences: &[api::roblox::UserPresence]) -> Vec<i64> {
    presences
        .iter()
        .filter(|p| presence_means_in_use(p.user_presence_type))
        .filter(|p| state.mark_used_if_stale(p.user_id, IN_USE_MARK_EVERY).unwrap_or(false))
        .map(|p| p.user_id)
        .collect()
}

/// Avisa a tela que o "usado há" da conta andou (evento `account-used`).
fn emit_account_used(app: &tauri::AppHandle, user_id: i64) {
    let _ = app.emit("account-used", serde_json::json!({ "userId": user_id }));
}

/// Erro de presença que é rate limit (429). O formato vem de
/// `social_presence::get_presence_as` ("Failed to get presence (status 429...)").
fn is_presence_rate_limited(err: &str) -> bool {
    err.contains("status 429")
}

#[tauri::command]
async fn get_presence(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    user_ids: Vec<i64>,
    viewer_user_id: Option<i64>,
) -> Result<Vec<api::roblox::UserPresence>, String> {
    let presences = presence_with_viewer_cookie(state.inner(), &user_ids, viewer_user_id).await?;
    for user_id in mark_in_game_accounts_used(state.inner(), &presences) {
        emit_account_used(&app, user_id);
    }
    Ok(presences)
}

/// Onde uma conta esta jogando agora.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountGameLocation {
    user_id: i64,
    in_game: bool,
    /// Place do jogo (`rootPlaceId` quando existe: e o que a tela usa).
    place_id: Option<i64>,
    /// Job id do servidor. **So vem com o cookie da propria conta** — sem ele a
    /// API do Roblox omite o campo.
    job_id: Option<String>,
}

/// Descobre o place e o servidor de uma conta que ja esta em jogo.
///
/// Serve para ligar o Botting Mode numa conta que o usuario lancou por fora:
/// sem saber onde ela esta, o primeiro ciclo a relancaria no place da sessao e
/// a tiraria do servidor em que estava.
///
/// **Sem `run_with_session_retry`** de proposito: e leitura, e o refresh chama
/// `signoutfromallsessionsandreauthenticate`, que derruba justamente o cliente
/// aberto que se quer preservar.
#[tauri::command]
async fn get_account_game_location(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
) -> Result<AccountGameLocation, String> {
    get_account_game_location_inner(state.inner(), user_id).await
}

/// O miolo, sem o `State` do Tauri, para o teste poder chamar.
async fn get_account_game_location_inner(
    state: &AccountStore,
    user_id: i64,
) -> Result<AccountGameLocation, String> {
    let presences = read_without_refresh(state, user_id, |cookie| async move {
        api::roblox::get_presence_as(Some(&cookie), &[user_id]).await
    })
    .await?;

    let found = presences.into_iter().find(|p| p.user_id == user_id);
    let Some(presence) = found else {
        return Ok(AccountGameLocation {
            user_id,
            in_game: false,
            place_id: None,
            job_id: None,
        });
    };

    // 2 = InGame na API de presenca do Roblox.
    let in_game = presence.user_presence_type == 2;
    Ok(AccountGameLocation {
        user_id,
        in_game,
        place_id: if in_game {
            presence.root_place_id.or(presence.place_id)
        } else {
            None
        },
        job_id: if in_game {
            presence.game_id.filter(|j| !j.is_empty())
        } else {
            None
        },
    })
}

// ── Amigos online ─────────────────────────────────────────────────────────────

/// Uma conta e os amigos online dela. `error` preenchido substitui a lista
/// daquela conta **sem** derrubar as outras entradas do lote.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountFriends {
    user_id: i64,
    friends: Vec<api::roblox::OnlineFriend>,
    error: Option<String>,
}

/// Payload do evento `friends-online-progress`.
///
/// Além da contagem, leva a entrada da conta que acabou de voltar (`entry`):
/// é o que deixa a aba Friends mostrar cada conta na hora, sem esperar o lote
/// inteiro. O `request_id` é o que a tela mandou no comando — com ele ela
/// descarta os eventos de uma consulta anterior que ainda esteja rodando.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FriendsOnlineProgress {
    done: usize,
    total: usize,
    request_id: Option<u64>,
    entry: Option<AccountFriends>,
}

/// Espaçamento default entre contas no lote. O rate limit da API de amigos é
/// por conta **e** por IP, e todas as contas saem do mesmo IP.
const FRIENDS_ONLINE_DELAY_MS: u64 = 350;

/// Amigos online de uma conta, ordenados (quem dá para entrar primeiro).
///
/// Usa `get_cookie` direto e **nunca** `run_with_session_retry`: o refresh dele
/// chama `signoutfromallsessionsandreauthenticate`, que derruba as sessões
/// abertas da conta — caro demais para uma leitura.
#[tauri::command]
async fn get_online_friends(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
) -> Result<Vec<api::roblox::OnlineFriend>, String> {
    let cookie = get_cookie(state.inner(), user_id)?;
    api::roblox::get_online_friends(&cookie, user_id).await
}

/// Percorre as contas **em sequência**, com pausa entre elas, e isola o erro de
/// cada uma na sua própria entrada. `on_progress` recebe `(feitas, total,
/// entrada)`: a entrada é a da conta que acabou de voltar (`None` só no zero
/// inicial).
///
/// Separado do comando Tauri para poder ser testado sem `AppHandle`.
async fn collect_online_friends<F>(
    store: &AccountStore,
    user_ids: Vec<i64>,
    delay_ms: Option<u64>,
    mut on_progress: F,
) -> Vec<AccountFriends>
where
    F: FnMut(usize, usize, Option<&AccountFriends>),
{
    let ids = dedupe_friend_ids(user_ids);
    let total = ids.len();
    let delay =
        std::time::Duration::from_millis(delay_ms.unwrap_or(FRIENDS_ONLINE_DELAY_MS));
    let mut results: Vec<AccountFriends> = Vec::with_capacity(total);

    on_progress(0, total, None);
    for (index, &user_id) in ids.iter().enumerate() {
        let entry = match get_cookie(store, user_id) {
            Ok(cookie) => match api::roblox::get_online_friends(&cookie, user_id).await {
                Ok(friends) => AccountFriends { user_id, friends, error: None },
                Err(error) => {
                    AccountFriends { user_id, friends: Vec::new(), error: Some(error) }
                }
            },
            Err(error) => AccountFriends { user_id, friends: Vec::new(), error: Some(error) },
        };
        on_progress(index + 1, total, Some(&entry));
        results.push(entry);

        if index + 1 < total && !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
    }

    results
}

/// Amigos online de várias contas, uma chamada por conta. Emite
/// `friends-online-progress` com `{ done, total, requestId, entry }` a cada
/// conta concluída — a tela desenha a conta na hora — e ainda devolve o lote
/// inteiro no fim, que é a versão final da lista.
#[tauri::command]
async fn get_online_friends_for_accounts(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    user_ids: Vec<i64>,
    delay_ms: Option<u64>,
    request_id: Option<u64>,
) -> Result<Vec<AccountFriends>, String> {
    let results = collect_online_friends(state.inner(), user_ids, delay_ms, |done, total, entry| {
        let _ = app.emit(
            "friends-online-progress",
            FriendsOnlineProgress { done, total, request_id, entry: entry.cloned() },
        );
    })
    .await;

    Ok(results)
}

// ---------------------------------------------------------------------------
// Escolha de servidor e região
// ---------------------------------------------------------------------------

/// Template de exibição da região. Configurável porque o usuário pode querer
/// só o país, ou o IP cru.
fn server_region_template(settings: &SettingsStore) -> String {
    settings
        .get("General", "ServerRegionFormat")
        .ok()
        .flatten()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "<city>, <countryCode>".to_string())
}

/// Acesso de uma conta a um servidor, para a aba Servidores.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountAccess {
    user_id: i64,
    /// Recusada por falta de permissão (o erro 524 do cliente).
    denied: bool,
    /// Outro motivo de não dar para saber (servidor cheio, rede, cookie).
    error: Option<String>,
}

/// Pergunta, **uma conta por vez** e com a pausa da região, se cada conta
/// pode entrar em `job_id`. Permissão costuma ser do place (regra de acesso
/// do jogo, teleporte, VC) e não do servidor, então um servidor responde pela
/// lista inteira — é o que deixa barato perguntar por todas as contas, em vez
/// de só pela primeira como a verificação das regiões faz.
#[tauri::command]
async fn check_place_access(
    state: tauri::State<'_, AccountStore>,
    user_ids: Vec<i64>,
    place_id: i64,
    job_id: String,
) -> Result<Vec<AccountAccess>, String> {
    let delay = std::time::Duration::from_millis(api::roblox::REGION_LOOKUP_DELAY_MS);
    let mut out = Vec::with_capacity(user_ids.len());
    for (index, user_id) in user_ids.into_iter().enumerate() {
        if index > 0 {
            tokio::time::sleep(delay).await;
        }
        let access = match get_cookie(state.inner(), user_id) {
            Err(error) => AccountAccess { user_id, denied: false, error: Some(error) },
            Ok(cookie) => match api::roblox::join_access(&cookie, place_id, &job_id).await {
                Ok(()) => AccountAccess { user_id, denied: false, error: None },
                Err(refusal) => AccountAccess {
                    user_id,
                    denied: refusal.denied(),
                    error: Some(refusal.message),
                },
            },
        };
        out.push(access);
    }
    Ok(out)
}

/// Região de vários servidores de um place, uma chamada de join por servidor.
///
/// Emite `server-region-progress` com `{ done, total }`. Sequencial e com pausa
/// de propósito: o `join-game-instance` é o mesmo endpoint que o cliente usa
/// para entrar no jogo, e disparar em rajada toma 429.
///
/// Usa `get_cookie` direto e **nunca** `run_with_session_retry`: o refresh dele
/// derruba as sessões abertas da conta.
#[tauri::command]
async fn get_server_regions(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    settings: tauri::State<'_, SettingsStore>,
    user_id: i64,
    place_id: i64,
    job_ids: Vec<String>,
) -> Result<Vec<api::roblox::ServerRegion>, String> {
    let cookie = get_cookie(state.inner(), user_id)?;
    let template = server_region_template(settings.inner());

    let results = api::roblox::resolve_server_regions(
        &cookie,
        place_id,
        &job_ids,
        &template,
        api::roblox::REGION_LOOKUP_DELAY_MS,
        |done, total| {
            let _ = app.emit(
                "server-region-progress",
                api::roblox::ServerRegionProgress { done, total },
            );
        },
    )
    .await;

    Ok(results)
}

/// Escolhe o servidor do lote conforme a preferência (`random` | `emptiest` |
/// `fullest`) e, opcionalmente, um país (`BR`).
///
/// Resolvido **uma vez** para o lote inteiro: a UI passa o Job ID devolvido
/// para o launch, e todas as contas entram no mesmo servidor.
#[tauri::command]
async fn pick_server(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    settings: tauri::State<'_, SettingsStore>,
    user_id: i64,
    place_id: i64,
    preference: String,
    accounts: Option<usize>,
    country_code: Option<String>,
    max_lookups: Option<usize>,
) -> Result<api::roblox::PickedServer, String> {
    let cookie = get_cookie(state.inner(), user_id)?;
    let template = server_region_template(settings.inner());

    // No Console: a escolha do servidor roda ANTES do launch e pode levar
    // segundos (várias páginas de servidores; com filtro de região, uma
    // consulta por servidor). Sem esta linha o tempo entre o clique e o
    // "Iniciando launch" não aparecia em lugar nenhum (relato do dono, 15/10).
    emit_launch_log(&app, user_id, "info", "server", log_msg!("Picking a server ({{preference}})...", preference = preference.clone()));
    let started = std::time::Instant::now();
    let result = api::roblox::pick_server(
        &cookie,
        place_id,
        api::roblox::parse_server_preference(&preference),
        accounts.unwrap_or(1),
        country_code.as_deref().unwrap_or(""),
        &template,
        max_lookups.unwrap_or(api::roblox::REGION_LOOKUP_MAX),
    )
    .await;
    let secs = started.elapsed().as_secs_f32();
    match &result {
        Ok(picked) => emit_launch_log(
            &app,
            user_id,
            "info",
            "server",
            log_msg!("Server picked in {{seconds}} s: {{job}}", seconds = format!("{secs:.1}"), job = picked.job_id.clone()),
        ),
        Err(e) => emit_launch_log(
            &app,
            user_id,
            "warn",
            "server",
            log_msg!("Could not pick a server in {{seconds}} s: {{error}}", seconds = format!("{secs:.1}"), error = e.to_string()),
        ),
    }
    result
}

#[tauri::command]
async fn batch_thumbnails(
    requests: Vec<api::roblox::ThumbnailRequest>,
) -> Result<Vec<api::roblox::ThumbnailResponse>, String> {
    api::roblox::batch_thumbnails(requests).await
}

#[tauri::command]
async fn get_avatar_headshots(
    user_ids: Vec<i64>,
    size: String,
) -> Result<Vec<api::roblox::ThumbnailResponse>, String> {
    api::roblox::get_avatar_headshots(&user_ids, &size).await
}

#[tauri::command]
async fn get_asset_thumbnails(
    state: tauri::State<'_, AccountStore>,
    asset_ids: Vec<i64>,
    size: String,
    user_id: Option<i64>,
) -> Result<Vec<api::roblox::ThumbnailResponse>, String> {
    let cookie = user_id.and_then(|id| get_cookie(&state, id).ok());
    api::roblox::get_asset_thumbnails(&asset_ids, &size, cookie.as_deref()).await
}

#[tauri::command]
async fn get_asset_details(
    state: tauri::State<'_, AccountStore>,
    asset_id: i64,
    user_id: Option<i64>,
) -> Result<api::roblox::AssetDetails, String> {
    let cookie = user_id.and_then(|id| get_cookie(&state, id).ok());
    api::roblox::get_asset_details(asset_id, cookie.as_deref()).await
}

#[tauri::command]
async fn purchase_product(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    product_id: i64,
    expected_price: i64,
    expected_seller_id: i64,
) -> Result<api::roblox::PurchaseResult, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| async move {
        api::roblox::purchase_product(&cookie, product_id, expected_price, expected_seller_id)
            .await
    })
    .await
}

#[tauri::command]
async fn change_password(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    current_password: String,
    new_password: String,
) -> Result<(), String> {
    let new_cookie = run_with_session_retry(state.inner(), user_id, |cookie| {
        let current_password = current_password.clone();
        let new_password = new_password.clone();
        async move { api::auth::change_password(&cookie, &current_password, &new_password).await }
    })
    .await?;

    if let Some(new_cookie) = new_cookie {
        persist_cookie_update(state.inner(), user_id, &new_cookie)?;
    }
    persist_password_update(state.inner(), user_id, &new_password)?;

    Ok(())
}

#[tauri::command]
async fn change_email(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    password: String,
    new_email: String,
) -> Result<(), String> {
    run_with_session_retry(state.inner(), user_id, |cookie| {
        let password = password.clone();
        let new_email = new_email.clone();
        async move { api::auth::change_email(&cookie, &password, &new_email).await }
    })
    .await
}

#[tauri::command]
async fn set_display_name(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    display_name: String,
) -> Result<(), String> {
    run_with_session_retry(state.inner(), user_id, |cookie| {
        let display_name = display_name.clone();
        async move { api::auth::set_display_name(&cookie, user_id, &display_name).await }
    })
    .await
}

#[tauri::command]
async fn quick_login_enter_code(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    code: String,
) -> Result<serde_json::Value, String> {
    run_with_session_retry(state.inner(), user_id, |cookie| {
        let code = code.clone();
        async move { api::auth::quick_login_enter_code(&cookie, &code).await }
    })
    .await
}

#[tauri::command]
async fn quick_login_validate_code(
    state: tauri::State<'_, AccountStore>,
    user_id: i64,
    code: String,
) -> Result<(), String> {
    run_with_session_retry(state.inner(), user_id, |cookie| {
        let code = code.clone();
        async move { api::auth::quick_login_validate_code(&cookie, &code).await }
    })
    .await
}
