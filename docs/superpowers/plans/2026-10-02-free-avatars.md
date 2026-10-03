# Free Avatars Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the owner build avatars from free official Roblox items, save them, and distribute them randomly across selected accounts (claiming missing items for free, then wearing them).

**Architecture:** New Rust API file `api/roblox/avatar_catalog.rs` (catalog search, collectible details, ownership, free claim), a JSON store `data/avatars.rs` (`RAMAvatars.json`), commands `commands/avatars.rs` (catalog cache, CRUD, sequential batch with progress event `avatar-batch-state`), pure frontend logic `src/avatarBuilder.ts`, and a dialog `AvatarsDialog.tsx` opened from the toolbar.

**Tech Stack:** Rust (reqwest, serde, wiremock tests, tokio), Tauri 2 commands/events, React 19 + TS strict, vitest, react-i18next, UI harness (`bun run dev:ui`).

**Spec:** `docs/superpowers/specs/2026-10-02-free-avatars-design.md`

## Global Constraints

- Only items with `creatorTargetId == 1` (Roblox) and `price == 0`. Never send a purchase with `expectedPrice` ≠ 0.
- Every Roblox URL through `endpoints::host("<sub>")` (`catalog`, `apis`, `inventory`, `avatar`) — never a literal `https://*.roblox.com` in `api/`.
- Reads that use a cookie go through `read_without_refresh`; never `run_with_session_retry` / `refresh_account_session` for reads. Wearing (`set_avatar`) keeps its existing command path.
- Rust tests live in `#[cfg(test)] mod <unique>_tests` inside each file; every new test module/file must be added to the new suite `avatars` in `scripts/test-suites.ts` (audit fails otherwise).
- Code in English, comments/docs in Portuguese, commit messages in English, **no Co-Authored-By / AI footer**. Commit + push to `develop` after each task (never `main`, never force).
- 7 s between two claims on the same account (`CLAIM_PAUSE`), 1.5 s between catalog pages; both injectable so tests run with zero delay.
- Excluded types: layered clothing (asset types 64–72), emotes/animations (61, bundle type 2), anything not in the allow-lists below.
- Allowed asset types: 2 (T-Shirt), 8 (Hat), 11 (Shirt), 12 (Pants), 41 (Hair), 42 (Face acc.), 43 (Neck), 44 (Shoulder), 45 (Front), 46 (Back), 47 (Waist). Allowed bundle types: 1 (Body), 4 (Dynamic head).
- Skin palette (BrickColor ids → hex): 1030 #FFCC99, 125 #EAB892, 18 #CC8E69, 38 #A05F35, 217 #7C5C46, 192 #694028, 5 #D7C59A, 226 #FDEA8D.

---

### Task 1: Catalog/claim API (`avatar_catalog.rs`)

**Files:**
- Create: `src-tauri/src/api/roblox/avatar_catalog.rs`
- Modify: `src-tauri/src/api/roblox.rs` (add `include!("roblox/avatar_catalog.rs");` after `economy.rs`)
- Modify: `scripts/test-suites.ts` (new suite `avatars`, rust: `["avatar_catalog_tests"]`, front: `[]`)

