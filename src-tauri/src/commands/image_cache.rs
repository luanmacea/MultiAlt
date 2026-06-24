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

#[tauri::command]
async fn batched_get_avatar_headshots(
    image_cache: tauri::State<'_, ImageCache>,
    user_ids: Vec<i64>,
    size: String,
) -> Result<Vec<api::batch::CachedThumbnail>, String> {
    let requests: Vec<(i64, String, String, String)> = user_ids
        .iter()
        .map(|&id| {
            (
                id,
                "AvatarHeadShot".to_string(),
                size.clone(),
                "png".to_string(),
            )
        })
        .collect();

    let batch_results = image_cache.get_images_batch(requests).await;

    Ok(batch_results
        .into_iter()
        .map(|(target_id, url)| api::batch::CachedThumbnail {
            target_id,
            image_url: url,
            thumbnail_type: "AvatarHeadShot".to_string(),
        })
        .collect())
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
