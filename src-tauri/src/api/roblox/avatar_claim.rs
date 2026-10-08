// Resgate dos itens gratuitos do catálogo para uma conta — o que a
// distribuição de avatares em lote usa. Só existe na edição completa
// (feature `avatar-batch`); a padrão não compila nada daqui.
//
// O resgate nunca manda `expectedPrice` diferente de 0: se o preço mudou, o
// Roblox recusa (`PriceMismatch`) e nada é cobrado.

#[derive(Debug, Clone, PartialEq)]
pub struct CollectibleDetails {
    pub collectible_item_id: String,
    pub collectible_product_id: String,
    pub price: i64,
    pub creator_id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
pub enum ClaimOutcome {
    Claimed,
    AlreadyOwned,
    NotFree,
    ChallengeRequired,
    Failed(String),
}

/// Quantos itens vão por chamada em `items/details`.
const COLLECTIBLE_DETAILS_CHUNK: usize = 50;

/// Lê `collectibleProductId`, preço e criador dos itens colecionáveis — o que
/// o resgate precisa para montar o corpo do purchase.
pub async fn collectible_details(ids: &[String]) -> Result<Vec<CollectibleDetails>, String> {
    let client = http_client::client();
    let url = format!("{}/marketplace-items/v1/items/details", endpoints::host("apis"));
    let mut out = Vec::new();

    for chunk in ids.chunks(COLLECTIBLE_DETAILS_CHUNK) {
        let body = serde_json::json!({ "itemIds": chunk });
        let response = send_with_retry(|| client.post(&url).json(&body)).await?;
        if !response.status().is_success() {
            return Err(format!(
                "Failed to get collectible details (status {})",
                response.status().as_u16()
            ));
        }
        let parsed: Vec<serde_json::Value> = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse collectible details: {}", e))?;

        out.extend(parsed.iter().filter_map(|v| {
            Some(CollectibleDetails {
                collectible_item_id: v.get("collectibleItemId")?.as_str()?.to_string(),
                collectible_product_id: v.get("collectibleProductId")?.as_str()?.to_string(),
                price: v.get("price").and_then(|p| p.as_i64()).unwrap_or(-1),
                creator_id: v.get("creatorId").and_then(|c| c.as_i64()).unwrap_or(0),
            })
        }));
    }

    Ok(out)
}

/// Uma peça de um bundle, com o tipo de asset dela (17 cabeça, 27 tronco,
/// 28/29 braços, 30/31 pernas...). Sem `assetType` na resposta, o tipo é 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundleAsset {
    pub id: i64,
    pub asset_type: i64,
}

