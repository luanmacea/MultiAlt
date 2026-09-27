//! O teto de tempo de toda chamada HTTP do app, num lugar só.
//!
//! Antes daqui cada chamada construía o seu `reqwest::Client::new()`, que **não
//! tem teto nenhum**: uma conexão pendurada (handshake que nunca fecha) ou um
//! endpoint do Roblox que aceita e não responde prendia a chamada pelo tempo da
//! pilha de rede, não do app. No caminho de launch isso é pior que lentidão: a
//! sequência de launch é reservada (um launch por vez, ver
//! `docs/features/multi-launch.md`), então **uma** conta em voo presa num
//! endpoint mudo faz o app recusar todo launch novo por tempo indeterminado.
//!
//! Os dois tetos são deliberadamente separados:
//!
//! * [`CONNECT_TIMEOUT`] fecha o caso "conexão pendurada" com bound próprio e
//!   mais curto. `timeout` sozinho também cobriria o handshake, mas só no fim do
//!   teto total — o que no cliente de download (3 min) significaria três minutos
//!   parado num socket que nunca falou TLS.
//! * [`REQUEST_TIMEOUT`] fecha a requisição inteira (conexão + resposta + corpo).
//!
//! O download de build do Roblox tem teto **próprio e maior**
//! ([`DOWNLOAD_REQUEST_TIMEOUT`]): ele é legitimamente longo (centenas de MB em
//! zips) e o valor de uma chamada de API o quebraria.
//!
//! A URL continua vindo de [`crate::api::endpoints`]: este módulo só configura o
//! cliente, nunca monta endereço. Trocar o cliente compartilhado não pode mudar
//! isso, senão os testes mockados deixam de valer.

use std::time::Duration;

/// Teto do handshake (DNS + TCP + TLS).
///
/// Um handshake frio com uma borda do Roblox fecha em bem menos de 1 s; 10 s é
/// ~20x isso, o que aguenta DNS lento, VPN ou proxy no caminho sem recusar um
/// launch que ia funcionar. É o mesmo valor que `chromium/download.rs` já usava
/// para o mesmo problema.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Teto da requisição inteira de uma chamada de API (conexão, resposta e corpo).
///
/// As chamadas do caminho de launch (auth ticket, private join/VIP, listagem de
/// servidores) são JSON pequeno e respondem em centenas de milissegundos; 30 s é
/// ~30x a latência típica, com folga para um endpoint degradado que ainda
/// responde, e é o valor que `chromium/download.rs` e `commands/generators.rs`
/// já usam. Curto demais aqui transformaria launch que funcionava em launch que
/// falha — daí a folga grande.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Teto da requisição de download de build do Roblox.
///
/// É o valor que `platform/windows/versions.rs` já praticava, preservado de
/// propósito: cada zip de uma build pode passar de 100 MB e um teto de chamada
/// de API cortaria um download que estava indo bem. O que muda é o handshake,
/// que agora tem o [`CONNECT_TIMEOUT`] em vez de esperar estes 3 minutos.
pub const DOWNLOAD_REQUEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Builder com teto explícito. Quem tem motivo para um teto diferente (o
/// download, a consulta de canal) passa o seu; todo o resto usa [`builder`].
pub fn builder_with(connect: Duration, total: Duration) -> reqwest::ClientBuilder {
    let (connect, total) = effective_timeouts(connect, total);
    reqwest::Client::builder()
        .connect_timeout(connect)
        .timeout(total)
}

/// Builder das chamadas de API do Roblox.
pub fn builder() -> reqwest::ClientBuilder {
    builder_with(CONNECT_TIMEOUT, REQUEST_TIMEOUT)
}

/// Builder do download de build (teto próprio e maior).
pub fn download_builder() -> reqwest::ClientBuilder {
    builder_with(CONNECT_TIMEOUT, DOWNLOAD_REQUEST_TIMEOUT)
}

/// O cliente das chamadas de API sem exigência própria de user agent ou
/// redirect — o substituto direto de `reqwest::Client::new()`.
///
/// `build()` só falha se o backend de TLS não inicializar, e nesse caso cair
/// para um cliente **sem teto** seria justamente o defeito que este módulo
/// existe para fechar; por isso aqui é `expect` e não um fallback silencioso.
pub fn client() -> reqwest::Client {
    builder().build().expect("TLS backend for the Roblox API client")
}

/// Transforma o erro de transporte do `reqwest` em frase que dá para mostrar.
///
/// Timeout é o caso que importa: o `Display` do `reqwest::Error` para um teto
/// estourado é `error sending request for url (…)`, e o "operation timed out"
/// fica escondido na cadeia de `source` — ou seja, o usuário lia um texto cru
/// que não dizia o que aconteceu. Os outros erros continuam com o mesmo
/// `Request failed: …` de antes, de propósito: só o timeout ganhou frase.
pub fn describe_error(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        return if err.is_connect() {
            "Could not reach Roblox: the connection timed out. Check your internet \
             connection (VPN or proxy included) and try again."
                .to_string()
        } else {
            "Roblox took too long to answer and the request was cancelled. Check your \
             internet connection and try again."
                .to_string()
        };
    }
    format!("Request failed: {}", err)
}

