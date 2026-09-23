async fn handle_running(Extension(_state): Extension<AppState>, v2: bool) -> Response {
    if v2 {
        reply(200, "Roblox Account Manager is running", true)
    } else {
        reply(200, "true", false)
    }
}

async fn handle_get_accounts(
    Extension(state): Extension<AppState>,
    Query(params): Query<AccountQuery>,
    v2: bool,
) -> Response {
    if !state.settings.get_bool("WebServer", "AllowGetAccounts") {
        return reply(401, "AllowGetAccounts is disabled", v2);
    }

    if !check_password(&state, &params.password) {
        return reply(401, "Invalid password", v2);
    }

    let accounts = match state.accounts.get_all() {
        Ok(a) => a,
        Err(e) => return reply(500, &e, v2),
    };

    let filtered: Vec<_> = if let Some(ref group) = params.group {
        accounts.into_iter().filter(|a| &a.group == group).collect()
    } else {
        accounts
    };

    let names: Vec<String> = filtered.iter().map(|a| a.username.clone()).collect();
    reply(200, &names.join(","), v2)
}

async fn handle_get_accounts_json(
    Extension(state): Extension<AppState>,
    Query(params): Query<AccountQuery>,
    v2: bool,
) -> Response {
    if !state.settings.get_bool("WebServer", "AllowGetAccounts") {
        return reply(401, "AllowGetAccounts is disabled", v2);
    }

    if !check_password(&state, &params.password) {
        return reply(401, "Invalid password", v2);
    }

    let include_cookies = params
        .include_cookies
        .as_deref()
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
        && state.settings.get_bool("WebServer", "AllowGetCookie")
        && check_password_required(&state, &params.password);

    let accounts = match state.accounts.get_all() {
        Ok(a) => a,
        Err(e) => return reply(500, &e, v2),
    };

    let filtered: Vec<_> = if let Some(ref group) = params.group {
        accounts.into_iter().filter(|a| &a.group == group).collect()
    } else {
        accounts
    };

    let json_accounts: Vec<serde_json::Value> = filtered
        .iter()
        .map(|a| {
            let mut obj = serde_json::json!({
                "Username": a.username,
                "UserID": a.user_id,
                "Alias": a.alias,
                "Description": a.description,
                "Group": a.group,
                "Fields": a.fields,
            });

            if include_cookies {
                obj["Cookie"] = serde_json::Value::String(a.security_token.clone());
            }

            obj
        })
        .collect();

    let body = serde_json::to_string(&json_accounts).unwrap_or_else(|_| "[]".to_string());

    if v2 {
        let wrapper = serde_json::json!({
            "Success": true,
            "Message": json_accounts,
        });
        Response::builder()
            .status(200)
            .header("content-type", "application/json; charset=utf-8")
            .body(Body::from(wrapper.to_string()))
            .unwrap()
    } else {
        Response::builder()
            .status(200)
            .header("content-type", "application/json; charset=utf-8")
            .body(Body::from(body))
            .unwrap()
    }
}

async fn handle_import_cookie(
    Extension(state): Extension<AppState>,
    Query(params): Query<AccountQuery>,
    v2: bool,
) -> Response {
    if !check_password(&state, &params.password) {
        return reply(401, "Invalid password", v2);
    }

    let cookie = match params.cookie {
        Some(ref c) if !c.is_empty() => c,
        _ => return reply(400, "Missing Cookie parameter", v2),
    };

    match auth::validate_cookie(cookie).await {
        Ok(info) => {
            let account = crate::data::accounts::Account::new(
                cookie.clone(),
                info.name.clone(),
                info.user_id,
            );
            match state.accounts.add(account) {
                Ok(_) => reply(200, &format!("Imported {}", info.name), v2),
                Err(e) => reply(500, &format!("Failed to save: {}", e), v2),
            }
        }
        Err(e) => reply(400, &format!("Invalid cookie: {}", e), v2),
    }
}

async fn handle_get_cookie(
    Extension(state): Extension<AppState>,
    Query(params): Query<AccountQuery>,
    v2: bool,
) -> Response {
    if !state.settings.get_bool("WebServer", "AllowGetCookie") {
        return reply(401, "AllowGetCookie is disabled", v2);
    }

    if !check_password_required(&state, &params.password) {
        return reply(401, "Invalid password", v2);
    }

    let identifier = match params.account {
        Some(ref a) if !a.is_empty() => a,
        _ => return reply(400, "Missing Account parameter", v2),
    };

    let accounts = match state.accounts.get_all() {
        Ok(a) => a,
        Err(e) => return reply(500, &e, v2),
    };

    match find_account(&accounts, identifier) {
        Some(account) => reply(200, &account.security_token, v2),
        None => reply(404, "Account not found", v2),
    }
}

