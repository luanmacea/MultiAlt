// Avatares gratuitos: catálogo, avatares salvos e o lote que distribui os
// avatares entre contas (resgata o que falta de graça e depois veste).

use std::time::Duration;

/// Pausa entre dois resgates na mesma conta (limite de taxa do Roblox).
const CLAIM_PAUSE: Duration = Duration::from_secs(7);

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarBatchSnapshot {
    pub running: bool,
    pub total: usize,
    pub done: usize,
    pub current_user_id: Option<i64>,
    pub accounts: Vec<AvatarAccountResult>,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AvatarAccountResult {
    pub user_id: i64,
    pub avatar_id: String,
    /// "ok" | "skipped" | "failed"
    pub status: String,
    /// "challenge" | "cancelled" | texto livre
    pub reason: Option<String>,
    pub claimed: usize,
    pub missing: usize,
}

/// Embaralha (Fisher-Yates) e devolve uma cópia da lista.
fn shuffled(avatar_ids: &[String], next_u64: &mut dyn FnMut() -> u64) -> Vec<String> {
    let mut deck = avatar_ids.to_vec();
    for i in (1..deck.len()).rev() {
        let j = (next_u64() % (i as u64 + 1)) as usize;
        deck.swap(i, j);
    }
    deck
}

/// Embaralha os avatares e reparte em rodízio: nenhum repete enquanto houver outro sem uso.
fn assign_avatars(
    user_ids: &[i64],
    avatar_ids: &[String],
    next_u64: &mut dyn FnMut() -> u64,
) -> Vec<(i64, String)> {
    if avatar_ids.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(user_ids.len());
    let mut deck: Vec<String> = Vec::new();
    for &user_id in user_ids {
        // Baralho vazio: embaralha de novo, então cada volta usa todos os avatares.
        if deck.is_empty() {
            deck = shuffled(avatar_ids, next_u64);
        }
        if let Some(avatar_id) = deck.pop() {
            out.push((user_id, avatar_id));
        }
    }
    out
}

/// avatar_json para o `set_avatar` existente.
fn build_wear_json(asset_ids: &[i64], skin_color: Option<i64>) -> serde_json::Value {
    let mut json = serde_json::json!({
        "playerAvatarType": "R15",
        "assets": asset_ids
            .iter()
            .map(|id| serde_json::json!({ "id": id }))
            .collect::<Vec<_>>(),
    });
    if let Some(color) = skin_color {
        json["bodyColors"] = serde_json::json!({
            "headColorId": color,
            "torsoColorId": color,
            "rightArmColorId": color,
            "leftArmColorId": color,
            "rightLegColorId": color,
            "leftLegColorId": color,
        });
    }
    json
}

struct ApplyPacing {
    claim_pause: Duration,
}

fn account_result(
    user_id: i64,
    avatar: &data::avatars::SavedAvatar,
    status: &str,
    reason: Option<String>,
    claimed: usize,
    missing: usize,
) -> AvatarAccountResult {
    AvatarAccountResult {
        user_id,
        avatar_id: avatar.id.clone(),
        status: status.to_string(),
        reason,
        claimed,
        missing,
    }
}

/// Resgata de graça o que a conta ainda não tem e veste o avatar. Só leitura +
/// resgate de itens gratuitos oficiais + `set_avatar`: nada de refresh de sessão.
/// Um desafio (captcha) interrompe a conta sem vestir nada.
async fn apply_avatar_to_account(
    cookie: &str,
    user_id: i64,
    avatar: &data::avatars::SavedAvatar,
    pacing: &ApplyPacing,
    cancel: &std::sync::atomic::AtomicBool,
) -> AvatarAccountResult {
    use api::roblox::{CatalogItemKind, ClaimOutcome};
    use std::sync::atomic::Ordering;

    let mut claimed = 0usize;
    let mut missing = 0usize;
    let mut wearable: Vec<&data::avatars::AvatarItemRef> = Vec::new();
    let mut attempted_claim = false;

    for item in &avatar.items {
        if cancel.load(Ordering::SeqCst) {
            return account_result(user_id, avatar, "skipped", Some("cancelled".into()), claimed, missing);
        }

        // Falha ao consultar o inventário = trata como "não tem" e tenta resgatar.
        let owned = api::roblox::owns_item(cookie, user_id, item.kind, item.id)
            .await
            .unwrap_or(false);
        if owned {
            wearable.push(item);
            continue;
        }

        if item.collectible_item_id.is_empty() {
            missing += 1;
            continue;
        }
        let details =
            match api::roblox::collectible_details(std::slice::from_ref(&item.collectible_item_id)).await {
                Ok(list) => list
                    .into_iter()
                    .find(|d| d.collectible_item_id == item.collectible_item_id),
                Err(_) => None,
            };
        let Some(details) = details else {
            missing += 1;
            continue;
        };

        if attempted_claim && !pacing.claim_pause.is_zero() {
            tokio::time::sleep(pacing.claim_pause).await;
        }
        let outcome = api::roblox::claim_free_item(cookie, user_id, &details).await;
        attempted_claim = true;
        match outcome {
            ClaimOutcome::Claimed => {
                claimed += 1;
                wearable.push(item);
            }
            ClaimOutcome::AlreadyOwned => wearable.push(item),
            ClaimOutcome::NotFree | ClaimOutcome::Failed(_) => missing += 1,
            ClaimOutcome::ChallengeRequired => {
                return account_result(user_id, avatar, "skipped", Some("challenge".into()), claimed, missing);
            }
        }
    }

    let mut asset_ids: Vec<i64> = Vec::new();
    for item in wearable {
        match item.kind {
            CatalogItemKind::Asset => asset_ids.push(item.id),
            CatalogItemKind::Bundle => match api::roblox::bundle_asset_ids(item.id).await {
                Ok(ids) if !ids.is_empty() => asset_ids.extend(ids),
                _ => missing += 1,
            },
        }
    }
    let mut seen = std::collections::HashSet::new();
    asset_ids.retain(|id| seen.insert(*id));

    if asset_ids.is_empty() {
        return account_result(user_id, avatar, "failed", Some("nothing to wear".into()), claimed, missing);
    }

    match api::roblox::set_avatar(cookie, build_wear_json(&asset_ids, avatar.skin_color)).await {
        Ok(_) => account_result(user_id, avatar, "ok", None, claimed, missing),
        Err(e) => account_result(user_id, avatar, "failed", Some(e), claimed, missing),
    }
}

// ---- Catálogo gratuito (cache de 6 h em memória) -------------------------

const FREE_CATALOG_TTL: Duration = Duration::from_secs(6 * 60 * 60);
/// Pausa entre as páginas da busca no catálogo.
const CATALOG_PAGE_PAUSE: Duration = Duration::from_millis(1500);

static FREE_CATALOG: std::sync::Mutex<Option<(std::time::Instant, Vec<api::roblox::FreeCatalogItem>)>> =
    std::sync::Mutex::new(None);

#[tauri::command]
async fn avatar_free_catalog() -> Result<Vec<api::roblox::FreeCatalogItem>, String> {
    if let Ok(cache) = FREE_CATALOG.lock() {
        if let Some((at, items)) = cache.as_ref() {
            if at.elapsed() < FREE_CATALOG_TTL {
                return Ok(items.clone());
            }
        }
    }
    let items = api::roblox::search_free_official_items(CATALOG_PAGE_PAUSE).await?;
    if let Ok(mut cache) = FREE_CATALOG.lock() {
        *cache = Some((std::time::Instant::now(), items.clone()));
    }
    Ok(items)
}

// ---- Avatares salvos -----------------------------------------------------

#[tauri::command]
fn avatar_list_saved(store: tauri::State<'_, data::avatars::AvatarStore>) -> Vec<data::avatars::SavedAvatar> {
    store.list()
}

#[tauri::command]
fn avatar_save(
    store: tauri::State<'_, data::avatars::AvatarStore>,
    avatar: data::avatars::SavedAvatar,
) -> Result<data::avatars::SavedAvatar, String> {
    store.upsert(avatar)
}

#[tauri::command]
fn avatar_delete(store: tauri::State<'_, data::avatars::AvatarStore>, id: String) -> Result<bool, String> {
    store.delete(&id)
}

// ---- Lote ----------------------------------------------------------------

/// Um lote por vez: a tela pode remontar e disparar outro, dobrando as chamadas.
static AVATAR_BATCH_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static AVATAR_BATCH_CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static AVATAR_BATCH: std::sync::LazyLock<std::sync::Mutex<AvatarBatchSnapshot>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(AvatarBatchSnapshot::default()));

