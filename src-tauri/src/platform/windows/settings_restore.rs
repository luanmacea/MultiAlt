// Devolver as configurações do Roblox ao fechar (ideia 21). Ver
// docs/features/performance.md ("Devolver as configurações do Roblox").
//
// O launch grava FPS, volume, qualidade, janela e FastFlags nos arquivos do
// Roblox (`GlobalBasicSettings_13.xml` e o `ClientAppSettings.json` da versão).
// Esses arquivos são do Roblox, não do app: o jogo aberto pelo site lê os
// mesmos. Com `General.RestoreRobloxSettingsOnExit` ligado, o app anota, por
// propriedade, o valor que estava lá **antes** da primeira mudança e o que ele
// escreveu por último. Ao fechar o app (sem cliente que ele abriu rodando), cada
// propriedade que ainda tem o valor do app volta ao do usuário; a que mudou
// depois (o jogador mexeu dentro do jogo) fica como está.
//
// Além disso, antes da primeira mudança de cada arquivo, uma cópia inteira dele
// vai para `RobloxSettingsBackup/` na pasta de dados — para recuperar à mão se
// algo der errado. Nada aqui fecha cliente.

/// Que arquivo é: decide como as propriedades são lidas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SettingsFileKind {
    /// `GlobalBasicSettings_13.xml`: as propriedades de `UserGameSettings`.
    Xml,
    /// `ClientAppSettings.json`: as chaves do topo do objeto.
    Json,
}

impl SettingsFileKind {
    pub fn of(path: &std::path::Path) -> Self {
        if path
            .extension()
            .map(|e| e.eq_ignore_ascii_case("xml"))
            .unwrap_or(false)
        {
            Self::Xml
        } else {
            Self::Json
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct RestoreKey {
    /// Como estava antes da primeira mudança do app. `None` = não existia.
    original: Option<String>,
    /// O que o app escreveu por último. `None` = o app tirou.
    written: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct RestoreFile {
    kind: SettingsFileKind,
    /// O arquivo existia antes da primeira mudança?
    existed: bool,
    /// Nome da cópia em `RobloxSettingsBackup/` (sem cópia se não existia).
    backup: Option<String>,
    keys: std::collections::BTreeMap<String, RestoreKey>,
}

/// O que o app mudou nos arquivos do Roblox desde a última devolução.
/// Persistido em `RobloxSettingsRestore.json`, na pasta de dados: sobrevive a
/// fechar o app com cliente aberto (aí a devolução fica para o próximo fechar).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RestoreJournal {
    files: std::collections::BTreeMap<String, RestoreFile>,
}

/// O que fazer com um arquivo na devolução.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreAction {
    /// Nada a devolver (o jogador mudou tudo depois, ou o arquivo sumiu).
    Keep,
    Write(String),
    /// O app criou o arquivo e nada do usuário ficou nele.
    Delete,
}

/// As propriedades do XML que o launch escreve (`rewrite_global_basic_settings`).
fn managed_xml_props() -> impl Iterator<Item = &'static str> {
    FPS_PROPS
        .iter()
        .chain(VOLUME_PROPS)
        .chain(GRAPHICS_PROPS)
        .chain(WINDOW_PROPS)
        .copied()
}

/// Propriedade → texto, como o arquivo está agora.
fn settings_keys(
    kind: SettingsFileKind,
    content: Option<&str>,
) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    let Some(content) = content else {
        return out;
    };
    match kind {
        SettingsFileKind::Xml => {
            if let Some((start, end)) = find_user_game_settings_properties_range(content) {
                let props = &content[start..end];
                for name in managed_xml_props() {
                    if let Some(element) = property_element(props, name) {
                        out.insert(name.to_string(), element);
                    }
                }
            }
        }
        SettingsFileKind::Json => {
            if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(content) {
                for (key, value) in map {
                    out.insert(key, value.to_string());
                }
            }
        }
    }
    out
}

