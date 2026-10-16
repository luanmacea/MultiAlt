//! Presets de launch: "estas contas → este jogo/servidor", com agendamento
//! opcional de abrir e fechar. Ver docs/features/presets.md.
//!
//! O arquivo `RAMLaunchPresets.json`, na pasta de dados, é a única cópia (está
//! em `DATA_FILES`, então backup, restauração e migração o levam). Mesmas
//! garantias do `RAMGameLists.json`:
//!
//! - gravação atômica (temporário + troca), com o conteúdo anterior em
//!   `RAMLaunchPresets.json.bak`;
//! - arquivo ilegível **trava** a gravação em vez de ser sobrescrito;
//! - o store **não guarda nada em memória**: cada leitura vai ao disco. Assim
//!   restaurar um backup vale na hora, sem reiniciar, e o agendador sempre vê o
//!   que está salvo.
//!
//! Aqui mora também a parte pura do agendamento (`due_actions`,
//! `next_open_after`, `next_close_after`), testada sem relógio de verdade.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const LAUNCH_PRESETS_FILE_NAME: &str = "RAMLaunchPresets.json";
const MAX_PRESETS_FILE_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_PRESETS: usize = 100;
pub const MAX_PRESET_ACCOUNTS: usize = 200;
pub const MAX_PRESET_NAME_CHARS: usize = 60;
const MAX_JOB_CHARS: usize = 512;
const MAX_LABEL_CHARS: usize = 120;

