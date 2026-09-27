use std::sync::OnceLock;

use sodiumoxide::crypto::hash::sha512;
use sodiumoxide::crypto::pwhash::argon2i13;
use sodiumoxide::crypto::secretbox;

pub const RAM_HEADER: &[u8] = b"Roblox Account Manager created by ic3w0lf22 @ github.com .......";
const TRANSITION_RAM_HEADER: &[u8] =
    b"Roblox Account Manager created by ic3w0lf2 and continued by niccdevs @ github.com .......";

#[derive(Debug)]
pub enum CryptoError {
    MissingHeader,
    InvalidData,
    DecryptionFailed,
    InvalidPassword,
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CryptoError::MissingHeader => write!(f, "Missing RAM header"),
            CryptoError::InvalidData => write!(f, "Invalid encrypted data"),
            CryptoError::DecryptionFailed => write!(f, "Decryption failed"),
            CryptoError::InvalidPassword => write!(f, "Invalid password"),
        }
    }
}

impl std::error::Error for CryptoError {}

pub fn hash_password(password: &str) -> Vec<u8> {
    let digest = sha512::hash(password.as_bytes());
    digest.as_ref().to_vec()
}

pub fn derive_key(password_hash: &[u8], salt: &[u8]) -> Result<secretbox::Key, CryptoError> {
    let salt = argon2i13::Salt::from_slice(salt).ok_or(CryptoError::InvalidData)?;

    let mut key_bytes = [0u8; secretbox::KEYBYTES];
    argon2i13::derive_key(
        &mut key_bytes,
        password_hash,
        &salt,
        argon2i13::OPSLIMIT_MODERATE,
        argon2i13::MEMLIMIT_MODERATE,
    )
    .map_err(|_| CryptoError::InvalidPassword)?;

    secretbox::Key::from_slice(&key_bytes).ok_or(CryptoError::InvalidData)
}

// ---------------------------------------------------------------------------
// DPAPI (escopo do usuário atual do Windows)
// ---------------------------------------------------------------------------

/// Embrulha bytes com o DPAPI **do usuário atual do Windows**, com a entropia
/// que o chamador escolher.
///
/// Existe um ponto só para os dois usos do app ("lembrar de mim" e a chave do
/// vault de contas) porque duplicar o bloco `unsafe` era duplicar a chance de
/// errar um ponteiro. A entropia vem do chamador, então o blob de um recurso
/// nunca abre no outro.
///
/// **Limite real da proteção, e não é pequeno:** o DPAPI é do *usuário do
/// Windows*. Isso protege arquivo copiado para outra máquina, backup vazado e
/// outro usuário no mesmo PC. **Não** protege contra programa rodando como o
/// próprio usuário — esse programa chama `CryptUnprotectData` exatamente como o
/// app chama. Não prometa mais do que isso na UI nem na doc.
///
/// `CRYPTPROTECT_UI_FORBIDDEN`: o app chama isso no boot e em gravações; se
/// alguma configuração de política exigisse interface, é melhor falhar e cair
/// no outro embrulho do que travar o processo num diálogo.
#[cfg(target_os = "windows")]
pub fn dpapi_protect(data: &[u8], entropy: &[u8]) -> Option<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    unsafe {
        let in_blob = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let entropy_blob = CRYPT_INTEGER_BLOB {
            cbData: entropy.len() as u32,
            pbData: entropy.as_ptr() as *mut u8,
        };
        let mut out_blob = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };

        let ok = CryptProtectData(
            &in_blob,
            std::ptr::null(),
            &entropy_blob,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        );
        if ok == 0 || out_blob.pbData.is_null() {
            return None;
        }

        let protected =
            std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize).to_vec();
        LocalFree(out_blob.pbData as *mut core::ffi::c_void);
        Some(protected)
    }
}

