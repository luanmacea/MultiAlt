async fn v1_running(ext: Extension<AppState>) -> Response {
    handle_running(ext, false).await
}
async fn v2_running(ext: Extension<AppState>) -> Response {
    handle_running(ext, true).await
}

async fn v1_get_accounts(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_get_accounts(ext, q, false).await
}
async fn v2_get_accounts(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_get_accounts(ext, q, true).await
}

async fn v1_get_accounts_json(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_get_accounts_json(ext, q, false).await
}
async fn v2_get_accounts_json(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_get_accounts_json(ext, q, true).await
}

async fn v1_import_cookie(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_import_cookie(ext, q, false).await
}
async fn v2_import_cookie(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_import_cookie(ext, q, true).await
}

async fn v1_get_cookie(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_get_cookie(ext, q, false).await
}
async fn v2_get_cookie(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_get_cookie(ext, q, true).await
}

async fn v1_get_csrf_token(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_get_csrf_token(ext, q, false).await
}
async fn v2_get_csrf_token(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_get_csrf_token(ext, q, true).await
}

async fn v1_launch_account(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_launch_account(ext, q, false).await
}
async fn v2_launch_account(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_launch_account(ext, q, true).await
}

async fn v1_follow_user(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_follow_user(ext, q, false).await
}
async fn v2_follow_user(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_follow_user(ext, q, true).await
}

async fn v1_set_server(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_set_server(ext, q, false).await
}
async fn v2_set_server(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_set_server(ext, q, true).await
}

async fn v1_set_recommended_server(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_set_recommended_server(ext, q, false).await
}
async fn v2_set_recommended_server(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_set_recommended_server(ext, q, true).await
}

async fn v1_get_alias(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_get_alias(ext, q, false).await
}
async fn v2_get_alias(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_get_alias(ext, q, true).await
}

async fn v1_get_description(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_get_description(ext, q, false).await
}
async fn v2_get_description(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_get_description(ext, q, true).await
}

async fn v1_get_field(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_get_field(ext, q, false).await
}
async fn v2_get_field(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_get_field(ext, q, true).await
}

async fn v1_set_field(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_set_field(ext, q, false).await
}
async fn v2_set_field(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_set_field(ext, q, true).await
}

async fn v1_remove_field(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_remove_field(ext, q, false).await
}
async fn v2_remove_field(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_remove_field(ext, q, true).await
}

async fn v1_set_alias(ext: Extension<AppState>, q: Query<AccountQuery>, body: String) -> Response {
    handle_set_alias(ext, q, body, false).await
}
async fn v2_set_alias(ext: Extension<AppState>, q: Query<AccountQuery>, body: String) -> Response {
    handle_set_alias(ext, q, body, true).await
}

async fn v1_set_description(ext: Extension<AppState>, q: Query<AccountQuery>, body: String) -> Response {
    handle_set_description(ext, q, body, false).await
}
async fn v2_set_description(ext: Extension<AppState>, q: Query<AccountQuery>, body: String) -> Response {
    handle_set_description(ext, q, body, true).await
}

async fn v1_append_description(ext: Extension<AppState>, q: Query<AccountQuery>, body: String) -> Response {
    handle_append_description(ext, q, body, false).await
}
async fn v2_append_description(ext: Extension<AppState>, q: Query<AccountQuery>, body: String) -> Response {
    handle_append_description(ext, q, body, true).await
}

async fn v1_set_avatar(ext: Extension<AppState>, q: Query<AccountQuery>, body: String) -> Response {
    handle_set_avatar(ext, q, body, false).await
}
async fn v2_set_avatar(ext: Extension<AppState>, q: Query<AccountQuery>, body: String) -> Response {
    handle_set_avatar(ext, q, body, true).await
}

async fn v1_block_user(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_block_user(ext, q, false).await
}
async fn v2_block_user(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_block_user(ext, q, true).await
}

