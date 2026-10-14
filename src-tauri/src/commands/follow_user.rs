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
    /// `pending` | `processing` | `browser` (a janela pediu a pessoa) |
    /// `followed` | `already` | `failed`
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

/// Uma conta segue o alvo **pela API** (`POST friends/v1/users/<id>/follow`).
/// Desde 14/10/2026 é só o plano B (navegador não abriu) e o caminho dos
/// testes: no teste real, as contas levaram "verification (captcha)" (403) por
/// ela, e o clique em "Seguir" no perfil, no navegador da conta, seguiu na hora.
/// Leitura "já segue?" com falha não impede o follow: na dúvida, segue.
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

type FollowFuture<'a> = std::pin::Pin<Box<dyn std::future::Future<Output = Result<bool, String>> + Send + 'a>>;

/// Como uma conta segue o alvo (`Ok(true)` = seguiu, `Ok(false)` = já seguia).
/// `needs_you` avisa que a janela apareceu e a pessoa tem algo a fazer nela.
trait Follower: Sync {
    fn follow<'a>(
        &'a self,
        user_id: i64,
        target_user_id: i64,
        needs_you: &'a tokio::sync::mpsc::UnboundedSender<()>,
    ) -> FollowFuture<'a>;
}

/// Pela API (plano B e testes).
struct ApiFollower<'a> {
    store: &'a AccountStore,
}

impl Follower for ApiFollower<'_> {
    fn follow<'a>(
        &'a self,
        user_id: i64,
        target_user_id: i64,
        _needs_you: &'a tokio::sync::mpsc::UnboundedSender<()>,
    ) -> FollowFuture<'a> {
        Box::pin(follow_one(self.store, user_id, target_user_id))
    }
}

/// O perfil do alvo aberto no navegador da conta, visto pelo fluxo (o real é
/// `AccountPageBrowser`; nos testes, um dublê).
trait FollowPage: Send {
    fn click_follow(&mut self) -> PageFuture<'_, Result<bool, String>>;
    fn show(&mut self) -> PageFuture<'_, ()>;
    fn is_closed(&mut self) -> bool;
    fn close(self: Box<Self>) -> PageFuture<'static, ()>;
}

impl FollowPage for crate::chromium::commands::AccountPageBrowser {
    fn click_follow(&mut self) -> PageFuture<'_, Result<bool, String>> {
        Box::pin(crate::chromium::commands::AccountPageBrowser::click_follow(self))
    }
    fn show(&mut self) -> PageFuture<'_, ()> {
        Box::pin(crate::chromium::commands::AccountPageBrowser::show(self))
    }
    fn is_closed(&mut self) -> bool {
        crate::chromium::commands::AccountPageBrowser::is_closed(self)
    }
    fn close(self: Box<Self>) -> PageFuture<'static, ()> {
        Box::pin(crate::chromium::commands::AccountPageBrowser::close(*self))
    }
}

/// Seguir pelo navegador da conta, como a entrada nos grupos
/// (`browser_join_flow`, mesmos tempos):
///
/// 1. já segue? Então nem abre janela;
/// 2. abre o perfil minimizado, abre o menu e clica em "Seguir";
/// 3. confere a cada `poll`; seguiu → fecha a janela;
/// 4. não seguiu em `quiet` (ou não achou o "Seguir") → a janela aparece e o
///    lote é avisado: o Roblox pediu algo, a pessoa termina lá;
/// 5. `total` sem seguir, ou a janela fechada → erro só daquela conta.
///
/// `Err` = o navegador nem abriu (quem chama cai para a API).
async fn browser_follow_flow<C, CF, O>(
    is_following: C,
    open: O,
    timings: BrowserJoinTimings,
    needs_you: &tokio::sync::mpsc::UnboundedSender<()>,
) -> Result<Result<bool, String>, String>
where
    C: Fn() -> CF,
    CF: std::future::Future<Output = Result<bool, String>>,
    O: std::future::Future<Output = Result<Box<dyn FollowPage>, String>>,
{
    if let Ok(true) = is_following().await {
        return Ok(Ok(false));
    }
    let mut page = open.await?;
    let clicked = match page.click_follow().await {
        Ok(clicked) => clicked,
        Err(e) => {
            page.close().await;
            return Ok(Err(e));
        }
    };
    let mut shown = false;
    if !clicked {
        page.show().await;
        let _ = needs_you.send(());
        shown = true;
    }
    let started = std::time::Instant::now();
    loop {
        tokio::time::sleep(timings.poll).await;
        if let Ok(true) = is_following().await {
            page.close().await;
            return Ok(Ok(true));
        }
        if page.is_closed() {
            return Ok(Err("The browser window was closed before the account followed".to_string()));
        }
        let elapsed = started.elapsed();
        if !shown && elapsed >= timings.quiet {
            page.show().await;
            let _ = needs_you.send(());
            shown = true;
        }
        if elapsed >= timings.total {
            page.close().await;
            return Ok(Err("Roblox asked for something in the browser and it wasn't finished".to_string()));
        }
    }
}

/// O caminho real: o perfil no navegador da conta; se ele não abrir, a API.
struct BrowserFollower<'a> {
    app: &'a tauri::AppHandle,
    store: &'a AccountStore,
}

impl Follower for BrowserFollower<'_> {
    fn follow<'a>(
        &'a self,
        user_id: i64,
        target_user_id: i64,
        needs_you: &'a tokio::sync::mpsc::UnboundedSender<()>,
    ) -> FollowFuture<'a> {
        Box::pin(async move {
            let cookie = get_cookie(self.store, user_id)?;
            let csrf = api::auth::get_csrf_token(&cookie).await.map_err(session_hint)?;
            let is_following = || api::roblox::is_following_with_csrf(&cookie, &csrf, target_user_id);
            let open = async {
                crate::chromium::commands::AccountPageBrowser::open_profile(self.app, user_id, &cookie, target_user_id)
                    .await
                    .map(|page| Box::new(page) as Box<dyn FollowPage>)
            };
            match browser_follow_flow(is_following, open, BROWSER_JOIN_TIMINGS, needs_you).await {
                Ok(result) => result,
                Err(e) => {
                    eprintln!("[follow] browser unavailable user={} target={}: {}", user_id, target_user_id, e);
                    ApiFollower { store: self.store }
                        .follow(user_id, target_user_id, needs_you)
                        .await
                        .map_err(|api| format!("Could not open the browser ({}); {}", e, api))
                }
            }
        })
    }
}

/// Uma conta, do jeito do `follower`, avisando quando a janela pede a pessoa.
async fn drive_follow(
    follower: &dyn Follower,
    user_id: i64,
    target_user_id: i64,
    mut on_needs_you: impl FnMut(),
) -> Result<bool, String> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    let mut follow = follower.follow(user_id, target_user_id, &tx);
    loop {
        tokio::select! {
            result = &mut follow => return result,
            Some(()) = rx.recv() => on_needs_you(),
        }
    }
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
    follower: &dyn Follower,
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
        let result = drive_follow(follower, uid, target_user_id, || {
            run.set(uid, "browser", None);
            publish(&run);
        })
        .await;
        let sent = match result {
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
    let follower = BrowserFollower { app: &app, store: state.inner() };
    let run = run_follow_batch(&follower, &ids, target_user_id, target_name, delay, |run| {
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
