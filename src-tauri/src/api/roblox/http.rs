fn cookie_header(security_token: &str) -> String {
    format!(".ROBLOSECURITY={}", security_token)
}

fn game_join_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("Roblox/WinInet")
        .build()
        .unwrap()
}

fn no_redirect_client() -> reqwest::Client {
    reqwest::Client::builder()
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
        match make_request().send().await {
            Ok(response) => {
                if response.status().as_u16() == 429 && attempt < 2 {
                    let delay = Duration::from_millis(400 * 2_u64.pow(attempt as u32));
                    sleep(delay).await;
                    continue;
                }
                return Ok(response);
            }
            Err(e) => {
                last_error = e.to_string();
                if attempt < 2 {
                    let delay = Duration::from_millis(400 * 2_u64.pow(attempt as u32));
                    sleep(delay).await;
                    continue;
                }
            }
        }
    }

    Err(format!("Request failed: {}", last_error))
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
