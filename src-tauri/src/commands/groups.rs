// Página Groups: busca de grupos, ícones, entrada em lote (uma conta por vez,
// com pausa entre elas) e a conferência de participação depois de um captcha.
// Ver docs/features/groups.md.
//
// Nada aqui usa `run_with_session_retry`: o refresh derruba as outras sessões
// da conta. O cookie sai direto do store (`get_cookie`); cookie vencido só
// falha aquela conta.


/// Retrato inteiro do lote, publicado a cada passo em `groups-join-state`.
#[derive(Debug, Clone, Default, serde::Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GroupJoinSnapshot {
    pub running: bool,
    pub group_id: Option<i64>,
    pub group_name: String,
    pub total: usize,
    pub done: usize,
    pub current_user_id: Option<i64>,
    pub accounts: Vec<GroupJoinAccountResult>,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GroupJoinAccountResult {
    pub user_id: i64,
    /// "waiting" | "joining" | "joined" | "pending" | "alreadyMember" |
    /// "challenge" | "failed" | "cancelled" | "notMember"
    pub status: String,
    /// Motivo da falha (ou a mensagem do desafio), como o Roblox escreveu.
    pub reason: Option<String>,
    /// Só em "challenge": o `rblx-challenge-type` ("captcha", "proofofwork",
    /// "unknown"...). A tela diz "captcha" só quando é captcha mesmo.
    pub challenge_type: Option<String>,
    /// Diagnóstico curto do que o Roblox respondeu ("HTTP 403 · code 0 ·
    /// ..."), para o dono ver na linha da conta. Nunca leva cookie/token.
    pub detail: Option<String>,
}

impl GroupJoinAccountResult {
    fn waiting(user_id: i64) -> Self {
        Self { user_id, status: "waiting".to_string(), reason: None, challenge_type: None, detail: None }
    }
}

/// O que uma tentativa deixa na linha da conta.
#[derive(Debug, Clone, PartialEq)]
struct JoinRowUpdate {
    status: &'static str,
    reason: Option<String>,
    challenge_type: Option<String>,
    detail: Option<String>,
}

impl JoinRowUpdate {
    fn failed(reason: String) -> Self {
        Self { status: "failed", reason: Some(reason), challenge_type: None, detail: None }
    }

    fn apply(self, row: &mut GroupJoinAccountResult) {
        row.status = self.status.to_string();
        row.reason = self.reason;
        row.challenge_type = self.challenge_type;
        row.detail = self.detail;
    }
}

/// Pausa entre uma conta e a próxima: sorteada entre `min` e `max`.
#[derive(Debug, Clone, Copy)]
struct JoinPacing {
    min: Duration,
    max: Duration,
}

/// Número aleatório do sistema (pausas e sorteios dos lotes). Mora aqui, e não
/// no lote de avatares, porque este arquivo entra nas duas edições e o
/// `avatar_batch.rs` só na completa — os dois usam.
fn os_random_u64() -> u64 {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).expect("OS RNG unavailable");
    u64::from_le_bytes(bytes)
}

/// Mantém a ordem e descarta repetidos (contas escolhidas para um lote).
fn dedupe_keep_order<T: Eq + std::hash::Hash + Clone>(items: Vec<T>) -> Vec<T> {
    let mut seen = std::collections::HashSet::new();
    items.into_iter().filter(|i| seen.insert(i.clone())).collect()
}

const JOIN_PACING: JoinPacing = JoinPacing {
    min: Duration::from_secs(2),
    max: Duration::from_secs(4),
};

/// Fatia da pausa: o Cancel vale no meio dela, não só no fim.
const PAUSE_SLICE: Duration = Duration::from_millis(100);

fn pause_length(pacing: JoinPacing, next_u64: &mut dyn FnMut() -> u64) -> Duration {
    let min = pacing.min.as_millis() as u64;
    let max = (pacing.max.as_millis() as u64).max(min);
    let span = max - min;
    let extra = if span == 0 { 0 } else { next_u64() % (span + 1) };
    Duration::from_millis(min + extra)
}

/// Dorme `length` em fatias. `true` = o Cancel chegou no meio.
async fn cancellable_pause(length: Duration, cancel: &std::sync::atomic::AtomicBool) -> bool {
    let mut left = length;
    while !left.is_zero() {
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            return true;
        }
        let step = left.min(PAUSE_SLICE);
        tokio::time::sleep(step).await;
        left -= step;
    }
    cancel.load(std::sync::atomic::Ordering::SeqCst)
}

fn outcome_status(outcome: api::roblox::GroupJoinOutcome) -> (&'static str, Option<String>) {
    use api::roblox::GroupJoinOutcome;
    match outcome {
        GroupJoinOutcome::Joined => ("joined", None),
        GroupJoinOutcome::Pending => ("pending", None),
        GroupJoinOutcome::AlreadyMember => ("alreadyMember", None),
        GroupJoinOutcome::ChallengeRequired { .. } => ("challenge", None),
        GroupJoinOutcome::Failed(message) => ("failed", Some(message)),
    }
}

fn attempt_row(attempt: api::roblox::GroupJoinAttempt) -> JoinRowUpdate {
    let detail = attempt.diagnostic();
    let message = attempt.error_message.clone();
    match attempt.outcome {
        api::roblox::GroupJoinOutcome::ChallengeRequired { challenge_type } => JoinRowUpdate {
            status: "challenge",
            reason: message,
            challenge_type: Some(challenge_type),
            detail,
        },
        outcome => {
            let (status, reason) = outcome_status(outcome);
            JoinRowUpdate { status, reason, challenge_type: None, detail }
        }
    }
}

/// Registro de depuração da entrada (o backend só tem `eprintln!`). Leva o
/// usuário, o grupo e o que o Roblox respondeu — nunca cookie nem token.
fn log_join_attempt(user_id: i64, group_id: i64, attempt: &api::roblox::GroupJoinAttempt) {
    let challenge = match &attempt.outcome {
        api::roblox::GroupJoinOutcome::ChallengeRequired { challenge_type } => challenge_type.as_str(),
        _ => "-",
    };
    eprintln!(
        "[groups] join user={} group={} status={} challenge={} code={} message={:?}",
        user_id,
        group_id,
        attempt.http_status.map(|s| s.to_string()).unwrap_or_else(|| "none".to_string()),
        challenge,
        attempt.error_code.map(|c| c.to_string()).unwrap_or_else(|| "-".to_string()),
        attempt.error_message.as_deref().unwrap_or("")
    );
}

/// Uma conta, uma tentativa. Sem cookie = falha só dela.
async fn join_one_account(
    user_id: i64,
    group: &api::roblox::GroupSummary,
    cookie_of: &(dyn Fn(i64) -> Result<String, String> + Sync),
) -> JoinRowUpdate {
    match cookie_of(user_id) {
        Ok(cookie) if !cookie.trim().is_empty() => {
            let attempt = api::roblox::join_group_attempt(&cookie, group.id, !group.public_entry_allowed).await;
            log_join_attempt(user_id, group.id, &attempt);
            attempt_row(attempt)
        }
        Ok(_) => JoinRowUpdate::failed("Account has no cookie".to_string()),
        Err(e) => JoinRowUpdate::failed(e),
    }
}

