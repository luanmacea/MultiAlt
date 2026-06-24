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

