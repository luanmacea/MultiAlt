use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

/// Endereço ao qual a porta de debug fica presa.
///
/// O padrão do Chromium já é a loopback, mas passamos explicitamente para que
/// uma policy corporativa, um `CHROME_FLAGS` herdado ou uma mudança de default
/// não consigam expor o endpoint CDP para a rede — aí o roubo do cookie
/// deixaria de exigir sequer um processo local.
const DEBUG_BIND_ADDRESS: &str = "127.0.0.1";

/// Command line handed to the browser. Split out of [`spawn_chrome`] so the
/// flag set can be asserted without actually starting a browser.
///
/// `debug` abre o endpoint CDP, que **não tem autenticação**: enquanto ele
/// existir, qualquer processo do mesmo usuário pode se conectar e ler o
/// cookie de sessão. Só passe `true` na janela em que o CDP é realmente
/// necessário e feche o browser assim que terminar.
fn chrome_args(profile: &Path, start_url: &str, debug: bool, stealth: bool) -> Vec<String> {
    let mut args = vec![
        format!("--user-data-dir={}", profile.to_string_lossy()),
        "--no-first-run".to_string(),
        "--no-default-browser-check".to_string(),
        // Pop-ups que não têm o que fazer aqui (pedido do dono, 26/10/2026),
        // medidos com o Chrome for Testing 154: `--disable-infobars` tira a
        // faixa "destinado apenas a testes automatizados" e `--disable-sync` o
        // "Fazer login no Chromium?". Salvar senha e preenchimento automático
        // saem pelas preferências do perfil (`prepare_profile`).
        "--disable-infobars".to_string(),
        "--disable-sync".to_string(),
        "--hide-crash-restore-bubble".to_string(),
        "--disable-search-engine-choice-screen".to_string(),
        "--disable-features=Translate,PasswordLeakDetection".to_string(),
    ];
    if stealth {
        args.push("--lang=en-US".to_string());
    }
    if debug {
        args.push("--remote-debugging-port=0".to_string());
        args.push(format!("--remote-debugging-address={}", DEBUG_BIND_ADDRESS));
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
    prepare_profile(profile);
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

    let mut child = child;
    for _ in 0..ACTIVE_PORT_POLLS {
        if let Ok(contents) = std::fs::read_to_string(&active_port_file) {
            if let Some(port) = parse_active_port(&contents) {
                return Ok((child, Some(port)));
            }
        }
        // O navegador já saiu (arquivo faltando, perfil em uso): esperar o resto
        // do prazo não muda nada.
        if matches!(child.try_wait(), Ok(Some(_))) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(ACTIVE_PORT_POLL_MS)).await;
    }

    Ok((child, None))
}

/// Preferências gravadas no perfil antes de cada abertura: sem oferta de salvar
/// senha nem de preencher endereço/cartão, sem pedido de notificação, e saída
/// "Normal" (sem o balão "Restaurar páginas?" depois de o app fechar a janela).
/// Nenhuma delas fica nas "Secure Preferences" (com assinatura): mexer nessas
/// faria o Chrome dizer que as configurações foram alteradas e resetá-las.
const PROFILE_PREFERENCES: &[(&[&str], fn() -> Value)] = &[
    (&["credentials_enable_service"], || json!(false)),
    (&["profile", "password_manager_enabled"], || json!(false)),
    (&["profile", "password_manager_leak_detection"], || json!(false)),
    (&["autofill", "profile_enabled"], || json!(false)),
    (&["autofill", "credit_card_enabled"], || json!(false)),
    (&["profile", "default_content_setting_values", "notifications"], || json!(2)),
    (&["profile", "exit_type"], || json!("Normal")),
    (&["profile", "exited_cleanly"], || json!(true)),
];

