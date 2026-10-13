// Seguir um usuário com várias contas (Actions › Follow a user). O alvo é
// digitado (nome, ID ou link do perfil) ou escolhido entre as contas do
// MultiAlt; ele não precisa estar no app. Uma conta por vez, com intervalo
// entre elas, e nunca renova sessão: o refresh desloga todas as sessões da
// conta. Ver docs/features/accounts.md ("Seguir um usuário").

/// O que o usuário digitou, já entendido.
#[derive(Debug, Clone, PartialEq)]
enum FollowTarget {
    /// ID numérico. `from_link` diz se veio de um link de perfil: aí não há
    /// como ser um nome só de dígitos.
    Id { id: i64, from_link: bool },
    Name(String),
}

/// Lê nome, ID ou link do perfil (`roblox.com/users/123/profile`). O `@` na
/// frente do nome é tolerado — é como o próprio Roblox mostra o nome.
fn parse_follow_target(input: &str) -> Result<FollowTarget, String> {
    let trimmed = input.trim();
    let invalid = || "Type a username, a user ID or a profile link.".to_string();
    if trimmed.is_empty() {
        return Err(invalid());
    }

    let lower = trimmed.to_ascii_lowercase();
    if let Some(pos) = lower.find("roblox.com/users/") {
        let rest = &trimmed[pos + "roblox.com/users/".len()..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        return digits
            .parse::<i64>()
            .ok()
            .filter(|id| *id > 0)
            .map(|id| FollowTarget::Id { id, from_link: true })
            .ok_or_else(invalid);
    }

    let name = trimmed.strip_prefix('@').unwrap_or(trimmed);
    if !name.is_empty() && name.chars().all(|c| c.is_ascii_digit()) {
        return name
            .parse::<i64>()
            .ok()
            .filter(|id| *id > 0)
            .map(|id| FollowTarget::Id { id, from_link: false })
            .ok_or_else(invalid);
    }
    // Regra do Roblox para nome de usuário: 3–20, letras, dígitos e `_`.
    let valid_name = (3..=20).contains(&name.len())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if valid_name {
        Ok(FollowTarget::Name(name.to_string()))
    } else {
        Err(invalid())
    }
}

/// Confere que o alvo existe e devolve ID, nome e apelido — a UI mostra quem
/// vai ser seguido antes de disparar o lote. Só leitura, sem cookie.
#[tauri::command]
async fn resolve_follow_target(query: String) -> Result<api::roblox::UserLookupResult, String> {
    match parse_follow_target(&query)? {
        FollowTarget::Name(name) => api::roblox::get_user_id(None, &name).await,
        FollowTarget::Id { id, from_link } => match api::roblox::get_user_info(None, id).await {
            Ok(info) => Ok(api::roblox::UserLookupResult {
                id: info.id,
                name: info.name,
                display_name: info.display_name,
            }),
            // Só dígitos e sem usuário com esse ID: pode ser um nome numérico.
            Err(e) if !from_link && e.contains("status 404") => {
                api::roblox::get_user_id(None, &id.to_string())
                    .await
                    .map_err(|_| format!("No user with ID or name {}", id))
            }
            Err(e) => Err(e),
        },
    }
}

/// Um lote de cada vez: dois lotes ao mesmo tempo dobram os pedidos e o
/// risco de rate limit/captcha.
static FOLLOW_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

struct FollowRunGuard;

impl Drop for FollowRunGuard {
    fn drop(&mut self) {
        FOLLOW_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

fn try_begin_follow() -> Result<FollowRunGuard, String> {
    if FOLLOW_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("A follow run is already in progress.".to_string());
    }
    Ok(FollowRunGuard)
}

/// Contas que vão seguir: sem repetição, sem ID inválido e sem o próprio alvo
/// (o Roblox não deixa uma conta seguir a si mesma). O `bool` diz se o alvo
/// estava na seleção, para a UI avisar.
fn plan_followers(user_ids: Vec<i64>, target_user_id: i64) -> (Vec<i64>, bool) {
    let had_target = user_ids.contains(&target_user_id);
    let ids = dedupe_friend_ids(user_ids)
        .into_iter()
        .filter(|&id| id != target_user_id)
        .collect();
    (ids, had_target)
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FollowAccount {
    user_id: i64,
    /// `pending` | `processing` | `followed` | `already` | `failed`
    state: String,
    error: Option<String>,
}

/// Retrato completo, reemitido a cada mudança (mesmo desenho do Make Friends).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FollowSnapshot {
    active: bool,
    processed: usize,
    total: usize,
    accounts: Vec<FollowAccount>,
    target_user_id: Option<i64>,
    target_name: Option<String>,
}

#[derive(Debug, Default)]
struct FollowRun {
    active: bool,
    target_user_id: Option<i64>,
    target_name: Option<String>,
    order: Vec<i64>,
    states: std::collections::HashMap<i64, (String, Option<String>)>,
}

impl FollowRun {
    fn start(&mut self, ids: &[i64], target_user_id: i64, target_name: Option<String>) {
        self.active = true;
        self.target_user_id = Some(target_user_id);
        self.target_name = target_name;
        self.order = ids.to_vec();
        self.states = ids
            .iter()
            .map(|&id| (id, ("pending".to_string(), None)))
            .collect();
    }

    fn set(&mut self, user_id: i64, state: &str, error: Option<String>) {
        if let Some(entry) = self.states.get_mut(&user_id) {
            *entry = (state.to_string(), error);
        }
    }

    fn finish(&mut self) {
        self.active = false;
    }

    fn count(&self, state: &str) -> usize {
        self.states.values().filter(|(s, _)| s == state).count()
    }

    fn snapshot(&self) -> FollowSnapshot {
        let accounts: Vec<FollowAccount> = self
            .order
            .iter()
            .map(|id| {
                let (state, error) = self
                    .states
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| ("pending".to_string(), None));
                FollowAccount { user_id: *id, state, error }
            })
            .collect();
        FollowSnapshot {
            active: self.active,
            processed: accounts
                .iter()
                .filter(|a| matches!(a.state.as_str(), "followed" | "already" | "failed"))
                .count(),
            total: accounts.len(),
            accounts,
            target_user_id: self.target_user_id,
            target_name: self.target_name.clone(),
        }
    }
}

static FOLLOW_STATE: std::sync::LazyLock<std::sync::Mutex<FollowSnapshot>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(FollowRun::default().snapshot()));

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FollowResult {
    total: usize,
    followed: usize,
    already_following: usize,
    failed: usize,
    /// O alvo estava entre as contas escolhidas e ficou de fora.
    skipped_target: bool,
}

