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
