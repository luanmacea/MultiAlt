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
                loop {
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

    {
        let mut conns = server.connections.lock().unwrap();
        conns.remove(&name);
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
