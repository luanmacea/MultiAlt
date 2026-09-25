/// Material de criptografia derivado uma única vez por unlock.
///
/// Por quê: `crypto::encrypt` sorteia um salt novo e chama `derive_key`
/// (argon2i, `OPSLIMIT_MODERATE`/`MEMLIMIT_MODERATE` ≈ 256 MiB) **a cada**
/// chamada. Como `save()` segura o lock de contas e os comandos Tauri síncronos
/// rodam na thread principal, mover N contas de grupo congelava a interface N
/// vezes. Agora o argon2 roda uma vez, no unlock.
///
/// Formato e segurança: o arquivo continua sendo
/// `RAM_HEADER | salt(16) | nonce(24) | ciphertext`, byte a byte igual ao que
/// `crypto::encrypt` produzia, e `crypto::decrypt` segue lendo normalmente. A
/// única diferença é que o salt passa a ser sorteado uma vez por unlock em vez
/// de uma vez por gravação. Isso não enfraquece nada: o salt existe para impedir
/// pré-computação de dicionário entre alvos diferentes (e ele continua aleatório
/// e rotacionado a cada unlock/mudança de senha); o valor que jamais pode se
/// repetir sob a mesma chave é o **nonce**, e esse continua sendo sorteado a
/// cada gravação. Reaproveitar a chave sem duplicar a montagem aqui exigiria um
/// `encrypt_with_key` em `data/crypto.rs`; enquanto aquele arquivo não puder ser
/// tocado, a montagem fica neste módulo.
struct SessionKey {
    /// SHA-512 da senha. Barato, e ainda necessário para decriptar arquivos
    /// gravados com outros salts (o próprio arquivo lido no unlock, por ex.).
    password_hash: Vec<u8>,
    salt: sodiumoxide::crypto::pwhash::argon2i13::Salt,
    key: sodiumoxide::crypto::secretbox::Key,
}

impl SessionKey {
    /// Espera a senha já normalizada (com trim) pelo chamador, do mesmo jeito
    /// que `crypto::hash_password`.
    fn derive(password: &str) -> Result<Self, String> {
        use sodiumoxide::crypto::pwhash::argon2i13;

        let password_hash = crypto::hash_password(password);
        let salt = argon2i13::gen_salt();
        let key = crypto::derive_key(&password_hash, salt.as_ref())
            .map_err(|e| format!("Failed to derive key: {}", e))?;
        Ok(Self {
            password_hash,
            salt,
            key,
        })
    }

    fn encrypt(&self, content: &str) -> Result<Vec<u8>, String> {
        use sodiumoxide::crypto::secretbox;

        if content.is_empty() {
            return Err("Failed to encrypt: Invalid encrypted data".to_string());
        }

        let nonce = secretbox::gen_nonce();
        let ciphertext = secretbox::seal(content.as_bytes(), &nonce, &self.key);

        let mut output =
            Vec::with_capacity(crypto::RAM_HEADER.len() + 16 + 24 + ciphertext.len());
        output.extend_from_slice(crypto::RAM_HEADER);
        output.extend_from_slice(self.salt.as_ref());
        output.extend_from_slice(nonce.as_ref());
        output.extend_from_slice(&ciphertext);
        Ok(output)
    }
}

pub struct AccountStore {
    accounts: Mutex<Vec<Account>>,
    /// `None` = sem senha (arquivo em texto puro) ou ainda trancado.
    /// Ordem de lock em todo o arquivo: `accounts` → `session`.
    session: Mutex<Option<SessionKey>>,
    file_path: PathBuf,
    /// Set when the on-disk file exists but could not be decoded. While set,
    /// `save()` refuses to write so an empty in-memory list never overwrites
    /// the user's accounts.
    load_failed: std::sync::atomic::AtomicBool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OldAccountImportSummary {
    pub total: usize,
    pub added: usize,
    pub replaced: usize,
    pub skipped: usize,
}

const IMPORT_PASSWORD_REQUIRED: &str = "IMPORT_PASSWORD_REQUIRED";

impl AccountStore {
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            accounts: Mutex::new(Vec::new()),
            session: Mutex::new(None),
            file_path,
            load_failed: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn is_encrypted(&self) -> Result<bool, String> {
        if !self.file_path.exists() {
            return Ok(false);
        }

        let data =
            fs::read(&self.file_path).map_err(|e| format!("Failed to read account file: {}", e))?;

        Ok(crypto::is_encrypted(&data))
    }

    pub fn needs_password(&self) -> Result<bool, String> {
        let session = self.session.lock().map_err(|e| e.to_string())?;
        if session.is_some() {
            return Ok(false);
        }
        drop(session);
        self.is_encrypted()
    }

    pub fn load(&self) -> Result<(), String> {
        if !self.file_path.exists() {
            return Ok(());
        }

        let data =
            fs::read(&self.file_path).map_err(|e| format!("Failed to read account file: {}", e))?;

        if data.is_empty() {
            return Ok(());
        }

        let accounts = match self.decode_accounts_for_load(&data) {
            Ok(accounts) => accounts,
            Err(e) => {
                self.load_failed
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                return Err(e);
            }
        };

        let mut store = self.accounts.lock().map_err(|e| e.to_string())?;
        *store = accounts;
        self.load_failed
            .store(false, std::sync::atomic::Ordering::SeqCst);

        Ok(())
    }

    pub fn load_with_password(&self, password: &str) -> Result<(), String> {
        // O unlock é o único ponto que paga o argon2: a chave derivada aqui é
        // reutilizada por todas as gravações da sessão.
        let trimmed = password.trim();
        let hash = crypto::hash_password(trimmed);

        if !self.file_path.exists() {
            let session = SessionKey::derive(trimmed)?;
            let mut slot = self.session.lock().map_err(|e| e.to_string())?;
            *slot = Some(session);
            return Ok(());
        }

        let data =
            fs::read(&self.file_path).map_err(|e| format!("Failed to read account file: {}", e))?;

        if data.is_empty() {
            let session = SessionKey::derive(trimmed)?;
            let mut accounts = self.accounts.lock().map_err(|e| e.to_string())?;
            *accounts = Vec::new();
            drop(accounts);
            let mut slot = self.session.lock().map_err(|e| e.to_string())?;
            *slot = Some(session);
            return Ok(());
        }

        let accounts = if crypto::is_encrypted(&data) {
            let decrypted =
                crypto::decrypt(&data, &hash).map_err(|e| format!("Failed to decrypt: {}", e))?;
            Self::parse_accounts_json(&decrypted)?
        } else {
            Self::decode_plain_or_legacy_accounts(&data)?
        };

        // Derivado só depois da senha ser aceita, para uma senha errada não
        // custar um argon2 extra.
        let session = SessionKey::derive(trimmed)?;

        let mut store = self.accounts.lock().map_err(|e| e.to_string())?;
        *store = accounts;
        drop(store);
        self.load_failed
            .store(false, std::sync::atomic::Ordering::SeqCst);

        let mut slot = self.session.lock().map_err(|e| e.to_string())?;
        *slot = Some(session);
        Ok(())
    }

