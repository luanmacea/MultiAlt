//! Single place where every `*.roblox.com` base URL is built.
//!
//! Production behaviour is unchanged: `host("auth")` returns
//! `https://auth.roblox.com`, so every call site produces byte-identical URLs.
//!
//! Tests call [`set_base_for_tests`] once to point *every* subdomain at one
//! local mock server. In that mode the subdomain becomes a path prefix
//! (`http://127.0.0.1:PORT/auth/...`) so two subdomains can never collide on
//! the same path while sharing a single process-wide server.
//!
//! Hosts that are not `<sub>.roblox.com` (the `clientsettingscdn`, `setup-aws`
//! and `ro.blox.com` CDNs used by the versions installer) are deliberately not
//! covered here and stay hardcoded at their call sites.

use std::sync::OnceLock;

/// Overrides the `https://<sub>.roblox.com` scheme+authority. Only ever set by
/// tests; `None` in production.
static BASE: OnceLock<String> = OnceLock::new();

/// Base URL (scheme + authority, no trailing slash) for a Roblox subdomain.
///
/// `sub` is the subdomain label: `"auth"`, `"apis"`, `"users"`, `"www"`, ...
pub fn host(sub: &str) -> String {
    format_host(BASE.get().map(|s| s.as_str()), sub)
}

/// Pure half of [`host`]: the URL for `sub` given the current override.
///
/// Split out so the production formatting (`base == None`) stays testable —
/// `BASE` is process-wide and every test binary sets it to the mock server.
fn format_host(base: Option<&str>, sub: &str) -> String {
    match base {
        Some(base) => format!("{}/{}", base, sub),
        None => format!("https://{}.roblox.com", sub),
    }
}

/// Points every subdomain at `url` (e.g. a wiremock server's URI) for the rest
/// of the process. The first call wins; later calls are ignored, which is what
/// lets every test share one server.
#[cfg(test)]
pub fn set_base_for_tests(url: &str) {
    let _ = BASE.set(url.trim_end_matches('/').to_string());
}

/// Shared wiremock server for the whole `api` test suite.
///
/// `BASE` is process-wide, so there can only be one mock server per test
/// binary. Every test awaits [`test_support::mock_server`] first and mounts its
/// mocks there; because mocks stay mounted, tests must not fight over the same
/// path+method with different responses — use a distinct `.ROBLOSECURITY`
/// cookie (or another precise matcher) per test instead.
#[cfg(test)]
pub mod test_support {
    use tokio::sync::OnceCell;
    use wiremock::MockServer;

    static SERVER: OnceCell<MockServer> = OnceCell::const_new();

    /// Starts the shared mock server on first use and wires `endpoints::host`
    /// to it. Never dropped, so mounted mocks survive for the whole run.
    pub async fn mock_server() -> &'static MockServer {
        SERVER
            .get_or_init(|| async {
                let server = MockServer::start().await;
                super::set_base_for_tests(&server.uri());
                server
            })
            .await
    }

    /// The URL an api call under test will actually hit, for building mock
    /// paths: `mock_path("auth", "/v1/account/pin/") == "/auth/v1/account/pin/"`.
    pub fn mock_path(sub: &str, path: &str) -> String {
        format!("/{}{}", sub, path)
    }

    /// The `Cookie` header value the api layer sends for an account token.
    /// Tests use a unique token each so their mocks never match each other.
    pub fn cookie_of(token: &str) -> String {
        format!(".ROBLOSECURITY={}", token)
    }

    /// Mounts the CSRF handshake every authenticated call performs first:
    /// Roblox answers the auth-ticket POST with 403 plus an `x-csrf-token`
    /// header. Matches only requests that do *not* already carry that header,
    /// so a second mock can answer the real auth-ticket POST on the same path.
    pub async fn mount_csrf(token: &str, csrf: &str) {
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, Request, ResponseTemplate};

        Mock::given(method("POST"))
            .and(path(mock_path("auth", "/v1/authentication-ticket/")))
            .and(header("cookie", cookie_of(token)))
            .and(|req: &Request| !req.headers.contains_key("x-csrf-token"))
            .respond_with(ResponseTemplate::new(403).insert_header("x-csrf-token", csrf))
            .mount(mock_server().await)
            .await;
    }

    #[tokio::test]
    async fn host_uses_the_shared_mock_server_in_tests() {
        let server = mock_server().await;
        assert_eq!(super::host("auth"), format!("{}/auth", server.uri()));
        assert_eq!(super::host("www"), format!("{}/www", server.uri()));
        assert_eq!(mock_path("users", "/v1/users/1"), "/users/v1/users/1");
    }
}

#[cfg(test)]
mod endpoint_host_tests {
    use super::format_host;

    /// Production formatting: `host("auth")` must stay byte-identical to the
    /// hardcoded `https://auth.roblox.com` every call site used to build.
    #[test]
    fn default_base_builds_the_roblox_subdomain() {
        for sub in [
            "auth",
            "apis",
            "users",
            "www",
            "web",
            "games",
            "gamejoin",
            "thumbnails",
            "economy",
            "friends",
            "groups",
            "presence",
            "avatar",
            "develop",
            "accountsettings",
        ] {
            assert_eq!(
                format_host(None, sub),
                format!("https://{}.roblox.com", sub)
            );
        }
    }

    /// In test mode the subdomain becomes a path prefix so two subdomains can
    /// never collide on the same path of the single shared mock server.
    #[test]
    fn an_override_turns_the_subdomain_into_a_path_prefix() {
        assert_eq!(
            format_host(Some("http://127.0.0.1:8080"), "auth"),
            "http://127.0.0.1:8080/auth"
        );
        assert_eq!(
            format_host(Some("http://127.0.0.1:8080"), "www"),
            "http://127.0.0.1:8080/www"
        );
    }

    #[test]
    fn the_override_is_stored_without_a_trailing_slash() {
        // `set_base_for_tests` trims trailing slashes; formatting must not add
        // a second one.
        assert_eq!(
            format_host(Some("http://127.0.0.1:8080"), "users"),
            "http://127.0.0.1:8080/users"
        );
    }
}
