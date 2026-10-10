// Disponibilidade de um nome de usuário no cadastro.
//
// `GET auth/v2/usernames/validate` responde sem autenticação e diz exatamente o
// que o formulário diria depois de o usuário resolver o CAPTCHA:
//
// ```json
// { "code": 0, "message": "Username is valid" }
// { "code": 1, "message": "Username is already in use" }
// ```
//
// Perguntar antes vale a viagem: descobrir que o nome está em uso só na hora do
// envio queima o CAPTCHA que a pessoa acabou de resolver.

/// Resposta da validação, já interpretada.
#[derive(Debug, Clone, PartialEq)]
pub struct UsernameCheck {
    pub available: bool,
    /// Mensagem do Roblox, para a UI poder mostrar o motivo real.
    pub message: String,
}

/// Lê a resposta da validação.
///
/// `code == 0` é o único "pode usar". Qualquer outro código (em uso, curto
/// demais, filtrado) é recusa, e a mensagem do Roblox vai junto.
pub fn parse_username_check(body: &serde_json::Value) -> UsernameCheck {
    let code = body["code"].as_i64();
    let message = body["message"]
        .as_str()
        .filter(|m| !m.trim().is_empty())
        .unwrap_or("Username is not available")
        .to_string();
    UsernameCheck {
        available: code == Some(0),
        message,
    }
}

/// Data de nascimento no formato que o endpoint espera (`1991-02-19T00:00:00.000Z`).
///
/// Mês vem em três letras em inglês (é o `value` do `<select>` do cadastro).
pub fn signup_birthday_iso(day: &str, month: &str, year: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month_number = MONTHS
        .iter()
        .position(|m| m.eq_ignore_ascii_case(month.trim()))
        .map(|i| i + 1)
        .unwrap_or(1);
    let day: u32 = day.trim().parse().unwrap_or(1);
    format!("{}-{:02}-{:02}T00:00:00.000Z", year.trim(), month_number, day)
}

/// O nome está livre para cadastro?
///
/// Falha de rede devolve `Err`: quem chama decide se segue assim mesmo (é o que
/// a sessão de cadastro faz — o formulário ainda vai validar).
pub async fn check_signup_username(
    username: &str,
    birthday_iso: &str,
) -> Result<UsernameCheck, String> {
    let url = format!(
        "{}/v2/usernames/validate?request.username={}&request.birthday={}&request.context=Signup",
        endpoints::host("auth"),
        urlencoding::encode(username),
        urlencoding::encode(birthday_iso)
    );

    let response = http_client::client()
        .get(&url)
        .send_noting()
        .await
        .map_err(|e| http_client::describe_error(&e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to validate username (status {})",
            response.status().as_u16()
        ));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse username validation: {}", e))?;
    Ok(parse_username_check(&body))
}

#[cfg(test)]
mod username_check_tests {
    use super::*;

    #[test]
    fn code_zero_is_the_only_available() {
        let check = parse_username_check(&serde_json::json!({
            "code": 0,
            "message": "Username is valid"
        }));
        assert!(check.available);
        assert_eq!(check.message, "Username is valid");
    }

    #[test]
    fn a_name_in_use_is_reported_with_the_roblox_message() {
        let check = parse_username_check(&serde_json::json!({
            "code": 1,
            "message": "Username is already in use"
        }));
        assert!(!check.available);
        assert_eq!(check.message, "Username is already in use");
    }

    /// Código desconhecido (filtro, tamanho) é recusa, não "pode usar".
    #[test]
    fn an_unknown_code_is_a_refusal() {
        assert!(!parse_username_check(&serde_json::json!({ "code": 2, "message": "Filtered" })).available);
        assert!(!parse_username_check(&serde_json::json!({})).available);
    }

    #[test]
    fn a_missing_message_still_says_something() {
        let check = parse_username_check(&serde_json::json!({ "code": 1 }));
        assert_eq!(check.message, "Username is not available");
    }

    #[test]
    fn the_birthday_is_formatted_the_way_the_endpoint_wants() {
        assert_eq!(
            signup_birthday_iso("19", "Feb", "1991"),
            "1991-02-19T00:00:00.000Z"
        );
        assert_eq!(
            signup_birthday_iso("07", "Dec", "2001"),
            "2001-12-07T00:00:00.000Z"
        );
    }

    /// Mês desconhecido não pode montar uma data inválida.
    #[test]
    fn an_unknown_month_falls_back_to_january() {
        assert_eq!(
            signup_birthday_iso("1", "Xxx", "2000"),
            "2000-01-01T00:00:00.000Z"
        );
    }
}

#[cfg(test)]
mod username_check_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{mock_path, mock_server};
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, ResponseTemplate};

    #[tokio::test]
    async fn an_available_name_comes_back_available() {
        Mock::given(method("GET"))
            .and(path(mock_path("auth", "/v2/usernames/validate")))
            .and(query_param("request.username", "Livre_Bison7411"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "Username is valid"
            })))
            .mount(mock_server().await)
            .await;

        let check = check_signup_username("Livre_Bison7411", "1991-02-19T00:00:00.000Z")
            .await
            .expect("check");
        assert!(check.available);
    }

    #[tokio::test]
    async fn a_taken_name_comes_back_unavailable() {
        Mock::given(method("GET"))
            .and(path(mock_path("auth", "/v2/usernames/validate")))
            .and(query_param("request.username", "Ultra_Lynx744"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1,
                "message": "Username is already in use"
            })))
            .mount(mock_server().await)
            .await;

        let check = check_signup_username("Ultra_Lynx744", "1991-02-19T00:00:00.000Z")
            .await
            .expect("check");
        assert!(!check.available);
        assert_eq!(check.message, "Username is already in use");
    }

    #[tokio::test]
    async fn a_failing_endpoint_is_an_error_not_a_silent_yes() {
        Mock::given(method("GET"))
            .and(path(mock_path("auth", "/v2/usernames/validate")))
            .and(query_param("request.username", "Erro_Bison1"))
            .respond_with(ResponseTemplate::new(503))
            .mount(mock_server().await)
            .await;

        let err = check_signup_username("Erro_Bison1", "1991-02-19T00:00:00.000Z")
            .await
            .unwrap_err();
        assert!(err.contains("status 503"), "erro inesperado: {}", err);
    }
}