/// Horário de abrir e de fechar. Os dias (0 = segunda … 6 = domingo, como o
/// `num_days_from_monday` do chrono) valem só para **abrir**: fechar é "todo
/// dia nesse horário, se este preset tiver cliente aberto" — senão um preset
/// que abre segunda às 22:00 e fecha às 02:00 nunca fecharia (é terça).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetSchedule {
    #[serde(default)]
    pub open_enabled: bool,
    /// `HH:MM`, 24 h, hora local.
    #[serde(default)]
    pub open_at: String,
    #[serde(default)]
    pub days: Vec<u8>,
    #[serde(default)]
    pub close_enabled: bool,
    #[serde(default)]
    pub close_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchPreset {
    #[serde(default)]
    pub id: String,
    pub name: String,
    /// Contas fixas. Vazio quando o preset segue um grupo (`group`).
    #[serde(default)]
    pub user_ids: Vec<i64>,
    /// Grupo de contas (ideia 42): com ele, as contas saem do grupo **a cada
    /// launch** (quem está nele agora, na ordem da lista) em vez de `user_ids`.
    /// Texto literal do `Account.group`, como a lista agrupa. Ausente nos
    /// presets antigos — e nos de contas fixas, que gravam sem a chave.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub place_id: i64,
    /// Vazio = servidor público; Job ID; ou `vip:<código>` de um VIP salvo nos
    /// favoritos (a forma que `resolve_launch_job` entende).
    #[serde(default)]
    pub job_id: String,
    /// Nome do jogo, só para mostrar (a tela descobre de novo pelo Place ID).
    #[serde(default)]
    pub game_name: Option<String>,
    /// Nome do VIP escolhido nos favoritos, só para mostrar.
    #[serde(default)]
    pub vip_name: Option<String>,
    /// Organizar as janelas em grade quando o launch terminar (o mesmo botão
    /// "Arrange in grid" da aba Windows).
    #[serde(default)]
    pub arrange_grid: bool,
    #[serde(default)]
    pub schedule: Option<PresetSchedule>,
    #[serde(default)]
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchPresetsFile {
    #[serde(default)]
    pub presets: Vec<LaunchPreset>,
}

/// `HH:MM` → hora. Aceita `8:05` também; recusa o resto.
pub fn parse_hhmm(text: &str) -> Option<NaiveTime> {
    let (h, m) = text.trim().split_once(':')?;
    if h.is_empty() || h.len() > 2 || m.len() != 2 {
        return None;
    }
    let h: u32 = h.parse().ok()?;
    let m: u32 = m.parse().ok()?;
    NaiveTime::from_hms_opt(h, m, 0)
}

fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// Confere e normaliza um preset antes de gravar. Devolve a frase (em inglês,
/// a tela traduz) do primeiro problema.
pub fn normalize_preset(mut preset: LaunchPreset) -> Result<LaunchPreset, String> {
    let name = preset.name.trim().to_string();
    if name.is_empty() {
        return Err("Give the preset a name.".into());
    }
    if name.chars().count() > MAX_PRESET_NAME_CHARS {
        return Err(format!("The name is too long (max {MAX_PRESET_NAME_CHARS} characters)."));
    }
    preset.name = name;

    // Grupo só com espaços não é grupo. O texto fica como veio (sem trim):
    // o grupo da conta é comparado literalmente, como a lista agrupa.
    preset.group = preset.group.take().filter(|g| !g.trim().is_empty());
    if let Some(group) = &preset.group {
        if group.chars().count() > MAX_LABEL_CHARS {
            return Err("The group name is too long.".into());
        }
        // As contas saem do grupo a cada launch: lista fixa guardada junto só
        // confundiria (e "fechar" usa o que a execução abriu, não esta lista).
        preset.user_ids.clear();
    } else {
        let mut seen = std::collections::HashSet::new();
        preset.user_ids.retain(|id| *id > 0 && seen.insert(*id));
        if preset.user_ids.is_empty() {
            return Err("Pick at least one account.".into());
        }
        if preset.user_ids.len() > MAX_PRESET_ACCOUNTS {
            return Err(format!("A preset holds up to {MAX_PRESET_ACCOUNTS} accounts."));
        }
    }
    if preset.place_id <= 0 {
        return Err("Pick a game (Place ID).".into());
    }
    preset.job_id = preset.job_id.trim().to_string();
    if preset.job_id.chars().count() > MAX_JOB_CHARS {
        return Err("The server is too long.".into());
    }
    preset.game_name = preset
        .game_name
        .map(|n| truncate_chars(n.trim(), MAX_LABEL_CHARS))
        .filter(|n| !n.is_empty());
    preset.vip_name = preset
        .vip_name
        .map(|n| truncate_chars(n.trim(), MAX_LABEL_CHARS))
        .filter(|n| !n.is_empty());

    if let Some(mut schedule) = preset.schedule.take() {
        let mut days: Vec<u8> = schedule.days.iter().copied().filter(|d| *d <= 6).collect();
        days.sort_unstable();
        days.dedup();
        schedule.days = days;
        if schedule.open_enabled {
            let at = parse_hhmm(&schedule.open_at).ok_or("Open time must be HH:MM.")?;
            schedule.open_at = at.format("%H:%M").to_string();
            if schedule.days.is_empty() {
                return Err("Pick at least one day to open.".into());
            }
        }
        if schedule.close_enabled {
            let at = parse_hhmm(&schedule.close_at).ok_or("Close time must be HH:MM.")?;
            schedule.close_at = at.format("%H:%M").to_string();
        }
        // Agenda toda desligada não é agenda.
        if schedule.open_enabled || schedule.close_enabled {
            preset.schedule = Some(schedule);
        }
    }
    Ok(preset)
}

/// As contas que este launch do preset abre, a partir da lista de contas do app
/// (`(UserID, grupo)`, na ordem da lista):
///
/// - preset de **grupo**: quem está no grupo **agora**, na ordem da lista (até
///   `MAX_PRESET_ACCOUNTS`). Grupo renomeado ou esvaziado = erro claro;
/// - preset de **contas fixas**: as dele que ainda existem no app, na ordem do
///   preset.
pub fn resolve_preset_accounts(preset: &LaunchPreset, accounts: &[(i64, &str)]) -> Result<Vec<i64>, String> {
    let mut seen = std::collections::HashSet::new();
    match &preset.group {
        Some(group) => {
            let ids: Vec<i64> = accounts
                .iter()
                .filter(|(id, g)| *id > 0 && *g == group.as_str() && seen.insert(*id))
                .map(|(id, _)| *id)
                .take(MAX_PRESET_ACCOUNTS)
                .collect();
            if ids.is_empty() {
                return Err(format!("Group \"{group}\" has no accounts."));
            }
            Ok(ids)
        }
        None => {
            let known: std::collections::HashSet<i64> = accounts.iter().map(|(id, _)| *id).collect();
            let ids: Vec<i64> = preset
                .user_ids
                .iter()
                .copied()
                .filter(|id| known.contains(id) && seen.insert(*id))
                .collect();
            if ids.is_empty() {
                return Err("None of this preset's accounts are in the app anymore.".into());
            }
            Ok(ids)
        }
    }
}

/// O que o agendador tem que fazer agora.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetAction {
    Open,
    Close,
}

