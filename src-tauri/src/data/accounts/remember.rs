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

#[cfg(test)]
mod remember_unlock_tests {
    use super::*;

    const HOUR_MS: i64 = 60 * 60 * 1000;

    #[test]
    fn the_expiry_is_the_requested_number_of_hours_ahead() {
        assert_eq!(expiry_for(24, 0), 24 * HOUR_MS);
        assert_eq!(expiry_for(1, 1_000), 1_000 + HOUR_MS);
    }

    /// Um prazo absurdo não pode virar "para sempre".
    #[test]
    fn the_expiry_is_capped() {
        assert_eq!(expiry_for(u64::MAX, 0), REMEMBER_MAX_HOURS as i64 * HOUR_MS);
        assert_eq!(
            expiry_for(REMEMBER_MAX_HOURS + 100, 0),
            REMEMBER_MAX_HOURS as i64 * HOUR_MS
        );
    }

    /// Pedir zero hora é erro de chamada, não instrução para não lembrar.
    #[test]
    fn zero_hours_falls_back_to_the_default() {
        assert_eq!(expiry_for(0, 0), REMEMBER_DEFAULT_HOURS as i64 * HOUR_MS);
    }

    #[test]
    fn a_remembered_unlock_stops_being_valid_at_the_expiry() {
        assert!(is_valid_at(1_000, 999));
        assert!(!is_valid_at(1_000, 1_000), "no instante exato já venceu");
        assert!(!is_valid_at(1_000, 5_000));
        // Relógio atrasado não ressuscita um lembrete de prazo zero.
        assert!(!is_valid_at(0, 0));
    }

    /// O prazo viaja **dentro** do blob cifrado: é isso que impede estender a
    /// validade mexendo no arquivo.
    #[test]
    fn the_payload_round_trips_through_json_with_its_expiry() {
        let payload = RememberedUnlock {
            expires_at_ms: 1_234_567,
            password: "hunter2".to_string(),
        };
        let json = serde_json::to_vec(&payload).expect("json");
        let back: RememberedUnlock = serde_json::from_slice(&json).expect("parse");
        assert_eq!(back, payload);
        assert!(
            String::from_utf8_lossy(&json).contains("expires_at_ms"),
            "o prazo tem que estar no conteúdo cifrado"
        );
    }

    #[test]
    fn the_file_lives_next_to_the_other_data_files() {
        let path = remembered_unlock_path();
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("RAMUnlock.bin")
        );
        assert_eq!(
            path.parent(),
            Some(crate::data::settings::get_runtime_data_dir().as_path())
        );
    }

    /// Senha vazia nunca vira lembrete: destrancaria nada e só deixaria
    /// arquivo para trás.
    #[test]
    fn an_empty_password_is_refused() {
        assert!(remember("", 24).is_err());
    }

    /// O blob protegido não pode conter a senha legível. No Windows o DPAPI
    /// cifra; nos outros sistemas o recurso nem existe.
    #[cfg(target_os = "windows")]
    #[test]
    fn the_protected_blob_does_not_contain_the_password() {
        let json = serde_json::to_vec(&RememberedUnlock {
            expires_at_ms: 1,
            password: "senha-secreta-123".to_string(),
        })
        .expect("json");
        let protected = protect(&json).expect("dpapi");
        assert!(
            !String::from_utf8_lossy(&protected).contains("senha-secreta-123"),
            "a senha vazou em texto puro no blob"
        );
        assert_eq!(unprotect(&protected).as_deref(), Some(json.as_slice()));
    }

    /// Lixo (ou um blob de outra máquina) não abre e não pode derrubar o app.
    #[test]
    fn garbage_does_not_unprotect() {
        assert!(unprotect(&[0, 1, 2, 3, 4]).is_none());
        assert!(unprotect(&[]).is_none());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn without_dpapi_the_feature_is_refused_instead_of_storing_plaintext() {
        assert!(!can_remember());
        assert!(remember("hunter2", 24).is_err());
    }
}
