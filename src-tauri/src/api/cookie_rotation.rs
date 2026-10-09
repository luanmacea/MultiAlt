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

#[cfg(test)]
mod cookie_rotation_tests {
    use super::*;
    use reqwest::header::HeaderValue;

    fn long_cookie(tag: &str) -> String {
        format!("_|WARNING:-DO-NOT-SHARE-THIS.--{}-{}", tag, "A".repeat(80))
    }

    fn set_cookies(lines: &[&str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for line in lines {
            headers.append(SET_COOKIE, HeaderValue::from_str(line).unwrap());
        }
        headers
    }

    #[test]
    fn a_new_roblosecurity_is_picked_among_other_cookies() {
        let new = long_cookie("new");
        let headers = set_cookies(&[
            "RBXEventTrackerV2=abc; path=/",
            &format!(".ROBLOSECURITY={new}; domain=.roblox.com; path=/; HttpOnly; Secure"),
        ]);
        assert_eq!(rotated_cookie_in(&headers).as_deref(), Some(new.as_str()));
    }

    #[test]
    fn no_set_cookie_means_no_rotation() {
        assert_eq!(rotated_cookie_in(&HeaderMap::new()), None);
        assert_eq!(rotated_cookie_in(&set_cookies(&["RBXSessionTracker=x; path=/"])), None);
    }

    #[test]
    fn malformed_values_are_ignored() {
        for line in [
            ".ROBLOSECURITY=; domain=.roblox.com; path=/",
            ".ROBLOSECURITY=short; path=/",
            ".ROBLOSECURITY",
            &format!(".ROBLOSECURITY={} with space; path=/", long_cookie("x")),
            &format!(".ROBLOSECURITY=\"{}\"; path=/", long_cookie("q")),
        ] {
            assert_eq!(parse_set_cookie(line), None, "should ignore: {line}");
        }
    }

    /// É assim que o Roblox apaga o cookie ao deslogar: gravar isso por cima
    /// da conta seria trocar uma sessão boa por nada.
    #[test]
    fn a_deletion_cookie_is_not_a_rotation() {
        let v = long_cookie("del");
        assert_eq!(parse_set_cookie(&format!(".ROBLOSECURITY={v}; Max-Age=0; path=/")), None);
        assert_eq!(
            parse_set_cookie(&format!(".ROBLOSECURITY={v}; expires=Thu, 01 Jan 1970 00:00:01 GMT")),
            None
        );
        assert_eq!(
            parse_set_cookie(&format!(".ROBLOSECURITY={v}; expires=Fri, 01 Jan 2055 00:00:00 GMT")).as_deref(),
            Some(v.as_str())
        );
    }

    #[test]
    fn the_sent_token_is_read_from_the_cookie_header() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static("a=1; .ROBLOSECURITY=TOKEN-X; b=2"));
        assert_eq!(sent_token(&headers).as_deref(), Some("TOKEN-X"));
        assert_eq!(sent_token(&HeaderMap::new()), None);
    }

    #[test]
    fn a_rotation_is_kept_per_sent_cookie_and_taken_once() {
        let new = long_cookie("kept");
        let headers = set_cookies(&[&format!(".ROBLOSECURITY={new}; path=/")]);
        note_response("rotation-old-1", &headers);

        assert_eq!(take_rotated("rotation-other"), None);
        assert_eq!(take_rotated("rotation-old-1").as_deref(), Some(new.as_str()));
        assert_eq!(take_rotated("rotation-old-1"), None, "taken once");
    }

    #[test]
    fn the_same_cookie_coming_back_is_not_a_rotation() {
        let same = long_cookie("same");
        note_response(&same, &set_cookies(&[&format!(".ROBLOSECURITY={same}; path=/")]));
        assert_eq!(take_rotated(&same), None);
    }

    #[test]
    fn a_call_without_an_account_records_nothing() {
        let new = long_cookie("anon");
        note_response("", &set_cookies(&[&format!(".ROBLOSECURITY={new}; path=/")]));
        assert_eq!(take_rotated(""), None);
    }
}
