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
/// Problema com o `AccountData.key` que a UI precisa mostrar.
///
/// **Estruturado de propósito.** A frase que o usuário lê tem que passar por
/// `t()` e pelo `i18n:extract` (Global Constraint 8), então o backend manda o
/// *código* e o *caminho*, e o catálogo tem a frase de cada código. Uma string em
/// inglês montada aqui nunca seria traduzida.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultKeyWarning {
    /// `writeFailed` | `writeFailedTransient` | `weakWrapper`.
    pub code: String,
    pub path: String,
    /// Detalhe técnico do SO, para o usuário poder reportar. Nunca tem segredo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl VaultKeyWarning {
    fn write_failed(path: &std::path::Path, detail: &str, transient: bool) -> Self {
        Self {
            code: if transient {
                "writeFailedTransient".to_string()
            } else {
                "writeFailed".to_string()
            },
            path: path.display().to_string(),
            detail: Some(detail.to_string()),
        }
    }

    fn weak_wrapper(path: &std::path::Path) -> Self {
        Self {
            code: "weakWrapper".to_string(),
            path: path.display().to_string(),
            detail: None,
        }
    }

    /// A migração para o formato cifrado falhou **fora** do arquivo de chave (a
    /// cópia de segurança ou a regravação do vault). O arquivo continua em texto
    /// puro, com o cookie de todas as contas legível, e antes disto **nada**
    /// aparecia: o `load()` devolvia `Err`, o `lib.rs` fazia `eprintln!`, o
    /// `needs_password()` era `false` e a tela dizia "Device Key". O app subia
    /// normal mentindo sobre o estado do arquivo.
    fn migration_failed(path: &std::path::Path, detail: &str) -> Self {
        Self {
            code: "migrationFailed".to_string(),
            path: path.display().to_string(),
            detail: Some(detail.to_string()),
        }
    }

    /// Gravou, mas sem confirmação do `fsync`.
    fn sync_unconfirmed(path: &std::path::Path) -> Self {
        Self {
            code: "syncUnconfirmed".to_string(),
            path: path.display().to_string(),
            detail: None,
        }
    }

    /// Peso do aviso. O slot é único, então sem isto um aviso brando **rebaixava**
    /// um grave por chegar depois — `writeFailed` ("faça backup agora") virava
    /// `syncUnconfirmed` ("pode perder a última alteração") no mesmo `save_locked`,
    /// e os dois co-ocorrem justamente no mesmo tipo de volume. Sub-avisar é o que
    /// custa contas.
    ///
    /// Cada aviso grave tem um caminho de limpeza pelo **escopo** dele
    /// (`clear_key_warning_for`), então nunca-rebaixar não deixa nada preso.
    fn severity(&self) -> u8 {
        match self.code.as_str() {
            "writeFailed" | "migrationFailed" => 2,
            _ => 1,
        }
    }
}

/// De onde veio o segredo que cifra o vault nesta sessão.
///
/// Importa porque as duas fontes têm regras diferentes: a senha do usuário é
/// pedida na tela de senha e o `.key` não existe; a chave do aparelho abre o
/// vault sozinha e vive naquele arquivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VaultSecret {
    /// Senha digitada pelo usuário (Pass Lock).
    UserPassword,
    /// Chave mestra do arquivo `.key` ao lado do vault.
    DeviceKey,
}

struct SessionKey {
    /// SHA-512 da senha. Barato, e ainda necessário para decriptar arquivos
    /// gravados com outros salts (o próprio arquivo lido no unlock, por ex.).
    password_hash: Vec<u8>,
    salt: sodiumoxide::crypto::pwhash::argon2i13::Salt,
    key: sodiumoxide::crypto::secretbox::Key,
    secret: VaultSecret,
    /// A chave mestra de 32 bytes, só quando o segredo é a chave do aparelho.
    ///
    /// Guardar isto é o que permite **recriar o `.key`** se ele desaparecer com o
    /// app rodando; com só o hash derivado, nada no processo poderia reconstruir o
    /// arquivo. Nunca sai daqui: não há `Debug`, não é serializado e nenhuma
    /// mensagem de erro a inclui.
    master: Option<Vec<u8>>,
}

impl SessionKey {
    /// Espera a senha já normalizada (com trim) pelo chamador, do mesmo jeito
    /// que `crypto::hash_password`.
    fn derive(password: &str) -> Result<Self, String> {
        Self::from_hash(
            crypto::hash_password(password),
            VaultSecret::UserPassword,
            None,
        )
    }

    /// A sessão da chave do aparelho, que carrega a chave mestra junto.
    fn from_master_key(master: Vec<u8>) -> Result<Self, String> {
        let hash = crate::data::vault_key::master_password_hash(&master);
        Self::from_hash(hash, VaultSecret::DeviceKey, Some(master))
    }

