fn get_cookie(state: &AccountStore, user_id: i64) -> Result<String, String> {
    let accounts = state.get_all()?;
    accounts
        .iter()
        .find(|a| a.user_id == user_id)
        .map(|a| a.security_token.clone())
        .ok_or_else(|| format!("Account {} not found", user_id))
}

#[cfg(test)]
mod account_helpers_tests {
    use super::*;

    fn temp_store(tag: &str) -> AccountStore {
        crypto::init();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        AccountStore::new(std::env::temp_dir().join(format!("ram-helpers-{tag}-{nanos}.json")))
    }

    fn account(user_id: i64, token: &str) -> data::accounts::Account {
        data::accounts::Account::new(token.to_string(), format!("user{user_id}"), user_id)
    }

    #[test]
    fn get_cookie_returns_the_token_of_the_matching_account() {
        let store = temp_store("cookie-ok");
        store.add(account(1, "TOKEN_ONE")).unwrap();
        store.add(account(2, "TOKEN_TWO")).unwrap();

        assert_eq!(get_cookie(&store, 1).unwrap(), "TOKEN_ONE");
        assert_eq!(get_cookie(&store, 2).unwrap(), "TOKEN_TWO");
    }

    #[test]
    fn get_cookie_errors_with_the_user_id_when_the_account_is_missing() {
        let store = temp_store("cookie-missing");
        store.add(account(1, "TOKEN_ONE")).unwrap();

        assert_eq!(
            get_cookie(&store, 99).unwrap_err(),
            "Account 99 not found".to_string()
        );
    }

    #[test]
    fn get_cookie_on_an_empty_store_errors() {
        let store = temp_store("cookie-empty");
        assert!(get_cookie(&store, 0).is_err());
    }

    #[test]
    fn get_cookie_handles_negative_and_extreme_user_ids() {
        let store = temp_store("cookie-extremes");
        store.add(account(-5, "NEGATIVE")).unwrap();
        store.add(account(i64::MAX, "HUGE")).unwrap();

        assert_eq!(get_cookie(&store, -5).unwrap(), "NEGATIVE");
        assert_eq!(get_cookie(&store, i64::MAX).unwrap(), "HUGE");
        assert!(get_cookie(&store, i64::MIN).is_err());
    }

    #[test]
    fn get_cookie_preserves_unicode_and_empty_tokens_verbatim() {
        let store = temp_store("cookie-unicode");
        store.add(account(7, "コッキー_|WARNING:-DO-NOT-SHARE")).unwrap();
        store.add(account(8, "")).unwrap();

        assert_eq!(get_cookie(&store, 7).unwrap(), "コッキー_|WARNING:-DO-NOT-SHARE");
        assert_eq!(get_cookie(&store, 8).unwrap(), "");
    }
}