// ---- Como cada conta entra ---------------------------------------------------

type JoinFuture<'a> = std::pin::Pin<Box<dyn std::future::Future<Output = JoinRowUpdate> + Send + 'a>>;
type PageFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// Como uma conta entra no grupo. O lote não sabe se é pelo navegador (o
/// caminho real) ou pela API (plano B, e os testes). `needs_you` avisa o lote
/// que a janela apareceu e a pessoa tem algo a fazer nela.
trait GroupJoiner: Sync {
    fn join<'a>(
        &'a self,
        user_id: i64,
        group: &'a api::roblox::GroupSummary,
        cancel: &'a std::sync::atomic::AtomicBool,
        needs_you: &'a tokio::sync::mpsc::UnboundedSender<()>,
    ) -> JoinFuture<'a>;
}

/// Entrada pela API (`POST groups/v1/groups/<id>/users`). Desde 13/10/2026 é
/// só o plano B, quando o navegador não abre: medido com contas de teste, o
/// Roblox responde a ela com um quebra-cabeça (`rostile`) que só o site mostra.
struct ApiJoiner<'c> {
    cookie_of: &'c (dyn Fn(i64) -> Result<String, String> + Sync),
}

impl GroupJoiner for ApiJoiner<'_> {
    fn join<'a>(
        &'a self,
        user_id: i64,
        group: &'a api::roblox::GroupSummary,
        _cancel: &'a std::sync::atomic::AtomicBool,
        _needs_you: &'a tokio::sync::mpsc::UnboundedSender<()>,
    ) -> JoinFuture<'a> {
        Box::pin(join_one_account(user_id, group, self.cookie_of))
    }
}

/// A página do grupo aberta no navegador da conta, vista pelo fluxo (a real é
/// `chromium::commands::GroupJoinBrowser`; nos testes, um dublê).
trait JoinPage: Send {
    fn click_join(&mut self) -> PageFuture<'_, Result<bool, String>>;
    fn show(&mut self) -> PageFuture<'_, ()>;
    fn is_closed(&mut self) -> bool;
    fn close(self: Box<Self>) -> PageFuture<'static, ()>;
}

impl JoinPage for crate::chromium::commands::GroupJoinBrowser {
    fn click_join(&mut self) -> PageFuture<'_, Result<bool, String>> {
        Box::pin(crate::chromium::commands::GroupJoinBrowser::click_join(self))
    }
    fn show(&mut self) -> PageFuture<'_, ()> {
        Box::pin(crate::chromium::commands::GroupJoinBrowser::show(self))
    }
    fn is_closed(&mut self) -> bool {
        crate::chromium::commands::GroupJoinBrowser::is_closed(self)
    }
    fn close(self: Box<Self>) -> PageFuture<'static, ()> {
        Box::pin(crate::chromium::commands::GroupJoinBrowser::close(*self))
    }
}

/// Tempos da entrada pelo navegador.
#[derive(Debug, Clone, Copy)]
struct BrowserJoinTimings {
    /// Intervalo entre duas conferências de participação.
    poll: Duration,
    /// Depois do clique, quanto esperar minimizado antes de mostrar a janela
    /// (o Roblox pediu algo — quebra-cabeça, confirmação).
    quiet: Duration,
    /// Quanto esperar a pessoa terminar na janela antes de desistir da conta.
    total: Duration,
}

const BROWSER_JOIN_TIMINGS: BrowserJoinTimings = BrowserJoinTimings {
    poll: Duration::from_secs(2),
    quiet: Duration::from_secs(12),
    total: Duration::from_secs(180),
};

fn browser_row(status: &'static str, reason: Option<&str>) -> JoinRowUpdate {
    JoinRowUpdate {
        status,
        reason: reason.map(str::to_string),
        challenge_type: (status == "challenge").then(|| "browser".to_string()),
        detail: None,
    }
}

/// A entrada de uma conta pelo navegador dela:
///
/// 1. já é membro (ou tem pedido pendente)? Então nem abre janela;
/// 2. abre a página do grupo minimizada e clica em "Join Community";
/// 3. confere a participação pela API a cada `poll`. Entrou → fecha a janela;
/// 4. não entrou em `quiet` → mostra a janela e avisa (`needs_you`): o Roblox
///    pediu algo que só a pessoa faz. Nunca resolve nada sozinho;
/// 5. `total` sem entrar, ou a pessoa fechou a janela → a conta fica marcada e
///    o lote segue.
///
/// `Err` = o navegador nem abriu (quem chama cai para a API).
async fn browser_join_flow<M, MF, O>(
    membership: M,
    open: O,
    timings: BrowserJoinTimings,
    cancel: &std::sync::atomic::AtomicBool,
    needs_you: &tokio::sync::mpsc::UnboundedSender<()>,
) -> Result<JoinRowUpdate, String>
where
    M: Fn() -> MF,
    MF: std::future::Future<Output = Result<api::roblox::GroupMembership, String>>,
    O: std::future::Future<Output = Result<Box<dyn JoinPage>, String>>,
{
    use api::roblox::GroupMembership;
    match membership().await {
        Ok(GroupMembership::Member) => return Ok(browser_row("alreadyMember", None)),
        Ok(GroupMembership::Pending) => return Ok(browser_row("pending", None)),
        _ => {}
    }

    let mut page = open.await?;
    let clicked = match page.click_join().await {
        Ok(clicked) => clicked,
        Err(e) => {
            page.close().await;
            return Ok(JoinRowUpdate::failed(e));
        }
    };
    let mut shown = false;
    if !clicked {
        // Sem botão: ou já entrou, ou a página pede algo antes (termos,
        // verificação). A pessoa vê a janela desde já.
        page.show().await;
        let _ = needs_you.send(());
        shown = true;
    }

    let started = std::time::Instant::now();
    loop {
        if cancellable_pause(timings.poll, cancel).await {
            page.close().await;
            return Ok(browser_row("cancelled", None));
        }
        match membership().await {
            Ok(GroupMembership::Member) => {
                page.close().await;
                return Ok(browser_row("joined", None));
            }
            Ok(GroupMembership::Pending) => {
                page.close().await;
                return Ok(browser_row("pending", None));
            }
            _ => {}
        }
        if page.is_closed() {
            return Ok(browser_row(
                "notMember",
                Some("The browser window was closed before the account joined"),
            ));
        }
        let elapsed = started.elapsed();
        if !shown && elapsed >= timings.quiet {
            page.show().await;
            let _ = needs_you.send(());
            shown = true;
        }
        if elapsed >= timings.total {
            page.close().await;
            return Ok(browser_row(
                "challenge",
                Some("Roblox asked for something in the browser and it wasn't finished"),
            ));
        }
    }
}

