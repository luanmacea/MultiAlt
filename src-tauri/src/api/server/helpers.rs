fn reply(status: u16, message: &str, v2: bool) -> Response {
    let body = if v2 {
        serde_json::json!({
            "Success": status < 300,
            "Message": message,
        })
        .to_string()
    } else {
        message.to_string()
    };

    let mut response = Response::builder().status(status);

    if v2 {
        response = response.header("content-type", "application/json; charset=utf-8");
    } else {
        response = response.header("content-type", "text/plain; charset=utf-8");
        if status > 299 {
            response = response.header("ws-error", message);
        }
    }

    response.body(Body::from(body)).unwrap()
}

fn find_account(accounts: &[crate::data::accounts::Account], identifier: &str) -> Option<crate::data::accounts::Account> {
    accounts
        .iter()
        .find(|a| a.username == identifier || a.user_id.to_string() == identifier)
        .cloned()
}


/// `reply` and `find_account`, plus the fixtures the other web-server test
/// modules build their routers from.
#[cfg(test)]
mod server_helpers_tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tower::ServiceExt;

    /// A leaked-store fixture wired exactly like the real server: the same
    /// `build_router`, so the middleware and the v1/v2 route wrappers are part
    /// of every request these tests make.
    ///
    /// The stores are `&'static` (that is what `AppState` holds), so they are
    /// deliberately leaked; only the two files are cleaned up.
    pub(super) struct TestApp {
        pub settings: &'static SettingsStore,
        pub accounts: &'static AccountStore,
        paths: Vec<PathBuf>,
    }

    impl TestApp {
        /// `name` must be unique per test: it goes into the temp file names.
        pub fn new(name: &str) -> Self {
            let settings_path = unique_path(name, "ini");
            let accounts_path = unique_path(name, "json");

            let settings: &'static SettingsStore =
                Box::leak(Box::new(SettingsStore::new(settings_path.clone())));
            let accounts: &'static AccountStore =
                Box::leak(Box::new(AccountStore::new(accounts_path.clone())));

            Self {
                settings,
                accounts,
                paths: vec![settings_path, accounts_path],
            }
        }

        pub fn state(&self) -> AppState {
            AppState {
                accounts: self.accounts,
                settings: self.settings,
            }
        }

        /// Turns on a setting in the `WebServer` section.
        pub fn allow(&self, key: &str) -> &Self {
            self.settings.set("WebServer", key, "true").unwrap();
            self
        }

        pub fn deny(&self, key: &str) -> &Self {
            self.settings.set("WebServer", key, "false").unwrap();
            self
        }

        /// Sets the web server password (must be 6+ chars to be accepted).
        pub fn password(&self, password: &str) -> &Self {
            self.settings.set("WebServer", "Password", password).unwrap();
            self
        }

        pub fn add_account(&self, username: &str, user_id: i64, token: &str) {
            let account = crate::data::accounts::Account::new(
                token.to_string(),
                username.to_string(),
                user_id,
            );
            self.accounts.add(account).expect("account should be added");
        }

        /// Drives one GET through the real router and returns status + body.
        pub async fn get(&self, uri: &str) -> (u16, String) {
            self.send("GET", uri, None).await
        }

        /// The same routing table as `build_router`, minus `/LaunchAccount`
        /// and `/FollowUser`.
        ///
        /// Those two are the only handlers that reach the Windows launch path,
        /// and merely *referencing* them from a test pulls tauri's wry runtime
        /// into the test binary. That runtime imports `TaskDialogIndirect`,
        /// which only resolves through the comctl32 v6 manifest the real app
        /// binary carries — a `cargo test` binary has no manifest, so the whole
        /// test executable then fails to start with STATUS_ENTRYPOINT_NOT_FOUND
        /// before a single test runs. Keep them out; see `handlers_launch.rs`.
        pub fn router(&self) -> Router {
            Router::new()
                .route("/Running", get(v1_running))
                .route("/v2/Running", get(v2_running))
                .route("/GetAccounts", get(v1_get_accounts))
                .route("/v2/GetAccounts", get(v2_get_accounts))
                .route("/GetAccountsJson", get(v1_get_accounts_json))
                .route("/v2/GetAccountsJson", get(v2_get_accounts_json))
                .route("/ImportCookie", get(v1_import_cookie))
                .route("/v2/ImportCookie", get(v2_import_cookie))
                .route("/GetCookie", get(v1_get_cookie))
                .route("/v2/GetCookie", get(v2_get_cookie))
                .route("/GetCSRFToken", get(v1_get_csrf_token))
                .route("/v2/GetCSRFToken", get(v2_get_csrf_token))
                .route("/SetServer", get(v1_set_server))
                .route("/v2/SetServer", get(v2_set_server))
                .route("/SetRecommendedServer", get(v1_set_recommended_server))
                .route("/v2/SetRecommendedServer", get(v2_set_recommended_server))
                .route("/GetAlias", get(v1_get_alias))
                .route("/v2/GetAlias", get(v2_get_alias))
                .route("/GetDescription", get(v1_get_description))
                .route("/v2/GetDescription", get(v2_get_description))
                .route("/GetField", get(v1_get_field))
                .route("/v2/GetField", get(v2_get_field))
                .route("/SetField", post(v1_set_field))
                .route("/v2/SetField", post(v2_set_field))
                .route("/RemoveField", post(v1_remove_field))
                .route("/v2/RemoveField", post(v2_remove_field))
                .route("/SetAlias", post(v1_set_alias))
                .route("/v2/SetAlias", post(v2_set_alias))
                .route("/SetDescription", post(v1_set_description))
                .route("/v2/SetDescription", post(v2_set_description))
                .route("/AppendDescription", post(v1_append_description))
                .route("/v2/AppendDescription", post(v2_append_description))
                .route("/SetAvatar", post(v1_set_avatar))
                .route("/v2/SetAvatar", post(v2_set_avatar))
                .route("/BlockUser", post(v1_block_user))
                .route("/v2/BlockUser", post(v2_block_user))
                .route("/UnblockUser", post(v1_unblock_user))
                .route("/v2/UnblockUser", post(v2_unblock_user))
                .route("/GetBlockedList", get(v1_get_blocked_list))
                .route("/v2/GetBlockedList", get(v2_get_blocked_list))
                .route("/UnblockEveryone", post(v1_unblock_everyone))
                .route("/v2/UnblockEveryone", post(v2_unblock_everyone))
                .layer(middleware::from_fn_with_state((), external_check))
                .layer(Extension(self.state()))
        }

        pub async fn send(&self, verb: &str, uri: &str, body: Option<&str>) -> (u16, String) {
            let router = self.router();
            let request = Request::builder()
                .method(verb)
                .uri(uri)
                .body(match body {
                    Some(b) => Body::from(b.to_string()),
                    None => Body::empty(),
                })
                .unwrap();
            let response = router.oneshot(request).await.unwrap();
            let status = response.status().as_u16();
            let bytes = http_body_util::BodyExt::collect(response.into_body())
                .await
                .unwrap()
                .to_bytes();
            (status, String::from_utf8_lossy(&bytes).to_string())
        }
    }

    impl Drop for TestApp {
        fn drop(&mut self) {
            for path in &self.paths {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    pub(super) fn unique_path(name: &str, ext: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("ram-api-{name}-{nanos}.{ext}"))
    }

    fn account(username: &str, user_id: i64, token: &str) -> crate::data::accounts::Account {
        crate::data::accounts::Account::new(token.to_string(), username.to_string(), user_id)
    }

    async fn body_of(response: Response) -> (u16, String) {
        let status = response.status().as_u16();
        let body = http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes();
        (status, String::from_utf8_lossy(&body).to_string())
    }

    /// v1 answers in plain text; scripts read the body as-is.
    #[tokio::test]
    async fn a_v1_reply_is_plain_text() {
        let response = reply(200, "true", false);
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok()),
            Some("text/plain; charset=utf-8")
        );
        assert!(response.headers().get("ws-error").is_none());
        assert_eq!(body_of(response).await, (200, "true".to_string()));
    }

    /// v1 errors also carry the message in a header, which the Lua client
    /// reads without touching the body.
    #[tokio::test]
    async fn a_v1_error_repeats_the_message_in_the_ws_error_header() {
        let response = reply(404, "Account not found", false);
        assert_eq!(
            response
                .headers()
                .get("ws-error")
                .and_then(|v| v.to_str().ok()),
            Some("Account not found")
        );
        assert_eq!(body_of(response).await, (404, "Account not found".to_string()));
    }

    /// v2 always answers with the `{Success, Message}` envelope.
    #[tokio::test]
    async fn a_v2_reply_is_a_success_envelope() {
        let (status, body) = body_of(reply(200, "ok", true)).await;
        assert_eq!(status, 200);
        let json: serde_json::Value = serde_json::from_str(&body).expect("v2 replies with JSON");
        assert_eq!(json["Success"], serde_json::json!(true));
        assert_eq!(json["Message"], serde_json::json!("ok"));

        let (status, body) = body_of(reply(401, "Invalid password", true)).await;
        assert_eq!(status, 401);
        let json: serde_json::Value = serde_json::from_str(&body).expect("v2 replies with JSON");
        assert_eq!(json["Success"], serde_json::json!(false));
        assert_eq!(json["Message"], serde_json::json!("Invalid password"));
    }

    /// 3xx counts as a failure in the envelope, since `Success` is `status < 300`.
    #[tokio::test]
    async fn a_v2_redirect_status_is_not_a_success() {
        let (_, body) = body_of(reply(302, "moved", true)).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["Success"], serde_json::json!(false));
    }

    /// v2 never sets `ws-error`: the message is inside the envelope.
    #[test]
    fn a_v2_error_has_no_ws_error_header() {
        let response = reply(500, "boom", true);
        assert!(response.headers().get("ws-error").is_none());
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok()),
            Some("application/json; charset=utf-8")
        );
    }

    #[test]
    fn an_account_is_found_by_username_or_user_id() {
        let accounts = vec![
            account("alt_one", 111, "token-one"),
            account("alt_two", 222, "token-two"),
        ];

        assert_eq!(
            find_account(&accounts, "alt_two").map(|a| a.user_id),
            Some(222)
        );
        assert_eq!(
            find_account(&accounts, "111").map(|a| a.username),
            Some("alt_one".to_string())
        );
    }

    #[test]
    fn an_unknown_identifier_finds_nothing() {
        let accounts = vec![account("alt_one", 111, "token-one")];
        assert!(find_account(&accounts, "nobody").is_none());
        assert!(find_account(&accounts, "999").is_none());
        assert!(find_account(&accounts, "").is_none());
        assert!(find_account(&[], "alt_one").is_none());
    }

    /// Documented limitation: the lookup is exact and case-sensitive, and an
    /// alias is not an identifier.
    #[test]
    fn the_lookup_is_exact_and_ignores_the_alias() {
        let mut aliased = account("alt_one", 111, "token-one");
        aliased.alias = "Main".to_string();
        let accounts = vec![aliased];

        assert!(find_account(&accounts, "Main").is_none());
        assert!(find_account(&accounts, "ALT_ONE").is_none());
        assert!(find_account(&accounts, " alt_one").is_none());
    }
}
