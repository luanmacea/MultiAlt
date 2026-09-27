use crate::api::endpoints;
use crate::api::http_client;
use reqwest::header::COOKIE;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{oneshot, Mutex};

const BATCH_WINDOW_MS: u64 = 50;
const MAX_BATCH_SIZE: usize = 100;
#[allow(dead_code)]
const MAX_PLACE_BATCH_SIZE: usize = 50;

/// Teto de URLs de thumbnail guardadas. O app mostra avatar + ícone de jogo por
/// conta e por servidor listado; algumas centenas cobrem a tela inteira com
/// folga, o resto é histórico de navegação.
const MAX_CACHED_THUMBNAILS: usize = 2_000;

/// Validade de uma URL de thumbnail. O CDN do Roblox assina as URLs e elas
/// param de servir depois de algumas horas — guardá-las além disso só entrega
/// link quebrado para a UI.
const THUMBNAIL_CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);

/// Teto do mapa place -> universe.
const MAX_CACHED_PLACE_UNIVERSES: usize = 1_000;

/// `placeId -> universeId` não muda na prática; o TTL existe só para o mapa não
/// segurar para sempre um place aberto uma única vez.
const PLACE_UNIVERSE_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// `placeId -> nome do jogo`. Mesmo teto e mesma validade do mapa de universo,
/// porque vem da **mesma** requisição: o corpo de `multiget-place-details`
/// traz nome e `universeId` juntos, e o nome era jogado fora.
const MAX_CACHED_PLACE_NAMES: usize = 1_000;

/// Cache em memória com teto de tamanho e validade por entrada.
///
/// **Política (escolhida por ser previsível, não por taxa de acerto):**
/// - *TTL*: uma entrada vence `ttl` depois de ter sido escrita. Uma leitura
///   vencida é um miss e descarta a entrada na hora.
/// - *Teto duro*: passando de `capacity` entradas, sai a **mais antiga por
///   ordem de escrita** (FIFO). Deliberadamente não é LRU: ler não mexe em
///   nada, então a leitura continua O(1) e dá para prever o que será despejado
///   só olhando a ordem em que as coisas entraram.
///
/// Reescrever uma chave existente renova o carimbo de tempo, mas **não** muda o
/// lugar dela na fila de despejo.
struct BoundedCache<K, V> {
    capacity: usize,
    ttl: Duration,
    entries: HashMap<K, (V, Instant)>,
    insertion_order: VecDeque<K>,
}

impl<K, V> BoundedCache<K, V>
where
    K: Eq + std::hash::Hash + Clone,
    V: Clone,
{
    fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            // `0` desligaria o cache e faria cada escrita se auto-despejar;
            // trate como "guarde ao menos uma".
            capacity: capacity.max(1),
            ttl,
            entries: HashMap::new(),
            insertion_order: VecDeque::new(),
        }
    }

    fn get_at(&mut self, key: &K, now: Instant) -> Option<V> {
        match self.entries.get(key) {
            Some((value, stored_at)) if now.duration_since(*stored_at) < self.ttl => {
                Some(value.clone())
            }
            Some(_) => {
                self.entries.remove(key);
                None
            }
            None => None,
        }
    }

    fn get(&mut self, key: &K) -> Option<V> {
        self.get_at(key, Instant::now())
    }

    fn insert_at(&mut self, key: K, value: V, now: Instant) {
        if self.entries.insert(key.clone(), (value, now)).is_none() {
            self.insertion_order.push_back(key);
        }
        self.evict_overflow();
    }

    fn insert(&mut self, key: K, value: V) {
        self.insert_at(key, value, Instant::now());
    }

    fn evict_overflow(&mut self) {
        while self.entries.len() > self.capacity {
            match self.insertion_order.pop_front() {
                Some(oldest) => {
                    self.entries.remove(&oldest);
                }
                None => break,
            }
        }
        // Entradas descartadas por TTL deixam chaves órfãs na fila de ordem;
        // compacta quando ela passa do dobro do teto, para a fila não crescer
        // sozinha numa sessão longa.
        if self.insertion_order.len() > self.capacity.saturating_mul(2) {
            let entries = &self.entries;
            self.insertion_order.retain(|key| entries.contains_key(key));
        }
    }

    fn fresh_entries_at(&self, now: Instant) -> Vec<(K, V)> {
        self.entries
            .iter()
            .filter(|(_, (_, stored_at))| now.duration_since(*stored_at) < self.ttl)
            .map(|(key, (value, _))| (key.clone(), value.clone()))
            .collect()
    }

    #[allow(dead_code)]
    fn fresh_entries(&self) -> Vec<(K, V)> {
        self.fresh_entries_at(Instant::now())
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.insertion_order.clear();
    }

    fn len(&self) -> usize {
        self.entries.len()
    }
}