/// Em produção o teto é o que o chamador pediu. Nos testes ele pode ser
/// encurtado (ver [`test_support`]) para provar que uma chamada pendurada é
/// cortada sem o teste levar 30 s.
#[cfg(not(test))]
fn effective_timeouts(connect: Duration, total: Duration) -> (Duration, Duration) {
    (connect, total)
}

#[cfg(test)]
fn effective_timeouts(connect: Duration, total: Duration) -> (Duration, Duration) {
    match test_support::override_in_effect() {
        Some((c, t)) => (c.unwrap_or(connect), t.unwrap_or(total)),
        None => (connect, total),
    }
}

/// Encurtamento do teto para os testes, e o lock que os serializa.
///
/// O override é process-wide (como `endpoints::BASE`), então dois testes de
/// timeout rodando juntos veriam o teto um do outro; [`gate`] existe para isso.
/// Testes de outros módulos não pegam o lock, mas eles batem em `wiremock` no
/// localhost e respondem em poucos milissegundos — ordens de magnitude abaixo do
/// menor override usado aqui.
#[cfg(test)]
pub mod test_support {
    use super::Duration;
    use std::sync::Mutex;

    type Override = Option<(Option<Duration>, Option<Duration>)>;

    static OVERRIDE: Mutex<Override> = Mutex::new(None);
    static GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// Serializa os testes que mexem no override.
    pub async fn gate() -> tokio::sync::MutexGuard<'static, ()> {
        GATE.lock().await
    }

    pub(super) fn override_in_effect() -> Override {
        OVERRIDE.lock().ok().and_then(|v| *v)
    }

    /// Restaura o teto anterior quando sai de escopo — inclusive se o teste
    /// falhar no meio, senão um `assert!` deixaria o override ligado para o
    /// resto do binário.
    pub struct TimeoutOverride;

    impl Drop for TimeoutOverride {
        fn drop(&mut self) {
            if let Ok(mut slot) = OVERRIDE.lock() {
                *slot = None;
            }
        }
    }

    /// Encurta o teto dos clientes construídos daqui para a frente. `None` em um
    /// dos campos mantém o valor que o chamador de produção pediu.
    pub fn shorten(connect: Option<Duration>, total: Option<Duration>) -> TimeoutOverride {
        if let Ok(mut slot) = OVERRIDE.lock() {
            *slot = Some((connect, total));
        }
        TimeoutOverride
    }
}