/// Instantes de `time` em cada dia de `from.date()` até `to.date()`.
fn occurrences(time: NaiveTime, from: NaiveDateTime, to: NaiveDateTime) -> Vec<NaiveDateTime> {
    let mut out = Vec::new();
    let mut day: NaiveDate = from.date();
    while day <= to.date() {
        out.push(day.and_time(time));
        match day.succ_opt() {
            Some(next) => day = next,
            None => break,
        }
    }
    out
}

fn open_day_enabled(schedule: &PresetSchedule, at: NaiveDateTime) -> bool {
    let weekday = at.weekday().num_days_from_monday() as u8;
    schedule.days.contains(&weekday)
}

/// O que vence no intervalo `(last, now]` (hora local, sem fuso). Quem chama
/// garante que o intervalo é curto: **não existe recuperação** de horário que
/// passou com o app fechado (ou com o PC dormindo).
pub fn due_actions(schedule: &PresetSchedule, last: NaiveDateTime, now: NaiveDateTime) -> Vec<PresetAction> {
    let mut out = Vec::new();
    if now <= last {
        return out;
    }
    if schedule.open_enabled {
        if let Some(time) = parse_hhmm(&schedule.open_at) {
            if occurrences(time, last, now)
                .into_iter()
                .any(|t| t > last && t <= now && open_day_enabled(schedule, t))
            {
                out.push(PresetAction::Open);
            }
        }
    }
    if schedule.close_enabled {
        if let Some(time) = parse_hhmm(&schedule.close_at) {
            if occurrences(time, last, now).into_iter().any(|t| t > last && t <= now) {
                out.push(PresetAction::Close);
            }
        }
    }
    out
}

/// Próxima abertura depois de `now` (a tela mostra "Next: …").
pub fn next_open_after(schedule: &PresetSchedule, now: NaiveDateTime) -> Option<NaiveDateTime> {
    if !schedule.open_enabled || schedule.days.is_empty() {
        return None;
    }
    let time = parse_hhmm(&schedule.open_at)?;
    occurrences(time, now, now + Duration::days(8))
        .into_iter()
        .find(|t| *t > now && open_day_enabled(schedule, *t))
}

/// Próximo fechamento depois de `now`.
pub fn next_close_after(schedule: &PresetSchedule, now: NaiveDateTime) -> Option<NaiveDateTime> {
    if !schedule.close_enabled {
        return None;
    }
    let time = parse_hhmm(&schedule.close_at)?;
    occurrences(time, now, now + Duration::days(2))
        .into_iter()
        .find(|t| *t > now)
}

pub struct LaunchPresetStore {
    file_path: PathBuf,
    lock: Mutex<()>,
}

fn sibling(file: &Path, suffix: &str) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

