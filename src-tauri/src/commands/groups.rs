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
    /// Motivo da falha, como o Roblox escreveu.
    pub reason: Option<String>,
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
        GroupJoinOutcome::ChallengeRequired => ("challenge", None),
        GroupJoinOutcome::Failed(message) => ("failed", Some(message)),
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
/// Um captcha marca a conta como `challenge` e o lote segue para a próxima —
/// nunca tenta resolver.
#[allow(clippy::too_many_arguments)]
async fn run_group_join(
    user_ids: &[i64],
    group: &api::roblox::GroupSummary,
    pacing: JoinPacing,
    cancel: &std::sync::atomic::AtomicBool,
    cookie_of: &(dyn Fn(i64) -> Result<String, String> + Sync),
    next_u64: &mut (dyn FnMut() -> u64 + Send),
    publish: &mut (dyn FnMut(&GroupJoinSnapshot) + Send),
) -> GroupJoinSnapshot {
    let requires_approval = !group.public_entry_allowed;
    let mut snapshot = GroupJoinSnapshot {
        running: true,
        group_id: Some(group.id),
        group_name: group.name.clone(),
        total: user_ids.len(),
        done: 0,
        current_user_id: None,
        accounts: user_ids
            .iter()
            .map(|&user_id| GroupJoinAccountResult {
                user_id,
                status: "waiting".to_string(),
                reason: None,
            })
            .collect(),
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

        let (status, reason) = match cookie_of(user_id) {
            Ok(cookie) if !cookie.trim().is_empty() => {
                outcome_status(api::roblox::join_group_outcome(&cookie, group.id, requires_approval).await)
            }
            Ok(_) => ("failed", Some("Account has no cookie".to_string())),
            Err(e) => ("failed", Some(e)),
        };
        snapshot.accounts[index].status = status.to_string();
        snapshot.accounts[index].reason = reason;
        snapshot.done = index + 1;
        snapshot.current_user_id = None;
        publish(&snapshot);
    }

    snapshot.running = false;
    snapshot.current_user_id = None;
    publish(&snapshot);
    snapshot
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

    let store = account_store.inner();
    let cookie_of = |user_id: i64| get_cookie(store, user_id);
    let mut publish = |snapshot: &GroupJoinSnapshot| publish_group_join(&app, snapshot);
    Ok(run_group_join(
        &user_ids,
        &group,
        JOIN_PACING,
        &GROUP_JOIN_CANCEL,
        &cookie_of,
        &mut os_random_u64,
        &mut publish,
    )
    .await)
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
            &cookies,
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
            &cookies,
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
            &cookies,
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
            &cookies,
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
            &cookies,
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
            &cookie_of,
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
            }],
            ..GroupJoinSnapshot::default()
        };
        assert!(!apply_membership_check(&mut snapshot, 1, 6, "joined"));
        assert_eq!(snapshot.accounts[0].status, "challenge");
        assert!(!apply_membership_check(&mut snapshot, 2, 5, "joined"));
        assert!(apply_membership_check(&mut snapshot, 1, 5, "joined"));
        assert_eq!(snapshot.accounts[0].status, "joined");
        assert_eq!(snapshot.accounts[0].reason, None);
    }

    #[test]
    fn statuses_use_the_names_the_screen_reads() {
        use api::roblox::{GroupJoinOutcome, GroupMembership};
        assert_eq!(outcome_status(GroupJoinOutcome::Joined).0, "joined");
        assert_eq!(outcome_status(GroupJoinOutcome::Pending).0, "pending");
        assert_eq!(outcome_status(GroupJoinOutcome::AlreadyMember).0, "alreadyMember");
        assert_eq!(outcome_status(GroupJoinOutcome::ChallengeRequired).0, "challenge");
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
    fn only_one_batch_runs_at_a_time() {
        let first = try_begin_group_join().expect("first");
        assert_eq!(try_begin_group_join().err().as_deref(), Some("A group join is already running"));
        drop(first);
        assert!(try_begin_group_join().is_ok());
    }
}
