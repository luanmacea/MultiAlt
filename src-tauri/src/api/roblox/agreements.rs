// Acordos (Termos de Uso / Política de Privacidade) que o Roblox pede para a
// conta aceitar: o aviso "Acordos atualizados" com o botão "Concordo".
//
// O que se sabe — e o quanto se confia (fonte: nenhuma documentação oficial;
// posts do DevForum sobre o `UserAgreementsUpdatedPopup` e o crate `roblox-api`):
// - `GET apis.roblox.com/user-agreements/v1/agreements-resolution/Web` lista o
//   que a conta ainda precisa aceitar; lista vazia = nenhum aviso. Confiança
//   média: o caminho é confirmado, o **formato da resposta não** — por isso a
//   leitura abaixo é tolerante (id e título são procurados em vários nomes de
//   campo, e o que não entendemos vai inteiro para o Console).
// - `POST apis.roblox.com/user-agreements/v1/acceptances` é o que o botão
//   "Concordo" chama. Corpo: `{"acceptances":[{"agreementId":"…"}]}` (crate
//   `roblox-api` 0.1.5); o DevForum só diz "acceptances = [ids]". Confiança
//   média-baixa: se o Roblox responder 400, repetimos uma vez com os ids soltos
//   e devolvemos **cada** resposta como veio.
//
// A leitura nunca renova sessão (`read_without_refresh` em quem chama) e o
// POST só roda quando a pessoa pede.

/// Um acordo que a conta ainda não aceitou.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingAgreement {
    /// Vazio quando o Roblox não deu um id que reconhecemos (aí o aceite pela
    /// API não é tentado; o navegador resolve).
    pub id: String,
    /// Título para a tela; sem título reconhecido, o id (ou "Agreement").
    pub title: String,
}

/// Resultado da leitura: o que está pendente e, se alguma parte da resposta não
/// foi entendida, o JSON cru (cortado) para o Console.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgreementsCheck {
    pub pending: Vec<PendingAgreement>,
    pub unrecognized: Option<String>,
}

/// Uma tentativa de aceite e a resposta exata do Roblox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptAttempt {
    /// Qual corpo foi enviado ("objects" ou "ids").
    pub shape: String,
    pub status: u16,
    /// Corpo da resposta, cortado em [`AGREEMENT_BODY_LIMIT`] caracteres.
    pub body: String,
}

const AGREEMENT_BODY_LIMIT: usize = 400;

/// Campos onde o id do acordo pode vir, do mais provável ao menos.
const AGREEMENT_ID_KEYS: [&str; 4] = ["agreementId", "id", "agreementID", "agreement_id"];
/// Campos onde um título legível pode vir.
const AGREEMENT_TITLE_KEYS: [&str; 8] = [
    "title",
    "name",
    "displayName",
    "agreementName",
    "agreementType",
    "documentType",
    "type",
    "agreementTypeName",
];
/// Campos que podem embrulhar a lista (`{"agreements":[…]}`).
const AGREEMENT_LIST_KEYS: [&str; 5] = ["agreements", "results", "pendingAgreements", "data", "items"];

fn clip(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= limit {
        trimmed.to_string()
    } else {
        let cut: String = trimmed.chars().take(limit).collect();
        format!("{cut}…")
    }
}

fn agreement_text(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| match value.get(*key) {
        Some(serde_json::Value::String(s)) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        _ => None,
    })
}

/// Lê uma entrada da lista; o `bool` diz se id e título foram achados. `None` = a entrada não é string nem objeto.
fn read_agreement(entry: &serde_json::Value) -> Option<(PendingAgreement, bool)> {
    match entry {
        serde_json::Value::String(id) if !id.trim().is_empty() => Some((
            PendingAgreement { id: id.trim().to_string(), title: id.trim().to_string() },
            true,
        )),
        serde_json::Value::Object(_) => {
            let id = agreement_text(entry, &AGREEMENT_ID_KEYS);
            let title = agreement_text(entry, &AGREEMENT_TITLE_KEYS);
            // "Entendido" = achamos o id e um título; faltando um, a tela ainda
            // mostra algo, mas o Console recebe o JSON cru.
            let understood = id.is_some() && title.is_some();
            Some((
                PendingAgreement {
                    title: title.or_else(|| id.clone()).unwrap_or_else(|| "Agreement".to_string()),
                    id: id.unwrap_or_default(),
                },
                understood,
            ))
        }
        _ => None,
    }
}

