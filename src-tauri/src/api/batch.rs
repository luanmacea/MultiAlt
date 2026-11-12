use crate::api::cookie_rotation::SendNoting;
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

        let response = request.send_noting().await.ok()?;
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
                        .send_noting()
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
        .send_noting()
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