fn cookie_header(security_token: &str) -> String {
    format!(".ROBLOSECURITY={}", security_token)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedThumbnail {
    #[serde(rename = "targetId")]
    pub target_id: i64,
    #[serde(rename = "imageUrl")]
    pub image_url: Option<String>,
    #[serde(rename = "thumbnailType")]
    pub thumbnail_type: String,
}

struct PendingRequest {
    target_id: i64,
    thumbnail_type: String,
    size: String,
    format: String,
    sender: oneshot::Sender<Option<String>>,
}

#[allow(dead_code)]
struct PendingPlaceRequest {
    place_id: i64,
    sender: oneshot::Sender<Option<i64>>,
}

/// O que a tela precisa para dizer **que jogo** é um Place ID.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInfo {
    pub place_id: i64,
    pub universe_id: Option<i64>,
    pub name: Option<String>,
    pub icon_url: Option<String>,
}

pub struct ImageCache {
    thumbnail_queue: Arc<Mutex<Vec<PendingRequest>>>,
    #[allow(dead_code)]
    place_queue: Arc<Mutex<Vec<PendingPlaceRequest>>>,
    cache: Arc<Mutex<BoundedCache<String, String>>>,
    place_universe_cache: Arc<Mutex<BoundedCache<i64, i64>>>,
    /// Nome vazio = "perguntei e o Roblox não deu nome", para não repetir.
    place_name_cache: Arc<Mutex<BoundedCache<i64, String>>>,
    batch_active: Arc<Mutex<bool>>,
    #[allow(dead_code)]
    place_batch_active: Arc<Mutex<bool>>,
}

impl ImageCache {
    pub fn new() -> Self {
        Self {
            thumbnail_queue: Arc::new(Mutex::new(Vec::new())),
            place_queue: Arc::new(Mutex::new(Vec::new())),
            cache: Arc::new(Mutex::new(BoundedCache::new(
                MAX_CACHED_THUMBNAILS,
                THUMBNAIL_CACHE_TTL,
            ))),
            place_universe_cache: Arc::new(Mutex::new(BoundedCache::new(
                MAX_CACHED_PLACE_UNIVERSES,
                PLACE_UNIVERSE_CACHE_TTL,
            ))),
            place_name_cache: Arc::new(Mutex::new(BoundedCache::new(
                MAX_CACHED_PLACE_NAMES,
                PLACE_UNIVERSE_CACHE_TTL,
            ))),
            batch_active: Arc::new(Mutex::new(false)),
            place_batch_active: Arc::new(Mutex::new(false)),
        }
    }

    fn cache_key(target_id: i64, thumbnail_type: &str, size: &str) -> String {
        format!("{}:{}:{}", target_id, thumbnail_type, size)
    }

    pub async fn get_image(
        &self,
        target_id: i64,
        thumbnail_type: &str,
        size: &str,
        format: &str,
    ) -> Option<String> {
        let key = Self::cache_key(target_id, thumbnail_type, size);

        {
            let mut cache = self.cache.lock().await;
            if let Some(url) = cache.get(&key) {
                return Some(url);
            }
        }

        let (tx, rx) = oneshot::channel();

        {
            let mut queue = self.thumbnail_queue.lock().await;
            queue.push(PendingRequest {
                target_id,
                thumbnail_type: thumbnail_type.to_string(),
                size: size.to_string(),
                format: format.to_string(),
                sender: tx,
            });
        }

        self.ensure_batch_running().await;
        rx.await.ok().flatten()
    }

    pub async fn get_game_icon(
        &self,
        place_id: i64,
        security_token: Option<&str>,
    ) -> Option<String> {
        let key = Self::cache_key(place_id, "GameIcon", "512x512");

        {
            let mut cache = self.cache.lock().await;
            if let Some(url) = cache.get(&key) {
                return Some(url);
            }
        }

        let universe_id = {
            let mut pu_cache = self.place_universe_cache.lock().await;
            pu_cache.get(&place_id)
        };

        let universe_id = match universe_id {
            Some(id) => id,
            None => {
                let resolved = self
                    .resolve_place_to_universe(place_id, security_token)
                    .await;
                match resolved {
                    Some(id) => id,
                    None => {
                        if let Ok(url) = get_asset_image_fallback(place_id, security_token).await {
                            let mut cache = self.cache.lock().await;
                            cache.insert(key, url.clone());
                            return Some(url);
                        }
                        return None;
                    }
                }
            }
        };

        let url = self
            .get_image(universe_id, "GameIcon", "512x512", "png")
            .await;
        if let Some(ref u) = url {
            let mut cache = self.cache.lock().await;
            cache.insert(key, u.clone());
        }
        url
    }

    async fn resolve_place_to_universe(
        &self,
        place_id: i64,
        security_token: Option<&str>,
    ) -> Option<i64> {
        let client = http_client::client();
        let url = format!(
            "{}/v1/games/multiget-place-details?placeIds={}",
            endpoints::host("games"),
            place_id
        );

        let mut request = client.get(&url);
        if let Some(token) = security_token {
            request = request.header(COOKIE, cookie_header(token));
        }

        let response = request.send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }

        let body: serde_json::Value = response.json().await.ok()?;
        let entry = body.as_array()?.first()?;

