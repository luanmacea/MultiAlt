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
    let client = http_client::client();

    let mut request = client
        .get(format!("{}/v2/assets/{}/details", endpoints::host("economy"), asset_id))
        .header("Accept", "application/json");

    if let Some(token) = security_token {
        request = request.header(COOKIE, cookie_header(token));
    }

    let response = request.send().await.map_err(|e| http_client::describe_error(&e))?;

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
    let client = http_client::client();

    let request = client
        .post(format!("{}/v1/purchases/products/{}", endpoints::host("economy"), product_id))
        .header(COOKIE, cookie_header(security_token))
        .json(&serde_json::json!({
            "expectedCurrency": 1,
            "expectedPrice": expected_price,
            "expectedSellerId": expected_seller_id,
        }));
    let response = crate::api::auth::send_with_csrf_retry(request, &csrf).await?;

    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Purchase failed: {}", body));
    }

    response.json().await.map_err(|e| format!("Failed to parse purchase result: {}", e))
}