impl LaunchPresetStore {
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            file_path,
            lock: Mutex::new(()),
        }
    }

    #[cfg(test)]
    pub fn backup_file_path(&self) -> PathBuf {
        sibling(&self.file_path, ".bak")
    }

    /// Arquivo ausente ou vazio = sem presets. Ilegível = `Err` (nunca "lista
    /// vazia", senão a próxima gravação apagaria o que ele tem).
    fn read_from_disk(&self) -> Result<Option<LaunchPresetsFile>, String> {
        let metadata = match fs::metadata(&self.file_path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("Failed to read the presets file: {e}")),
        };
        if metadata.len() > MAX_PRESETS_FILE_BYTES {
            return Err(format!(
                "The presets file is too large (max {MAX_PRESETS_FILE_BYTES} bytes)"
            ));
        }
        let data = fs::read(&self.file_path).map_err(|e| format!("Failed to read the presets file: {e}"))?;
        if data.iter().all(|b| b.is_ascii_whitespace()) {
            return Ok(None);
        }
        serde_json::from_slice::<LaunchPresetsFile>(&data)
            .map(Some)
            .map_err(|e| format!("Failed to parse the presets file: {e}"))
    }

    pub fn list(&self) -> Result<Vec<LaunchPreset>, String> {
        let _guard = self.lock.lock().map_err(|e| e.to_string())?;
        Ok(self.read_from_disk()?.unwrap_or_default().presets)
    }

    pub fn get(&self, id: &str) -> Result<Option<LaunchPreset>, String> {
        Ok(self.list()?.into_iter().find(|p| p.id == id))
    }

    fn write(&self, file: &LaunchPresetsFile, existed: bool) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(file).map_err(|e| format!("Failed to serialize presets: {e}"))?;
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create the data folder: {e}"))?;
        }
        if existed {
            fs::copy(&self.file_path, sibling(&self.file_path, ".bak"))
                .map_err(|e| format!("Failed to keep the previous presets ({e}); nothing was saved."))?;
        }
        let tmp = sibling(&self.file_path, ".tmp");
        match crate::data::versions::write_all_synced(&tmp, &bytes) {
            Ok(true) => {}
            Ok(false) => eprintln!("Warning: presets written but fsync could not be confirmed"),
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                return Err(format!("Failed to write the presets file: {e}"));
            }
        }
        crate::data::versions::atomic_replace(&tmp, &self.file_path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            format!("Failed to write the presets file: {e}")
        })
    }

    /// Cria (id vazio ou desconhecido) ou substitui o preset com o mesmo id.
    pub fn upsert(&self, preset: LaunchPreset, now_ms: i64) -> Result<LaunchPreset, String> {
        let mut preset = normalize_preset(preset)?;
        let _guard = self.lock.lock().map_err(|e| e.to_string())?;
        let current = self.read_from_disk().map_err(|e| {
            format!("{e}; refusing to overwrite it. Fix or restore {LAUNCH_PRESETS_FILE_NAME} and restart.")
        })?;
        let existed = current.is_some();
        let mut file = current.unwrap_or_default();

        match file.presets.iter_mut().find(|p| !preset.id.is_empty() && p.id == preset.id) {
            Some(slot) => {
                preset.created_at = slot.created_at;
                *slot = preset.clone();
            }
            None => {
                if file.presets.len() >= MAX_PRESETS {
                    return Err(format!("You can keep up to {MAX_PRESETS} presets."));
                }
                let mut n = 0u32;
                let mut id = format!("preset-{now_ms}");
                while file.presets.iter().any(|p| p.id == id) {
                    n += 1;
                    id = format!("preset-{now_ms}-{n}");
                }
                preset.id = id;
                preset.created_at = now_ms;
                file.presets.push(preset.clone());
            }
        }
        self.write(&file, existed)?;
        Ok(preset)
    }

    /// Faz os presets de **grupo** que apontam para `from` passarem a `to`.
    ///
    /// Chamado quando o usuário renomeia (ou junta) um grupo de contas: sem isto
    /// o preset guarda o nome antigo e o launch dele falha depois com
    /// "Group \"...\" has no accounts.", mesmo com as contas ali — elas só estão
    /// num grupo com outro nome. Junção (o `to` já existe como grupo) é o mesmo
    /// caminho: o preset passa a resolver pelas contas do grupo de destino.
    /// Compara o texto literal, como `resolve_preset_accounts` agrupa. Grava só
    /// se algum preset mudou; devolve quantos.
    pub fn rename_group(&self, from: &str, to: &str) -> Result<usize, String> {
        if from == to {
            return Ok(0);
        }
        let _guard = self.lock.lock().map_err(|e| e.to_string())?;
        let Some(mut file) = self.read_from_disk().map_err(|e| {
            format!("{e}; refusing to overwrite it. Fix or restore {LAUNCH_PRESETS_FILE_NAME} and restart.")
        })?
        else {
            return Ok(0);
        };
        let mut changed = 0usize;
        for preset in file.presets.iter_mut() {
            if preset.group.as_deref() == Some(from) {
                preset.group = Some(to.to_string());
                changed += 1;
            }
        }
        if changed == 0 {
            return Ok(0);
        }
        self.write(&file, true)?;
        Ok(changed)
    }

    /// Apaga pelo id. `Ok(false)` se não existia.
    pub fn delete(&self, id: &str) -> Result<bool, String> {
        let _guard = self.lock.lock().map_err(|e| e.to_string())?;
        let Some(mut file) = self.read_from_disk()? else {
            return Ok(false);
        };
        let before = file.presets.len();
        file.presets.retain(|p| p.id != id);
        if file.presets.len() == before {
            return Ok(false);
        }
        self.write(&file, true)?;
        Ok(true)
    }
}