/// Desembrulha o que `dpapi_protect` guardou. `None` para qualquer problema
/// (arquivo de outro usuário, de outra máquina, entropia errada, lixo).
#[cfg(target_os = "windows")]
pub fn dpapi_unprotect(data: &[u8], entropy: &[u8]) -> Option<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    unsafe {
        let in_blob = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let entropy_blob = CRYPT_INTEGER_BLOB {
            cbData: entropy.len() as u32,
            pbData: entropy.as_ptr() as *mut u8,
        };
        let mut out_blob = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };

        let ok = CryptUnprotectData(
            &in_blob,
            std::ptr::null_mut(),
            &entropy_blob,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        );
        if ok == 0 || out_blob.pbData.is_null() {
            return None;
        }

        let plain = std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize).to_vec();
        LocalFree(out_blob.pbData as *mut core::ffi::c_void);
        Some(plain)
    }
}

#[cfg(not(target_os = "windows"))]
pub fn dpapi_protect(_data: &[u8], _entropy: &[u8]) -> Option<Vec<u8>> {
    None
}

#[cfg(not(target_os = "windows"))]
pub fn dpapi_unprotect(_data: &[u8], _entropy: &[u8]) -> Option<Vec<u8>> {
    None
}

// ---------------------------------------------------------------------------
// Hash derivado do aparelho
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn encode_wide(s: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "windows")]
fn read_machine_guid() -> Option<String> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_LOCAL_MACHINE, KEY_READ,
        KEY_WOW64_64KEY, REG_SZ,
    };

    let sub_key = encode_wide("SOFTWARE\\Microsoft\\Cryptography");
    let value_name = encode_wide("MachineGuid");

    // O app é 64-bit, mas pedir a view de 64 bits explicitamente no retry evita
    // depender de como o processo foi carregado.
    for access in [KEY_READ, KEY_READ | KEY_WOW64_64KEY] {
        unsafe {
            let mut hkey = std::ptr::null_mut();
            if RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                sub_key.as_ptr(),
                0,
                access,
                &mut hkey,
            ) != 0
            {
                continue;
            }

            let mut buf = [0u16; 256];
            let mut buf_size = (buf.len() * 2) as u32;
            let mut value_type = 0u32;
            let result = RegQueryValueExW(
                hkey,
                value_name.as_ptr(),
                std::ptr::null_mut(),
                &mut value_type,
                buf.as_mut_ptr() as *mut u8,
                &mut buf_size,
            );
            RegCloseKey(hkey);

            if result != 0 || value_type != REG_SZ {
                continue;
            }

            let len = (buf_size as usize / 2).saturating_sub(1);
            let value = String::from_utf16_lossy(&buf[..len]).trim().to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

/// Identificadores do aparelho, **do mais estável para o menos**.
///
/// O primeiro da lista é o que embrulha a chave do vault; os outros só são
/// tentados na leitura, para um arquivo escrito quando o primeiro não existia
/// continuar abrindo (é para isso que os fallbacks servem).
///
/// **Por que o `MachineGuid` não é o primeiro neste fork:** o isolamento
/// pré-launch (`Isolation.SpoofMachineGuid`) **reescreve**
/// `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid`. Amarrar a chave do vault
/// a esse valor significaria o app trancar o usuário fora das próprias contas na
/// primeira vez que ele usasse um recurso central do programa. Foi exatamente
/// esse o defeito que o upstream teve que consertar duas vezes; aqui o
/// identificador que manda é o nome da máquina, que o app nunca escreve.
///
/// O último candidato é uma constante: sem ela, uma máquina sem nome de
/// computador legível não teria como guardar a chave, e ficar sem criptografia é
/// pior que ter uma amarração fraca. No Windows o DPAPI continua sendo a
/// proteção de verdade.
fn device_identifier_candidates() -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();

    #[cfg(target_os = "windows")]
    {
        if let Ok(name) = std::env::var("COMPUTERNAME") {
            if !name.trim().is_empty() {
                candidates.push(name.trim().to_string());
            }
        }
        if let Some(guid) = read_machine_guid() {
            candidates.push(guid);
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(name) = std::env::var("HOSTNAME") {
            if !name.trim().is_empty() {
                candidates.push(name.trim().to_string());
            }
        }
    }

    candidates.push("ram-device".to_string());
    candidates
}

fn device_user() -> String {
    #[cfg(target_os = "windows")]
    let user = std::env::var("USERNAME");
    #[cfg(not(target_os = "windows"))]
    let user = std::env::var("USER");
    user.ok()
        .map(|v| v.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "user".to_string())
}

/// Hash no formato que `derive_key` espera, a partir de um identificador de
/// aparelho. Público para os testes poderem montar um vault "de outro aparelho"
/// sem mexer no ambiente.
pub fn device_hash_for_identifier(identifier: &str) -> Vec<u8> {
    hash_password(&format!("{}|{}|ram-device-v1", identifier, device_user()))
}

fn compute_device_hashes() -> Vec<Vec<u8>> {
    let mut hashes: Vec<Vec<u8>> = Vec::new();
    for identifier in device_identifier_candidates() {
        let hash = device_hash_for_identifier(&identifier);
        if !hashes.contains(&hash) {
            hashes.push(hash);
        }
    }
    hashes
}

/// Ler o registro a cada tentativa de destrancar seria caro e inútil: os
/// identificadores não mudam no meio de uma execução.
static DEVICE_HASHES: OnceLock<Vec<Vec<u8>>> = OnceLock::new();

/// Todos os hashes do aparelho, em ordem. O `[0]` é o que se usa para gravar.
pub fn device_hash_candidates() -> &'static [Vec<u8>] {
    DEVICE_HASHES.get_or_init(compute_device_hashes)
}

