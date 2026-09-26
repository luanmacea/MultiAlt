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

/// Nome + ícone + universo de um place, numa chamada só e com cache.
///
/// A tela que mostra "que jogo é este Place ID" precisava de dois comandos:
/// `get_place_details` (sem cache nenhum, bate na rede toda vez) e
/// `batched_get_game_icon` (que descartava o nome vindo no mesmo corpo).
#[tauri::command]
async fn batched_get_game_info(
    image_cache: tauri::State<'_, ImageCache>,
    account_store: tauri::State<'_, AccountStore>,
    place_id: i64,
    user_id: Option<i64>,
) -> Result<api::batch::GameInfo, String> {
    let cookie = user_id.and_then(|id| get_cookie(&account_store, id).ok());
    Ok(image_cache.get_game_info(place_id, cookie.as_deref()).await)
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

#[cfg(test)]
mod image_cache_command_tests {
    use super::*;

    #[test]
    fn avatar_headshot_requests_tags_every_id_with_type_size_and_png() {
        let requests = avatar_headshot_requests(&[1, 2, 3], "150x150");

        assert_eq!(requests.len(), 3);
        for (i, (id, kind, size, format)) in requests.iter().enumerate() {
            assert_eq!(*id, (i + 1) as i64);
            assert_eq!(kind, "AvatarHeadShot");
            assert_eq!(size, "150x150");
            assert_eq!(format, "png");
        }
    }

    #[test]
    fn avatar_headshot_requests_on_an_empty_list_is_empty() {
        assert!(avatar_headshot_requests(&[], "420x420").is_empty());
    }

    #[test]
    fn avatar_headshot_requests_keeps_duplicates_and_odd_ids_verbatim() {
        // The cache dedupes internally; this mapping must not silently drop
        // entries, otherwise the response would not line up with the request.
        let requests = avatar_headshot_requests(&[5, 5, 0, -1, i64::MAX], "");
        let ids: Vec<i64> = requests.iter().map(|r| r.0).collect();
        assert_eq!(ids, vec![5, 5, 0, -1, i64::MAX]);
        assert!(requests.iter().all(|r| r.2.is_empty()));
    }

    #[test]
    fn avatar_headshot_requests_passes_unicode_sizes_through_unchanged() {
        let requests = avatar_headshot_requests(&[1], "１５０ｘ１５０");
        assert_eq!(requests[0].2, "１５０ｘ１５０");
    }

    #[test]
    fn avatar_headshot_results_preserves_order_and_missing_urls() {
        let results = avatar_headshot_results(vec![
            (1, Some("https://img/1.png".to_string())),
            (2, None),
        ]);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].target_id, 1);
        assert_eq!(results[0].image_url.as_deref(), Some("https://img/1.png"));
        assert_eq!(results[0].thumbnail_type, "AvatarHeadShot");
        assert_eq!(results[1].target_id, 2);
        assert_eq!(results[1].image_url, None);
        assert_eq!(results[1].thumbnail_type, "AvatarHeadShot");
    }

    #[test]
    fn avatar_headshot_results_on_an_empty_batch_is_empty() {
        assert!(avatar_headshot_results(Vec::new()).is_empty());
    }

    #[test]
    fn avatar_headshot_results_serializes_with_the_camel_case_keys_the_ui_reads() {
        let results = avatar_headshot_results(vec![(9, Some("u".to_string()))]);
        let json = serde_json::to_value(&results[0]).unwrap();
        assert_eq!(json["targetId"], 9);
        assert_eq!(json["imageUrl"], "u");
        assert_eq!(json["thumbnailType"], "AvatarHeadShot");
    }

    #[tokio::test]
    async fn a_fresh_image_cache_has_no_cached_url_and_clears_cleanly() {
        let cache = ImageCache::new();
        assert_eq!(cache.get_cached_url(1, "AvatarHeadShot", "150x150").await, None);
        cache.clear_cache().await;
        assert!(cache.get_cached_thumbnails().await.is_empty());
    }
}
