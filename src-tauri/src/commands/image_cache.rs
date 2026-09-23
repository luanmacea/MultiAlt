#[tauri::command]
async fn batched_get_image(
    image_cache: tauri::State<'_, ImageCache>,
    target_id: i64,
    thumbnail_type: String,
    size: String,
    format: String,
) -> Result<Option<String>, String> {
    Ok(image_cache
        .get_image(target_id, &thumbnail_type, &size, &format)
        .await)
}

/// Thumbnail type every headshot request/response in this file is tagged with.
const AVATAR_HEADSHOT_TYPE: &str = "AvatarHeadShot";

/// Builds the `(target_id, type, size, format)` tuples the image cache batches.
fn avatar_headshot_requests(user_ids: &[i64], size: &str) -> Vec<(i64, String, String, String)> {
    user_ids
        .iter()
        .map(|&id| {
            (
                id,
                AVATAR_HEADSHOT_TYPE.to_string(),
                size.to_string(),
                "png".to_string(),
            )
        })
        .collect()
}

/// Re-tags the cache's `(target_id, url)` pairs as headshot thumbnails.
fn avatar_headshot_results(
    batch_results: Vec<(i64, Option<String>)>,
) -> Vec<api::batch::CachedThumbnail> {
    batch_results
        .into_iter()
        .map(|(target_id, url)| api::batch::CachedThumbnail {
            target_id,
            image_url: url,
            thumbnail_type: AVATAR_HEADSHOT_TYPE.to_string(),
        })
        .collect()
}

#[tauri::command]
async fn batched_get_avatar_headshots(
    image_cache: tauri::State<'_, ImageCache>,
    user_ids: Vec<i64>,
    size: String,
) -> Result<Vec<api::batch::CachedThumbnail>, String> {
    let requests = avatar_headshot_requests(&user_ids, &size);
    let batch_results = image_cache.get_images_batch(requests).await;
    Ok(avatar_headshot_results(batch_results))
}

#[tauri::command]
async fn batched_get_game_icon(
    image_cache: tauri::State<'_, ImageCache>,
    account_store: tauri::State<'_, AccountStore>,
    place_id: i64,
    user_id: Option<i64>,
) -> Result<Option<String>, String> {
    let cookie = user_id.and_then(|id| get_cookie(&account_store, id).ok());
    Ok(image_cache.get_game_icon(place_id, cookie.as_deref()).await)
}

#[tauri::command]
async fn get_cached_thumbnail(
    image_cache: tauri::State<'_, ImageCache>,
    target_id: i64,
    thumbnail_type: String,
    size: String,
) -> Result<Option<String>, String> {
    Ok(image_cache
        .get_cached_url(target_id, &thumbnail_type, &size)
        .await)
}

#[tauri::command]
async fn clear_image_cache(image_cache: tauri::State<'_, ImageCache>) -> Result<(), String> {
    image_cache.clear_cache().await;
    Ok(())
}