/// Solta a trava do lote, seja qual for o fim.
struct AvatarBatchRunGuard;

impl Drop for AvatarBatchRunGuard {
    fn drop(&mut self) {
        AVATAR_BATCH_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

fn try_begin_avatar_batch() -> Result<AvatarBatchRunGuard, String> {
    if AVATAR_BATCH_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("Já existe um lote de avatares em andamento.".to_string());
    }
    Ok(AvatarBatchRunGuard)
}

/// Muda o estado e publica o retrato inteiro. Mutex envenenado não derruba o lote.
fn update_avatar_batch(
    app: &tauri::AppHandle,
    f: impl FnOnce(&mut AvatarBatchSnapshot),
) -> AvatarBatchSnapshot {
    let snapshot = match AVATAR_BATCH.lock() {
        Ok(mut state) => {
            f(&mut state);
            state.clone()
        }
        Err(_) => AvatarBatchSnapshot::default(),
    };
    let _ = app.emit("avatar-batch-state", snapshot.clone());
    snapshot
}

#[tauri::command]
fn get_avatar_batch_state() -> AvatarBatchSnapshot {
    AVATAR_BATCH.lock().map(|s| s.clone()).unwrap_or_default()
}

#[tauri::command]
fn avatar_cancel_batch() {
    AVATAR_BATCH_CANCEL.store(true, std::sync::atomic::Ordering::SeqCst);
}

fn os_random_u64() -> u64 {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).expect("OS RNG unavailable");
    u64::from_le_bytes(bytes)
}

