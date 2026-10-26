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
