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

/// O teto é o que o chamador pediu.
fn effective_timeouts(connect: Duration, total: Duration) -> (Duration, Duration) {
    (connect, total)
}