/// Resume o corpo do `agreements-resolution`. Corpo vazio, `null`, `[]` e `{}`
/// são "nada pendente"; uma forma que não conhecemos (objeto cheio sem lista)
/// é **erro**, nunca "nada pendente".
fn parse_agreements(text: &str) -> Result<AgreementsCheck, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(AgreementsCheck { pending: Vec::new(), unrecognized: None });
    }
    let value: serde_json::Value = serde_json::from_str(trimmed)
        .map_err(|_| format!("Unexpected agreements response: {}", clip(trimmed, 160)))?;

    let list: &Vec<serde_json::Value> = match &value {
        serde_json::Value::Null => return Ok(AgreementsCheck { pending: Vec::new(), unrecognized: None }),
        serde_json::Value::Array(list) => list,
        serde_json::Value::Object(map) if map.is_empty() => {
            return Ok(AgreementsCheck { pending: Vec::new(), unrecognized: None });
        }
        serde_json::Value::Object(map) => {
            match AGREEMENT_LIST_KEYS.iter().find_map(|key| map.get(*key).and_then(|v| v.as_array())) {
                Some(list) => list,
                None => return Err(format!("Unexpected agreements response: {}", clip(trimmed, 160))),
            }
        }
        _ => return Err(format!("Unexpected agreements response: {}", clip(trimmed, 160))),
    };

    let mut pending = Vec::new();
    let mut all_understood = true;
    for entry in list {
        match read_agreement(entry) {
            Some((agreement, understood)) => {
                all_understood &= understood;
                pending.push(agreement);
            }
            None => all_understood = false,
        }
    }
    // Entrada que não é string nem objeto: há algo pendente que não sabemos ler.
    if pending.len() < list.len() {
        pending.push(PendingAgreement { id: String::new(), title: "Agreement".to_string() });
    }
    Ok(AgreementsCheck {
        pending,
        unrecognized: (!all_understood).then(|| clip(trimmed, 600)),
    })
}

/// Lista o que a conta ainda precisa aceitar. Leitura pura (GET, sem CSRF).
pub async fn get_pending_agreements(
    security_token: &str,
    client: &reqwest::Client,
) -> Result<AgreementsCheck, String> {
    let response = client
        .get(format!("{}/user-agreements/v1/agreements-resolution/Web", endpoints::host("apis")))
        .header(COOKIE, cookie_header(security_token))
        .header("Accept", "application/json")
        .send_noting()
        .await
        .map_err(|e| http_client::describe_error(&e))?;

    match response.status().as_u16() {
        200..=299 => {}
        401 => return Err("Agreements check failed (status 401)".to_string()),
        429 => return Err("Roblox is limiting requests right now. Try again in a minute.".to_string()),
        status => return Err(format!("Agreements check failed (status {status})")),
    }
    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read the agreements response: {e}"))?;
    parse_agreements(&text)
}

/// Corpo do POST para cada forma conhecida.
fn acceptance_body(shape: &str, ids: &[String]) -> serde_json::Value {
    match shape {
        "ids" => serde_json::json!({ "acceptances": ids }),
        _ => serde_json::json!({
            "acceptances": ids.iter().map(|id| serde_json::json!({ "agreementId": id })).collect::<Vec<_>>()
        }),
    }
}

