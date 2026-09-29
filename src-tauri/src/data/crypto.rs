use std::sync::OnceLock;

use argon2::{Algorithm, Argon2, Params, Version};
use crypto_secretbox::aead::{Aead, KeyInit};
use crypto_secretbox::XSalsa20Poly1305;
use sha2::{Digest, Sha512};

/// Chave de 32 bytes do secretbox (XSalsa20-Poly1305).
pub type Key = [u8; 32];
/// Salt de 16 bytes do Argon2i (mesmo tamanho do libsodium).
pub type Salt = [u8; 16];
/// Nonce de 24 bytes do secretbox.
pub type Nonce = [u8; 24];

/// Preenche `buf` com bytes aleatorios do SO. Falta de CSPRNG e irrecuperavel
/// para criptografia, entao aborta em vez de seguir com bytes fracos.
fn random_fill(buf: &mut [u8]) {
    getrandom::fill(buf).expect("OS RNG unavailable");
}

/// Salt novo de 16 bytes.
pub fn gen_salt() -> Salt {
    let mut s = [0u8; 16];
    random_fill(&mut s);
    s
}

/// Nonce novo de 24 bytes.
pub fn gen_nonce() -> Nonce {
    let mut n = [0u8; 24];
    random_fill(&mut n);
    n
}

/// Bytes aleatorios do SO (a chave mestra do aparelho usa isto).
pub fn random_bytes(len: usize) -> Vec<u8> {
    let mut v = vec![0u8; len];
    random_fill(&mut v);
    v
}

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
    Sha512::digest(password.as_bytes()).to_vec()
}

/// Deriva a chave de 32 bytes com Argon2i, **exatamente** como o libsodium
/// (`crypto_pwhash` argon2i13, OPSLIMIT/MEMLIMIT_MODERATE): Argon2i, versao
/// 0x13, t_cost=6, m_cost=131072 KiB (128 MiB), paralelismo 1, saida de 32
/// bytes. Os numeros sairam das constantes do proprio libsodium-sys; mudar
/// qualquer um torna os arquivos ja gravados ilegiveis. Travado pelo
/// `decrypts_a_libsodium_fixture`.
pub fn derive_key(password_hash: &[u8], salt: &[u8]) -> Result<Key, CryptoError> {
    if salt.len() != 16 {
        return Err(CryptoError::InvalidData);
    }
    let params = Params::new(131072, 6, 1, Some(32)).map_err(|_| CryptoError::InvalidData)?;
    let argon = Argon2::new(Algorithm::Argon2i, Version::V0x13, params);
    let mut key: Key = [0u8; 32];
    argon
        .hash_password_into(password_hash, salt, &mut key)
        .map_err(|_| CryptoError::InvalidPassword)?;
    Ok(key)
}

/// Sela como o `crypto_secretbox` do libsodium: XSalsa20-Poly1305, com o MAC de
/// 16 bytes **na frente** do texto cifrado (o `crypto_secretbox` do RustCrypto
/// ja produz esse layout). Nao falha: a chave tem o tamanho certo por tipo.
pub fn seal(plaintext: &[u8], nonce: &Nonce, key: &Key) -> Vec<u8> {
    let cipher = XSalsa20Poly1305::new_from_slice(key).expect("32-byte key");
    let n = crypto_secretbox::Nonce::from_slice(nonce);
    cipher
        .encrypt(n, plaintext)
        .expect("secretbox encryption never fails")
}

/// Abre o que `seal` (ou o libsodium) produziu.
pub fn open(ciphertext: &[u8], nonce: &Nonce, key: &Key) -> Result<Vec<u8>, CryptoError> {
    let cipher = XSalsa20Poly1305::new_from_slice(key).map_err(|_| CryptoError::InvalidData)?;
    let n = crypto_secretbox::Nonce::from_slice(nonce);
    cipher
        .decrypt(n, ciphertext)
        .map_err(|_| CryptoError::DecryptionFailed)
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
/// **Limite real da proteção:** o DPAPI é do *usuário do Windows*. Isso protege
/// um arquivo copiado para outra máquina e outro usuário no mesmo PC. **Não**
/// protege contra programa rodando como o próprio usuário — esse programa chama
/// `CryptUnprotectData` exatamente como o app chama.
///
/// E **não** decide sozinho se um backup está protegido: isso depende de o blob
/// viajar ou não no backup. Para o vault de contas ele viaja (ver
/// [`crate::data::vault_key`]), então lá o backup **não** está protegido por
/// isto. Uma versão anterior deste comentário afirmava o contrário. Não prometa
/// na UI nem na doc mais do que o caso concreto entrega.
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

    let mut nonce: Nonce = [0u8; 24];
    nonce.copy_from_slice(nonce_bytes);
    open(ciphertext, &nonce, &key)
}

pub fn encrypt(content: &str, password_hash: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if content.is_empty() {
        return Err(CryptoError::InvalidData);
    }

    let salt = gen_salt();
    let key = derive_key(password_hash, &salt)?;
    let nonce = gen_nonce();
    let ciphertext = seal(content.as_bytes(), &nonce, &key);

    let mut output = Vec::with_capacity(RAM_HEADER.len() + 16 + 24 + ciphertext.len());
    output.extend_from_slice(RAM_HEADER);
    output.extend_from_slice(&salt);
    output.extend_from_slice(&nonce);
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

/// Mantido por compatibilidade com os chamadores (lib.rs). A criptografia em
/// Rust puro nao tem inicializacao global; sempre pronta.
pub fn init() -> bool {
    true
}
