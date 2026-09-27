pub fn get_account_data_path() -> PathBuf {
    crate::data::settings::get_runtime_data_dir().join("AccountData.json")
}

#[tauri::command]
pub fn get_accounts(state: tauri::State<'_, AccountStore>) -> Result<Vec<Account>, String> {
    state.get_all()
}

#[tauri::command]
pub fn save_accounts(state: tauri::State<'_, AccountStore>) -> Result<(), String> {
    state.save()
}

#[tauri::command]
pub fn add_account(
    state: tauri::State<'_, AccountStore>,
    security_token: String,
    username: String,
    user_id: i64,
    password: Option<String>,
) -> Result<(), String> {
    let mut account = Account::new(security_token, username, user_id);
    if let Some(password) = password {
        account.password = password;
    }
    state.add(account)
}

#[tauri::command]
pub fn remove_account(state: tauri::State<'_, AccountStore>, user_id: i64) -> Result<bool, String> {
    state.remove(user_id)
}

#[tauri::command]
pub fn update_account(
    state: tauri::State<'_, AccountStore>,
    mut account: Account,
) -> Result<bool, String> {
    // The webview holds a snapshot that goes stale whenever the backend
    // rotates a cookie (session refresh, webserver SetField...). Editing an
    // alias/group from that snapshot must not write the old, invalidated
    // cookie back, so credentials always come from the store.
    if let Some(stored) = state
        .get_all()?
        .into_iter()
        .find(|a| a.user_id == account.user_id)
    {
        account.security_token = stored.security_token;
        account.password = stored.password;
    }
    state.update(account)
}

#[tauri::command]
pub fn unlock_accounts(
    state: tauri::State<'_, AccountStore>,
    password: String,
    remember_hours: Option<u64>,
) -> Result<(), String> {
    state.load_with_password(&password)?;

    // Só depois de destrancar: guardar uma senha que não abre nada seria pior
    // que não guardar nada.
    match remember_hours {
        Some(hours) if hours > 0 => {
            if let Err(e) = remember(&password, hours) {
                // O unlock valeu; o lembrete é conveniência.
                eprintln!("Não foi possível lembrar a senha: {}", e);
            }
        }
        _ => forget(),
    }
    Ok(())
}

/// Destranca com a senha lembrada, se houver uma válida.
///
/// Devolve `false` quando não há lembrete — a UI mostra a tela de senha. Um
/// lembrete que não destranca mais (senha trocada por fora, arquivo de outra
/// instalação) é **apagado**, para não ficar tentando para sempre.
#[tauri::command]
pub fn try_remembered_unlock(state: tauri::State<'_, AccountStore>) -> Result<bool, String> {
    let Some(password) = remembered_password() else {
        return Ok(false);
    };
    match state.load_with_password(&password) {
        Ok(()) => Ok(true),
        Err(_) => {
            forget();
            Ok(false)
        }
    }
}

/// Estado da caixa "lembrar de mim" para a tela de senha.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RememberState {
    /// `false` fora do Windows: sem proteção do SO, a caixa não aparece.
    pub supported: bool,
    /// Há um lembrete guardado agora.
    pub active: bool,
    pub default_hours: u64,
}

#[tauri::command]
pub fn remembered_unlock_state() -> RememberState {
    RememberState {
        supported: can_remember(),
        active: can_remember() && remembered_unlock_path().exists(),
        default_hours: REMEMBER_DEFAULT_HOURS,
    }
}

#[tauri::command]
pub fn forget_remembered_unlock() -> Result<(), String> {
    forget();
    Ok(())
}

/// Para a UI, "encrypted" sempre quis dizer **"protegido por senha"** — é isso
/// que a tela de criptografia mostra e o que o usuário decide ali. Desde que o
/// vault sem senha também é cifrado (pela chave do aparelho), `is_encrypted()`
/// deixou de responder essa pergunta: ela é verdadeira nos dois casos. Então o
/// comando passou a devolver `has_user_password()`; trocar para os bytes do
/// arquivo faria a tela dizer "Pass Lock" para quem não tem senha nenhuma.
#[tauri::command]
pub fn is_accounts_encrypted(state: tauri::State<'_, AccountStore>) -> Result<bool, String> {
    state.has_user_password()
}

#[tauri::command]
pub fn needs_password(state: tauri::State<'_, AccountStore>) -> Result<bool, String> {
    state.needs_password()
}

/// Problema com o `AccountData.key` que o usuário precisa ver **hoje**.
///
/// `eprintln!` numa build GUI não vai a lugar nenhum, e um `.key` ilegível é
/// justamente o defeito que passa o dia inteiro invisível — a chave mestra está
/// em memória, tudo funciona — para virar lockout no boot seguinte. Este comando
/// é o que leva isso à tela.
#[tauri::command]
pub fn vault_key_warning(state: tauri::State<'_, AccountStore>) -> Option<String> {
    state.vault_key_warning()
}

#[tauri::command]
pub fn set_encryption_password(
    state: tauri::State<'_, AccountStore>,
    password: Option<String>,
) -> Result<(), String> {
    state.set_password(password.as_deref())?;
    // A senha guardada não abre mais nada (ou não é mais necessária): guardá-la
    // só deixaria uma senha antiga em disco.
    forget();
    Ok(())
}

#[tauri::command]
pub fn reorder_accounts(
    state: tauri::State<'_, AccountStore>,
    user_ids: Vec<i64>,
) -> Result<(), String> {
    state.reorder(&user_ids)
}

#[tauri::command]
pub fn import_old_account_data(
    state: tauri::State<'_, AccountStore>,
    file_data: Vec<u8>,
    password: Option<String>,
) -> Result<OldAccountImportSummary, String> {
    state.import_old_account_data(&file_data, password.as_deref())
}

#[cfg(test)]
mod account_path_tests {
    use super::*;

    #[test]
    fn get_account_data_path_lives_in_the_runtime_data_dir() {
        // Deixou de ser "ao lado do exe": os dados agora ficam no perfil do
        // usuário, para o executável poder ser movido de pasta.
        let path = get_account_data_path();
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("AccountData.json")
        );
        assert!(path.is_absolute(), "{}", path.display());
        assert_eq!(
            path.parent(),
            Some(crate::data::settings::get_runtime_data_dir().as_path())
        );
        assert_eq!(get_account_data_path(), path, "the path is stable");
    }
}
