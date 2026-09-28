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
//! - `AccountData.json` copiado para outra máquina (pen drive, upload, anexo)
//!   **sem** o `.key` do lado;
//! - outro usuário do Windows no mesmo PC.
//!
//! E **não** protege contra:
//!
//! - **malware rodando como o próprio usuário** — esse programa lê o `.key` e
//!   chama `CryptUnprotectData` exatamente como o app chama. Nada guardado no
//!   perfil do usuário resiste a isso;
//! - **backup vazado.** O zip de backup do app **tem** que levar o `.key` junto
//!   (`DATA_FILES`), senão um vault cifrado nunca poderia ser restaurado. Então
//!   quem tem o zip tem a chave, e o que sobra protegendo é o embrulho do
//!   aparelho — `sha512("COMPUTERNAME|USERNAME|ram-device-v1")`, duas strings que
//!   quem tem o zip normalmente já sabe (o `USERNAME` aparece em caminhos dentro
//!   do próprio `RAMSettings.ini`). Uma versão anterior deste comentário listava
//!   backup vazado como **protegido**: era falso, e a tela de criptografia
//!   repetia a mentira;
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
    load_master_key_blob(&data)
}

/// Só a chave mestra, a partir dos bytes do arquivo. Usado por
/// [`inspect_key_file`], que precisa separar erro de leitura de conteúdo ruim.
fn load_master_key_from(data: &[u8]) -> Option<Vec<u8>> {
    load_master_key_blob(data).map(|r| r.master)
}