/// O caminho real: navegador da conta; se ele não abrir, a API.
struct BrowserJoiner<'a> {
    app: &'a tauri::AppHandle,
    store: &'a AccountStore,
}

impl GroupJoiner for BrowserJoiner<'_> {
    fn join<'a>(
        &'a self,
        user_id: i64,
        group: &'a api::roblox::GroupSummary,
        cancel: &'a std::sync::atomic::AtomicBool,
        needs_you: &'a tokio::sync::mpsc::UnboundedSender<()>,
    ) -> JoinFuture<'a> {
        Box::pin(async move {
            let cookie = match get_cookie(self.store, user_id) {
                Ok(cookie) if !cookie.trim().is_empty() => cookie,
                Ok(_) => return JoinRowUpdate::failed("Account has no cookie".to_string()),
                Err(e) => return JoinRowUpdate::failed(e),
            };
            let membership = || api::roblox::group_membership(&cookie, user_id, group.id);
            let open = async {
                crate::chromium::commands::GroupJoinBrowser::open(self.app, user_id, &cookie, group.id)
                    .await
                    .map(|page| Box::new(page) as Box<dyn JoinPage>)
            };
            match browser_join_flow(membership, open, BROWSER_JOIN_TIMINGS, cancel, needs_you).await {
                Ok(update) => {
                    eprintln!("[groups] browser join user={} group={} status={}", user_id, group.id, update.status);
                    update
                }
                Err(e) => {
                    eprintln!("[groups] browser unavailable user={} group={}: {}", user_id, group.id, e);
                    let cookie_of = |_: i64| Ok(cookie.clone());
                    let mut update = join_one_account(user_id, group, &cookie_of).await;
                    if update.status == "challenge" || update.status == "failed" {
                        update.reason = Some(format!("Could not open the browser: {}", e));
                    }
                    update
                }
            }
        })
    }
}

/// Uma conta, do jeito do `joiner`, avisando quando a janela pede a pessoa.
async fn drive_join(
    joiner: &dyn GroupJoiner,
    user_id: i64,
    group: &api::roblox::GroupSummary,
    cancel: &std::sync::atomic::AtomicBool,
    mut on_needs_you: impl FnMut(),
) -> JoinRowUpdate {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    let mut join = joiner.join(user_id, group, cancel, &tx);
    loop {
        tokio::select! {
            update = &mut join => return update,
            Some(()) = rx.recv() => on_needs_you(),
        }
    }
}

fn membership_status(membership: api::roblox::GroupMembership) -> &'static str {
    match membership {
        api::roblox::GroupMembership::Member => "joined",
        api::roblox::GroupMembership::Pending => "pending",
        api::roblox::GroupMembership::NotMember => "notMember",
    }
}

fn mark_remaining_cancelled(snapshot: &mut GroupJoinSnapshot, from: usize) {
    for row in snapshot.accounts.iter_mut().skip(from) {
        if row.status == "waiting" {
            row.status = "cancelled".to_string();
        }
    }
}

/// O lote em si, sem Tauri: entra com cada conta, na ordem, uma por vez.
/// Conta que esbarra num pedido do Roblox fica `browser` (a janela apareceu)
/// enquanto a pessoa resolve, e `challenge` se não resolver — o lote nunca
/// tenta resolver nada sozinho.
#[allow(clippy::too_many_arguments)]
async fn run_group_join(
    user_ids: &[i64],
    group: &api::roblox::GroupSummary,
    pacing: JoinPacing,
    cancel: &std::sync::atomic::AtomicBool,
    joiner: &dyn GroupJoiner,
    next_u64: &mut (dyn FnMut() -> u64 + Send),
    publish: &mut (dyn FnMut(&GroupJoinSnapshot) + Send),
) -> GroupJoinSnapshot {
    let mut snapshot = GroupJoinSnapshot {
        running: true,
        group_id: Some(group.id),
        group_name: group.name.clone(),
        total: user_ids.len(),
        done: 0,
        current_user_id: None,
        accounts: user_ids.iter().map(|&user_id| GroupJoinAccountResult::waiting(user_id)).collect(),
    };
    publish(&snapshot);

    for (index, &user_id) in user_ids.iter().enumerate() {
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            mark_remaining_cancelled(&mut snapshot, index);
            break;
        }
        if index > 0 && cancellable_pause(pause_length(pacing, next_u64), cancel).await {
            mark_remaining_cancelled(&mut snapshot, index);
            break;
        }

        snapshot.accounts[index].status = "joining".to_string();
        snapshot.current_user_id = Some(user_id);
        publish(&snapshot);

        let update = drive_join(joiner, user_id, group, cancel, || {
            snapshot.accounts[index].status = "browser".to_string();
            publish(&snapshot);
        })
        .await;
        update.apply(&mut snapshot.accounts[index]);
        snapshot.done = index + 1;
        snapshot.current_user_id = None;
        publish(&snapshot);
    }

    snapshot.running = false;
    snapshot.current_user_id = None;
    publish(&snapshot);
    snapshot
}

/// "Tentar de novo" de uma conta só (depois de aceitar os termos, resolver a
/// verificação etc. no navegador). Mexe só na linha dela; as outras ficam como
/// estavam. Se o retrato é de outro grupo (ou vazio), começa um retrato novo
/// com essa conta.
async fn retry_group_join_account(
    snapshot: &mut GroupJoinSnapshot,
    user_id: i64,
    group: &api::roblox::GroupSummary,
    cancel: &std::sync::atomic::AtomicBool,
    joiner: &dyn GroupJoiner,
    publish: &mut (dyn FnMut(&GroupJoinSnapshot) + Send),
) {
    if snapshot.group_id != Some(group.id) {
        *snapshot = GroupJoinSnapshot {
            group_id: Some(group.id),
            group_name: group.name.clone(),
            ..GroupJoinSnapshot::default()
        };
    }
    snapshot.running = false;
    let index = match snapshot.accounts.iter().position(|row| row.user_id == user_id) {
        Some(index) => index,
        None => {
            snapshot.accounts.push(GroupJoinAccountResult::waiting(user_id));
            snapshot.total += 1;
            snapshot.done += 1;
            snapshot.accounts.len() - 1
        }
    };
    let row = &mut snapshot.accounts[index];
    row.status = "joining".to_string();
    row.reason = None;
    row.challenge_type = None;
    row.detail = None;
    snapshot.current_user_id = Some(user_id);
    publish(snapshot);

    let update = drive_join(joiner, user_id, group, cancel, || {
        snapshot.accounts[index].status = "browser".to_string();
        publish(snapshot);
    })
    .await;
    update.apply(&mut snapshot.accounts[index]);
    snapshot.current_user_id = None;
    publish(snapshot);
}

