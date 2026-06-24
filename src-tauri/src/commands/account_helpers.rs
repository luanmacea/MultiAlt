fn get_cookie(state: &AccountStore, user_id: i64) -> Result<String, String> {
    let accounts = state.get_all()?;
    accounts
        .iter()
        .find(|a| a.user_id == user_id)
        .map(|a| a.security_token.clone())
        .ok_or_else(|| format!("Account {} not found", user_id))
}
