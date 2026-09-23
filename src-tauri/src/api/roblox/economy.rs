#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetCreator {
    #[serde(rename = "Id")]
    pub id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetDetails {
    #[serde(rename = "Id")]
    pub id: i64,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "IsForSale", default)]
    pub is_for_sale: bool,
    #[serde(rename = "PriceInRobux")]
    pub price_in_robux: Option<i64>,
    #[serde(rename = "ProductId")]
    pub product_id: Option<i64>,
    #[serde(rename = "Creator")]
    pub creator: AssetCreator,
}

pub async fn get_asset_details(asset_id: i64, security_token: Option<&str>) -> Result<AssetDetails, String> {
    let client = reqwest::Client::new();

    let mut request = client
        .get(format!("{}/v2/assets/{}/details", endpoints::host("economy"), asset_id))
        .header("Accept", "application/json");

    if let Some(token) = security_token {
        request = request.header(COOKIE, cookie_header(token));
    }

    let response = request.send().await.map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Failed to get asset details (status {})", response.status().as_u16()));
    }

    response.json().await.map_err(|e| format!("Failed to parse asset details: {}", e))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseResult {
    pub purchased: bool,
    #[serde(rename = "errorMsg")]
    pub error_msg: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

pub async fn purchase_product(
    security_token: &str,
    product_id: i64,
    expected_price: i64,
    expected_seller_id: i64,
) -> Result<PurchaseResult, String> {
    let csrf = crate::api::auth::get_csrf_token(security_token).await?;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{}/v1/purchases/products/{}", endpoints::host("economy"), product_id))
        .header(COOKIE, cookie_header(security_token))
        .header("X-CSRF-Token", &csrf)
        .json(&serde_json::json!({
            "expectedCurrency": 1,
            "expectedPrice": expected_price,
            "expectedSellerId": expected_seller_id,
        }))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Purchase failed: {}", body));
    }

    response.json().await.map_err(|e| format!("Failed to parse purchase result: {}", e))
}

#[cfg(test)]
mod economy_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    #[tokio::test]
    async fn reads_the_robux_balance() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("economy", "/v1/user/currency")))
            .and(header("cookie", cookie_of("robux-account")))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "robux": 1337 })),
            )
            .mount(server)
            .await;

        assert_eq!(get_robux("robux-account").await.expect("robux"), 1337);
    }

    #[tokio::test]
    async fn reads_asset_details() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("economy", "/v2/assets/4242/details")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "Id": 4242,
                "Name": "Cool Hat",
                "IsForSale": true,
                "PriceInRobux": 25,
                "ProductId": 9001,
                "Creator": { "Id": 1 }
            })))
            .mount(server)
            .await;

        let details = get_asset_details(4242, None).await.expect("asset details");
        assert_eq!(details.name, "Cool Hat");
        assert_eq!(details.price_in_robux, Some(25));
        assert_eq!(details.product_id, Some(9001));
        assert_eq!(details.creator.id, 1);
    }
}
