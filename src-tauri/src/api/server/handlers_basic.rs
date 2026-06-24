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
        && check_password(&state, &params.password);

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

