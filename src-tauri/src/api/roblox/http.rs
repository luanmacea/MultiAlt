fn cookie_header(security_token: &str) -> String {
    format!(".ROBLOSECURITY={}", security_token)
}

/// Cliente dos endpoints de join (`gamejoin`, PlaceLauncher, private/VIP).
/// O teto vem de [`http_client::builder`]: é por aqui que passa a resolução de
/// private join, que fica no caminho de launch.
fn game_join_client() -> reqwest::Client {
    http_client::builder()
        .user_agent("Roblox/WinInet")
        .build()
        .unwrap()
}

/// Cliente que para no 3xx (resolução de share link e refresh de sessão),
/// também com o teto de [`http_client::builder`].
fn no_redirect_client() -> reqwest::Client {
    http_client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

async fn send_with_retry<F>(mut make_request: F) -> Result<reqwest::Response, String>
where
    F: FnMut() -> reqwest::RequestBuilder,
{
    let mut last_error = String::new();

    for attempt in 0..3 {
        // O `.ROBLOSECURITY` novo que a resposta trouxer fica registrado para
        // a conta que mandou o pedido (`cookie_rotation`).
        match crate::api::cookie_rotation::send(make_request()).await {
            Ok(response) => {
                if response.status().as_u16() == 429 && attempt < 2 {
                    let delay = Duration::from_millis(400 * 2_u64.pow(attempt as u32));
                    sleep(delay).await;
                    continue;
                }
                return Ok(response);
            }
            Err(e) => {
                // Já vem como frase: um teto estourado tem de chegar ao usuário
                // dizendo o que aconteceu, não como texto cru do reqwest.
                last_error = http_client::describe_error(&e);
                if attempt < 2 {
                    let delay = Duration::from_millis(400 * 2_u64.pow(attempt as u32));
                    sleep(delay).await;
                    continue;
                }
            }
        }
    }

    Err(last_error)
}