async fn v1_unblock_user(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_unblock_user(ext, q, false).await
}
async fn v2_unblock_user(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_unblock_user(ext, q, true).await
}

async fn v1_get_blocked_list(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_get_blocked_list(ext, q, false).await
}
async fn v2_get_blocked_list(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_get_blocked_list(ext, q, true).await
}

async fn v1_unblock_everyone(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_unblock_everyone(ext, q, false).await
}
async fn v2_unblock_everyone(ext: Extension<AppState>, q: Query<AccountQuery>) -> Response {
    handle_unblock_everyone(ext, q, true).await
}



/// The v1/v2 wrappers exist only to pass the `v2` flag and to be namable in
/// `build_router`. What matters is that each path reaches the right handler
/// with the right flag, and that the HTTP verbs stay as the Lua clients expect.
///
/// The fixture mirrors `build_router` minus the two launch routes; see
/// `server_helpers_tests::TestApp::router` for why they cannot be linked into a
/// test binary.
#[cfg(test)]
mod route_wrapper_tests {
    use super::server_helpers_tests::TestApp;

    const PASSWORD: &str = "sup3rsecret";

    /// The same endpoint answers plain text under `/X` and the JSON envelope
    /// under `/v2/X`.
    #[tokio::test]
    async fn each_endpoint_is_wired_under_both_versions() {
        let app = TestApp::new("wrappers-both");
        app.allow("AllowGetAccounts").password(PASSWORD);
        app.add_account("alt_one", 111, "token-one");

        for path in ["/Running", "/GetAccounts", "/GetAccountsJson"] {
            let (v1_status, v1_body) = app.get(path).await;
            assert_eq!(v1_status, 200, "v1 {}", path);

            let (v2_status, v2_body) = app.get(&format!("/v2{}", path)).await;
            assert_eq!(v2_status, 200, "v2 {}", path);

            let json: serde_json::Value =
                serde_json::from_str(&v2_body).unwrap_or_else(|_| panic!("v2 {} is JSON", path));
            assert_eq!(json["Success"], serde_json::json!(true), "v2 {}", path);
            assert_ne!(v1_body, v2_body, "{} must differ between versions", path);
        }
    }

    /// Endpoints that change something are POST-only; a GET must not fall
    /// through to a read endpoint.
    #[tokio::test]
    async fn the_editing_endpoints_are_post_only() {
        let app = TestApp::new("wrappers-verbs");
        app.password(PASSWORD);

        for path in [
            "/SetField",
            "/RemoveField",
            "/SetAlias",
            "/SetDescription",
            "/AppendDescription",
            "/SetAvatar",
            "/BlockUser",
            "/UnblockUser",
            "/UnblockEveryone",
        ] {
            let (status, _) = app.get(path).await;
            assert_eq!(status, 405, "GET {} must not be allowed", path);

            let (v2_status, _) = app.get(&format!("/v2{}", path)).await;
            assert_eq!(v2_status, 405, "GET /v2{} must not be allowed", path);
        }
    }

    /// The read endpoints are GET-only in the same way.
    #[tokio::test]
    async fn the_read_endpoints_are_get_only() {
        let app = TestApp::new("wrappers-read-verbs");
        app.password(PASSWORD);

        for path in ["/Running", "/GetAccounts", "/GetCookie", "/GetBlockedList"] {
            let (status, _) = app.send("POST", path, None).await;
            assert_eq!(status, 405, "POST {} must not be allowed", path);
        }
    }

    #[tokio::test]
    async fn an_unknown_path_is_a_404() {
        let app = TestApp::new("wrappers-unknown");
        assert_eq!(app.get("/NotAnEndpoint").await.0, 404);
        assert_eq!(app.get("/v2/NotAnEndpoint").await.0, 404);
        // The routes are matched exactly, not case-insensitively.
        assert_eq!(app.get("/running").await.0, 404);
    }
}
