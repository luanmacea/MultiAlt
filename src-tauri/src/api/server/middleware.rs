async fn external_check(
    Extension(state): Extension<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();
    let is_v2 = path.starts_with("/v2/");
    let is_running = path.eq_ignore_ascii_case("/Running") || path.eq_ignore_ascii_case("/v2/Running");

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