    /// Mesma montagem, mas a partir de um hash já pronto — é assim que a chave
    /// mestra do aparelho entra, já que ela não é uma senha digitada.
    fn from_hash(
        password_hash: Vec<u8>,
        secret: VaultSecret,
        master: Option<Vec<u8>>,
    ) -> Result<Self, String> {
        use sodiumoxide::crypto::pwhash::argon2i13;

        let salt = argon2i13::gen_salt();
        let key = crypto::derive_key(&password_hash, salt.as_ref())
            .map_err(|e| format!("Failed to derive key: {}", e))?;
        Ok(Self {
            password_hash,
            salt,
            key,
            secret,
            master,
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
    /// `None` = o vault ainda não foi aberto (trancado, ou nem tentado).
    /// `Some` = há segredo em memória, de senha ou da chave do aparelho.
    /// Ordem de lock em todo o arquivo: `accounts` → `session`.
    session: Mutex<Option<SessionKey>>,
    file_path: PathBuf,
    /// Set when the on-disk file exists but could not be decoded. While set,
    /// `save()` refuses to write so an empty in-memory list never overwrites
    /// the user's accounts.
    load_failed: std::sync::atomic::AtomicBool,
    /// Motivo para recusar gravação até o app reiniciar, quando o **arquivo**
    /// está bom e é a memória que está velha (ver `lock_writes_until_restart`).
    write_block: Mutex<Option<String>>,
    /// Problema com o **arquivo de chave** que a UI precisa mostrar.
    ///
    /// `eprintln!` numa build GUI não vai a lugar nenhum, e o `.key` ruim é
    /// justamente o defeito que passa o dia inteiro invisível (o master está em
    /// memória) para virar lockout no boot seguinte. Isto existe para o usuário
    /// saber **no dia em que o arquivo fica ruim**.
    key_warning: Mutex<Option<VaultKeyWarning>>,
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
            write_block: Mutex::new(None),
            key_warning: Mutex::new(None),
        }
    }

    /// Onde fica a chave mestra deste vault. Segue o **arquivo**, não um caminho
    /// fixo, então o modo portátil leva os dois juntos.
    fn key_file_path(&self) -> PathBuf {
        crate::data::vault_key::key_file_path_for(&self.file_path)
    }

    /// Cópia do vault em `AccountData.json.bak`, **antes** da migração.
    ///
    /// Erro aqui **aborta** a migração: sem rede de segurança não se troca o
    /// formato do arquivo que guarda as contas do usuário. Esta é a única cópia em
    /// **texto puro** que o app faz, ou seja, a única que abre sem chave nenhuma —
    /// e por isso a única que vale como rede. `set_password` não faz cópia (ver lá
    /// o porquê), então não existe mais um sufixo para escolher.
    fn backup_vault_file(&self) -> Result<(), String> {
        if !self.file_path.exists() {
            return Ok(());
        }
        let backup = self.file_path.with_extension("json.bak");
        fs::copy(&self.file_path, &backup).map_err(|e| {
            format!(
                "Failed to back up the account file to {}: {}",
                backup.display(),
                e
            )
        })?;
        Ok(())
    }

    /// Mensagem para o caso em que o vault existe, está cifrado e **nada** abre.
    ///
    /// A presença do `.key` separa os dois motivos, e eles pedem respostas
    /// diferentes do usuário: sem `.key` é vault de senha (digite a senha); com
    /// `.key` que não abre, a chave deste aparelho se perdeu (Windows
    /// reinstalado, perfil novo, arquivo trocado por antivírus) e o caminho é a
    /// cópia em `.json.bak` ou um backup do app.
    fn locked_vault_message(&self) -> String {
        let key_path = self.key_file_path();
        if !key_path.exists() {
            return "Password required for encrypted file".to_string();
        }

        // Só citar o `.json.bak` quando ele **existe**: numa instalação que nasceu
        // cifrada esse arquivo nunca existiu, e mandar restaurá-lo é mandar a
        // pessoa caçar um arquivo inexistente exatamente no momento de pânico.
        let plain_backup = self.file_path.with_extension("json.bak");
        let where_to_look = if plain_backup.exists() {
            format!(
                "Restore the copy at {}, or a backup from Settings > Misc > Data",
                plain_backup.display()
            )
        } else {
            "Restore a backup from Settings > Misc > Data (it contains both the account file and its key)"
                .to_string()
        };

        format!(
            "The account vault is encrypted with this device's key and that key could not be recovered ({}). \
             Nothing was deleted or overwritten. {}, on the machine that created it — or start over by moving \
             both files aside.",
            key_path.display(),
            where_to_look
        )
    }

    /// Abre o `.key` e **regrava os dois embrulhos** com os identificadores de
    /// agora. É o único ponto que recupera a chave mestra.
    ///
    /// A regravação é **incondicional**, e isso é o ponto: o embrulho que não foi
    /// usado para abrir pode estar morto sem ninguém notar. O caso que custa as
    /// contas é o silencioso — o usuário renomeia o PC, o blob do aparelho fica
    /// preso ao nome antigo, **o DPAPI continua abrindo e nada parece errado**; um
    /// perfil recriado meses depois (justamente o caso para o qual o segundo
    /// embrulho existe) não encontra caminho nenhum. Conferir sem regravar
    /// custaria o mesmo argon2 que regravar, então não há motivo para só conferir.
    ///
    /// Falha na regravação **não** é fatal: a chave em memória continua boa e o
    /// `.key` de antes continua no disco.
    fn recover_and_refresh_master_key(&self) -> Option<Vec<u8>> {
        let key_path = self.key_file_path();
        let recovered = crate::data::vault_key::load_master_key(&key_path)?;
        self.refresh_key_file(&recovered.master);
        Some(recovered.master)
    }

    /// Regrava o `.key` com os embrulhos de agora e **registra o que deu errado**
    /// onde a UI consiga ler. Devolve `true` quando o arquivo ficou bom.
    fn refresh_key_file(&self, master: &[u8]) -> bool {
        let key_path = self.key_file_path();
        match crate::data::vault_key::store_master_key(
            &key_path,
            master,
            &crypto::primary_device_hash(),
        ) {
            Ok(health) => {
                if cfg!(target_os = "windows") && !health.dpapi_present {
                    // O `.key` ficou só com o embrulho fraco. Não é perda, mas é
                    // degradação — e degradar calado foi exatamente o problema.
                    self.set_key_warning(VaultKeyWarning::weak_wrapper(&key_path));
                } else if !health.synced {
                    self.set_key_warning(VaultKeyWarning::sync_unconfirmed(&key_path));
                } else {
                    // **Com escopo.** Este reparo resolve o `.key` e mais nada:
                    // limpar o slot inteiro aqui apagava um `migrationFailed` (que
                    // fala do `AccountData.json`) antes de a gravação do vault
                    // sequer acontecer. A invariante é que nenhum caminho limpa
                    // aviso de escopo alheio.
                    self.clear_key_warning_for(&key_path);
                }
                true
            }
            Err(e) => {
                // Transitório (antivírio/indexador segurando o handle) recebe o
                // tom brando: a gravação seguinte tenta de novo e limpa o aviso.
                // Alarmar alto num arquivo que está perfeito treina o usuário a
                // ignorar avisos, e aí a rede contra o lockout não vale nada.
                self.set_key_warning(VaultKeyWarning::write_failed(
                    &key_path,
                    &e.message,
                    e.transient,
                ));
                false
            }
        }
    }

    /// O mutex do aviso é **destravado do envenenamento** de propósito.
    ///
    /// `if let Ok(..)` descartava a escrita e `.lock().ok()` respondia "tudo em
    /// ordem" com o mutex envenenado: falha não observável **no canal que existe
    /// para observar falhas**. O dado aqui é um aviso, não invariante de
    /// segurança — recuperá-lo com `into_inner()` é sempre melhor que engolir.
    fn key_warning_slot(&self) -> std::sync::MutexGuard<'_, Option<VaultKeyWarning>> {
        self.key_warning
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn set_key_warning(&self, warning: VaultKeyWarning) {
        let mut slot = self.key_warning_slot();
        if let Some(current) = slot.as_ref() {
            // Idêntico: não repete nem torna a logar. Sem isto, um volume que
            // nunca confirma o `fsync` escrevia a mesma linha a cada gravação.
            if *current == warning {
                return;
            }
            // Nunca rebaixa a gravidade (ver `VaultKeyWarning::severity`).
            if current.severity() > warning.severity() {
                return;
            }
        }
        eprintln!("Aviso ({}): {}", warning.code, warning.path);
        *slot = Some(warning);
    }

    fn clear_key_warning(&self) {
        *self.key_warning_slot() = None;
    }

    /// Limpa o aviso **só se ele for sobre este arquivo**.
    ///
    /// O slot é único mas os avisos falam de arquivos diferentes: `writeFailed` e
    /// `weakWrapper` são do `.key`, `migrationFailed` é do `AccountData.json`.
    /// Limpar sem escopo fazia o braço "o `.key` está saudável" apagar a faixa
    /// vermelha que dizia que o vault continua em texto puro — e apagar **antes**
    /// de a gravação acontecer, então nem o sucesso justificava.
    fn clear_key_warning_for(&self, path: &std::path::Path) {
        let mut slot = self.key_warning_slot();
        let belongs = slot
            .as_ref()
            .is_some_and(|w| w.path == path.display().to_string());
        if belongs {
            *slot = None;
        }
    }

    /// O que a UI mostra sobre o arquivo de chave. `None` = tudo em ordem.
    ///
    /// Estruturado (código + caminho), não frase pronta: a frase mora no catálogo
    /// de i18n e passa por `t()`, como todo texto que o usuário lê.
    pub fn vault_key_warning(&self) -> Option<VaultKeyWarning> {
        self.key_warning_slot().clone()
    }

    /// Recusa a operação quando a gravação está trancada.
    ///
    /// Checado na **entrada** de `set_password`: só olhar dentro de `save_locked`
    /// deixava "somente leitura" não ser literal — ele já mexia em arquivo (cópia,
    /// e a regravação do `.key`) antes de descobrir que não podia gravar.
    fn ensure_writable(&self) -> Result<(), String> {
        let block = self.write_block.lock().map_err(|e| e.to_string())?;
        match block.as_deref() {
            Some(reason) => Err(format!(
                "Refusing to write the account file: {}. Restart the app before changing accounts.",
                reason
            )),
            None => Ok(()),
        }
    }

    /// A gravação está trancada agora?
    ///
    /// Existe para quem **tranca** poder saber se a trava já era de outro: soltar
    /// uma trava alheia é o mesmo que nunca tê-la ligado.
    pub fn writes_locked(&self) -> bool {
        self.write_block
            .lock()
            .map(|slot| slot.is_some())
            // Mutex envenenado: assumir trancado é o lado seguro.
            .unwrap_or(true)
    }

    /// Libera a gravação depois de o arquivo ter sido **relido de verdade**.
    ///
    /// Contraparte de [`Self::lock_writes_until_restart`]. Existe porque a
    /// restauração de backup passou a trancar **por padrão**: é este o único ponto
    /// que destranca, e só quem releu o arquivo pode chamá-lo.
    pub fn allow_writes_after_reload(&self) {
        if let Ok(mut slot) = self.write_block.lock() {
            *slot = None;
        }
    }

    /// Recusa **toda** gravação até o app reiniciar.
    ///
    /// Diferente de `load_failed`: ali o arquivo em disco é que está ruim; aqui o
    /// arquivo está ótimo e é a **memória** que está velha. É o caso da
    /// restauração de backup — o segredo da sessão não é mais o do arquivo, e
    /// qualquer gravação (um launch já chama `mark_used`) cifraria o vault
    /// restaurado com o segredo antigo, deixando-o sem abrir no próximo boot.
    pub fn lock_writes_until_restart(&self, reason: &str) {
        if let Ok(mut slot) = self.write_block.lock() {
            *slot = Some(reason.to_string());
        }
    }

    /// Estabelece a sessão da **chave do aparelho**, reaproveitando o `.key` que
    /// já existir e criando um novo só quando não houver nenhum.
    ///
    /// Idempotente de propósito: se o processo morrer entre gravar o `.key` e
    /// regravar o vault, a abertura seguinte cai aqui de novo, reencontra a mesma
    /// chave e termina a migração.
    fn ensure_device_session(&self) -> Result<(), String> {
        {
            let session = self.session.lock().map_err(|e| e.to_string())?;
            if session.is_some() {
                return Ok(());
            }
        }

        let session = self.build_device_session()?;
        let mut slot = self.session.lock().map_err(|e| e.to_string())?;
        *slot = Some(session);
        Ok(())
    }

    /// Pode existir vault cifrado a perder? **Erro conta como "sim".**
    ///
    /// Isto guarda a decisão de sortear uma chave mestra nova, que destrói a única
    /// cópia da antiga. `is_encrypted().unwrap_or(false)` era **fail-open** nessa
    /// guarda: "não consegui saber" virava "não existe". As duas leituras falham
    /// juntas com mais frequência do que parece — varredura de antivírus e
    /// rehidratação do OneDrive pegam a **pasta inteira**, então `.key` ilegível e
    /// `AccountData.json` ilegível são eventos correlacionados, não independentes.
    /// Se depois disso a regravação do vault falhasse com o `.key` novo já
    /// publicado, o resultado era lockout permanente de todas as contas.
    fn vault_may_hold_data(&self) -> bool {
        match self.is_encrypted() {
            Ok(encrypted) => encrypted,
            Err(_) => true,
        }
    }

    /// Monta a sessão da chave do aparelho sem tocar no que já está em memória.
    ///
    /// Separado de [`Self::ensure_device_session`] para `set_password(None)`
    /// poder falhar **sem** derrubar a sessão da senha que o usuário já tinha.
    fn build_device_session(&self) -> Result<SessionKey, String> {
        let key_path = self.key_file_path();
        let master = match self.recover_and_refresh_master_key() {
            Some(master) => master,
            // Só é proibido sortear chave nova quando **há vault cifrado a
            // perder**: aí um `.key` que não abre pode ser a única cópia da chave
            // daquele arquivo, e substituí-lo o tornaria ilegível para sempre.
            // Antes a condição era só `key_path.exists()`, o que travava também a
            // **instalação nova** quando havia qualquer coisa no lugar do `.key`
            // (um diretório, por exemplo) — sem vault nenhum para proteger.
            None if key_path.exists() && self.vault_may_hold_data() => {
                return Err(self.locked_vault_message());
            }
            None => {
                let master = crate::data::vault_key::generate_master_key();
                // O `.key` é gravado **antes** de o vault ser cifrado com ele: na
                // ordem contrária, uma falha aqui deixaria um vault que ninguém
                // abre. E passa por `refresh_key_file` justamente para o health e o
                // erro chegarem ao aviso da UI: este é o **primeiro boot** e o
                // `set_password(None)`, e antes os dois degradavam calados (o
                // health era descartado) ou morriam num `eprintln!` do `lib.rs`.
                if !self.refresh_key_file(&master) {
                    return Err(format!(
                        "The account key file ({}) could not be created, so the account file was \
                         left unencrypted. See the warning on screen.",
                        key_path.display()
                    ));
                }
                master
            }
        };

        SessionKey::from_master_key(master)
    }

    /// Os bytes no disco começam com um header RAM? Fato bruto do arquivo, usado
    /// pelas guardas de gravação — **não** é "tem senha" (ver
    /// [`Self::has_user_password`]).
    pub fn is_encrypted(&self) -> Result<bool, String> {
        if !self.file_path.exists() {
            return Ok(false);
        }

        let data =
            fs::read(&self.file_path).map_err(|e| format!("Failed to read account file: {}", e))?;

        Ok(crypto::is_encrypted(&data))
    }

    /// O vault é protegido por **senha do usuário**?
    ///
    /// É isso que a tela de criptografia mostra. Desde que o vault sem senha
    /// também é cifrado, `is_encrypted()` deixou de responder essa pergunta: ela
    /// é "true" nos dois casos. O sinal é o arquivo `.key` — ele existe quando a
    /// chave é do aparelho e é removido quando o usuário define uma senha.
    pub fn has_user_password(&self) -> Result<bool, String> {
        {
            let session = self.session.lock().map_err(|e| e.to_string())?;
            if let Some(session) = session.as_ref() {
                return Ok(session.secret == VaultSecret::UserPassword);
            }
        }
        if !self.is_encrypted()? {
            return Ok(false);
        }
        Ok(!self.key_file_path().exists())
    }

    /// O usuário precisa digitar a senha para o app abrir as contas?
    ///
    /// Depois de `load()`, "sim" significa: o arquivo está cifrado e nada em
    /// memória abre. Isso cobre tanto o vault de senha (caso normal) quanto o
    /// vault cuja chave de aparelho se perdeu — nos dois a tela de senha é a
    /// tela certa, e a mensagem de erro do unlock explica a diferença.
    pub fn needs_password(&self) -> Result<bool, String> {
        let session = self.session.lock().map_err(|e| e.to_string())?;
        if session.is_some() {
            return Ok(false);
        }
        drop(session);
        self.is_encrypted()
    }

    /// Abre o vault sem senha do usuário: chave do aparelho, ou JSON puro (que é
    /// migrado na hora).
    pub fn load(&self) -> Result<(), String> {
        let data = if self.file_path.exists() {
            fs::read(&self.file_path).map_err(|e| format!("Failed to read account file: {}", e))?
        } else {
            Vec::new()
        };

        if data.is_empty() {
            // Instalação nova (ou arquivo de 0 byte, que é um vazio legítimo): o
            // vault já nasce com a chave do aparelho, senão o primeiro `add`
            // gravaria JSON puro e a migração nunca aconteceria.
            //
            // Falhar aqui **não** trava o app: sem chave, o store segue sem
            // segredo e grava em texto puro, como antes desta mudança. É uma
            // proteção a menos, não um usuário sem acesso às contas.
            return self.ensure_device_session();
        }

        if crypto::is_encrypted(&data) {
            return self.load_encrypted(&data);
        }

        // JSON puro (ou DPAPI legado do RAM v3): ler e migrar.
        let accounts = match Self::decode_plain_or_legacy_accounts(&data) {
            Ok(accounts) => accounts,
            Err(e) => {
                self.load_failed
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                return Err(e);
            }
        };

        let mut store = self.accounts.lock().map_err(|e| e.to_string())?;
        *store = accounts;
        drop(store);
        self.mark_memory_fresh();

        self.migrate_plain_vault()
    }

    /// A memória acabou de ser lida do disco, então ela **não está velha**.
    ///
    /// É o que solta o `write_block`, e é por isso que a migração de um vault em
    /// texto puro restaurado de backup consegue rodar: o latch existe para impedir
    /// que memória velha sobrescreva o arquivo, e aqui a memória é o arquivo. Sem
    /// isto, a trava que a restauração liga bloqueava o `save_locked` da própria
    /// migração — `load()` devolvia "não foi possível reler" **depois** de ler com
    /// sucesso, e o caminho "texto puro recarrega na hora" ficava inalcançável.
    fn mark_memory_fresh(&self) {
        self.load_failed
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.allow_writes_after_reload();
    }

    /// Regrava um vault em texto puro como vault cifrado.
    ///
    /// A ordem existe para nenhuma queda no meio custar contas:
    /// 1. `.json.bak` — sem backup, não migra;
    /// 2. `.key` — a chave antes do arquivo que ela cifra;
    /// 3. regravação do vault, atômica (`save_locked`).
    ///
    /// Morrer entre 1 e 2, ou entre 2 e 3, deixa o vault **em texto puro** e a
    /// próxima abertura recomeça daqui com a mesma chave. Morrer dentro de 3 não
    /// existe: a troca é atômica, o arquivo é o de antes ou o de depois.
    fn migrate_plain_vault(&self) -> Result<(), String> {
        // **Toda** falha daqui tem que virar aviso na tela, não só `Err`.
        //
        // Este era o pior fail-open que sobrou: `load()` devolvia `Err`, o boot
        // fazia `eprintln!` (invisível em build GUI), o `mark_memory_fresh` já
        // tinha limpado o `load_failed`, `needs_password()` era `false` porque o
        // arquivo está em texto puro, e nenhum `key_warning` era setado. Resultado:
        // o app subia normal, a tela de criptografia dizia "Device Key", e o
        // `AccountData.json` continuava em texto puro com o `.ROBLOSECURITY` de
        // todas as contas — sem um sinal em lugar nenhum. É o mesmo padrão que a
        // Quebra 1 mandou matar, sobrevivendo no caminho principal desta tarefa.
        self.backup_vault_file().map_err(|e| {
            self.set_key_warning(VaultKeyWarning::migration_failed(&self.file_path, &e));
            e
        })?;

        if let Err(e) = self.ensure_device_session() {
            // Sem chave o arquivo fica como está — em texto puro, legível, mas
            // **inteiro**. Perder o arquivo é pior que ficar sem a criptografia.
            // `ensure_device_session` já avisou pelo `refresh_key_file`; aqui só
            // garantimos que o aviso exista mesmo se a falha vier de outro ponto.
            if self.vault_key_warning().is_none() {
                self.set_key_warning(VaultKeyWarning::migration_failed(&self.file_path, &e));
            }
            return Err(format!(
                "Accounts are loaded, but the file could not be encrypted and was left as-is: {}",
                e
            ));
        }

        let accounts = self.accounts.lock().map_err(|e| e.to_string())?;
        self.save_locked(&accounts).map_err(|e| {
            self.set_key_warning(VaultKeyWarning::migration_failed(&self.file_path, &e));
            e
        })
    }

    /// Abre um vault já cifrado sem senha do usuário.
    fn load_encrypted(&self, data: &[u8]) -> Result<(), String> {
        // Sessão já estabelecida (unlock anterior): usa o segredo que está lá.
        let session_hash = {
            let session = self.session.lock().map_err(|e| e.to_string())?;
            session.as_ref().map(|s| s.password_hash.clone())
        };
        if let Some(hash) = session_hash {
            let Ok(decrypted) = crypto::decrypt(data, &hash) else {
                // O segredo em memória não abre o arquivo que está no disco: a
                // memória está velha (arquivo restaurado por fora, por exemplo).
                // Latchar é obrigatório — sem isso a gravação seguinte cifraria o
                // arquivo novo com o segredo antigo e o próximo boot não abriria.
                self.load_failed
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                return Err(
                    "The account file on disk was not written by this session's key; \
                     refusing to touch it. Restart the app."
                        .to_string(),
                );
            };
            return self.commit_loaded(decrypted);
        }

        let key_path = self.key_file_path();
        if !key_path.exists() {
            // Sem arquivo de chave o vault é de senha. Não vale gastar um argon2
            // por candidato de aparelho a cada boot para descobrir isso.
            self.load_failed
                .store(true, std::sync::atomic::Ordering::SeqCst);
            return Err(self.locked_vault_message());
        }

        let Some(master) = self.recover_and_refresh_master_key() else {
            // Chave irrecuperável: **não** apagar, **não** regravar, **não**
            // travar o app. Só dizer o que houve e onde está a cópia.
            self.load_failed
                .store(true, std::sync::atomic::Ordering::SeqCst);
            return Err(self.locked_vault_message());
        };

        let hash = crate::data::vault_key::master_password_hash(&master);
        let Ok(decrypted) = crypto::decrypt(data, &hash) else {
            // O `.key` abriu **mas não é a chave deste vault**. Dois caminhos
            // chegam aqui, e a mensagem tem que servir aos dois:
            //  - `.key` órfão de um `set_password` interrompido → o vault é de
            //    senha e basta digitá-la;
            //  - vault cifrado por **outra** chave de aparelho (o usuário copiou um
            //    `AccountData.json` antigo por cima, ou restaurou um vault de um zip
            //    sem a chave dele) → nunca houve senha, e pedir senha e parar é um
            //    beco sem saída.
            // Nada é gravado nos dois casos (o latch abaixo garante).
            self.load_failed
                .store(true, std::sync::atomic::Ordering::SeqCst);
            return Err(format!(
                "Password required for encrypted file. If you never set a password, this file was \
                 encrypted with a different device key — restore the matching {} next to it (both \
                 come in the same backup zip). Nothing was deleted or overwritten.",
                self.key_file_path().display()
            ));
        };

        let session = SessionKey::from_master_key(master)?;
        let mut slot = self.session.lock().map_err(|e| e.to_string())?;
        *slot = Some(session);
        drop(slot);

        self.commit_loaded(decrypted)
    }

    fn commit_loaded(&self, decrypted: Vec<u8>) -> Result<(), String> {
        let accounts = match Self::parse_accounts_json(&decrypted) {
            Ok(accounts) => accounts,
            Err(e) => {
                self.load_failed
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                return Err(e);
            }
        };
        let mut store = self.accounts.lock().map_err(|e| e.to_string())?;
        *store = accounts;
        drop(store);
        self.mark_memory_fresh();
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

        let was_encrypted = crypto::is_encrypted(&data);
        let accounts = if was_encrypted {
            let decrypted = crypto::decrypt(&data, &hash).map_err(|_| {
                // Senha errada é o caso comum; vault cifrado pela chave de um
                // aparelho que não existe mais é o caso raro e grave, e pedir a
                // senha de novo não resolve. A mensagem tem que separar os dois.
                if self.key_file_path().exists() {
                    self.locked_vault_message()
                } else {
                    // Sem `.key` os dois motivos são indistinguíveis pelos
                    // arquivos: senha errada, ou vault **sem** senha cujo `.key`
                    // foi apagado (usuário, antivírus, limpeza de disco). O
                    // segundo não se resolve digitando de novo, e quem não souber
                    // disso vai ficar tentando senhas até desistir.
                    format!(
                        "Failed to decrypt: wrong password. If this vault never had a password, \
                         its device key file is missing — restore {} (together with the account \
                         file) from a backup instead of retyping.",
                        self.key_file_path().display()
                    )
                }
            })?;
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
        drop(slot);

        if was_encrypted {
            // A senha abriu o arquivo, então este vault é de senha — e qualquer
            // `.key` ao lado dele é órfão (sobra de um `set_password` que morreu
            // entre gravar o vault e apagar a chave). Sem isto ele fica para
            // sempre, e todo boot acusa "a chave deste aparelho não pôde ser
            // recuperada" para quem só precisa digitar a senha. A senha é a
            // autoridade, igual em `set_password(Some)`.
            crate::data::vault_key::remove_key_file(&self.key_file_path());
        }
        Ok(())
    }

    pub fn save(&self) -> Result<(), String> {
        let accounts = self.accounts.lock().map_err(|e| e.to_string())?;
        self.save_locked(&accounts)
    }

    /// Serializa e grava o snapshot **que o chamador ainda está segurando**.
    ///
    /// `add`/`remove`/`update`/`reorder` soltavam o lock antes de `save()`
    /// reobtê-lo, então duas escritas concorrentes podiam intercalar e deixar o
    /// arquivo uma entrada atrás da memória. Mantendo o guard vivo até o
    /// `atomic_replace`, o que vai para o disco é sempre o estado que acabou de
    /// ser produzido.
    ///
    /// Sem segredo em memória a gravação só é permitida quando o arquivo **não**
    /// está cifrado: é o caminho degradado de quem não conseguiu criar a chave do
    /// aparelho (ver `load`). Com o arquivo cifrado e nada em memória, gravar
    /// seria trocar as contas do usuário por uma lista vazia.
    fn save_locked(&self, accounts: &[Account]) -> Result<(), String> {
        if self.load_failed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(
                "Account file could not be loaded; refusing to overwrite it. Fix or restore AccountData.json and restart.".to_string(),
            );
        }
        {
            let block = self.write_block.lock().map_err(|e| e.to_string())?;
            if let Some(reason) = block.as_deref() {
                return Err(format!(
                    "Refusing to write the account file: {}. Restart the app before changing accounts.",
                    reason
                ));
            }
        }

        let json = serde_json::to_string_pretty(accounts)
            .map_err(|e| format!("Failed to serialize accounts: {}", e))?;

        let session = self.session.lock().map_err(|e| e.to_string())?;

        // O `.key` pode ter desaparecido **ou ficado ilegível** com o app rodando
        // (antivírus, limpeza de disco, queda de energia no meio da regravação).
        // Sem isto, o app seguiria gravando o dia todo um vault que ninguém mais
        // abre — e cada backup automático criado depois levaria o vault **sem** a
        // chave dele, até a poda apagar os backups bons. É por isso que a
        // `SessionKey` guarda a chave mestra, não só o hash dela.
        //
        // A condição é "o `.key` guarda **a chave desta sessão**". Dois defeitos
        // reais moldaram isso:
        //  - "não existe" era o primeiro, e deixava passar o arquivo truncado;
        //  - "abre" deixava passar o `.key` que abre para **outra** chave — o caso
        //    da pasta de dados em OneDrive/Dropbox, em que o `.key` volta a uma
        //    versão anterior enquanto o vault fica novo. O vault seguia sendo
        //    gravado com o master da sessão e o boot seguinte pedia uma senha que
        //    nunca existiu.
        //
        // Qualquer outro estado **tenta** o reparo, inclusive "não consegui ler
        // agora": ler falhar não quer dizer que escrever vai falhar, e pular
        // deixaria um arquivo ruim sem reparo enquanto o processo vivesse. Quem
        // separa o transitório do grave é o **tom do aviso** (`is_transient_io`,
        // medido), não o pular.
        //
        // No caminho saudável isto custa uma leitura e uma chamada de DPAPI (sem
        // argon2); só o caminho quebrado paga caro, e paga uma vez.
        if let Some(session) = session.as_ref() {
            if let Some(master) = session.master.as_ref() {
                if crate::data::vault_key::key_file_holds_master(&self.key_file_path(), master) {
                    // Um aviso **sobre o `.key`** que tenha sobrado de uma falha
                    // anterior sai da tela. Com escopo: sem ele, isto apagava
                    // também o aviso de que o `AccountData.json` continua em texto
                    // puro, e apagava antes de a gravação abaixo dar certo.
                    self.clear_key_warning_for(&self.key_file_path());
                } else {
                    self.refresh_key_file(master);
                }
            }
        }

        let wrote_encrypted = session.is_some();
        let data = if let Some(session) = session.as_ref() {
            session.encrypt(&json)?
        } else {
            // Still locked: never replace an encrypted file with plaintext (it
            // would drop every account the user has not unlocked yet).
            if self.is_encrypted()? {
                return Err("Accounts are locked; unlock them before making changes.".to_string());
            }
            json.into_bytes()
        };

        // Write-then-fsync-then-rename: sem o fsync, o rename pode publicar um
        // arquivo cujo conteúdo ainda está em cache, e a queda de energia entre os
        // dois deixaria um `AccountData.json` que existe e está vazio.
        let tmp_path = self.file_path.with_extension("json.tmp");
        let synced = crate::data::versions::write_all_synced(&tmp_path, &data)
            .map_err(|e| format!("Failed to write account file: {}", e))?;
        crate::data::versions::atomic_replace(&tmp_path, &self.file_path)
            .map_err(|e| format!("Failed to replace account file: {}", e))?;

        // **Só depois de o arquivo estar no lugar.** Um aviso sobre o vault (o
        // "continua em texto puro" da migração) só pode ser considerado resolvido
        // por uma gravação que (a) deu certo e (b) foi **cifrada** — no caminho
        // degradado o arquivo continua legível, e limpar ali seria mentir.
        if wrote_encrypted {
            self.clear_key_warning_for(&self.file_path);
        }
        if !synced {
            // A gravação vale (os bytes foram entregues ao SO); o que se perde é a
            // garantia contra queda de energia. Descartar este `bool` tornava a
            // degradação 100% invisível. `set_key_warning` não deixa isto rebaixar
            // um aviso grave nem repetir a mesma linha a cada gravação.
            self.set_key_warning(VaultKeyWarning::sync_unconfirmed(&self.file_path));
        }

        Ok(())
    }

    pub fn set_password(&self, password: Option<&str>) -> Result<(), String> {
        // Antes de qualquer validação que mexa em arquivo: com a gravação trancada
        // (restauração de backup) isto não pode nem começar.
        self.ensure_writable()?;

        if let Some(value) = password {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Err("Password cannot be empty".to_string());
            }
            if trimmed.chars().count() < 8 {
                return Err("Password must be at least 8 characters".to_string());
            }
        }
        let key_path = self.key_file_path();

        {
            let slot = self.session.lock().map_err(|e| e.to_string())?;
            if slot.is_none() && self.is_encrypted()? {
                // Re-keying a file we never decrypted would encrypt an empty list
                // over the user's accounts.
                return Err(
                    "Accounts are locked; unlock them before changing the password.".to_string(),
                );
            }
        }

        // **Aqui não se faz cópia**, e isso é decisão, não esquecimento.
        //
        // A cópia que existia (`.json.rekey.bak`) era cifrada pela chave do
        // aparelho e o `.key` era apagado três linhas depois: um arquivo que nunca
        // mais abria, que ninguém limpava e que nada avisava — exatamente o
        // "arquivo com cara de backup que não abre com nada" que o comentário da
        // rodada anterior usou como argumento para renomeá-la. Renomear não
        // resolveu; eliminar resolve.
        //
        // O que protege esta operação não é uma cópia: é (a) a gravação atômica,
        // que deixa o arquivo antigo intacto em qualquer falha, e (b) a ordem
        // "chave antes do arquivo que ela cifra". A cópia em texto puro da
        // migração (`.json.bak`) continua sendo a rede de verdade, e continua
        // intocada aqui.
        //
        // Sobra de versão anterior do app sai de cena, para ninguém confiar nela.
        let _ = fs::remove_file(self.file_path.with_extension("json.rekey.bak"));

        match password {
            Some(value) => {
                {
                    let mut slot = self.session.lock().map_err(|e| e.to_string())?;
                    *slot = Some(SessionKey::derive(value.trim())?);
                }
                // Solta `session` antes de pegar `accounts`: a ordem de lock do
                // resto do arquivo é accounts → session, e inverter aqui criaria
                // deadlock.
                let accounts = self.accounts.lock().map_err(|e| e.to_string())?;
                self.save_locked(&accounts)?;
                drop(accounts);
                // O `.key` só sai **depois** de o vault estar gravado com a
                // senha. Na ordem contrária, uma falha na gravação deixaria um
                // vault cifrado pela chave do aparelho sem a chave para abri-lo.
                crate::data::vault_key::remove_key_file(&key_path);
                // A partir daqui o `.key` não importa mais, então um aviso sobre
                // ele é falso alarme. Sem isto, quem estava com a faixa vermelha e
                // definia uma senha ficava com ela na tela o resto da sessão,
                // apontando para um arquivo que acabou de ser apagado — e nenhum
                // caminho de sessão-com-senha alcança o `clear_key_warning`.
                self.clear_key_warning();
                Ok(())
            }
            None => {
                // Tirar a senha volta para a chave do aparelho, **não** para
                // texto puro. A chave é montada (e o `.key` gravado) antes de a
                // sessão da senha ser trocada: erro aqui deixa o arquivo e a
                // sessão exatamente como estavam.
                let device_session = self.build_device_session()?;
                {
                    let mut slot = self.session.lock().map_err(|e| e.to_string())?;
                    *slot = Some(device_session);
                }
                let accounts = self.accounts.lock().map_err(|e| e.to_string())?;
                self.save_locked(&accounts)
            }
        }
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
        self.save_locked(&accounts)
    }

