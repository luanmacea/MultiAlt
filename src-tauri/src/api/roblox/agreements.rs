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
