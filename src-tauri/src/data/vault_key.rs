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

/// Chave mestra recuperada, e por qual caminho.
pub struct RecoveredKey {
    pub master: Vec<u8>,
    /// `false` quando só o embrulho do aparelho abriu. O chamador usa isso para
    /// regravar o `.key` e voltar a ter os dois caminhos.
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

#[cfg(test)]
mod vault_key_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_key_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        crypto::init();
        std::env::temp_dir().join(format!("ram-vaultkey-{name}-{nanos}.key"))
    }

    struct TempKey(PathBuf);

    impl Drop for TempKey {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
            let _ = fs::remove_file(self.0.with_extension("key.tmp"));
        }
    }

    #[test]
    fn the_key_file_sits_next_to_the_vault_so_portable_mode_carries_both() {
        let vault = PathBuf::from("D:\\pendrive\\RAM\\AccountData.json");
        let key = key_file_path_for(&vault);
        assert_eq!(key.parent(), vault.parent(), "a chave segue o vault");
        assert_eq!(
            key.file_name().and_then(|n| n.to_str()),
            Some("AccountData.key")
        );
        // Sem extensão fixa no meio: trocar a pasta de dados leva os dois.
        assert_ne!(key, vault);
    }

    #[test]
    fn a_generated_master_key_is_32_random_bytes() {
        crypto::init();
        let a = generate_master_key();
        let b = generate_master_key();
        assert_eq!(a.len(), MASTER_KEY_LEN);
        assert_eq!(b.len(), MASTER_KEY_LEN);
        assert_ne!(a, b, "a chave tem que ser sorteada, não derivada");
        assert!(a.iter().any(|byte| *byte != 0), "chave toda zero");
    }

    #[test]
    fn the_master_hash_is_stable_per_key_and_separate_from_user_passwords() {
        crypto::init();
        let master = vec![7u8; MASTER_KEY_LEN];
        let other = vec![8u8; MASTER_KEY_LEN];
        assert_eq!(master_password_hash(&master), master_password_hash(&master));
        assert_ne!(master_password_hash(&master), master_password_hash(&other));
        // Uma senha de usuário nunca pode cair no mesmo hash de uma chave mestra.
        assert_ne!(
            master_password_hash(&master),
            crypto::hash_password(&hex_encode(&master))
        );
    }

    /// O teste que a Task 8 pede: a chave mestra volta inteira pelos **dois**
    /// embrulhos. No Windows os dois existem; fora dele só o do aparelho.
    #[test]
    fn the_master_key_round_trips_through_both_wrappers() {
        let path = temp_key_path("round-trip");
        let _guard = TempKey(path.clone());
        let master = generate_master_key();
        let device_hash = crypto::primary_device_hash();

        store_master_key(&path, &master, &device_hash).expect("gravar o .key");

        // 1. O caminho normal recupera a chave.
        let recovered = load_master_key(&path).expect("recuperar a chave");
        assert_eq!(recovered.master, master);
        #[cfg(target_os = "windows")]
        assert!(
            recovered.via_dpapi,
            "no Windows o DPAPI é o embrulho principal"
        );

        // 2. Com o embrulho do DPAPI arrancado, o do aparelho ainda abre.
        let raw = fs::read(&path).expect("ler o .key");
        let mut file: KeyFile = serde_json::from_slice(&raw).expect("parse");
        file.dpapi = None;
        fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();
        let only_device = load_master_key(&path).expect("o embrulho do aparelho tem que abrir");
        assert_eq!(only_device.master, master);
        assert!(!only_device.via_dpapi);

        // 3. Com o embrulho do aparelho arrancado, o DPAPI ainda abre (Windows).
        #[cfg(target_os = "windows")]
        {
            let mut file: KeyFile = serde_json::from_slice(&raw).expect("parse");
            file.device = None;
            fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();
            let only_dpapi = load_master_key(&path).expect("o DPAPI tem que abrir");
            assert_eq!(only_dpapi.master, master);
            assert!(only_dpapi.via_dpapi);
        }
    }

    /// A chave em claro não pode aparecer no arquivo — nem em hex, nem crua.
    #[test]
    fn the_key_file_never_contains_the_master_key_in_the_clear() {
        let path = temp_key_path("no-plaintext");
        let _guard = TempKey(path.clone());
        let master = generate_master_key();

        store_master_key(&path, &master, &crypto::primary_device_hash()).expect("gravar");

        let raw = fs::read(&path).expect("ler");
        let hex = hex_encode(&master);
        assert!(
            !String::from_utf8_lossy(&raw).contains(&hex),
            "a chave vazou em hex no .key"
        );
        assert!(
            !raw.windows(master.len()).any(|w| w == master.as_slice()),
            "a chave vazou crua no .key"
        );
    }

    /// Arquivo de outro aparelho / de outro usuário do Windows: não abre, e
    /// devolver `None` é a única resposta aceitável — quem chama não pode achar
    /// que pode regravar o vault.
    #[test]
    fn a_key_file_from_another_device_does_not_open() {
        let path = temp_key_path("foreign");
        let _guard = TempKey(path.clone());
        let master = generate_master_key();
        let foreign_hash = crypto::device_hash_for_identifier("outro-computador-que-nao-existe");

        // Grava só o embrulho do aparelho, com um identificador que este PC não tem.
        let device_blob = crypto::encrypt(&hex_encode(&master), &foreign_hash).unwrap();
        let file = KeyFile {
            v: 1,
            dpapi: None,
            device: Some(hex_encode(&device_blob)),
        };
        fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();

        assert!(load_master_key(&path).is_none());
    }

    /// Nem arquivo ausente, nem lixo, nem hex torto podem derrubar o app: tudo
    /// vira `None`.
    #[test]
    fn garbage_and_missing_files_return_none_instead_of_panicking() {
        crypto::init();
        let missing = temp_key_path("missing");
        assert!(load_master_key(&missing).is_none());

        let path = temp_key_path("garbage");
        let _guard = TempKey(path.clone());
        for content in [
            b"".to_vec(),
            b"not json".to_vec(),
            br#"{"v":1}"#.to_vec(),
            br#"{"v":1,"device":"zz"}"#.to_vec(),
            br#"{"v":1,"device":"abc"}"#.to_vec(),
            br#"{"v":1,"dpapi":"00","device":"0011"}"#.to_vec(),
        ] {
            fs::write(&path, &content).unwrap();
            assert!(
                load_master_key(&path).is_none(),
                "abriu conteúdo inválido: {:?}",
                String::from_utf8_lossy(&content)
            );
        }
    }

    #[test]
    fn hex_round_trips_and_rejects_malformed_input() {
        assert_eq!(hex_encode(&[0x00, 0x0f, 0xff]), "000fff");
        assert_eq!(hex_decode("000fff"), Some(vec![0x00, 0x0f, 0xff]));
        assert_eq!(hex_decode(" 000fff \n"), Some(vec![0x00, 0x0f, 0xff]));
        assert_eq!(hex_decode(""), None);
        assert_eq!(hex_decode("abc"), None, "tamanho ímpar");
        assert_eq!(hex_decode("zz"), None);
        assert_eq!(hex_decode("00gg"), None);
    }

    #[test]
    fn a_master_key_with_the_wrong_length_is_refused_instead_of_stored() {
        let path = temp_key_path("bad-length");
        let _guard = TempKey(path.clone());
        crypto::init();
        assert!(store_master_key(&path, &[1u8; 16], &crypto::primary_device_hash()).is_err());
        assert!(!path.exists(), "não pode deixar .key pela metade");
    }

    #[test]
    fn removing_the_key_file_also_clears_a_leftover_temp_file() {
        let path = temp_key_path("remove");
        let _guard = TempKey(path.clone());
        crypto::init();
        store_master_key(&path, &generate_master_key(), &crypto::primary_device_hash()).unwrap();
        fs::write(path.with_extension("key.tmp"), b"lixo").unwrap();

        remove_key_file(&path);

        assert!(!path.exists());
        assert!(!path.with_extension("key.tmp").exists());
        // Remover duas vezes não pode explodir.
        remove_key_file(&path);
    }

    /// O `[0]` dos candidatos é o que embrulha a chave. Se ele fosse o
    /// `MachineGuid`, o isolamento pré-launch (que reescreve esse valor)
    /// trancaria o usuário fora das contas.
    #[test]
    fn the_primary_device_hash_is_the_first_candidate_and_is_stable() {
        crypto::init();
        let candidates = crypto::device_hash_candidates();
        assert!(!candidates.is_empty());
        assert_eq!(crypto::primary_device_hash(), candidates[0]);
        assert_eq!(crypto::primary_device_hash(), crypto::primary_device_hash());
        assert_eq!(candidates[0].len(), 64, "sha512");

        // O último candidato é a constante de último recurso: existe para nunca
        // faltar um hash, e por isso não pode ser o primeiro num PC normal.
        let last_resort = crypto::device_hash_for_identifier("ram-device");
        assert_eq!(
            candidates.last(),
            Some(&last_resort),
            "a constante é o último recurso"
        );
    }
}