/// **Aceita** os acordos `ids` em nome da conta. Só roda por pedido explícito
/// (botão "Accept terms", depois da confirmação). Devolve a resposta de cada
/// tentativa como o Roblox mandou; a segunda forma de corpo só é tentada se a
/// primeira levar 400. `Err` = não chegou a enviar (cookie/CSRF/rede).
pub async fn accept_agreements(
    security_token: &str,
    ids: &[String],
) -> Result<Vec<AcceptAttempt>, String> {
    if ids.is_empty() || ids.iter().any(|id| id.trim().is_empty()) {
        return Err("Roblox did not give an id for the agreement.".to_string());
    }
    let csrf = crate::api::auth::get_csrf_token(security_token).await?;
    let client = http_client::client();
    let mut attempts = Vec::new();
    for shape in ["objects", "ids"] {
        let request = client
            .post(format!("{}/user-agreements/v1/acceptances", endpoints::host("apis")))
            .header(COOKIE, cookie_header(security_token))
            .header("Referer", format!("{}/", endpoints::host("www")))
            .json(&acceptance_body(shape, ids));
        let response = crate::api::auth::send_with_csrf_retry(request, &csrf).await?;
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        attempts.push(AcceptAttempt { shape: shape.to_string(), status, body: clip(&body, AGREEMENT_BODY_LIMIT) });
        if status != 400 {
            break;
        }
    }
    Ok(attempts)
}

