fn build_router(state: AppState) -> Router {
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
        .route("/LaunchAccount", get(v1_launch_account))
        .route("/v2/LaunchAccount", get(v2_launch_account))
        .route("/FollowUser", get(v1_follow_user))
        .route("/v2/FollowUser", get(v2_follow_user))
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
        .layer(Extension(state))
}

pub async fn start(accounts: &'static AccountStore, settings: &'static SettingsStore) -> Result<u16, String> {
    {
        let guard = SERVER_STATE.lock().map_err(|e| e.to_string())?;
        if guard.is_some() {
            return Err("Web server is already running".into());
        }
    }

    let port = settings.get_int("WebServer", "WebServerPort").unwrap_or(7963) as u16;
    let allow_external = settings.get_bool("WebServer", "AllowExternalConnections");

    let bind_addr: SocketAddr = if allow_external {
        ([0, 0, 0, 0], port).into()
    } else {
        ([127, 0, 0, 1], port).into()
    };

    let state = AppState {
        accounts,
        settings,
    };

    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .map_err(|e| format!("Failed to bind to {}: {}", bind_addr, e))?;

    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            while !*shutdown_rx.borrow_and_update() {
                if shutdown_rx.changed().await.is_err() {
                    break;
                }
            }
        })
        .await
        .ok();
    });

    let mut guard = SERVER_STATE.lock().map_err(|e| e.to_string())?;
    *guard = Some(ServerHandle { shutdown_tx, port });

    Ok(port)
}

pub fn stop() -> Result<(), String> {
    let mut guard = SERVER_STATE.lock().map_err(|e| e.to_string())?;
    match guard.take() {
        Some(handle) => {
            let _ = handle.shutdown_tx.send(true);
            Ok(())
        }
        None => Err("Web server is not running".into()),
    }
}

pub fn is_running() -> bool {
    SERVER_STATE
        .lock()
        .map(|g| g.is_some())
        .unwrap_or(false)
}

pub fn get_port() -> u16 {
    SERVER_STATE
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|h| h.port))
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize)]
pub struct WebServerStatus {
    pub running: bool,
    pub port: u16,
}
