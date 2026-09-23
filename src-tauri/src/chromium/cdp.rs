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