    pub fn save(&self) -> Result<(), String> {
        let accounts = self.accounts.lock().map_err(|e| e.to_string())?;
        self.save_locked(&accounts, false)
    }

    /// Serializa e grava o snapshot **que o chamador ainda está segurando**.
    ///
    /// `add`/`remove`/`update`/`reorder` soltavam o lock antes de `save()`
    /// reobtê-lo, então duas escritas concorrentes podiam intercalar e deixar o
    /// arquivo uma entrada atrás da memória. Mantendo o guard vivo até o
    /// `atomic_replace`, o que vai para o disco é sempre o estado que acabou de
    /// ser produzido.
    ///
    /// `replace_encrypted_with_plain`: only for deliberately removing encryption
    /// from an already-unlocked store (see `set_password`).
    fn save_locked(
        &self,
        accounts: &[Account],
        replace_encrypted_with_plain: bool,
    ) -> Result<(), String> {
        if self.load_failed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(
                "Account file could not be loaded; refusing to overwrite it. Fix or restore AccountData.json and restart.".to_string(),
            );
        }

        let json = serde_json::to_string_pretty(accounts)
            .map_err(|e| format!("Failed to serialize accounts: {}", e))?;

        let session = self.session.lock().map_err(|e| e.to_string())?;

        let data = if let Some(session) = session.as_ref() {
            session.encrypt(&json)?
        } else {
            // Still locked: never replace an encrypted file with plaintext (it
            // would drop every account the user has not unlocked yet).
            if !replace_encrypted_with_plain && self.is_encrypted()? {
                return Err("Accounts are locked; unlock them before making changes.".to_string());
            }
            json.into_bytes()
        };

        // Write-then-rename so a crash or full disk never leaves a truncated file.
        let tmp_path = self.file_path.with_extension("json.tmp");
        fs::write(&tmp_path, data)
            .map_err(|e| format!("Failed to write account file: {}", e))?;
        crate::data::versions::atomic_replace(&tmp_path, &self.file_path)
            .map_err(|e| format!("Failed to replace account file: {}", e))?;

        Ok(())
    }

    pub fn set_password(&self, password: Option<&str>) -> Result<(), String> {
        if let Some(value) = password {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Err("Password cannot be empty".to_string());
            }
            if trimmed.chars().count() < 8 {
                return Err("Password must be at least 8 characters".to_string());
            }
        }
        let mut slot = self.session.lock().map_err(|e| e.to_string())?;
        let was_unlocked = slot.is_some();
        if !was_unlocked && self.is_encrypted()? {
            // Re-keying a file we never decrypted would encrypt an empty list
            // over the user's accounts.
            return Err("Accounts are locked; unlock them before changing the password.".to_string());
        }
        *slot = match password {
            Some(p) => Some(SessionKey::derive(p.trim())?),
            None => None,
        };
        // Solta `session` antes de pegar `accounts`: a ordem de lock do resto do
        // arquivo é accounts → session, e inverter aqui criaria deadlock.
        drop(slot);

