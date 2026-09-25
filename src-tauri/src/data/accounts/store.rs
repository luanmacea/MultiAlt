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