#[cfg(test)]
mod agreements_http_tests {
    use super::*;
    use crate::api::endpoints::test_support::{cookie_of, mock_path, mock_server, mount_csrf};
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    async fn mount_get(token: &str, response: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path(mock_path("apis", "/user-agreements/v1/agreements-resolution/Web")))
            .and(header("cookie", cookie_of(token)))
            .respond_with(response)
            .mount(mock_server().await)
            .await;
    }

    async fn pending_of(token: &str) -> Result<AgreementsCheck, String> {
        get_pending_agreements(token, &http_client::client()).await
    }

    #[tokio::test]
    async fn an_empty_list_means_nothing_is_pending() {
        mount_get("agr-none", ResponseTemplate::new(200).set_body_json(serde_json::json!([]))).await;
        let check = pending_of("agr-none").await.unwrap();
        assert!(check.pending.is_empty());
        assert_eq!(check.unrecognized, None);
    }

    #[tokio::test]
    async fn an_empty_body_or_object_or_null_is_nothing_pending() {
        mount_get("agr-blank", ResponseTemplate::new(200)).await;
        mount_get("agr-obj", ResponseTemplate::new(200).set_body_json(serde_json::json!({}))).await;
        mount_get("agr-null", ResponseTemplate::new(200).set_body_string("null")).await;
        for token in ["agr-blank", "agr-obj", "agr-null"] {
            assert!(pending_of(token).await.unwrap().pending.is_empty(), "{token}");
        }
    }

    #[tokio::test]
    async fn pending_agreements_come_with_id_and_title() {
        mount_get(
            "agr-two",
            ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "agreementId": "a-111", "title": "Terms of Use", "version": 7 },
                { "agreementId": "a-222", "title": "Privacy Policy" }
            ])),
        )
        .await;
        let check = pending_of("agr-two").await.unwrap();
        assert_eq!(
            check.pending,
            vec![
                PendingAgreement { id: "a-111".into(), title: "Terms of Use".into() },
                PendingAgreement { id: "a-222".into(), title: "Privacy Policy".into() },
            ]
        );
        assert_eq!(check.unrecognized, None);
    }

    /// O formato real não é conhecido: id sob outro nome, lista embrulhada,
    /// entrada só com o id — tudo é lido, e o que faltou vai cru para o Console.
    #[tokio::test]
    async fn unknown_field_names_are_read_tolerantly_and_reported_raw() {
        mount_get(
            "agr-alt",
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "agreements": [ { "id": "x-1", "documentType": "PrivacyPolicy" }, { "id": "x-2" } ]
            })),
        )
        .await;
        let check = pending_of("agr-alt").await.unwrap();
        assert_eq!(check.pending[0], PendingAgreement { id: "x-1".into(), title: "PrivacyPolicy".into() });
        assert_eq!(check.pending[1], PendingAgreement { id: "x-2".into(), title: "x-2".into() });
        assert!(check.unrecognized.is_some(), "an entry without a title is logged raw");
    }

    #[tokio::test]
    async fn an_entry_without_any_id_is_pending_but_cannot_be_accepted_by_api() {
        mount_get(
            "agr-noid",
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{ "something": "else" }])),
        )
        .await;
        let check = pending_of("agr-noid").await.unwrap();
        assert_eq!(check.pending.len(), 1);
        assert_eq!(check.pending[0].id, "");
        assert!(check.unrecognized.is_some());
    }

    /// Forma desconhecida **nunca** vira "nada pendente".
    #[tokio::test]
    async fn an_unknown_shape_is_an_error_not_a_clean_account() {
        mount_get(
            "agr-weird",
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "mustAccept": true })),
        )
        .await;
        let err = pending_of("agr-weird").await.unwrap_err();
        assert!(err.contains("Unexpected agreements response"), "{err}");
        mount_get("agr-text", ResponseTemplate::new(200).set_body_string("<html>")).await;
        assert!(pending_of("agr-text").await.is_err());
    }

    #[tokio::test]
    async fn statuses_map_to_session_limit_and_other_errors() {
        mount_get("agr-401", ResponseTemplate::new(401)).await;
        mount_get("agr-429", ResponseTemplate::new(429)).await;
        mount_get("agr-503", ResponseTemplate::new(503)).await;
        assert!(pending_of("agr-401").await.unwrap_err().contains("status 401"));
        assert!(pending_of("agr-429").await.unwrap_err().contains("Try again"));
        assert!(pending_of("agr-503").await.unwrap_err().contains("status 503"));
    }

    #[test]
    fn the_two_known_post_bodies() {
        let ids = vec!["a-1".to_string(), "a-2".to_string()];
        assert_eq!(
            acceptance_body("objects", &ids),
            serde_json::json!({ "acceptances": [ { "agreementId": "a-1" }, { "agreementId": "a-2" } ] })
        );
        assert_eq!(acceptance_body("ids", &ids), serde_json::json!({ "acceptances": ["a-1", "a-2"] }));
    }

    #[tokio::test]
    async fn accepting_posts_the_ids_with_the_csrf_handshake_and_reports_the_answer() {
        mount_csrf("agr-post", "csrf-1").await;
        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/user-agreements/v1/acceptances")))
            .and(header("cookie", cookie_of("agr-post")))
            .and(header("x-csrf-token", "csrf-1"))
            .and(body_json(serde_json::json!({ "acceptances": [ { "agreementId": "a-9" } ] })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "results": [] })))
            .mount(mock_server().await)
            .await;
        let attempts = accept_agreements("agr-post", &["a-9".to_string()]).await.unwrap();
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0].status, 200);
        assert_eq!(attempts[0].shape, "objects");
    }

    /// 400 na primeira forma: tenta a segunda **uma vez** e devolve as duas respostas.
    #[tokio::test]
    async fn a_400_retries_once_with_bare_ids_and_reports_both_answers() {
        mount_csrf("agr-400", "csrf-2").await;
        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/user-agreements/v1/acceptances")))
            .and(header("cookie", cookie_of("agr-400")))
            .and(body_json(serde_json::json!({ "acceptances": [ { "agreementId": "a-4" } ] })))
            .respond_with(ResponseTemplate::new(400).set_body_string("{\"message\":\"bad shape\"}"))
            .mount(mock_server().await)
            .await;
        Mock::given(method("POST"))
            .and(path(mock_path("apis", "/user-agreements/v1/acceptances")))
            .and(header("cookie", cookie_of("agr-400")))
            .and(body_json(serde_json::json!({ "acceptances": ["a-4"] })))
            .respond_with(ResponseTemplate::new(403).set_body_string("{\"message\":\"nope\"}"))
            .mount(mock_server().await)
            .await;
        let attempts = accept_agreements("agr-400", &["a-4".to_string()]).await.unwrap();
        assert_eq!(attempts.len(), 2);
        assert_eq!((attempts[0].status, attempts[0].body.as_str()), (400, "{\"message\":\"bad shape\"}"));
        assert_eq!((attempts[1].shape.as_str(), attempts[1].status), ("ids", 403));
    }

    #[tokio::test]
    async fn accepting_without_a_known_id_sends_nothing() {
        assert!(accept_agreements("agr-empty", &[]).await.is_err());
        assert!(accept_agreements("agr-empty", &[String::new()]).await.is_err());
    }
}
