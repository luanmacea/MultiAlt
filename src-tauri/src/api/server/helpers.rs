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