/// Antes de abrir o navegador num perfil: apaga a sessão anterior (as abas
/// voltavam a cada abertura — relato do dono, 26/10/2026, com o perfil de
/// login persistente) e grava `PROFILE_PREFERENCES`. Cookie, cache e histórico
/// ficam: são o perfil persistente (`Login.PersistentProfile`) funcionando.
/// Preferências que não dão para ler ficam como estão.
pub(crate) fn prepare_profile(profile: &Path) {
    let default = profile.join("Default");
    let _ = std::fs::remove_dir_all(default.join("Sessions"));
    for legacy in ["Current Session", "Current Tabs", "Last Session", "Last Tabs"] {
        let _ = std::fs::remove_file(default.join(legacy));
    }
    let path = default.join("Preferences");
    let mut prefs = match std::fs::read_to_string(&path) {
        Ok(raw) => match serde_json::from_str::<Value>(&raw) {
            Ok(value) if value.is_object() => value,
            _ => return,
        },
        Err(_) => json!({}),
    };
    for (keys, value) in PROFILE_PREFERENCES {
        let mut node = &mut prefs;
        for key in &keys[..keys.len() - 1] {
            if !node.get(*key).is_some_and(Value::is_object) {
                node[*key] = json!({});
            }
            node = &mut node[*key];
        }
        node[keys[keys.len() - 1]] = value();
    }
    if std::fs::create_dir_all(&default).is_ok() {
        let _ = std::fs::write(&path, prefs.to_string());
    }
}

/// Quantas vezes (a cada `ACTIVE_PORT_POLL_MS`) se espera o `DevToolsActivePort`
/// aparecer: 30 s. Eram 8 s, pouco para a primeira abertura de um Chromium
/// recém-extraído sob antivírus.
const ACTIVE_PORT_POLLS: u32 = 600;
const ACTIVE_PORT_POLL_MS: u64 = 50;

/// Erro quando o navegador não chegou a abrir o endpoint de depuração no prazo.
/// Em inglês: o frontend traduz (`chromiumErrorText`, src/utils/chromiumErrors.ts,
/// que lê esta constante do fonte no teste). A janela que ficou aberta em
/// about:blank é fechada por quem devolve este erro.
pub const BROWSER_DID_NOT_OPEN: &str = "The browser did not open in time. Check your antivirus and firewall, then try Reinstall in Settings > Accounts.";

