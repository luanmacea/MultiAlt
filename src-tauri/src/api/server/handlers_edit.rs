async fn handle_get_alias(
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

    let identifier = match params.account {
        Some(ref a) if !a.is_empty() => a,
        _ => return reply(400, "Missing Account parameter", v2),
    };

    let accounts = match state.accounts.get_all() {
        Ok(a) => a,
        Err(e) => return reply(500, &e, v2),
    };

    match find_account(&accounts, identifier) {
        Some(account) => reply(200, &account.alias, v2),
        None => reply(404, "Account not found", v2),
    }
}

async fn handle_get_description(
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

    let identifier = match params.account {
        Some(ref a) if !a.is_empty() => a,
        _ => return reply(400, "Missing Account parameter", v2),
    };

    let accounts = match state.accounts.get_all() {
        Ok(a) => a,
        Err(e) => return reply(500, &e, v2),
    };

    match find_account(&accounts, identifier) {
        Some(account) => reply(200, &account.description, v2),
        None => reply(404, "Account not found", v2),
    }
}

async fn handle_get_field(
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

    let identifier = match params.account {
        Some(ref a) if !a.is_empty() => a,
        _ => return reply(400, "Missing Account parameter", v2),
    };

    let field_name = match params.field {
        Some(ref f) if !f.is_empty() => f,
        _ => return reply(400, "Missing Field parameter", v2),
    };

    let accounts = match state.accounts.get_all() {
        Ok(a) => a,
        Err(e) => return reply(500, &e, v2),
    };

    match find_account(&accounts, identifier) {
        Some(account) => {
            let value = account.fields.get(field_name.as_str()).map(|v| v.as_str()).unwrap_or("");
            reply(200, value, v2)
        }
        None => reply(404, "Account not found", v2),
    }
}

async fn handle_set_field(
    Extension(state): Extension<AppState>,
    Query(params): Query<AccountQuery>,
    v2: bool,
) -> Response {
    if !check_password(&state, &params.password) {
        return reply(401, "Invalid password", v2);
    }

    if !state.settings.get_bool("WebServer", "AllowAccountEditing") {
        return reply(401, "AllowAccountEditing is disabled", v2);
    }

    let identifier = match params.account {
        Some(ref a) if !a.is_empty() => a,
        _ => return reply(400, "Missing Account parameter", v2),
    };

    let field_name = match params.field {
        Some(ref f) if !f.is_empty() => f.clone(),
        _ => return reply(400, "Missing Field parameter", v2),
    };

    let field_value = match params.value {
        Some(ref v) => v.clone(),
        None => return reply(400, "Missing Value parameter", v2),
    };

    let accounts = match state.accounts.get_all() {
        Ok(a) => a,
        Err(e) => return reply(500, &e, v2),
    };

    let mut account = match find_account(&accounts, identifier) {
        Some(a) => a,
        None => return reply(404, "Account not found", v2),
    };

    account.set_field(field_name, field_value);
    match state.accounts.update(account) {
        Ok(_) => reply(200, "Field set successfully", v2),
        Err(e) => reply(500, &e, v2),
    }
}

async fn handle_remove_field(
    Extension(state): Extension<AppState>,
    Query(params): Query<AccountQuery>,
    v2: bool,
) -> Response {
    if !check_password(&state, &params.password) {
        return reply(401, "Invalid password", v2);
    }

    if !state.settings.get_bool("WebServer", "AllowAccountEditing") {
        return reply(401, "AllowAccountEditing is disabled", v2);
    }

    let identifier = match params.account {
        Some(ref a) if !a.is_empty() => a,
        _ => return reply(400, "Missing Account parameter", v2),
    };

    let field_name = match params.field {
        Some(ref f) if !f.is_empty() => f,
        _ => return reply(400, "Missing Field parameter", v2),
    };

    let accounts = match state.accounts.get_all() {
        Ok(a) => a,
        Err(e) => return reply(500, &e, v2),
    };

    let mut account = match find_account(&accounts, identifier) {
        Some(a) => a,
        None => return reply(404, "Account not found", v2),
    };

    account.remove_field(field_name);
    match state.accounts.update(account) {
        Ok(_) => reply(200, "Field removed successfully", v2),
        Err(e) => reply(500, &e, v2),
    }
}