        // O nome vem no MESMO corpo. Guardar aqui é o que evita uma segunda
        // requisição (`get_place_details`, sem cache) só para dizer que jogo é
        // aquele place. Nome ausente vira string vazia de propósito: é o
        // registro de "já perguntei", e sem ele a tela perguntaria de novo a
        // cada abertura.
        {
            let name = entry
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            let mut name_cache = self.place_name_cache.lock().await;
            name_cache.insert(place_id, name);
        }

        let universe_id = entry.get("universeId")?.as_i64()?;

        {
            let mut pu_cache = self.place_universe_cache.lock().await;
            pu_cache.insert(place_id, universe_id);
        }

        Some(universe_id)
    }

    /// Nome + ícone + universo de um place, numa chamada e com cache.
    ///
    /// Existe porque a tela quer as duas coisas ao mesmo tempo (“que jogo é
    /// este Place ID?”) e o caminho antigo eram dois comandos: um sem cache
    /// nenhum (`get_place_details`) e outro que descartava o nome que já tinha
    /// em mãos. Nunca falha: o que não deu para descobrir volta como `None`.
    pub async fn get_game_info(
        &self,
        place_id: i64,
        security_token: Option<&str>,
    ) -> GameInfo {
        let cached_name = {
            let mut name_cache = self.place_name_cache.lock().await;
            name_cache.get(&place_id)
        };

        // Sem nome guardado, uma resolução resolve os dois — e `get_game_icon`
        // logo abaixo reaproveita o universo que ela deixou em cache.
        if cached_name.is_none() {
            let _ = self.resolve_place_to_universe(place_id, security_token).await;
        }

        let name = match cached_name {
            Some(name) => Some(name),
            None => {
                let mut name_cache = self.place_name_cache.lock().await;
                name_cache.get(&place_id)
            }
        };

        let universe_id = {
            let mut pu_cache = self.place_universe_cache.lock().await;
            pu_cache.get(&place_id)
        };

        GameInfo {
            place_id,
            universe_id,
            name: name.filter(|n| !n.is_empty()),
            icon_url: self.get_game_icon(place_id, security_token).await,
        }
    }

    async fn ensure_batch_running(&self) {
        let mut active = self.batch_active.lock().await;
        if *active {
            return;
        }
        *active = true;

        let queue = self.thumbnail_queue.clone();
        let cache = self.cache.clone();
        let batch_active = self.batch_active.clone();

        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(BATCH_WINDOW_MS)).await;

            loop {
                let pending: Vec<PendingRequest> = {
                    let mut q = queue.lock().await;
                    q.drain(..).collect()
                };

                if pending.is_empty() {
                    break;
                }

                let mut requests_by_key: HashMap<String, Vec<PendingRequest>> = HashMap::new();
                for req in pending {
                    let key = format!("{}:{}:{}", req.target_id, req.thumbnail_type, req.size);
                    requests_by_key.entry(key).or_default().push(req);
                }

                let unique_requests: Vec<(i64, String, String, String)> = requests_by_key
                    .keys()
                    .map(|k| {
                        let first = &requests_by_key[k][0];
                        (
                            first.target_id,
                            first.thumbnail_type.clone(),
                            first.size.clone(),
                            first.format.clone(),
                        )
                    })
                    .collect();

                let client = http_client::client();

                for chunk in unique_requests.chunks(MAX_BATCH_SIZE) {
                    let batch_body: Vec<serde_json::Value> = chunk
                        .iter()
                        .map(|(target_id, thumbnail_type, size, format)| {
                            serde_json::json!({
                                "requestId": format!("{}:undefined:{}:{}:{}:regular",
                                    target_id, thumbnail_type, size, format),
                                "type": thumbnail_type,
                                "targetId": target_id,
                                "size": size,
                                "format": format
                            })
                        })
                        .collect();

                    let response = client
                        .post(format!("{}/v1/batch", endpoints::host("thumbnails")))
                        .json(&batch_body)
                        .send()
                        .await;

                    if let Ok(resp) = response {
                        if resp.status().is_success() {
                            if let Ok(body) = resp.json::<serde_json::Value>().await {
                                if let Some(data) = body["data"].as_array() {
                                    let mut c = cache.lock().await;
                                    for item in data {
                                        let target_id = item["targetId"].as_i64().unwrap_or(0);
                                        let image_url =
                                            item["imageUrl"].as_str().unwrap_or_default();
                                        let error_code = item["errorCode"].as_i64().unwrap_or(-1);

                                        if error_code == 0 && !image_url.is_empty() {
                                            let req_id =
                                                item["requestId"].as_str().unwrap_or_default();
                                            let parts: Vec<&str> = req_id.split(':').collect();
                                            if parts.len() >= 4 {
                                                let key = format!(
                                                    "{}:{}:{}",
                                                    target_id, parts[2], parts[3]
                                                );
                                                c.insert(key, image_url.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                {
                    let mut c = cache.lock().await;
                    for (key, senders) in requests_by_key {
                        let url = c.get(&key);
                        for req in senders {
                            let _ = req.sender.send(url.clone());
                        }
                    }
                }

                let q = queue.lock().await;
                if q.is_empty() {
                    break;
                }
                drop(q);
            }

            let mut active = batch_active.lock().await;
            *active = false;
        });
    }

    #[allow(dead_code)]
    pub async fn get_cached_thumbnails(&self) -> Vec<CachedThumbnail> {
        let cache = self.cache.lock().await;
        cache
            .fresh_entries()
            .into_iter()
            .map(|(key, url)| {
                let parts: Vec<&str> = key.splitn(3, ':').collect();
                CachedThumbnail {
                    target_id: parts.first().and_then(|s| s.parse().ok()).unwrap_or(0),
                    thumbnail_type: parts.get(1).unwrap_or(&"").to_string(),
                    image_url: Some(url),
                }
            })
            .collect()
    }

    pub async fn clear_cache(&self) {
        let mut cache = self.cache.lock().await;
        cache.clear();
        let mut pu_cache = self.place_universe_cache.lock().await;
        pu_cache.clear();
    }

    pub async fn get_cached_url(
        &self,
        target_id: i64,
        thumbnail_type: &str,
        size: &str,
    ) -> Option<String> {
        let key = Self::cache_key(target_id, thumbnail_type, size);
        let mut cache = self.cache.lock().await;
        cache.get(&key)
    }

    pub async fn get_images_batch(
        &self,
        requests: Vec<(i64, String, String, String)>,
    ) -> Vec<(i64, Option<String>)> {
        let mut receivers = Vec::new();

        {
            let mut queue = self.thumbnail_queue.lock().await;
            let mut cache = self.cache.lock().await;

            for (target_id, thumbnail_type, size, format) in &requests {
                let key = Self::cache_key(*target_id, thumbnail_type, size);
                if let Some(url) = cache.get(&key) {
                    receivers.push((*target_id, None, Some(url)));
                } else {
                    let (tx, rx) = oneshot::channel();
                    queue.push(PendingRequest {
                        target_id: *target_id,
                        thumbnail_type: thumbnail_type.clone(),
                        size: size.clone(),
                        format: format.clone(),
                        sender: tx,
                    });
                    receivers.push((*target_id, Some(rx), None));
                }
            }
        }

        self.ensure_batch_running().await;

        let mut results = Vec::new();
        for (target_id, rx, cached) in receivers {
            if let Some(url) = cached {
                results.push((target_id, Some(url)));
            } else if let Some(rx) = rx {
                let url = rx.await.ok().flatten();
                results.push((target_id, url));
            }
        }

        results
    }
}

async fn get_asset_image_fallback(
    asset_id: i64,
    security_token: Option<&str>,
) -> Result<String, String> {
    let client = http_client::client();

    let mut request = client.get(format!(
        "{}/v1/assets?assetIds={}&returnPolicy=PlaceHolder&size=150x150&format=Png&isCircular=false",
        endpoints::host("thumbnails"),
        asset_id
    ));

    if let Some(token) = security_token {
        request = request.header(COOKIE, cookie_header(token));
    }

    let response = request
        .send()
        .await
        .map_err(|e| http_client::describe_error(&e))?;

    if !response.status().is_success() {
        return Err("Asset thumbnail request failed".to_string());
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Parse failed: {}", e))?;

    body["data"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|item| item["imageUrl"].as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| "No image URL in response".to_string())
}

/// The thumbnail batcher: cache hits, the 50 ms coalescing window, chunking at
/// 100 requests, the place -> universe -> icon chain and its asset fallback.
///
/// `api/roblox/thumbnails.rs` posts to the same `/thumbnails/v1/batch` path on
/// the shared mock server, so every mock here matches on target ids that no
/// other test uses (75xxx-78xxx).
#[cfg(test)]
mod image_cache_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server};
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, Request, ResponseTemplate};

    /// Matches a batch body that mentions this target id.
    fn batch_for(target_id: i64) -> impl Fn(&Request) -> bool {
        let needle = format!("\"targetId\":{}", target_id);
        move |req: &Request| String::from_utf8_lossy(&req.body).contains(&needle)
    }

    /// The `data` entry the batcher needs in order to cache a URL: the request
    /// id it echoes back is what the cache key is rebuilt from.
    fn entry(target_id: i64, thumbnail_type: &str, size: &str, url: &str) -> serde_json::Value {
        serde_json::json!({
            "targetId": target_id,
            "requestId": format!("{}:undefined:{}:{}:png:regular", target_id, thumbnail_type, size),
            "errorCode": 0,
            "state": "Completed",
            "imageUrl": url
        })
    }

    async fn mount_batch(target_id: i64, body: serde_json::Value) {
        Mock::given(method("POST"))
            .and(path(mock_path("thumbnails", "/v1/batch")))
            .and(batch_for(target_id))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(mock_server().await)
            .await;
    }

    #[test]
    fn the_cache_key_is_target_type_and_size() {
        assert_eq!(
            ImageCache::cache_key(1, "Avatar", "150x150"),
            "1:Avatar:150x150"
        );
    }

    // ---- BoundedCache --------------------------------------------------------

    fn bounded(capacity: usize) -> BoundedCache<i64, String> {
        BoundedCache::new(capacity, std::time::Duration::from_secs(60))
    }

    #[test]
    fn a_bounded_cache_returns_what_was_stored() {
        let mut cache = bounded(4);
        cache.insert(1, "a".to_string());
        assert_eq!(cache.get(&1).as_deref(), Some("a"));
        assert_eq!(cache.get(&2), None);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn a_bounded_cache_evicts_the_oldest_entry_past_the_limit() {
        // Regressão: o mapa de URLs só crescia e `clear_cache` era a única
        // saída, então uma sessão longa com muitas contas/jogos segurava tudo.
        let mut cache = bounded(3);
        for id in 1..=3 {
            cache.insert(id, format!("url-{id}"));
        }
        assert_eq!(cache.len(), 3);

        cache.insert(4, "url-4".to_string());
        assert_eq!(cache.len(), 3, "o teto de tamanho tem que ser respeitado");
        assert_eq!(cache.get(&1), None, "a entrada mais antiga é a que sai");
        assert_eq!(cache.get(&2).as_deref(), Some("url-2"));
        assert_eq!(cache.get(&4).as_deref(), Some("url-4"));
    }

    #[test]
    fn a_bounded_cache_never_grows_past_the_limit_however_many_writes_arrive() {
        let mut cache = bounded(8);
        for id in 0..500 {
            cache.insert(id, format!("url-{id}"));
        }
        assert_eq!(cache.len(), 8);
        // As 8 últimas (492..=499) sobreviveram; as anteriores saíram.
        assert!(cache.get(&499).is_some());
        assert!(cache.get(&492).is_some());
        assert!(cache.get(&491).is_none());
        assert!(cache.get(&0).is_none());
    }

    #[test]
    fn a_bounded_cache_capacity_is_at_least_one() {
        let mut cache = BoundedCache::<i64, String>::new(0, std::time::Duration::from_secs(60));
        cache.insert(1, "a".to_string());
        assert_eq!(cache.len(), 1);
        cache.insert(2, "b".to_string());
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get(&2).as_deref(), Some("b"));
    }

    #[test]
    fn a_bounded_cache_entry_expires_after_its_ttl() {
        let ttl = std::time::Duration::from_secs(60);
        let mut cache = BoundedCache::<i64, String>::new(4, ttl);
        let t0 = std::time::Instant::now();
        cache.insert_at(1, "a".to_string(), t0);

        assert_eq!(cache.get_at(&1, t0 + ttl / 2).as_deref(), Some("a"));
        assert_eq!(cache.get_at(&1, t0 + ttl), None, "no TTL a entrada já venceu");
        // Uma leitura vencida descarta a entrada em vez de deixá-la ocupando
        // espaço até o despejo por tamanho.
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn a_bounded_cache_refreshes_the_timestamp_on_rewrite() {
        let ttl = std::time::Duration::from_secs(60);
        let mut cache = BoundedCache::<i64, String>::new(4, ttl);
        let t0 = std::time::Instant::now();
        cache.insert_at(1, "a".to_string(), t0);
        cache.insert_at(1, "b".to_string(), t0 + ttl / 2);
        assert_eq!(cache.len(), 1, "reescrever não duplica a entrada");
        assert_eq!(
            cache.get_at(&1, t0 + ttl - std::time::Duration::from_secs(1)).as_deref(),
            Some("b")
        );
    }

    #[test]
    fn a_bounded_cache_lists_only_fresh_entries() {
        let ttl = std::time::Duration::from_secs(60);
        let mut cache = BoundedCache::<i64, String>::new(4, ttl);
        let t0 = std::time::Instant::now();
        cache.insert_at(1, "velha".to_string(), t0);
        cache.insert_at(2, "nova".to_string(), t0 + ttl);

        let fresh = cache.fresh_entries_at(t0 + ttl);
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0], (2, "nova".to_string()));
    }

    #[test]
    fn a_bounded_cache_clears() {
        let mut cache = bounded(4);
        cache.insert(1, "a".to_string());
        cache.insert(2, "b".to_string());
        cache.clear();
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.get(&1), None);
    }

    /// O teto configurado tem que valer para os dois mapas do `ImageCache`.
    #[tokio::test]
    async fn the_image_cache_drops_old_thumbnails_instead_of_growing_forever() {
        let cache = ImageCache::new();
        {
            let mut urls = cache.cache.lock().await;
            for id in 0..(MAX_CACHED_THUMBNAILS + 50) {
                urls.insert(format!("{id}:Avatar:150x150"), format!("https://cdn/{id}.png"));
            }
            assert_eq!(urls.len(), MAX_CACHED_THUMBNAILS);
        }
        {
            let mut places = cache.place_universe_cache.lock().await;
            for id in 0..(MAX_CACHED_PLACE_UNIVERSES + 50) {
                places.insert(id as i64, id as i64);
            }
            assert_eq!(places.len(), MAX_CACHED_PLACE_UNIVERSES);
        }
        cache.clear_cache().await;
        assert_eq!(cache.cache.lock().await.len(), 0);
        assert_eq!(cache.place_universe_cache.lock().await.len(), 0);
    }

    /// The second read of the same thumbnail must not reach the network.
    #[tokio::test]
    async fn an_image_is_fetched_once_and_then_served_from_the_cache() {
        let server = mock_server().await;
        let calls = Mock::given(method("POST"))
            .and(path(mock_path("thumbnails", "/v1/batch")))
            .and(batch_for(75_001))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [entry(75_001, "Avatar", "150x150", "https://cdn/75001.png")]
            })))
            .expect(1)
            .named("one upstream call for two reads")
            .mount_as_scoped(server)
            .await;

        let cache = ImageCache::new();
        assert!(cache.get_cached_url(75_001, "Avatar", "150x150").await.is_none());

        let first = cache.get_image(75_001, "Avatar", "150x150", "png").await;
        assert_eq!(first.as_deref(), Some("https://cdn/75001.png"));

        let second = cache.get_image(75_001, "Avatar", "150x150", "png").await;
        assert_eq!(second, first);
        assert_eq!(
            cache.get_cached_url(75_001, "Avatar", "150x150").await,
            first
        );

        drop(calls);
    }

    /// Two readers asking for the same thumbnail inside the batch window share
    /// a single upstream request and both get the URL.
    #[tokio::test]
    async fn concurrent_requests_for_the_same_image_are_deduped() {
        let server = mock_server().await;
        let calls = Mock::given(method("POST"))
            .and(path(mock_path("thumbnails", "/v1/batch")))
            .and(batch_for(75_002))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [entry(75_002, "Avatar", "150x150", "https://cdn/75002.png")]
            })))
            .expect(1)
            .named("two concurrent readers, one upstream call")
            .mount_as_scoped(server)
            .await;

        let cache = ImageCache::new();
        let (a, b) = tokio::join!(
            cache.get_image(75_002, "Avatar", "150x150", "png"),
            cache.get_image(75_002, "Avatar", "150x150", "png")
        );
        assert_eq!(a.as_deref(), Some("https://cdn/75002.png"));
        assert_eq!(a, b);

        drop(calls);
    }

    /// A failed batch resolves every waiter with `None` instead of hanging.
    #[tokio::test]
    async fn a_failing_batch_call_yields_no_url() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("thumbnails", "/v1/batch")))
            .and(batch_for(75_003))
            .respond_with(ResponseTemplate::new(500))
            .mount(server)
            .await;

        let cache = ImageCache::new();
        assert!(cache
            .get_image(75_003, "Avatar", "150x150", "png")
            .await
            .is_none());
        // Nothing is cached, so a later call can still succeed.
        assert!(cache
            .get_cached_url(75_003, "Avatar", "150x150")
            .await
            .is_none());
    }

    /// Roblox reports per-item failures with an `errorCode`; those must not be
    /// cached as if they were URLs.
    #[tokio::test]
    async fn an_entry_with_an_error_code_is_not_cached() {
        mount_batch(
            75_004,
            serde_json::json!({
                "data": [{
                    "targetId": 75_004,
                    "requestId": "75004:undefined:Avatar:150x150:png:regular",
                    "errorCode": 4,
                    "state": "Blocked",
                    "imageUrl": ""
                }]
            }),
        )
        .await;

        let cache = ImageCache::new();
        assert!(cache
            .get_image(75_004, "Avatar", "150x150", "png")
            .await
            .is_none());
        assert!(cache
            .get_cached_url(75_004, "Avatar", "150x150")
            .await
            .is_none());
    }

    #[tokio::test]
    async fn a_batch_read_mixes_cached_and_fetched_entries() {
        let server = mock_server().await;
        let warm = Mock::given(method("POST"))
            .and(path(mock_path("thumbnails", "/v1/batch")))
            .and(batch_for(75_005))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [entry(75_005, "Avatar", "150x150", "https://cdn/75005.png")]
            })))
            .expect(1)
            .named("the warm entry is fetched exactly once")
            .mount_as_scoped(server)
            .await;

        Mock::given(method("POST"))
            .and(path(mock_path("thumbnails", "/v1/batch")))
            .and(batch_for(75_006))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [entry(75_006, "Avatar", "150x150", "https://cdn/75006.png")]
            })))
            .mount(server)
            .await;

        let cache = ImageCache::new();
        // Warm one of the two up first.
        cache.get_image(75_005, "Avatar", "150x150", "png").await;

        let results = cache
            .get_images_batch(vec![
                (
                    75_005,
                    "Avatar".to_string(),
                    "150x150".to_string(),
                    "png".to_string(),
                ),
                (
                    75_006,
                    "Avatar".to_string(),
                    "150x150".to_string(),
                    "png".to_string(),
                ),
            ])
            .await;

        assert_eq!(results.len(), 2);
        let cached = results.iter().find(|(id, _)| *id == 75_005).unwrap();
        let fetched = results.iter().find(|(id, _)| *id == 75_006).unwrap();
        assert_eq!(cached.1.as_deref(), Some("https://cdn/75005.png"));
        assert_eq!(fetched.1.as_deref(), Some("https://cdn/75006.png"));

        drop(warm);
    }

    /// More than `MAX_BATCH_SIZE` pending requests are split across calls.
    #[tokio::test]
    async fn a_large_batch_is_split_at_a_hundred_requests() {
        let server = mock_server().await;
        // Every target in this test starts with 76, and no other test uses
        // that prefix, so this matcher sees exactly this test's chunks.
        let chunks = Mock::given(method("POST"))
            .and(path(mock_path("thumbnails", "/v1/batch")))
            .and(|req: &Request| String::from_utf8_lossy(&req.body).contains("\"targetId\":76"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "data": [] })),
            )
            .expect(2)
            .named("120 requests are sent as two chunks")
            .mount_as_scoped(server)
            .await;

        let requests: Vec<(i64, String, String, String)> = (76_000..76_120)
            .map(|id| {
                (
                    id,
                    "Avatar".to_string(),
                    "150x150".to_string(),
                    "png".to_string(),
                )
            })
            .collect();

        let results = ImageCache::new().get_images_batch(requests).await;
        assert_eq!(results.len(), 120);
        assert!(results.iter().all(|(_, url)| url.is_none()));

        drop(chunks);
    }

    #[tokio::test]
    async fn clearing_the_cache_drops_the_stored_urls() {
        mount_batch(
            75_007,
            serde_json::json!({
                "data": [entry(75_007, "Avatar", "150x150", "https://cdn/75007.png")]
            }),
        )
        .await;

        let cache = ImageCache::new();
        assert!(cache
            .get_image(75_007, "Avatar", "150x150", "png")
            .await
            .is_some());
        assert!(!cache.get_cached_thumbnails().await.is_empty());

        cache.clear_cache().await;

        assert!(cache
            .get_cached_url(75_007, "Avatar", "150x150")
            .await
            .is_none());
        assert!(cache.get_cached_thumbnails().await.is_empty());
    }

    /// The cached-thumbnail snapshot rebuilds the target id and type from the
    /// cache key.
    #[tokio::test]
    async fn the_cached_snapshot_reports_target_and_type() {
        mount_batch(
            75_008,
            serde_json::json!({
                "data": [entry(75_008, "Avatar", "150x150", "https://cdn/75008.png")]
            }),
        )
        .await;

        let cache = ImageCache::new();
        cache.get_image(75_008, "Avatar", "150x150", "png").await;

        let snapshot = cache.get_cached_thumbnails().await;
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].target_id, 75_008);
        assert_eq!(snapshot[0].thumbnail_type, "Avatar");
        assert_eq!(
            snapshot[0].image_url.as_deref(),
            Some("https://cdn/75008.png")
        );
    }

    /// A game icon is a universe thumbnail, so the place id has to be resolved
    /// first — and that mapping is cached too.
    #[tokio::test]
    async fn a_game_icon_resolves_the_place_to_its_universe_once() {
        let server = mock_server().await;
        let place_lookup = Mock::given(method("GET"))
            .and(path(mock_path("games", "/v1/games/multiget-place-details")))
            .and(query_param("placeIds", "88001"))
            .and(header("cookie", cookie_of("icon-account")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "placeId": 88001, "universeId": 77001 }
            ])))
            .expect(1)
            .named("the place -> universe mapping is cached")
            .mount_as_scoped(server)
            .await;

        mount_batch(
            77_001,
            serde_json::json!({
                "data": [entry(77_001, "GameIcon", "512x512", "https://cdn/icon.png")]
            }),
        )
        .await;

        let cache = ImageCache::new();
        let icon = cache.get_game_icon(88_001, Some("icon-account")).await;
        assert_eq!(icon.as_deref(), Some("https://cdn/icon.png"));

        // The second read is served from the place-keyed cache entry.
        assert_eq!(
            cache.get_cached_url(88_001, "GameIcon", "512x512").await,
            icon
        );
        assert_eq!(cache.get_game_icon(88_001, Some("icon-account")).await, icon);

        drop(place_lookup);
    }

    /// O nome do jogo vem no **mesmo corpo** que da o universeId, e era jogado
    /// fora: quem quisesse o nome pagava outra requisicao (`get_place_details`,
    /// que nao tem cache nenhum). Agora a mesma chamada serve aos dois.
    #[tokio::test]
    async fn game_info_reuses_the_place_details_call_for_the_name() {
        let server = mock_server().await;
        let place_lookup = Mock::given(method("GET"))
            .and(path(mock_path("games", "/v1/games/multiget-place-details")))
            .and(query_param("placeIds", "88010"))
            .and(header("cookie", cookie_of("info-account")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "placeId": 88010, "universeId": 77010, "name": "Jailbreak" }
            ])))
            .expect(1)
            .named("nome e universo saem de uma requisicao so, e ficam em cache")
            .mount_as_scoped(server)
            .await;

        mount_batch(
            77_010,
            serde_json::json!({
                "data": [entry(77_010, "GameIcon", "512x512", "https://cdn/jb.png")]
            }),
        )
        .await;

        let cache = ImageCache::new();
        let info = cache.get_game_info(88_010, Some("info-account")).await;
        assert_eq!(info.name.as_deref(), Some("Jailbreak"));
        assert_eq!(info.icon_url.as_deref(), Some("https://cdn/jb.png"));
        assert_eq!(info.universe_id, Some(77_010));

        // Segunda tela pedindo o mesmo place nao volta a rede: o `expect(1)`
        // acima falha se voltar.
        let de_novo = cache.get_game_info(88_010, Some("info-account")).await;
        assert_eq!(de_novo.name.as_deref(), Some("Jailbreak"));
        assert_eq!(de_novo.icon_url.as_deref(), Some("https://cdn/jb.png"));

        drop(place_lookup);
    }

    /// Place que existe mas nao devolve nome nao pode virar pedido repetido a
    /// cada vez que a tela perguntar.
    #[tokio::test]
    async fn a_place_without_a_name_is_not_asked_again() {
        let server = mock_server().await;
        let place_lookup = Mock::given(method("GET"))
            .and(path(mock_path("games", "/v1/games/multiget-place-details")))
            .and(query_param("placeIds", "88011"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "placeId": 88011, "universeId": 77011 }
            ])))
            .expect(1)
            .named("a ausencia de nome tambem fica guardada")
            .mount_as_scoped(server)
            .await;

        mount_batch(
            77_011,
            serde_json::json!({
                "data": [entry(77_011, "GameIcon", "512x512", "https://cdn/sem-nome.png")]
            }),
        )
        .await;

        let cache = ImageCache::new();
        assert_eq!(cache.get_game_info(88_011, None).await.name, None);
        assert_eq!(cache.get_game_info(88_011, None).await.name, None);

        drop(place_lookup);
    }

    /// When the place cannot be resolved the asset thumbnail of the place id
    /// itself is used instead.
    #[tokio::test]
    async fn a_game_icon_falls_back_to_the_asset_thumbnail() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("games", "/v1/games/multiget-place-details")))
            .and(query_param("placeIds", "88002"))
            .respond_with(ResponseTemplate::new(404))
            .mount(server)
            .await;

        Mock::given(method("GET"))
            .and(path(mock_path("thumbnails", "/v1/assets")))
            .and(query_param("assetIds", "88002"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "targetId": 88002, "imageUrl": "https://cdn/asset.png" }]
            })))
            .mount(server)
            .await;

        let cache = ImageCache::new();
        let icon = cache.get_game_icon(88_002, None).await;
        assert_eq!(icon.as_deref(), Some("https://cdn/asset.png"));
        // The fallback URL is cached under the game-icon key.
        assert_eq!(
            cache.get_cached_url(88_002, "GameIcon", "512x512").await,
            icon
        );
    }

    #[tokio::test]
    async fn a_game_icon_without_any_source_is_none() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("games", "/v1/games/multiget-place-details")))
            .and(query_param("placeIds", "88003"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(server)
            .await;

        Mock::given(method("GET"))
            .and(path(mock_path("thumbnails", "/v1/assets")))
            .and(query_param("assetIds", "88003"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "data": [] })),
            )
            .mount(server)
            .await;

        assert!(ImageCache::new().get_game_icon(88_003, None).await.is_none());
    }

    #[tokio::test]
    async fn the_asset_fallback_reports_a_failed_request() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("thumbnails", "/v1/assets")))
            .and(query_param("assetIds", "88004"))
            .respond_with(ResponseTemplate::new(503))
            .mount(server)
            .await;

        assert_eq!(
            get_asset_image_fallback(88_004, None).await.unwrap_err(),
            "Asset thumbnail request failed"
        );
    }

    #[tokio::test]
    async fn the_asset_fallback_rejects_an_empty_image_url() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("thumbnails", "/v1/assets")))
            .and(query_param("assetIds", "88005"))
            .and(header("cookie", cookie_of("asset-fallback")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "targetId": 88005, "imageUrl": "" }]
            })))
            .mount(server)
            .await;

        assert_eq!(
            get_asset_image_fallback(88_005, Some("asset-fallback"))
                .await
                .unwrap_err(),
            "No image URL in response"
        );
    }

    #[tokio::test]
    async fn the_asset_fallback_reports_a_malformed_body() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("thumbnails", "/v1/assets")))
            .and(query_param("assetIds", "88006"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("nope", "text/plain"))
            .mount(server)
            .await;

        let err = get_asset_image_fallback(88_006, None).await.unwrap_err();
        assert!(err.starts_with("Parse failed: "), "unexpected: {}", err);
    }
}