/// Põe o resultado de uma conferência no retrato (se for do mesmo grupo).
fn apply_membership_check(snapshot: &mut GroupJoinSnapshot, user_id: i64, group_id: i64, status: &str) -> bool {
    if snapshot.group_id != Some(group_id) {
        return false;
    }
    match snapshot.accounts.iter_mut().find(|row| row.user_id == user_id) {
        Some(row) => {
            row.status = status.to_string();
            row.reason = None;
            row.challenge_type = None;
            row.detail = None;
            true
        }
        None => false,
    }
}

// ---- Estado do lote --------------------------------------------------------

/// Um lote por vez: dois lotes juntos dobrariam os pedidos ao Roblox.
static GROUP_JOIN_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static GROUP_JOIN_CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static GROUP_JOIN: std::sync::LazyLock<std::sync::Mutex<GroupJoinSnapshot>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(GroupJoinSnapshot::default()));

struct GroupJoinRunGuard;

impl Drop for GroupJoinRunGuard {
    fn drop(&mut self) {
        GROUP_JOIN_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

fn try_begin_group_join() -> Result<GroupJoinRunGuard, String> {
    if GROUP_JOIN_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("A group join is already running".to_string());
    }
    Ok(GroupJoinRunGuard)
}

fn publish_group_join(app: &tauri::AppHandle, snapshot: &GroupJoinSnapshot) {
    if let Ok(mut state) = GROUP_JOIN.lock() {
        *state = snapshot.clone();
    }
    let _ = app.emit("groups-join-state", snapshot.clone());
}

// ---- Comandos --------------------------------------------------------------

/// Leitura com fallback: primeiro sem cookie; se o Roblox recusar, uma vez com
/// o cookie de uma conta (sem refresh, como a presença).
async fn read_with_viewer_fallback<T, F, Fut>(state: &AccountStore, read: F) -> Result<T, String>
where
    F: Fn(Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    match read(None).await {
        Ok(value) => Ok(value),
        Err(error) => match state.get_all().ok().and_then(|accounts| pick_viewer_cookie(&accounts)) {
            Some(cookie) => read(Some(cookie)).await.map_err(|_| error),
            None => Err(error),
        },
    }
}

/// Busca por palavra, ou um grupo só quando a pessoa colou link/id.
#[tauri::command]
async fn groups_search(
    state: tauri::State<'_, AccountStore>,
    query: String,
    cursor: Option<String>,
) -> Result<api::roblox::GroupSearchPage, String> {
    if let Some(group_id) = api::roblox::parse_group_reference(&query) {
        let group = read_with_viewer_fallback(state.inner(), |cookie| async move {
            api::roblox::get_group(cookie.as_deref(), group_id).await
        })
        .await?;
        return Ok(api::roblox::GroupSearchPage { groups: vec![group], next_cursor: None });
    }

    let keyword = query.trim().to_string();
    if keyword.chars().count() < 2 {
        return Err("Type at least 2 characters".to_string());
    }
    read_with_viewer_fallback(state.inner(), |cookie| {
        let keyword = keyword.clone();
        let cursor = cursor.clone();
        async move { api::roblox::search_groups(cookie.as_deref(), &keyword, cursor.as_deref()).await }
    })
    .await
}

const GROUP_ICON_TYPE: &str = "GroupIcon";

/// Ícones dos grupos, pelo mesmo lote/cache das fotos das contas.
#[tauri::command]
async fn groups_icons(
    image_cache: tauri::State<'_, ImageCache>,
    group_ids: Vec<i64>,
) -> Result<Vec<api::batch::CachedThumbnail>, String> {
    let requests = group_ids
        .iter()
        .map(|&id| (id, GROUP_ICON_TYPE.to_string(), "150x150".to_string(), "png".to_string()))
        .collect();
    Ok(image_cache
        .get_images_batch(requests)
        .await
        .into_iter()
        .map(|(target_id, image_url)| api::batch::CachedThumbnail {
            target_id,
            image_url,
            thumbnail_type: GROUP_ICON_TYPE.to_string(),
        })
        .collect())
}

#[tauri::command]
fn get_groups_join_state() -> GroupJoinSnapshot {
    GROUP_JOIN.lock().map(|s| s.clone()).unwrap_or_default()
}

#[tauri::command]
fn groups_cancel_join() {
    GROUP_JOIN_CANCEL.store(true, std::sync::atomic::Ordering::SeqCst);
}

#[tauri::command]
async fn groups_join_batch(
    app: tauri::AppHandle,
    account_store: tauri::State<'_, AccountStore>,
    user_ids: Vec<i64>,
    group_id: i64,
) -> Result<GroupJoinSnapshot, String> {
    let _running = try_begin_group_join()?;
    GROUP_JOIN_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);

    let user_ids = dedupe_keep_order(user_ids.into_iter().filter(|id| *id > 0).collect());
    if user_ids.is_empty() {
        return Err("No account selected".to_string());
    }
    if group_id <= 0 {
        return Err("Pick a group first".to_string());
    }
    // Detalhes de novo: o grupo pode ter mudado entre a busca e o clique, e é
    // daqui que sai o "pendente" de quem pede aprovação.
    let group = api::roblox::get_group(None, group_id)
        .await
        .map_err(|e| format!("Could not read the group: {}", e))?;
    if group.is_locked {
        return Err("This group is locked and does not accept new members".to_string());
    }

    let joiner = BrowserJoiner { app: &app, store: account_store.inner() };
    let mut publish = |snapshot: &GroupJoinSnapshot| publish_group_join(&app, snapshot);
    Ok(run_group_join(
        &user_ids,
        &group,
        JOIN_PACING,
        &GROUP_JOIN_CANCEL,
        &joiner,
        &mut os_random_u64,
        &mut publish,
    )
    .await)
}

/// "Tentar de novo" numa conta só. Usa a mesma trava do lote: não roda junto
/// de um lote (nem de outro "tentar de novo").
#[tauri::command]
async fn groups_join_retry(
    app: tauri::AppHandle,
    account_store: tauri::State<'_, AccountStore>,
    user_id: i64,
    group_id: i64,
) -> Result<GroupJoinSnapshot, String> {
    let _running = try_begin_group_join()?;
    if user_id <= 0 {
        return Err("No account selected".to_string());
    }
    if group_id <= 0 {
        return Err("Pick a group first".to_string());
    }
    let group = api::roblox::get_group(None, group_id)
        .await
        .map_err(|e| format!("Could not read the group: {}", e))?;
    if group.is_locked {
        return Err("This group is locked and does not accept new members".to_string());
    }

    GROUP_JOIN_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);
    let joiner = BrowserJoiner { app: &app, store: account_store.inner() };
    let mut snapshot = GROUP_JOIN.lock().map(|s| s.clone()).unwrap_or_default();
    let mut publish = |snapshot: &GroupJoinSnapshot| publish_group_join(&app, snapshot);
    retry_group_join_account(&mut snapshot, user_id, &group, &GROUP_JOIN_CANCEL, &joiner, &mut publish).await;
    Ok(snapshot)
}