async fn handle_get_csrf_token(
    Extension(state): Extension<AppState>,
    Query(params): Query<AccountQuery>,
    v2: bool,
) -> Response {
    if !check_password(&state, &params.password) {
        return reply(401, "Invalid password", v2);
    }

    let identifier = match params.account {
        Some(ref a) if !a.is_empty() => a,
        _ => return reply(400, "Missing Account parameter", v2),
    };

    let accounts = match state.accounts.get_all() {
        Ok(a) => a,
        Err(e) => return reply(500, &e, v2),
    };

    let account = match find_account(&accounts, identifier) {
        Some(a) => a,
        None => return reply(404, "Account not found", v2),
    };

    match auth::get_csrf_token(&account.security_token).await {
        Ok(token) => reply(200, &token, v2),
        Err(e) => reply(400, &e, v2),
    }
}



/// The read-only endpoints, driven through the real router (middleware and the
/// v1/v2 wrappers included) with `tower::ServiceExt::oneshot`.
#[cfg(test)]
mod handlers_basic_tests {
    use super::server_helpers_tests::TestApp;
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server, mount_csrf};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    const PASSWORD: &str = "sup3rsecret";

    #[tokio::test]
    async fn running_answers_true_in_v1_and_an_envelope_in_v2() {
        let app = TestApp::new("running");

        assert_eq!(app.get("/Running").await, (200, "true".to_string()));

        let (status, body) = app.get("/v2/Running").await;
        assert_eq!(status, 200);
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["Success"], serde_json::json!(true));
        assert_eq!(
            json["Message"],
            serde_json::json!("Roblox Account Manager is running")
        );
    }

    #[tokio::test]
    async fn get_accounts_is_refused_while_the_feature_is_off() {
        let app = TestApp::new("accounts-off");
        app.deny("AllowGetAccounts").password(PASSWORD);

        assert_eq!(
            app.get("/GetAccounts").await,
            (401, "AllowGetAccounts is disabled".to_string())
        );

        let (status, body) = app.get("/v2/GetAccounts").await;
        assert_eq!(status, 401);
        assert!(body.contains("AllowGetAccounts is disabled"));
    }

    /// A wrong password is refused even when the flag that makes it mandatory
    /// is off.
    #[tokio::test]
    async fn get_accounts_refuses_a_wrong_password() {
        let app = TestApp::new("accounts-badpw");
        app.allow("AllowGetAccounts").password(PASSWORD);

        assert_eq!(
            app.get("/GetAccounts?Password=nope").await,
            (401, "Invalid password".to_string())
        );
    }

    /// With no password configured at all nothing is readable: `check_password`
    /// refuses anything shorter than 6 characters.
    #[tokio::test]
    async fn get_accounts_refuses_when_no_password_is_configured() {
        let app = TestApp::new("accounts-nopw");
        app.allow("AllowGetAccounts");

        assert_eq!(
            app.get("/GetAccounts").await,
            (401, "Invalid password".to_string())
        );
    }

    #[tokio::test]
    async fn get_accounts_lists_the_usernames() {
        let app = TestApp::new("accounts-list");
        app.allow("AllowGetAccounts").password(PASSWORD);
        app.add_account("alt_one", 111, "token-one");
        app.add_account("alt_two", 222, "token-two");

        assert_eq!(
            app.get("/GetAccounts").await,
            (200, "alt_one,alt_two".to_string())
        );
        assert_eq!(
            app.get(&format!("/GetAccounts?Password={}", PASSWORD)).await,
            (200, "alt_one,alt_two".to_string())
        );
    }

    #[tokio::test]
    async fn get_accounts_filters_by_group() {
        let app = TestApp::new("accounts-group");
        app.allow("AllowGetAccounts").password(PASSWORD);
        app.add_account("alt_one", 111, "token-one");
        app.add_account("alt_two", 222, "token-two");

        let mut bot = app.accounts.get_all().unwrap().pop().unwrap();
        bot.group = "Bots".to_string();
        app.accounts.update(bot).unwrap();

        assert_eq!(
            app.get("/GetAccounts?Group=Bots").await,
            (200, "alt_two".to_string())
        );
        assert_eq!(
            app.get("/GetAccounts?Group=Nope").await,
            (200, String::new())
        );
    }

    #[tokio::test]
    async fn get_accounts_json_returns_the_account_objects() {
        let app = TestApp::new("accounts-json");
        app.allow("AllowGetAccounts").password(PASSWORD);
        app.add_account("alt_one", 111, "token-one");

        let (status, body) = app.get("/GetAccountsJson").await;
        assert_eq!(status, 200);
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json[0]["Username"], serde_json::json!("alt_one"));
        assert_eq!(json[0]["UserID"], serde_json::json!(111));
        assert_eq!(json[0]["Group"], serde_json::json!("Default"));
        // The cookie is never included unless it was asked for and allowed.
        assert!(json[0].get("Cookie").is_none());
    }

    /// The cookie only comes out when the feature is on *and* the password was
    /// supplied, even though `EveryRequestRequiresPassword` is off.
    #[tokio::test]
    async fn get_accounts_json_gates_the_cookie() {
        let app = TestApp::new("accounts-json-cookie");
        app.allow("AllowGetAccounts").password(PASSWORD);
        app.add_account("alt_one", 111, "token-one");

        // Asked for, but `AllowGetCookie` is off.
        app.deny("AllowGetCookie");
        let (_, body) = app
            .get(&format!(
                "/GetAccountsJson?IncludeCookies=true&Password={}",
                PASSWORD
            ))
            .await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(json[0].get("Cookie").is_none());

        // Allowed, but no password supplied.
        app.allow("AllowGetCookie");
        let (_, body) = app.get("/GetAccountsJson?IncludeCookies=true").await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(json[0].get("Cookie").is_none());

        // Allowed and authenticated.
        let (_, body) = app
            .get(&format!(
                "/GetAccountsJson?IncludeCookies=true&Password={}",
                PASSWORD
            ))
            .await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json[0]["Cookie"], serde_json::json!("token-one"));
    }

    /// v2 wraps the same array in the `{Success, Message}` envelope.
    #[tokio::test]
    async fn get_accounts_json_v2_wraps_the_array() {
        let app = TestApp::new("accounts-json-v2");
        app.allow("AllowGetAccounts").password(PASSWORD);
        app.add_account("alt_one", 111, "token-one");

        let (status, body) = app.get("/v2/GetAccountsJson").await;
        assert_eq!(status, 200);
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["Success"], serde_json::json!(true));
        assert_eq!(json["Message"][0]["Username"], serde_json::json!("alt_one"));
    }

    #[tokio::test]
    async fn get_accounts_json_is_refused_while_the_feature_is_off() {
        let app = TestApp::new("accounts-json-off");
        app.deny("AllowGetAccounts").password(PASSWORD);

        assert_eq!(
            app.get("/GetAccountsJson").await,
            (401, "AllowGetAccounts is disabled".to_string())
        );
    }

    #[tokio::test]
    async fn get_cookie_is_refused_while_the_feature_is_off() {
        let app = TestApp::new("cookie-off");
        app.deny("AllowGetCookie").password(PASSWORD);

        assert_eq!(
            app.get("/GetCookie?Account=alt_one").await,
            (401, "AllowGetCookie is disabled".to_string())
        );
    }

    /// Reading a cookie always needs the password, unlike `GetAccounts`.
    #[tokio::test]
    async fn get_cookie_always_requires_the_password() {
        let app = TestApp::new("cookie-pw");
        app.allow("AllowGetCookie").password(PASSWORD);
        app.add_account("alt_one", 111, "token-one");

        assert_eq!(
            app.get("/GetCookie?Account=alt_one").await,
            (401, "Invalid password".to_string())
        );
        assert_eq!(
            app.get(&format!("/GetCookie?Account=alt_one&Password={}", PASSWORD))
                .await,
            (200, "token-one".to_string())
        );
    }

    #[tokio::test]
    async fn get_cookie_needs_an_account_and_finds_it_by_user_id() {
        let app = TestApp::new("cookie-lookup");
        app.allow("AllowGetCookie").password(PASSWORD);
        app.add_account("alt_one", 111, "token-one");

        assert_eq!(
            app.get(&format!("/GetCookie?Password={}", PASSWORD)).await,
            (400, "Missing Account parameter".to_string())
        );
        assert_eq!(
            app.get(&format!("/GetCookie?Account=&Password={}", PASSWORD))
                .await,
            (400, "Missing Account parameter".to_string())
        );
        assert_eq!(
            app.get(&format!("/GetCookie?Account=111&Password={}", PASSWORD))
                .await,
            (200, "token-one".to_string())
        );
        assert_eq!(
            app.get(&format!("/GetCookie?Account=nobody&Password={}", PASSWORD))
                .await,
            (404, "Account not found".to_string())
        );
    }

    #[tokio::test]
    async fn import_cookie_needs_a_password_and_a_cookie() {
        let app = TestApp::new("import-guards");
        app.password(PASSWORD);

        assert_eq!(
            app.get("/ImportCookie?Cookie=abc&Password=wrong").await,
            (401, "Invalid password".to_string())
        );
        assert_eq!(
            app.get(&format!("/ImportCookie?Password={}", PASSWORD)).await,
            (400, "Missing Cookie parameter".to_string())
        );
        assert_eq!(
            app.get(&format!("/ImportCookie?Cookie=&Password={}", PASSWORD))
                .await,
            (400, "Missing Cookie parameter".to_string())
        );
    }

    #[tokio::test]
    async fn import_cookie_rejects_a_cookie_roblox_refuses() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("www", "/my/account/json")))
            .and(header("cookie", cookie_of("ws-import-bad")))
            .respond_with(ResponseTemplate::new(401))
            .mount(server)
            .await;

        let app = TestApp::new("import-bad");
        app.password(PASSWORD);

        let (status, body) = app
            .get(&format!(
                "/ImportCookie?Cookie=ws-import-bad&Password={}",
                PASSWORD
            ))
            .await;
        assert_eq!(status, 400);
        assert!(body.starts_with("Invalid cookie: "), "body: {}", body);
        assert!(app.accounts.get_all().unwrap().is_empty());
    }

    #[tokio::test]
    async fn import_cookie_stores_the_validated_account() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("www", "/my/account/json")))
            .and(header("cookie", cookie_of("ws-import-ok")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "UserId": 4242,
                "Name": "imported_alt",
                "DisplayName": "Imported"
            })))
            .mount(server)
            .await;

        let app = TestApp::new("import-ok");
        app.password(PASSWORD);

        assert_eq!(
            app.get(&format!(
                "/ImportCookie?Cookie=ws-import-ok&Password={}",
                PASSWORD
            ))
            .await,
            (200, "Imported imported_alt".to_string())
        );

        let accounts = app.accounts.get_all().unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].user_id, 4242);
        assert_eq!(accounts[0].security_token, "ws-import-ok");
    }

    #[tokio::test]
    async fn get_csrf_token_needs_a_known_account() {
        let app = TestApp::new("csrf-lookup");
        app.password(PASSWORD);

        assert_eq!(
            app.get("/GetCSRFToken").await,
            (400, "Missing Account parameter".to_string())
        );
        assert_eq!(
            app.get("/GetCSRFToken?Account=nobody").await,
            (404, "Account not found".to_string())
        );
    }

    #[tokio::test]
    async fn get_csrf_token_returns_the_token_for_the_account() {
        mount_csrf("ws-csrf-ok", "csrf-ws-ok").await;

        let app = TestApp::new("csrf-ok");
        app.password(PASSWORD);
        app.add_account("alt_csrf", 555, "ws-csrf-ok");

        assert_eq!(
            app.get(&format!("/GetCSRFToken?Account=alt_csrf&Password={}", PASSWORD))
                .await,
            (200, "csrf-ws-ok".to_string())
        );
    }

    #[tokio::test]
    async fn get_csrf_token_reports_a_refused_handshake() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path(mock_path("auth", "/v1/authentication-ticket/")))
            .and(header("cookie", cookie_of("ws-csrf-bad")))
            .respond_with(ResponseTemplate::new(403).set_body_string("Token Validation Failed"))
            .mount(server)
            .await;

        let app = TestApp::new("csrf-bad");
        app.password(PASSWORD);
        app.add_account("alt_csrf_bad", 556, "ws-csrf-bad");

        let (status, body) = app
            .get(&format!("/GetCSRFToken?Account=alt_csrf_bad&Password={}", PASSWORD))
            .await;
        assert_eq!(status, 400);
        assert!(body.contains("403"), "body: {}", body);
    }

    /// The middleware runs in front of every route the fixture builds.
    #[tokio::test]
    async fn a_browser_issued_request_is_refused_before_the_handler() {
        let app = TestApp::new("basic-origin");
        app.allow("AllowGetAccounts").password(PASSWORD);

        let router = app.router();
        let request = Request::builder()
            .uri("/GetAccounts")
            .header("origin", "https://evil.example")
            .body(Body::empty())
            .unwrap();
        let response = tower::ServiceExt::oneshot(router, request).await.unwrap();
        assert_eq!(response.status().as_u16(), 403);
    }
}