        let accounts = self.accounts.lock().map_err(|e| e.to_string())?;
        self.save_locked(&accounts, was_unlocked)
    }

    pub fn get_all(&self) -> Result<Vec<Account>, String> {
        let accounts = self.accounts.lock().map_err(|e| e.to_string())?;
        Ok(accounts.clone())
    }

    pub fn add(&self, account: Account) -> Result<(), String> {
        let mut accounts = self.accounts.lock().map_err(|e| e.to_string())?;

        if let Some(existing) = accounts.iter_mut().find(|a| a.user_id == account.user_id) {
            existing.security_token = account.security_token;
            existing.username = account.username;
            existing.valid = account.valid;
            existing.last_use = account.last_use;
            if !account.password.is_empty() {
                existing.password = account.password;
            }
        } else {
            accounts.push(account);
        }

        // O guard continua vivo: o arquivo recebe exatamente este snapshot.
        self.save_locked(&accounts, false)
    }

    pub fn remove(&self, user_id: i64) -> Result<bool, String> {
        let mut accounts = self.accounts.lock().map_err(|e| e.to_string())?;
        let initial_len = accounts.len();
        accounts.retain(|a| a.user_id != user_id);
        let removed = accounts.len() < initial_len;

        if removed {
            self.save_locked(&accounts, false)?;
        }

        Ok(removed)
    }

    pub fn update(&self, account: Account) -> Result<bool, String> {
        let mut accounts = self.accounts.lock().map_err(|e| e.to_string())?;

        if let Some(existing) = accounts.iter_mut().find(|a| a.user_id == account.user_id) {
            *existing = account;
            self.save_locked(&accounts, false)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Marca que a conta **foi usada agora**. Chamado no sucesso do launch (app,
    /// botting e web server): sem isso `last_use` só era escrito ao criar ou
    /// re-adicionar a conta, e a coluna "3d"/"2mo" da lista media idade do
    /// cadastro em vez de inatividade de jogo. Devolve `false` quando não existe
    /// conta com esse id — lançar uma conta que saiu da lista não é erro.
    pub fn mark_used(&self, user_id: i64) -> Result<bool, String> {
        let mut accounts = self.accounts.lock().map_err(|e| e.to_string())?;

        let Some(account) = accounts.iter_mut().find(|a| a.user_id == user_id) else {
            return Ok(false);
        };
        account.last_use = Utc::now();

        self.save_locked(&accounts, false)?;
        Ok(true)
    }

    pub fn reorder(&self, user_ids: &[i64]) -> Result<(), String> {
        let mut accounts = self.accounts.lock().map_err(|e| e.to_string())?;

        if accounts.is_empty() || user_ids.is_empty() {
            return Ok(());
        }

        let mut ordered = Vec::with_capacity(accounts.len());

        for user_id in user_ids {
            if let Some(pos) = accounts.iter().position(|a| a.user_id == *user_id) {
                ordered.push(accounts.remove(pos));
            }
        }

        ordered.append(&mut accounts);
        *accounts = ordered;

        self.save_locked(&accounts, false)
    }

    fn decode_plain_or_legacy_accounts(data: &[u8]) -> Result<Vec<Account>, String> {
        if let Ok(accounts) = Self::parse_accounts_json(data) {
            return Ok(accounts);
        }

        if let Some(legacy_decrypted) = crypto::try_decrypt_legacy_dpapi(data) {
            return Self::parse_accounts_json(&legacy_decrypted);
        }

        Err("Invalid account data format (failed plaintext and legacy DPAPI decode)".to_string())
    }

    fn decode_accounts_for_load(&self, data: &[u8]) -> Result<Vec<Account>, String> {
        if data.is_empty() {
            return Ok(Vec::new());
        }

        if crypto::is_encrypted(data) {
            let session = self.session.lock().map_err(|e| e.to_string())?;
            let hash = session
                .as_ref()
                .map(|s| s.password_hash.as_slice())
                .ok_or_else(|| "Password required for encrypted file".to_string())?;
            let decrypted =
                crypto::decrypt(data, hash).map_err(|e| format!("Failed to decrypt: {}", e))?;
            return Self::parse_accounts_json(&decrypted);
        }

        Self::decode_plain_or_legacy_accounts(data)
    }

    fn decode_accounts_for_import(
        &self,
        data: &[u8],
        import_password: Option<&str>,
    ) -> Result<Vec<Account>, String> {
        if data.is_empty() {
            return Ok(Vec::new());
        }

        if crypto::is_encrypted(data) {
            let Some(password) = import_password else {
                return Err(IMPORT_PASSWORD_REQUIRED.to_string());
            };
            // Mesmo trim de `load_with_password`: a senha colada com espaço no
            // fim desbloqueava o app mas era recusada no import.
            let hash = crypto::hash_password(password.trim());
            let decrypted = crypto::decrypt(data, &hash)
                .map_err(|_| "Import password is incorrect".to_string())?;
            return Self::parse_accounts_json(&decrypted);
        }

        Self::decode_plain_or_legacy_accounts(data)
    }

    fn parse_accounts_json(data: &[u8]) -> Result<Vec<Account>, String> {
        serde_json::from_slice::<Vec<Account>>(data)
            .map_err(|e| format!("Failed to parse account JSON: {}", e))
    }

    pub fn import_old_account_data(
        &self,
        data: &[u8],
        import_password: Option<&str>,
    ) -> Result<OldAccountImportSummary, String> {
        let imported_accounts = self.decode_accounts_for_import(data, import_password)?;
        let total = imported_accounts.len();
        let mut skipped = 0usize;

        let mut imported_by_user_id: HashMap<i64, Account> = HashMap::new();
        let mut imported_order: Vec<i64> = Vec::new();

        for account in imported_accounts {
            let user_id = account.user_id;
            if user_id <= 0 {
                skipped += 1;
                continue;
            }

            if imported_by_user_id.contains_key(&user_id) {
                skipped += 1;
            } else {
                imported_order.push(user_id);
            }
            imported_by_user_id.insert(user_id, account);
        }

        let mut accounts = self.accounts.lock().map_err(|e| e.to_string())?;
        let mut current_index_by_user_id: HashMap<i64, usize> = accounts
            .iter()
            .enumerate()
            .map(|(idx, account)| (account.user_id, idx))
            .collect();

        let mut added = 0usize;
        let mut replaced = 0usize;

        for user_id in imported_order {
            let Some(account) = imported_by_user_id.remove(&user_id) else {
                continue;
            };
            if let Some(existing_index) = current_index_by_user_id.get(&user_id).copied() {
                accounts[existing_index] = account;
                replaced += 1;
            } else {
                let next_index = accounts.len();
                current_index_by_user_id.insert(user_id, next_index);
                accounts.push(account);
                added += 1;
            }
        }

        let mut seen_user_ids = HashSet::new();
        accounts.retain(|account| seen_user_ids.insert(account.user_id));

        if added > 0 || replaced > 0 {
            self.save_locked(&accounts, false)?;
        }
        drop(accounts);

        Ok(OldAccountImportSummary {
            total,
            added,
            replaced,
            skipped,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_test_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("ram-{name}-{nanos}.json"))
    }

    fn new_test_store(name: &str) -> AccountStore {
        crypto::init();
        AccountStore::new(unique_test_path(name))
    }

    #[test]
    fn decode_plain_or_legacy_accounts_should_accept_legacy_null_string_fields() {
        let json = br#"
        [
          {
            "Valid": true,
            "SecurityToken": "_|WARNING:-DO-NOT-SHARE",
            "Username": "LegacyUser",
            "LastUse": "2024-03-05T12:34:56",
            "Alias": null,
            "Description": null,
            "Password": null,
            "Group": null,
            "UserID": 12345,
            "Fields": { "Note": null, "Rank": "Admin" },
            "LastAttemptedRefresh": "2024-03-05T12:34:56",
            "BrowserTrackerID": null
          }
        ]
        "#;

        let accounts = AccountStore::decode_plain_or_legacy_accounts(json).unwrap();

        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].alias, "");
        assert_eq!(accounts[0].description, "");
        assert_eq!(accounts[0].password, "");
        assert_eq!(accounts[0].group, "Default");
        assert_eq!(accounts[0].browser_tracker_id, "");
        assert_eq!(accounts[0].fields.get("Note").map(String::as_str), Some(""));
        assert_eq!(
            accounts[0].fields.get("Rank").map(String::as_str),
            Some("Admin")
        );
    }

    #[test]
    fn import_old_account_data_should_accept_current_v4_encrypted_exports() {
        let store = new_test_store("import-current-v4");
        let current = vec![Account::new(
            "_|WARNING:-DO-NOT-SHARE".to_string(),
            "CurrentUser".to_string(),
            67890,
        )];
        let json = serde_json::to_string(&current).unwrap();
        let password = "compatibility-pass";
        let hash = crypto::hash_password(password);
        let encrypted = crypto::encrypt(&json, &hash).unwrap();

        let summary = store
            .import_old_account_data(&encrypted, Some(password))
            .unwrap();
        let imported = store.get_all().unwrap();

        assert_eq!(summary.total, 1);
        assert_eq!(summary.added, 1);
        assert_eq!(summary.replaced, 0);
        assert_eq!(summary.skipped, 0);
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].user_id, 67890);
        assert_eq!(imported[0].username, "CurrentUser");

        let _ = fs::remove_file(&store.file_path);
    }

    #[test]
    fn save_should_not_overwrite_encrypted_file_while_locked() {
        let store = new_test_store("locked-save");
        let existing = vec![Account::new("cookie".to_string(), "Kept".to_string(), 111)];
        let json = serde_json::to_string(&existing).unwrap();
        let encrypted = crypto::encrypt(&json, &crypto::hash_password("secret-pass")).unwrap();
        fs::write(&store.file_path, &encrypted).unwrap();

        let result = store.add(Account::new("c2".to_string(), "New".to_string(), 222));

        assert!(result.is_err());
        assert_eq!(fs::read(&store.file_path).unwrap(), encrypted);
        let _ = fs::remove_file(&store.file_path);
    }

    #[test]
    fn set_password_none_should_decrypt_an_unlocked_store() {
        let store = new_test_store("remove-encryption");
        let existing = vec![Account::new("cookie".to_string(), "Kept".to_string(), 111)];
        let json = serde_json::to_string(&existing).unwrap();
        let encrypted = crypto::encrypt(&json, &crypto::hash_password("secret-pass")).unwrap();
        fs::write(&store.file_path, &encrypted).unwrap();
        store.load_with_password("secret-pass").unwrap();

        store.set_password(None).unwrap();

        assert!(!store.is_encrypted().unwrap());
        assert_eq!(store.get_all().unwrap()[0].user_id, 111);
        let _ = fs::remove_file(&store.file_path);
    }

    #[test]
    fn save_should_refuse_after_failed_load() {
        let store = new_test_store("failed-load");
        fs::write(&store.file_path, b"not valid account data").unwrap();

        assert!(store.load().is_err());
        assert!(store.add(Account::new("c".to_string(), "U".to_string(), 1)).is_err());
        assert_eq!(fs::read(&store.file_path).unwrap(), b"not valid account data");
        let _ = fs::remove_file(&store.file_path);
    }

    #[test]
    fn save_should_write_atomically_and_round_trip() {
        let store = new_test_store("atomic-save");
        store.add(Account::new("c".to_string(), "U".to_string(), 7)).unwrap();

        assert!(!store.file_path.with_extension("json.tmp").exists());
        let reloaded = AccountStore::new(store.file_path.clone());
        reloaded.load().unwrap();
        assert_eq!(reloaded.get_all().unwrap()[0].user_id, 7);
        let _ = fs::remove_file(&store.file_path);
    }

    #[test]
    fn import_old_account_data_should_accept_current_v4_plain_exports() {
        let store = new_test_store("import-current-v4-plain");
        let current = vec![Account::new(
            "_|WARNING:-DO-NOT-SHARE".to_string(),
            "PlainUser".to_string(),
            24680,
        )];
        let json = serde_json::to_vec(&current).unwrap();

        let summary = store.import_old_account_data(&json, None).unwrap();
        let imported = store.get_all().unwrap();

        assert_eq!(summary.total, 1);
        assert_eq!(summary.added, 1);
        assert_eq!(summary.replaced, 0);
        assert_eq!(summary.skipped, 0);
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].user_id, 24680);
        assert_eq!(imported[0].username, "PlainUser");

        let _ = fs::remove_file(&store.file_path);
    }
}

