// Catálogo oficial gratuito do Roblox.
//
// Só entra aqui o que é de criador Roblox (`creatorTargetId == 1`) e custa
// zero. O resgate desses itens para uma conta fica em `avatar_claim.rs`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CatalogItemKind {
    Asset,
    Bundle,
}

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

/// T-Shirt, Hat, Shirt, Pants, Hair, Face/Neck/Shoulder/Front/Back/Waist acc.
pub const ALLOWED_ASSET_TYPES: &[i64] = &[2, 8, 11, 12, 41, 42, 43, 44, 45, 46, 47];
/// Body, Dynamic head.
pub const ALLOWED_BUNDLE_TYPES: &[i64] = &[1, 4];

const CATALOG_PAGE_LIMIT: u32 = 120;
/// Teto de páginas da busca, para um cursor que nunca acaba não prender o app.
const CATALOG_MAX_PAGES: usize = 20;
/// Criador "Roblox".
const ROBLOX_CREATOR_ID: i64 = 1;

fn catalog_item_from_json(v: &serde_json::Value) -> Option<FreeCatalogItem> {
    let price = v.get("price").and_then(|p| p.as_i64()).unwrap_or(-1);
    let creator = v.get("creatorTargetId").and_then(|c| c.as_i64()).unwrap_or(0);
    if price != 0 || creator != ROBLOX_CREATOR_ID {
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

/// Percorre o catálogo gratuito do Roblox e devolve só os itens permitidos
/// (acessórios/roupas clássicas e bundles de corpo), na ordem do catálogo.
pub async fn search_free_official_items(page_pause: Duration) -> Result<Vec<FreeCatalogItem>, String> {
    let client = http_client::client();
    let mut items = Vec::new();
    let mut cursor: Option<String> = None;

    for page in 0..CATALOG_MAX_PAGES {
        if page > 0 {
            sleep(page_pause).await;
        }

        let mut url = format!(
            "{}/v2/search/items/details?Category=1&CreatorName=Roblox&MaxPrice=0&MinPrice=0&SalesTypeFilter=1&Limit={}",
            endpoints::host("catalog"),
            CATALOG_PAGE_LIMIT
        );
        if let Some(c) = &cursor {
            url.push_str("&Cursor=");
            url.push_str(c);
        }

        let response = send_with_retry(|| client.get(&url).header("Accept", "application/json")).await?;
        if !response.status().is_success() {
            return Err(format!("Catalog search failed (status {})", response.status().as_u16()));
        }
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse catalog search: {}", e))?;

        if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
            items.extend(data.iter().filter_map(catalog_item_from_json));
        }

        cursor = body
            .get("nextPageCursor")
            .and_then(|c| c.as_str())
            .filter(|c| !c.is_empty())
            .map(|c| c.to_string());
        if cursor.is_none() {
            break;
        }
    }

    Ok(items)
}

// O resgate (o que só a distribuição em lote usa) mora à parte e só entra na
// edição completa.
#[cfg(feature = "avatar-batch")]
include!("avatar_claim.rs");

#[cfg(test)]
mod avatar_catalog_tests {
    use super::*;
    use crate::api::endpoints::test_support::{mock_path, mock_server};
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, Request, ResponseTemplate};

    fn catalog_item(
        id: i64,
        item_type: &str,
        type_key: &str,
        type_id: i64,
        price: i64,
        creator: i64,
        collectible: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "itemType": item_type,
            type_key: type_id,
            "name": format!("item {}", id),
            "price": price,
            "creatorTargetId": creator,
            "collectibleItemId": collectible,
        })
    }

    #[tokio::test]
    async fn search_keeps_only_free_official_allowed_items() {
        let server = mock_server().await;
        let route = mock_path("catalog", "/v2/search/items/details");

        Mock::given(method("GET"))
            .and(path(route.clone()))
            .and(query_param("Cursor", "c2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [catalog_item(6, "Asset", "assetType", 11, 0, 1, "col-shirt")],
                "nextPageCursor": serde_json::Value::Null,
            })))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path(route))
            .and(query_param("CreatorName", "Roblox"))
            .and(query_param("MaxPrice", "0"))
            .and(|req: &Request| !req.url.query_pairs().any(|(k, _)| k == "Cursor"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    catalog_item(1, "Asset", "assetType", 41, 0, 1, "col-hair"),
                    catalog_item(2, "Asset", "assetType", 8, 5, 1, "col-paid"),
                    catalog_item(3, "Asset", "assetType", 8, 0, 999, "col-ugc"),
                    catalog_item(4, "Asset", "assetType", 64, 0, 1, "col-layered"),
                    catalog_item(5, "Bundle", "bundleType", 1, 0, 1, "col-body"),
                ],
                "nextPageCursor": "c2",
            })))
            .mount(server)
            .await;

        let items = search_free_official_items(Duration::ZERO).await.expect("search");
        let got: Vec<(i64, CatalogItemKind, i64, &str)> = items
            .iter()
            .map(|i| (i.id, i.kind, i.type_id, i.collectible_item_id.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                (1, CatalogItemKind::Asset, 41, "col-hair"),
                (5, CatalogItemKind::Bundle, 1, "col-body"),
                (6, CatalogItemKind::Asset, 11, "col-shirt"),
            ]
        );
    }
}