/// Mantém a ordem e descarta repetidos.
fn dedupe_keep_order<T: Eq + std::hash::Hash + Clone>(items: Vec<T>) -> Vec<T> {
    let mut seen = std::collections::HashSet::new();
    items.into_iter().filter(|i| seen.insert(i.clone())).collect()
}

#[tauri::command]
async fn avatar_apply_batch(
    app: tauri::AppHandle,
    account_store: tauri::State<'_, AccountStore>,
    avatar_store: tauri::State<'_, data::avatars::AvatarStore>,
    user_ids: Vec<i64>,
    avatar_ids: Vec<String>,
) -> Result<AvatarBatchSnapshot, String> {
    use std::sync::atomic::Ordering;

    let _running = try_begin_avatar_batch()?;
    AVATAR_BATCH_CANCEL.store(false, Ordering::SeqCst);

    let user_ids = dedupe_keep_order(user_ids.into_iter().filter(|id| *id > 0).collect());
    let saved = avatar_store.list();
    let avatars: Vec<data::avatars::SavedAvatar> = dedupe_keep_order(avatar_ids)
        .iter()
        .filter_map(|id| saved.iter().find(|a| &a.id == id).cloned())
        .collect();
    if user_ids.is_empty() {
        return Err("Nenhuma conta selecionada.".to_string());
    }
    if avatars.is_empty() {
        return Err("Nenhum avatar salvo selecionado.".to_string());
    }

    let known_ids: Vec<String> = avatars.iter().map(|a| a.id.clone()).collect();
    let plan = assign_avatars(&user_ids, &known_ids, &mut os_random_u64);
    let pacing = ApplyPacing { claim_pause: CLAIM_PAUSE };

    update_avatar_batch(&app, |s| {
        *s = AvatarBatchSnapshot {
            running: true,
            total: plan.len(),
            ..AvatarBatchSnapshot::default()
        };
    });

    for (user_id, avatar_id) in plan {
        let Some(avatar) = avatars.iter().find(|a| a.id == avatar_id) else { continue };

        let result = if AVATAR_BATCH_CANCEL.load(Ordering::SeqCst) {
            account_result(user_id, avatar, "skipped", Some("cancelled".into()), 0, 0)
        } else {
            update_avatar_batch(&app, |s| s.current_user_id = Some(user_id));
            // Cookie direto, sem refresh: um cookie vencido só falha esta conta.
            match get_cookie(&account_store, user_id) {
                Ok(cookie) => {
                    apply_avatar_to_account(&cookie, user_id, avatar, &pacing, &AVATAR_BATCH_CANCEL).await
                }
                Err(e) => account_result(user_id, avatar, "failed", Some(e), 0, 0),
            }
        };

        update_avatar_batch(&app, |s| {
            s.accounts.push(result);
            s.done = s.accounts.len();
            s.current_user_id = None;
        });
    }

    Ok(update_avatar_batch(&app, |s| {
        s.running = false;
        s.current_user_id = None;
    }))
}