async fn handle_set_alias(
    Extension(state): Extension<AppState>,
    Query(params): Query<AccountQuery>,
    body: String,
    v2: bool,
) -> Response {
    if !check_password(&state, &params.password) {
        return reply(401, "Invalid password", v2);
    }

    if !state.settings.get_bool("WebServer", "AllowAccountEditing") {
        return reply(401, "AllowAccountEditing is disabled", v2);
    }

    let identifier = match params.account {
        Some(ref a) if !a.is_empty() => a,
        _ => return reply(400, "Missing Account parameter", v2),
    };

    if body.is_empty() {
        return reply(400, "Missing body", v2);
    }

    let accounts = match state.accounts.get_all() {
        Ok(a) => a,
        Err(e) => return reply(500, &e, v2),
    };

    let mut account = match find_account(&accounts, identifier) {
        Some(a) => a,
        None => return reply(404, "Account not found", v2),
    };

    account.alias = body;
    match state.accounts.update(account) {
        Ok(_) => reply(200, "Alias set successfully", v2),
        Err(e) => reply(500, &e, v2),
    }
}

async fn handle_set_description(
    Extension(state): Extension<AppState>,
    Query(params): Query<AccountQuery>,
    body: String,
    v2: bool,
) -> Response {
    if !check_password(&state, &params.password) {
        return reply(401, "Invalid password", v2);
    }

    if !state.settings.get_bool("WebServer", "AllowAccountEditing") {
        return reply(401, "AllowAccountEditing is disabled", v2);
    }

    let identifier = match params.account {
        Some(ref a) if !a.is_empty() => a,
        _ => return reply(400, "Missing Account parameter", v2),
    };

    if body.is_empty() {
        return reply(400, "Missing body", v2);
    }

    let accounts = match state.accounts.get_all() {
        Ok(a) => a,
        Err(e) => return reply(500, &e, v2),
    };

    let mut account = match find_account(&accounts, identifier) {
        Some(a) => a,
        None => return reply(404, "Account not found", v2),
    };

    account.description = body;
    match state.accounts.update(account) {
        Ok(_) => reply(200, "Description set successfully", v2),
        Err(e) => reply(500, &e, v2),
    }
}

async fn handle_append_description(
    Extension(state): Extension<AppState>,
    Query(params): Query<AccountQuery>,
    body: String,
    v2: bool,
) -> Response {
    if !check_password(&state, &params.password) {
        return reply(401, "Invalid password", v2);
    }

    if !state.settings.get_bool("WebServer", "AllowAccountEditing") {
        return reply(401, "AllowAccountEditing is disabled", v2);
    }

    let identifier = match params.account {
        Some(ref a) if !a.is_empty() => a,
        _ => return reply(400, "Missing Account parameter", v2),
    };

    if body.is_empty() {
        return reply(400, "Missing body", v2);
    }

    let accounts = match state.accounts.get_all() {
        Ok(a) => a,
        Err(e) => return reply(500, &e, v2),
    };

    let mut account = match find_account(&accounts, identifier) {
        Some(a) => a,
        None => return reply(404, "Account not found", v2),
    };

    account.description.push_str(&body);
    match state.accounts.update(account) {
        Ok(_) => reply(200, "Description appended successfully", v2),
        Err(e) => reply(500, &e, v2),
    }
}

async fn handle_set_avatar(
    Extension(state): Extension<AppState>,
    Query(params): Query<AccountQuery>,
    body: String,
    v2: bool,
) -> Response {
    if !check_password(&state, &params.password) {
        return reply(401, "Invalid password", v2);
    }

    if !state.settings.get_bool("WebServer", "AllowAccountEditing") {
        return reply(401, "AllowAccountEditing is disabled", v2);
    }

    let identifier = match params.account {
        Some(ref a) if !a.is_empty() => a,
        _ => return reply(400, "Missing Account parameter", v2),
    };

    let avatar_json: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return reply(400, "Invalid JSON body", v2),
    };

    let accounts = match state.accounts.get_all() {
        Ok(a) => a,
        Err(e) => return reply(500, &e, v2),
    };

    let account = match find_account(&accounts, identifier) {
        Some(a) => a,
        None => return reply(404, "Account not found", v2),
    };

    match roblox::set_avatar(&account.security_token, avatar_json).await {
        Ok(_) => reply(200, "Avatar set successfully", v2),
        Err(e) => reply(400, &e, v2),
    }
}