#[cfg(test)]
mod launch_preset_store_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!("ram-presets-{}-{nanos}-{n}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn file(&self) -> PathBuf {
            self.0.join(LAUNCH_PRESETS_FILE_NAME)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn preset(name: &str) -> LaunchPreset {
        LaunchPreset {
            name: name.into(),
            user_ids: vec![1, 2],
            place_id: 920587237,
            ..Default::default()
        }
    }

    #[test]
    fn a_new_preset_gets_an_id_and_round_trips_in_camel_case() {
        let dir = TempDir::new();
        let store = LaunchPresetStore::new(dir.file());
        assert!(store.list().unwrap().is_empty(), "no file = no presets");

        let saved = store.upsert(preset("Farm"), 1_000).unwrap();
        assert_eq!(saved.id, "preset-1000");
        assert_eq!(saved.created_at, 1_000);
        assert_eq!(store.list().unwrap(), vec![saved.clone()]);

        let raw: serde_json::Value = serde_json::from_slice(&fs::read(dir.file()).unwrap()).unwrap();
        assert!(raw["presets"][0].get("userIds").is_some(), "{raw}");
        assert!(raw["presets"][0].get("placeId").is_some());
    }

    #[test]
    fn saving_with_an_existing_id_renames_in_place_and_keeps_the_creation_time() {
        let dir = TempDir::new();
        let store = LaunchPresetStore::new(dir.file());
        let first = store.upsert(preset("Farm"), 1_000).unwrap();
        let mut renamed = first.clone();
        renamed.name = "Night farm".into();
        let saved = store.upsert(renamed, 9_999).unwrap();

        assert_eq!(saved.id, first.id);
        assert_eq!(saved.created_at, 1_000);
        let all = store.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "Night farm");
    }

    #[test]
    fn two_presets_saved_in_the_same_millisecond_get_different_ids() {
        let dir = TempDir::new();
        let store = LaunchPresetStore::new(dir.file());
        let a = store.upsert(preset("A"), 5).unwrap();
        let b = store.upsert(preset("B"), 5).unwrap();
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn every_write_keeps_the_previous_file_in_a_bak() {
        let dir = TempDir::new();
        let store = LaunchPresetStore::new(dir.file());
        store.upsert(preset("A"), 1).unwrap();
        assert!(!store.backup_file_path().exists());
        let before = fs::read(dir.file()).unwrap();
        store.upsert(preset("B"), 2).unwrap();
        assert_eq!(fs::read(store.backup_file_path()).unwrap(), before);
    }

    #[test]
    fn an_unreadable_file_is_never_overwritten() {
        let dir = TempDir::new();
        fs::write(dir.file(), b"{ broken").unwrap();
        let store = LaunchPresetStore::new(dir.file());
        assert!(store.list().is_err());
        assert!(store.upsert(preset("A"), 1).is_err());
        assert_eq!(fs::read(dir.file()).unwrap(), b"{ broken");
    }

    #[test]
    fn delete_removes_only_that_preset() {
        let dir = TempDir::new();
        let store = LaunchPresetStore::new(dir.file());
        let a = store.upsert(preset("A"), 1).unwrap();
        let b = store.upsert(preset("B"), 2).unwrap();
        assert!(store.delete(&a.id).unwrap());
        assert!(!store.delete(&a.id).unwrap());
        assert_eq!(store.list().unwrap(), vec![b]);
    }

    #[test]
    fn the_store_reads_the_disk_every_time_so_a_restored_file_counts_at_once() {
        let dir = TempDir::new();
        let store = LaunchPresetStore::new(dir.file());
        store.upsert(preset("A"), 1).unwrap();
        // Um backup restaurado troca o arquivo por fora do store.
        let restored = LaunchPresetsFile {
            presets: vec![LaunchPreset {
                id: "preset-7".into(),
                ..preset("From backup")
            }],
        };
        fs::write(dir.file(), serde_json::to_vec(&restored).unwrap()).unwrap();
        assert_eq!(store.list().unwrap()[0].name, "From backup");
    }

    fn group_preset(name: &str, group: &str) -> LaunchPreset {
        LaunchPreset {
            name: name.into(),
            group: Some(group.into()),
            user_ids: vec![],
            place_id: 920587237,
            ..Default::default()
        }
    }

    /// Fix 2: renomear um grupo faz os presets de grupo seguirem o nome novo
    /// (senão o launch do preset falha com "Group ... has no accounts").
    #[test]
    fn renaming_a_group_points_its_group_presets_at_the_new_name() {
        let dir = TempDir::new();
        let store = LaunchPresetStore::new(dir.file());
        store.upsert(group_preset("Night", "Alts"), 1).unwrap();
        // Preset de contas fixas: não tem grupo, não pode ser tocado.
        store.upsert(preset("Fixed"), 2).unwrap();

        assert_eq!(store.rename_group("Alts", "Mains").unwrap(), 1);
        let all = store.list().unwrap();
        assert_eq!(
            all.iter().find(|p| p.name == "Night").unwrap().group.as_deref(),
            Some("Mains")
        );
        assert_eq!(all.iter().find(|p| p.name == "Fixed").unwrap().group, None);
        // Grupo que nenhum preset usa: nada muda, nada é gravado.
        assert_eq!(store.rename_group("Ghost", "X").unwrap(), 0);
    }

    /// Fix 2: juntar grupos (o destino já existe) aponta os presets do grupo que
    /// some para o grupo de destino.
    #[test]
    fn merging_a_group_points_its_presets_at_the_target_group() {
        let dir = TempDir::new();
        let store = LaunchPresetStore::new(dir.file());
        store.upsert(group_preset("A", "Alts"), 1).unwrap();
        store.upsert(group_preset("B", "Mains"), 2).unwrap();

        assert_eq!(store.rename_group("Alts", "Mains").unwrap(), 1);
        let all = store.list().unwrap();
        assert_eq!(
            all.iter().filter(|p| p.group.as_deref() == Some("Mains")).count(),
            2,
            "os dois presets agora resolvem pelo grupo de destino"
        );
    }
}

