//! Chave mestra do vault de contas — a criptografia que vale **sem** senha.
//!
//! Sem isto, `AccountData.json` sem senha é JSON puro com o `.ROBLOSECURITY` de
//! todas as contas: qualquer programa que leia o arquivo entra em todas elas, sem
//! senha e sem 2FA. Com isto, o arquivo é cifrado por uma chave aleatória de 32
//! bytes que fica num arquivo `.key` ao lado do vault.
//!
//! # O limite real desta proteção
//!
//! A chave fica **num arquivo ao lado do vault** e o embrulho principal dela é o
//! DPAPI **do usuário do Windows**. Então isto protege:
//!
//! - `AccountData.json` copiado para outra máquina (pen drive, upload, anexo);
//! - backup vazado (zip em nuvem sincronizada, cópia esquecida);
//! - outro usuário do Windows no mesmo PC.
//!
//! E **não** protege contra:
//!
//! - **malware rodando como o próprio usuário** — esse programa lê o `.key` e
//!   chama `CryptUnprotectData` exatamente como o app chama. Nada guardado no
//!   perfil do usuário resiste a isso;
//! - quem já tem o `.key` *e* o vault juntos e roda no mesmo perfil.
//!
//! Quem quer proteção contra alguém com acesso ao perfil precisa de **senha**
//! (Pass Lock): aí a chave vem da cabeça do usuário e não existe em disco. Não
//! vender na UI nem na doc proteção que não existe.
//!
//! # Os dois embrulhos
//!
//! A chave mestra é guardada duas vezes no mesmo arquivo `.key`:
//!
//! 1. **DPAPI** (`CryptProtectData`, escopo do usuário, com entropia própria) —
//!    é a proteção de verdade, e só existe no Windows;
//! 2. **hash derivado do aparelho** (`crypto::device_hash_candidates`) — é o
//!    caminho comum a todos os sistemas e o seguro contra "o DPAPI parou de
//!    abrir" (perfil recriado, política nova).
//!
//! **Qualquer um dos dois abre.** Ter só um seria o app trancar o usuário fora
//! das contas dele na primeira vez que aquele um falhasse.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::data::crypto;

pub const MASTER_KEY_LEN: usize = 32;

/// Entropia do DPAPI deste recurso. Amarra o embrulho a este app e o separa do
/// blob do "lembrar de mim".
#[cfg(target_os = "windows")]
const VAULT_KEY_ENTROPY: &[u8] = b"RAM4 vault master key v1";

/// Conteúdo do arquivo `.key`. Os dois embrulhos são opcionais **no formato**
/// (fora do Windows não há DPAPI), mas [`store_master_key`] recusa gravar um
/// arquivo sem nenhum dos dois: um `.key` que não abre é um vault perdido.
#[derive(Serialize, Deserialize)]
struct KeyFile {
    v: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dpapi: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    device: Option<String>,
}

/// Chave mestra recuperada, e por qual embrulho.
///
/// Quem chama **não** deve usar isto para decidir se regrava o `.key`: o
/// embrulho que não foi usado pode estar morto sem ninguém notar, e "abriu por
/// um deles" não diz nada sobre o outro. O reparo é incondicional em
/// `AccountStore::recover_and_refresh_master_key`. Este campo serve para teste e
/// diagnóstico.
pub struct RecoveredKey {
    pub master: Vec<u8>,
    /// `true` quando o embrulho do DPAPI foi o que abriu.
    pub via_dpapi: bool,
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{:02x}", byte);
    }
    out
}

fn hex_decode(input: &str) -> Option<Vec<u8>> {
    let input = input.trim();
    if input.is_empty() || input.len() % 2 != 0 {
        return None;
    }
    (0..input.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(input.get(i..i + 2)?, 16).ok())
        .collect()
}

/// O `.key` acompanha o **vault**, não um caminho fixo: no modo portátil os dois
/// viajam juntos no pen drive, e é `get_runtime_data_dir()` que decide onde o
/// vault mora.
pub fn key_file_path_for(vault_path: &Path) -> PathBuf {
    vault_path.with_extension("key")
}

