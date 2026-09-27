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