#[cfg(test)]
mod account_store_tests {
    use super::*;
    use std::sync::OnceLock;
    use std::time::{SystemTime, UNIX_EPOCH};

    const SAMPLE_PASSWORD: &str = "sample-password";

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("ram-acct-{name}-{nanos}.json"))
    }

    /// Store + RAII cleanup of its file and any leftover `.json.tmp`.
    struct TestStore {
        store: AccountStore,
    }

    impl Drop for TestStore {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.store.file_path);
            let _ = fs::remove_file(self.store.file_path.with_extension("json.tmp"));
        }
    }

    impl std::ops::Deref for TestStore {
        type Target = AccountStore;
        fn deref(&self) -> &AccountStore {
            &self.store
        }
    }

    fn store(name: &str) -> TestStore {
        crypto::init();
        TestStore {
            store: AccountStore::new(temp_path(name)),
        }
    }

    /// `LastUse` era escrito so na criacao/re-adicao da conta: a coluna "3d"/"2mo"
    /// e a bolinha de envelhecimento mediam idade do **cadastro**, nao inatividade
    /// de jogo. Quem lanca precisa poder marcar uso.
    #[test]
    fn mark_used_moves_last_use_forward_and_persists_it() {
        let store = store("mark-used");
        let mut old = account(7, "ann");
        old.last_use = chrono::Utc::now() - chrono::Duration::days(40);
        let before = old.last_use;
        store.add(old).unwrap();

        assert!(store.mark_used(7).unwrap(), "a conta existe, entao marcou");

        let after = store.get_all().unwrap()[0].last_use;
        assert!(after > before, "last_use andou para frente: {before} -> {after}");

        // E foi para o disco, nao so para a memoria.
        let reloaded = AccountStore::new(store.file_path.clone());
        reloaded.load().unwrap();
        assert_eq!(reloaded.get_all().unwrap()[0].last_use, after);
    }

    #[test]
    fn mark_used_says_when_the_account_is_not_there() {
        let store = store("mark-used-missing");
        store.add(account(7, "ann")).unwrap();
        assert!(!store.mark_used(999).unwrap(), "conta inexistente nao marca nada");
    }

    fn account(user_id: i64, username: &str) -> Account {
        Account::new(format!("cookie-{user_id}"), username.to_string(), user_id)
    }

    /// Deriving an argon2i key is deliberately slow, so the tests that only
    /// need "some encrypted file" share one blob.
    fn encrypted_sample() -> &'static Vec<u8> {
        static SAMPLE: OnceLock<Vec<u8>> = OnceLock::new();
        SAMPLE.get_or_init(|| {
            crypto::init();
            let json = serde_json::to_string(&vec![account(111, "Sample")]).unwrap();
            crypto::encrypt(&json, &crypto::hash_password(SAMPLE_PASSWORD)).unwrap()
        })
    }

    fn ids(store: &AccountStore) -> Vec<i64> {
        store.get_all().unwrap().iter().map(|a| a.user_id).collect()
    }

    // ---- is_encrypted / needs_password ---------------------------------------

    #[test]
    fn is_encrypted_and_needs_password_follow_the_file_on_disk() {
        let s = store("encstate");

        // No file yet: nothing is encrypted and nothing is locked.
        assert!(!s.is_encrypted().unwrap());
        assert!(!s.needs_password().unwrap());

        // Plain file: still unlocked.
        fs::write(&s.file_path, b"[]").unwrap();
        assert!(!s.is_encrypted().unwrap());
        assert!(!s.needs_password().unwrap());

        // Empty file: `is_encrypted` looks at the bytes, so it is not encrypted.
        fs::write(&s.file_path, b"").unwrap();
        assert!(!s.is_encrypted().unwrap());
        assert!(!s.needs_password().unwrap());

        // Encrypted file with no password in memory: locked.
        fs::write(&s.file_path, encrypted_sample()).unwrap();
        assert!(s.is_encrypted().unwrap());
        assert!(s.needs_password().unwrap());

        // Once a password is held, it is no longer "needs password" even
        // though the file is still encrypted.
        *s.session.lock().unwrap() = Some(SessionKey::derive(SAMPLE_PASSWORD).unwrap());
        assert!(s.is_encrypted().unwrap());
        assert!(!s.needs_password().unwrap());
    }

    // ---- load ----------------------------------------------------------------

    #[test]
    fn load_is_a_no_op_for_a_missing_or_empty_file() {
        let s = store("load-empty");
        s.load().expect("missing file must not be an error");
        assert!(ids(&s).is_empty());
        assert!(!s.file_path.exists(), "load must not create the file");

        fs::write(&s.file_path, b"").unwrap();
        s.load().expect("empty file must not be an error");
        assert!(ids(&s).is_empty());
        // An empty file is not a failed load, so saving stays allowed.
        s.add(account(1, "A")).expect("save after empty load");
        assert_eq!(ids(&s), vec![1]);
    }

    #[test]
    fn load_reads_plain_json_and_replaces_the_in_memory_list() {
        let s = store("load-plain");
        s.add(account(9, "Stale")).unwrap();

        let json = serde_json::to_vec(&vec![account(1, "One"), account(2, "Two")]).unwrap();
        fs::write(&s.file_path, json).unwrap();
        s.load().unwrap();

        assert_eq!(ids(&s), vec![1, 2], "load replaces, it does not merge");
    }

    #[test]
    fn load_of_an_encrypted_file_without_a_password_fails_and_latches_load_failed() {
        let s = store("load-locked");
        fs::write(&s.file_path, encrypted_sample()).unwrap();

        let err = s.load().expect_err("locked file must not load");
        assert!(err.contains("Password required"), "{err}");
        assert!(s.load_failed.load(std::sync::atomic::Ordering::SeqCst));

        // The latch must keep save() from clobbering the file.
        let err = s.save().expect_err("save must refuse after a failed load");
        assert!(err.contains("refusing to overwrite"), "{err}");
        assert_eq!(&fs::read(&s.file_path).unwrap(), encrypted_sample());
    }

    #[test]
    fn load_of_unreadable_bytes_fails_and_keeps_the_file_intact() {
        let s = store("load-garbage");
        fs::write(&s.file_path, b"\x00\x01\x02 definitely not json").unwrap();

        let err = s.load().expect_err("garbage must not load");
        assert!(
            err.contains("failed plaintext and legacy DPAPI decode"),
            "{err}"
        );
        assert!(
            s.add(account(1, "New")).is_err(),
            "writes must go through the save latch"
        );
        assert_eq!(
            fs::read(&s.file_path).unwrap(),
            b"\x00\x01\x02 definitely not json"
        );
    }

    #[test]
    fn a_successful_load_clears_a_previous_load_failure() {
        let s = store("load-recover");
        fs::write(&s.file_path, b"broken").unwrap();
        assert!(s.load().is_err());
        assert!(s.load_failed.load(std::sync::atomic::Ordering::SeqCst));

        fs::write(&s.file_path, b"[]").unwrap();
        s.load().expect("a good file must clear the latch");
        assert!(!s.load_failed.load(std::sync::atomic::Ordering::SeqCst));
        s.add(account(5, "Ok")).expect("saving is allowed again");
    }

    // ---- load_with_password ---------------------------------------------------

    #[test]
    fn load_with_password_on_a_missing_file_only_arms_the_password() {
        let s = store("pw-missing");
        s.load_with_password("  some-password  ").unwrap();

        assert!(!s.file_path.exists(), "no file must be created");
        assert!(ids(&s).is_empty());
        // The password is trimmed before hashing.
        assert_eq!(
            s.session
                .lock()
                .unwrap()
                .as_ref()
                .map(|k| k.password_hash.clone()),
            Some(crypto::hash_password("some-password"))
        );
    }

    #[test]
    fn load_with_password_on_an_empty_file_clears_accounts_and_arms_the_password() {
        let s = store("pw-empty");
        s.add(account(7, "Stale")).unwrap();
        fs::write(&s.file_path, b"").unwrap();

        s.load_with_password("another-password").unwrap();

        assert!(ids(&s).is_empty());
        assert!(s.session.lock().unwrap().is_some());
    }

    #[test]
    fn load_with_password_reads_a_plain_file_and_upgrades_it_on_the_next_save() {
        let s = store("pw-plain");
        let json = serde_json::to_vec(&vec![account(3, "Plain")]).unwrap();
        fs::write(&s.file_path, json).unwrap();

        s.load_with_password("upgrade-password").unwrap();
        assert_eq!(ids(&s), vec![3]);

        // Holding a password makes every later save encrypt.
        s.add(account(4, "New")).unwrap();
        assert!(s.is_encrypted().unwrap());
    }

    #[test]
    fn load_with_password_round_trips_an_encrypted_file_and_rejects_the_wrong_password() {
        let s = store("pw-roundtrip");
        fs::write(&s.file_path, encrypted_sample()).unwrap();

        let err = s
            .load_with_password("not-the-password")
            .expect_err("wrong password must fail");
        assert!(err.contains("Failed to decrypt"), "{err}");
        assert!(ids(&s).is_empty(), "a failed unlock must not load anything");
        assert!(
            s.session.lock().unwrap().is_none(),
            "a failed unlock must not arm the wrong password"
        );

        s.load_with_password(SAMPLE_PASSWORD).unwrap();
        assert_eq!(ids(&s), vec![111]);
        assert_eq!(s.get_all().unwrap()[0].username, "Sample");
    }

    // ---- save -----------------------------------------------------------------

    #[test]
    fn save_leaves_no_temp_file_and_round_trips_through_a_fresh_store() {
        let s = store("save-roundtrip");
        let mut a = account(21, "Persisted");
        a.alias = "Alt 21".to_string();
        a.group = "Farm".to_string();
        a.set_field("Note".into(), "keep me".into());
        s.add(a).unwrap();

        assert!(!s.file_path.with_extension("json.tmp").exists());

        let reloaded = AccountStore::new(s.file_path.clone());
        reloaded.load().unwrap();
        let loaded = reloaded.get_all().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].alias, "Alt 21");
        assert_eq!(loaded[0].group, "Farm");
        assert_eq!(loaded[0].get_field("Note").map(String::as_str), Some("keep me"));
    }

    #[test]
    fn save_writes_plain_json_when_no_password_is_set() {
        let s = store("save-plain");
        s.add(account(31, "Plain")).unwrap();

        let raw = fs::read_to_string(&s.file_path).unwrap();
        assert!(raw.trim_start().starts_with('['), "{raw}");
        assert!(raw.contains("\"UserID\": 31"), "{raw}");
        assert!(!s.is_encrypted().unwrap());
    }

    // ---- set_password ---------------------------------------------------------

    #[test]
    fn set_password_validates_the_new_password_before_touching_anything() {
        let s = store("pw-validate");
        s.add(account(41, "A")).unwrap();
        let before = fs::read(&s.file_path).unwrap();

        assert_eq!(
            s.set_password(Some("")).unwrap_err(),
            "Password cannot be empty"
        );
        assert_eq!(
            s.set_password(Some("    ")).unwrap_err(),
            "Password cannot be empty"
        );
        assert_eq!(
            s.set_password(Some("1234567")).unwrap_err(),
            "Password must be at least 8 characters"
        );
        // The length rule counts characters, not bytes, and applies after trim.
        assert_eq!(
            s.set_password(Some("  1234567  ")).unwrap_err(),
            "Password must be at least 8 characters"
        );
        assert!(
            s.set_password(Some("aaaaaaa\u{00e7}")).is_ok(),
            "8 characters must be accepted even when they are 9 bytes"
        );

        assert_ne!(fs::read(&s.file_path).unwrap(), before, "the file is re-keyed");
        assert!(s.is_encrypted().unwrap());
    }

    #[test]
    fn set_password_refuses_to_rekey_a_file_that_was_never_unlocked() {
        let s = store("pw-rekey-locked");
        fs::write(&s.file_path, encrypted_sample()).unwrap();

        let err = s
            .set_password(Some("brand-new-password"))
            .expect_err("re-keying a locked file must fail");
        assert!(err.contains("unlock them before changing the password"), "{err}");
        assert_eq!(&fs::read(&s.file_path).unwrap(), encrypted_sample());

        // Clearing the password on a locked store is refused for the same reason.
        let err = s.set_password(None).expect_err("clearing must fail too");
        assert!(err.contains("unlock them before changing the password"), "{err}");
        assert_eq!(&fs::read(&s.file_path).unwrap(), encrypted_sample());
    }

    #[test]
    fn set_password_none_on_a_plain_store_is_a_no_op_that_keeps_the_file_plain() {
        let s = store("pw-none-plain");
        s.add(account(51, "A")).unwrap();
        s.set_password(None).unwrap();
        assert!(!s.is_encrypted().unwrap());
        assert_eq!(ids(&s), vec![51]);
    }

    // ---- add ------------------------------------------------------------------

    #[test]
    fn add_appends_new_accounts_in_order() {
        let s = store("add-order");
        s.add(account(1, "One")).unwrap();
        s.add(account(2, "Two")).unwrap();
        s.add(account(3, "Three")).unwrap();
        assert_eq!(ids(&s), vec![1, 2, 3]);
    }

    #[test]
    fn add_of_an_existing_user_id_refreshes_credentials_but_keeps_user_metadata() {
        let s = store("add-merge");
        let mut original = account(60, "OldName");
        original.alias = "My Alt".to_string();
        original.group = "Farm".to_string();
        original.description = "notes".to_string();
        original.password = "old-password".to_string();
        original.set_field("Note".into(), "kept".into());
        original.valid = false;
        s.add(original).unwrap();

        let mut incoming = account(60, "NewName");
        incoming.security_token = "fresh-cookie".to_string();
        incoming.valid = true;
        incoming.password = String::new(); // empty => must not clear the stored one
        s.add(incoming).unwrap();

        let stored = s.get_all().unwrap();
        assert_eq!(stored.len(), 1, "add must not duplicate a user id");
        assert_eq!(stored[0].username, "NewName");
        assert_eq!(stored[0].security_token, "fresh-cookie");
        assert!(stored[0].valid);
        assert_eq!(stored[0].password, "old-password", "an empty password must not wipe");
        assert_eq!(stored[0].alias, "My Alt");
        assert_eq!(stored[0].group, "Farm");
        assert_eq!(stored[0].description, "notes");
        assert_eq!(stored[0].get_field("Note").map(String::as_str), Some("kept"));

        // A non-empty password does replace it.
        let mut with_password = account(60, "NewName");
        with_password.password = "new-password".to_string();
        s.add(with_password).unwrap();
        assert_eq!(s.get_all().unwrap()[0].password, "new-password");
    }

    #[test]
    fn add_accepts_the_zero_user_id_as_a_normal_slot() {
        // `add` has no user-id validation: the guard only exists on import.
        let s = store("add-zero");
        s.add(account(0, "Zero")).unwrap();
        s.add(account(0, "ZeroAgain")).unwrap();
        assert_eq!(ids(&s), vec![0]);
        assert_eq!(s.get_all().unwrap()[0].username, "ZeroAgain");
    }

    // ---- remove ---------------------------------------------------------------

    #[test]
    fn remove_reports_whether_anything_was_removed_and_only_saves_when_it_was() {
        let s = store("remove");
        assert!(!s.remove(1).unwrap(), "removing from an empty store is false");
        assert!(!s.file_path.exists(), "a no-op remove must not write the file");

        s.add(account(1, "One")).unwrap();
        s.add(account(2, "Two")).unwrap();

        assert!(!s.remove(999).unwrap());
        assert_eq!(ids(&s), vec![1, 2]);

        assert!(s.remove(1).unwrap());
        assert_eq!(ids(&s), vec![2]);

        let reloaded = AccountStore::new(s.file_path.clone());
        reloaded.load().unwrap();
        assert_eq!(ids(&reloaded), vec![2]);

        assert!(!s.remove(1).unwrap(), "removing twice is false the second time");
    }

    // ---- update ---------------------------------------------------------------

    #[test]
    fn update_replaces_the_whole_account_and_reports_a_miss_without_saving() {
        let s = store("update");
        assert!(
            !s.update(account(1, "Ghost")).unwrap(),
            "updating an unknown id must be false"
        );
        assert!(!s.file_path.exists(), "a missed update must not write the file");

        s.add(account(1, "One")).unwrap();
        s.add(account(2, "Two")).unwrap();

        let mut edited = account(1, "Renamed");
        edited.alias = "alias".to_string();
        edited.group = "Group".to_string();
        assert!(s.update(edited).unwrap());

        let stored = s.get_all().unwrap();
        assert_eq!(ids(&s), vec![1, 2], "update must keep the position");
        assert_eq!(stored[0].username, "Renamed");
        assert_eq!(stored[0].alias, "alias");
        assert_eq!(stored[0].group, "Group");

        let reloaded = AccountStore::new(s.file_path.clone());
        reloaded.load().unwrap();
        assert_eq!(reloaded.get_all().unwrap()[0].group, "Group");
    }

    #[test]
    fn group_round_trips_through_the_file_including_the_implicit_default() {
        let s = store("groups");
        s.add(account(1, "Default")).unwrap();
        let mut grouped = account(2, "Grouped");
        grouped.group = "Bots".to_string();
        s.add(grouped).unwrap();

        // The default group is omitted from the JSON but restored on read.
        let raw = fs::read_to_string(&s.file_path).unwrap();
        assert_eq!(raw.matches("\"Group\"").count(), 1, "{raw}");
        assert!(raw.contains("\"Bots\""), "{raw}");

        let reloaded = AccountStore::new(s.file_path.clone());
        reloaded.load().unwrap();
        let loaded = reloaded.get_all().unwrap();
        assert_eq!(loaded[0].group, "Default");
        assert_eq!(loaded[1].group, "Bots");
    }

    // ---- reorder --------------------------------------------------------------

    #[test]
    fn reorder_is_a_no_op_for_an_empty_store_or_an_empty_id_list() {
        let s = store("reorder-noop");
        s.reorder(&[1, 2, 3]).expect("empty store");
        assert!(!s.file_path.exists(), "a no-op reorder must not write the file");

        s.add(account(1, "One")).unwrap();
        s.add(account(2, "Two")).unwrap();
        s.reorder(&[]).expect("empty id list");
        assert_eq!(ids(&s), vec![1, 2]);
    }

    #[test]
    fn reorder_moves_the_listed_ids_to_the_front_and_keeps_the_rest_in_order() {
        let s = store("reorder-partial");
        for id in 1..=5 {
            s.add(account(id, &format!("User{id}"))).unwrap();
        }

        // Only a subset is listed: the rest keeps its relative order behind it.
        s.reorder(&[4, 2]).unwrap();
        assert_eq!(ids(&s), vec![4, 2, 1, 3, 5]);

        // A full list is an exact permutation.
        s.reorder(&[5, 4, 3, 2, 1]).unwrap();
        assert_eq!(ids(&s), vec![5, 4, 3, 2, 1]);

        let reloaded = AccountStore::new(s.file_path.clone());
        reloaded.load().unwrap();
        assert_eq!(ids(&reloaded), vec![5, 4, 3, 2, 1]);
    }

    #[test]
    fn reorder_ignores_unknown_ids_and_repeated_ids_without_losing_accounts() {
        let s = store("reorder-weird");
        for id in 1..=3 {
            s.add(account(id, &format!("User{id}"))).unwrap();
        }

        // Unknown ids are skipped, a repeated id only moves once.
        s.reorder(&[999, 3, 3, -1, 0, 1]).unwrap();
        assert_eq!(ids(&s), vec![3, 1, 2]);

        // Nothing known at all: the list is untouched.
        s.reorder(&[777, 888]).unwrap();
        assert_eq!(ids(&s), vec![3, 1, 2]);
    }

    // ---- import ---------------------------------------------------------------

    #[test]
    fn import_of_empty_data_is_an_empty_summary_and_writes_nothing() {
        let s = store("import-empty");
        let summary = s.import_old_account_data(b"", None).unwrap();
        assert_eq!(
            (summary.total, summary.added, summary.replaced, summary.skipped),
            (0, 0, 0, 0)
        );
        assert!(!s.file_path.exists(), "an empty import must not write the file");

        // An empty JSON array behaves the same way.
        let summary = s.import_old_account_data(b"[]", None).unwrap();
        assert_eq!(summary.total, 0);
        assert!(!s.file_path.exists());
    }

    #[test]
    fn import_counts_added_replaced_and_skipped_entries() {
        let s = store("import-counts");
        s.add(account(1, "Existing")).unwrap();

        let mut duplicate_a = account(2, "DupFirst");
        duplicate_a.alias = "first".to_string();
        let mut duplicate_b = account(2, "DupLast");
        duplicate_b.alias = "last".to_string();

        let payload = serde_json::to_vec(&vec![
            account(1, "Replaced"),
            account(3, "Added"),
            duplicate_a,
            duplicate_b,
            account(0, "InvalidZero"),
            account(-5, "InvalidNegative"),
        ])
        .unwrap();

        let summary = s.import_old_account_data(&payload, None).unwrap();

        assert_eq!(summary.total, 6, "total counts every decoded entry");
        assert_eq!(summary.added, 2, "user 3 and user 2");
        assert_eq!(summary.replaced, 1, "user 1");
        assert_eq!(
            summary.skipped, 3,
            "two non-positive ids plus the duplicated user 2"
        );

        assert_eq!(ids(&s), vec![1, 3, 2], "replacements keep their slot");
        let stored = s.get_all().unwrap();
        assert_eq!(stored[0].username, "Replaced");
        assert_eq!(
            stored[2].alias, "last",
            "the last duplicate wins, the earlier one is the skipped copy"
        );
    }

    #[test]
    fn import_drops_pre_existing_duplicate_user_ids_keeping_the_first() {
        let s = store("import-dedup");
        {
            let mut accounts = s.accounts.lock().unwrap();
            let mut first = account(8, "First");
            first.alias = "first".to_string();
            let mut second = account(8, "Second");
            second.alias = "second".to_string();
            accounts.push(first);
            accounts.push(second);
        }

        let payload = serde_json::to_vec(&vec![account(9, "New")]).unwrap();
        let summary = s.import_old_account_data(&payload, None).unwrap();

        assert_eq!(summary.added, 1);
        assert_eq!(ids(&s), vec![8, 9]);
        assert_eq!(
            s.get_all().unwrap()[0].alias,
            "first",
            "retain() keeps the first occurrence"
        );
    }

    #[test]
    fn import_rejects_data_that_is_neither_json_nor_a_legacy_blob() {
        let s = store("import-garbage");
        let err = s
            .import_old_account_data(b"\x01\x02\x03 not json", None)
            .unwrap_err();
        assert!(
            err.contains("failed plaintext and legacy DPAPI decode"),
            "{err}"
        );
        assert!(!s.file_path.exists());

        // Valid JSON of the wrong shape reports a parse error instead.
        let err = s
            .import_old_account_data(br#"{"accounts": []}"#, None)
            .unwrap_err();
        assert!(err.contains("failed plaintext and legacy DPAPI decode"), "{err}");
    }

    #[test]
    fn import_of_an_encrypted_export_requires_and_validates_the_password() {
        let s = store("import-encrypted");

        let err = s
            .import_old_account_data(encrypted_sample(), None)
            .unwrap_err();
        assert_eq!(err, "IMPORT_PASSWORD_REQUIRED");
        assert!(!s.file_path.exists());

        let err = s
            .import_old_account_data(encrypted_sample(), Some("wrong-password"))
            .unwrap_err();
        assert_eq!(err, "Import password is incorrect");
        assert!(!s.file_path.exists());

        let summary = s
            .import_old_account_data(encrypted_sample(), Some(SAMPLE_PASSWORD))
            .unwrap();
        assert_eq!(summary.added, 1);
        assert_eq!(ids(&s), vec![111]);
        // The import password is not adopted as the store password.
        assert!(!s.is_encrypted().unwrap(), "the store stays plain");
    }

    #[test]
    fn import_trims_the_import_password_just_like_unlocking() {
        // Este teste fixava o comportamento antigo: `load_with_password` fazia
        // trim e `decode_accounts_for_import` não, então a mesma senha colada
        // com espaço no fim desbloqueava o app mas era recusada no import.
        // Agora as duas normalizam igual.
        let s = store("import-trim");
        let summary = s
            .import_old_account_data(encrypted_sample(), Some(&format!("  {SAMPLE_PASSWORD}\t\n")))
            .expect("a senha com espaços em volta deve ser aceita");
        assert_eq!(summary.added, 1);
        assert_eq!(ids(&s), vec![111]);

        // Uma senha realmente diferente continua sendo recusada.
        let other = store("import-trim-wrong");
        assert_eq!(
            other
                .import_old_account_data(encrypted_sample(), Some(" not-the-password "))
                .unwrap_err(),
            "Import password is incorrect"
        );
    }

    #[test]
    fn import_refuses_to_run_after_a_failed_load() {
        let s = store("import-latched");
        fs::write(&s.file_path, b"corrupt").unwrap();
        assert!(s.load().is_err());

        let payload = serde_json::to_vec(&vec![account(1, "New")]).unwrap();
        let err = s.import_old_account_data(&payload, None).unwrap_err();
        assert!(err.contains("refusing to overwrite"), "{err}");
        assert_eq!(fs::read(&s.file_path).unwrap(), b"corrupt");
    }

    #[test]
    fn import_summary_serializes_in_camel_case_for_the_frontend() {
        let summary = OldAccountImportSummary {
            total: 4,
            added: 2,
            replaced: 1,
            skipped: 1,
        };
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["total"], 4);
        assert_eq!(json["added"], 2);
        assert_eq!(json["replaced"], 1);
        assert_eq!(json["skipped"], 1);
    }

    // ---- decoding helpers -----------------------------------------------------

    #[test]
    fn parse_accounts_json_reports_the_parse_error() {
        let err = AccountStore::parse_accounts_json(b"[{").unwrap_err();
        assert!(err.starts_with("Failed to parse account JSON:"), "{err}");
        assert!(AccountStore::parse_accounts_json(b"[]").unwrap().is_empty());
    }

    #[test]
    fn decode_accounts_for_load_handles_empty_plain_and_encrypted_input() {
        let s = store("decode-load");
        assert!(s.decode_accounts_for_load(b"").unwrap().is_empty());

        let plain = serde_json::to_vec(&vec![account(1, "A")]).unwrap();
        assert_eq!(s.decode_accounts_for_load(&plain).unwrap().len(), 1);

        let err = s.decode_accounts_for_load(encrypted_sample()).unwrap_err();
        assert!(err.contains("Password required"), "{err}");

        *s.session.lock().unwrap() = Some(SessionKey::derive("nope-not-it").unwrap());
        let err = s.decode_accounts_for_load(encrypted_sample()).unwrap_err();
        assert!(err.contains("Failed to decrypt"), "{err}");
    }

    // ---- concurrency ----------------------------------------------------------

    #[test]
    fn concurrent_adds_all_land_in_memory_and_leave_a_readable_file() {
        let s = store("concurrent");
        let store_ref = &s.store;

        std::thread::scope(|scope| {
            for thread in 0..4i64 {
                scope.spawn(move || {
                    for i in 0..5i64 {
                        let id = thread * 100 + i + 1;
                        store_ref.add(account(id, &format!("U{id}"))).unwrap();
                    }
                });
            }
        });

        let mut got = ids(&s);
        got.sort();
        assert_eq!(got.len(), 20, "every add must survive: {got:?}");

        // Este teste aceitava que o arquivo ficasse atrás da memória ("only
        // check it is a subset") porque `save()` reobtinha o lock depois da
        // mutação. Agora a serialização acontece sob o mesmo guard, então o
        // arquivo tem exatamente o que está em memória.
        let mut on_disk: Vec<i64> = serde_json::from_slice::<Vec<Account>>(
            &fs::read(&s.file_path).unwrap(),
        )
        .expect("file stays valid")
        .iter()
        .map(|a| a.user_id)
        .collect();
        on_disk.sort();
        assert_eq!(on_disk, got, "o arquivo não pode ficar atrás da memória");
        assert!(!s.file_path.with_extension("json.tmp").exists());
    }

    #[test]
    fn every_mutation_leaves_the_file_equal_to_the_in_memory_list() {
        // Invariante nova: quando `add`/`update`/`remove`/`reorder` retornam,
        // o arquivo já contém exatamente o snapshot que a mutação produziu —
        // não existe mais janela entre soltar o lock e gravar.
        let s = store("snapshot-invariant");

        let on_disk = |s: &AccountStore| -> Vec<i64> {
            serde_json::from_slice::<Vec<Account>>(&fs::read(&s.file_path).unwrap())
                .expect("arquivo válido")
                .iter()
                .map(|a| a.user_id)
                .collect()
        };

        for id in 1..=4 {
            s.add(account(id, &format!("U{id}"))).unwrap();
            assert_eq!(on_disk(&s), ids(&s));
        }

        assert!(s.update(account(2, "Renamed")).unwrap());
        assert_eq!(on_disk(&s), ids(&s));

        s.reorder(&[4, 1]).unwrap();
        assert_eq!(on_disk(&s), ids(&s));

        assert!(s.remove(1).unwrap());
        assert_eq!(on_disk(&s), ids(&s));
    }

    // ---- derivação de chave ---------------------------------------------------

    #[test]
    fn unlocking_derives_the_key_once_and_every_save_reuses_it() {
        // `save()` chamava `crypto::encrypt`, que sorteia salt e roda argon2i
        // MODERATE (256 MiB) a cada gravação, segurando o lock — mover N contas
        // de grupo congelava a UI N vezes. Agora o argon2 roda só no unlock.
        //
        // O observável direto disso é o salt no arquivo: se a chave fosse
        // re-derivada, cada gravação traria um salt novo.
        let salt_of = |bytes: &[u8]| bytes[crypto::RAM_HEADER.len()..crypto::RAM_HEADER.len() + 16].to_vec();

        let s = store("key-reuse");
        s.load_with_password(SAMPLE_PASSWORD).unwrap();

        s.add(account(1, "One")).unwrap();
        let first = fs::read(&s.file_path).unwrap();
        s.add(account(2, "Two")).unwrap();
        let second = fs::read(&s.file_path).unwrap();
        s.update(account(2, "Two Renamed")).unwrap();
        let third = fs::read(&s.file_path).unwrap();

        assert!(crypto::is_encrypted(&first));
        assert_eq!(salt_of(&first), salt_of(&second), "o salt da sessão é estável");
        assert_eq!(salt_of(&second), salt_of(&third));
        // O nonce, esse sim, continua sorteado a cada gravação.
        let nonce_of = |b: &[u8]| b[crypto::RAM_HEADER.len() + 16..crypto::RAM_HEADER.len() + 40].to_vec();
        assert_ne!(nonce_of(&second), nonce_of(&third), "nonce nunca se repete");

        // O formato do arquivo não mudou: `crypto::decrypt` com o hash da senha
        // continua abrindo, e um store novo relê tudo.
        let decrypted =
            crypto::decrypt(&third, &crypto::hash_password(SAMPLE_PASSWORD)).unwrap();
        let parsed: Vec<Account> = serde_json::from_slice(&decrypted).unwrap();
        assert_eq!(parsed.len(), 2);

        let reloaded = AccountStore::new(s.file_path.clone());
        reloaded.load_with_password(SAMPLE_PASSWORD).unwrap();
        assert_eq!(ids(&reloaded), vec![1, 2]);
        assert_eq!(reloaded.get_all().unwrap()[1].username, "Two Renamed");

        // Cada unlock sorteia um salt novo, então ele não vira uma constante.
        reloaded.add(account(3, "Three")).unwrap();
        assert_ne!(
            salt_of(&fs::read(&s.file_path).unwrap()),
            salt_of(&third),
            "um unlock novo rotaciona o salt"
        );
    }

    #[test]
    fn saving_many_times_costs_far_less_than_one_key_derivation() {
        // Guarda de regressão para a UI travada: um argon2i MODERATE custa
        // centenas de milissegundos; várias gravações juntas têm que custar uma
        // fração disso. A margem é folgada de propósito.
        let s = store("save-cost");
        s.load_with_password(SAMPLE_PASSWORD).unwrap();
        s.add(account(1, "One")).unwrap();

        let derive_started = std::time::Instant::now();
        SessionKey::derive(SAMPLE_PASSWORD).unwrap();
        let one_derivation = derive_started.elapsed();

        let saves_started = std::time::Instant::now();
        for id in 2..=11 {
            s.add(account(id, &format!("U{id}"))).unwrap();
        }
        let ten_saves = saves_started.elapsed();

        assert_eq!(ids(&s).len(), 11);
        assert!(
            ten_saves < one_derivation,
            "10 gravações ({ten_saves:?}) não podem custar mais que uma derivação ({one_derivation:?})"
        );
    }
}
