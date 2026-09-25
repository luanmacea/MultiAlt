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

/// Base URL of a service that is **not** `<sub>.roblox.com` but still has to be
/// mockable — today only the IP geolocation used to name a server's region.
///
/// `name` is the logical service name; in tests it becomes a path prefix on the
/// shared mock server, exactly like [`host`].
pub fn external_host(name: &str) -> String {
    match (BASE.get().map(|s| s.as_str()), name) {
        (Some(base), _) => format!("{}/{}", base, name),
        (None, "ipwhois") => "https://ipwho.is".to_string(),
        (None, "ipapi") => "http://ip-api.com".to_string(),
        (None, other) => panic!("unknown external host: {}", other),
    }
}
