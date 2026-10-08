// Avatares gratuitos: catálogo, avatares salvos e o lote que distribui os
// avatares entre contas (resgata o que falta de graça e depois veste).
//
// O lote mora em `avatar_batch.rs` e só entra na edição completa (feature
// `avatar-batch`). Na padrão, os três comandos dele existem com o mesmo nome e
// respondem `AVATAR_BATCH_DISABLED_ERR` (o estado volta sempre parado) — mesmo
// padrão do Nexus e do Web Server em `services.rs`.

use std::time::Duration;

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

#[cfg(feature = "avatar-batch")]
include!("avatar_batch.rs");

/// O que os comandos do lote respondem na edição padrão.
#[cfg(not(feature = "avatar-batch"))]
const AVATAR_BATCH_DISABLED_ERR: &str = "Avatar distribution is not in this edition";

#[cfg(not(feature = "avatar-batch"))]
#[tauri::command]
fn get_avatar_batch_state() -> AvatarBatchSnapshot {
    AvatarBatchSnapshot::default()
}

#[cfg(not(feature = "avatar-batch"))]
#[tauri::command]
fn avatar_cancel_batch() -> Result<(), String> {
    Err(AVATAR_BATCH_DISABLED_ERR.into())
}

#[cfg(not(feature = "avatar-batch"))]
#[tauri::command]
async fn avatar_apply_batch(
    // Mesmos nomes do comando de verdade: argumento com outro nome faria o
    // `invoke` falhar na leitura dos parâmetros, antes desta mensagem.
    user_ids: Vec<i64>,
    avatar_ids: Vec<String>,
) -> Result<AvatarBatchSnapshot, String> {
    let _ = (user_ids, avatar_ids);
    Err(AVATAR_BATCH_DISABLED_ERR.into())
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

#[cfg(all(test, not(feature = "avatar-batch")))]
mod avatar_batch_disabled_tests {
    use super::*;

    #[test]
    fn the_standard_edition_answers_the_batch_commands_with_the_edition_message() {
        assert_eq!(AVATAR_BATCH_DISABLED_ERR, "Avatar distribution is not in this edition");
        assert_eq!(avatar_cancel_batch(), Err(AVATAR_BATCH_DISABLED_ERR.to_string()));
        let started = tauri::async_runtime::block_on(avatar_apply_batch(vec![1], vec!["av".into()]));
        assert_eq!(started.unwrap_err(), AVATAR_BATCH_DISABLED_ERR);
    }

    #[test]
    fn the_standard_edition_reports_an_idle_batch() {
        let state = get_avatar_batch_state();
        assert!(!state.running);
        assert_eq!((state.total, state.done), (0, 0));
        assert!(state.current_user_id.is_none());
        assert!(state.accounts.is_empty());
    }
}