/// Os assets de um bundle, com o tipo de cada um. O `UserOutfit` (a roupa
/// montada) não é um asset e fica de fora.
pub async fn bundle_asset_ids(bundle_id: i64) -> Result<Vec<BundleAsset>, String> {
    let client = http_client::client();
    let url = format!("{}/v1/bundles/{}/details", endpoints::host("catalog"), bundle_id);

    let response = send_with_retry(|| client.get(&url).header("Accept", "application/json")).await?;
    if !response.status().is_success() {
        return Err(format!("Failed to get bundle details (status {})", response.status().as_u16()));
    }
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse bundle details: {}", e))?;

    Ok(body
        .get("items")
        .and_then(|i| i.as_array())
        .map(|items| {
            items
                .iter()
                .filter(|i| i.get("type").and_then(|t| t.as_str()) == Some("Asset"))
                .filter_map(|i| {
                    Some(BundleAsset {
                        id: i.get("id").and_then(|id| id.as_i64())?,
                        asset_type: i.get("assetType").and_then(|t| t.as_i64()).unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

/// A conta já tem o item? Leitura simples: não faz refresh de sessão.
pub async fn owns_item(cookie: &str, user_id: i64, kind: CatalogItemKind, id: i64) -> Result<bool, String> {
    let client = http_client::client();
    let kind_path = match kind {
        CatalogItemKind::Asset => "Asset",
        CatalogItemKind::Bundle => "Bundle",
    };
    let url = format!(
        "{}/v1/users/{}/items/{}/{}/is-owned",
        endpoints::host("inventory"),
        user_id,
        kind_path,
        id
    );

    let response = send_with_retry(|| client.get(&url).header(COOKIE, cookie_header(cookie))).await?;
    if !response.status().is_success() {
        return Err(format!("Failed to check ownership (status {})", response.status().as_u16()));
    }
    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read ownership: {}", e))?;
    match text.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("Unexpected ownership response: {}", other)),
    }
}

/// UUID v4 para o `idempotencyKey` do resgate (sem depender de `uuid`/`rand`).
fn random_uuid_v4() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("OS RNG unavailable");
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{:02x}", x)).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// Resgata um item gratuito oficial para a conta. Nunca manda preço diferente
/// de zero; um desafio (captcha etc.) volta como `ChallengeRequired`, sem
/// tentar resolver.
pub async fn claim_free_item(cookie: &str, user_id: i64, details: &CollectibleDetails) -> ClaimOutcome {
    if details.price != 0 || details.creator_id != ROBLOX_CREATOR_ID {
        return ClaimOutcome::NotFree;
    }

    let csrf = match crate::api::auth::get_csrf_token(cookie).await {
        Ok(token) => token,
        Err(e) => return ClaimOutcome::Failed(e),
    };
    let client = http_client::client();
    let request = client
        .post(format!(
            "{}/marketplace-sales/v1/item/{}/purchase-item",
            endpoints::host("apis"),
            details.collectible_item_id
        ))
        .header(COOKIE, cookie_header(cookie))
        .json(&serde_json::json!({
            "collectibleItemId": details.collectible_item_id,
            "collectibleProductId": details.collectible_product_id,
            "expectedCurrency": 1,
            "expectedPrice": 0,
            "expectedPurchaserId": user_id.to_string(),
            "expectedPurchaserType": "User",
            "expectedSellerId": ROBLOX_CREATOR_ID,
            "expectedSellerType": "User",
            "idempotencyKey": random_uuid_v4(),
        }));
    let response = match crate::api::auth::send_with_csrf_retry(request, &csrf).await {
        Ok(response) => response,
        Err(e) => return ClaimOutcome::Failed(e),
    };

    let status = response.status();
    // Desafio antes de qualquer outra leitura do 403: não é falha de csrf.
    if status.as_u16() == 403 && response.headers().contains_key("rblx-challenge-id") {
        return ClaimOutcome::ChallengeRequired;
    }

    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return ClaimOutcome::Failed(format!("Claim failed (status {}): {}", status.as_u16(), body));
    }
    let parsed: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => return ClaimOutcome::Failed(format!("Failed to parse claim result: {}", e)),
    };

    if parsed.get("purchased").and_then(|p| p.as_bool()) == Some(true) {
        return ClaimOutcome::Claimed;
    }
    match parsed.get("errorMessage").and_then(|m| m.as_str()).unwrap_or_default() {
        "PriceMismatch" => ClaimOutcome::NotFree,
        "AlreadyOwned" => ClaimOutcome::AlreadyOwned,
        "" => ClaimOutcome::Failed("Claim was not completed".to_string()),
        other => ClaimOutcome::Failed(other.to_string()),
    }
}

#[cfg(test)]
mod avatar_claim_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server, mount_csrf};
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    #[tokio::test]
    async fn collectible_details_reads_product_id_price_and_creator() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/marketplace-items/v1/items/details")))
            .and(body_partial_json(serde_json::json!({ "itemIds": ["abc"] })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "collectibleItemId": "abc", "collectibleProductId": "p1", "price": 0, "creatorId": 1 }
            ])))
            .mount(server)
            .await;

        let details = collectible_details(&["abc".to_string()]).await.expect("details");
        assert_eq!(
            details,
            vec![CollectibleDetails {
                collectible_item_id: "abc".into(),
                collectible_product_id: "p1".into(),
                price: 0,
                creator_id: 1,
            }]
        );
    }

    #[tokio::test]
    async fn bundle_asset_ids_skips_the_user_outfit() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("catalog", "/v1/bundles/192/details")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    { "id": 1, "type": "Asset", "assetType": 28 },
                    { "id": 2, "type": "UserOutfit" },
                    { "id": 3, "type": "Asset", "assetType": 17 },
                    { "id": 4, "type": "Asset" }
                ]
            })))
            .mount(server)
            .await;

        assert_eq!(
            bundle_asset_ids(192).await.expect("bundle"),
            vec![
                BundleAsset { id: 1, asset_type: 28 },
                BundleAsset { id: 3, asset_type: 17 },
                // Sem `assetType` a peça continua; só não entra no conflito de tipos.
                BundleAsset { id: 4, asset_type: 0 },
            ]
        );
    }

    #[tokio::test]
    async fn owns_item_uses_the_account_cookie() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("inventory", "/v1/users/42/items/Asset/451221329/is-owned")))
            .and(header("cookie", cookie_of("own-acc")))
            .respond_with(ResponseTemplate::new(200).set_body_string("true"))
            .mount(server)
            .await;

        assert!(owns_item("own-acc", 42, CatalogItemKind::Asset, 451221329)
            .await
            .expect("owns"));
    }

    fn free_details(collectible: &str, product: &str) -> CollectibleDetails {
        CollectibleDetails {
            collectible_item_id: collectible.into(),
            collectible_product_id: product.into(),
            price: 0,
            creator_id: 1,
        }
    }

    #[tokio::test]
    async fn claim_sends_price_zero_and_reports_claimed() {
        let server = mock_server().await;
        mount_csrf("claim-ok", "csrf-claim-ok").await;
        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/marketplace-sales/v1/item/abc/purchase-item")))
            .and(header("cookie", cookie_of("claim-ok")))
            .and(header("x-csrf-token", "csrf-claim-ok"))
            .and(body_partial_json(serde_json::json!({
                "expectedPrice": 0,
                "expectedCurrency": 1,
                "collectibleProductId": "p1",
                "expectedPurchaserId": "42",
                "expectedSellerId": 1
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "purchased": true })),
            )
            .mount(server)
            .await;

        let outcome = claim_free_item("claim-ok", 42, &free_details("abc", "p1")).await;
        assert_eq!(outcome, ClaimOutcome::Claimed);
    }

    async fn mount_declined(token: &str, collectible: &str, message: &str) {
        mount_csrf(token, &format!("csrf-{}", token)).await;
        Mock::given(method("POST"))
            .and(path(mock_path(
                "apis",
                &format!("/marketplace-sales/v1/item/{}/purchase-item", collectible),
            )))
            .and(header("cookie", cookie_of(token)))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({ "purchased": false, "errorMessage": message }),
            ))
            .mount(mock_server().await)
            .await;
    }

    #[tokio::test]
    async fn claim_maps_price_mismatch_to_not_free() {
        mount_declined("claim-mismatch", "mismatch-item", "PriceMismatch").await;
        let outcome =
            claim_free_item("claim-mismatch", 42, &free_details("mismatch-item", "p2")).await;
        assert_eq!(outcome, ClaimOutcome::NotFree);
    }

    #[tokio::test]
    async fn claim_maps_already_owned() {
        mount_declined("claim-owned", "owned-item", "AlreadyOwned").await;
        let outcome = claim_free_item("claim-owned", 42, &free_details("owned-item", "p3")).await;
        assert_eq!(outcome, ClaimOutcome::AlreadyOwned);
    }

    #[tokio::test]
    async fn claim_keeps_other_messages_as_failed() {
        mount_declined("claim-other", "other-item", "SomethingElse").await;
        let outcome = claim_free_item("claim-other", 42, &free_details("other-item", "p4")).await;
        assert_eq!(outcome, ClaimOutcome::Failed("SomethingElse".into()));
    }

    #[tokio::test]
    async fn claim_detects_a_challenge() {
        let server = mock_server().await;
        mount_csrf("claim-challenge", "csrf-claim-challenge").await;
        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/marketplace-sales/v1/item/challenge-item/purchase-item")))
            .and(header("cookie", cookie_of("claim-challenge")))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("rblx-challenge-id", "x")
                    .insert_header("rblx-challenge-type", "captcha"),
            )
            .mount(server)
            .await;

        let outcome =
            claim_free_item("claim-challenge", 42, &free_details("challenge-item", "p5")).await;
        assert_eq!(outcome, ClaimOutcome::ChallengeRequired);
    }

    #[tokio::test]
    async fn claim_never_sends_when_details_are_not_free() {
        let server = mock_server().await;
        let calls = Mock::given(method("POST"))
            .and(path(mock_path("apis", "/marketplace-sales/v1/item/paid-item/purchase-item")))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount_as_scoped(server)
            .await;

        let mut paid = free_details("paid-item", "p6");
        paid.price = 5;
        assert_eq!(claim_free_item("claim-paid", 42, &paid).await, ClaimOutcome::NotFree);

        let mut third_party = free_details("paid-item", "p6");
        third_party.creator_id = 999;
        assert_eq!(
            claim_free_item("claim-paid", 42, &third_party).await,
            ClaimOutcome::NotFree
        );

        drop(calls);
    }
}
