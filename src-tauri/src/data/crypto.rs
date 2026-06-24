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