/// O texto de erro de um `Page.navigate` que falhou (DNS, proxy, firewall), se
/// houve: o comando responde com sucesso e põe o erro no resultado.
fn navigation_error(result: &Value) -> Option<String> {
    result
        .get("errorText")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
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
/// Foca um campo e apaga o que estiver nele, para a digitação começar limpa.
fn focus_and_clear_script(selector: &str) -> String {
    let selector = serde_json::to_string(selector).unwrap_or_default();
    format!(
        "(function(){{var el=document.querySelector({selector});if(!el)return false;\
el.focus();\
var s=Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype,'value').set;\
if(s){{s.call(el,'');el.dispatchEvent(new Event('input',{{bubbles:true}}));}}\
return document.activeElement===el;}})()",
        selector = selector
    )
}

/// Guarda a conta que a **resposta do login** do Roblox devolveu
/// (`auth.roblox.com/v2/login` → `{"user":{"id","name"}}`) no `localStorage`
/// do roblox.com, com a hora. É a prova de quem entrou quando as consultas de
/// "de quem é esta sessão" recusam (conta moderada: relato de 26/10/2026, a
/// Rapid_Raven878 logava e o app jogava a sessão fora).
pub const LOGIN_USER_HOOK: &str = "(function(){\
if(window.__ramLoginHook)return;window.__ramLoginHook=true;\
function keep(t){try{var d=JSON.parse(t);var u=d&&d.user;if(u&&u.id){localStorage.setItem('__ramLoginUser',JSON.stringify({id:u.id,name:u.name||'',at:Date.now()}));}}catch(e){}}\
var f=window.fetch;if(f){window.fetch=function(i){var url=(typeof i==='string')?i:((i&&i.url)||'');var p=f.apply(this,arguments);\
if(/\\/v2\\/login/.test(url)){p.then(function(r){r.clone().text().then(keep).catch(function(){});}).catch(function(){});}return p;};}\
var o=XMLHttpRequest.prototype.open,s=XMLHttpRequest.prototype.send;\
XMLHttpRequest.prototype.open=function(m,u){this.__ramUrl=u;return o.apply(this,arguments);};\
XMLHttpRequest.prototype.send=function(){var x=this;if(/\\/v2\\/login/.test(String(x.__ramUrl||''))){x.addEventListener('load',function(){keep(x.responseText);});}return s.apply(this,arguments);};\
})()";

/// Lê o que o [`LOGIN_USER_HOOK`] guardou.
pub const READ_LOGIN_USER_SCRIPT: &str = "localStorage.getItem('__ramLoginUser')";

/// A conta da resposta do login, se ela foi guardada **depois** de
/// `since_ms` (o perfil persistente guarda o `localStorage` de logins
/// anteriores — de outra conta).
pub fn parse_login_user(stored: &Value, since_ms: i64) -> Option<(i64, String)> {
    let text = stored.as_str()?;
    let parsed: Value = serde_json::from_str(text).ok()?;
    let id = parsed.get("id")?.as_i64().filter(|id| *id > 0)?;
    let at = parsed.get("at")?.as_i64()?;
    if at < since_ms {
        return None;
    }
    let name = parsed.get("name").and_then(Value::as_str).unwrap_or("").to_string();
    Some((id, name))
}

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

/// Parâmetros de `Network.setCookie` para o cookie de sessão do Roblox.
///
/// Com `expires`, o Chromium grava o cookie no perfil (cookie persistente);
/// sem ele, o cookie vive só na memória do processo e some no restart — que é
/// justamente o que o fluxo de duas fases do browser de conta precisa que
/// **não** aconteça.
fn set_cookie_params(value: &str, domain: &str, expires: Option<f64>) -> Value {
    let mut params = json!({
        "name": ".ROBLOSECURITY",
        "value": value,
        "domain": domain,
        "path": "/",
        "secure": true,
        "httpOnly": true,
        "sameSite": "None",
    });
    if let Some(expires) = expires {
        params["expires"] = json!(expires);
    }
    params
}

/// Validade dada ao cookie plantado, em segundos desde a época.
pub fn persistent_cookie_expiry(now_secs: f64) -> f64 {
    now_secs + 365.0 * 24.0 * 60.0 * 60.0
}

/// `true` quando o erro veio do socket ter sumido, e não do comando em si.
/// Ao mandar o browser fechar isso é o resultado esperado, não uma falha.
fn is_disconnect(message: &str) -> bool {
    message.contains("connection closed") || message.contains("connection lost")
}

pub struct CdpClient {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    next_id: i64,
}

impl CdpClient {
    pub async fn connect(port: u16) -> Result<Self, String> {
        let mut ws_url = None;
        // Cliente com prazo: o `reqwest::get` não tinha, e um proxy do sistema
        // ou um firewall travava a conexão local para sempre.
        let http = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()
            .map_err(|e| format!("Could not attach to the browser: {}", e))?;
        for _ in 0..40 {
            if let Ok(resp) = http.get(format!("http://127.0.0.1:{}/json", port)).send().await {
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
        let (socket, _) = tokio::time::timeout(Duration::from_secs(10), connect_async(ws_url.as_str()))
            .await
            .map_err(|_| "Could not attach to the browser: timed out".to_string())?
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

    /// Digita num campo como o teclado faria.
    ///
    /// `Input.insertText` passa pelo mesmo caminho de um IME, então o React
    /// enxerga a digitação mesmo quando o componente ignora um `value` escrito
    /// por fora — foi o que fez o nome de usuário do cadastro ficar vazio
    /// enquanto a senha, no mesmo formulário, era preenchida.
    ///
    /// Devolve `false` quando o campo não está na página.
    pub async fn type_into(&mut self, selector: &str, text: &str) -> Result<bool, String> {
        let focus = focus_and_clear_script(selector);
        if self.eval(&focus).await? != Value::Bool(true) {
            return Ok(false);
        }
        self.send("Input.insertText", json!({ "text": text })).await?;
        Ok(true)
    }

    pub async fn navigate(&mut self, url: &str) -> Result<(), String> {
        let result = self.send("Page.navigate", json!({ "url": url })).await?;
        match navigation_error(&result) {
            Some(error) => Err(format!("The page did not load: {error}")),
            None => Ok(()),
        }
    }

    /// Liga o [`LOGIN_USER_HOOK`] nas páginas que abrirem daqui em diante e na
    /// que está aberta.
    pub async fn watch_login_response(&mut self) -> Result<(), String> {
        let _ = self.send("Page.enable", json!({})).await;
        self.send(
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": LOGIN_USER_HOOK }),
        )
        .await?;
        let _ = self.eval(LOGIN_USER_HOOK).await;
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

    pub async fn set_roblosecurity(
        &mut self,
        value: &str,
        domain: &str,
        expires: Option<f64>,
    ) -> Result<(), String> {
        self.send("Network.setCookie", set_cookie_params(value, domain, expires))
            .await?;
        Ok(())
    }

    /// Pede ao browser que se encerre sozinho.
    ///
    /// O shutdown gracioso é o que faz o Chromium gravar os cookies
    /// persistentes no perfil antes de sair; um `kill()` seco pode perder a
    /// gravação. `Browser.close` não existe em toda sessão de página, então
    /// caímos para `Page.close` (fechar a última aba também encerra o
    /// browser). Perder o socket no meio é sucesso: o browser já foi embora.
    pub async fn close_browser(&mut self) -> Result<(), String> {
        match self.send("Browser.close", json!({})).await {
            Ok(_) => Ok(()),
            Err(e) if is_disconnect(&e) => Ok(()),
            Err(_) => match self.send("Page.close", json!({})).await {
                Ok(_) => Ok(()),
                Err(e) if is_disconnect(&e) => Ok(()),
                Err(e) => Err(e),
            },
        }
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

    // ── espera pela porta e falha de navegação ──────────────────────────────

    /// Relato (23/10/2026): com o navegador demorando mais que 8 s para abrir
    /// (antivírus examinando os 300 MB, disco lento), o app desistia e deixava
    /// a janela aberta em about:blank. A espera agora tem folga.
    #[test]
    fn the_wait_for_the_debug_port_is_long_enough_for_a_slow_first_start() {
        let waited_ms = ACTIVE_PORT_POLLS as u64 * ACTIVE_PORT_POLL_MS;
        assert!(waited_ms >= 30_000, "{waited_ms} ms");
    }

    #[test]
    fn the_browser_did_not_open_message_says_what_to_try() {
        assert!(BROWSER_DID_NOT_OPEN.contains("antivirus"));
        assert!(BROWSER_DID_NOT_OPEN.contains("Reinstall"));
    }

    #[test]
    fn a_failed_navigation_is_reported_with_its_error_text() {
        let failed = json!({ "frameId": "A1", "errorText": "net::ERR_NAME_NOT_RESOLVED" });
        assert_eq!(
            navigation_error(&failed),
            Some("net::ERR_NAME_NOT_RESOLVED".to_string())
        );
        assert_eq!(navigation_error(&json!({ "frameId": "A1", "loaderId": "L1" })), None);
        assert_eq!(navigation_error(&json!({ "errorText": "" })), None);
        assert_eq!(navigation_error(&json!({})), None);
    }

    // ── chrome_args ────────────────────────────────────────────────────────

    // ── perfil sem abas antigas nem pop-ups (pedido do dono, 26/10/2026) ──────

    #[test]
    fn chrome_args_hide_the_testing_bar_the_sign_in_button_and_the_password_prompts() {
        let args = chrome_args(Path::new("p"), "about:blank", true, true);
        // Medido com o Chrome for Testing 154: `--disable-infobars` tira a faixa
        // "destinado apenas a testes automatizados" (`--test-type` não tira) e
        // `--disable-sync` tira o "Fazer login no Chromium?".
        assert!(args.contains(&"--disable-infobars".to_string()));
        assert!(args.contains(&"--disable-sync".to_string()));
        assert!(args.contains(&"--hide-crash-restore-bubble".to_string()));
        let features = args.iter().find(|a| a.starts_with("--disable-features=")).expect("features");
        assert!(features.contains("Translate") && features.contains("PasswordLeakDetection"), "{features}");
        assert_eq!(args.last().unwrap(), "about:blank");
    }

    fn temp_profile(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ram-cdp-profile-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Default").join("Sessions")).unwrap();
        dir
    }

    #[test]
    fn preparing_a_profile_drops_the_old_tabs_and_keeps_the_rest() {
        let dir = temp_profile("tabs");
        let default = dir.join("Default");
        std::fs::write(default.join("Sessions").join("Tabs_1"), b"x").unwrap();
        std::fs::write(default.join("Sessions").join("Session_1"), b"x").unwrap();
        std::fs::write(default.join("Cookies"), b"keep").unwrap();
        prepare_profile(&dir);
        assert!(!default.join("Sessions").exists(), "the tabs of the last session would reopen");
        assert_eq!(std::fs::read(default.join("Cookies")).unwrap(), b"keep");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preparing_a_profile_turns_off_saving_passwords_and_keeps_other_preferences() {
        let dir = temp_profile("prefs");
        let prefs_path = dir.join("Default").join("Preferences");
        std::fs::write(&prefs_path, r#"{"intl":{"accept_languages":"en-US"},"profile":{"name":"x","exit_type":"Crashed"}}"#).unwrap();
        prepare_profile(&dir);
        let prefs: Value = serde_json::from_str(&std::fs::read_to_string(&prefs_path).unwrap()).unwrap();
        assert_eq!(prefs["credentials_enable_service"], json!(false));
        assert_eq!(prefs["profile"]["password_manager_enabled"], json!(false));
        assert_eq!(prefs["profile"]["password_manager_leak_detection"], json!(false));
        assert_eq!(prefs["autofill"]["profile_enabled"], json!(false));
        assert_eq!(prefs["profile"]["default_content_setting_values"]["notifications"], json!(2));
        // Saída "Normal": sem o balão "Restaurar páginas?".
        assert_eq!(prefs["profile"]["exit_type"], json!("Normal"));
        // O que já estava lá fica.
        assert_eq!(prefs["intl"]["accept_languages"], json!("en-US"));
        assert_eq!(prefs["profile"]["name"], json!("x"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preparing_a_new_profile_creates_the_preferences_and_an_unreadable_one_is_left_alone() {
        let fresh = std::env::temp_dir().join(format!("ram-cdp-profile-fresh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&fresh);
        prepare_profile(&fresh);
        let prefs: Value =
            serde_json::from_str(&std::fs::read_to_string(fresh.join("Default").join("Preferences")).unwrap()).unwrap();
        assert_eq!(prefs["credentials_enable_service"], json!(false));
        let _ = std::fs::remove_dir_all(&fresh);

        let broken = temp_profile("broken");
        let path = broken.join("Default").join("Preferences");
        std::fs::write(&path, "not json").unwrap();
        prepare_profile(&broken);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "not json", "a file the app cannot read is not overwritten");
        let _ = std::fs::remove_dir_all(&broken);
    }

    /// Com o Chromium de verdade (`RAM_CHROME_BINARY`): abre uma página, fecha
    /// pelo caminho normal (que grava a sessão no perfil) e abre de novo no mesmo
    /// perfil — só a página pedida pode estar lá, não a da vez anterior.
    #[tokio::test]
    #[ignore]
    async fn real_chromium_reopens_a_profile_without_the_old_tabs() {
        let binary = std::path::PathBuf::from(std::env::var("RAM_CHROME_BINARY").expect("RAM_CHROME_BINARY"));
        let profile = std::env::temp_dir().join(format!("ram-real-tabs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&profile);
        let page_urls = |port: u16| async move {
            let targets: Value = reqwest::get(format!("http://127.0.0.1:{port}/json"))
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            targets
                .as_array()
                .unwrap()
                .iter()
                .filter(|t| t["type"] == "page")
                .map(|t| t["url"].as_str().unwrap_or("").to_string())
                .collect::<Vec<_>>()
        };

        let (mut first, port) = spawn_chrome(&binary, &profile, "https://example.com/", true, false).await.unwrap();
        let port = port.expect("debug port");
        tokio::time::sleep(Duration::from_secs(4)).await;
        let before = page_urls(port).await;
        if std::env::var("RAM_CHROME_KILL").is_ok() {
            // O caminho do login: a janela é fechada matando o processo.
            let _ = first.kill();
        } else {
            let mut cdp = CdpClient::connect(port).await.unwrap();
            let _ = cdp.close_browser().await;
        }
        for _ in 0..50 {
            if matches!(first.try_wait(), Ok(Some(_))) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let saved_sessions = profile.join("Default").join("Sessions").exists();

        let (mut second, port) = spawn_chrome(&binary, &profile, "about:blank", true, false).await.unwrap();
        let port = port.expect("debug port");
        tokio::time::sleep(Duration::from_secs(4)).await;
        let after = page_urls(port).await;
        let _ = second.kill();
        let _ = first.kill();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let _ = std::fs::remove_dir_all(&profile);
        eprintln!("first run pages {before:?}, sessions saved: {saved_sessions}, second run pages {after:?}");
        assert!(after.iter().all(|u| !u.contains("example.com")), "the old tab came back: {after:?}");
    }

    #[test]
    fn chrome_args_always_pass_the_profile_and_the_start_url() {
        let args = chrome_args(Path::new("C:\\profiles\\42"), "about:blank", false, false);
        assert_eq!(args[0], "--user-data-dir=C:\\profiles\\42");
        assert_eq!(args.last().unwrap(), "about:blank");
        assert!(args.contains(&"--no-first-run".to_string()));
        assert!(args.contains(&"--no-default-browser-check".to_string()));
        assert!(args.iter().any(|a| a.starts_with("--disable-features=") && a.contains("Translate")));
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

    #[test]
    fn chrome_args_pin_the_debugging_port_to_the_loopback() {
        // A porta CDP não tem autenticação: se ela sair da loopback, qualquer
        // máquina da rede rouba o cookie de sessão.
        let on = chrome_args(Path::new("p"), "about:blank", true, false);
        assert!(on.contains(&"--remote-debugging-address=127.0.0.1".to_string()));
        assert_eq!(on.last().unwrap(), "about:blank");

        let off = chrome_args(Path::new("p"), "about:blank", false, false);
        assert!(!off.iter().any(|a| a.starts_with("--remote-debugging-address")));
    }

    #[test]
    fn chrome_args_never_hand_a_web_origin_permission_to_attach() {
        // Desde o Chrome 111 um handshake WebSocket com cabeçalho `Origin` é
        // recusado, a menos que a origem esteja em `--remote-allow-origins`.
        // Esse default é o que impede uma página aberta no próprio browser de
        // falar CDP; nenhuma variação de argumentos pode afrouxá-lo.
        for (debug, stealth) in [(false, false), (true, false), (false, true), (true, true)] {
            let args = chrome_args(Path::new("p"), "https://www.roblox.com/login", debug, stealth);
            assert!(
                !args.iter().any(|a| a.starts_with("--remote-allow-origins")),
                "args were: {:?}",
                args
            );
        }
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

    // ── set_cookie_params / persistent_cookie_expiry ───────────────────────

    #[test]
    fn set_cookie_params_always_mark_the_session_cookie_as_secure() {
        let params = set_cookie_params("token", ".roblox.com", None);
        assert_eq!(params["name"], ".ROBLOSECURITY");
        assert_eq!(params["value"], "token");
        assert_eq!(params["domain"], ".roblox.com");
        assert_eq!(params["path"], "/");
        assert_eq!(params["secure"], true);
        assert_eq!(params["httpOnly"], true);
        assert_eq!(params["sameSite"], "None");
    }

    #[test]
    fn set_cookie_params_only_carry_an_expiry_when_one_is_given() {
        // Sem `expires` o cookie é de sessão e morre com o processo; o fluxo
        // de duas fases depende de ele sobreviver ao restart do browser.
        assert!(set_cookie_params("token", ".roblox.com", None)
            .get("expires")
            .is_none());

        let params = set_cookie_params("token", ".roblox.com", Some(1_700_000_000.0));
        assert_eq!(params["expires"], 1_700_000_000.0);
    }

    #[test]
    fn persistent_cookie_expiry_is_about_a_year_ahead() {
        let now = 1_700_000_000.0;
        let expiry = persistent_cookie_expiry(now);
        assert!(expiry > now);
        assert_eq!(expiry - now, 365.0 * 24.0 * 60.0 * 60.0);
    }

    // ── is_disconnect ──────────────────────────────────────────────────────

    #[test]
    fn is_disconnect_recognises_the_socket_going_away() {
        assert!(is_disconnect("Browser connection closed"));
        assert!(is_disconnect("Browser connection lost: reset"));
    }

    #[test]
    fn is_disconnect_does_not_swallow_a_real_command_failure() {
        assert!(!is_disconnect("'Browser.close' wasn't found"));
        assert!(!is_disconnect("Browser command failed"));
        assert!(!is_disconnect("Browser stopped responding"));
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
    fn the_login_user_is_read_only_when_it_is_from_this_sign_in() {
        let stored = json!(r#"{"id":11715660717,"name":"Rapid_Raven878","at":5000}"#);
        assert_eq!(parse_login_user(&stored, 4000), Some((11715660717, "Rapid_Raven878".to_string())));
        // Guardado num login anterior (perfil persistente): não vale.
        assert_eq!(parse_login_user(&stored, 6000), None);
        assert_eq!(parse_login_user(&Value::Null, 0), None);
        assert_eq!(parse_login_user(&json!("not json"), 0), None);
        assert_eq!(parse_login_user(&json!(r#"{"id":0,"at":9}"#), 0), None);
    }

    #[test]
    fn the_login_hook_watches_the_login_request_and_keeps_only_the_user() {
        assert!(LOGIN_USER_HOOK.contains("/v2\\/login") || LOGIN_USER_HOOK.contains("v2\\/login"));
        assert!(LOGIN_USER_HOOK.contains("__ramLoginUser"));
        assert!(LOGIN_USER_HOOK.contains("d.user"));
        // Nada além de id, nome e hora: o corpo (que pode ter token) não é guardado.
        assert!(!LOGIN_USER_HOOK.contains("setItem('__ramLoginUser',t"));
    }

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