async fn handle_block_user(
    Extension(state): Extension<AppState>,
    Query(params): Query<AccountQuery>,
    v2: bool,
) -> Response {
    if !check_password(&state, &params.password) {
        return reply(401, "Invalid password", v2);
    }

    if !state.settings.get_bool("WebServer", "AllowAccountEditing") {
        return reply(401, "AllowAccountEditing is disabled", v2);
    }

    let identifier = match params.account {
        Some(ref a) if !a.is_empty() => a,
        _ => return reply(400, "Missing Account parameter", v2),
    };

    let target_user_id: i64 = match params.user_id.as_deref().and_then(|v| v.parse().ok()) {
        Some(id) => id,
        None => return reply(400, "Missing or invalid UserId parameter", v2),
    };

    let accounts = match state.accounts.get_all() {
        Ok(a) => a,
        Err(e) => return reply(500, &e, v2),
    };

    let account = match find_account(&accounts, identifier) {
        Some(a) => a,
        None => return reply(404, "Account not found", v2),
    };

    match roblox::block_user(&account.security_token, target_user_id).await {
        Ok(_) => reply(200, "User blocked successfully", v2),
        Err(e) => reply(500, &e, v2),
    }
}

async fn handle_unblock_user(
    Extension(state): Extension<AppState>,
    Query(params): Query<AccountQuery>,
    v2: bool,
) -> Response {
    if !check_password(&state, &params.password) {
        return reply(401, "Invalid password", v2);
    }

    if !state.settings.get_bool("WebServer", "AllowAccountEditing") {
        return reply(401, "AllowAccountEditing is disabled", v2);
    }

    let identifier = match params.account {
        Some(ref a) if !a.is_empty() => a,
        _ => return reply(400, "Missing Account parameter", v2),
    };

    let target_user_id: i64 = match params.user_id.as_deref().and_then(|v| v.parse().ok()) {
        Some(id) => id,
        None => return reply(400, "Missing or invalid UserId parameter", v2),
    };

    let accounts = match state.accounts.get_all() {
        Ok(a) => a,
        Err(e) => return reply(500, &e, v2),
    };

    let account = match find_account(&accounts, identifier) {
        Some(a) => a,
        None => return reply(404, "Account not found", v2),
    };

    match roblox::unblock_user(&account.security_token, target_user_id).await {
        Ok(_) => reply(200, "User unblocked successfully", v2),
        Err(e) => reply(500, &e, v2),
    }
}

