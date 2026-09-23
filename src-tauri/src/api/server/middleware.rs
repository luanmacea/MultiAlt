async fn external_check(
    Extension(state): Extension<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();
    let is_v2 = path.starts_with("/v2/");
    let is_running = path.eq_ignore_ascii_case("/Running") || path.eq_ignore_ascii_case("/v2/Running");

    // Block requests issued by web pages (CSRF): any site could otherwise hit
    // http://127.0.0.1:<port>/... via <img>/fetch and read or edit accounts.
    // Browsers always tag those with Origin and/or Sec-Fetch-Site; Lua
    // executors, curl and scripts don't send either header.
    let from_browser_page = req.headers().contains_key("origin")
        || req
            .headers()
            .get("sec-fetch-site")
            .and_then(|v| v.to_str().ok())
            .map(|v| !v.eq_ignore_ascii_case("none"))
            .unwrap_or(false);
    if from_browser_page {
        return reply(403, "Requests from web pages are not allowed", is_v2);
    }

    let allow_external = state.settings.get_bool("WebServer", "AllowExternalConnections");

    if !allow_external {
        if let Some(addr) = req.extensions().get::<axum::extract::ConnectInfo<SocketAddr>>() {
            let ip = addr.ip();
            if !ip.is_loopback() {
                return reply(403, "External connections are not allowed", is_v2);
            }
        }
    }

    if !is_running && state.settings.get_bool("WebServer", "EveryRequestRequiresPassword") {
        let ws_password = state.settings.get_string("WebServer", "Password");
        let provided_password = req
            .uri()
            .query()
            .and_then(|query| {
                query.split('&').find_map(|entry| {
                    let mut parts = entry.splitn(2, '=');
                    let key = parts.next().unwrap_or_default();
                    if !key.eq_ignore_ascii_case("password") {
                        return None;
                    }
                    let raw = parts.next().unwrap_or_default().replace('+', " ");
                    Some(urlencoding::decode(&raw).map(|v| v.into_owned()).unwrap_or(raw))
                })
            });

        if ws_password.len() < 6 || provided_password.as_deref() != Some(ws_password.as_str()) {
            return reply(
                401,
                "Invalid Password, make sure your password contains 6 or more characters",
                is_v2,
            );
        }
    }

    next.run(req).await
}


#[cfg(test)]
mod middleware_tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tower::ServiceExt;

    fn unique_path(name: &str, ext: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("ram-mw-{name}-{nanos}.{ext}"))
    }

    /// Minimal router wired exactly like `build_router`: `external_check` in
    /// front of a trivial handler, with the `AppState` extension outside it.
    fn test_router(name: &str) -> (Router, Vec<PathBuf>) {
        let settings_path = unique_path(name, "ini");
        let accounts_path = unique_path(name, "json");

        let settings: &'static SettingsStore =
            Box::leak(Box::new(SettingsStore::new(settings_path.clone())));
        let accounts: &'static AccountStore =
            Box::leak(Box::new(AccountStore::new(accounts_path.clone())));
        let state = AppState { accounts, settings };

        let router = Router::new()
            .route("/Running", get(|| async { "true" }))
            .route("/v2/Running", get(|| async { "true" }))
            .layer(middleware::from_fn_with_state((), external_check))
            .layer(Extension(state));

        (router, vec![settings_path, accounts_path])
    }

    fn cleanup(paths: Vec<PathBuf>) {
        for path in paths {
            let _ = std::fs::remove_file(path);
        }
    }

    async fn status_for(name: &str, headers: &[(&str, &str)]) -> u16 {
        let (router, paths) = test_router(name);
        let mut builder = Request::builder().uri("/Running");
        for (key, value) in headers {
            builder = builder.header(*key, *value);
        }
        let request = builder.body(Body::empty()).unwrap();
        let response = router.oneshot(request).await.unwrap();
        cleanup(paths);
        response.status().as_u16()
    }

    #[tokio::test]
    async fn an_origin_header_is_rejected() {
        // Regression guard for the CSRF fix: any web page can reach
        // http://127.0.0.1:<port>/... , and browsers always tag those requests.
        assert_eq!(status_for("origin", &[("origin", "https://evil.example")]).await, 403);
    }

    #[tokio::test]
    async fn a_null_origin_header_is_rejected_too() {
        assert_eq!(status_for("origin-null", &[("origin", "null")]).await, 403);
    }

    #[tokio::test]
    async fn sec_fetch_site_cross_site_is_rejected() {
        assert_eq!(
            status_for("cross-site", &[("sec-fetch-site", "cross-site")]).await,
            403
        );
    }

    #[tokio::test]
    async fn sec_fetch_site_same_origin_is_rejected() {
        assert_eq!(
            status_for("same-origin", &[("sec-fetch-site", "same-origin")]).await,
            403
        );
    }

    #[tokio::test]
    async fn sec_fetch_site_none_is_allowed() {
        // "none" means the user navigated directly; not a page-issued request.
        assert_eq!(status_for("none", &[("sec-fetch-site", "none")]).await, 200);
        assert_eq!(status_for("none-case", &[("sec-fetch-site", "None")]).await, 200);
    }

    #[tokio::test]
    async fn a_plain_request_without_browser_headers_is_allowed() {
        // curl / Lua executors / scripts send neither header.
        assert_eq!(status_for("plain", &[]).await, 200);
    }

    #[tokio::test]
    async fn the_rejection_body_explains_why() {
        let (router, paths) = test_router("body");
        let request = Request::builder()
            .uri("/Running")
            .header("origin", "https://evil.example")
            .body(Body::empty())
            .unwrap();
        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status().as_u16(), 403);
        let bytes = http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes();
        assert_eq!(
            String::from_utf8_lossy(&bytes),
            "Requests from web pages are not allowed"
        );
        cleanup(paths);
    }

    #[tokio::test]
    async fn the_v2_rejection_body_is_json() {
        let (router, paths) = test_router("v2body");
        let request = Request::builder()
            .uri("/v2/Running")
            .header("origin", "https://evil.example")
            .body(Body::empty())
            .unwrap();
        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status().as_u16(), 403);
        let bytes = http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("v2 replies with JSON");
        assert_eq!(json["Success"], serde_json::Value::Bool(false));
        assert_eq!(
            json["Message"],
            serde_json::Value::String("Requests from web pages are not allowed".to_string())
        );
        cleanup(paths);
    }
}