#[cfg(test)]
mod launch_preset_validation_tests {
    use super::*;

    fn valid() -> LaunchPreset {
        LaunchPreset {
            name: "  Farm  ".into(),
            user_ids: vec![3, 1, 3, -1],
            place_id: 1,
            job_id: "  abc  ".into(),
            ..Default::default()
        }
    }

    #[test]
    fn names_and_servers_are_trimmed_and_accounts_deduplicated() {
        let p = normalize_preset(valid()).unwrap();
        assert_eq!(p.name, "Farm");
        assert_eq!(p.user_ids, vec![3, 1]);
        assert_eq!(p.job_id, "abc");
    }

    #[test]
    fn a_preset_needs_a_name_an_account_and_a_game() {
        assert!(normalize_preset(LaunchPreset { name: " ".into(), ..valid() }).is_err());
        assert!(normalize_preset(LaunchPreset { user_ids: vec![], ..valid() }).is_err());
        assert!(normalize_preset(LaunchPreset { place_id: 0, ..valid() }).is_err());
        let long = "x".repeat(MAX_PRESET_NAME_CHARS + 1);
        assert!(normalize_preset(LaunchPreset { name: long, ..valid() }).is_err());
    }

    #[test]
    fn times_must_be_hh_mm_and_opening_needs_a_day() {
        let schedule = |open_at: &str, days: Vec<u8>| PresetSchedule {
            open_enabled: true,
            open_at: open_at.into(),
            days,
            ..Default::default()
        };
        let ok = normalize_preset(LaunchPreset { schedule: Some(schedule("8:05", vec![6, 0, 0, 9])), ..valid() }).unwrap();
        let s = ok.schedule.unwrap();
        assert_eq!(s.open_at, "08:05");
        assert_eq!(s.days, vec![0, 6], "sorted, deduped, out-of-range dropped");

        assert!(normalize_preset(LaunchPreset { schedule: Some(schedule("25:00", vec![0])), ..valid() }).is_err());
        assert!(normalize_preset(LaunchPreset { schedule: Some(schedule("8h", vec![0])), ..valid() }).is_err());
        assert!(normalize_preset(LaunchPreset { schedule: Some(schedule("08:00", vec![])), ..valid() }).is_err());
    }