async fn handle_get_blocked_list(
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

    match roblox::get_blocked_users(&account.security_token).await {
        Ok(blocked) => {
            let body = serde_json::to_string(&blocked).unwrap_or_else(|_| "[]".to_string());
            if v2 {
                let wrapper = serde_json::json!({
                    "Success": true,
                    "Message": blocked,
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
        Err(e) => reply(500, &e, v2),
    }
}

async fn handle_unblock_everyone(
    Extension(state): Extension<AppState>,
    Query(params): Query<AccountQuery>,
    v2: bool,
) -> Response {
    if !check_password(&state, &params.password) {
        return reply(401, "Invalid password", v2);
    }

    if !state.settings.get_bool("WebServer", "AllowAccountEditing") {
        return reply(401, "AllowAccountEditing is disabled", v2);
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

    match roblox::unblock_all_users(&account.security_token).await {
        Ok(count) => reply(200, &format!("Unblocked {} users", count), v2),
        Err(e) => reply(500, &e, v2),
    }
}

/// Strict variant for secrets (cookies): the configured password must be sent,
/// even when EveryRequestRequiresPassword is off.
fn check_password_required(state: &AppState, password: &Option<String>) -> bool {
    let ws_password = state.settings.get_string("WebServer", "Password");
    ws_password.len() >= 6 && matches!(password, Some(p) if *p == ws_password)
}

fn check_password(state: &AppState, password: &Option<String>) -> bool {
    let ws_password = state.settings.get_string("WebServer", "Password");
    let every_request_requires_password = state
        .settings
        .get_bool("WebServer", "EveryRequestRequiresPassword");

    if ws_password.len() < 6 {
        return false;
    }

    if every_request_requires_password {
        return matches!(password, Some(p) if *p == ws_password);
    }

    match password {
        Some(p) => *p == ws_password,
        None => true,
    }
}


#[cfg(test)]
mod password_tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_path(name: &str, ext: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("ram-wspw-{name}-{nanos}.{ext}"))
    }

    /// `AppState` holds `&'static` stores, so the test stores are leaked.
    /// Returns the state plus the two temp files to clean up.
    fn test_state(name: &str, password: &str, every_request: bool) -> (AppState, Vec<PathBuf>) {
        let settings_path = unique_path(name, "ini");
        let accounts_path = unique_path(name, "json");

        let settings: &'static SettingsStore =
            Box::leak(Box::new(SettingsStore::new(settings_path.clone())));
        settings.set("WebServer", "Password", password).ok();
        settings
            .set(
                "WebServer",
                "EveryRequestRequiresPassword",
                if every_request { "true" } else { "false" },
            )
            .unwrap();

        let accounts: &'static AccountStore =
            Box::leak(Box::new(AccountStore::new(accounts_path.clone())));

        (
            AppState { accounts, settings },
            vec![settings_path, accounts_path],
        )
    }

    fn cleanup(paths: Vec<PathBuf>) {
        for path in paths {
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn a_password_shorter_than_six_characters_never_authenticates() {
        let (state, paths) = test_state("short", "12345", false);
        assert!(!check_password(&state, &Some("12345".to_string())));
        assert!(!check_password(&state, &None));
        assert!(!check_password_required(&state, &Some("12345".to_string())));
        assert!(!check_password_required(&state, &None));
        cleanup(paths);
    }

    #[test]
    fn an_unset_password_never_authenticates() {
        let (state, paths) = test_state("unset", "", false);
        assert_eq!(state.settings.get_string("WebServer", "Password"), "");
        assert!(!check_password(&state, &None));
        assert!(!check_password(&state, &Some(String::new())));
        assert!(!check_password_required(&state, &None));
        cleanup(paths);
    }

    #[test]
    fn check_password_lets_a_missing_password_through_when_the_flag_is_off() {
        let (state, paths) = test_state("optional", "sup3rsecret", false);
        assert!(check_password(&state, &None));
        assert!(check_password(&state, &Some("sup3rsecret".to_string())));
        // A wrong password is still rejected even when it is optional.
        assert!(!check_password(&state, &Some("wrongpass".to_string())));
        cleanup(paths);
    }

    #[test]
    fn check_password_required_never_lets_a_missing_password_through() {
        let (state, paths) = test_state("required-off", "sup3rsecret", false);
        // Same settings as the test above, where check_password accepts None.
        assert!(check_password(&state, &None));
        assert!(!check_password_required(&state, &None));
        assert!(!check_password_required(&state, &Some(String::new())));
        assert!(!check_password_required(&state, &Some("wrongpass".to_string())));
        assert!(check_password_required(&state, &Some("sup3rsecret".to_string())));
        cleanup(paths);
    }

    #[test]
    fn an_exact_match_is_required_when_every_request_requires_a_password() {
        let (state, paths) = test_state("required-on", "sup3rsecret", true);
        assert!(!check_password(&state, &None));
        assert!(!check_password(&state, &Some("sup3rsecre".to_string())));
        assert!(!check_password(&state, &Some("SUP3RSECRET".to_string())));
        assert!(check_password(&state, &Some("sup3rsecret".to_string())));

        assert!(!check_password_required(&state, &None));
        assert!(check_password_required(&state, &Some("sup3rsecret".to_string())));
        cleanup(paths);
    }
}

/// `Allow Account Editing` existe para que o web server, mesmo com a senha certa,
/// não possa editar a conta do Roblox sem o usuário ter liberado. Quatro rotas
/// que editam a conta ficavam de fora do gate: `/SetAvatar`, `/BlockUser`,
/// `/UnblockUser` e `/UnblockEveryone` — bastava a senha.
#[cfg(test)]
mod edit_permission_tests {
    use super::*;
    use axum::http::Uri;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    const PASSWORD: &str = "senha-boa-123";

    fn unique_path(name: &str, ext: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("ram-wsedit-{name}-{nanos}.{ext}"))
    }

    fn test_state(name: &str, allow_editing: bool) -> (AppState, Vec<PathBuf>) {
        let settings_path = unique_path(name, "ini");
        let accounts_path = unique_path(name, "json");
        let settings: &'static SettingsStore =
            Box::leak(Box::new(SettingsStore::new(settings_path.clone())));
        settings.set("WebServer", "Password", PASSWORD).ok();
        settings
            .set(
                "WebServer",
                "AllowAccountEditing",
                if allow_editing { "true" } else { "false" },
            )
            .unwrap();
        let accounts: &'static AccountStore =
            Box::leak(Box::new(AccountStore::new(accounts_path.clone())));
        (
            AppState { accounts, settings },
            vec![settings_path, accounts_path],
        )
    }

    fn cleanup(paths: Vec<PathBuf>) {
        for path in paths {
            let _ = std::fs::remove_file(path);
        }
    }

    /// Query igual à que o axum monta a partir da URL de verdade.
    fn query(extra: &str) -> Query<AccountQuery> {
        let uri: Uri = format!("http://127.0.0.1/Endpoint?Password={PASSWORD}&{extra}")
            .parse()
            .unwrap();
        Query::<AccountQuery>::try_from_uri(&uri).expect("query")
    }

    async fn body_of(response: Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        String::from_utf8_lossy(&bytes).to_string()
    }

    #[tokio::test]
    async fn set_avatar_requires_the_editing_permission() {
        let (state, paths) = test_state("avatar-off", false);
        let response = handle_set_avatar(
            Extension(state),
            query("Account=ann"),
            "{\"assets\":[]}".to_string(),
            false,
        )
        .await;
        assert_eq!(response.status(), 401);
        assert!(body_of(response).await.contains("AllowAccountEditing"));
        cleanup(paths);
    }

    #[tokio::test]
    async fn block_user_requires_the_editing_permission() {
        let (state, paths) = test_state("block-off", false);
        let response = handle_block_user(Extension(state), query("Account=ann&UserId=7"), false).await;
        assert_eq!(response.status(), 401);
        assert!(body_of(response).await.contains("AllowAccountEditing"));
        cleanup(paths);
    }

    #[tokio::test]
    async fn unblock_user_requires_the_editing_permission() {
        let (state, paths) = test_state("unblock-off", false);
        let response =
            handle_unblock_user(Extension(state), query("Account=ann&UserId=7"), false).await;
        assert_eq!(response.status(), 401);
        assert!(body_of(response).await.contains("AllowAccountEditing"));
        cleanup(paths);
    }

    #[tokio::test]
    async fn unblock_everyone_requires_the_editing_permission() {
        let (state, paths) = test_state("unblockall-off", false);
        let response = handle_unblock_everyone(Extension(state), query("Account=ann"), false).await;
        assert_eq!(response.status(), 401);
        assert!(body_of(response).await.contains("AllowAccountEditing"));
        cleanup(paths);
    }

    /// Com a permissão ligada o gate sai da frente: a rota segue o seu caminho
    /// normal (aqui, reclamar da conta que não existe) em vez de 401 de permissão.
    #[tokio::test]
    async fn the_gate_gets_out_of_the_way_once_editing_is_allowed() {
        let (state, paths) = test_state("avatar-on", true);
        let response = handle_set_avatar(
            Extension(state),
            query("Account=ann"),
            "{\"assets\":[]}".to_string(),
            false,
        )
        .await;
        assert_ne!(response.status(), 401);
        assert!(!body_of(response).await.contains("AllowAccountEditing"));
        cleanup(paths);
    }
}