/// O "hash de senha" que representa a chave mestra para o resto da criptografia.
///
/// O vault continua sendo cifrado pelo mesmo `crypto::encrypt` de sempre — o que
/// muda é só de onde vem o hash. O prefixo separa o domínio: uma senha de
/// usuário nunca colide com uma chave mestra.
pub fn master_password_hash(master: &[u8]) -> Vec<u8> {
    crypto::hash_password(&format!("ram-master-v1|{}", hex_encode(master)))
}

pub fn generate_master_key() -> Vec<u8> {
    sodiumoxide::randombytes::randombytes(MASTER_KEY_LEN)
}

/// Recupera a chave mestra do arquivo `.key`, tentando o DPAPI e depois cada
/// hash do aparelho.
///
/// `None` significa "não abriu" — e quem chama **não pode** tratar isso como
/// permissão para regravar o vault: perder o arquivo é pior que ficar sem
/// criptografia.
pub fn load_master_key(key_path: &Path) -> Option<RecoveredKey> {
    let data = fs::read(key_path).ok()?;
    let file: KeyFile = serde_json::from_slice(&data).ok()?;

    if let Some(blob) = file.dpapi.as_deref().and_then(hex_decode) {
        if let Some(master) = crypto::dpapi_unprotect(&blob, dpapi_entropy()) {
            if master.len() == MASTER_KEY_LEN {
                return Some(RecoveredKey {
                    master,
                    via_dpapi: true,
                });
            }
        }
    }

    let blob = file.device.as_deref().and_then(hex_decode)?;
    for hash in crypto::device_hash_candidates() {
        let Ok(decrypted) = crypto::decrypt(&blob, hash) else {
            continue;
        };
        let master = std::str::from_utf8(&decrypted).ok().and_then(hex_decode);
        if let Some(master) = master {
            if master.len() == MASTER_KEY_LEN {
                return Some(RecoveredKey {
                    master,
                    via_dpapi: false,
                });
            }
        }
    }

    None
}

/// Grava o `.key` com os dois embrulhos.
///
/// A gravação é write-then-rename pelo mesmo motivo do vault: um `.key`
/// truncado por queda de energia é um vault que não abre mais pelo caminho do
/// aparelho.
pub fn store_master_key(key_path: &Path, master: &[u8], device_hash: &[u8]) -> Result<(), String> {
    if master.len() != MASTER_KEY_LEN {
        return Err("Chave mestra com tamanho inválido".to_string());
    }

    let device_blob = crypto::encrypt(&hex_encode(master), device_hash)
        .map_err(|e| format!("Falha ao embrulhar a chave com o hash do aparelho: {}", e))?;

    let file = KeyFile {
        v: 1,
        dpapi: dpapi_wrap(master).map(|blob| hex_encode(&blob)),
        device: Some(hex_encode(&device_blob)),
    };

    if file.dpapi.is_none() && file.device.is_none() {
        return Err("Nenhum embrulho disponível para a chave mestra".to_string());
    }

    let json =
        serde_json::to_string(&file).map_err(|e| format!("Falha ao serializar o .key: {}", e))?;

    if let Some(parent) = key_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let tmp_path = key_path.with_extension("key.tmp");
    fs::write(&tmp_path, json).map_err(|e| format!("Falha ao gravar o .key: {}", e))?;
    crate::data::versions::atomic_replace(&tmp_path, key_path)
        .map_err(|e| format!("Falha ao substituir o .key: {}", e))?;
    Ok(())
}

/// Chamado quando o usuário passa a usar senha: a senha manda, e um `.key`
/// esquecido em disco só serviria para confundir a recuperação.
pub fn remove_key_file(key_path: &Path) {
    if key_path.exists() {
        let _ = fs::remove_file(key_path);
    }
    let _ = fs::remove_file(key_path.with_extension("key.tmp"));
}

#[cfg(target_os = "windows")]
fn dpapi_entropy() -> &'static [u8] {
    VAULT_KEY_ENTROPY
}

#[cfg(not(target_os = "windows"))]
fn dpapi_entropy() -> &'static [u8] {
    &[]
}

#[cfg(target_os = "windows")]
fn dpapi_wrap(master: &[u8]) -> Option<Vec<u8>> {
    crypto::dpapi_protect(master, VAULT_KEY_ENTROPY)
}

#[cfg(not(target_os = "windows"))]
fn dpapi_wrap(_master: &[u8]) -> Option<Vec<u8>> {
    None
}