/// O avatar mudou: o headshot em cache dessas contas não vale mais.
#[tauri::command]
async fn invalidate_avatar_headshots(
    image_cache: tauri::State<'_, ImageCache>,
    user_ids: Vec<i64>,
) -> Result<(), String> {
    image_cache.invalidate_targets(AVATAR_HEADSHOT_TYPE, &user_ids).await;
    Ok(())
}

#[cfg(test)]
mod avatar_batch_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server, mount_csrf};
    use crate::api::roblox::CatalogItemKind;
    use data::avatars::{AvatarItemRef, SavedAvatar};
    use std::collections::HashMap;
    use std::sync::atomic::AtomicBool;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockGuard, Request, ResponseTemplate};

    fn xorshift(seed: u64) -> impl FnMut() -> u64 {
        let mut x = seed;
        move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        }
    }

    fn ids(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn assign_spreads_before_repeating() {
        let mut rng = xorshift(0x9E37_79B9_7F4A_7C15);

        // 5 contas, 2 avatares: 3 + 2, e toda conta aparece uma vez.
        let users = [10, 20, 30, 40, 50];
        let picked = assign_avatars(&users, &ids(&["a", "b"]), &mut rng);
        assert_eq!(picked.len(), 5);
        let got_users: Vec<i64> = picked.iter().map(|(u, _)| *u).collect();
        assert_eq!(got_users, users.to_vec());
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for (_, a) in &picked {
            *counts.entry(a.as_str()).or_default() += 1;
        }
        let mut sorted: Vec<usize> = counts.values().copied().collect();
        sorted.sort();
        assert_eq!(sorted, vec![2, 3]);

        // 2 contas, 3 avatares: dois avatares diferentes.
        for seed in 1..40u64 {
            let mut rng = xorshift(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) | 1);
            let picked = assign_avatars(&[1, 2], &ids(&["a", "b", "c"]), &mut rng);
            assert_eq!(picked.len(), 2);
            assert_ne!(picked[0].1, picked[1].1, "seed {seed}");
        }

        // Sem avatares, nada a distribuir.
        assert!(assign_avatars(&[1, 2], &[], &mut rng).is_empty());
    }

    #[test]
    fn wear_json_sets_r15_assets_and_skin() {
        let json = build_wear_json(&[11, 22], Some(1030));
        assert_eq!(json["playerAvatarType"], "R15");
        assert_eq!(json["assets"], serde_json::json!([{ "id": 11 }, { "id": 22 }]));
        for part in [
            "headColorId",
            "torsoColorId",
            "rightArmColorId",
            "leftArmColorId",
            "rightLegColorId",
            "leftLegColorId",
        ] {
            assert_eq!(json["bodyColors"][part], 1030, "{part}");
        }

        let plain = build_wear_json(&[11], None);
        assert!(plain.get("bodyColors").is_none());
    }

    fn item(id: i64, kind: CatalogItemKind) -> AvatarItemRef {
        AvatarItemRef {
            id,
            kind,
            type_id: 8,
            name: format!("item {id}"),
            collectible_item_id: format!("col-{id}"),
        }
    }

    fn avatar(items: Vec<AvatarItemRef>, skin: Option<i64>) -> SavedAvatar {
        SavedAvatar { id: "av".into(), name: "Av".into(), items, skin_color: skin }
    }

    fn no_pause() -> ApplyPacing {
        ApplyPacing { claim_pause: Duration::ZERO }
    }

    async fn mount_owned(token: &str, user: i64, kind: &str, id: i64, owned: bool) {
        Mock::given(method("GET"))
            .and(path(mock_path("inventory", &format!("/v1/users/{user}/items/{kind}/{id}/is-owned"))))
            .and(header("cookie", cookie_of(token)))
            .respond_with(ResponseTemplate::new(200).set_body_string(owned.to_string()))
            .mount(mock_server().await)
            .await;
    }

    async fn mount_details(collectible: &str, price: i64) {
        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/marketplace-items/v1/items/details")))
            .and(body_partial_json(serde_json::json!({ "itemIds": [collectible] })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "collectibleItemId": collectible,
                "collectibleProductId": format!("prod-{collectible}"),
                "price": price,
                "creatorId": 1
            }])))
            .mount(mock_server().await)
            .await;
    }

    async fn mount_purchase(token: &str, collectible: &str, response: ResponseTemplate, times: u64) -> MockGuard {
        Mock::given(method("POST"))
            .and(path(mock_path("apis", &format!("/marketplace-sales/v1/item/{collectible}/purchase-item"))))
            .and(header("cookie", cookie_of(token)))
            .respond_with(response)
            .expect(times)
            .mount_as_scoped(mock_server().await)
            .await
    }

    /// O que `set_avatar` chama além do wearing: tipo de avatar e cores.
    async fn mount_wear_side_endpoints(token: &str) {
        for endpoint in ["/v1/avatar/set-player-avatar-type", "/v1/avatar/set-body-colors"] {
            Mock::given(method("POST"))
                .and(path(mock_path("avatar", endpoint)))
                .and(header("cookie", cookie_of(token)))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
                .mount(mock_server().await)
                .await;
        }
    }

    /// `expected = None` => o wearing não pode ser chamado.
    async fn mount_wearing(token: &str, expected: Option<Vec<i64>>) -> MockGuard {
        let wanted = expected.clone();
        Mock::given(method("POST"))
            .and(path(mock_path("avatar", "/v2/avatar/set-wearing-assets")))
            .and(header("cookie", cookie_of(token)))
            .and(move |req: &Request| match &wanted {
                None => true,
                Some(ids) => {
                    let body: serde_json::Value =
                        serde_json::from_slice(&req.body).unwrap_or_default();
                    let want: Vec<serde_json::Value> =
                        ids.iter().map(|id| serde_json::json!({ "id": id })).collect();
                    body["assets"] == serde_json::Value::Array(want)
                }
            })
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .expect(if expected.is_some() { 1 } else { 0 })
            .mount_as_scoped(mock_server().await)
            .await
    }

    #[tokio::test]
    async fn apply_skips_items_already_owned() {
        let token = "avb-owned";
        mock_server().await;
        mount_csrf(token, "csrf-avb-owned").await;
        mount_wear_side_endpoints(token).await;
        mount_owned(token, 601, "Asset", 910_001, true).await;
        let purchase = mount_purchase(token, "col-910001", ResponseTemplate::new(200), 0).await;
        let wearing = mount_wearing(token, Some(vec![910_001])).await;

        let result = apply_avatar_to_account(
            token,
            601,
            &avatar(vec![item(910_001, CatalogItemKind::Asset)], Some(1030)),
            &no_pause(),
            &AtomicBool::new(false),
        )
        .await;

        assert_eq!(result.status, "ok", "{result:?}");
        assert_eq!((result.claimed, result.missing), (0, 0));
        drop((purchase, wearing));
    }

    #[tokio::test]
    async fn apply_claims_a_free_item_then_wears_it() {
        let token = "avb-claim";
        mock_server().await;
        mount_csrf(token, "csrf-avb-claim").await;
        mount_wear_side_endpoints(token).await;
        mount_owned(token, 602, "Asset", 910_011, false).await;
        mount_details("col-910011", 0).await;
        let purchase = mount_purchase(
            token,
            "col-910011",
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "purchased": true })),
            1,
        )
        .await;
        let wearing = mount_wearing(token, Some(vec![910_011])).await;

        let result = apply_avatar_to_account(
            token,
            602,
            &avatar(vec![item(910_011, CatalogItemKind::Asset)], None),
            &no_pause(),
            &AtomicBool::new(false),
        )
        .await;

        assert_eq!(result.status, "ok", "{result:?}");
        assert_eq!((result.claimed, result.missing), (1, 0));
        drop((purchase, wearing));
    }

    #[tokio::test]
    async fn apply_never_claims_a_paid_item() {
        let token = "avb-paid";
        mock_server().await;
        mount_csrf(token, "csrf-avb-paid").await;
        mount_wear_side_endpoints(token).await;
        mount_owned(token, 603, "Asset", 910_021, false).await;
        mount_owned(token, 603, "Asset", 910_022, true).await;
        mount_details("col-910021", 5).await;
        let purchase = mount_purchase(token, "col-910021", ResponseTemplate::new(200), 0).await;
        let wearing = mount_wearing(token, Some(vec![910_022])).await;

        let result = apply_avatar_to_account(
            token,
            603,
            &avatar(
                vec![item(910_021, CatalogItemKind::Asset), item(910_022, CatalogItemKind::Asset)],
                None,
            ),
            &no_pause(),
            &AtomicBool::new(false),
        )
        .await;

        assert_eq!(result.status, "ok", "{result:?}");
        assert_eq!((result.claimed, result.missing), (0, 1));
        drop((purchase, wearing));
    }

    #[tokio::test]
    async fn apply_stops_the_account_on_a_challenge() {
        let token = "avb-challenge";
        mock_server().await;
        mount_csrf(token, "csrf-avb-challenge").await;
        mount_wear_side_endpoints(token).await;
        mount_owned(token, 604, "Asset", 910_031, false).await;
        mount_owned(token, 604, "Asset", 910_032, true).await;
        mount_details("col-910031", 0).await;
        let purchase = mount_purchase(
            token,
            "col-910031",
            ResponseTemplate::new(403)
                .insert_header("rblx-challenge-id", "x")
                .insert_header("rblx-challenge-type", "captcha"),
            1,
        )
        .await;
        let wearing = mount_wearing(token, None).await;

        let result = apply_avatar_to_account(
            token,
            604,
            &avatar(
                vec![item(910_031, CatalogItemKind::Asset), item(910_032, CatalogItemKind::Asset)],
                None,
            ),
            &no_pause(),
            &AtomicBool::new(false),
        )
        .await;

        assert_eq!(result.status, "skipped", "{result:?}");
        assert_eq!(result.reason.as_deref(), Some("challenge"));
        drop((purchase, wearing));
    }

    #[tokio::test]
    async fn apply_expands_bundles_into_their_assets() {
        let token = "avb-bundle";
        mock_server().await;
        mount_csrf(token, "csrf-avb-bundle").await;
        mount_wear_side_endpoints(token).await;
        mount_owned(token, 605, "Bundle", 920_001, true).await;
        mount_owned(token, 605, "Asset", 910_041, true).await;
        Mock::given(method("GET"))
            .and(path(mock_path("catalog", "/v1/bundles/920001/details")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    { "id": 920_101, "type": "Asset" },
                    { "id": 920_102, "type": "UserOutfit" },
                    { "id": 920_103, "type": "Asset" }
                ]
            })))
            .mount(mock_server().await)
            .await;
        let wearing = mount_wearing(token, Some(vec![920_101, 920_103, 910_041])).await;

        let result = apply_avatar_to_account(
            token,
            605,
            &avatar(
                vec![item(920_001, CatalogItemKind::Bundle), item(910_041, CatalogItemKind::Asset)],
                None,
            ),
            &no_pause(),
            &AtomicBool::new(false),
        )
        .await;

        assert_eq!(result.status, "ok", "{result:?}");
        drop(wearing);
    }

    #[tokio::test]
    async fn apply_with_nothing_wearable_fails_without_wearing() {
        let token = "avb-nothing";
        mock_server().await;
        mount_csrf(token, "csrf-avb-nothing").await;
        mount_wear_side_endpoints(token).await;
        mount_owned(token, 606, "Asset", 910_051, false).await;
        mount_details("col-910051", 5).await;
        let wearing = mount_wearing(token, None).await;

        let result = apply_avatar_to_account(
            token,
            606,
            &avatar(vec![item(910_051, CatalogItemKind::Asset)], None),
            &no_pause(),
            &AtomicBool::new(false),
        )
        .await;

        assert_eq!(result.status, "failed", "{result:?}");
        assert_eq!(result.reason.as_deref(), Some("nothing to wear"));
        assert_eq!(result.missing, 1);
        drop(wearing);
    }

    #[tokio::test]
    async fn apply_honours_a_cancel_before_touching_anything() {
        let token = "avb-cancel";
        mock_server().await;
        let wearing = mount_wearing(token, None).await;

        let result = apply_avatar_to_account(
            token,
            607,
            &avatar(vec![item(910_061, CatalogItemKind::Asset)], None),
            &no_pause(),
            &AtomicBool::new(true),
        )
        .await;

        assert_eq!(result.status, "skipped", "{result:?}");
        assert_eq!(result.reason.as_deref(), Some("cancelled"));
        drop(wearing);
    }
}
