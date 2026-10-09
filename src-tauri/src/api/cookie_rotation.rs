//! O cookie `.ROBLOSECURITY` novo que o Roblox devolve numa resposta, num lugar só.
//!
//! Às vezes o Roblox troca o cookie da conta no meio de uma chamada qualquer
//! (`Set-Cookie: .ROBLOSECURITY=…`). Antes daqui só dois fluxos o liam (sair
//! das outras sessões e trocar a senha); em qualquer outra hora a conta salva
//! ficava com o cookie velho e aparecia "inválida" sem motivo.
//!
//! Como funciona:
//!
//! 1. Os helpers de envio compartilhados (`auth::send_with_csrf_retry`,
//!    `roblox::send_with_retry`) e as chamadas de `auth.rs` passam cada resposta
//!    por [`note_response`], com o cookie que **foi enviado** no pedido.
//! 2. Se a resposta traz um `.ROBLOSECURITY` novo e bem formado, ele fica
//!    guardado em memória, indexado pelo cookie velho.
//! 3. A camada de comandos (`read_without_refresh` / `run_with_session_retry`,
//!    em `commands/account_api.rs`) chama [`take_rotated`] depois da chamada e
//!    grava o novo pelo caminho normal e criptografado do `AccountStore` — e só
//!    se a conta ainda tiver o cookie velho (`replace_token_if`).
//!
//! **O valor nunca vai para log**, nem para mensagem de erro: aqui não há
//! `println!`/`format!` com ele, e o tipo que o guarda não é `Debug`.

use reqwest::header::{HeaderMap, COOKIE, SET_COOKIE};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// Cookie velho → cookie novo. Só em memória; vazio na maior parte do tempo.
static ROTATED: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

const COOKIE_NAME: &str = ".ROBLOSECURITY";

/// Menor `.ROBLOSECURITY` que aceitamos. O real tem centenas de caracteres
/// (`_|WARNING:-DO-NOT-SHARE-THIS…` + o token); um valor curto é lixo, e gravar
/// lixo por cima de um cookie que funciona é pior que não gravar nada.
const MIN_COOKIE_LEN: usize = 50;

/// Teto de rotações à espera de serem gravadas.
const MAX_PENDING: usize = 256;

/// O `.ROBLOSECURITY` que um `Set-Cookie` entrega, se for um cookie de verdade.
///
/// Ignora: outro nome de cookie, valor vazio, cookie de **apagar** (`Max-Age=0`
/// ou `Expires` em 1970, que é como o Roblox desloga), valor curto demais ou com
/// caractere que não cabe num cookie.
fn parse_set_cookie(line: &str) -> Option<String> {
    let mut parts = line.split(';');
    let pair = parts.next()?.trim();
    let (name, value) = pair.split_once('=')?;
    if name.trim() != COOKIE_NAME {
        return None;
    }
    let value = value.trim();
    if value.len() < MIN_COOKIE_LEN
        || !value
            .bytes()
            .all(|b| b.is_ascii_graphic() && b != b';' && b != b',' && b != b'"' && b != b'\\')
    {
        return None;
    }
    for attr in parts {
        let attr = attr.trim().to_ascii_lowercase();
        if let Some(age) = attr.strip_prefix("max-age=") {
            if age.trim().parse::<i64>().map(|n| n <= 0).unwrap_or(false) {
                return None;
            }
        }
        if attr.starts_with("expires=") && attr.contains("1970") {
            return None;
        }
    }
    Some(value.to_string())
}

/// O primeiro `.ROBLOSECURITY` válido entre os `Set-Cookie` de uma resposta.
pub fn rotated_cookie_in(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(parse_set_cookie)
}

/// O token que um pedido carregou no header `Cookie` (`.ROBLOSECURITY=<token>`).
pub fn sent_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|line| line.split(';'))
        .find_map(|pair| {
            let (name, value) = pair.trim().split_once('=')?;
            (name.trim() == COOKIE_NAME && !value.trim().is_empty())
                .then(|| value.trim().to_string())
        })
}

/// Guarda o cookie novo que `headers` (a resposta) traz para quem mandou
/// `sent`. Não faz nada se não houver cookie novo, se ele for igual ao enviado
/// ou se `sent` estiver vazio (chamada sem conta).
pub fn note_response(sent: &str, headers: &HeaderMap) {
    if sent.trim().is_empty() {
        return;
    }
    let Some(new) = rotated_cookie_in(headers) else {
        return;
    };
    if new == sent {
        return;
    }
    if let Ok(mut map) = ROTATED.lock() {
        // Chamada com cookie que nenhum comando recolhe (leitura direta por
        // `get_cookie`) deixa a entrada aqui; o teto impede o mapa de crescer.
        if map.len() >= MAX_PENDING && !map.contains_key(sent) {
            map.clear();
        }
        map.insert(sent.to_string(), new);
    }
}

/// `builder.send()` que também registra o cookie novo da resposta para o
/// cookie que o pedido levou. É o caminho de envio das chamadas autenticadas
/// (`auth.rs`, `send_with_csrf_retry`, `send_with_retry`).
pub async fn send(builder: reqwest::RequestBuilder) -> Result<reqwest::Response, reqwest::Error> {
    let (client, request) = builder.build_split();
    let request = request?;
    let sent = sent_token(request.headers());
    let response = client.execute(request).await?;
    if let Some(sent) = sent.as_deref() {
        note_response(sent, response.headers());
    }
    Ok(response)
}

/// `.send_noting()` no lugar de `.send()`: o mesmo [`send`], no formato de
/// método, para trocar a chamada sem reescrever a montagem do pedido.
pub trait SendNoting {
    fn send_noting(
        self,
    ) -> impl std::future::Future<Output = Result<reqwest::Response, reqwest::Error>> + Send;
}

impl SendNoting for reqwest::RequestBuilder {
    fn send_noting(
        self,
    ) -> impl std::future::Future<Output = Result<reqwest::Response, reqwest::Error>> + Send {
        send(self)
    }
}

/// Tira (e devolve) o cookie novo guardado para `sent`, se houver.
pub fn take_rotated(sent: &str) -> Option<String> {
    ROTATED.lock().ok()?.remove(sent)
}
