// "Lembrar de mim": destrancar as contas sem redigitar a senha por um tempo.
//
// A senha destranca os cookies das contas, então guardá-la em disco é uma
// decisão de segurança, não de conveniência. As regras aqui existem para o
// arquivo valer o mínimo possível para quem não for o dono da máquina:
//
// - o blob é cifrado pelo **DPAPI do Windows no escopo do usuário atual**, com
//   entropia própria do app: copiar o arquivo para outra máquina, ou abri-lo
//   com outra conta do Windows, não devolve nada;
// - o **prazo mora dentro do blob cifrado**, então editar o arquivo (ou mexer
//   no relógio do arquivo) não estende a validade;
// - o lembrete é apagado quando expira, quando a senha muda ou sai, e quando
//   ele próprio não abre mais;
// - é **opt-in**: só existe se o usuário marcar a caixa.
//
// Fora do Windows não há DPAPI e o recurso simplesmente não é oferecido —
// guardar a senha em texto puro seria pior que digitá-la.

/// Quanto tempo o usuário pode pedir, no máximo.
pub const REMEMBER_MAX_HOURS: u64 = 24 * 7;

/// O que a caixa "lembrar de mim" oferece hoje.
pub const REMEMBER_DEFAULT_HOURS: u64 = 24;

/// Entropia do DPAPI. Amarra o blob a este app: outro programa rodando como o
/// mesmo usuário do Windows não abre o arquivo sem conhecê-la.
#[cfg(target_os = "windows")]
const REMEMBER_ENTROPY: &[u8] = b"RAM4 remembered unlock v1";

/// Conteúdo cifrado do lembrete.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct RememberedUnlock {
    /// Instante (ms desde a época) em que o lembrete deixa de valer.
    expires_at_ms: i64,
    password: String,
}

pub fn remembered_unlock_path() -> PathBuf {
    crate::data::settings::get_runtime_data_dir().join("RAMUnlock.bin")
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

/// Prazo pedido → instante de expiração, com o teto aplicado.
///
/// Horas zero (ou negativas, que nem chegam aqui) viram o padrão: pedir para
/// lembrar por nada é um erro de chamada, não uma instrução.
pub fn expiry_for(hours: u64, now_ms: i64) -> i64 {
    let hours = if hours == 0 {
        REMEMBER_DEFAULT_HOURS
    } else {
        hours.min(REMEMBER_MAX_HOURS)
    };
    now_ms + (hours as i64) * 60 * 60 * 1000
}

/// O lembrete ainda vale neste instante?
pub fn is_valid_at(remembered_expiry_ms: i64, now_ms: i64) -> bool {
    remembered_expiry_ms > now_ms
}

// ---------------------------------------------------------------------------
// DPAPI
// ---------------------------------------------------------------------------

/// O sistema consegue guardar a senha com proteção do SO?
#[cfg(target_os = "windows")]
pub fn can_remember() -> bool {
    true
}

#[cfg(not(target_os = "windows"))]
pub fn can_remember() -> bool {
    false
}

/// O `unsafe` do DPAPI mora em [`crypto::dpapi_protect`]; aqui só entra a
/// entropia deste recurso. Dois blocos `unsafe` fazendo a mesma coisa eram duas
/// chances de errar um ponteiro.
#[cfg(target_os = "windows")]
fn protect(data: &[u8]) -> Option<Vec<u8>> {
    crypto::dpapi_protect(data, REMEMBER_ENTROPY)
}

#[cfg(target_os = "windows")]
fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    crypto::dpapi_unprotect(data, REMEMBER_ENTROPY)
}

#[cfg(not(target_os = "windows"))]
fn protect(_data: &[u8]) -> Option<Vec<u8>> {
    None
}

#[cfg(not(target_os = "windows"))]
fn unprotect(_data: &[u8]) -> Option<Vec<u8>> {
    None
}

// ---------------------------------------------------------------------------
// Arquivo
// ---------------------------------------------------------------------------

/// Apaga o lembrete. Chamado quando ele expira, quando a senha muda ou sai, e
/// quando o usuário desmarca a caixa.
pub fn forget() {
    let _ = fs::remove_file(remembered_unlock_path());
}

/// Guarda a senha por `hours` horas. Erro quando o SO não oferece proteção —
/// nunca grava senha em texto puro.
pub fn remember(password: &str, hours: u64) -> Result<(), String> {
    if !can_remember() {
        return Err("Este sistema não guarda a senha com segurança".to_string());
    }
    if password.is_empty() {
        return Err("Senha vazia".to_string());
    }

    let payload = RememberedUnlock {
        expires_at_ms: expiry_for(hours, now_ms()),
        password: password.to_string(),
    };
    let json = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
    let protected = protect(&json).ok_or("Não foi possível proteger a senha neste sistema")?;

    let path = remembered_unlock_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&path, protected).map_err(|e| format!("Falha ao guardar o lembrete: {}", e))
}

/// A senha lembrada, se ainda valer. Qualquer problema — arquivo ausente,
/// blob de outro usuário, prazo vencido — devolve `None` e limpa o arquivo.
pub fn remembered_password() -> Option<String> {
    let data = fs::read(remembered_unlock_path()).ok()?;
    let Some(plain) = unprotect(&data) else {
        // Blob que não abre não vai abrir nunca: some com ele.
        forget();
        return None;
    };
    let Ok(payload) = serde_json::from_slice::<RememberedUnlock>(&plain) else {
        forget();
        return None;
    };
    if !is_valid_at(payload.expires_at_ms, now_ms()) {
        forget();
        return None;
    }
    Some(payload.password)
}
