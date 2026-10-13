// Backup e restauração dos dados do usuário.
//
// Cada backup é **um** arquivo `.zip` em `<pasta de dados>/backups/`, chamado
// `backup-AAAAMMDD-HHMMSS[-rótulo].zip`. Dentro vão os arquivos/pastas do
// usuário (`DATA_FILES`/`DATA_DIRS` de `data/settings/paths.rs`) que existirem,
// mais o catálogo de versões (`RAMVersions.json`, que pode morar fora da pasta
// de dados) e um `backup-manifest.json` com formato, data (UTC), rótulo, se o
// backup é automático e a lista de arquivos com o tamanho de cada um.
//
// Regras que não podem ser quebradas:
// - **Restaurar nunca escreve sem rede de segurança:** `restore_backup` cria
//   antes um backup automático do estado atual e devolve o id dele.
// - **Nada é extraído para fora da pasta de dados:** só entram nomes que estão
//   na allowlist (`resolve_entry_target`); qualquer outro vira `skipped`.
// - **Retenção só apaga o que o app criou:** a limpeza mantém no máximo
//   `MAX_AUTOMATIC_BACKUPS` backups automáticos (os de segurança criados antes
//   de restaurar). Backup feito pelo usuário — com ou sem rótulo — e zip
//   ilegível (do qual não dá para afirmar que é automático) nunca são apagados
//   automaticamente.

/// Pasta dos backups, dentro da pasta de dados.
pub const BACKUPS_DIR_NAME: &str = "backups";
/// Manifesto gravado dentro de cada zip.
pub const BACKUP_MANIFEST_NAME: &str = "backup-manifest.json";
/// Versão do formato do backup (campo `format` do manifesto).
pub const BACKUP_FORMAT: &str = "ram-backup-v1";
/// Nome do catálogo de versões dentro do zip.
pub const VERSIONS_CATALOG_NAME: &str = "RAMVersions.json";
/// Quantos backups **automáticos** ficam guardados.
pub const MAX_AUTOMATIC_BACKUPS: usize = 10;
/// Limite do rótulo no nome do arquivo.
const MAX_LABEL_CHARS: usize = 40;
/// Rótulo do backup criado automaticamente antes de uma restauração.
const SAFETY_BACKUP_LABEL: &str = "antes-da-restauracao";
/// Sufixo dos temporários (nunca termina em `.zip`, então não aparece na lista).
const TMP_SUFFIX: &str = ".ram-tmp";

/// Onde cada arquivo do backup mora em disco. Separado de `get_runtime_data_dir`
/// (resolvida uma vez por processo com `OnceLock`) para os testes poderem usar
/// pastas temporárias.
#[derive(Debug, Clone)]
pub struct BackupLayout {
    pub data_dir: std::path::PathBuf,
    pub versions_catalog: std::path::PathBuf,
}