    #[test]
    fn a_group_preset_needs_no_fixed_accounts_and_drops_them() {
        let p = normalize_preset(LaunchPreset {
            user_ids: vec![],
            group: Some("10 Alts".into()),
            ..valid()
        })
        .unwrap();
        assert_eq!(p.group.as_deref(), Some("10 Alts"), "literal text, prefix kept");
        assert!(p.user_ids.is_empty());

        let p = normalize_preset(LaunchPreset { group: Some("Farm".into()), ..valid() }).unwrap();
        assert!(p.user_ids.is_empty(), "a group preset keeps no stale fixed list");
    }

    #[test]
    fn a_blank_group_is_a_fixed_list_preset() {
        let p = normalize_preset(LaunchPreset { group: Some("   ".into()), ..valid() }).unwrap();
        assert_eq!(p.group, None);
        assert_eq!(p.user_ids, vec![3, 1]);
        assert!(normalize_preset(LaunchPreset { group: Some(" ".into()), user_ids: vec![], ..valid() }).is_err());
        let long = "g".repeat(MAX_LABEL_CHARS + 1);
        assert!(normalize_preset(LaunchPreset { group: Some(long), ..valid() }).is_err());
    }

    #[test]
    fn old_presets_without_a_group_load_unchanged_and_fixed_ones_save_without_the_key() {
        let old = r#"{ "id": "preset-1", "name": "Farm", "userIds": [1, 2], "placeId": 5 }"#;
        let p: LaunchPreset = serde_json::from_str(old).unwrap();
        assert_eq!(p.group, None);
        assert_eq!(p.user_ids, vec![1, 2]);
        let json = serde_json::to_value(&p).unwrap();
        assert!(json.get("group").is_none(), "{json}");

        let grouped: LaunchPreset =
            serde_json::from_str(r#"{ "name": "G", "group": "Alts", "placeId": 5 }"#).unwrap();
        assert_eq!(grouped.group.as_deref(), Some("Alts"));
        assert!(grouped.user_ids.is_empty());
    }

    #[test]
    fn a_group_preset_launches_the_current_members_in_list_order() {
        let preset = LaunchPreset { group: Some("Alts".into()), ..valid() };
        let accounts = [(5, "Alts"), (2, "Main"), (9, "Alts"), (1, "Alts")];
        assert_eq!(resolve_preset_accounts(&preset, &accounts).unwrap(), vec![5, 9, 1]);
        // Conta nova no grupo entra no próximo launch; quem saiu, não.
        let later = [(5, "Main"), (9, "Alts"), (1, "Alts"), (7, "Alts")];
        assert_eq!(resolve_preset_accounts(&preset, &later).unwrap(), vec![9, 1, 7]);
        // Literal: "alts" e "1 Alts" são outros grupos.
        let others = [(5, "alts"), (6, "1 Alts")];
        assert!(resolve_preset_accounts(&preset, &others).is_err());
    }

    #[test]
    fn a_renamed_or_emptied_group_says_so_on_launch() {
        let preset = LaunchPreset { group: Some("Alts".into()), ..valid() };
        assert_eq!(
            resolve_preset_accounts(&preset, &[(1, "Renamed")]).unwrap_err(),
            "Group \"Alts\" has no accounts."
        );
        assert_eq!(resolve_preset_accounts(&preset, &[]).unwrap_err(), "Group \"Alts\" has no accounts.");
    }

    #[test]
    fn a_fixed_preset_launches_its_accounts_still_in_the_app_in_its_order() {
        let preset = LaunchPreset { user_ids: vec![3, 1, 8], ..valid() };
        let accounts = [(1, "Default"), (3, "Alts")];
        assert_eq!(resolve_preset_accounts(&preset, &accounts).unwrap(), vec![3, 1]);
        assert_eq!(
            resolve_preset_accounts(&preset, &[(4, "Default")]).unwrap_err(),
            "None of this preset's accounts are in the app anymore."
        );
    }

    #[test]
    fn a_schedule_with_nothing_on_is_dropped() {
        let p = normalize_preset(LaunchPreset {
            schedule: Some(PresetSchedule { open_at: "garbage".into(), ..Default::default() }),
            ..valid()
        })
        .unwrap();
        assert_eq!(p.schedule, None);
    }
}

#[cfg(test)]
mod launch_preset_schedule_tests {
    use super::*;

