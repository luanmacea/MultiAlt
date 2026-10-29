// Cópia de credencial (cookie, senha, user:pass, user:pass:cookie) para a área de transferência.
//
// No Windows o texto vai marcado para não entrar no histórico do Win+V nem na
// nuvem e é apagado depois de `SECRET_CLEAR_SECS` — só se ninguém tiver copiado
// outra coisa (ver `platform/windows/clipboard.rs`, que só escreve e nunca lê).
// O texto sai do store, não do snapshot da tela: quando o Roblox troca o cookie
// (`api::cookie_rotation`), o snapshot do frontend fica velho.

/// Depois de quanto tempo o app apaga a credencial copiada.
const SECRET_CLEAR_SECS: u64 = 30;

/// Resposta para quem não tem a cópia protegida (macOS): o frontend reconhece
/// este texto exato e copia pelo `navigator.clipboard`, sem limpeza.
#[cfg_attr(target_os = "windows", allow(dead_code))]
const CLIPBOARD_UNSUPPORTED: &str = "CLIPBOARD_UNSUPPORTED";

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SecretCopyResult {
    count: usize,
    clears_in_secs: Option<u64>,
}

/// As linhas a copiar, na ordem dos ids pedidos. Cookie e senha vazios ficam
/// de fora (conta sem sessão não tem o que copiar); id que não existe é
/// ignorado.
fn secret_lines(
    accounts: &[data::accounts::Account],
    user_ids: &[i64],
    kind: &str,
) -> Result<Vec<String>, String> {
    if !matches!(kind, "cookie" | "password" | "userpass" | "userpasscookie") {
        return Err(format!("Unknown credential kind: {kind}"));
    }
    Ok(user_ids
        .iter()
        .filter_map(|id| accounts.iter().find(|a| a.user_id == *id))
        .filter_map(|a| {
            let line = match kind {
                "cookie" => a.security_token.clone(),
                "password" => a.password.clone(),
                "userpasscookie" => {
                    format!("{}:{}:{}", a.username, a.password, a.security_token)
                }
                _ => format!("{}:{}", a.username, a.password),
            };
            (!line.trim().is_empty()).then_some(line)
        })
        .collect())
}

#[tauri::command]
async fn copy_account_secret(
    state: tauri::State<'_, AccountStore>,
    user_ids: Vec<i64>,
    kind: String,
) -> Result<SecretCopyResult, String> {
    let lines = secret_lines(&state.get_all()?, &user_ids, &kind)?;
    if lines.is_empty() {
        return Ok(SecretCopyResult { count: 0, clears_in_secs: None });
    }
    copy_protected(lines).await
}

#[cfg(target_os = "windows")]
async fn copy_protected(lines: Vec<String>) -> Result<SecretCopyResult, String> {
    let count = lines.len();
    let text = lines.join("\n");
    let sequence = tokio::task::spawn_blocking(move || platform::windows::copy_secret_text(&text))
        .await
        .map_err(|e| e.to_string())??;
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(SECRET_CLEAR_SECS)).await;
        let _ = tokio::task::spawn_blocking(move || {
            platform::windows::clear_secret_if_unchanged(sequence)
        })
        .await;
    });
    Ok(SecretCopyResult { count, clears_in_secs: Some(SECRET_CLEAR_SECS) })
}

#[cfg(not(target_os = "windows"))]
async fn copy_protected(_lines: Vec<String>) -> Result<SecretCopyResult, String> {
    Err(CLIPBOARD_UNSUPPORTED.to_string())
}