/// "Join Group" do painel de uma conta: a mesma entrada da página Groups (pelo
/// navegador da conta), para uma conta só. Devolve `"joined"`, `"pending"` ou
/// `"alreadyMember"`; o resto é erro com o motivo.
///
/// Até 13/10/2026 este comando passava por `run_with_session_retry`: um 401
/// renovava a sessão, o que desloga a conta de todas as sessões (CLAUDE.md).
#[tauri::command]
async fn join_group(
    app: tauri::AppHandle,
    account_store: tauri::State<'_, AccountStore>,
    user_id: i64,
    group_id: i64,
) -> Result<String, String> {
    if group_id <= 0 {
        return Err("Invalid group ID".to_string());
    }
    let _running = try_begin_group_join()?;
    GROUP_JOIN_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);
    let group = api::roblox::get_group(None, group_id)
        .await
        .map_err(|e| format!("Could not read the group: {}", e))?;
    if group.is_locked {
        return Err("This group is locked and does not accept new members".to_string());
    }
    let joiner = BrowserJoiner { app: &app, store: account_store.inner() };
    let update = drive_join(&joiner, user_id, &group, &GROUP_JOIN_CANCEL, || {}).await;
    single_join_result(update)
}

/// O que o "Join Group" do painel devolve para uma linha do lote.
fn single_join_result(update: JoinRowUpdate) -> Result<String, String> {
    match update.status {
        "joined" | "pending" | "alreadyMember" => Ok(update.status.to_string()),
        "challenge" => Err(update
            .reason
            .unwrap_or_else(|| "Roblox asked for something that wasn't finished in the browser".to_string())),
        _ => Err(update.reason.unwrap_or_else(|| format!("Could not join ({})", update.status))),
    }
}

/// Quantos detalhes de grupo os "Grupos populares" pedem ao mesmo tempo.
const POPULAR_GROUPS_CONCURRENCY: usize = 4;

/// "Grupos populares" (campo de busca vazio): os dados de agora dos grupos
/// curados em `POPULAR_GROUP_IDS`, maior primeiro. Leitura pública, sem
/// cookie. Nenhum respondeu = erro (a tela volta para a dica).
#[tauri::command]
async fn groups_popular() -> Result<Vec<api::roblox::GroupSummary>, String> {
    let groups =
        api::roblox::get_groups_by_members(&api::roblox::POPULAR_GROUP_IDS, POPULAR_GROUPS_CONCURRENCY).await;
    if groups.is_empty() {
        return Err("Could not load the popular groups".to_string());
    }
    Ok(groups)
}

/// Depois de a pessoa resolver o captcha no navegador da conta: a conta já
/// está no grupo (ou com pedido pendente)? Só leitura, sem refresh de sessão.
#[tauri::command]
async fn groups_check_membership(
    app: tauri::AppHandle,
    account_store: tauri::State<'_, AccountStore>,
    user_id: i64,
    group_id: i64,
) -> Result<String, String> {
    let cookie = get_cookie(&account_store, user_id)?;
    let membership = api::roblox::group_membership(&cookie, user_id, group_id).await?;
    let status = membership_status(membership);
    let updated = GROUP_JOIN.lock().ok().and_then(|mut state| {
        apply_membership_check(&mut state, user_id, group_id, status).then(|| state.clone())
    });
    if let Some(snapshot) = updated {
        let _ = app.emit("groups-join-state", snapshot);
    }
    Ok(status.to_string())
}