    pub fn remove(&self, user_id: i64) -> Result<bool, String> {
        let mut accounts = self.accounts.lock().map_err(|e| e.to_string())?;
        let initial_len = accounts.len();
        accounts.retain(|a| a.user_id != user_id);
        let removed = accounts.len() < initial_len;

        if removed {
            self.save_locked(&accounts)?;
        }

        Ok(removed)
    }

    pub fn update(&self, account: Account) -> Result<bool, String> {
        let mut accounts = self.accounts.lock().map_err(|e| e.to_string())?;

        if let Some(existing) = accounts.iter_mut().find(|a| a.user_id == account.user_id) {
            *existing = account;
            self.save_locked(&accounts)?;
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

        self.save_locked(&accounts)?;
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

        self.save_locked(&accounts)
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

    fn decode_accounts_for_import(
        &self,
        data: &[u8],
        import_password: Option<&str>,
    ) -> Result<Vec<Account>, String> {
        if data.is_empty() {
            return Ok(Vec::new());
        }

        if crypto::is_encrypted(data) {
            if let Some(password) = import_password {
                // Mesmo trim de `load_with_password`: a senha colada com espaço no
                // fim desbloqueava o app mas era recusada no import.
                let hash = crypto::hash_password(password.trim());
                let decrypted = crypto::decrypt(data, &hash)
                    .map_err(|_| "Import password is incorrect".to_string())?;
                return Self::parse_accounts_json(&decrypted);
            }

            // Sem senha: o arquivo pode ser um backup **deste** vault, cifrado
            // pela chave do aparelho. Tentar os segredos que já temos resolve o
            // caso normal de "exportei e reimportei nesta máquina" sem pedir uma
            // senha que nunca existiu.
            for hash in self.own_secret_hashes()? {
                if let Ok(decrypted) = crypto::decrypt(data, &hash) {
                    return Self::parse_accounts_json(&decrypted);
                }
            }

            return Err(IMPORT_PASSWORD_REQUIRED.to_string());
        }

        Self::decode_plain_or_legacy_accounts(data)
    }

    /// Hashes que este app pode ter usado para cifrar um vault: o da sessão atual
    /// e o da chave mestra em disco. Nunca vaza a chave — só o hash derivado, que
    /// é o que `crypto::decrypt` consome.
    fn own_secret_hashes(&self) -> Result<Vec<Vec<u8>>, String> {
        let mut hashes: Vec<Vec<u8>> = Vec::new();
        {
            let session = self.session.lock().map_err(|e| e.to_string())?;
            if let Some(session) = session.as_ref() {
                hashes.push(session.password_hash.clone());
            }
        }
        if let Some(recovered) = crate::data::vault_key::load_master_key(&self.key_file_path()) {
            let hash = crate::data::vault_key::master_password_hash(&recovered.master);
            if !hashes.contains(&hash) {
                hashes.push(hash);
            }
        }
        Ok(hashes)
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
            self.save_locked(&accounts)?;
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