impl BackupLayout {
    pub fn backups_dir(&self) -> std::path::PathBuf {
        self.data_dir.join(BACKUPS_DIR_NAME)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFileEntry {
    pub name: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    pub format: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub label: Option<String>,
    /// `true` só nos backups que o app cria sozinho (hoje, o de segurança antes
    /// de restaurar). É o que a retenção usa para decidir o que pode apagar.
    #[serde(default)]
    pub automatic: bool,
    #[serde(default)]
    pub app_version: Option<String>,
    #[serde(default)]
    pub files: Vec<BackupFileEntry>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupEntry {
    pub id: String,
    pub file_name: String,
    /// RFC3339 em UTC.
    pub created_at: String,
    pub label: Option<String>,
    pub size_bytes: u64,
    pub files: Vec<String>,
    /// `false` quando o zip não abre ou não tem um manifesto deste formato. O
    /// arquivo continua na lista — o usuário precisa ver que ele existe.
    pub valid: bool,
    pub automatic: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RestoreOutcome {
    pub restored: Vec<String>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    pub backup_id: String,
    /// Backup automático do estado anterior, criado antes de qualquer escrita.
    pub safety_backup_id: Option<String>,
    pub restored: Vec<String>,
    /// Entradas do zip recusadas (nome fora da allowlist).
    pub skipped: Vec<String>,
    pub accounts_reloaded: bool,
    pub requires_restart: bool,
    pub restart_reasons: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupsInfo {
    pub dir: String,
    pub portable: bool,
    pub total_bytes: u64,
    pub count: usize,
}

// ---- nomes -----------------------------------------------------------------------

/// Reduz o rótulo a algo seguro para nome de arquivo: letras/números (incluindo
/// unicode), `-` e `_`; qualquer outra coisa (barra, `..`, controles, `*?"<>|:`)
/// vira um único `-`. Devolve `None` quando não sobra nada.
pub fn sanitize_backup_label(label: &str) -> Option<String> {
    let mut out = String::new();
    let mut pending_sep = false;
    for ch in label.chars() {
        if ch.is_alphanumeric() || ch == '-' || ch == '_' {
            if pending_sep && !out.is_empty() {
                out.push('-');
            }
            pending_sep = false;
            out.push(ch);
            if out.chars().count() >= MAX_LABEL_CHARS {
                break;
            }
        } else {
            pending_sep = true;
        }
    }

    let trimmed = out.trim_matches('-').trim_matches('_');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Rótulo como o usuário vê na lista: o texto que ele digitou, sem controles,
/// com espaços normalizados e do mesmo tamanho máximo do nome do arquivo. O
/// nome do arquivo continua usando a versão saneada — aqui é só exibição.
pub fn display_backup_label(label: &str) -> Option<String> {
    sanitize_backup_label(label)?;
    let cleaned: String = label
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let normalized = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let capped: String = normalized.chars().take(MAX_LABEL_CHARS).collect();
    let trimmed = capped.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// `backup-AAAAMMDD-HHMMSS[-rótulo].zip`.
pub fn backup_file_name(now: chrono::DateTime<chrono::Utc>, label: Option<&str>) -> String {
    let stamp = now.format("%Y%m%d-%H%M%S");
    match label.and_then(sanitize_backup_label) {
        Some(label) => format!("backup-{}-{}.zip", stamp, label),
        None => format!("backup-{}.zip", stamp),
    }
}

/// Lê data e rótulo de volta do nome do arquivo — é o que salva a listagem
/// quando o zip está ilegível e o manifesto não pode ser aberto.
fn parse_backup_file_stem(stem: &str) -> Option<(chrono::DateTime<chrono::Utc>, Option<String>)> {
    let rest = stem.strip_prefix("backup-")?;
    // `get`, não `split_at`: um nome fora do padrão com acento logo no
    // começo não pode derrubar a listagem.
    let stamp = rest.get(..15)?;
    let tail = &rest[15..];
    let naive = chrono::NaiveDateTime::parse_from_str(stamp, "%Y%m%d-%H%M%S").ok()?;
    let label = tail
        .strip_prefix('-')
        .map(|l| l.to_string())
        .filter(|l| !l.is_empty());
    Some((naive.and_utc(), label))
}

/// Id do backup = nome do arquivo sem **um** `.zip` no fim.
fn backup_id_from_file_name(file_name: &str) -> String {
    file_name
        .strip_suffix(".zip")
        .unwrap_or(file_name)
        .to_string()
}

/// Id → caminho do zip, recusando qualquer coisa que não seja um nome simples
/// dentro da pasta de backups.
pub fn resolve_backup_path(dir: &std::path::Path, id: &str) -> Result<std::path::PathBuf, String> {
    let trimmed = id.trim();
    // Sem cortar por posição em bytes: num rótulo acentuado ("avançado") o
    // corte caía no meio de uma letra e o Rust entrava em pânico, fechando o app.
    let stem = match trimmed.len().checked_sub(4).and_then(|cut| trimmed.get(cut..).map(|ext| (cut, ext))) {
        Some((cut, ext)) if ext.eq_ignore_ascii_case(".zip") => &trimmed[..cut],
        _ => trimmed,
    };

    if stem.is_empty()
        || stem == "."
        || stem == ".."
        || stem.contains('/')
        || stem.contains('\\')
        || stem.contains(':')
        || stem.contains('\0')
    {
        return Err(format!("Invalid backup id: {id:?}"));
    }

    Ok(dir.join(format!("{stem}.zip")))
}

// ---- allowlist de extração ----------------------------------------------------------

/// Único ponto que decide onde uma entrada do zip pode ser gravada. Só nomes da
/// allowlist passam, então zip-slip (`../`, caminho absoluto, `C:\…`) não tem
/// como virar um caminho fora da pasta de dados.
pub fn resolve_entry_target(
    layout: &BackupLayout,
    name: &str,
) -> Option<std::path::PathBuf> {
    let normalized = name.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').collect();
    if parts.iter().any(|p| {
        p.is_empty() || *p == "." || *p == ".." || p.contains(':') || p.contains('\0')
    }) {
        return None;
    }

    let (first, rest) = parts.split_first()?;

    if rest.is_empty() {
        if *first == VERSIONS_CATALOG_NAME {
            return Some(layout.versions_catalog.clone());
        }
        if data::settings::DATA_FILES.contains(first) {
            return Some(layout.data_dir.join(first));
        }
        return None;
    }

    if data::settings::DATA_DIRS.contains(first) {
        let mut path = layout.data_dir.join(first);
        for part in rest {
            path.push(part);
        }
        return Some(path);
    }

    None
}

// ---- criar ---------------------------------------------------------------------------

fn collect_dir_entries(
    layout: &BackupLayout,
    root: &std::path::Path,
    prefix: &str,
    out: &mut Vec<(String, std::path::PathBuf)>,
) {
    let Ok(read) = std::fs::read_dir(root) else {
        return;
    };
    let mut children: Vec<std::fs::DirEntry> = read.filter_map(|e| e.ok()).collect();
    children.sort_by_key(|e| e.file_name());

    for child in children {
        let Some(name) = child.file_name().to_str().map(|s| s.to_string()) else {
            continue;
        };
        let archive_name = format!("{prefix}/{name}");
        let path = child.path();
        if path.is_dir() {
            collect_dir_entries(layout, &path, &archive_name, out);
        } else if path.is_file() && resolve_entry_target(layout, &archive_name).is_some() {
            out.push((archive_name, path));
        }
    }
}

/// O que entra no zip: os `DATA_FILES`/`DATA_DIRS` que existirem + o catálogo de
/// versões. Nada que não possa ser restaurado depois é guardado.
pub fn collect_backup_sources(layout: &BackupLayout) -> Vec<(String, std::path::PathBuf)> {
    let mut out: Vec<(String, std::path::PathBuf)> = Vec::new();

    for name in data::settings::DATA_FILES {
        let path = layout.data_dir.join(name);
        if path.is_file() {
            out.push(((*name).to_string(), path));
        }
    }

    for dir in data::settings::DATA_DIRS {
        let root = layout.data_dir.join(dir);
        if root.is_dir() {
            collect_dir_entries(layout, &root, dir, &mut out);
        }
    }

    if layout.versions_catalog.is_file() && !out.iter().any(|(n, _)| n == VERSIONS_CATALOG_NAME) {
        out.push((
            VERSIONS_CATALOG_NAME.to_string(),
            layout.versions_catalog.clone(),
        ));
    }

    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Reserva um nome livre na pasta de backups (dois backups no mesmo segundo não
/// podem se sobrescrever). O arquivo vazio criado aqui é trocado no fim pelo zip
/// pronto, com `atomic_replace`.
fn reserve_backup_path(
    dir: &std::path::Path,
    now: chrono::DateTime<chrono::Utc>,
    label: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    let base = backup_file_name(now, label);
    let stem = backup_id_from_file_name(&base);

    for attempt in 0..1000 {
        let name = if attempt == 0 {
            base.clone()
        } else {
            format!("{stem}-{}.zip", attempt + 1)
        };
        let path = dir.join(&name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Failed to create backup file: {e}")),
        }
    }

    Err("Failed to pick a backup file name".to_string())
}

pub fn create_backup_in(
    layout: &BackupLayout,
    label: Option<&str>,
    automatic: bool,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<BackupEntry, String> {
    use std::io::Write;

    let dir = layout.backups_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create backups folder: {e}"))?;

    let sources = collect_backup_sources(layout);
    let final_path = reserve_backup_path(&dir, now, label)?;
    let tmp_path = final_path.with_extension(format!("zip{TMP_SUFFIX}"));

    let written = (|| -> Result<BackupManifest, String> {
        let file = std::fs::File::create(&tmp_path)
            .map_err(|e| format!("Failed to create backup file: {e}"))?;
        let mut writer = zip::ZipWriter::new(file);
        let options =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        let mut files = Vec::new();
        for (name, path) in &sources {
            let bytes = match std::fs::read(path) {
                Ok(bytes) => bytes,
                // Arquivo sumiu/travou entre listar e ler: o backup continua,
                // mas sem mentir no manifesto sobre o que tem dentro.
                Err(_) => continue,
            };
            writer
                .start_file(name, options)
                .map_err(|e| format!("Failed to add {name} to the backup: {e}"))?;
            writer
                .write_all(&bytes)
                .map_err(|e| format!("Failed to add {name} to the backup: {e}"))?;
            files.push(BackupFileEntry {
                name: name.clone(),
                size_bytes: bytes.len() as u64,
            });
        }

        let manifest = BackupManifest {
            format: BACKUP_FORMAT.to_string(),
            created_at: now,
            label: label.and_then(display_backup_label),
            automatic,
            app_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            files,
        };
        let json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| format!("Failed to serialize the backup manifest: {e}"))?;
        writer
            .start_file(BACKUP_MANIFEST_NAME, options)
            .map_err(|e| format!("Failed to write the backup manifest: {e}"))?;
        writer
            .write_all(json.as_bytes())
            .map_err(|e| format!("Failed to write the backup manifest: {e}"))?;
        writer
            .finish()
            .map_err(|e| format!("Failed to finalize the backup: {e}"))?;
        Ok(manifest)
    })();

    // Falhou no meio: nem o temporário nem o nome reservado ficam para trás
    // (um zip de 0 byte apareceria na lista como backup inválido).
    let manifest = match written {
        Ok(manifest) => manifest,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp_path);
            let _ = std::fs::remove_file(&final_path);
            return Err(e);
        }
    };

    if let Err(e) = data::versions::atomic_replace(&tmp_path, &final_path) {
        let _ = std::fs::remove_file(&tmp_path);
        let _ = std::fs::remove_file(&final_path);
        return Err(format!("Failed to store the backup: {e}"));
    }

    let size_bytes = std::fs::metadata(&final_path).map(|m| m.len()).unwrap_or(0);
    let file_name = final_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();

    Ok(BackupEntry {
        id: backup_id_from_file_name(&file_name),
        file_name,
        created_at: manifest.created_at.to_rfc3339(),
        label: manifest.label.clone(),
        size_bytes,
        files: manifest.files.iter().map(|f| f.name.clone()).collect(),
        valid: true,
        automatic,
    })
}

// ---- listar --------------------------------------------------------------------------

/// Manifesto do zip. Erro aqui = backup ilegível (aparece com `valid: false`).
pub fn read_manifest_from(path: &std::path::Path) -> Result<BackupManifest, String> {
    use std::io::Read;

    let file = std::fs::File::open(path).map_err(|e| format!("Failed to open backup: {e}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Invalid backup zip: {e}"))?;
    let mut raw = String::new();
    {
        let mut entry = archive
            .by_name(BACKUP_MANIFEST_NAME)
            .map_err(|_| format!("Missing {BACKUP_MANIFEST_NAME} in the backup"))?;
        entry
            .read_to_string(&mut raw)
            .map_err(|e| format!("Failed to read {BACKUP_MANIFEST_NAME}: {e}"))?;
    }

    let manifest: BackupManifest = serde_json::from_str(&raw)
        .map_err(|e| format!("Invalid {BACKUP_MANIFEST_NAME}: {e}"))?;
    if manifest.format != BACKUP_FORMAT {
        return Err(format!("Unsupported backup format: {}", manifest.format));
    }
    Ok(manifest)
}

fn read_backup_entry(path: &std::path::Path) -> Option<BackupEntry> {
    let file_name = path.file_name().and_then(|n| n.to_str())?.to_string();
    let stem = backup_id_from_file_name(&file_name);
    let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    match read_manifest_from(path) {
        Ok(manifest) => Some(BackupEntry {
            id: stem,
            file_name,
            created_at: manifest.created_at.to_rfc3339(),
            label: manifest.label,
            size_bytes,
            files: manifest.files.into_iter().map(|f| f.name).collect(),
            valid: true,
            automatic: manifest.automatic,
        }),
        Err(_) => {
            // Sem manifesto: data e rótulo saem do nome; se nem isso der, a data
            // de modificação. O backup continua visível para o usuário decidir.
            let (created_at, label) = parse_backup_file_stem(&stem)
                .map(|(dt, label)| (dt.to_rfc3339(), label))
                .unwrap_or_else(|| {
                    let modified = std::fs::metadata(path)
                        .and_then(|m| m.modified())
                        .map(chrono::DateTime::<chrono::Utc>::from)
                        .unwrap_or_else(|_| chrono::Utc::now());
                    (modified.to_rfc3339(), None)
                });
            Some(BackupEntry {
                id: stem,
                file_name,
                created_at,
                label,
                size_bytes,
                files: Vec::new(),
                valid: false,
                // Não dá para afirmar que é automático → a retenção não encosta.
                automatic: false,
            })
        }
    }
}

/// Backups da pasta, mais recente primeiro.
pub fn list_backups_in(dir: &std::path::Path) -> Vec<BackupEntry> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut entries: Vec<BackupEntry> = read
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.to_ascii_lowercase().ends_with(".zip"))
                    .unwrap_or(false)
        })
        .filter_map(|p| read_backup_entry(&p))
        .collect();

    entries.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.id.cmp(&a.id))
    });
    entries
}