/// Prova que o teto corta de verdade: uma resposta pendurada, uma conexão
/// pendurada, e o mesmo teto valendo no caminho de launch (auth ticket).
///
/// Todos pegam [`test_support::gate`] porque o encurtamento do teto é
/// process-wide.
#[cfg(test)]
mod http_timeout_tests {
    use super::*;
    use crate::api::endpoints;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server};
    use std::time::Instant;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    /// Um endpoint que aceita a requisição e demora **mais** que o teto: o
    /// cliente tem de desistir, e a mensagem tem de ser frase, não texto cru da
    /// biblioteca.
    #[tokio::test]
    async fn a_hung_response_is_cut_by_the_request_ceiling() {
        let _gate = test_support::gate().await;
        let server = mock_server().await;
        Mock::given(method("GET"))
            .and(path(mock_path("apis", "/test/hung-response")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("late")
                    .set_delay(Duration::from_secs(10)),
            )
            .mount(server)
            .await;

        let client = builder_with(CONNECT_TIMEOUT, Duration::from_millis(400))
            .build()
            .expect("client");

        let started = Instant::now();
        let err = client
            .get(format!("{}/test/hung-response", endpoints::host("apis")))
            .send()
            .await
            .expect_err("the ceiling must cut the hung response");

        assert!(err.is_timeout(), "expected a timeout, got: {}", err);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the call waited {:?}, so no ceiling cut it",
            started.elapsed()
        );
        let message = describe_error(&err);
        assert_eq!(
            message,
            "Roblox took too long to answer and the request was cancelled. Check your \
             internet connection and try again."
        );
        assert!(
            !message.contains("error sending request"),
            "raw reqwest text leaked to the user: {}",
            message
        );
    }

    /// Handshake que nunca completa: o socket aceita a conexão TCP e nunca fala
    /// TLS. `connect_timeout` é quem fecha esse caso com bound próprio — o teto
    /// total aqui é 10 s e a chamada não pode esperar por ele.
    #[tokio::test]
    async fn a_hung_handshake_is_cut_by_the_connect_ceiling() {
        let _gate = test_support::gate().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let port = listener.local_addr().expect("addr").port();
        // Aceita e nunca responde: o cliente fica preso no TLS.
        let silent = tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((socket, _)) = listener.accept().await {
                held.push(socket);
            }
        });

        let client = builder_with(Duration::from_millis(400), Duration::from_secs(10))
            .build()
            .expect("client");

        let started = Instant::now();
        let err = client
            .get(format!("https://127.0.0.1:{}/never-answers", port))
            .send()
            .await
            .expect_err("the connect ceiling must cut the hung handshake");

        silent.abort();
        assert!(err.is_timeout(), "expected a timeout, got: {}", err);
        assert!(err.is_connect(), "expected a connect error, got: {}", err);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the handshake waited {:?}, so connect_timeout did not cut it",
            started.elapsed()
        );
        assert_eq!(
            describe_error(&err),
            "Could not reach Roblox: the connection timed out. Check your internet \
             connection (VPN or proxy included) and try again."
        );
    }

    /// O teste que pega a regressão de verdade: o pedido de **auth ticket** — a
    /// primeira chamada de rede de todo launch — tem de herdar o teto do
    /// builder compartilhado. Tirar o `.timeout()` de `api::auth::build_client`
    /// faz este teste esperar o mock inteiro e reprovar.
    #[tokio::test]
    async fn the_auth_ticket_request_inherits_the_ceiling() {
        let _gate = test_support::gate().await;
        let server = mock_server().await;
        let _short = test_support::shorten(None, Some(Duration::from_millis(400)));

        Mock::given(method("POST"))
            .and(path(mock_path("auth", "/v1/authentication-ticket/")))
            .and(header("cookie", cookie_of("hung-ticket-account")))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("x-csrf-token", "csrf-never-arrives")
                    .set_delay(Duration::from_secs(10)),
            )
            .mount(server)
            .await;

        let started = Instant::now();
        let err = crate::api::auth::get_auth_ticket("hung-ticket-account")
            .await
            .expect_err("a hung auth ticket must fail, not hold the launch queue");

        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the auth ticket waited {:?}: the launch path has no ceiling",
            started.elapsed()
        );
        assert_eq!(
            err,
            "Roblox took too long to answer and the request was cancelled. Check your \
             internet connection and try again."
        );
    }

    /// O download de build não pode herdar o teto de uma chamada de API — ele é
    /// legitimamente longo. Mas também não pode ficar sem teto.
    #[test]
    fn the_build_download_has_its_own_larger_ceiling() {
        assert!(
            DOWNLOAD_REQUEST_TIMEOUT > REQUEST_TIMEOUT,
            "a build download cut by the API ceiling would never finish"
        );
    }
}
