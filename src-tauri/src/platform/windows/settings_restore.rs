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

/// O texto de dentro de `<tag name="…">texto</tag>`, sem espaços sobrando.
fn element_text(element: &str) -> String {
    let inner = match (element.find('>'), element.rfind('<')) {
        (Some(open), Some(close)) if close > open => &element[open + 1..close],
        _ => element,
    };
    inner.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Dois elementos da mesma propriedade guardam o mesmo valor? Pelo número,
/// não pelo texto: o Roblox regrava o arquivo inteiro ao fechar, no formato
/// dele (`0.300000` vira `0.300000012`, `1.000000` vira `1`).
fn same_value(a: &str, b: &str) -> bool {
    let (a, b) = (element_text(a), element_text(b));
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => (x - y).abs() <= 1e-4 * x.abs().max(1.0),
        _ => a == b,
    }
}

/// A propriedade ainda tem o valor que o app pôs (então volta à do usuário)?
/// A janela (`WINDOW_PROPS`) é sempre do app enquanto estiver anotada: o
/// Roblox a regrava com o tamanho real ao fechar, e a grade do app mexe nela.
fn still_the_apps(name: &str, now: Option<&String>, written: Option<&String>) -> bool {
    if WINDOW_PROPS.contains(&name) {
        return true;
    }
    match (now, written) {
        (Some(now), Some(written)) => same_value(now, written),
        (None, None) => true,
        _ => false,
    }
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
                    if still_the_apps(name, now.get(name), key.written.as_ref()) {
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
    LAST_SETTINGS_RECORD.store(unix_now_secs(), std::sync::atomic::Ordering::SeqCst);
    record_roblox_settings_change_in(&settings_restore_dir(), snapshot);
}

/// Quando o launch escreveu nos arquivos pela última vez (segundos Unix).
static LAST_SETTINGS_RECORD: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Folga depois da última escrita do launch: o patch é o último passo antes
/// do spawn, mas o processo do cliente leva uns segundos para aparecer, e
/// devolver nesse meio deixaria o cliente novo sem a otimização.
pub const RESTORE_WATCH_GRACE_SECS: u64 = 60;

fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Segundos desde a última escrita do launch (`u64::MAX` = nenhuma nesta
/// execução do app).
pub fn secs_since_last_settings_record() -> u64 {
    match LAST_SETTINGS_RECORD.load(std::sync::atomic::Ordering::SeqCst) {
        0 => u64::MAX,
        at => unix_now_secs().saturating_sub(at),
    }
}

/// Com o app aberto: é hora de devolver? Só com a opção ligada, algo anotado,
/// **nenhum** Roblox rodando (nem o aberto pelo site: ele regravaria os valores
/// que tem na memória ao fechar) e a folga do último launch vencida.
///
/// Antes a devolução só acontecia ao fechar o app: quem fechava o jogo e
/// abria pelo site com o MultiAlt aberto ficava com a otimização (13/10/2026).
pub fn restore_watch_due(enabled: bool, pending: bool, roblox_running: bool, secs_since_record: u64) -> bool {
    enabled && pending && !roblox_running && secs_since_record >= RESTORE_WATCH_GRACE_SECS
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

#[cfg(test)]
mod win_settings_restore_tests {
    use super::*;

    const XML_USER: &str = "<roblox><Item class=\"UserGameSettings\"><Properties>\n\t\t\t<int name=\"FramerateCap\">60</int>\n\t\t\t<float name=\"MasterVolume\">0.800000</float>\n\t\t\t<string name=\"Other\">x</string>\n</Properties></Item></roblox>";

    fn xml_with(fps: &str, volume: Option<&str>) -> String {
        let volume = volume
            .map(|v| format!("\t\t\t<float name=\"MasterVolume\">{v}</float>\n"))
            .unwrap_or_default();
        format!(
            "<roblox><Item class=\"UserGameSettings\"><Properties>\n\t\t\t<int name=\"FramerateCap\">{fps}</int>\n{volume}\t\t\t<string name=\"Other\">x</string>\n</Properties></Item></roblox>"
        )
    }

    #[test]
    fn what_the_app_wrote_goes_back_to_the_users_value() {
        let mut journal = RestoreJournal::default();
        let launched = xml_with("240", Some("0.300000"));
        journal.record("g.xml", SettingsFileKind::Xml, Some(XML_USER), Some(&launched), None);
        assert_eq!(
            journal.restore_action("g.xml", Some(&launched)),
            RestoreAction::Write(XML_USER.to_string())
        );
    }

    #[test]
    fn a_value_the_player_changed_in_game_stays() {
        let mut journal = RestoreJournal::default();
        let launched = xml_with("240", Some("0.300000"));
        journal.record("g.xml", SettingsFileKind::Xml, Some(XML_USER), Some(&launched), None);
        // O jogador mudou o volume dentro do jogo; o FPS ainda é o do app.
        let played = xml_with("240", Some("0.500000"));
        assert_eq!(
            journal.restore_action("g.xml", Some(&played)),
            RestoreAction::Write(xml_with("60", Some("0.500000")))
        );
        // Mudou tudo: nada a devolver.
        assert_eq!(
            journal.restore_action("g.xml", Some(&xml_with("30", Some("0.500000")))),
            RestoreAction::Keep
        );
    }

    /// Bug de 13/10/2026: o app grava `0.300000`, o Roblox, ao fechar, regrava
    /// o arquivo inteiro no formato dele (`0.300000012`). A comparação por
    /// texto achava que "o jogador mudou" e o volume otimizado ficava.
    #[test]
    fn the_value_roblox_rewrote_in_its_own_format_still_goes_back() {
        let mut journal = RestoreJournal::default();
        let launched = xml_with("240", Some("0.300000"));
        journal.record("g.xml", SettingsFileKind::Xml, Some(XML_USER), Some(&launched), None);
        let rewritten = xml_with("240", Some("0.300000012"));
        assert_eq!(
            journal.restore_action("g.xml", Some(&rewritten)),
            RestoreAction::Write(XML_USER.to_string())
        );
    }

    /// A janela o Roblox regrava com o tamanho real ao fechar (a grade do app
    /// muda a janela): ela é do app enquanto estiver anotada e sempre volta.
    #[test]
    fn the_window_goes_back_even_after_roblox_rewrote_it() {
        let user = "<roblox><Item class=\"UserGameSettings\"><Properties>\n\t\t\t<bool name=\"StartMaximized\">true</bool>\n</Properties></Item></roblox>";
        let launched = user.replace(">true<", ">false<");
        let mut journal = RestoreJournal::default();
        journal.record("g.xml", SettingsFileKind::Xml, Some(user), Some(&launched), None);
        // O Roblox fechou maximizado pela grade? Não: regravou outra coisa.
        let rewritten = user.replace("<bool name=\"StartMaximized\">true</bool>", "<bool name=\"StartMaximized\">false</bool>\n\t\t\t<string name=\"Other\">y</string>");
        assert_eq!(
            journal.restore_action("g.xml", Some(&rewritten)),
            RestoreAction::Write(user.replace("<bool name=\"StartMaximized\">true</bool>", "<bool name=\"StartMaximized\">true</bool>\n\t\t\t<string name=\"Other\">y</string>"))
        );
        let size_user = "<roblox><Item class=\"UserGameSettings\"><Properties>\n\t\t\t<Vector2 name=\"StartScreenSize\"><X>1280</X><Y>720</Y></Vector2>\n</Properties></Item></roblox>";
        let size_app = size_user.replace("1280", "640").replace("720", "360");
        let size_roblox = size_user.replace("1280", "656").replace("720", "399");
        let mut journal = RestoreJournal::default();
        journal.record("s.xml", SettingsFileKind::Xml, Some(size_user), Some(&size_app), None);
        assert_eq!(
            journal.restore_action("s.xml", Some(&size_roblox)),
            RestoreAction::Write(size_user.to_string())
        );
    }

    #[test]
    fn same_value_reads_numbers_not_text() {
        assert!(same_value("<float name=\"MasterVolume\">0.300000</float>", "<float name=\"MasterVolume\">0.300000012</float>"));
        assert!(same_value("<float name=\"MasterVolume\">1.000000</float>", "<float name=\"MasterVolume\">1</float>"));
        assert!(!same_value("<float name=\"MasterVolume\">0.3</float>", "<float name=\"MasterVolume\">0.5</float>"));
        assert!(same_value("<int name=\"FramerateCap\">240</int>", "<int name=\"FramerateCap\"> 240 </int>"));
        assert!(!same_value("<int name=\"FramerateCap\">240</int>", "<int name=\"FramerateCap\">60</int>"));
        assert!(same_value("<bool name=\"MaxQualityEnabled\">false</bool>", "<bool name=\"MaxQualityEnabled\">false</bool>"));
    }

    #[test]
    fn two_launches_in_a_row_still_give_back_the_value_from_before_the_first() {
        let mut journal = RestoreJournal::default();
        let first = xml_with("240", Some("0.800000"));
        let second = xml_with("144", Some("0.800000"));
        journal.record("g.xml", SettingsFileKind::Xml, Some(XML_USER), Some(&first), None);
        journal.record("g.xml", SettingsFileKind::Xml, Some(&first), Some(&second), None);
        assert_eq!(
            journal.restore_action("g.xml", Some(&second)),
            RestoreAction::Write(XML_USER.to_string())
        );
    }

    #[test]
    fn a_property_the_app_created_is_removed_again() {
        let mut journal = RestoreJournal::default();
        let before = xml_with("60", None);
        let launched = xml_with("60", Some("0.300000"));
        journal.record("g.xml", SettingsFileKind::Xml, Some(&before), Some(&launched), None);
        assert_eq!(journal.restore_action("g.xml", Some(&launched)), RestoreAction::Write(before));
    }

    #[test]
    fn putting_the_users_value_back_leaves_nothing_to_restore() {
        let mut journal = RestoreJournal::default();
        let launched = xml_with("240", Some("0.800000"));
        journal.record("g.xml", SettingsFileKind::Xml, Some(XML_USER), Some(&launched), None);
        journal.record("g.xml", SettingsFileKind::Xml, Some(&launched), Some(XML_USER), None);
        assert!(journal.is_empty());
    }

    #[test]
    fn properties_the_app_does_not_write_are_never_touched() {
        let mut journal = RestoreJournal::default();
        let other = XML_USER.replace(">x<", ">y<");
        journal.record("g.xml", SettingsFileKind::Xml, Some(XML_USER), Some(&other), None);
        assert!(journal.is_empty());
    }

    #[test]
    fn fast_flags_go_back_and_the_users_own_flags_stay() {
        let mut journal = RestoreJournal::default();
        let before = r#"{"DFIntTaskSchedulerTargetFps":60,"FFlagMine":true}"#;
        let after = r#"{"DFIntTaskSchedulerTargetFps":240,"FFlagMine":true,"FIntApp":1}"#;
        journal.record("c.json", SettingsFileKind::Json, Some(before), Some(after), None);
        // Depois, o usuário acrescentou uma flag dele à mão.
        let now = r#"{"DFIntTaskSchedulerTargetFps":240,"FFlagMine":true,"FIntApp":1,"FFlagLater":false}"#;
        let RestoreAction::Write(out) = journal.restore_action("c.json", Some(now)) else {
            panic!("expected a write");
        };
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"DFIntTaskSchedulerTargetFps": 60, "FFlagMine": true, "FFlagLater": false})
        );
    }

    #[test]
    fn a_settings_file_the_app_created_is_deleted_when_nothing_else_is_in_it() {
        let mut journal = RestoreJournal::default();
        let after = r#"{"DFIntTaskSchedulerTargetFps":240}"#;
        journal.record("c.json", SettingsFileKind::Json, None, Some(after), None);
        assert_eq!(journal.restore_action("c.json", Some(after)), RestoreAction::Delete);
        // Com algo do usuário dentro, só tira o que o app pôs.
        let now = r#"{"DFIntTaskSchedulerTargetFps":240,"FFlagMine":true}"#;
        assert_eq!(
            journal.restore_action("c.json", Some(now)),
            RestoreAction::Write(r#"{"FFlagMine":true}"#.to_string())
        );
    }

    #[test]
    fn a_file_that_disappeared_is_left_alone() {
        let mut journal = RestoreJournal::default();
        journal.record("g.xml", SettingsFileKind::Xml, Some(XML_USER), Some(&xml_with("240", Some("0.8"))), None);
        assert_eq!(journal.restore_action("g.xml", None), RestoreAction::Keep);
    }

    #[test]
    fn with_the_app_open_it_restores_once_no_roblox_runs() {
        assert!(restore_watch_due(true, true, false, RESTORE_WATCH_GRACE_SECS));
        assert!(restore_watch_due(true, true, false, u64::MAX));
    }

    #[test]
    fn a_running_client_even_from_the_website_holds_the_restore() {
        assert!(!restore_watch_due(true, true, true, u64::MAX));
    }

    #[test]
    fn right_after_a_launch_wrote_the_files_it_waits() {
        // O cliente novo ainda não apareceu: devolver agora tiraria a otimização dele.
        assert!(!restore_watch_due(true, true, false, 5));
        assert!(!restore_watch_due(true, true, false, RESTORE_WATCH_GRACE_SECS - 1));
    }

    #[test]
    fn nothing_recorded_or_the_option_off_does_nothing() {
        assert!(!restore_watch_due(true, false, false, u64::MAX));
        assert!(!restore_watch_due(false, true, false, u64::MAX));
    }

    #[test]
    fn the_kind_comes_from_the_extension() {
        assert_eq!(SettingsFileKind::of(std::path::Path::new("a/GlobalBasicSettings_13.xml")), SettingsFileKind::Xml);
        assert_eq!(SettingsFileKind::of(std::path::Path::new("a/ClientAppSettings.json")), SettingsFileKind::Json);
    }

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!("ram-settings-restore-{tag}-{nanos}"));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Do começo ao fim, só em pastas temporárias: a cópia antes da primeira
    /// mudança, a anotação que sobrevive a reabrir o app e a devolução.
    #[test]
    fn the_backup_is_taken_before_the_first_change_and_the_restore_clears_the_journal() {
        let data = TempDir::new("data");
        let roblox = TempDir::new("roblox");
        let xml = roblox.0.join("GlobalBasicSettings_13.xml");
        std::fs::write(&xml, XML_USER).unwrap();

        let snapshot = snapshot_roblox_settings(vec![xml.clone()]);
        std::fs::write(&xml, xml_with("240", Some("0.300000"))).unwrap();
        record_roblox_settings_change_in(&data.0, snapshot);

        assert!(has_pending_roblox_settings_restore_in(&data.0));
        let backup = data.0.join(RESTORE_BACKUP_DIR).join("0-GlobalBasicSettings_13.xml");
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), XML_USER);

        // Segunda abertura: a cópia continua a de antes da primeira.
        let snapshot = snapshot_roblox_settings(vec![xml.clone()]);
        std::fs::write(&xml, xml_with("144", Some("0.300000"))).unwrap();
        record_roblox_settings_change_in(&data.0, snapshot);
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), XML_USER);

        assert_eq!(restore_roblox_settings_in(&data.0), 1);
        assert_eq!(std::fs::read_to_string(&xml).unwrap(), XML_USER);
        assert!(!has_pending_roblox_settings_restore_in(&data.0));
        // A cópia fica: é para recuperar à mão se algo der errado.
        assert!(backup.exists());
    }

    #[test]
    fn turning_the_option_off_forgets_what_was_recorded() {
        let data = TempDir::new("discard");
        let roblox = TempDir::new("discard-roblox");
        let json = roblox.0.join("ClientAppSettings.json");
        let snapshot = snapshot_roblox_settings(vec![json.clone()]);
        std::fs::write(&json, r#"{"DFIntTaskSchedulerTargetFps":240}"#).unwrap();
        record_roblox_settings_change_in(&data.0, snapshot);
        assert!(has_pending_roblox_settings_restore_in(&data.0));
        discard_roblox_settings_restore_in(&data.0);
        assert!(!has_pending_roblox_settings_restore_in(&data.0));
        assert!(json.exists(), "forgetting never touches the Roblox file");
    }
}