pub fn backups_usage_in(dir: &std::path::Path) -> (u64, usize) {
    let entries = list_backups_in(dir);
    (entries.iter().map(|e| e.size_bytes).sum(), entries.len())
}

pub fn delete_backup_in(dir: &std::path::Path, id: &str) -> Result<bool, String> {
    let path = resolve_backup_path(dir, id)?;
    if !path.is_file() {
        return Ok(false);
    }
    std::fs::remove_file(&path).map_err(|e| format!("Failed to delete backup: {e}"))?;
    Ok(true)
}

/// Mantém no máximo `max` backups **automáticos**; devolve os ids apagados.
/// Backup do usuário e zip ilegível nunca entram na conta nem são apagados.
pub fn prune_automatic_backups(dir: &std::path::Path, max: usize) -> Vec<String> {
    let automatic: Vec<BackupEntry> = list_backups_in(dir)
        .into_iter()
        .filter(|e| e.valid && e.automatic)
        .collect();

    let mut removed = Vec::new();
    for entry in automatic.into_iter().skip(max) {
        if delete_backup_in(dir, &entry.id).unwrap_or(false) {
            removed.push(entry.id);
        }
    }
    removed
}

// ---- restaurar -------------------------------------------------------------------------

/// Extrai por cima dos arquivos atuais, um a um e de forma atômica (tmp +
/// `atomic_replace`). Entradas fora da allowlist viram `skipped`.
pub fn restore_backup_archive(
    layout: &BackupLayout,
    zip_path: &std::path::Path,
) -> Result<RestoreOutcome, String> {
    let file =
        std::fs::File::open(zip_path).map_err(|e| format!("Failed to open backup: {e}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Invalid backup zip: {e}"))?;

    let mut outcome = RestoreOutcome::default();

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("Failed to read the backup: {e}"))?;
        let name = entry.name().to_string();
        if entry.is_dir() || name.ends_with('/') || name.ends_with('\\') {
            continue;
        }
        if name == BACKUP_MANIFEST_NAME {
            continue;
        }

        let Some(target) = resolve_entry_target(layout, &name) else {
            outcome.skipped.push(name);
            continue;
        };

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
        }

        let tmp = target.with_file_name(format!(
            "{}{TMP_SUFFIX}",
            target
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("restore")
        ));
        {
            let mut out = std::fs::File::create(&tmp)
                .map_err(|e| format!("Failed to write {}: {e}", tmp.display()))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| format!("Failed to write {}: {e}", tmp.display()))?;
        }
        if let Err(e) = data::versions::atomic_replace(&tmp, &target) {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("Failed to restore {name}: {e}"));
        }

        outcome.restored.push(name);
    }

    Ok(outcome)
}

/// Stores que são construídas no startup e não têm recarga: se o arquivo delas
/// voltou do backup, a memória continua com o conteúdo antigo até reiniciar —
/// e a próxima gravação passaria por cima do que acabou de ser restaurado.
const RELOADLESS_FILES: &[(&str, &str)] = &[
    ("RAMSettings.ini", "as configurações"),
    ("RAMTheme.ini", "o tema"),
    ("RAMThemePresets.json", "os presets de tema"),
    ("RAMScripts.json", "os scripts"),
    ("RAMAvatars.json", "os avatares salvos"),
    ("RAMVersions.json", "o catálogo de versões"),
];

/// A chave do vault (`AccountData.key`) voltou do backup?
///
/// Ela **exige** reinício, mesmo que o `AccountData.json` não tenha sido
/// restaurado: a chave desta sessão foi derivada da chave mestra que estava em
/// disco antes: se o arquivo agora guarda outra, a próxima gravação cifra o vault
/// com a chave antiga e no boot seguinte nada abre. Reiniciar é a única resposta
/// que não custa as contas do usuário.
pub fn restored_vault_key_requires_restart(restored: &[String]) -> bool {
    restored.iter().any(|name| name == "AccountData.key")
}

/// A restauração mexeu em algum arquivo que torna a **memória** velha?
///
/// Quando sim, a gravação é trancada **por padrão** e só o caminho que releu o
/// arquivo com sucesso destranca. A ordem inversa (destrancado por padrão, com
/// cada ramo lembrando de trancar) já falhou: o ramo em que `is_encrypted()`
/// **erra** (antivírus segurando o arquivo recém-substituído, permissão) só
/// empilhava aviso, e um launch depois disso regravava a lista de antes da
/// restauração — desfazendo-a em silêncio, com o arquivo ainda abrindo, então o
/// usuário só descobria pelas contas velhas. Agora um ramo novo nasce seguro.
pub fn restore_touches_accounts(restored: &[String]) -> bool {
    restored
        .iter()
        .any(|name| name == "AccountData.json" || name == "AccountData.key")
}

/// Depois da extração, a trava de gravação **continua**?
///
/// A trava é ligada **antes** de extrair, porque só dá para saber o que o zip
/// mexeu depois de ele ter mexido. Isso fecha a janela de quem **começa** a gravar
/// depois da substituição do arquivo. A de quem **já estava** gravando — um ciclo
/// de Auto Rejoin que passou pela checagem com a trava aberta e ainda estava no
/// fsync — só fecha porque `lock_writes_until_restart` espera essa gravação
/// terminar antes de voltar (ver lá). Sem essa espera, o `MoveFileExW` daquela
/// gravação caía por cima do vault recém-extraído. A trava só é solta por um
/// `load()` bem-sucedido.
///
/// `extraction_failed` mantém trancado **por não saber**: com a extração pela
/// metade, não há lista confiável de arquivos para consultar.
pub fn restore_keeps_writes_locked(extraction_failed: bool, restored: &[String]) -> bool {
    extraction_failed || restore_touches_accounts(restored)
}

/// Esta restauração pode **soltar** a trava de gravação?
///
/// `was_locked_before` é o que faltava: trancar antes de extrair obrigou a
/// destrancar quando o zip não mexeu em arquivo de conta, mas a trava **não sabe
/// de quem é**. Restaurar um backup com `AccountData.key`, não reiniciar, e depois
/// restaurar um que só tem `RAMSettings.ini` soltava a trava da primeira — com a
/// memória velha e o segredo da sessão anterior. O launch seguinte regravaria o
/// vault com o master antigo e o `.key` por cima do restaurado, matando justamente
/// o estado que a primeira restauração trouxe.
pub fn restore_may_release_lock(was_locked_before: bool, restored: &[String]) -> bool {
    !was_locked_before && !restore_touches_accounts(restored)
}

/// Motivos legíveis para reiniciar, a partir do que foi restaurado. As contas
/// ficam de fora: dependem da criptografia e são decididas em `restore_backup`.
pub fn restart_reasons_for(restored: &[String]) -> Vec<String> {
    RELOADLESS_FILES
        .iter()
        .filter(|(name, _)| restored.iter().any(|r| r == name))
        .map(|(name, what)| {
            format!("Reinicie o app para {what} ({name}) serem relidos do disco; até lá o app segue com os valores antigos em memória.")
        })
        .collect()
}

// ---- comandos ----------------------------------------------------------------------------

fn runtime_backup_layout() -> BackupLayout {
    BackupLayout {
        data_dir: data::settings::get_runtime_data_dir(),
        versions_catalog: get_versions_catalog_path(),
    }
}

#[tauri::command]
fn list_backups() -> Result<Vec<BackupEntry>, String> {
    Ok(list_backups_in(&runtime_backup_layout().backups_dir()))
}

#[tauri::command]
fn create_backup(label: Option<String>) -> Result<BackupEntry, String> {
    let layout = runtime_backup_layout();
    // Backup pedido pelo usuário nunca é automático: a retenção não o apaga.
    let entry = create_backup_in(&layout, label.as_deref(), false, chrono::Utc::now())?;
    prune_automatic_backups(&layout.backups_dir(), MAX_AUTOMATIC_BACKUPS);
    Ok(entry)
}

#[tauri::command]
fn delete_backup(id: String) -> Result<bool, String> {
    delete_backup_in(&runtime_backup_layout().backups_dir(), &id)
}

#[tauri::command]
fn backups_info() -> Result<BackupsInfo, String> {
    let layout = runtime_backup_layout();
    let dir = layout.backups_dir();
    let (total_bytes, count) = backups_usage_in(&dir);
    Ok(BackupsInfo {
        dir: dir.to_string_lossy().into_owned(),
        portable: data::settings::exe_dir()
            .join(data::settings::PORTABLE_MARKER)
            .exists(),
        total_bytes,
        count,
    })
}

