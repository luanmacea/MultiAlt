pub fn get_account_data_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        .join("AccountData.json")
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
) -> Result<(), String> {
    state.load_with_password(&password)
}

#[tauri::command]
pub fn is_accounts_encrypted(state: tauri::State<'_, AccountStore>) -> Result<bool, String> {
    state.is_encrypted()
}

#[tauri::command]
pub fn needs_password(state: tauri::State<'_, AccountStore>) -> Result<bool, String> {
    state.needs_password()
}

#[tauri::command]
pub fn set_encryption_password(
    state: tauri::State<'_, AccountStore>,
    password: Option<String>,
) -> Result<(), String> {
    state.set_password(password.as_deref())
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
    fn get_account_data_path_sits_next_to_the_executable() {
        let path = get_account_data_path();
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("AccountData.json")
        );
        assert!(path.is_absolute(), "{}", path.display());

        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .expect("the test binary has a parent directory");
        assert_eq!(path.parent(), Some(exe_dir.as_path()));
        assert_eq!(get_account_data_path(), path, "the path is stable");
    }
}
