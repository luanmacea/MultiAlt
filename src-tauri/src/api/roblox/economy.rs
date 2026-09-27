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

/// Asset details error paths and the purchase call.
#[cfg(test)]
mod economy_extra_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server, mount_csrf};
    use wiremock::matchers::{body_string_contains, header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    /// Free items have no price and no product id; they must still parse.
    #[tokio::test]
    async fn asset_details_allow_a_missing_price_and_product() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("economy", "/v2/assets/4243/details")))
            .and(header("cookie", cookie_of("asset-free")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "Id": 4243,
                "Creator": { "Id": 1 }
            })))
            .mount(server)
            .await;

        let details = get_asset_details(4243, Some("asset-free"))
            .await
            .expect("asset details");
        assert_eq!(details.id, 4243);
        assert!(details.name.is_empty());
        assert!(!details.is_for_sale);
        assert!(details.price_in_robux.is_none());
        assert!(details.product_id.is_none());
    }

    #[tokio::test]
    async fn asset_details_report_the_status() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("economy", "/v2/assets/4244/details")))
            .respond_with(ResponseTemplate::new(404))
            .mount(server)
            .await;

        assert_eq!(
            get_asset_details(4244, None).await.unwrap_err(),
            "Failed to get asset details (status 404)"
        );
    }

    /// A payload without the mandatory `Creator` cannot be used downstream.
    #[tokio::test]
    async fn asset_details_without_a_creator_are_a_parse_error() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("economy", "/v2/assets/4245/details")))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "Id": 4245 })),
            )
            .mount(server)
            .await;

        let err = get_asset_details(4245, None).await.unwrap_err();
        assert!(
            err.starts_with("Failed to parse asset details: "),
            "unexpected error: {}",
            err
        );
    }

    #[tokio::test]
    async fn a_purchase_sends_the_expected_price_and_seller() {
        let server = mock_server().await;
        mount_csrf("buy-ok", "csrf-buy-ok").await;

        Mock::given(method("POST"))
            .and(path(mock_path("economy", "/v1/purchases/products/9001")))
            .and(header("cookie", cookie_of("buy-ok")))
            .and(header("x-csrf-token", "csrf-buy-ok"))
            .and(body_string_contains("\"expectedPrice\":25"))
            .and(body_string_contains("\"expectedSellerId\":77"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({ "purchased": true, "errorMsg": serde_json::Value::Null }),
            ))
            .mount(server)
            .await;

        let result = purchase_product("buy-ok", 9001, 25, 77)
            .await
            .expect("purchase");
        assert!(result.purchased);
        assert!(result.error_msg.is_none());
        assert!(result.title.is_none());
    }

    #[tokio::test]
    async fn a_declined_purchase_is_parsed_from_a_200_body() {
        let server = mock_server().await;
        mount_csrf("buy-declined", "csrf-buy-declined").await;

        Mock::given(method("POST"))
            .and(path(mock_path("economy", "/v1/purchases/products/9002")))
            .and(header("cookie", cookie_of("buy-declined")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "purchased": false,
                "errorMsg": "InsufficientFunds",
                "title": "Not enough Robux"
            })))
            .mount(server)
            .await;

        let result = purchase_product("buy-declined", 9002, 10, 1)
            .await
            .expect("purchase");
        assert!(!result.purchased);
        assert_eq!(result.error_msg.as_deref(), Some("InsufficientFunds"));
        assert_eq!(result.title.as_deref(), Some("Not enough Robux"));
    }

    #[tokio::test]
    async fn a_failing_purchase_echoes_the_body() {
        let server = mock_server().await;
        mount_csrf("buy-error", "csrf-buy-error").await;

        Mock::given(method("POST"))
            .and(path(mock_path("economy", "/v1/purchases/products/9003")))
            .and(header("cookie", cookie_of("buy-error")))
            .respond_with(ResponseTemplate::new(429).set_body_string("Too many purchases"))
            .mount(server)
            .await;

        assert_eq!(
            purchase_product("buy-error", 9003, 1, 1).await.unwrap_err(),
            "Purchase failed: Too many purchases"
        );
    }
}