#[tauri::command]
fn open_backups_folder() -> Result<(), String> {
    let dir = runtime_backup_layout().backups_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create backups folder: {e}"))?;
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&dir)
            .spawn()
            .map_err(|e| format!("Could not open folder: {}", e))?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = dir;
        Err("Available on Windows only".into())
    }
}

/// Restaura um backup por cima dos dados atuais. Antes de qualquer escrita cria
/// um backup automático do estado atual (`antes-da-restauracao`) e devolve o id
/// dele no relatório — é a saída se a restauração não era o que o usuário queria.
#[tauri::command]
fn restore_backup(
    app: tauri::AppHandle,
    accounts: tauri::State<'_, AccountStore>,
    settings: tauri::State<'_, SettingsStore>,
    id: String,
) -> Result<RestoreReport, String> {
    let layout = runtime_backup_layout();
    let dir = layout.backups_dir();
    let path = resolve_backup_path(&dir, &id)?;
    if !path.is_file() {
        return Err(format!("Backup '{id}' não encontrado."));
    }
    // Backup ilegível é recusado antes de mexer em qualquer arquivo.
    read_manifest_from(&path)
        .map_err(|e| format!("Backup inválido ({e}); nada foi alterado."))?;

    let safety = create_backup_in(
        &layout,
        Some(SAFETY_BACKUP_LABEL),
        true,
        chrono::Utc::now(),
    )
    .map_err(|e| {
        format!("Não foi possível criar o backup de segurança antes de restaurar ({e}); nada foi alterado.")
    })?;
    prune_automatic_backups(&dir, MAX_AUTOMATIC_BACKUPS);

    // **Trancado antes de extrair.** Qualquer arquivo de conta que volte do backup
    // torna a memória velha, e um único launch (`mark_used` → `save`) bastaria para
    // regravar a lista de antes por cima do que acabou de ser restaurado. Só dá
    // para saber **o que** o zip mexeu depois de ele ter mexido, então a ordem
    // segura é trancar primeiro. E `lock_writes_until_restart` só volta depois de a
    // gravação que já estiver em andamento terminar (um ciclo de Auto Rejoin no
    // meio do fsync): sem essa espera, ela publicava o vault de antes **depois** da
    // extração. Só um `load()` bem-sucedido solta a trava.
    // Lido **antes** de trancar: se já havia trava, ela é de outra restauração que
    // ainda espera o reinício, e soltá-la aqui é o mesmo que nunca tê-la ligado.
    let was_locked_before = accounts.writes_locked();
    accounts.lock_writes_until_restart("a backup is being restored");

    // `?` aqui mantém a trava, e é o que se quer: com a extração pela metade não
    // existe lista confiável de arquivos para decidir qualquer coisa.
    let outcome = restore_backup_archive(&layout, &path)?;

    let mut restart_reasons = restart_reasons_for(&outcome.restored);
    let mut accounts_reloaded = false;

    let vault_key_restored = restored_vault_key_requires_restart(&outcome.restored);
    if restore_keeps_writes_locked(false, &outcome.restored) {
        // Já está trancado; aqui só se troca o motivo pelo específico, que é o que
        // a mensagem de reinício vai explicar.
        accounts.lock_writes_until_restart(if vault_key_restored {
            "the vault key file was restored from a backup"
        } else {
            "the account file was restored from a backup"
        });
    } else if restore_may_release_lock(was_locked_before, &outcome.restored) {
        // O zip não mexeu em arquivo de conta **e** não havia trava antes: a
        // premissa da trava não vale, e manter as contas somente-leitura (com
        // pedido de reinício) seria punir quem restaurou só o tema ou as settings.
        accounts.allow_writes_after_reload();
    }

    // A chave restaurada é tratada **antes** de qualquer tentativa de recarregar:
    // com um `.key` novo em disco, o segredo desta sessão é o velho, e até a
    // migração de um vault em texto puro gravaria com o segredo errado.
    if !vault_key_restored && outcome.restored.iter().any(|n| n == "AccountData.json") {
        match accounts.is_encrypted() {
            // A chave desta sessão foi derivada da senha antiga e vive em
            // memória; reler aqui poderia travar o store com `load_failed`.
            // Já está trancado (default acima); aqui só se explica o motivo.
            Ok(true) => {
                restart_reasons.push(
                    "O AccountData.json restaurado está criptografado: reinicie o app para ele ser aberto com a chave daquele backup (senha daquele backup, ou o AccountData.key que vem no zip). Até reiniciar, as contas estão somente para leitura — o segredo desta sessão é o de antes da restauração."
                        .to_string(),
                )
            }
            Ok(false) => {
                let session_uses_password = settings
                    .get_string("General", "EncryptionMethod")
                    .eq_ignore_ascii_case("password");
                if session_uses_password {
                    restart_reasons.push(
                        "O AccountData.json restaurado está sem criptografia, mas esta sessão está com senha ativa: reinicie o app antes de mexer nas contas. Até lá as contas estão somente para leitura, senão a próxima gravação re-criptografa o arquivo."
                            .to_string(),
                    );
                } else {
                    match accounts.load() {
                        // **O único ponto que destranca**: o arquivo foi de fato
                        // relido, então a memória não está mais velha.
                        Ok(()) => {
                            accounts.allow_writes_after_reload();
                            accounts_reloaded = true;
                        }
                        Err(e) => restart_reasons.push(format!(
                            "Não foi possível reler o AccountData.json restaurado ({e}); reinicie o app."
                        )),
                    }
                }
            }
            Err(e) => restart_reasons.push(format!(
                "Não foi possível inspecionar o AccountData.json restaurado ({e}); reinicie o app."
            )),
        }
    }

    if restored_vault_key_requires_restart(&outcome.restored) {
        restart_reasons.push(
            "A chave do vault (AccountData.key) foi restaurada: reinicie o app antes de mexer nas contas. O segredo desta sessão é o de antes, e gravar com ele deixaria o arquivo sem abrir no próximo boot."
                .to_string(),
        );
        // Vale mais que o "recarreguei na hora" do caminho de texto puro.
        accounts_reloaded = false;
    }

    let report = RestoreReport {
        backup_id: id,
        safety_backup_id: Some(safety.id),
        restored: outcome.restored,
        skipped: outcome.skipped,
        accounts_reloaded,
        requires_restart: !restart_reasons.is_empty(),
        restart_reasons,
    };

    let _ = app.emit("backup-restored", &report);
    Ok(report)
}