    fn at(text: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    // 2026-10-12 é uma segunda-feira.
    fn weekdays_8am_close_17() -> PresetSchedule {
        PresetSchedule {
            open_enabled: true,
            open_at: "08:00".into(),
            days: vec![0, 1, 2, 3, 4],
            close_enabled: true,
            close_at: "17:00".into(),
        }
    }

    #[test]
    fn opening_fires_once_when_the_tick_crosses_the_time() {
        let s = weekdays_8am_close_17();
        assert_eq!(
            due_actions(&s, at("2026-10-12 07:59:50"), at("2026-10-12 08:00:05")),
            vec![PresetAction::Open]
        );
        // A passada seguinte não repete.
        assert!(due_actions(&s, at("2026-10-12 08:00:05"), at("2026-10-12 08:00:20")).is_empty());
    }

    #[test]
    fn opening_respects_the_days_and_closing_runs_every_day() {
        let s = weekdays_8am_close_17();
        // Sábado: não abre.
        assert!(due_actions(&s, at("2026-10-17 07:59:50"), at("2026-10-17 08:00:05")).is_empty());
        // Mas fecha (se houver cliente do preset aberto — quem decide é quem chama).
        assert_eq!(
            due_actions(&s, at("2026-10-17 16:59:55"), at("2026-10-17 17:00:10")),
            vec![PresetAction::Close]
        );
    }

    #[test]
    fn a_window_across_midnight_still_sees_the_time() {
        let s = PresetSchedule {
            open_enabled: true,
            open_at: "00:00".into(),
            days: vec![1],
            ..Default::default()
        };
        // Segunda 23:59:50 → terça 00:00:05: a terça está nos dias.
        assert_eq!(
            due_actions(&s, at("2026-10-12 23:59:50"), at("2026-10-13 00:00:05")),
            vec![PresetAction::Open]
        );
    }

    #[test]
    fn nothing_fires_when_the_clock_does_not_move_forward() {
        let s = weekdays_8am_close_17();
        assert!(due_actions(&s, at("2026-10-12 08:00:05"), at("2026-10-12 07:59:00")).is_empty());
    }

    #[test]
    fn the_next_opening_skips_disabled_days() {
        let s = weekdays_8am_close_17();
        // Sexta 09:00 → próxima segunda 08:00.
        assert_eq!(next_open_after(&s, at("2026-10-16 09:00:00")), Some(at("2026-10-19 08:00:00")));
        // Segunda 07:00 → hoje 08:00.
        assert_eq!(next_open_after(&s, at("2026-10-12 07:00:00")), Some(at("2026-10-12 08:00:00")));
        assert_eq!(next_close_after(&s, at("2026-10-12 17:00:00")), Some(at("2026-10-13 17:00:00")));
    }

    #[test]
    fn without_opening_there_is_no_next_opening() {
        let s = PresetSchedule {
            close_enabled: true,
            close_at: "17:00".into(),
            ..Default::default()
        };
        assert_eq!(next_open_after(&s, at("2026-10-12 07:00:00")), None);
        assert_eq!(next_close_after(&s, at("2026-10-12 07:00:00")), Some(at("2026-10-12 17:00:00")));
    }
}