fn load_master_key_blob(data: &[u8]) -> Option<RecoveredKey> {
    let file: KeyFile = serde_json::from_slice(data).ok()?;

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

/// Quais embrulhos o `.key` gravado realmente ficou tendo.
///
/// Existe para o chamador poder **avisar** quando o arquivo degradou, em vez de
/// degradar em silêncio (ver [`resolve_dpapi_blob`]).
pub struct StoredKeyHealth {
    pub dpapi_present: bool,
    pub device_present: bool,
    /// `false` quando os bytes foram gravados mas o `sync_all` não pôde ser
    /// confirmado — a gravação segue valendo, o que se perde é a garantia contra
    /// queda de energia. Existe para isso **não** ser invisível: a versão anterior
    /// devolvia esse fato num `bool` que ninguém lia.
    pub synced: bool,
}

/// Falha ao gravar o `.key`, com a informação que decide o tom do aviso.
///
/// `transient` separa "o antivírus segurou o arquivo por um instante" de "este
/// arquivo não vai ser gravado". Sem essa distinção o app dispara o aviso mais
/// assustador que tem para um arquivo que está perfeito — e aviso falso treina o
/// usuário a ignorar avisos, o que desarma justamente a rede contra o lockout.
#[derive(Debug)]
pub struct KeyWriteError {
    pub message: String,
    pub transient: bool,
}

/// Erros de IO que **somem sozinhos**: scanner de antivírus, indexador ou backup
/// segurando o handle por um instante.
///
/// **Isto foi medido, não deduzido** (Windows, `rustc 1.96`), porque a primeira
/// versão classificava por `ErrorKind` e saía invertida nos dois casos que
/// importavam:
///
/// | Situação | `raw_os_error` | `ErrorKind` |
/// |---|---|---|
/// | diretório no lugar do `.key` (**permanente**) | `5` | `PermissionDenied` |
/// | ACL negada, portátil em `Program Files` (**permanente**) | `5` | `PermissionDenied` |
/// | antivírus com handle exclusivo (**transitório**) | **`32`** | `Uncategorized` |
/// | `rename` sobre arquivo preso (**transitório**) | `5` | `PermissionDenied` |
///
/// Ou seja: o caso benigno chegava como `Uncategorized` e levava o alarme
/// vermelho, e o caso permanente chegava como `PermissionDenied` e levava
/// "tento de novo na próxima alteração" **para sempre**. `ERROR_SHARING_VIOLATION`
/// (32) e `ERROR_LOCK_VIOLATION` (33) não têm `ErrorKind` estável, então só o
/// `raw_os_error` os identifica.
///
/// `PermissionDenied` aparece nos **dois** lados, logo é ambíguo — e ambíguo conta
/// como **permanente**: sub-avisar é o que custa contas. Um vermelho falso se
/// limpa na gravação seguinte; um âmbar falso esconde a falha permanente.
fn is_transient_io(error: &std::io::Error) -> bool {
    use std::io::ErrorKind;

    #[cfg(target_os = "windows")]
    if matches!(error.raw_os_error(), Some(32) | Some(33)) {
        return true;
    }

    matches!(
        error.kind(),
        ErrorKind::Interrupted | ErrorKind::WouldBlock | ErrorKind::TimedOut
    )
}

/// O arquivo de chave guarda **esta** chave mestra?
///
/// É a única pergunta que o reparo precisa fazer, e é por isso que não existe mais
/// um enum de estado aqui: a versão anterior separava "não consegui ler" de "está
/// corrompido" e **nenhum consumidor olhava a diferença** — os dois caíam no mesmo
/// braço. Separação decorativa, com um comentário que contradizia o código.
///
/// `false` cobre tudo que pede reparo: ausente, ilegível agora, truncado, JSON
/// inválido, nenhum embrulho que abra, e o `.key` que abre mas entrega **outra**
/// chave (o caso do OneDrive revertendo o arquivo).
pub fn key_file_holds_master(key_path: &Path, master: &[u8]) -> bool {
    match fs::read(key_path) {
        Ok(data) => load_master_key_from(&data).as_deref() == Some(master),
        Err(_) => false,
    }
}

/// Qual embrulho DPAPI vai para o arquivo.
///
/// `new_blob` é o que o `CryptProtectData` produziu agora — `None` quando ele
/// falhou (ou fora do Windows). Neste caso o embrulho **anterior é preservado**,
/// porque a regravação incondicional a cada open significaria, sem isto,
/// substituir um DPAPI saudável por nada num instante de falha: o `.key` que
/// acabou de abrir pelo DPAPI passaria a ter só a proteção fraca, sem aviso.
///
/// Só preserva um blob que abre para **esta mesma** chave mestra. Preservar o de
/// outra chave faria `load_master_key` devolver a chave errada — pior que ficar
/// sem DPAPI.
fn resolve_dpapi_blob(
    new_blob: Option<String>,
    key_path: &Path,
    master: &[u8],
) -> Option<String> {
    if new_blob.is_some() {
        return new_blob;
    }
    let previous = fs::read(key_path).ok()?;
    let previous: KeyFile = serde_json::from_slice(&previous).ok()?;
    let hex = previous.dpapi?;
    let blob = hex_decode(&hex)?;
    // Barato: desembrulhar pelo DPAPI não paga argon2.
    let recovered = crypto::dpapi_unprotect(&blob, dpapi_entropy())?;
    (recovered == master).then_some(hex)
}

/// Grava o `.key` com os dois embrulhos.
///
/// A gravação é write-then-**fsync**-then-rename: sem o fsync, o rename pode
/// publicar um arquivo cujo conteúdo ainda não chegou ao disco, e um `.key`
/// vazio/truncado é um vault que não abre mais pelo caminho do aparelho. Isso
/// passou a importar mais desde que a regravação acontece a cada open — a janela
/// deixou de ser uma vez na vida do arquivo.
pub fn store_master_key(
    key_path: &Path,
    master: &[u8],
    device_hash: &[u8],
) -> Result<StoredKeyHealth, KeyWriteError> {
    let fatal = |message: String| KeyWriteError {
        message,
        transient: false,
    };

    if master.len() != MASTER_KEY_LEN {
        return Err(fatal("Chave mestra com tamanho inválido".to_string()));
    }

    let device_blob = crypto::encrypt(&hex_encode(master), device_hash).map_err(|e| {
        fatal(format!(
            "Falha ao embrulhar a chave com o hash do aparelho: {}",
            e
        ))
    })?;

    let file = KeyFile {
        v: 1,
        dpapi: resolve_dpapi_blob(
            dpapi_wrap(master).map(|blob| hex_encode(&blob)),
            key_path,
            master,
        ),
        device: Some(hex_encode(&device_blob)),
    };

    if file.dpapi.is_none() && file.device.is_none() {
        return Err(fatal(
            "Nenhum embrulho disponível para a chave mestra".to_string(),
        ));
    }
    let json = serde_json::to_string(&file)
        .map_err(|e| fatal(format!("Falha ao serializar o .key: {}", e)))?;

    if let Some(parent) = key_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let tmp_path = key_path.with_extension("key.tmp");
    let synced = crate::data::versions::write_all_synced(&tmp_path, json.as_bytes()).map_err(
        |e| KeyWriteError {
            message: format!("Falha ao gravar o .key: {}", e),
            transient: is_transient_io(&e),
        },
    )?;
    // O rename é o passo que o antivírus costuma barrar; um erro aqui é o caso
    // mais provável de "tenta de novo no save seguinte e funciona".
    crate::data::versions::atomic_replace_io(&tmp_path, key_path).map_err(|e| KeyWriteError {
        message: format!("Falha ao substituir o .key: {}", e),
        transient: is_transient_io(&e),
    })?;

    Ok(StoredKeyHealth {
        dpapi_present: file.dpapi.is_some(),
        device_present: file.device.is_some(),
        synced,
    })
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

    /// **Quebra 6.** A regravação incondicional (correção do Critical 1) trouxe um
    /// risco novo: se o `CryptProtectData` falhar **naquele instante**, o `.key`
    /// que acabou de abrir pelo DPAPI seria substituído por um sem DPAPI, sem
    /// aviso — uma janela com só a proteção fraca. Quando o embrulho novo não pode
    /// ser produzido, o anterior é **preservado**.
    ///
    /// Mas só se ele for da **mesma** chave mestra: preservar o blob de outra
    /// chave faria `load_master_key` devolver a chave errada, o que é pior que não
    /// ter DPAPI nenhum.
    #[test]
    fn a_healthy_dpapi_wrapper_is_preserved_when_a_new_one_cannot_be_made() {
        let path = temp_key_path("preserve-dpapi");
        let _guard = TempKey(path.clone());
        let master = generate_master_key();
        store_master_key(&path, &master, &crypto::primary_device_hash()).expect("gravar");

        let existing = fs::read(&path).expect("ler");
        let existing_dpapi = serde_json::from_slice::<KeyFile>(&existing)
            .expect("parse")
            .dpapi;

        #[cfg(target_os = "windows")]
        {
            let existing_dpapi = existing_dpapi.clone().expect("no Windows tem DPAPI");
            // Embrulho novo indisponível + blob anterior da **mesma** chave: preserva.
            assert_eq!(
                resolve_dpapi_blob(None, &path, &master),
                Some(existing_dpapi),
                "deixou cair um embrulho DPAPI saudável"
            );
            // Blob anterior de **outra** chave: não serve, e preservá-lo faria
            // `load_master_key` devolver a chave errada.
            let other = generate_master_key();
            assert_eq!(
                resolve_dpapi_blob(None, &path, &other),
                None,
                "preservou um embrulho de outra chave mestra"
            );
        }

        // O embrulho novo, quando existe, é o que vale.
        assert_eq!(
            resolve_dpapi_blob(Some("aa".to_string()), &path, &master),
            Some("aa".to_string())
        );
        // Arquivo inexistente não inventa embrulho.
        let missing = temp_key_path("preserve-dpapi-missing");
        assert_eq!(resolve_dpapi_blob(None, &missing, &master), None);
        let _ = existing_dpapi;
    }

    /// Quem grava precisa saber se o `.key` ficou com os dois embrulhos ou só com
    /// um — é o que permite avisar em vez de degradar em silêncio (Quebra 6).
    #[test]
    fn storing_the_key_reports_whether_the_dpapi_wrapper_made_it() {
        let path = temp_key_path("health");
        let _guard = TempKey(path.clone());
        let health = store_master_key(&path, &generate_master_key(), &crypto::primary_device_hash())
            .expect("gravar");
        assert!(health.device_present, "o embrulho do aparelho é obrigatório");
        #[cfg(target_os = "windows")]
        assert!(
            health.dpapi_present,
            "no Windows o DPAPI tem que estar presente num caminho saudável"
        );
        #[cfg(not(target_os = "windows"))]
        assert!(!health.dpapi_present, "fora do Windows não existe DPAPI");
    }

    /// **Quebra 4.** O que decide o tom do aviso — e a primeira versão estava
    /// **invertida nos dois casos que motivaram a correção**. Medido no Windows
    /// (`rustc 1.96`, sonda em `fs::read`/`File::create`/`fs::rename`):
    ///
    /// | Situação | `raw_os_error` | `ErrorKind` |
    /// |---|---|---|
    /// | diretório no lugar do `.key` (**permanente**) | `5` | `PermissionDenied` |
    /// | ACL negada, portátil em `Program Files` (**permanente**) | `5` | `PermissionDenied` |
    /// | antivírus/indexador com handle exclusivo (**transitório**) | **`32`** | `Uncategorized` |
    /// | `rename` sobre arquivo preso (**transitório**) | `5` | `PermissionDenied` |
    ///
    /// Ou seja: `ErrorKind` sozinho **não** resolve. O código 32
    /// (`ERROR_SHARING_VIOLATION`) e o 33 (`ERROR_LOCK_VIOLATION`) não têm
    /// `ErrorKind` estável, então só o `raw_os_error` os identifica; e
    /// `PermissionDenied` aparece nos dois lados, ou seja, é ambíguo.
    ///
    /// **Ambíguo conta como permanente**, de propósito: sub-avisar é o que custa
    /// contas, e um vermelho falso se limpa na gravação seguinte, enquanto um
    /// âmbar falso ("tento de novo") pode esconder uma falha permanente para
    /// sempre — que foi exatamente o bug medido.
    #[test]
    fn the_transient_classifier_matches_the_codes_windows_actually_reports() {
        use std::io::{Error, ErrorKind};

        // O antivírus segurando o handle: o caso que a Quebra 4 existe para acalmar.
        assert!(
            is_transient_io(&Error::from_raw_os_error(32)),
            "ERROR_SHARING_VIOLATION (32) é o antivírus segurando o handle"
        );
        assert!(
            is_transient_io(&Error::from_raw_os_error(33)),
            "ERROR_LOCK_VIOLATION (33) é da mesma família"
        );

        // Diretório no lugar do arquivo / ACL negada: permanente, e tem que
        // receber o aviso grave em vez de "tento de novo" para sempre.
        assert!(
            !is_transient_io(&Error::from_raw_os_error(5)),
            "ACCESS_DENIED é ambíguo, e ambíguo conta como permanente"
        );
        assert!(
            !is_transient_io(&Error::from(ErrorKind::PermissionDenied)),
            "PermissionDenied sem código também é ambíguo"
        );

        // Os que o `ErrorKind` já resolve sozinho.
        for kind in [
            ErrorKind::Interrupted,
            ErrorKind::WouldBlock,
            ErrorKind::TimedOut,
        ] {
            assert!(
                is_transient_io(&Error::from(kind)),
                "{kind:?} devia ser transitório"
            );
        }
        for kind in [
            ErrorKind::NotFound,
            ErrorKind::InvalidInput,
            ErrorKind::InvalidData,
            ErrorKind::Unsupported,
            ErrorKind::OutOfMemory,
        ] {
            assert!(
                !is_transient_io(&Error::from(kind)),
                "{kind:?} não é transitório"
            );
        }
    }

    /// A pergunta que o reparo faz: **este arquivo guarda a chave desta sessão?**
    ///
    /// Substituiu o `KeyFileState`, que separava `Unreadable` de `Unusable` sem
    /// nenhum consumidor olhar a diferença — separação decorativa, e com um
    /// comentário que contradizia o código.
    #[test]
    fn key_file_holds_master_answers_only_the_question_the_repair_asks() {
        let path = temp_key_path("holds");
        let _guard = TempKey(path.clone());
        let master = generate_master_key();
        let other = generate_master_key();

        // Ausente, lixo, vazio e JSON sem embrulho que abra: não guarda.
        assert!(!key_file_holds_master(&path, &master), "arquivo ausente");
        for content in [
            b"nao e json".to_vec(),
            b"".to_vec(),
            br#"{"v":1,"device":"00ff"}"#.to_vec(),
        ] {
            fs::write(&path, &content).unwrap();
            assert!(!key_file_holds_master(&path, &master));
        }

        store_master_key(&path, &master, &crypto::primary_device_hash()).unwrap();
        assert!(key_file_holds_master(&path, &master), "o .key desta sessão");
        assert!(
            !key_file_holds_master(&path, &other),
            "abre, mas guarda outra chave — é o caso do OneDrive revertendo o .key"
        );
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
