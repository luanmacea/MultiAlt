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

#[cfg(test)]
mod http_retry_tests {
    use super::*;
    use crate::api::endpoints::test_support::{mock_path, mock_server};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};

    /// 429 on the first attempt, 200 on the retry: the helper must swallow the
    /// throttle and hand back the successful response.
    ///
    /// Mocks are matched in registration order, so the throttled one (capped at
    /// a single match) goes first. Do not give mocks a custom priority here:
    /// wiremock re-sorts its mock vector on every request and a `MockGuard`
    /// holds a plain index into it, so reordering breaks scoped verification.
    #[tokio::test]
    async fn retries_once_after_a_429() {
        let server = mock_server().await;
        let url = format!("{}/test/retry-once", endpoints::host("apis"));
        let route = mock_path("apis", "/test/retry-once");

        let throttled = Mock::given(method("GET"))
            .and(path(route.clone()))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(1)
            .expect(1)
            .named("first attempt is throttled")
            .mount_as_scoped(server)
            .await;

        let ok = Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_string("done"))
            .expect(1)
            .named("retry succeeds")
            .mount_as_scoped(server)
            .await;

        let client = reqwest::Client::new();
        let response = send_with_retry(|| client.get(&url)).await.expect("response");
        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(response.text().await.unwrap(), "done");

        // Dropping the guards verifies the two `expect(1)` counts above.
        drop(throttled);
        drop(ok);
    }

    /// The retry budget is bounded at 3 attempts. Note the helper only returns
    /// `Err` for transport failures: after the last 429 it returns the throttled
    /// response as-is, and the caller turns it into an error (see
    /// `thumbnail_http_tests::headshots_fail_after_the_retry_budget`).
    #[tokio::test]
    async fn stops_after_three_attempts_when_always_throttled() {
        let server = mock_server().await;
        let url = format!("{}/test/retry-exhausted", endpoints::host("apis"));

        let throttled = Mock::given(method("GET"))
            .and(path(mock_path("apis", "/test/retry-exhausted")))
            .respond_with(ResponseTemplate::new(429))
            .expect(3)
            .named("exactly three attempts")
            .mount_as_scoped(server)
            .await;

        let client = reqwest::Client::new();
        let response = send_with_retry(|| client.get(&url)).await.expect("response");
        assert_eq!(response.status().as_u16(), 429);

        drop(throttled);
    }
}

/// The client builders and the transport-failure branch of `send_with_retry`,
/// which `http_retry_tests` above does not reach.
#[cfg(test)]
mod http_client_tests {
    use super::*;
    use crate::api::endpoints::test_support::{mock_path, mock_server};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    #[test]
    fn the_cookie_header_is_the_roblosecurity_pair() {
        assert_eq!(cookie_header("tok"), ".ROBLOSECURITY=tok");
        assert_eq!(cookie_header(""), ".ROBLOSECURITY=");
    }

    /// Roblox only accepts the game-join endpoints from its own client, so the
    /// join client must keep sending the `Roblox/WinInet` user agent.
    #[tokio::test]
    async fn the_game_join_client_sends_the_roblox_user_agent() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("gamejoin", "/test/user-agent")))
            .and(header("user-agent", "Roblox/WinInet"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ua-ok"))
            .mount(server)
            .await;

        let response = game_join_client()
            .get(format!("{}/test/user-agent", endpoints::host("gamejoin")))
            .send()
            .await
            .expect("response");
        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(response.text().await.unwrap(), "ua-ok");
    }

    /// The no-redirect client is what lets short-link resolution and the
    /// session refresh read a `Location`/`Set-Cookie` off a 3xx instead of
    /// silently following it.
    #[tokio::test]
    async fn the_no_redirect_client_stops_on_a_302() {
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("www", "/test/redirect-source")))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", "https://www.roblox.com/games/1"),
            )
            .mount(server)
            .await;

        let response = no_redirect_client()
            .get(format!("{}/test/redirect-source", endpoints::host("www")))
            .send()
            .await
            .expect("response");
        assert_eq!(response.status().as_u16(), 302);
        assert_eq!(
            response
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok()),
            Some("https://www.roblox.com/games/1")
        );
    }

    /// A transport error (nothing listening) is retried too, and the last
    /// error is reported once the budget runs out.
    #[tokio::test]
    async fn a_transport_failure_is_reported_after_the_retries() {
        let client = reqwest::Client::new();
        // Port 1 is never a Roblox mock: the connection is refused immediately.
        let err = send_with_retry(|| client.get("http://127.0.0.1:1/test/unreachable"))
            .await
            .unwrap_err();
        assert!(
            err.starts_with("Request failed: "),
            "unexpected error: {}",
            err
        );
    }
}
