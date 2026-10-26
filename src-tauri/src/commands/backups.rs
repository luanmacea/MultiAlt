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
    /// A versão do app que fez o backup ("v2.4.0"), quando o manifesto diz.
    pub app_version: Option<String>,
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
    create_backup_with_version_in(layout, label, automatic, now, None)
}

/// A versão que o backup grava: a que a tela mostra no título (vem do
/// frontend), ou a do pacote (`tauri.conf.json`, que a release preenche).
fn backup_app_version(from_screen: Option<String>, package_version: &str) -> String {
    match from_screen.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()) {
        Some(version) => version,
        None => format!("v{package_version}"),
    }
}

/// `create_backup_in` gravando a versão do app no manifesto (`app_version`).
pub fn create_backup_with_version_in(
    layout: &BackupLayout,
    label: Option<&str>,
    automatic: bool,
    now: chrono::DateTime<chrono::Utc>,
    app_version: Option<&str>,
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
            app_version: app_version.map(str::to_string),
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
        app_version: manifest.app_version.clone(),
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
            app_version: manifest.app_version,
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
                app_version: None,
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
fn create_backup(
    app: tauri::AppHandle,
    label: Option<String>,
    app_version: Option<String>,
) -> Result<BackupEntry, String> {
    let layout = runtime_backup_layout();
    let version = backup_app_version(app_version, &app.package_info().version.to_string());
    // Backup pedido pelo usuário nunca é automático: a retenção não o apaga.
    let entry = create_backup_with_version_in(&layout, label.as_deref(), false, chrono::Utc::now(), Some(&version))?;
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
    app_version: Option<String>,
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

    let version = backup_app_version(app_version, &app.package_info().version.to_string());
    let safety = create_backup_with_version_in(
        &layout,
        Some(SAFETY_BACKUP_LABEL),
        true,
        chrono::Utc::now(),
        Some(&version),
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
    // Os presets podem ter voltado com o backup: o menu da bandeja os lista.
    refresh_tray_menu(&app);

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