impl RestoreJournal {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Já há anotação deste arquivo? (Sem anotação, a mudança é a primeira e
    /// pede a cópia de segurança.)
    pub fn knows(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    /// Anota uma escrita do app: `before` e `after` são o arquivo inteiro
    /// (`None` = não existia). Só as propriedades que mudaram entram; a que
    /// já estava anotada guarda o valor de antes da **primeira** mudança.
    pub fn record(
        &mut self,
        path: &str,
        kind: SettingsFileKind,
        before: Option<&str>,
        after: Option<&str>,
        backup: Option<String>,
    ) {
        let old = settings_keys(kind, before);
        let new = settings_keys(kind, after);
        let names: std::collections::BTreeSet<&String> = old.keys().chain(new.keys()).collect();
        let changed: Vec<String> = names
            .into_iter()
            .filter(|name| old.get(*name) != new.get(*name))
            .cloned()
            .collect();
        if changed.is_empty() {
            return;
        }
        let file = self.files.entry(path.to_string()).or_insert_with(|| RestoreFile {
            kind,
            existed: before.is_some(),
            backup,
            keys: Default::default(),
        });
        for name in changed {
            let written = new.get(&name).cloned();
            let entry = file.keys.entry(name.clone()).or_insert_with(|| RestoreKey {
                original: old.get(&name).cloned(),
                written: written.clone(),
            });
            entry.written = written;
            // O app pôs de volta o valor do usuário: não há o que devolver.
            if entry.written == entry.original {
                file.keys.remove(&name);
            }
        }
        if file.keys.is_empty() && file.existed {
            self.files.remove(path);
        }
    }

    /// Os arquivos anotados, com o tipo de cada um.
    pub fn paths(&self) -> Vec<(String, SettingsFileKind)> {
        self.files
            .iter()
            .map(|(path, file)| (path.clone(), file.kind))
            .collect()
    }

    /// O que a devolução faz com o arquivo `path`, que agora tem `current`.
    /// Propriedade que ainda tem o valor do app volta à do usuário; a que o
    /// jogador mudou depois fica.
    pub fn restore_action(&self, path: &str, current: Option<&str>) -> RestoreAction {
        let Some(file) = self.files.get(path) else {
            return RestoreAction::Keep;
        };
        let now = settings_keys(file.kind, current);
        match file.kind {
            SettingsFileKind::Xml => {
                let Some(xml) = current else {
                    return RestoreAction::Keep;
                };
                let Some((start, end)) = find_user_game_settings_properties_range(xml) else {
                    return RestoreAction::Keep;
                };
                let mut props = xml[start..end].to_string();
                let mut changed = false;
                for (name, key) in &file.keys {
                    if now.get(name) == key.written.as_ref() {
                        restore_property(&mut props, name, key.original.as_deref());
                        changed = true;
                    }
                }
                if !changed {
                    return RestoreAction::Keep;
                }
                let mut out = xml[..start].to_string();
                out.push_str(&props);
                out.push_str(&xml[end..]);
                RestoreAction::Write(out)
            }
            SettingsFileKind::Json => {
                let mut value: serde_json::Value = current
                    .and_then(|raw| serde_json::from_str(raw).ok())
                    .filter(|v: &serde_json::Value| v.is_object())
                    .unwrap_or_else(|| serde_json::json!({}));
                let mut changed = false;
                if let Some(map) = value.as_object_mut() {
                    for (name, key) in &file.keys {
                        if now.get(name) != key.written.as_ref() {
                            continue;
                        }
                        changed = true;
                        match key.original.as_deref().and_then(|raw| serde_json::from_str(raw).ok()) {
                            Some(original) => {
                                map.insert(name.clone(), original);
                            }
                            None => {
                                map.remove(name);
                            }
                        }
                    }
                }
                if !changed {
                    return RestoreAction::Keep;
                }
                let empty = value.as_object().map(|m| m.is_empty()).unwrap_or(false);
                if !file.existed && empty {
                    return RestoreAction::Delete;
                }
                RestoreAction::Write(serde_json::to_string(&value).unwrap_or_default())
            }
        }
    }
}

// ── arquivos ────────────────────────────────────────────────────────────────

/// Os arquivos do Roblox que uma abertura pode escrever: o XML global e o
/// `ClientAppSettings.json` da pasta da versão (`None` = produção).
pub fn roblox_settings_files(base_path: Option<&str>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(xml) = get_global_basic_settings_file() {
        out.push(xml);
    }
    let json = match base_path {
        Some(base) => get_client_settings_file_in(base),
        None => get_client_settings_file(),
    };
    if let Ok(json) = json {
        out.push(json);
    }
    out
}

/// Os arquivos como estavam antes de uma escrita.
pub struct RobloxSettingsSnapshot {
    files: Vec<(PathBuf, Option<String>)>,
}

pub fn snapshot_roblox_settings(paths: Vec<PathBuf>) -> RobloxSettingsSnapshot {
    RobloxSettingsSnapshot {
        files: paths
            .into_iter()
            .map(|path| {
                let content = std::fs::read_to_string(&path).ok();
                (path, content)
            })
            .collect(),
    }
}

const RESTORE_JOURNAL_FILE: &str = "RobloxSettingsRestore.json";
const RESTORE_BACKUP_DIR: &str = "RobloxSettingsBackup";

fn load_restore_journal(dir: &std::path::Path) -> RestoreJournal {
    std::fs::read_to_string(dir.join(RESTORE_JOURNAL_FILE))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_restore_journal(dir: &std::path::Path, journal: &RestoreJournal) {
    let path = dir.join(RESTORE_JOURNAL_FILE);
    if journal.is_empty() {
        let _ = std::fs::remove_file(path);
    } else if let Ok(raw) = serde_json::to_string_pretty(journal) {
        let _ = std::fs::write(path, raw);
    }
}

/// Anota o que mudou desde `snapshot`, em `dir` (a pasta de dados). A primeira
/// mudança de cada arquivo deixa a cópia inteira dele em `RobloxSettingsBackup/`.
pub fn record_roblox_settings_change_in(dir: &std::path::Path, snapshot: RobloxSettingsSnapshot) {
    let mut journal = load_restore_journal(dir);
    let fresh = journal.is_empty();
    let backup_dir = dir.join(RESTORE_BACKUP_DIR);
    for (index, (path, before)) in snapshot.files.into_iter().enumerate() {
        let after = std::fs::read_to_string(&path).ok();
        if after == before {
            continue;
        }
        let key = path.to_string_lossy().to_string();
        let backup = if journal.knows(&key) {
            None
        } else if let Some(content) = before.as_deref() {
            // Um ciclo novo começa limpo: cópias de um ciclo já devolvido saem.
            if fresh && index == 0 {
                let _ = std::fs::remove_dir_all(&backup_dir);
            }
            let _ = std::fs::create_dir_all(&backup_dir);
            let name = format!(
                "{}-{}",
                journal.paths().len(),
                path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
            );
            std::fs::write(backup_dir.join(&name), content).ok().map(|_| name)
        } else {
            None
        };
        journal.record(&key, SettingsFileKind::of(&path), before.as_deref(), after.as_deref(), backup);
    }
    save_restore_journal(dir, &journal);
}

/// Há algo a devolver em `dir`?
pub fn has_pending_roblox_settings_restore_in(dir: &std::path::Path) -> bool {
    !load_restore_journal(dir).is_empty()
}

/// Esquece o que estava anotado (a opção foi desligada).
pub fn discard_roblox_settings_restore_in(dir: &std::path::Path) {
    let _ = std::fs::remove_file(dir.join(RESTORE_JOURNAL_FILE));
}

/// Devolve as configurações do usuário. Devolve quantos arquivos mudaram. A
/// anotação só sai depois de devolver tudo; arquivo que não pôde ser gravado
/// fica anotado para a próxima vez.
pub fn restore_roblox_settings_in(dir: &std::path::Path) -> usize {
    let mut journal = load_restore_journal(dir);
    let mut restored = 0;
    let mut failed = std::collections::BTreeSet::new();
    for (path, _) in journal.paths() {
        let current = std::fs::read_to_string(&path).ok();
        let ok = match journal.restore_action(&path, current.as_deref()) {
            RestoreAction::Keep => true,
            RestoreAction::Write(content) => {
                let ok = std::fs::write(&path, content).is_ok();
                if ok {
                    restored += 1;
                }
                ok
            }
            RestoreAction::Delete => {
                let ok = std::fs::remove_file(&path).is_ok();
                if ok {
                    restored += 1;
                }
                ok
            }
        };
        if !ok {
            failed.insert(path);
        }
    }
    journal.files.retain(|path, _| failed.contains(path));
    save_restore_journal(dir, &journal);
    restored
}

fn settings_restore_dir() -> PathBuf {
    crate::data::settings::get_runtime_data_dir()
}

pub fn record_roblox_settings_change(snapshot: RobloxSettingsSnapshot) {
    record_roblox_settings_change_in(&settings_restore_dir(), snapshot);
}

pub fn has_pending_roblox_settings_restore() -> bool {
    has_pending_roblox_settings_restore_in(&settings_restore_dir())
}

pub fn discard_roblox_settings_restore() {
    discard_roblox_settings_restore_in(&settings_restore_dir());
}

pub fn restore_roblox_settings() -> usize {
    restore_roblox_settings_in(&settings_restore_dir())
}