#[cfg(test)]
mod backups_tests {
    use super::*;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    fn nanos() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    }

    /// Pasta de dados isolada por teste — nada aqui toca `get_runtime_data_dir`,
    /// que é resolvida uma vez por processo (`OnceLock`) e não pode ser trocada.
    fn temp_layout(name: &str) -> BackupLayout {
        let root = std::env::temp_dir().join(format!("ram-backups-{name}-{}", nanos()));
        let data_dir = root.join("data");
        std::fs::create_dir_all(&data_dir).expect("temp data dir");
        BackupLayout {
            data_dir: data_dir.clone(),
            versions_catalog: data_dir.join(VERSIONS_CATALOG_NAME),
        }
    }

    fn seed_data(layout: &BackupLayout) {
        std::fs::write(layout.data_dir.join("AccountData.json"), b"[{\"UserID\":1}]").unwrap();
        std::fs::write(layout.data_dir.join("RAMSettings.ini"), b"[General]\nA=1\n").unwrap();
        std::fs::create_dir_all(layout.data_dir.join("RAMThemeFonts").join("sub")).unwrap();
        std::fs::write(
            layout.data_dir.join("RAMThemeFonts").join("sub").join("a.ttf"),
            b"font-bytes",
        )
        .unwrap();
        if let Some(parent) = layout.versions_catalog.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&layout.versions_catalog, b"{\"installed\":[]}").unwrap();
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let opts = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in entries {
            writer.start_file(*name, opts).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }

    fn at(iso: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(iso)
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    // ---- saneamento do rótulo -----------------------------------------------------

    #[test]
    fn the_label_is_sanitized_into_a_safe_file_name_fragment() {
        assert_eq!(sanitize_backup_label("antes do update"), Some("antes-do-update".into()));
        // Barras, `..` e caracteres de caminho nunca sobrevivem.
        assert_eq!(sanitize_backup_label("../../etc/passwd"), Some("etc-passwd".into()));
        assert_eq!(sanitize_backup_label(".."), None);
        assert_eq!(sanitize_backup_label("C:\\Windows\\System32"), Some("C-Windows-System32".into()));
        assert_eq!(sanitize_backup_label("a/b\\c"), Some("a-b-c".into()));
        // Só pontuação/espaço → nada aproveitável.
        assert_eq!(sanitize_backup_label("   "), None);
        assert_eq!(sanitize_backup_label(""), None);
        assert_eq!(sanitize_backup_label("///"), None);
        // Unicode é preservado (é nome de arquivo, não identificador).
        assert_eq!(sanitize_backup_label("cópia número 2"), Some("cópia-número-2".into()));
        assert_eq!(sanitize_backup_label("日本語"), Some("日本語".into()));
        // O rótulo de exibição mantém o texto digitado, mas sem controles e
        // com o mesmo teto de tamanho.
        assert_eq!(display_backup_label("com rótulo"), Some("com rótulo".into()));
        assert_eq!(display_backup_label("  a	b  "), Some("a b".into()));
        assert_eq!(display_backup_label(".."), None);
        assert_eq!(display_backup_label("  "), None);
        assert_eq!(
            display_backup_label(&"b".repeat(500)).unwrap().chars().count(),
            MAX_LABEL_CHARS
        );

        // Nome gigante é truncado.
        let huge = "a".repeat(500);
        let sanitized = sanitize_backup_label(&huge).unwrap();
        assert_eq!(sanitized.chars().count(), MAX_LABEL_CHARS);
        // Controles e caracteres proibidos em nome de arquivo no Windows somem.
        assert_eq!(sanitize_backup_label("a\0b\tc*?\"<>|d"), Some("a-b-c-d".into()));
    }

    #[test]
    fn the_file_name_carries_the_timestamp_and_the_sanitized_label() {
        let now = at("2026-09-24T18:05:09Z");
        assert_eq!(backup_file_name(now, None), "backup-20260924-180509.zip");
        assert_eq!(
            backup_file_name(now, Some("antes do update")),
            "backup-20260924-180509-antes-do-update.zip"
        );
    }

    // ---- criar / listar / restaurar ------------------------------------------------

    /// Visto no app do dono (03/10/2026): apagar o backup "avançado" fechava o
    /// app. O id era cortado por posição em bytes e o corte caía no meio do "ç"
    /// — pânico do Rust, app fechado. Rótulo acentuado vem de `sanitize_backup_label`,
    /// que aceita letras unicode de propósito.
    #[test]
    fn an_accented_label_resolves_lists_and_deletes_without_panicking() {
        let layout = temp_layout("accented");
        let dir = layout.backups_dir();
        std::fs::create_dir_all(&dir).unwrap();
        for id in [
            "backup-20260925-222516-avançado",
            "backup-20260925-222516-avançado.zip",
            "backup-20261004-015512-Versão-final",
            "backup-20260925-222516-ção",
            "ção",
        ] {
            let path = resolve_backup_path(&dir, id).expect(id);
            assert!(path.to_string_lossy().ends_with(".zip"), "{id}");
        }
        assert!(parse_backup_file_stem("backup-çççççççççç").is_none());

        let entry = create_backup_in(&layout, Some("avançado"), false, at("2026-09-25T22:25:16Z")).unwrap();
        assert_eq!(entry.id, "backup-20260925-222516-avançado");
        assert!(list_backups_in(&dir).iter().any(|e| e.id == entry.id));
        assert!(delete_backup_in(&dir, &entry.id).unwrap());
        assert!(list_backups_in(&dir).is_empty());
    }

    #[test]
    fn a_backup_round_trips_through_create_list_and_restore() {
        let layout = temp_layout("roundtrip");
        seed_data(&layout);

        let entry = create_backup_in(&layout, Some("manual"), false, chrono::Utc::now())
            .expect("create_backup_in");
        assert!(entry.valid);
        assert_eq!(entry.label.as_deref(), Some("manual"));
        assert!(entry.size_bytes > 0);
        assert!(entry.files.iter().any(|f| f == "AccountData.json"));
        assert!(entry.files.iter().any(|f| f == "RAMVersions.json"));
        assert!(entry.files.iter().any(|f| f == "RAMThemeFonts/sub/a.ttf"));

        let listed = list_backups_in(&layout.backups_dir());
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, entry.id);
        assert_eq!(listed[0].files, entry.files);
        assert!(listed[0].valid);

        // O usuário apaga tudo...
        std::fs::remove_file(layout.data_dir.join("AccountData.json")).unwrap();
        std::fs::write(layout.data_dir.join("RAMSettings.ini"), b"estragado").unwrap();
        std::fs::remove_dir_all(layout.data_dir.join("RAMThemeFonts")).unwrap();
        std::fs::remove_file(&layout.versions_catalog).unwrap();

        // ...e restaura.
        let zip_path = resolve_backup_path(&layout.backups_dir(), &entry.id).unwrap();
        let outcome = restore_backup_archive(&layout, &zip_path).expect("restore");
        assert!(outcome.skipped.is_empty(), "{:?}", outcome.skipped);
        assert_eq!(outcome.restored.len(), entry.files.len());

        assert_eq!(
            std::fs::read(layout.data_dir.join("AccountData.json")).unwrap(),
            b"[{\"UserID\":1}]"
        );
        assert_eq!(
            std::fs::read(layout.data_dir.join("RAMSettings.ini")).unwrap(),
            b"[General]\nA=1\n"
        );
        assert_eq!(
            std::fs::read(layout.data_dir.join("RAMThemeFonts").join("sub").join("a.ttf")).unwrap(),
            b"font-bytes"
        );
        assert_eq!(
            std::fs::read(&layout.versions_catalog).unwrap(),
            b"{\"installed\":[]}"
        );
        // Nenhum arquivo temporário sobrou.
        let leftovers: Vec<_> = std::fs::read_dir(&layout.data_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    /// Pedido do dono (03/10/2026): os favoritos — com os links dos servidores
    /// VIP — e os recentes moravam só no `localStorage` do WebView e ficavam fora
    /// do backup. Agora moram em `RAMGameLists.json`, e restaurar os traz de volta.
    #[test]
    fn restoring_a_backup_brings_the_favorites_and_their_vip_servers_back() {
        use crate::data::game_lists::{GameLists, GameListsStore, GAME_LISTS_FILE_NAME};

        let layout = temp_layout("game-lists");
        let store = GameListsStore::new(layout.data_dir.join(GAME_LISTS_FILE_NAME));
        let saved = GameLists {
            favorites: vec![serde_json::json!({
                "placeId": 606849621,
                "name": "Jailbreak",
                "iconUrl": null,
                "addedAt": 1,
                "vipServers": [{ "id": "v1", "name": "Squad", "link": "https://www.roblox.com/share?code=abc&type=Server" }],
            })],
            recent_games: vec![serde_json::json!({ "placeId": 189707, "name": "NDS", "iconUrl": null, "lastPlayed": 2 })],
            recent_jobs: vec![serde_json::json!({ "kind": "vip", "raw": "vip:123", "placeId": 606849621, "lastUsed": 3, "userIds": [7] })],
        };
        store.save(&saved, false).unwrap();

        let entry = create_backup_in(&layout, None, false, chrono::Utc::now()).unwrap();
        assert!(
            entry.files.iter().any(|f| f == GAME_LISTS_FILE_NAME),
            "the backup must carry the game lists: {:?}",
            entry.files
        );

        // O usuário apaga o VIP (exclusão explícita) e depois restaura.
        let mut without_vip = saved.clone();
        without_vip.favorites[0]["vipServers"] = serde_json::json!([]);
        store.save(&without_vip, true).unwrap();
        assert_eq!(store.load().unwrap().unwrap().vip_server_count(), 0);

        let zip_path = resolve_backup_path(&layout.backups_dir(), &entry.id).unwrap();
        let outcome = restore_backup_archive(&layout, &zip_path).unwrap();
        assert!(outcome.restored.iter().any(|f| f == GAME_LISTS_FILE_NAME));
        assert_eq!(store.load().unwrap(), Some(saved));

        // A store relê o disco a cada leitura: nada a reiniciar por causa dela.
        assert!(restart_reasons_for(&outcome.restored).is_empty());
        assert!(resolve_entry_target(&layout, GAME_LISTS_FILE_NAME).is_some());
    }

    /// Presets de launch (ideia 13): entram no zip e voltam sem reiniciar (o
    /// store relê o disco a cada leitura).
    #[test]
    fn restoring_a_backup_brings_the_launch_presets_back_without_a_restart() {
        use crate::data::launch_presets::{LaunchPreset, LaunchPresetStore, LAUNCH_PRESETS_FILE_NAME};

        let layout = temp_layout("launch-presets");
        let store = LaunchPresetStore::new(layout.data_dir.join(LAUNCH_PRESETS_FILE_NAME));
        let saved = store
            .upsert(
                LaunchPreset {
                    name: "Farm".into(),
                    user_ids: vec![7, 8],
                    place_id: 920587237,
                    ..Default::default()
                },
                1,
            )
            .unwrap();

        let entry = create_backup_in(&layout, None, false, chrono::Utc::now()).unwrap();
        assert!(entry.files.iter().any(|f| f == LAUNCH_PRESETS_FILE_NAME), "{:?}", entry.files);

        assert!(store.delete(&saved.id).unwrap());
        let zip_path = resolve_backup_path(&layout.backups_dir(), &entry.id).unwrap();
        let outcome = restore_backup_archive(&layout, &zip_path).unwrap();
        assert!(outcome.restored.iter().any(|f| f == LAUNCH_PRESETS_FILE_NAME));
        assert_eq!(store.list().unwrap(), vec![saved]);
        assert!(restart_reasons_for(&outcome.restored).is_empty());
    }

    /// Histórico de sessões (ideia 6): entra no zip e volta sem reiniciar (o
    /// store lê o disco a cada consulta e grava por append).
    #[test]
    fn restoring_a_backup_brings_the_session_history_back_without_a_restart() {
        use crate::data::session_history::{
            HistoryEvent, HistoryEventKind, SessionHistoryStore, SESSION_HISTORY_FILE_NAME,
        };

        let layout = temp_layout("session-history");
        let store = SessionHistoryStore::new(layout.data_dir.join(SESSION_HISTORY_FILE_NAME));
        let now = chrono::Utc::now().timestamp_millis();
        let joined = HistoryEvent::new(now, 7, HistoryEventKind::Joined).at_place(Some(1), Some("job".into()));
        store.append(&[joined.clone()], now).unwrap();

        let entry = create_backup_in(&layout, None, false, chrono::Utc::now()).unwrap();
        assert!(entry.files.iter().any(|f| f == SESSION_HISTORY_FILE_NAME), "{:?}", entry.files);

        std::fs::remove_file(layout.data_dir.join(SESSION_HISTORY_FILE_NAME)).unwrap();
        let zip_path = resolve_backup_path(&layout.backups_dir(), &entry.id).unwrap();
        let outcome = restore_backup_archive(&layout, &zip_path).unwrap();
        assert!(outcome.restored.iter().any(|f| f == SESSION_HISTORY_FILE_NAME));
        assert_eq!(store.read_all(), vec![joined]);
        assert!(restart_reasons_for(&outcome.restored).is_empty());
    }

    #[test]
    fn the_catalog_is_backed_up_and_restored_even_outside_the_data_dir() {
        let mut layout = temp_layout("catalog-outside");
        let elsewhere = layout.data_dir.parent().unwrap().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        layout.versions_catalog = elsewhere.join(VERSIONS_CATALOG_NAME);
        seed_data(&layout);

        let entry =
            create_backup_in(&layout, None, false, chrono::Utc::now()).expect("create_backup_in");
        assert!(entry.files.iter().any(|f| f == "RAMVersions.json"));

        std::fs::remove_file(&layout.versions_catalog).unwrap();
        let zip_path = resolve_backup_path(&layout.backups_dir(), &entry.id).unwrap();
        restore_backup_archive(&layout, &zip_path).expect("restore");

        assert!(layout.versions_catalog.exists());
        assert!(!layout.data_dir.join(VERSIONS_CATALOG_NAME).exists());
    }

    #[test]
    fn creating_two_backups_in_the_same_second_never_overwrites_the_first() {
        let layout = temp_layout("same-second");
        seed_data(&layout);
        let now = at("2026-09-24T18:05:09Z");

        let first = create_backup_in(&layout, None, true, now).unwrap();
        let second = create_backup_in(&layout, None, true, now).unwrap();

        assert_ne!(first.id, second.id);
        assert_eq!(list_backups_in(&layout.backups_dir()).len(), 2);
    }

    #[test]
    fn listing_puts_the_newest_backup_first() {
        let layout = temp_layout("order");
        seed_data(&layout);
        create_backup_in(&layout, None, true, at("2026-01-01T00:00:00Z")).unwrap();
        create_backup_in(&layout, Some("meio"), false, at("2026-06-01T00:00:00Z")).unwrap();
        create_backup_in(&layout, None, true, at("2026-09-01T00:00:00Z")).unwrap();

        let listed = list_backups_in(&layout.backups_dir());
        let dates: Vec<&str> = listed.iter().map(|e| e.created_at.as_str()).collect();
        let mut sorted = dates.clone();
        sorted.sort();
        sorted.reverse();
        assert_eq!(dates, sorted, "mais recente primeiro");
        assert!(listed[0].created_at.starts_with("2026-09-01"));
    }

    #[test]
    fn an_empty_data_dir_still_produces_a_readable_backup() {
        let layout = temp_layout("empty");
        let entry = create_backup_in(&layout, None, true, chrono::Utc::now()).unwrap();
        assert!(entry.valid);
        assert!(entry.files.is_empty());
        assert_eq!(list_backups_in(&layout.backups_dir()).len(), 1);
    }

    // ---- manifesto -----------------------------------------------------------------

    #[test]
    fn the_manifest_is_readable_and_lists_every_file_with_its_size() {
        let layout = temp_layout("manifest");
        seed_data(&layout);
        let entry = create_backup_in(&layout, Some("com rótulo"), false, at("2026-09-24T18:05:09Z"))
            .unwrap();

        let zip_path = resolve_backup_path(&layout.backups_dir(), &entry.id).unwrap();
        let manifest = read_manifest_from(&zip_path).expect("manifest");

        assert_eq!(manifest.format, BACKUP_FORMAT);
        // O manifesto guarda o rótulo como o usuário digitou; o nome do arquivo
        // usa a versão saneada.
        assert_eq!(manifest.label.as_deref(), Some("com rótulo"));
        assert_eq!(entry.file_name, "backup-20260924-180509-com-rótulo.zip");
        assert_eq!(entry.label.as_deref(), Some("com rótulo"));
        assert!(!manifest.automatic);
        assert_eq!(manifest.created_at.format("%Y%m%d-%H%M%S").to_string(), "20260924-180509");

        let account = manifest
            .files
            .iter()
            .find(|f| f.name == "AccountData.json")
            .expect("AccountData.json no manifesto");
        assert_eq!(account.size_bytes, b"[{\"UserID\":1}]".len() as u64);
        assert!(manifest.files.iter().any(|f| f.name == "RAMThemeFonts/sub/a.ttf"));
        // O manifesto não se lista a si mesmo.
        assert!(!manifest.files.iter().any(|f| f.name == BACKUP_MANIFEST_NAME));
    }

    // ---- zip-slip ------------------------------------------------------------------

    #[test]
    fn resolve_entry_target_only_accepts_the_known_data_files() {
        let layout = temp_layout("resolve");

        assert_eq!(
            resolve_entry_target(&layout, "AccountData.json"),
            Some(layout.data_dir.join("AccountData.json"))
        );
        assert_eq!(
            resolve_entry_target(&layout, "RAMThemeFonts/sub/a.ttf"),
            Some(layout.data_dir.join("RAMThemeFonts").join("sub").join("a.ttf"))
        );
        assert_eq!(
            resolve_entry_target(&layout, "RAMVersions.json"),
            Some(layout.versions_catalog.clone())
        );

        for hostile in [
            "../evil.json",
            "..\\evil.json",
            "../../AccountData.json",
            "RAMThemeFonts/../../evil.ttf",
            "RAMThemeFonts/./a.ttf",
            "/etc/passwd",
            "/AccountData.json",
            "\\AccountData.json",
            "C:/Windows/System32/evil.dll",
            "C:\\Windows\\evil.dll",
            "RAMThemeFonts",
            "AccountData.json/extra",
            "qualquer-outro.json",
            "backup-manifest.json",
            "",
            ".",
            "..",
        ] {
            assert_eq!(resolve_entry_target(&layout, hostile), None, "{hostile:?}");
        }
    }

    #[test]
    fn restoring_a_zip_slip_archive_writes_nothing_outside_the_data_dir() {
        let layout = temp_layout("zip-slip");
        std::fs::create_dir_all(&layout.data_dir).unwrap();
        let outside = layout.data_dir.parent().unwrap().join("pwned.txt");
        let zip_path = layout.backups_dir().join("backup-20260101-000000-slip.zip");
        write_zip(
            &zip_path,
            &[
                ("../pwned.txt", b"owned"),
                ("..\\pwned.txt", b"owned"),
                ("RAMThemeFonts/../../pwned.txt", b"owned"),
                ("AccountData.json", b"[]"),
            ],
        );

        let outcome = restore_backup_archive(&layout, &zip_path).expect("restore");

        assert!(!outside.exists(), "zip-slip escapou para {}", outside.display());
        assert_eq!(outcome.restored, vec!["AccountData.json".to_string()]);
        assert_eq!(outcome.skipped.len(), 3, "{:?}", outcome.skipped);
        assert_eq!(std::fs::read(layout.data_dir.join("AccountData.json")).unwrap(), b"[]");
    }

    #[test]
    fn a_backup_id_can_never_point_outside_the_backups_folder() {
        let dir = std::env::temp_dir().join(format!("ram-backups-id-{}", nanos()));
        std::fs::create_dir_all(&dir).unwrap();

        for hostile in [
            "../AccountData",
            "..\\AccountData",
            "..",
            ".",
            "",
            "   ",
            "sub/backup-20260101-000000",
            "C:/Windows/System32/config",
            "backup\0null",
        ] {
            assert!(resolve_backup_path(&dir, hostile).is_err(), "{hostile:?}");
        }

        let ok = resolve_backup_path(&dir, "backup-20260101-000000-teste").unwrap();
        assert_eq!(ok, dir.join("backup-20260101-000000-teste.zip"));
        // O `.zip` do id também é aceito (a UI pode mandar o fileName).
        assert_eq!(
            resolve_backup_path(&dir, "backup-20260101-000000-teste.zip").unwrap(),
            ok
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- zip corrompido -------------------------------------------------------------

    #[test]
    fn a_corrupt_zip_is_listed_as_invalid_instead_of_disappearing() {
        let layout = temp_layout("corrupt");
        seed_data(&layout);
        create_backup_in(&layout, None, true, at("2026-01-01T00:00:00Z")).unwrap();

        let broken = layout.backups_dir().join("backup-20260202-101112-quebrado.zip");
        std::fs::write(&broken, b"isso nao e um zip").unwrap();
        // Zip válido, mas sem o manifesto do app.
        let no_manifest = layout.backups_dir().join("backup-20260303-101112.zip");
        write_zip(&no_manifest, &[("AccountData.json", b"[]")]);

        let listed = list_backups_in(&layout.backups_dir());
        assert_eq!(listed.len(), 3, "o inválido continua visível");

        let bad = listed.iter().find(|e| e.id == "backup-20260202-101112-quebrado").unwrap();
        assert!(!bad.valid);
        assert!(bad.size_bytes > 0);
        assert!(bad.files.is_empty());
        // Data e rótulo saem do nome do arquivo, já que o manifesto não abriu.
        assert!(bad.created_at.starts_with("2026-02-02T10:11:12"));
        assert_eq!(bad.label.as_deref(), Some("quebrado"));

        let missing_manifest = listed.iter().find(|e| e.id == "backup-20260303-101112").unwrap();
        assert!(!missing_manifest.valid);

        // Restaurar um zip sem manifesto é recusado antes de escrever qualquer coisa.
        assert!(restore_backup_archive(&layout, &broken).is_err());
    }

    #[test]
    fn deleting_removes_only_the_requested_backup() {
        let layout = temp_layout("delete");
        seed_data(&layout);
        let keep = create_backup_in(&layout, Some("fica"), false, at("2026-01-01T00:00:00Z")).unwrap();
        let drop = create_backup_in(&layout, Some("sai"), false, at("2026-02-01T00:00:00Z")).unwrap();

        assert!(delete_backup_in(&layout.backups_dir(), &drop.id).unwrap());
        assert!(!delete_backup_in(&layout.backups_dir(), &drop.id).unwrap());

        let listed = list_backups_in(&layout.backups_dir());
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, keep.id);
    }

    // ---- retenção --------------------------------------------------------------------

    #[test]
    fn retention_deletes_only_automatic_backups_and_keeps_the_newest_ones() {
        let layout = temp_layout("retention");
        seed_data(&layout);

        let mut automatic = Vec::new();
        for day in 1..=5 {
            let entry = create_backup_in(
                &layout,
                Some("antes-da-restauracao"),
                true,
                at(&format!("2026-01-0{day}T00:00:00Z")),
            )
            .unwrap();
            automatic.push(entry.id);
        }
        let manual =
            create_backup_in(&layout, Some("meu backup"), false, at("2026-01-02T12:00:00Z"))
                .unwrap();
        let manual_no_label =
            create_backup_in(&layout, None, false, at("2026-01-03T12:00:00Z")).unwrap();
        // Zip ilegível: sem manifesto não dá para afirmar que é automático → fica.
        let unknown = layout.backups_dir().join("backup-20250101-000000-misterio.zip");
        std::fs::write(&unknown, b"nao e zip").unwrap();

        let removed = prune_automatic_backups(&layout.backups_dir(), 2);

        assert_eq!(removed.len(), 3);
        for id in &automatic[..3] {
            assert!(removed.contains(id), "{id} deveria ter sido apagado");
        }
        let remaining: Vec<String> = list_backups_in(&layout.backups_dir())
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert!(remaining.contains(&manual.id), "backup do usuário nunca é apagado");
        assert!(remaining.contains(&manual_no_label.id));
        assert!(remaining.iter().any(|id| id == "backup-20250101-000000-misterio"));
        assert!(remaining.contains(&automatic[3]));
        assert!(remaining.contains(&automatic[4]));
        assert_eq!(remaining.len(), 5);
    }

    #[test]
    fn retention_does_nothing_when_there_is_room_left() {
        let layout = temp_layout("retention-room");
        seed_data(&layout);
        create_backup_in(&layout, None, true, at("2026-01-01T00:00:00Z")).unwrap();
        create_backup_in(&layout, None, true, at("2026-01-02T00:00:00Z")).unwrap();

        assert!(prune_automatic_backups(&layout.backups_dir(), MAX_AUTOMATIC_BACKUPS).is_empty());
        assert_eq!(list_backups_in(&layout.backups_dir()).len(), 2);
    }

    // ---- uso em disco ------------------------------------------------------------------

    #[test]
    fn the_usage_summary_counts_every_backup_file() {
        let layout = temp_layout("usage");
        seed_data(&layout);
        assert_eq!(backups_usage_in(&layout.backups_dir()), (0, 0));

        create_backup_in(&layout, None, true, at("2026-01-01T00:00:00Z")).unwrap();
        create_backup_in(&layout, None, true, at("2026-01-02T00:00:00Z")).unwrap();
        let (bytes, count) = backups_usage_in(&layout.backups_dir());
        assert_eq!(count, 2);
        assert!(bytes > 0);
    }

    // ---- relatório de restauração --------------------------------------------------------

    /// Restaurar a chave do vault **sempre** exige reinício, com ou sem o
    /// `AccountData.json` no mesmo zip: a chave desta sessão é a de antes, e
    /// gravar com ela por cima de um `.key` diferente deixaria o vault sem abrir
    /// no próximo boot.
    #[test]
    fn restoring_the_vault_key_always_requires_a_restart() {
        assert!(restored_vault_key_requires_restart(&[
            "AccountData.key".to_string()
        ]));
        assert!(restored_vault_key_requires_restart(&[
            "AccountData.json".to_string(),
            "AccountData.key".to_string(),
        ]));
        // Sem a chave no zip, esta regra não se aplica.
        assert!(!restored_vault_key_requires_restart(&[
            "AccountData.json".to_string()
        ]));
        assert!(!restored_vault_key_requires_restart(&[]));
    }

    /// **Quebra 2.** Trancar a gravação passou a ser o **default** de qualquer
    /// restauração que mexa em arquivo de conta, porque a ordem inversa já falhou:
    /// o ramo em que `is_encrypted()` **erra** só empilhava aviso, e um launch
    /// depois disso regravava a lista de antes da restauração — desfazendo-a em
    /// silêncio, com o arquivo ainda abrindo. Agora ramo novo nasce seguro.
    #[test]
    fn any_restore_touching_the_account_files_counts_as_stale_memory() {
        assert!(restore_touches_accounts(&["AccountData.json".to_string()]));
        assert!(restore_touches_accounts(&["AccountData.key".to_string()]));
        assert!(restore_touches_accounts(&[
            "RAMSettings.ini".to_string(),
            "AccountData.key".to_string(),
        ]));
        // O que não é arquivo de conta não torna a memória de contas velha.
        assert!(!restore_touches_accounts(&["RAMSettings.ini".to_string()]));
        assert!(!restore_touches_accounts(&[
            "RAMThemeFonts/sub/a.ttf".to_string()
        ]));
        assert!(!restore_touches_accounts(&[]));

        // E tem que cobrir **tudo** que o vault usa: se um arquivo novo entrar no
        // par vault+chave, ele entra aqui também.
        for name in ["AccountData.json", "AccountData.key"] {
            assert!(
                data::settings::DATA_FILES.contains(&name),
                "{name} saiu do conjunto de backup"
            );
            assert!(restore_touches_accounts(&[name.to_string()]));
        }
    }

    /// **N4.** A trava é ligada **antes** da extração, porque só depois de extrair
    /// se sabe o que o zip mexeu — e nesse intervalo um ciclo de Auto Rejoin
    /// gravaria com o segredo antigo. Isto trava a regra de quando ela **continua**
    /// ligada depois, e o caso que importa é o primeiro: extração pela metade
    /// mantém trancado **por não saber**.
    /// **Fail-open que a inversão "trancar antes de extrair" criou.** O `else` que
    /// destranca quando o zip não mexeu em arquivo de conta não sabia **de quem
    /// era** a trava.
    ///
    /// Sequência real: restaura um backup com `AccountData.key` → trancado,
    /// "reinicie"; o dono **não** reinicia; restaura outro backup que só tem
    /// `RAMSettings.ini` → destrancava, com a memória velha e o segredo da sessão
    /// anterior. Um launch depois regrava o vault com o master antigo e o
    /// `refresh_key_file` regrava o `.key` por cima do restaurado: o estado
    /// restaurado morre. É exatamente o que o latch existe para impedir.
    #[test]
    fn a_restore_never_releases_a_lock_that_was_already_there() {
        // Nada trancado antes e nenhum arquivo de conta no zip: pode destrancar —
        // senão quem restaurou só o tema ficaria somente-leitura à toa.
        assert!(restore_may_release_lock(
            false,
            &["RAMSettings.ini".to_string()]
        ));
        assert!(restore_may_release_lock(false, &[]));

        // **Já estava trancado**: a trava é de outra restauração, e destrancá-la
        // aqui mata o estado restaurado por ela.
        assert!(!restore_may_release_lock(
            true,
            &["RAMSettings.ini".to_string()]
        ));
        assert!(!restore_may_release_lock(true, &[]));

        // E o zip que mexe em arquivo de conta nunca destranca, tenha ou não
        // trava anterior.
        for was_locked in [false, true] {
            assert!(!restore_may_release_lock(
                was_locked,
                &["AccountData.json".to_string()]
            ));
            assert!(!restore_may_release_lock(
                was_locked,
                &["AccountData.key".to_string()]
            ));
        }
    }

    #[test]
    fn a_failed_extraction_keeps_writes_locked_even_with_nothing_listed() {
        assert!(restore_keeps_writes_locked(true, &[]));
        assert!(restore_keeps_writes_locked(
            true,
            &["RAMSettings.ini".to_string()]
        ));

        // Extração ok e arquivo de conta mexido: continua trancado.
        assert!(restore_keeps_writes_locked(
            false,
            &["AccountData.json".to_string()]
        ));
        assert!(restore_keeps_writes_locked(
            false,
            &["AccountData.key".to_string()]
        ));

        // Extração ok e nenhum arquivo de conta: solta, senão quem restaurou só o
        // tema ficaria com as contas somente-leitura pedindo reinício.
        assert!(!restore_keeps_writes_locked(
            false,
            &["RAMSettings.ini".to_string(), "RAMTheme.ini".to_string()]
        ));
        assert!(!restore_keeps_writes_locked(false, &[]));
    }

    /// **O guardião de verdade do `.key` no backup.** O teste acima trava a
    /// função; este trava o comportamento: um vault cifrado de verdade, com a
    /// chave dele, tem que **sair no zip** e **voltar abrindo** depois de um
    /// "reinício" (store novo). Sem a chave no zip isso é perda total, e uma
    /// asserção sobre a constante `DATA_FILES` não pegaria — ela travaria a
    /// constante e mais nada.
    #[test]
    fn a_backup_carries_the_vault_key_and_the_restored_pair_opens_again() {
        let layout = temp_layout("vault-roundtrip");
        seed_data(&layout);

        // Um vault cifrado real, criado pelo próprio AccountStore.
        let vault_path = layout.data_dir.join("AccountData.json");
        let key_path = layout.data_dir.join("AccountData.key");
        {
            let store = crate::data::accounts::AccountStore::new(vault_path.clone());
            store.load().expect("abrir vault novo");
            store
                .add(crate::data::accounts::Account::new(
                    "cookie".to_string(),
                    "NoBackup".to_string(),
                    4242,
                ))
                .expect("adicionar conta");
        }
        assert!(key_path.exists(), "o vault tinha que ter criado o .key");
        assert!(crate::data::crypto::is_encrypted(
            &std::fs::read(&vault_path).unwrap()
        ));

        let entry =
            create_backup_in(&layout, Some("com-chave"), false, at("2026-02-01T00:00:00Z"))
                .expect("criar backup");

        // 1. O zip leva os dois arquivos.
        assert!(
            entry.files.iter().any(|f| f == "AccountData.json"),
            "o zip não levou o vault"
        );
        assert!(
            entry.files.iter().any(|f| f == "AccountData.key"),
            "o zip não levou a chave — vault cifrado sem a chave dele não restaura"
        );

        // 2. Desastre: os dois arquivos somem.
        std::fs::remove_file(&vault_path).unwrap();
        std::fs::remove_file(&key_path).unwrap();

        let archive = resolve_backup_path(&layout.backups_dir(), &entry.id).unwrap();
        let outcome = restore_backup_archive(&layout, &archive).expect("restaurar");
        assert!(outcome.restored.iter().any(|n| n == "AccountData.json"));
        assert!(outcome.restored.iter().any(|n| n == "AccountData.key"));

        // 3. Depois do "reinício", o par restaurado abre e a conta está lá.
        let reopened = crate::data::accounts::AccountStore::new(vault_path.clone());
        reopened
            .load()
            .expect("o par vault+chave restaurado tem que abrir");
        assert!(!reopened.needs_password().unwrap());
        // `seed_data` deixa um `AccountData.json` em texto puro, então o store
        // migrou aquela conta antes de receber a nossa: as duas têm que voltar.
        let ids: Vec<i64> = reopened
            .get_all()
            .unwrap()
            .iter()
            .map(|a| a.user_id)
            .collect();
        assert!(ids.contains(&4242), "a conta gravada sumiu: {ids:?}");
        assert!(ids.contains(&1), "a conta migrada sumiu: {ids:?}");
    }

    #[test]
    fn the_restart_report_is_honest_about_what_could_not_be_reloaded() {
        let restored = vec![
            "AccountData.json".to_string(),
            "RAMSettings.ini".to_string(),
            "RAMScripts.json".to_string(),
            "RAMVersions.json".to_string(),
        ];
        let reasons = restart_reasons_for(&restored);
        assert!(reasons.iter().any(|r| r.contains("RAMSettings.ini")));
        assert!(reasons.iter().any(|r| r.contains("RAMScripts.json")));
        assert!(reasons.iter().any(|r| r.contains("RAMVersions.json")));
        // Contas são tratadas à parte (dependem da criptografia), não entram aqui.
        assert!(!reasons.iter().any(|r| r.contains("AccountData.json")));

        // Só fontes → nada exige reinício (são lidas do disco sob demanda).
        assert!(restart_reasons_for(&["RAMThemeFonts/a.ttf".to_string()]).is_empty());
        assert!(restart_reasons_for(&[]).is_empty());
    }

    #[test]
    fn the_entry_and_the_report_serialize_in_camel_case_for_the_ui() {
        let entry = BackupEntry {
            id: "backup-20260101-000000".into(),
            file_name: "backup-20260101-000000.zip".into(),
            created_at: "2026-01-01T00:00:00+00:00".into(),
            label: Some("x".into()),
            size_bytes: 10,
            files: vec!["AccountData.json".into()],
            valid: true,
            automatic: false,
        };
        let json = serde_json::to_value(&entry).unwrap();
        for key in ["id", "fileName", "createdAt", "label", "sizeBytes", "files", "valid"] {
            assert!(json.get(key).is_some(), "falta {key}: {json}");
        }

        let report = RestoreReport {
            backup_id: "a".into(),
            safety_backup_id: Some("b".into()),
            restored: vec!["AccountData.json".into()],
            skipped: vec![],
            accounts_reloaded: false,
            requires_restart: true,
            restart_reasons: vec!["motivo".into()],
        };
        let json = serde_json::to_value(&report).unwrap();
        for key in [
            "backupId",
            "safetyBackupId",
            "restored",
            "skipped",
            "accountsReloaded",
            "requiresRestart",
            "restartReasons",
        ] {
            assert!(json.get(key).is_some(), "falta {key}: {json}");
        }

        let info = BackupsInfo {
            dir: "C:/x".into(),
            portable: false,
            total_bytes: 1,
            count: 1,
        };
        let json = serde_json::to_value(&info).unwrap();
        for key in ["dir", "portable", "totalBytes", "count"] {
            assert!(json.get(key).is_some(), "falta {key}: {json}");
        }
    }

    #[test]
    fn the_backups_dir_hangs_off_the_data_dir() {
        let layout = BackupLayout {
            data_dir: PathBuf::from("C:/dados"),
            versions_catalog: PathBuf::from("C:/dados/RAMVersions.json"),
        };
        assert_eq!(layout.backups_dir(), PathBuf::from("C:/dados").join(BACKUPS_DIR_NAME));
        // A pasta de backups não é ela mesma um alvo de restauração.
        assert_eq!(resolve_entry_target(&layout, "backups/x.zip"), None);
    }
}