#[cfg(test)]
mod group_join_batch_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server, mount_csrf};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockGuard, ResponseTemplate};

    const NO_PAUSE: JoinPacing = JoinPacing { min: Duration::ZERO, max: Duration::ZERO };
    static NO_CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    fn group(id: i64, public_entry_allowed: bool) -> api::roblox::GroupSummary {
        api::roblox::GroupSummary {
            id,
            name: format!("Group {id}"),
            description: String::new(),
            member_count: 10,
            public_entry_allowed,
            has_verified_badge: false,
            is_locked: false,
        }
    }

    fn token(user_id: i64) -> String {
        format!("grp-batch-{user_id}")
    }

    async fn mount_join(user_id: i64, group_id: i64, response: ResponseTemplate, times: u64) -> MockGuard {
        mount_csrf(&token(user_id), &format!("csrf-{}", token(user_id))).await;
        Mock::given(method("POST"))
            .and(path(mock_path("groups", &format!("/v1/groups/{group_id}/users"))))
            .and(header("cookie", cookie_of(&token(user_id))))
            .respond_with(response)
            .expect(times)
            .mount_as_scoped(mock_server().await)
            .await
    }

    fn statuses(snapshot: &GroupJoinSnapshot) -> Vec<(i64, String)> {
        snapshot.accounts.iter().map(|r| (r.user_id, r.status.clone())).collect()
    }

    fn cookies(user_id: i64) -> Result<String, String> {
        Ok(token(user_id))
    }

    #[tokio::test]
    async fn joins_one_account_at_a_time_and_keeps_going_after_a_captcha() {
        let gid = 990_001;
        let error = |code: i64| serde_json::json!({ "errors": [{ "code": code, "message": "m" }] });
        let a = mount_join(6101, gid, ResponseTemplate::new(200), 1).await;
        let b = mount_join(
            6102,
            gid,
            ResponseTemplate::new(403).insert_header("rblx-challenge-id", "c"),
            1,
        )
        .await;
        let c = mount_join(6103, gid, ResponseTemplate::new(409).set_body_json(error(8)), 1).await;
        let d = mount_join(6104, gid, ResponseTemplate::new(429).set_body_json(error(10)), 1).await;

        let mut published: Vec<GroupJoinSnapshot> = Vec::new();
        let final_state = run_group_join(
            &[6101, 6102, 6103, 6104],
            &group(gid, true),
            NO_PAUSE,
            &std::sync::atomic::AtomicBool::new(false),
            &ApiJoiner { cookie_of: &cookies },
            &mut || 0,
            &mut |s| published.push(s.clone()),
        )
        .await;

        assert!(!final_state.running);
        assert_eq!(final_state.done, 4);
        assert_eq!(
            statuses(&final_state),
            vec![
                (6101, "joined".to_string()),
                (6102, "challenge".to_string()),
                (6103, "alreadyMember".to_string()),
                (6104, "failed".to_string()),
            ]
        );
        assert_eq!(final_state.accounts[3].reason.as_deref(), Some("m"));
        // O primeiro retrato já traz a lista inteira esperando.
        assert!(published[0].accounts.iter().all(|r| r.status == "waiting"));
        // Nunca duas contas "joining" ao mesmo tempo.
        assert!(published
            .iter()
            .all(|s| s.accounts.iter().filter(|r| r.status == "joining").count() <= 1));
        drop((a, b, c, d));
    }

    #[tokio::test]
    async fn an_open_group_reports_joined_and_an_approval_group_pending() {
        let gid = 990_002;
        let a = mount_join(6201, gid, ResponseTemplate::new(200), 1).await;
        let final_state = run_group_join(
            &[6201],
            &group(gid, false),
            NO_PAUSE,
            &std::sync::atomic::AtomicBool::new(false),
            &ApiJoiner { cookie_of: &cookies },
            &mut || 0,
            &mut |_| {},
        )
        .await;
        // `public_entry_allowed: false` acima = exige aprovação → pendente.
        assert_eq!(statuses(&final_state), vec![(6201, "pending".to_string())]);
        drop(a);

        let gid = 990_003;
        let a = mount_join(6202, gid, ResponseTemplate::new(200), 1).await;
        let final_state = run_group_join(
            &[6202],
            &group(gid, true),
            NO_PAUSE,
            &std::sync::atomic::AtomicBool::new(false),
            &ApiJoiner { cookie_of: &cookies },
            &mut || 0,
            &mut |_| {},
        )
        .await;
        assert_eq!(statuses(&final_state), vec![(6202, "joined".to_string())]);
        drop(a);
    }

    #[tokio::test]
    async fn a_cancel_before_the_start_sends_nothing() {
        let gid = 990_004;
        let a = mount_join(6301, gid, ResponseTemplate::new(200), 0).await;
        let final_state = run_group_join(
            &[6301],
            &group(gid, true),
            NO_PAUSE,
            &std::sync::atomic::AtomicBool::new(true),
            &ApiJoiner { cookie_of: &cookies },
            &mut || 0,
            &mut |_| {},
        )
        .await;
        assert_eq!(statuses(&final_state), vec![(6301, "cancelled".to_string())]);
        assert_eq!(final_state.done, 0);
        drop(a);
    }

    /// O Cancel chega durante a pausa entre duas contas: a segunda não sai.
    #[tokio::test]
    async fn a_cancel_during_the_pause_stops_before_the_next_account() {
        let gid = 990_005;
        let a = mount_join(6401, gid, ResponseTemplate::new(200), 1).await;
        let b = mount_join(6402, gid, ResponseTemplate::new(200), 0).await;
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let pacing = JoinPacing { min: Duration::from_millis(300), max: Duration::from_millis(300) };
        let final_state = run_group_join(
            &[6401, 6402],
            &group(gid, true),
            pacing,
            &cancel,
            &ApiJoiner { cookie_of: &cookies },
            &mut || 0,
            &mut |s| {
                if s.done == 1 {
                    cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            },
        )
        .await;
        assert_eq!(
            statuses(&final_state),
            vec![(6401, "joined".to_string()), (6402, "cancelled".to_string())]
        );
        drop((a, b));
    }

    #[tokio::test]
    async fn an_account_without_a_cookie_fails_alone() {
        let gid = 990_006;
        let b = mount_join(6502, gid, ResponseTemplate::new(200), 1).await;
        let cookie_of = |user_id: i64| -> Result<String, String> {
            if user_id == 6501 {
                Err("Account 6501 not found".to_string())
            } else {
                Ok(token(user_id))
            }
        };
        let final_state = run_group_join(
            &[6501, 6502],
            &group(gid, true),
            NO_PAUSE,
            &std::sync::atomic::AtomicBool::new(false),
            &ApiJoiner { cookie_of: &cookie_of },
            &mut || 0,
            &mut |_| {},
        )
        .await;
        assert_eq!(
            statuses(&final_state),
            vec![(6501, "failed".to_string()), (6502, "joined".to_string())]
        );
        assert_eq!(final_state.accounts[0].reason.as_deref(), Some("Account 6501 not found"));
        drop(b);
    }

    #[test]
    fn the_pause_stays_between_two_and_four_seconds() {
        for seed in [0u64, 1, 1999, 2000, 2001, u64::MAX] {
            let mut next = move || seed;
            let pause = pause_length(JOIN_PACING, &mut next);
            assert!(pause >= Duration::from_secs(2) && pause <= Duration::from_secs(4), "{pause:?}");
        }
        assert_eq!(pause_length(NO_PAUSE, &mut || 7), Duration::ZERO);
    }

    #[test]
    fn a_membership_check_updates_only_the_same_group() {
        let mut snapshot = GroupJoinSnapshot {
            group_id: Some(5),
            accounts: vec![GroupJoinAccountResult {
                user_id: 1,
                status: "challenge".into(),
                reason: Some("x".into()),
                challenge_type: Some("captcha".into()),
                detail: Some("HTTP 403".into()),
            }],
            ..GroupJoinSnapshot::default()
        };
        assert!(!apply_membership_check(&mut snapshot, 1, 6, "joined"));
        assert_eq!(snapshot.accounts[0].status, "challenge");
        assert!(!apply_membership_check(&mut snapshot, 2, 5, "joined"));
        assert!(apply_membership_check(&mut snapshot, 1, 5, "joined"));
        assert_eq!(snapshot.accounts[0].status, "joined");
        assert_eq!(snapshot.accounts[0].reason, None);
        assert_eq!(snapshot.accounts[0].challenge_type, None);
        assert_eq!(snapshot.accounts[0].detail, None);
    }

    /// O dono viu "Needs captcha" em conta que só tinha o aviso de termos: o
    /// tipo do desafio chega à linha, com o que o Roblox respondeu.
    #[tokio::test]
    async fn a_challenge_row_carries_its_type_and_roblox_answer() {
        let gid = 990_007;
        let body = serde_json::json!({ "errors": [{ "code": 0, "message": "Challenge is required" }] });
        let a = mount_join(
            6601,
            gid,
            ResponseTemplate::new(403)
                .insert_header("rblx-challenge-id", "c")
                .insert_header("rblx-challenge-type", "reauthentication")
                .set_body_json(body),
            1,
        )
        .await;
        let final_state = run_group_join(
            &[6601],
            &group(gid, true),
            NO_PAUSE,
            &std::sync::atomic::AtomicBool::new(false),
            &ApiJoiner { cookie_of: &cookies },
            &mut || 0,
            &mut |_| {},
        )
        .await;
        let row = &final_state.accounts[0];
        assert_eq!(row.status, "challenge");
        assert_eq!(row.challenge_type.as_deref(), Some("reauthentication"));
        assert_eq!(row.reason.as_deref(), Some("Challenge is required"));
        assert_eq!(
            row.detail.as_deref(),
            Some("HTTP 403 · challenge reauthentication · code 0")
        );
        let json = serde_json::to_value(row).unwrap();
        assert_eq!(json["challengeType"], "reauthentication");
        assert!(json.get("detail").is_some());
        drop(a);
    }

    /// "Tentar de novo" mexe só na linha daquela conta.
    #[tokio::test]
    async fn a_retry_updates_only_that_account() {
        let gid = 990_008;
        let a = mount_join(6701, gid, ResponseTemplate::new(200), 1).await;
        let mut snapshot = GroupJoinSnapshot {
            group_id: Some(gid),
            group_name: "Group".into(),
            total: 2,
            done: 2,
            accounts: vec![
                GroupJoinAccountResult {
                    user_id: 6700,
                    status: "joined".into(),
                    reason: None,
                    challenge_type: None,
                    detail: None,
                },
                GroupJoinAccountResult {
                    user_id: 6701,
                    status: "challenge".into(),
                    reason: Some("Challenge is required".into()),
                    challenge_type: Some("generic".into()),
                    detail: Some("HTTP 403".into()),
                },
            ],
            ..GroupJoinSnapshot::default()
        };
        let mut published: Vec<GroupJoinSnapshot> = Vec::new();
        retry_group_join_account(&mut snapshot, 6701, &group(gid, true), &NO_CANCEL, &ApiJoiner { cookie_of: &cookies }, &mut |s| {
            published.push(s.clone())
        })
        .await;
        assert_eq!(
            statuses(&snapshot),
            vec![(6700, "joined".to_string()), (6701, "joined".to_string())]
        );
        assert_eq!(snapshot.accounts[1].challenge_type, None);
        assert_eq!(snapshot.accounts[1].detail, None);
        assert_eq!((snapshot.total, snapshot.done), (2, 2));
        assert!(!snapshot.running);
        assert_eq!(published[0].accounts[1].status, "joining");
        assert_eq!(published.len(), 2);
        drop(a);
    }

    #[tokio::test]
    async fn a_retry_for_another_group_starts_a_fresh_snapshot() {
        let gid = 990_009;
        let a = mount_join(6801, gid, ResponseTemplate::new(200), 1).await;
        let mut snapshot = GroupJoinSnapshot {
            group_id: Some(1),
            accounts: vec![GroupJoinAccountResult::waiting(5)],
            total: 1,
            ..GroupJoinSnapshot::default()
        };
        retry_group_join_account(&mut snapshot, 6801, &group(gid, true), &NO_CANCEL, &ApiJoiner { cookie_of: &cookies }, &mut |_| {}).await;
        assert_eq!(snapshot.group_id, Some(gid));
        assert_eq!(statuses(&snapshot), vec![(6801, "joined".to_string())]);
        assert_eq!((snapshot.total, snapshot.done), (1, 1));
        drop(a);
    }

    #[test]
    fn statuses_use_the_names_the_screen_reads() {
        use api::roblox::{GroupJoinOutcome, GroupMembership};
        assert_eq!(outcome_status(GroupJoinOutcome::Joined).0, "joined");
        assert_eq!(outcome_status(GroupJoinOutcome::Pending).0, "pending");
        assert_eq!(outcome_status(GroupJoinOutcome::AlreadyMember).0, "alreadyMember");
        assert_eq!(
            outcome_status(GroupJoinOutcome::ChallengeRequired { challenge_type: "captcha".into() }).0,
            "challenge"
        );
        assert_eq!(
            outcome_status(GroupJoinOutcome::Failed("why".into())),
            ("failed", Some("why".to_string()))
        );
        assert_eq!(membership_status(GroupMembership::Member), "joined");
        assert_eq!(membership_status(GroupMembership::Pending), "pending");
        assert_eq!(membership_status(GroupMembership::NotMember), "notMember");

        let json = serde_json::to_value(GroupJoinSnapshot {
            group_id: Some(1),
            current_user_id: Some(2),
            ..GroupJoinSnapshot::default()
        })
        .unwrap();
        assert_eq!(json["groupId"], 1);
        assert_eq!(json["currentUserId"], 2);
        assert!(json.get("groupName").is_some());
    }

    #[test]
    fn the_account_panel_join_reports_pending_and_member_as_success() {
        assert_eq!(single_join_result(browser_row("joined", None)), Ok("joined".to_string()));
        assert_eq!(single_join_result(browser_row("pending", None)), Ok("pending".to_string()));
        assert_eq!(single_join_result(browser_row("alreadyMember", None)), Ok("alreadyMember".to_string()));
        assert_eq!(
            single_join_result(browser_row("challenge", Some("finish it"))),
            Err("finish it".to_string())
        );
        assert_eq!(
            single_join_result(JoinRowUpdate::failed("boom".into())),
            Err("boom".to_string())
        );
    }

    #[test]
    fn only_one_batch_runs_at_a_time() {
        let first = try_begin_group_join().expect("first");
        assert_eq!(try_begin_group_join().err().as_deref(), Some("A group join is already running"));
        drop(first);
        assert!(try_begin_group_join().is_ok());
    }
}

