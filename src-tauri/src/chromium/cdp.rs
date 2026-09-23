use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

/// Command line handed to the browser. Split out of [`spawn_chrome`] so the
/// flag set can be asserted without actually starting a browser.
fn chrome_args(profile: &Path, start_url: &str, debug: bool, stealth: bool) -> Vec<String> {
    let mut args = vec![
        format!("--user-data-dir={}", profile.to_string_lossy()),
        "--no-first-run".to_string(),
        "--no-default-browser-check".to_string(),
        "--disable-features=Translate".to_string(),
    ];
    if stealth {
        args.push("--lang=en-US".to_string());
    }
    if debug {
        args.push("--remote-debugging-port=0".to_string());
    }
    args.push(start_url.to_string());
    args
}

pub async fn spawn_chrome(
    binary: &Path,
    profile: &Path,
    start_url: &str,
    debug: bool,
    stealth: bool,
) -> Result<(Child, Option<u16>), String> {
    let _ = std::fs::create_dir_all(profile);
    let active_port_file = profile.join("DevToolsActivePort");
    if debug {
        let _ = std::fs::remove_file(&active_port_file);
    }

    let mut cmd = Command::new(binary);
    cmd.args(chrome_args(profile, start_url, debug, stealth));
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());

    let child = cmd.spawn().map_err(|e| format!("Could not start browser: {}", e))?;

    if !debug {
        return Ok((child, None));
    }

    for _ in 0..160 {
        if let Ok(contents) = std::fs::read_to_string(&active_port_file) {
            if let Some(port) = parse_active_port(&contents) {
                return Ok((child, Some(port)));
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    Ok((child, None))
}

/// First line of `DevToolsActivePort`, which holds the port the browser
/// actually bound to when it was started with `--remote-debugging-port=0`.
fn parse_active_port(contents: &str) -> Option<u16> {
    contents
        .lines()
        .next()
        .and_then(|l| l.trim().parse::<u16>().ok())
}

/// WebSocket debugger URL of the first `page` target in a `/json` listing.
fn page_ws_url_from_targets(targets: &Value) -> Option<String> {
    targets.as_array().and_then(|arr| {
        arr.iter()
            .find(|t| t.get("type").and_then(Value::as_str) == Some("page"))
            .and_then(|t| t.get("webSocketDebuggerUrl").and_then(Value::as_str))
            .map(|s| s.to_string())
    })
}

/// Outgoing CDP frame for `method`/`params` under the given request id.
fn cdp_request_payload(id: i64, method: &str, params: Value) -> Value {
    json!({ "id": id, "method": method, "params": params })
}

/// Classify an incoming CDP frame against the request id we are waiting on.
/// `None` means "not our frame, keep reading".
fn cdp_response_for_id(value: &Value, id: i64) -> Option<Result<Value, String>> {
    if value.get("id").and_then(Value::as_i64) != Some(id) {
        return None;
    }
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Browser command failed");
        return Some(Err(message.to_string()));
    }
    Some(Ok(value.get("result").cloned().unwrap_or(Value::Null)))
}

/// Value of the first non-empty `.ROBLOSECURITY` cookie in a
/// `Network.getAllCookies` result.
fn roblosecurity_from_cookies(result: &Value) -> Option<String> {
    result
        .get("cookies")
        .and_then(Value::as_array)
        .and_then(|cookies| {
            cookies
                .iter()
                .find(|c| {
                    c.get("name").and_then(Value::as_str) == Some(".ROBLOSECURITY")
                        && c.get("value")
                            .and_then(Value::as_str)
                            .map(|v| !v.is_empty())
                            .unwrap_or(false)
                })
                .and_then(|c| c.get("value").and_then(Value::as_str))
                .map(|s| s.to_string())
        })
}

/// JS expression testing whether `selector` currently matches an element.
/// The selector is embedded as a JSON string literal, never concatenated raw.
fn selector_exists_expression(selector: &str) -> String {
    format!(
        "document.querySelector({}) ? true : false",
        serde_json::to_string(selector).unwrap_or_default()
    )
}

/// JS snippet that types the credentials into the Roblox login form.
/// `username`/`password` are serialized as JSON string literals so quotes,
/// backslashes and control characters cannot terminate the literal and run as
/// code.
fn login_fill_script(username: &str, password: &str) -> String {
    let user = serde_json::to_string(username).unwrap_or_default();
    let pass = serde_json::to_string(password).unwrap_or_default();
    format!(
        "(function(){{\
var u=document.querySelector('#login-username');\
var p=document.querySelector('#login-password');\
var b=document.querySelector('#login-button');\
if(!u||!p)return 'no-fields';\
var s=Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype,'value').set;\
s.call(u,{user});u.dispatchEvent(new Event('input',{{bubbles:true}}));\
s.call(p,{pass});p.dispatchEvent(new Event('input',{{bubbles:true}}));\
if(b){{b.disabled=false;b.click();return 'clicked';}}\
return 'filled';}})()",
        user = user,
        pass = pass
    )
}

pub struct CdpClient {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    next_id: i64,
}

impl CdpClient {
    pub async fn connect(port: u16) -> Result<Self, String> {
        let mut ws_url = None;
        for _ in 0..40 {
            if let Ok(resp) = reqwest::get(format!("http://127.0.0.1:{}/json", port)).await {
                if let Ok(targets) = resp.json::<Value>().await {
                    ws_url = page_ws_url_from_targets(&targets);
                    if ws_url.is_some() {
                        break;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let ws_url = ws_url.ok_or("Could not attach to the browser")?;
        let (socket, _) = connect_async(ws_url.as_str())
            .await
            .map_err(|e| format!("Could not attach to the browser: {}", e))?;

        let mut client = Self { socket, next_id: 0 };
        let _ = client.send("Network.enable", json!({})).await;
        Ok(client)
    }

    pub async fn send(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        let payload = cdp_request_payload(id, method, params);
        self.socket
            .send(Message::Text(payload.to_string().into()))
            .await
            .map_err(|e| format!("Browser command failed: {}", e))?;

        let socket = &mut self.socket;
        let recv = async {
            loop {
                let frame = match socket.next().await {
                    Some(frame) => frame.map_err(|e| format!("Browser connection lost: {}", e))?,
                    None => return Err("Browser connection closed".to_string()),
                };

                let text = match frame {
                    Message::Text(text) => text,
                    Message::Close(_) => return Err("Browser connection closed".to_string()),
                    _ => continue,
                };

                let value: Value = match serde_json::from_str(&text) {
                    Ok(value) => value,
                    Err(_) => continue,
                };

                if let Some(outcome) = cdp_response_for_id(&value, id) {
                    return outcome;
                }
            }
        };

        match tokio::time::timeout(Duration::from_secs(20), recv).await {
            Ok(result) => result,
            Err(_) => Err("Browser stopped responding".to_string()),
        }
    }

    pub async fn eval(&mut self, expression: &str) -> Result<Value, String> {
        let result = self
            .send(
                "Runtime.evaluate",
                json!({ "expression": expression, "returnByValue": true, "awaitPromise": true }),
            )
            .await?;
        Ok(result
            .get("result")
            .and_then(|r| r.get("value"))
            .cloned()
            .unwrap_or(Value::Null))
    }

    pub async fn navigate(&mut self, url: &str) -> Result<(), String> {
        self.send("Page.navigate", json!({ "url": url })).await?;
        Ok(())
    }

    pub async fn inject_stealth(&mut self) -> Result<(), String> {
        let _ = self.send("Page.enable", json!({})).await;
        let source = "Object.defineProperty(navigator, 'webdriver', { get: () => undefined });";
        self.send(
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": source }),
        )
        .await?;
        Ok(())
    }

    pub async fn delete_roblosecurity(&mut self, domain: &str) -> Result<(), String> {
        self.send(
            "Network.deleteCookies",
            json!({ "name": ".ROBLOSECURITY", "domain": domain, "path": "/" }),
        )
        .await?;
        Ok(())
    }

    pub async fn set_roblosecurity(&mut self, value: &str, domain: &str) -> Result<(), String> {
        self.send(
            "Network.setCookie",
            json!({
                "name": ".ROBLOSECURITY",
                "value": value,
                "domain": domain,
                "path": "/",
                "secure": true,
                "httpOnly": true,
                "sameSite": "None",
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn get_roblosecurity(&mut self) -> Result<Option<String>, String> {
        let result = self.send("Network.getAllCookies", json!({})).await?;
        Ok(roblosecurity_from_cookies(&result))
    }

    pub async fn wait_for_selector(&mut self, selector: &str, attempts: u32) -> bool {
        let expression = selector_exists_expression(selector);
        for _ in 0..attempts {
            if let Ok(Value::Bool(true)) = self.eval(&expression).await {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        false
    }

    pub async fn fill_login(&mut self, username: &str, password: &str) -> Result<(), String> {
        let script = login_fill_script(username, password);
        self.eval(&script).await?;
        Ok(())
    }
}

#[cfg(test)]
mod chromium_cdp_tests {
    use super::*;
    use serde_json::json;

    // Message building and parsing only: nothing here starts a browser or
    // opens a socket.

    // ── chrome_args ────────────────────────────────────────────────────────

    #[test]
    fn chrome_args_always_pass_the_profile_and_the_start_url() {
        let args = chrome_args(Path::new("C:\\profiles\\42"), "about:blank", false, false);
        assert_eq!(args[0], "--user-data-dir=C:\\profiles\\42");
        assert_eq!(args.last().unwrap(), "about:blank");
        assert!(args.contains(&"--no-first-run".to_string()));
        assert!(args.contains(&"--no-default-browser-check".to_string()));
        assert!(args.contains(&"--disable-features=Translate".to_string()));
    }

    #[test]
    fn chrome_args_only_open_the_debugging_port_when_debugging() {
        let off = chrome_args(Path::new("p"), "about:blank", false, false);
        assert!(!off.iter().any(|a| a.starts_with("--remote-debugging-port")));

        let on = chrome_args(Path::new("p"), "about:blank", true, false);
        assert!(on.contains(&"--remote-debugging-port=0".to_string()));
        // The URL must stay last so the browser treats it as the page to open.
        assert_eq!(on.last().unwrap(), "about:blank");
    }

    #[test]
    fn chrome_args_only_force_the_locale_in_stealth_mode() {
        assert!(!chrome_args(Path::new("p"), "u", false, false).contains(&"--lang=en-US".to_string()));
        assert!(chrome_args(Path::new("p"), "u", false, true).contains(&"--lang=en-US".to_string()));
    }

    #[test]
    fn chrome_args_keep_a_login_url_as_the_final_argument() {
        let args = chrome_args(
            Path::new("p"),
            "https://www.roblox.com/login",
            true,
            true,
        );
        assert_eq!(args.last().unwrap(), "https://www.roblox.com/login");
    }

    // ── parse_active_port ──────────────────────────────────────────────────

    #[test]
    fn parse_active_port_reads_the_first_line() {
        // The real file is "<port>\n<browser ws path>".
        assert_eq!(
            parse_active_port("54321\n/devtools/browser/abc-def\n"),
            Some(54321)
        );
        assert_eq!(parse_active_port("  9222  \r\n"), Some(9222));
        assert_eq!(parse_active_port("1"), Some(1));
    }

    #[test]
    fn parse_active_port_returns_none_for_anything_unparsable() {
        assert_eq!(parse_active_port(""), None);
        assert_eq!(parse_active_port("\n54321"), None);
        assert_eq!(parse_active_port("not-a-port"), None);
        assert_eq!(parse_active_port("-1"), None);
        assert_eq!(parse_active_port("70000"), None); // beyond u16
    }

    // ── page_ws_url_from_targets ───────────────────────────────────────────

    #[test]
    fn page_ws_url_from_targets_picks_the_first_page_target() {
        let targets = json!([
            { "type": "background_page", "webSocketDebuggerUrl": "ws://bg" },
            { "type": "page", "webSocketDebuggerUrl": "ws://first-page" },
            { "type": "page", "webSocketDebuggerUrl": "ws://second-page" },
        ]);
        assert_eq!(
            page_ws_url_from_targets(&targets).as_deref(),
            Some("ws://first-page")
        );
    }

    #[test]
    fn page_ws_url_from_targets_returns_none_when_no_page_is_attachable() {
        assert!(page_ws_url_from_targets(&json!([])).is_none());
        assert!(page_ws_url_from_targets(&json!({})).is_none());
        assert!(page_ws_url_from_targets(&json!("nonsense")).is_none());
        assert!(page_ws_url_from_targets(&json!([{ "type": "worker" }])).is_none());
        // A page with no debugger URL cannot be attached to.
        assert!(page_ws_url_from_targets(&json!([{ "type": "page" }])).is_none());
    }

    // ── cdp_request_payload ────────────────────────────────────────────────

    #[test]
    fn cdp_request_payload_has_the_id_method_and_params() {
        let payload = cdp_request_payload(7, "Page.navigate", json!({ "url": "https://x" }));
        assert_eq!(payload["id"], 7);
        assert_eq!(payload["method"], "Page.navigate");
        assert_eq!(payload["params"]["url"], "https://x");
        assert_eq!(payload.as_object().unwrap().len(), 3);
    }

    #[test]
    fn cdp_request_payload_keeps_empty_params_as_an_object() {
        let payload = cdp_request_payload(1, "Network.enable", json!({}));
        assert!(payload["params"].is_object());
        assert_eq!(payload.to_string(), r#"{"id":1,"method":"Network.enable","params":{}}"#);
    }

    // ── cdp_response_for_id ────────────────────────────────────────────────

    #[test]
    fn cdp_response_for_id_ignores_frames_for_another_request() {
        assert!(cdp_response_for_id(&json!({ "id": 2, "result": {} }), 1).is_none());
        // Events carry no id at all.
        assert!(cdp_response_for_id(&json!({ "method": "Network.requestWillBeSent" }), 1).is_none());
        assert!(cdp_response_for_id(&json!({}), 1).is_none());
    }

    #[test]
    fn cdp_response_for_id_returns_the_result_of_a_matching_frame() {
        let outcome = cdp_response_for_id(&json!({ "id": 3, "result": { "ok": true } }), 3);
        assert_eq!(outcome.unwrap().unwrap(), json!({ "ok": true }));
    }

    #[test]
    fn cdp_response_for_id_yields_null_when_a_matching_frame_has_no_result() {
        let outcome = cdp_response_for_id(&json!({ "id": 3 }), 3);
        assert_eq!(outcome.unwrap().unwrap(), Value::Null);
    }

    #[test]
    fn cdp_response_for_id_surfaces_the_error_message() {
        let outcome =
            cdp_response_for_id(&json!({ "id": 3, "error": { "message": "no such node" } }), 3);
        assert_eq!(outcome.unwrap().unwrap_err(), "no such node");
    }

    #[test]
    fn cdp_response_for_id_falls_back_to_a_generic_error_message() {
        let outcome = cdp_response_for_id(&json!({ "id": 3, "error": { "code": -32000 } }), 3);
        assert_eq!(outcome.unwrap().unwrap_err(), "Browser command failed");
    }

    #[test]
    fn cdp_response_for_id_prefers_the_error_over_a_result() {
        let outcome = cdp_response_for_id(
            &json!({ "id": 3, "result": { "ok": true }, "error": { "message": "boom" } }),
            3,
        );
        assert_eq!(outcome.unwrap().unwrap_err(), "boom");
    }

    // ── roblosecurity_from_cookies ─────────────────────────────────────────

    #[test]
    fn roblosecurity_from_cookies_finds_the_session_cookie() {
        let result = json!({
            "cookies": [
                { "name": "RBXEventTrackerV2", "value": "junk" },
                { "name": ".ROBLOSECURITY", "value": "_|WARNING:-token" },
            ]
        });
        assert_eq!(
            roblosecurity_from_cookies(&result).as_deref(),
            Some("_|WARNING:-token")
        );
    }

    #[test]
    fn roblosecurity_from_cookies_skips_an_empty_value() {
        let result = json!({
            "cookies": [
                { "name": ".ROBLOSECURITY", "value": "" },
                { "name": ".ROBLOSECURITY", "value": "real-token" },
            ]
        });
        assert_eq!(
            roblosecurity_from_cookies(&result).as_deref(),
            Some("real-token")
        );
    }

    #[test]
    fn roblosecurity_from_cookies_returns_none_when_it_is_absent() {
        assert!(roblosecurity_from_cookies(&json!({ "cookies": [] })).is_none());
        assert!(roblosecurity_from_cookies(&json!({})).is_none());
        assert!(roblosecurity_from_cookies(&json!({ "cookies": "nope" })).is_none());
        assert!(roblosecurity_from_cookies(
            &json!({ "cookies": [{ "name": ".ROBLOSECURITY" }] })
        )
        .is_none());
        assert!(roblosecurity_from_cookies(
            &json!({ "cookies": [{ "name": ".ROBLOSECURITY", "value": "" }] })
        )
        .is_none());
    }

    #[test]
    fn roblosecurity_from_cookies_is_exact_about_the_cookie_name() {
        let result = json!({
            "cookies": [{ "name": ".roblosecurity", "value": "wrong-case" }]
        });
        assert!(roblosecurity_from_cookies(&result).is_none());
    }

    // ── selector_exists_expression (script-injection safety) ───────────────

    #[test]
    fn selector_exists_expression_embeds_the_selector_as_a_json_string() {
        assert_eq!(
            selector_exists_expression("#login-username"),
            "document.querySelector(\"#login-username\") ? true : false"
        );
    }

    #[test]
    fn selector_exists_expression_escapes_a_selector_that_would_close_the_literal() {
        let expr = selector_exists_expression("a\") || (alert(1)) || (\"");
        // The injected quote is escaped, so the payload stays one string.
        assert!(expr.contains("\\\""), "expression was: {}", expr);
        assert_eq!(
            expr.matches("document.querySelector").count(),
            1,
            "expression was: {}",
            expr
        );
        // Round-tripping the literal gives back exactly the original selector.
        let literal = expr
            .trim_start_matches("document.querySelector(")
            .trim_end_matches(") ? true : false");
        assert_eq!(
            serde_json::from_str::<String>(literal).unwrap(),
            "a\") || (alert(1)) || (\""
        );
    }

    // ── login_fill_script (credential-injection safety) ────────────────────

    #[test]
    fn login_fill_script_targets_the_roblox_login_form() {
        let script = login_fill_script("someone", "hunter2");
        assert!(script.contains("#login-username"));
        assert!(script.contains("#login-password"));
        assert!(script.contains("#login-button"));
        assert!(script.contains("\"someone\""));
        assert!(script.contains("\"hunter2\""));
    }

    #[test]
    fn login_fill_script_escapes_credentials_that_would_close_the_literal() {
        // Regression test: credentials are attacker-influenced input (a user
        // can paste anything) and must never become executable script.
        let script = login_fill_script("bob\"); alert(1); //", "p\\\"; evil()");

        assert!(!script.contains("alert(1); //\n"));
        // Every injected quote is backslash-escaped inside the JSON literal.
        assert!(script.contains("bob\\\""), "script was: {}", script);
        // The script still has exactly the three selectors it is supposed to.
        assert_eq!(script.matches("document.querySelector").count(), 3);
    }

    #[test]
    fn login_fill_script_round_trips_unicode_and_control_characters() {
        for (user, pass) in [
            ("a\nb", "c\td"),
            ("émile", "señor"),
            ("<script>", "</script>"),
            ("\\", "\""),
            ("", ""),
        ] {
            let script = login_fill_script(user, pass);
            // No raw newline or tab may reach the script body.
            assert!(!script.contains('\n'), "raw newline for {:?}", user);
            assert!(!script.contains('\t'), "raw tab for {:?}", pass);
            assert!(
                script.contains(&serde_json::to_string(user).unwrap()),
                "user {:?} not embedded as a JSON literal",
                user
            );
            assert!(
                script.contains(&serde_json::to_string(pass).unwrap()),
                "password {:?} not embedded as a JSON literal",
                pass
            );
        }
    }

    #[test]
    fn login_fill_script_is_stable_for_the_same_input() {
        assert_eq!(
            login_fill_script("a", "b"),
            login_fill_script("a", "b"),
            "the script must not depend on anything but its inputs"
        );
    }
}
