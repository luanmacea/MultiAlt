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

/// O que a remoção em lote devolve: o id do lote (o toast leva-o para o Undo
/// desfazer **esse** lote) e quantas contas saíram.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveBatchResult {
    pub batch_id: u64,
    pub count: usize,
}

/// Remove um lote numa gravação só e guarda-o para o "Undo" do toast. O
/// `remove_account` (uma conta) continua para scripts e compatibilidade, e não
/// mexe no lote.
#[tauri::command]
pub fn remove_accounts(
    state: tauri::State<'_, AccountStore>,
    user_ids: Vec<i64>,
) -> Result<RemoveBatchResult, String> {
    let (batch_id, count) = state.remove_many(&user_ids)?;
    Ok(RemoveBatchResult { batch_id, count })
}

/// Desfaz o lote `batch_id` de `remove_accounts` (só em memória: fechar o app
/// esquece). Lote já substituído por uma remoção mais nova (toast antigo) é
/// recusado com erro, em vez de desfazer o lote errado.
#[tauri::command]
pub fn undo_remove_accounts(
    state: tauri::State<'_, AccountStore>,
    batch_id: u64,
) -> Result<usize, String> {
    state.undo_remove(batch_id)
}

/// `General.GroupOrder` depois de renomear `from` para `to`, ou `None` quando
/// não há o que mudar (sem ordem manual, valor estragado, grupo fora da lista).
///
/// O nome novo fica **na posição do antigo**. Se `to` já estava na ordem (é uma
/// junção de grupos), o antigo só sai: o grupo de destino fica onde estava.
/// O valor é JSON, como o frontend grava (`serializeGroupOrder`).
pub fn rename_group_in_order(raw: &str, from: &str, to: &str) -> Option<String> {
    let order: Vec<String> = serde_json::from_str(raw).ok()?;
    let position = order.iter().position(|g| g == from)?;
    let mut next = order;
    if next.iter().any(|g| g == to) {
        next.remove(position);
    } else {
        next[position] = to.to_string();
    }
    serde_json::to_string(&next).ok()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameGroupResult {
    pub renamed: usize,
    /// O `General.GroupOrder` novo, quando mudou e foi gravado; o frontend o
    /// põe nas settings em memória sem reler o INI.
    pub group_order: Option<String>,
}

/// Renomeia um grupo (ou junta-o a outro já existente): as contas, numa gravação
/// só, e a entrada dele em `General.GroupOrder`. O grupo `Default` não se
/// renomeia. O estado "colapsado" dos grupos vive só na tela e é migrado lá.
#[tauri::command]
pub fn rename_group(
    state: tauri::State<'_, AccountStore>,
    settings: tauri::State<'_, crate::data::settings::SettingsStore>,
    from: String,
    to: String,
) -> Result<RenameGroupResult, String> {
    let to = to.trim().to_string();
    let renamed = state.rename_group(&from, &to)?;
    let mut group_order = None;
    if renamed > 0 {
        let raw = settings.get_string("General", "GroupOrder");
        if let Some(next) = rename_group_in_order(&raw, &from, &to) {
            // Falhar aqui não desfaz o rename: o grupo só perde o lugar da ordem
            // manual e vai para o fim, como um grupo novo.
            match settings.set("General", "GroupOrder", &next) {
                Ok(()) => group_order = Some(next),
                Err(e) => eprintln!("Não foi possível gravar a ordem dos grupos: {}", e),
            }
        }
    }
    Ok(RenameGroupResult {
        renamed,
        group_order,
    })
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
pub fn vault_key_warning(state: tauri::State<'_, AccountStore>) -> Option<VaultKeyWarning> {
    state.vault_key_warning()
}

/// Evento com cada mudança do aviso do `.key`. Payload: o aviso, ou `null`
/// quando ele sumiu. O frontend ouve pelo mesmo nome (`src/types.ts`).
pub const VAULT_KEY_WARNING_EVENT: &str = "vault-key-warning-changed";

/// Leva à janela, **na hora**, cada mudança do aviso do `.key`.
///
/// O comando `vault_key_warning` só responde quando a UI pergunta, e ela só
/// pergunta no boot, ao recarregar contas e quando trocar a criptografia falha.
/// Gravador de fundo não passa por nada disso: com o Auto Rejoin a noite inteira,
/// o `.key` que ficava ruim de madrugada virava aviso só no backend, e o dono
/// descobria no boot seguinte — o lockout que a faixa existe para evitar.
///
/// A thread é dela, e fala com o Tauri **fora** de qualquer lock do store: o
/// aviso muda dentro de `save_locked`, que roda segurando o lock de contas, e um
/// comando síncrono na thread principal (restaurar backup) pode estar esperando
/// justamente esse lock. O store só enfileira no canal, o que nunca bloqueia.
pub fn forward_vault_key_warning(app: &tauri::AppHandle, store: &AccountStore) {
    use tauri::Emitter;

    let changes = store.watch_key_warning();
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("vault-key-warning".to_string())
        .spawn(move || {
            for warning in changes {
                let _ = app.emit(VAULT_KEY_WARNING_EVENT, warning);
            }
        });
    if let Err(e) = spawned {
        // Sem a thread a faixa volta a ser lida só quando a UI pergunta; o app
        // segue funcionando.
        eprintln!("Não foi possível acompanhar o aviso da chave do vault: {}", e);
    }
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

/// Tentativas erradas seguidas na tela "trancado por inatividade" (ideia 27)
/// e até quando a próxima fica recusada.
struct AppLockAttempts {
    failures: u32,
    blocked_until: Option<std::time::Instant>,
}

static APP_LOCK_ATTEMPTS: Mutex<AppLockAttempts> = Mutex::new(AppLockAttempts {
    failures: 0,
    blocked_until: None,
});

/// Espera depois de `failures` senhas erradas seguidas: as três primeiras são
/// livres (dedo errado), depois 5 s dobrando até 60 s.
fn app_lock_retry_delay_secs(failures: u32) -> u64 {
    if failures < 3 {
        return 0;
    }
    let doublings = (failures - 3).min(4);
    (5u64 << doublings).min(60)
}

/// Destranca a tela de inatividade. **Só confere a senha** — não relê as
/// contas, não troca a sessão, não para nada que esteja rodando.
#[tauri::command]
pub fn verify_app_password(state: tauri::State<'_, AccountStore>, password: String) -> Result<(), String> {
    check_app_password(&APP_LOCK_ATTEMPTS, std::time::Instant::now(), |p| state.verify_password(p), &password)
}

fn check_app_password(
    attempts: &Mutex<AppLockAttempts>,
    now: std::time::Instant,
    verify: impl Fn(&str) -> Result<bool, String>,
    password: &str,
) -> Result<(), String> {
    let mut slot = attempts.lock().map_err(|e| e.to_string())?;
    if let Some(until) = slot.blocked_until {
        if now < until {
            let wait = until.duration_since(now).as_secs().max(1);
            return Err(format!("Too many wrong passwords. Wait {wait} seconds and try again."));
        }
    }
    if verify(password)? {
        slot.failures = 0;
        slot.blocked_until = None;
        return Ok(());
    }
    slot.failures = slot.failures.saturating_add(1);
    let delay = app_lock_retry_delay_secs(slot.failures);
    slot.blocked_until = (delay > 0).then(|| now + std::time::Duration::from_secs(delay));
    Err("Wrong password.".to_string())
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

/// Nome do arquivo do export protegido. Começa com `AccountData` e termina em
/// `.json` de propósito: é o que o "Import AccountData.json" aceita
/// (`isAccountDataFileName` no [ImportDialog.tsx]).
fn account_export_file_name(now: chrono::DateTime<chrono::Local>) -> String {
    format!("AccountData-export-{}.json", now.format("%Y%m%d-%H%M%S"))
}

/// "Export accounts with a password" (ideia 38): grava as contas cifradas com a
/// senha escolhida agora em `<pasta de dados>/exports/` e abre o Explorer com o
/// arquivo selecionado — o mesmo caminho do export do histórico; o app não tem
/// diálogo de salvar. Devolve o caminho.
///
/// `async` para o Argon2 (128 MiB) não rodar na thread da janela.
#[tauri::command(async)]
pub fn export_accounts_with_password(
    state: tauri::State<'_, AccountStore>,
    password: String,
) -> Result<String, String> {
    let data = state.export_with_password(&password)?;
    let dir = crate::data::settings::get_runtime_data_dir().join("exports");
    fs::create_dir_all(&dir).map_err(|e| format!("Could not create the exports folder: {e}"))?;
    let path = dir.join(account_export_file_name(chrono::Local::now()));
    fs::write(&path, &data).map_err(|e| format!("Could not save the file: {e}"))?;
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn();
    }
    Ok(path.display().to_string())
}