/// O hash com que a chave mestra é embrulhada ao gravar.
pub fn primary_device_hash() -> Vec<u8> {
    device_hash_candidates()[0].clone()
}

pub fn is_encrypted(data: &[u8]) -> bool {
    data.starts_with(RAM_HEADER) || data.starts_with(TRANSITION_RAM_HEADER)
}

pub fn decrypt(encrypted: &[u8], password_hash: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let header = if encrypted.starts_with(RAM_HEADER) {
        RAM_HEADER
    } else if encrypted.starts_with(TRANSITION_RAM_HEADER) {
        TRANSITION_RAM_HEADER
    } else {
        return Err(CryptoError::MissingHeader);
    };

    if encrypted.len() < header.len() + 16 + 24 + 16 {
        return Err(CryptoError::InvalidData);
    }

    let offset = header.len();
    let salt = &encrypted[offset..offset + 16];
    let nonce_bytes = &encrypted[offset + 16..offset + 16 + 24];
    let ciphertext = &encrypted[offset + 16 + 24..];

    let key = derive_key(password_hash, salt)?;

    let nonce = secretbox::Nonce::from_slice(nonce_bytes).ok_or(CryptoError::InvalidData)?;

    secretbox::open(ciphertext, &nonce, &key).map_err(|_| CryptoError::DecryptionFailed)
}

pub fn encrypt(content: &str, password_hash: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if content.is_empty() {
        return Err(CryptoError::InvalidData);
    }

    let salt = argon2i13::gen_salt();
    let key = derive_key(password_hash, salt.as_ref())?;
    let nonce = secretbox::gen_nonce();
    let ciphertext = secretbox::seal(content.as_bytes(), &nonce, &key);

    let mut output = Vec::with_capacity(RAM_HEADER.len() + 16 + 24 + ciphertext.len());
    output.extend_from_slice(RAM_HEADER);
    output.extend_from_slice(salt.as_ref());
    output.extend_from_slice(nonce.as_ref());
    output.extend_from_slice(&ciphertext);

    Ok(output)
}

#[cfg(target_os = "windows")]
pub fn try_decrypt_legacy_dpapi(data: &[u8]) -> Option<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};

    const LEGACY_ENTROPY: [u8; 56] = [
        0x52, 0x4f, 0x42, 0x4c, 0x4f, 0x58, 0x20, 0x41, 0x43, 0x43, 0x4f, 0x55, 0x4e, 0x54, 0x20,
        0x4d, 0x41, 0x4e, 0x41, 0x47, 0x45, 0x52, 0x20, 0x7c, 0x20, 0x3a, 0x29, 0x20, 0x7c, 0x20,
        0x42, 0x52, 0x4f, 0x55, 0x47, 0x48, 0x54, 0x20, 0x54, 0x4f, 0x20, 0x59, 0x4f, 0x55, 0x20,
        0x42, 0x55, 0x59, 0x20, 0x69, 0x63, 0x33, 0x77, 0x30, 0x6c, 0x66,
    ];

    unsafe {
        let in_blob = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let entropy_blob = CRYPT_INTEGER_BLOB {
            cbData: LEGACY_ENTROPY.len() as u32,
            pbData: LEGACY_ENTROPY.as_ptr() as *mut u8,
        };
        let mut out_blob = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };

        let ok = CryptUnprotectData(
            &in_blob,
            std::ptr::null_mut(),
            &entropy_blob,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
            &mut out_blob,
        );
        if ok == 0 || out_blob.pbData.is_null() {
            return None;
        }

        let decrypted =
            std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize).to_vec();
        LocalFree(out_blob.pbData as *mut core::ffi::c_void);
        Some(decrypted)
    }
}