/// Uma conta segue o alvo. Leitura "já segue?" com falha não impede o follow:
/// na dúvida, segue (seguir de novo não duplica nada no Roblox).
async fn follow_one(store: &AccountStore, user_id: i64, target_user_id: i64) -> Result<bool, String> {
    let cookie = get_cookie(store, user_id)?;
    let csrf = api::auth::get_csrf_token(&cookie).await.map_err(session_hint)?;
    if let Ok(true) = api::roblox::is_following_with_csrf(&cookie, &csrf, target_user_id).await {
        return Ok(false);
    }
    api::roblox::follow_user_with_csrf(&cookie, &csrf, target_user_id)
        .await
        .map_err(session_hint)?;
    Ok(true)
}

/// Sessão vencida não é "consertada" com refresh (derrubaria os clientes da
/// conta): a mensagem diz o que fazer.
fn session_hint(err: String) -> String {
    if is_auth_session_error(&err) {
        format!("{} (invalid session — log the account in again)", err)
    } else {
        err
    }
}

/// O lote em si, sem `AppHandle` — os testes chamam direto. `publish` recebe
/// o estado a cada mudança.
async fn run_follow_batch(
    store: &AccountStore,
    ids: &[i64],
    target_user_id: i64,
    target_name: Option<String>,
    delay: std::time::Duration,
    mut publish: impl FnMut(&FollowRun),
) -> FollowRun {
    let mut run = FollowRun::default();
    run.start(ids, target_user_id, target_name);
    publish(&run);

    for (idx, &uid) in ids.iter().enumerate() {
        run.set(uid, "processing", None);
        publish(&run);
        let sent = match follow_one(store, uid, target_user_id).await {
            Ok(true) => {
                run.set(uid, "followed", None);
                true
            }
            Ok(false) => {
                run.set(uid, "already", None);
                false
            }
            Err(e) => {
                run.set(uid, "failed", Some(e));
                true
            }
        };
        publish(&run);
        // Só espera depois de um pedido de verdade: quem já seguia não gastou
        // a cota de escrita.
        if sent && delay > std::time::Duration::ZERO && idx + 1 < ids.len() {
            tokio::time::sleep(delay).await;
        }
    }

    run.finish();
    publish(&run);
    run
}

/// Faz as contas escolhidas seguirem `target_user_id`. Progresso no evento
/// `follow-user-state` (uma entrada por conta), como o Make Friends.
#[tauri::command]
async fn follow_user_with_accounts(
    app: tauri::AppHandle,
    state: tauri::State<'_, AccountStore>,
    settings: tauri::State<'_, SettingsStore>,
    user_ids: Vec<i64>,
    target_user_id: i64,
    target_name: Option<String>,
    delay_ms: Option<u64>,
) -> Result<FollowResult, String> {
    if target_user_id <= 0 {
        return Err("Invalid user to follow.".to_string());
    }
    let (ids, skipped_target) = plan_followers(user_ids, target_user_id);
    if ids.is_empty() {
        return Err("Select at least one account other than the one being followed.".to_string());
    }
    let _running = try_begin_follow()?;

    let delay = std::time::Duration::from_millis(resolve_friend_delay_ms(
        delay_ms,
        settings.get_int("Friends", "RequestDelayMs"),
    ));
    let run = run_follow_batch(state.inner(), &ids, target_user_id, target_name, delay, |run| {
        let snapshot = run.snapshot();
        if let Ok(mut current) = FOLLOW_STATE.lock() {
            *current = snapshot.clone();
        }
        let _ = app.emit("follow-user-state", snapshot);
    })
    .await;

    Ok(FollowResult {
        total: ids.len(),
        followed: run.count("followed"),
        already_following: run.count("already"),
        failed: run.count("failed"),
        skipped_target,
    })
}

/// Retrato atual, para a tela que abre (ou remonta) no meio do lote.
#[tauri::command]
fn get_follow_user_state() -> FollowSnapshot {
    FOLLOW_STATE
        .lock()
        .map(|s| s.clone())
        .unwrap_or_else(|_| FollowRun::default().snapshot())
}
