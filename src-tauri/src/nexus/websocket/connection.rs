async fn handle_connection(
    stream: tokio::net::TcpStream,
    app: tauri::AppHandle,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut uri_path = String::new();

    let ws_stream = match tokio_tungstenite::accept_hdr_async(
        stream,
        |req: &tokio_tungstenite::tungstenite::handshake::server::Request,
         res: tokio_tungstenite::tungstenite::handshake::server::Response| {
            // Web pages can open ws://127.0.0.1 too; browsers always send an
            // Origin header, Roblox executors connecting from Lua don't.
            if req.headers().contains_key("origin") {
                let reject = tokio_tungstenite::tungstenite::http::Response::builder()
                    .status(403)
                    .body(Some("Connections from web pages are not allowed".to_string()))
                    .unwrap_or_default();
                return Err(reject);
            }
            uri_path = req.uri().to_string();
            Ok(res)
        },
    )
    .await
    {
        Ok(ws) => ws,
        Err(_) => return,
    };

    let params = parse_query_params(&uri_path);

    let name = match params.get("name") {
        Some(n) if !n.is_empty() => n.clone(),
        _ => return,
    };

    let id_str = match params.get("id") {
        Some(i) if !i.is_empty() => i.clone(),
        _ => return,
    };

    if id_str.parse::<i64>().is_err() {
        return;
    }

    let job_id = params
        .get("jobId")
        .cloned()
        .unwrap_or_else(|| "UNKNOWN".to_string());

    let server = nexus();

    {
        let accounts = server.accounts.lock().unwrap();
        if !accounts.iter().any(|a| a.username == name) {
            return;
        }
    }

    {
        let mut accounts = server.accounts.lock().unwrap();
        if let Some(account) = accounts.iter_mut().find(|a| a.username == name) {
            account.status = AccountStatus::Online;
            account.last_ping = Some(Instant::now());
            account.in_game_job_id = job_id;
            account.client_can_receive = false;
        }
    }

    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    // Kept to recognise *this* connection on cleanup (see below).
    let own_sender = tx.clone();

    {
        let mut conns = server.connections.lock().unwrap();
        conns.insert(
            name.clone(),
            NexusConnection {
                sender: tx,
                username: name.clone(),
            },
        );
    }

    let _ = app.emit(
        "nexus-account-connected",
        serde_json::json!({ "username": &name }),
    );

    let auto_exec = {
        let accounts = server.accounts.lock().unwrap();
        accounts
            .iter()
            .find(|a| a.username == name)
            .map(|a| a.auto_execute.clone())
            .unwrap_or_default()
    };

    if !auto_exec.is_empty() {
        let auto_exec_msg = format!("execute {}", auto_exec);
        let name_clone = name.clone();
        let tx_clone = {
            let conns = server.connections.lock().unwrap();
            conns.get(&name_clone).map(|c| c.sender.clone())
        };
        if let Some(sender) = tx_clone {
            tokio::spawn(async move {
                let deadline = Instant::now() + Duration::from_secs(60);
                loop {
                    if sender.is_closed() || Instant::now() >= deadline {
                        break;
                    }
                    let ready = {
                        let accounts = nexus().accounts.lock().unwrap();
                        accounts
                            .iter()
                            .find(|a| a.username == name_clone)
                            .map(|a| a.client_can_receive)
                            .unwrap_or(false)
                    };
                    if ready {
                        let _ = sender.send(auto_exec_msg);
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(80)).await;
                }
            });
        }
    }

    let (mut ws_write, mut ws_read) = ws_stream.split();

    let username_clone = name.clone();
    let app_clone = app.clone();

    loop {
        tokio::select! {
            msg = ws_read.next() => {
                match msg {
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                        server.handle_message(&username_clone, &text, Some(&app_clone));
                    }
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
            outgoing = rx.recv() => {
                match outgoing {
                    Some(msg) => {
                        if ws_write.send(tokio_tungstenite::tungstenite::Message::Text(msg.into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
        }
    }

    // On a rejoin the new socket registers under the same name before the old
    // one closes; only clean up if the entry is still ours.
    let still_current = {
        let mut conns = server.connections.lock().unwrap();
        let ours = conns
            .get(&name)
            .map(|c| c.sender.same_channel(&own_sender))
            .unwrap_or(false);
        if ours {
            conns.remove(&name);
        }
        ours
    };
    if !still_current {
        return;
    }

    {
        let mut accounts = server.accounts.lock().unwrap();
        if let Some(account) = accounts.iter_mut().find(|a| a.username == name) {
            account.status = AccountStatus::Offline;
            account.client_can_receive = false;
        }
    }

    let _ = app.emit(
        "nexus-account-disconnected",
        serde_json::json!({ "username": &name }),
    );
}

fn parse_query_params(uri: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    if let Some(query) = uri.split('?').nth(1) {
        for pair in query.split('&') {
            let mut parts = pair.splitn(2, '=');
            if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
                params.insert(
                    urlencoding::decode(key).unwrap_or_default().into_owned(),
                    urlencoding::decode(value).unwrap_or_default().into_owned(),
                );
            }
        }
    }
    params
}

#[cfg(test)]
mod nexus_query_tests {
    use super::*;

    #[test]
    fn parse_query_params_reads_simple_pairs() {
        let params = parse_query_params("/?name=Player1&id=42");
        assert_eq!(params.get("name").map(String::as_str), Some("Player1"));
        assert_eq!(params.get("id").map(String::as_str), Some("42"));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn parse_query_params_decodes_percent_encoding_in_keys_and_values() {
        let params = parse_query_params("/?display%20name=Player%20One&path=a%2Fb");
        assert_eq!(
            params.get("display name").map(String::as_str),
            Some("Player One")
        );
        assert_eq!(params.get("path").map(String::as_str), Some("a/b"));
    }

    #[test]
    fn parse_query_params_ignores_pairs_without_an_equals_sign() {
        let params = parse_query_params("/?flag&name=Player1&another");
        assert_eq!(params.len(), 1);
        assert_eq!(params.get("name").map(String::as_str), Some("Player1"));
        assert!(!params.contains_key("flag"));
    }

    #[test]
    fn parse_query_params_returns_empty_without_a_query_string() {
        assert!(parse_query_params("/").is_empty());
        assert!(parse_query_params("").is_empty());
        assert!(parse_query_params("/?").is_empty());
    }

    #[test]
    fn parse_query_params_keeps_everything_after_the_first_equals_sign() {
        let params = parse_query_params("/?token=abc=def");
        assert_eq!(params.get("token").map(String::as_str), Some("abc=def"));
    }

    #[test]
    fn parse_query_params_keeps_the_last_value_of_a_repeated_key() {
        let params = parse_query_params("/?id=1&id=2");
        assert_eq!(params.get("id").map(String::as_str), Some("2"));
    }

    #[test]
    fn parse_query_params_only_looks_at_the_first_question_mark_segment() {
        let params = parse_query_params("ws://127.0.0.1:5242/socket?id=7");
        assert_eq!(params.get("id").map(String::as_str), Some("7"));
    }
}