#[cfg(not(target_os = "windows"))]
pub fn try_decrypt_legacy_dpapi(_data: &[u8]) -> Option<Vec<u8>> {
    None
}

pub fn init() -> bool {
    sodiumoxide::init().is_ok()
}

#[cfg(test)]
mod crypto_tests {
    use super::*;

    fn init_sodium() {
        // sodiumoxide::init is idempotent and safe to call from every test.
        assert!(init(), "sodiumoxide must initialize");
    }

    // ---- hash_password -------------------------------------------------------

    #[test]
    fn hash_password_is_deterministic_and_is_a_sha512_digest() {
        init_sodium();
        let a = hash_password("hunter2");
        let b = hash_password("hunter2");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64, "sha512 digests are 64 bytes");
    }

    #[test]
    fn hash_password_separates_different_passwords_and_does_not_trim() {
        init_sodium();
        assert_ne!(hash_password("hunter2"), hash_password("hunter3"));
        // The store trims before hashing; the primitive itself must not.
        assert_ne!(hash_password(" hunter2 "), hash_password("hunter2"));
        // The empty password still hashes (the length rule lives in the store).
        assert_eq!(hash_password("").len(), 64);
    }

    #[test]
    fn hash_password_handles_non_ascii_passwords() {
        init_sodium();
        let a = hash_password("senha-cao-\u{1F512}");
        assert_eq!(a.len(), 64);
        assert_eq!(a, hash_password("senha-cao-\u{1F512}"));
        assert_ne!(a, hash_password("senha-cao-"));
    }

    // ---- is_encrypted --------------------------------------------------------

    #[test]
    fn is_encrypted_detects_both_headers_and_rejects_everything_else() {
        assert!(is_encrypted(RAM_HEADER));
        assert!(is_encrypted(TRANSITION_RAM_HEADER));

        let mut with_payload = RAM_HEADER.to_vec();
        with_payload.extend_from_slice(&[0u8; 64]);
        assert!(is_encrypted(&with_payload));

        let mut transition_with_payload = TRANSITION_RAM_HEADER.to_vec();
        transition_with_payload.extend_from_slice(&[0u8; 64]);
        assert!(is_encrypted(&transition_with_payload));

        // Plain JSON, empty and short buffers are never "encrypted".
        assert!(!is_encrypted(b""));
        assert!(!is_encrypted(b"["));
        assert!(!is_encrypted(b"[]"));
        assert!(!is_encrypted(br#"[{"UserID":1}]"#));
        assert!(!is_encrypted(&[0u8; 8]));

        // A truncated header must not be mistaken for a full one.
        assert!(!is_encrypted(&RAM_HEADER[..RAM_HEADER.len() - 1]));
        assert!(!is_encrypted(
            &TRANSITION_RAM_HEADER[..TRANSITION_RAM_HEADER.len() - 1]
        ));

        // Leading whitespace/garbage before the header defeats detection.
        let mut prefixed = vec![b' '];
        prefixed.extend_from_slice(RAM_HEADER);
        assert!(!is_encrypted(&prefixed));
    }

    #[test]
    fn the_transition_header_is_not_a_prefix_of_the_current_one() {
        // decrypt() picks the header by prefix match; if one were a prefix of
        // the other the offsets would silently be wrong.
        assert!(!TRANSITION_RAM_HEADER.starts_with(RAM_HEADER));
        assert!(!RAM_HEADER.starts_with(TRANSITION_RAM_HEADER));
        assert_eq!(RAM_HEADER.len(), 64);
        assert_eq!(TRANSITION_RAM_HEADER.len(), 89);
    }

    // ---- derive_key ----------------------------------------------------------

    #[test]
    fn derive_key_rejects_salts_that_are_not_16_bytes() {
        init_sodium();
        let hash = hash_password("some-password");
        for bad_len in [0usize, 1, 15, 17, 32] {
            let salt = vec![7u8; bad_len];
            let err = derive_key(&hash, &salt).expect_err("short/long salt must fail");
            assert!(
                matches!(err, CryptoError::InvalidData),
                "unexpected error for salt len {}: {:?}",
                bad_len,
                err
            );
        }
    }

    #[test]
    fn derive_key_is_deterministic_per_salt_and_changes_with_the_salt() {
        init_sodium();
        let hash = hash_password("derive-key-password");
        let salt_a = [1u8; 16];
        let salt_b = [2u8; 16];

        let key_a1 = derive_key(&hash, &salt_a).expect("derive a1");
        let key_a2 = derive_key(&hash, &salt_a).expect("derive a2");
        let key_b = derive_key(&hash, &salt_b).expect("derive b");

        assert_eq!(key_a1.as_ref(), key_a2.as_ref(), "same salt => same key");
        assert_ne!(key_a1.as_ref(), key_b.as_ref(), "salt must change the key");
        assert_eq!(key_a1.as_ref().len(), secretbox::KEYBYTES);
    }

    // ---- encrypt / decrypt ---------------------------------------------------

    #[test]
    fn encrypt_refuses_empty_content() {
        init_sodium();
        let err = encrypt("", &hash_password("pw")).expect_err("empty content must fail");
        assert!(matches!(err, CryptoError::InvalidData));
    }

    #[test]
    fn encrypt_output_has_the_header_salt_and_nonce_layout_and_is_randomized() {
        init_sodium();
        let hash = hash_password("layout-password");
        let payload = r#"[{"UserID":1}]"#;

        let a = encrypt(payload, &hash).expect("encrypt a");
        let b = encrypt(payload, &hash).expect("encrypt b");

        assert!(a.starts_with(RAM_HEADER));
        assert!(is_encrypted(&a));
        // header + salt(16) + nonce(24) + MAC(16) + payload
        assert_eq!(a.len(), RAM_HEADER.len() + 16 + 24 + 16 + payload.len());
        // Fresh salt and nonce per call, so the same plaintext never repeats.
        assert_ne!(a, b, "encryption must not be deterministic");
        assert_ne!(
            a[RAM_HEADER.len()..RAM_HEADER.len() + 16],
            b[RAM_HEADER.len()..RAM_HEADER.len() + 16],
            "salt must be random per call"
        );
        // The plaintext must not be recoverable from the blob.
        assert!(
            !a.windows(payload.len()).any(|w| w == payload.as_bytes()),
            "plaintext leaked into the ciphertext"
        );
    }

    #[test]
    fn decrypt_rejects_data_without_a_known_header() {
        init_sodium();
        let hash = hash_password("pw");
        let samples: [&[u8]; 4] = [b"", b"[]", br#"[{"UserID":1}]"#, &[0u8; 200]];
        for data in samples {
            let err = decrypt(data, &hash).expect_err("must not decrypt headerless data");
            assert!(matches!(err, CryptoError::MissingHeader), "{:?}", err);
        }
    }

    #[test]
    fn decrypt_rejects_blobs_that_are_too_short_for_salt_nonce_and_mac() {
        init_sodium();
        let hash = hash_password("pw");
        let min_len = RAM_HEADER.len() + 16 + 24 + 16;

        for extra in [0usize, 1, 16, 39, 55] {
            let mut data = RAM_HEADER.to_vec();
            data.extend(std::iter::repeat(0u8).take(extra));
            assert!(data.len() < min_len);
            let err = decrypt(&data, &hash).expect_err("truncated blob must fail");
            assert!(matches!(err, CryptoError::InvalidData), "{:?}", err);
        }

        // The same rule applies to the transition header, with its own offset.
        let mut transition = TRANSITION_RAM_HEADER.to_vec();
        transition.extend(std::iter::repeat(0u8).take(55));
        let err = decrypt(&transition, &hash).expect_err("truncated transition blob must fail");
        assert!(matches!(err, CryptoError::InvalidData), "{:?}", err);
    }

    #[test]
    fn encrypt_decrypt_round_trips_and_rejects_wrong_passwords_and_tampering() {
        init_sodium();
        let payload = r#"[{"UserID":42,"Username":"Round Trip","SecurityToken":"_|WARNING"}]"#;
        let hash = hash_password("correct horse battery staple");
        let encrypted = encrypt(payload, &hash).expect("encrypt");

        // 1. Correct password round trips byte for byte.
        let decrypted = decrypt(&encrypted, &hash).expect("decrypt");
        assert_eq!(String::from_utf8(decrypted).unwrap(), payload);

        // 2. A wrong password fails and never returns plaintext.
        let wrong = hash_password("correct horse battery stapl");
        let err = decrypt(&encrypted, &wrong).expect_err("wrong password must fail");
        assert!(matches!(err, CryptoError::DecryptionFailed), "{:?}", err);

        // 3. A flipped ciphertext byte is rejected by the MAC.
        let mut tampered = encrypted.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0xFF;
        let err = decrypt(&tampered, &hash).expect_err("tampered ciphertext must fail");
        assert!(matches!(err, CryptoError::DecryptionFailed), "{:?}", err);

        // 4. A flipped nonce byte is rejected too.
        let mut bad_nonce = encrypted.clone();
        bad_nonce[RAM_HEADER.len() + 16] ^= 0x01;
        let err = decrypt(&bad_nonce, &hash).expect_err("tampered nonce must fail");
        assert!(matches!(err, CryptoError::DecryptionFailed), "{:?}", err);

        // 5. The legacy "transition" header is still accepted for reading.
        let mut transition = TRANSITION_RAM_HEADER.to_vec();
        transition.extend_from_slice(&encrypted[RAM_HEADER.len()..]);
        assert!(is_encrypted(&transition));
        let decrypted = decrypt(&transition, &hash).expect("transition header decrypt");
        assert_eq!(String::from_utf8(decrypted).unwrap(), payload);
    }

    // ---- legacy DPAPI --------------------------------------------------------

    #[test]
    fn try_decrypt_legacy_dpapi_returns_none_for_garbage_instead_of_panicking() {
        init_sodium();
        assert!(try_decrypt_legacy_dpapi(&[]).is_none());
        assert!(try_decrypt_legacy_dpapi(b"[]").is_none());
        assert!(try_decrypt_legacy_dpapi(b"not a DPAPI blob at all").is_none());
        assert!(try_decrypt_legacy_dpapi(&[0xFFu8; 1024]).is_none());
        assert!(try_decrypt_legacy_dpapi(RAM_HEADER).is_none());

        let mut blob = RAM_HEADER.to_vec();
        blob.extend_from_slice(&[0u8; 128]);
        assert!(try_decrypt_legacy_dpapi(&blob).is_none());
    }

    // ---- error type ----------------------------------------------------------

    #[test]
    fn crypto_error_renders_a_distinct_message_for_each_variant() {
        let messages: Vec<String> = [
            CryptoError::MissingHeader,
            CryptoError::InvalidData,
            CryptoError::DecryptionFailed,
            CryptoError::InvalidPassword,
        ]
        .iter()
        .map(|e| e.to_string())
        .collect();

        assert_eq!(messages[0], "Missing RAM header");
        assert_eq!(messages[1], "Invalid encrypted data");
        assert_eq!(messages[2], "Decryption failed");
        assert_eq!(messages[3], "Invalid password");

        let mut unique = messages.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), messages.len(), "messages must be distinct");

        // It must be usable as a std::error::Error (the `?` paths rely on it).
        let boxed: Box<dyn std::error::Error> = Box::new(CryptoError::InvalidData);
        assert_eq!(boxed.to_string(), "Invalid encrypted data");
        assert!(format!("{:?}", CryptoError::MissingHeader).contains("MissingHeader"));
    }

    #[test]
    fn init_is_idempotent() {
        assert!(init());
        assert!(init());
    }
}