/// A entrada pelo navegador (`browser_join_flow`) e o estado `browser` do lote,
/// com uma página e uma participação de mentira — nada de Chromium nem Roblox.
#[cfg(test)]
mod group_browser_join_tests {
    use super::*;
    use api::roblox::GroupMembership;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct PageLog {
        clicks: usize,
        shown: usize,
        closed: bool,
    }

    struct FakePage {
        log: Arc<Mutex<PageLog>>,
        has_button: bool,
        closed_by_user: bool,
    }

    impl JoinPage for FakePage {
        fn click_join(&mut self) -> PageFuture<'_, Result<bool, String>> {
            self.log.lock().unwrap().clicks += 1;
            let has_button = self.has_button;
            Box::pin(async move { Ok(has_button) })
        }
        fn show(&mut self) -> PageFuture<'_, ()> {
            self.log.lock().unwrap().shown += 1;
            Box::pin(async {})
        }
        fn is_closed(&mut self) -> bool {
            self.closed_by_user
        }
        fn close(self: Box<Self>) -> PageFuture<'static, ()> {
            self.log.lock().unwrap().closed = true;
            Box::pin(async {})
        }
    }

    const FAST: BrowserJoinTimings = BrowserJoinTimings {
        poll: Duration::from_millis(1),
        quiet: Duration::from_millis(30),
        total: Duration::from_millis(120),
    };

    type MembershipFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Result<GroupMembership, String>> + Send>>;

    /// Participação que muda com o tempo: cada leitura tira a próxima resposta
    /// da lista (a última se repete).
    fn membership_script(answers: Vec<GroupMembership>) -> impl Fn() -> MembershipFuture {
        let answers = Arc::new(Mutex::new(answers));
        move || {
            let answers = answers.clone();
            Box::pin(async move {
                let mut answers = answers.lock().unwrap();
                Ok(if answers.len() > 1 { answers.remove(0) } else { answers[0].clone() })
            })
        }
    }

    fn page(log: &Arc<Mutex<PageLog>>, has_button: bool, closed_by_user: bool) -> Result<Box<dyn JoinPage>, String> {
        Ok(Box::new(FakePage { log: log.clone(), has_button, closed_by_user }))
    }

    async fn run(
        answers: Vec<GroupMembership>,
        open: Result<Box<dyn JoinPage>, String>,
        cancel: bool,
    ) -> (Result<JoinRowUpdate, String>, usize) {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let cancel = std::sync::atomic::AtomicBool::new(cancel);
        let result = browser_join_flow(membership_script(answers), async { open }, FAST, &cancel, &tx).await;
        let mut needs_you = 0;
        while rx.try_recv().is_ok() {
            needs_you += 1;
        }
        (result, needs_you)
    }

    #[tokio::test]
    async fn an_account_already_in_the_group_never_opens_a_window() {
        let log = Arc::new(Mutex::new(PageLog::default()));
        let (result, _) = run(vec![GroupMembership::Member], page(&log, true, false), false).await;
        assert_eq!(result.unwrap().status, "alreadyMember");
        assert_eq!(log.lock().unwrap().clicks, 0);
        let (result, _) = run(vec![GroupMembership::Pending], page(&log, true, false), false).await;
        assert_eq!(result.unwrap().status, "pending");
        assert_eq!(log.lock().unwrap().clicks, 0);
    }

    /// O caso medido em 13/10/2026: clica, entra, a janela fecha sem nunca ter
    /// aparecido.
    #[tokio::test]
    async fn the_click_joins_and_the_window_closes_without_showing() {
        let log = Arc::new(Mutex::new(PageLog::default()));
        let (result, needs_you) = run(
            vec![GroupMembership::NotMember, GroupMembership::NotMember, GroupMembership::Member],
            page(&log, true, false),
            false,
        )
        .await;
        assert_eq!(result.unwrap().status, "joined");
        let log = log.lock().unwrap();
        assert_eq!((log.clicks, log.shown, log.closed), (1, 0, true));
        assert_eq!(needs_you, 0);
    }

    #[tokio::test]
    async fn an_approval_group_ends_pending() {
        let log = Arc::new(Mutex::new(PageLog::default()));
        let (result, _) = run(
            vec![GroupMembership::NotMember, GroupMembership::Pending],
            page(&log, true, false),
            false,
        )
        .await;
        assert_eq!(result.unwrap().status, "pending");
        assert!(log.lock().unwrap().closed);
    }

    /// O Roblox pediu algo (quebra-cabeça): a janela aparece uma vez, o lote é
    /// avisado, e quando a pessoa termina a conta entra.
    #[tokio::test]
    async fn when_roblox_asks_for_something_the_window_shows_and_the_join_still_counts() {
        let log = Arc::new(Mutex::new(PageLog::default()));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        // Conta só vira membro depois que a janela apareceu (a pessoa resolveu).
        let shown_log = log.clone();
        let membership = move || {
            let shown = shown_log.lock().unwrap().shown;
            Box::pin(async move {
                Ok(if shown > 0 { GroupMembership::Member } else { GroupMembership::NotMember })
            }) as MembershipFuture
        };
        let result = browser_join_flow(membership, async { page(&log, true, false) }, FAST, &cancel, &tx).await;
        assert_eq!(result.unwrap().status, "joined");
        assert_eq!(log.lock().unwrap().shown, 1);
        assert!(rx.try_recv().is_ok());
        assert!(rx.try_recv().is_err(), "the batch is told once");
    }

    #[tokio::test]
    async fn nobody_finishing_in_the_window_marks_the_account_and_closes_it() {
        let log = Arc::new(Mutex::new(PageLog::default()));
        let (result, needs_you) = run(vec![GroupMembership::NotMember], page(&log, true, false), false).await;
        let row = result.unwrap();
        assert_eq!(row.status, "challenge");
        assert_eq!(row.challenge_type.as_deref(), Some("browser"));
        assert!(row.reason.is_some());
        assert!(log.lock().unwrap().closed);
        assert_eq!(needs_you, 1);
    }

    #[tokio::test]
    async fn without_the_join_button_the_window_shows_right_away() {
        let log = Arc::new(Mutex::new(PageLog::default()));
        let (result, needs_you) = run(
            vec![GroupMembership::NotMember, GroupMembership::Member],
            page(&log, false, false),
            false,
        )
        .await;
        assert_eq!(result.unwrap().status, "joined");
        assert_eq!(log.lock().unwrap().shown, 1);
        assert_eq!(needs_you, 1);
    }

    #[tokio::test]
    async fn closing_the_window_marks_the_account_as_not_a_member() {
        let log = Arc::new(Mutex::new(PageLog::default()));
        let (result, _) = run(vec![GroupMembership::NotMember], page(&log, true, true), false).await;
        assert_eq!(result.unwrap().status, "notMember");
    }

    #[tokio::test]
    async fn a_cancel_closes_the_window() {
        let log = Arc::new(Mutex::new(PageLog::default()));
        let (result, _) = run(vec![GroupMembership::NotMember], page(&log, true, false), true).await;
        assert_eq!(result.unwrap().status, "cancelled");
        assert!(log.lock().unwrap().closed);
    }

    #[tokio::test]
    async fn a_browser_that_does_not_open_is_an_error_for_the_caller() {
        let (result, _) = run(vec![GroupMembership::NotMember], Err("no chromium".into()), false).await;
        assert_eq!(result.unwrap_err(), "no chromium");
    }

    /// Dublê de `GroupJoiner` que avisa "a janela apareceu" e depois entra.
    struct AskingJoiner;

    impl GroupJoiner for AskingJoiner {
        fn join<'a>(
            &'a self,
            _user_id: i64,
            _group: &'a api::roblox::GroupSummary,
            _cancel: &'a std::sync::atomic::AtomicBool,
            needs_you: &'a tokio::sync::mpsc::UnboundedSender<()>,
        ) -> JoinFuture<'a> {
            Box::pin(async move {
                let _ = needs_you.send(());
                tokio::time::sleep(Duration::from_millis(20)).await;
                browser_row("joined", None)
            })
        }
    }

    #[tokio::test]
    async fn the_batch_shows_browser_while_the_window_waits_for_the_person() {
        let group = api::roblox::GroupSummary {
            id: 1,
            name: "G".into(),
            description: String::new(),
            member_count: 1,
            public_entry_allowed: true,
            has_verified_badge: false,
            is_locked: false,
        };
        let mut published: Vec<String> = Vec::new();
        let final_state = run_group_join(
            &[7],
            &group,
            JoinPacing { min: Duration::ZERO, max: Duration::ZERO },
            &std::sync::atomic::AtomicBool::new(false),
            &AskingJoiner,
            &mut || 0,
            &mut |s| published.push(s.accounts[0].status.clone()),
        )
        .await;
        assert_eq!(published, vec!["waiting", "joining", "browser", "joined", "joined"]);
        assert_eq!(final_state.accounts[0].status, "joined");
    }
}