**Interfaces — Produces:**
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CatalogItemKind { Asset, Bundle }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreeCatalogItem {
    pub id: i64,
    pub kind: CatalogItemKind,
    /// assetType para Asset, bundleType para Bundle.
    pub type_id: i64,
    pub name: String,
    pub collectible_item_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CollectibleDetails {
    pub collectible_item_id: String,
    pub collectible_product_id: String,
    pub price: i64,
    pub creator_id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
pub enum ClaimOutcome { Claimed, AlreadyOwned, NotFree, ChallengeRequired, Failed(String) }

pub const ALLOWED_ASSET_TYPES: &[i64] = &[2, 8, 11, 12, 41, 42, 43, 44, 45, 46, 47];
pub const ALLOWED_BUNDLE_TYPES: &[i64] = &[1, 4];

pub async fn search_free_official_items(page_pause: Duration) -> Result<Vec<FreeCatalogItem>, String>;
pub async fn collectible_details(ids: &[String]) -> Result<Vec<CollectibleDetails>, String>;
pub async fn bundle_asset_ids(bundle_id: i64) -> Result<Vec<i64>, String>;
pub async fn owns_item(cookie: &str, user_id: i64, kind: CatalogItemKind, id: i64) -> Result<bool, String>;
pub async fn claim_free_item(cookie: &str, user_id: i64, details: &CollectibleDetails) -> ClaimOutcome;
```

- [ ] **Step 1: Write failing wiremock tests** in `mod avatar_catalog_tests` (follow `economy_http_tests` for `mock_server`, `mock_path`, `cookie_of`):
  - `search_keeps_only_free_official_allowed_items`: page 1 (`GET /catalog/v2/search/items/details`, query contains `CreatorName=Roblox`, `MaxPrice=0`) returns 5 items — free Roblox hair (assetType 41), paid hat (price 5), free UGC hat (`creatorTargetId` 999), layered jacket (assetType 64), free body bundle (`itemType:"Bundle"`, `bundleType:1`) — and `nextPageCursor:"c2"`; page 2 (query `Cursor=c2`) returns a free Roblox shirt (11) and `nextPageCursor:null`. Expect exactly hair, bundle, shirt, in that order, with `kind`/`type_id`/`collectible_item_id` set. Call with `Duration::ZERO`.
  - `collectible_details_reads_product_id_price_and_creator`: `POST /apis/marketplace-items/v1/items/details` with body `{"itemIds":["abc"]}` → `[{"collectibleItemId":"abc","collectibleProductId":"p1","price":0,"creatorId":1}]`.
  - `bundle_asset_ids_skips_the_user_outfit`: `GET /catalog/v1/bundles/192/details` → `items:[{id:1,type:"Asset"},{id:2,type:"UserOutfit"},{id:3,type:"Asset"}]` → `[1,3]`.
  - `owns_item_uses_the_account_cookie`: `GET /inventory/v1/users/42/items/Asset/451221329/is-owned` with header cookie `cookie_of("own-acc")` → body `true`.
  - `claim_sends_price_zero_and_reports_claimed`: mount csrf like other tests do (see `mount_csrf` usage in `account_api.rs` tests or `economy_http_tests`); `POST /apis/marketplace-sales/v1/item/abc/purchase-item` with `body_partial_json` `{"expectedPrice":0,"expectedCurrency":1,"collectibleProductId":"p1","expectedPurchaserId":"42","expectedSellerId":1}` → `{"purchased":true}` → `ClaimOutcome::Claimed`.
  - `claim_maps_price_mismatch_to_not_free`: response `{"purchased":false,"errorMessage":"PriceMismatch"}` → `NotFree`.
  - `claim_maps_already_owned`: `{"purchased":false,"errorMessage":"AlreadyOwned"}` → `AlreadyOwned`.
  - `claim_detects_a_challenge`: 403 with header `rblx-challenge-id: x` and `rblx-challenge-type: captcha` (no `x-csrf-token`) → `ChallengeRequired`.
  - `claim_never_sends_when_details_are_not_free`: `details.price = 5` → returns `NotFree` and the mock (`.expect(0)`) gets no request.
- [ ] **Step 2: Run** `bun run t avatars` — expect compile failure (functions missing).
- [ ] **Step 3: Implement.** Key code:

```rust
const CATALOG_PAGE_LIMIT: u32 = 120;

fn catalog_item_from_json(v: &serde_json::Value) -> Option<FreeCatalogItem> {
    let price = v.get("price").and_then(|p| p.as_i64()).unwrap_or(-1);
    let creator = v.get("creatorTargetId").and_then(|c| c.as_i64()).unwrap_or(0);
    if price != 0 || creator != 1 {
        return None;
    }
    let (kind, type_id) = match v.get("itemType").and_then(|t| t.as_str())? {
        "Asset" => (CatalogItemKind::Asset, v.get("assetType")?.as_i64()?),
        "Bundle" => (CatalogItemKind::Bundle, v.get("bundleType")?.as_i64()?),
        _ => return None,
    };
    let allowed = match kind {
        CatalogItemKind::Asset => ALLOWED_ASSET_TYPES,
        CatalogItemKind::Bundle => ALLOWED_BUNDLE_TYPES,
    };
    if !allowed.contains(&type_id) {
        return None;
    }
    Some(FreeCatalogItem {
        id: v.get("id")?.as_i64()?,
        kind,
        type_id,
        name: v.get("name").and_then(|n| n.as_str()).unwrap_or_default().to_string(),
        collectible_item_id: v.get("collectibleItemId")?.as_str()?.to_string(),
    })
}
```
  Search URL: `{catalog}/v2/search/items/details?Category=1&CreatorName=Roblox&MaxPrice=0&MinPrice=0&SalesTypeFilter=1&Limit=120[&Cursor=..]`, loop until `nextPageCursor` is null/empty, cap at 20 pages, `sleep(page_pause)` between pages, use `send_with_retry`. Claim body adds `"expectedPurchaserType":"User"`, `"expectedSellerType":"User"`, `"idempotencyKey": random_uuid_v4()` — `uuid`/`rand` are **not** dependencies; only `getrandom = "0.4.3"` is. Write a small `fn random_uuid_v4() -> String` from 16 `getrandom` bytes (set version/variant bits, format 8-4-4-4-12). Do not add crates. `expectedPurchaserId` goes as a **string** (Roblox expects it so). Challenge detection: status 403 and header `rblx-challenge-id` present → `ChallengeRequired`, checked **before** treating 403 as csrf; use `get_csrf_token` + `send_with_csrf_retry` (the retry only fires when a fresh `x-csrf-token` comes back, so a challenge 403 passes through). Error message mapping: `PriceMismatch`→`NotFree`, `AlreadyOwned`→`AlreadyOwned`, other → `Failed(msg)`. Guard first line: `if details.price != 0 || details.creator_id != 1 { return ClaimOutcome::NotFree; }`.
- [ ] **Step 4: Run** `bun run t avatars` — all pass.
- [ ] **Step 5: Commit** `Add free official catalog search and free-item claim API` and push.

---

### Task 2: Saved avatars store (`data/avatars.rs`)

**Files:**
- Create: `src-tauri/src/data/avatars.rs`
- Modify: data module declaration (where `pub mod scripts;` lives) to add `pub mod avatars;`
- Modify: `src-tauri/src/data/settings/paths.rs` — add `"RAMAvatars.json"` to `DATA_FILES` after `"RAMScripts.json"`, and a `get_avatars_path()` next to `get_scripts_path()` (same pattern).
- Modify: `scripts/test-suites.ts` — add `"avatar_store_tests"` to suite `avatars`.

**Interfaces — Produces:**
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarItemRef {
    pub id: i64,
    pub kind: crate::api::roblox::CatalogItemKind,
    pub type_id: i64,
    pub name: String,
    pub collectible_item_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedAvatar {
    pub id: String,
    pub name: String,
    pub items: Vec<AvatarItemRef>,
    /// BrickColor id aplicado às seis partes do corpo.
    pub skin_color: Option<i64>,
}

pub struct AvatarStore { /* Mutex<Vec<SavedAvatar>>, path */ }
impl AvatarStore {
    pub fn new(path: PathBuf) -> Self;     // loads; missing/corrupt file => empty list (corrupt is renamed to .bak like scripts store does, if it does)
    pub fn list(&self) -> Vec<SavedAvatar>;
    pub fn upsert(&self, avatar: SavedAvatar) -> Result<SavedAvatar, String>; // validates, writes file
    pub fn delete(&self, id: &str) -> Result<bool, String>;
}
pub fn validate_avatar(a: &SavedAvatar) -> Result<(), String>;
```
  Validation: id 1–96 chars `[A-Za-z0-9_-]`; name trimmed 1–60 chars; 1–12 items; no duplicate item ids; max 100 avatars in the store.

- [ ] **Step 1: Failing tests** `mod avatar_store_tests` (temp dir under `std::env::temp_dir()` with unique name): round-trip upsert→new store reads it; upsert same id replaces; delete returns true/false; validation rejects empty name, 0 items, 13 items, duplicate item ids, bad id; missing file → empty list.
- [ ] **Step 2: Run** `bun run t avatars` — fails.
- [ ] **Step 3: Implement** following `data/scripts.rs` for file IO (write to temp + rename if scripts does so).
- [ ] **Step 4: Run** — passes.
- [ ] **Step 5: Commit** `Add saved avatars store` and push.

---

### Task 3: Avatar commands and batch (`commands/avatars.rs`)

**Files:**
- Create: `src-tauri/src/commands/avatars.rs` and `include!("commands/avatars.rs");` in `lib.rs` next to the others
- Modify: `src-tauri/src/lib.rs` — manage `AvatarStore::new(get_avatars_path())`, register commands
- Modify: `src-tauri/src/api/batch.rs` — add `pub async fn invalidate_targets(&self, thumbnail_type: &str, target_ids: &[i64])` that removes cache entries whose key is `"{id}:{type}:*"` (read the key format at the `target:type:size` builder first)
- Modify: `scripts/test-suites.ts` — add `"avatar_batch_tests"` (and the batch.rs test name if you add one there) to suite `avatars`

**Interfaces — Consumes:** Task 1 functions/types, Task 2 store. **Produces (Tauri commands, camelCase JSON):**
- `avatar_free_catalog() -> Vec<FreeCatalogItem>` (6 h in-memory cache, `static` `Mutex<Option<(Instant, Vec<_>)>>`)
- `avatar_list_saved() -> Vec<SavedAvatar>`, `avatar_save(avatar: SavedAvatar) -> SavedAvatar`, `avatar_delete(id: String) -> bool`
- `avatar_apply_batch(user_ids: Vec<i64>, avatar_ids: Vec<String>) -> AvatarBatchSnapshot` (returns final snapshot)
- `avatar_cancel_batch()`, `get_avatar_batch_state() -> AvatarBatchSnapshot`
- `invalidate_avatar_headshots(user_ids: Vec<i64>)` → `image_cache.invalidate_targets("AvatarHeadShot", &ids)` (use the type string the headshot requests actually use)
- Event `avatar-batch-state` with `AvatarBatchSnapshot`:
```rust
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarBatchSnapshot {
    pub running: bool,
    pub total: usize,
    pub done: usize,
    pub current_user_id: Option<i64>,
    pub accounts: Vec<AvatarAccountResult>,
}
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AvatarAccountResult {
    pub user_id: i64,
    pub avatar_id: String,
    /// "ok" | "skipped" | "failed"
    pub status: String,
    /// "challenge" | "cancelled" | free text
    pub reason: Option<String>,
    pub claimed: usize,
    pub missing: usize,
}
```

Pure helpers (all unit-tested):
```rust
/// Embaralha os avatares e reparte em rodízio: nenhum repete enquanto houver outro sem uso.
fn assign_avatars(user_ids: &[i64], avatar_ids: &[String], next_u64: &mut dyn FnMut() -> u64) -> Vec<(i64, String)>;
/// avatar_json para o `set_avatar` existente.
fn build_wear_json(asset_ids: &[i64], skin_color: Option<i64>) -> serde_json::Value;
```
  `build_wear_json` → `{"playerAvatarType":"R15","assets":[{"id":n},...],"bodyColors":{headColorId,torsoColorId,rightArmColorId,leftArmColorId,rightLegColorId,leftLegColorId}}` (bodyColors only when `Some`). There is no `rand` crate: production passes a closure backed by `getrandom` (fill 8 bytes → `u64::from_le_bytes`); tests pass a deterministic counter/xorshift closure. Shuffle = Fisher–Yates using `next_u64() % (i+1)`.

Per-account flow (testable function, no Tauri types):
```rust
struct ApplyPacing { claim_pause: Duration }
async fn apply_avatar_to_account(
    cookie: &str,
    user_id: i64,
    avatar: &SavedAvatar,
    pacing: &ApplyPacing,
    cancel: &std::sync::atomic::AtomicBool,
) -> AvatarAccountResult
```
  For each item: `owns_item` (error ⇒ treat as not owned) → if not owned: `collectible_details(&[id])`, then `claim_free_item`; `Claimed`/`AlreadyOwned` ⇒ wearable (count `claimed` only for `Claimed`), `NotFree`/`Failed` ⇒ `missing += 1` and not worn, `ChallengeRequired` ⇒ return `skipped` with reason `"challenge"` immediately (nothing worn). Sleep `claim_pause` between two actual claims. Check `cancel` before each item ⇒ `skipped`/`"cancelled"`. Then expand bundles via `bundle_asset_ids` (Assets keep their id), call `api::roblox::set_avatar(cookie, build_wear_json(..))`; `Ok` ⇒ `ok`, `Err(e)` ⇒ `failed` with `e`. Wearing with zero wearable items ⇒ `failed` "nothing to wear".

  The command: single-run guard (copy `try_begin_friend_link` pattern with its own `AtomicBool`), dedupe ids, `assign_avatars` with the `getrandom`-backed closure, loop accounts sequentially; cookie via `get_cookie` (no refresh — a stale cookie just fails that account); emit snapshot after each account and on start/end; `CLAIM_PAUSE = 7s`.

- [ ] **Step 1: Failing tests** `mod avatar_batch_tests`:
  - `assign_spreads_before_repeating`: 5 users, 2 avatars ⇒ counts are {3,2}; 2 users, 3 avatars ⇒ two distinct avatars; every user appears once; empty avatars ⇒ empty.
  - `wear_json_sets_r15_assets_and_skin`.
  - wiremock `apply_skips_items_already_owned` (is-owned true ⇒ no purchase mock hit, `.expect(0)`; set-wearing-assets hit once with the asset).
  - `apply_never_claims_a_paid_item` (details price 5 ⇒ purchase `.expect(0)`, `missing == 1`).
  - `apply_stops_the_account_on_a_challenge` (purchase 403 with `rblx-challenge-id` ⇒ status `skipped`, reason `challenge`, set-wearing-assets `.expect(0)`).
  - `apply_expands_bundles_into_their_assets`.
  - All with `claim_pause: Duration::ZERO`. Mock csrf the same way Task 1 did.
- [ ] **Step 2: Run** `bun run t avatars` — fails.
- [ ] **Step 3: Implement** + register commands in `lib.rs` `generate_handler!`.
- [ ] **Step 4: Run** `bun run t avatars` — passes; `cd src-tauri && cargo build` ok.
- [ ] **Step 5: Commit** `Add avatar commands and the sequential free-avatar batch` and push.

---

### Task 4: Frontend builder logic (`src/avatarBuilder.ts`)

**Files:**
- Create: `src/avatarBuilder.ts`, `src/avatarBuilder.test.ts`
- Modify: `scripts/test-suites.ts` — suite `avatars` front: add `"src/avatarBuilder.test.ts"`

**Interfaces — Produces:**
```ts
export type CatalogItemKind = "Asset" | "Bundle";
export interface FreeCatalogItem { id: number; kind: CatalogItemKind; typeId: number; name: string; collectibleItemId: string }
export interface SavedAvatar { id: string; name: string; items: FreeCatalogItem[]; skinColor: number | null }
export type AvatarCategory = "hair" | "hat" | "accessory" | "shirt" | "pants" | "tshirt" | "body" | "head";
export const AVATAR_CATEGORIES: readonly AvatarCategory[]; // order above
export const REQUIRED_CATEGORIES: readonly AvatarCategory[]; // ["shirt","pants","body"]
export const MAX_ACCESSORIES = 3;
export const SKIN_COLORS: readonly { id: number; hex: string }[]; // palette from Global Constraints
export function categoryOf(item: FreeCatalogItem): AvatarCategory | null;
// Asset 41 hair, 8 hat, 42-47 accessory, 11 shirt, 12 pants, 2 tshirt; Bundle 1 body, 4 head
export function groupByCategory(items: FreeCatalogItem[]): Record<AvatarCategory, FreeCatalogItem[]>;
export type Selection = Record<AvatarCategory, FreeCatalogItem[]>; // accessory up to 3, others 0..1
export function emptySelection(): Selection;
export function toggleItem(sel: Selection, item: FreeCatalogItem): Selection; // single-slot replace; accessory toggles up to MAX
export function randomizeSelection(catalog: Record<AvatarCategory, FreeCatalogItem[]>, rand: () => number, only?: AvatarCategory, base?: Selection): Selection;
// required categories always filled (when catalog has items); optional ones empty with ~40% chance; accessory 0..2 picks; `only` rerolls just that category keeping `base`
export function randomSkin(rand: () => number): number;
export function selectionItems(sel: Selection): FreeCatalogItem[];
export function selectionFromAvatar(avatar: SavedAvatar): Selection;
export function validateAvatarDraft(name: string, sel: Selection): string | null; // i18n key or null
export function newAvatarId(): string; // "av_" + base36 time + random
```
  Note the backend's `AvatarItemRef` serializes to the same shape as `FreeCatalogItem` (camelCase: id, kind, typeId, name, collectibleItemId) — keep them identical.

- [ ] **Step 1: Failing tests** for: categoryOf mapping incl. excluded types → null; toggleItem replace vs accessory cap; randomizeSelection with a seeded `rand` always fills shirt/pants/body, `only:"hair"` keeps the rest of `base`; validateAvatarDraft (empty name → `"avatars.errors.name"`, missing required → `"avatars.errors.required"`).
- [ ] **Step 2: Run** `bun run t avatars` — fails. **Step 3: Implement. Step 4: Run** — passes.
- [ ] **Step 5: Commit** `Add avatar builder logic` and push.

---

### Task 5: Avatars dialog, store wiring, toolbar, i18n, harness

**Files:**
- Create: `src/components/dialogs/AvatarsDialog.tsx`, `src/components/dialogs/AvatarsDialog.test.tsx`
- Modify: `src/store.tsx` (`avatarsDialogOpen` + `setAvatarsDialogOpen`, same pattern as `afkDialogOpen`; `refreshAvatarHeadshots(userIds)` that calls `invalidate_avatar_headshots`, deletes those ids from `avatarUrls` and re-runs `loadAvatars` for them)
- Modify: `src/App.tsx` (mount next to `AfkDialog`), `src/components/layout/Toolbar.tsx` (button next to AFK, lucide icon `Shirt`)
- Modify: `src/locales/en/common.json`, `src/locales/pt/common.json` (keys under `avatars.*`; run `bun scripts/i18n/extract-keys.ts` if the project requires it)
- Modify: `src/test-utils/renderWithStore.tsx` (new store fields in the mock)
- Modify: `src/dev/harness/scenarios.ts` — scenario `avatars`
- Modify: `scripts/test-suites.ts` — front: `"src/components/dialogs/AvatarsDialog.test.tsx"`

**Dialog layout** (reuse the existing dialog shell/components used by `AfkDialog` and `ui/` primitives; follow theme tokens, no hard-coded colors):
- Header "Avatars" + short subtitle "Free official Roblox items only — never spends Robux".
- Two tabs: **Build** and **Distribute**.
- **Build:** left column = category list with counts and the current pick thumbnail; center = grid of items of the active category (150x150 thumbnails via `batch_thumbnails` with `type: "Asset"` / `"BundleThumbnail"`, lazy per category, name on hover/title, selected item highlighted; accessory allows 3); right column = preview (grid of chosen thumbnails + skin swatch), skin palette row, buttons **🎲 Randomize all**, per-category reroll (dice on the category row), name input, **Save** (toast on success), and the saved list (click to load into the builder, trash icon to delete with confirm). Loading and error states for the catalog (retry button).
- **Distribute:** saved avatars as checkable cards (thumbnail mosaic of first 4 items + name), "Use all" toggle; selected accounts summary (from `store.selectedAccounts`, with the existing headshots; message when none selected); **Apply avatars** button disabled without accounts/avatars; while running: progress bar `done/total`, current account, Cancel; after: per-account list with status chip (ok / verification required — skipped / failed + reason / cancelled) and claimed/missing counts. On finish call `store.refreshAvatarHeadshots(userIds)`.
- Listen to `avatar-batch-state` while open; on mount call `get_avatar_batch_state` to resume a running batch.

**Harness scenario `avatars`:** mocks `avatar_free_catalog` (≈24 items across all categories with realistic names, delayed 400 ms), `avatar_list_saved`/`avatar_save`/`avatar_delete` (in-memory), `batch_thumbnails` (placeholder data-URI images or the harness's existing image stub), `avatar_apply_batch` (emits `avatar-batch-state` progressively: first account ok with 3 claimed, second `skipped` reason `challenge`, rest ok), `get_avatar_batch_state`, `invalidate_avatar_headshots`. The scenario must not implement builder logic — it only returns data.

- [ ] **Step 1: Failing component test** (`AvatarsDialog.test.tsx`, follow `AfkDialog` tests' `renderWithStore` + invoke mock): renders catalog categories; Randomize fills shirt/pants/body picks; Save calls `avatar_save` with the selection; Distribute disabled without selected accounts; a `skipped`/`challenge` result shows the "verification required" text.
- [ ] **Step 2: Run** `bun run t avatars` — fails. **Step 3: Implement** dialog, store, toolbar, i18n (en + pt), harness. **Step 4: Run** `bun run t avatars` and `bun run typecheck` (or the project's tsc script) — pass.
- [ ] **Step 5: Commit** `Add Avatars dialog to build and distribute free avatars` and push.

---

### Task 6: Docs, full check, browser verification, build and scan

**Files:**
- Create: `docs/features/avatars.md` (how it works, endpoints, safety rules: price 0 twice, challenge = skip, pacing, what's out of scope)
- Modify: `docs/features/accounts.md` (command table), `docs/README.md` (index), `CLAUDE.md` (map: `avatar_catalog.rs`, `data/avatars.rs`, `commands/avatars.rs`, `AvatarsDialog`)

- [ ] **Step 1:** Write docs. **Step 2:** `bun run check` — must pass. **Step 3:** `bun run dev:ui`, open `?scenario=avatars&accounts=6`, drive Build (randomize, reroll, skin, save, load, delete) and Distribute (select accounts, apply, see progress/challenge/summary) in light and dark, check console errors, fix design issues. **Step 4:** Commit `Document free avatars` and push. **Step 5:** `bun run tauri build --no-bundle` with signing env, then `bun run scan <exe>` (VirusTotal + Defender) and report path/size/time/verdict.
