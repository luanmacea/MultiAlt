// O dashboard que o próprio script desenha (docs/features/python-automation.md,
// "Dashboard"). Uma seção por script, com o layout (widgets), os valores
// (`"id"` ou `"id@userId"`) e as séries dos gráficos. O formato de cada valor é
// do script (o SDK documenta em docs/API.md): aqui só os limites valem, e o que
// chega é guardado como JSON, nunca interpretado como HTML.
//
// Persistido em `<pasta de trabalho>/dashboard.json`, com a gravação agrupada
// (no máximo uma por segundo); a tela recebe no máximo 4 atualizações por
// segundo por script — o resto é agrupado na seguinte.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use super::errors::PyError;
use super::types::DashboardSection;

pub const MAX_WIDGETS: usize = 40;
pub const MAX_TEXT_BYTES: usize = 2 * 1024;
pub const MAX_TABLE_ROWS: usize = 200;
pub const MAX_POINTS: usize = 500;
/// Teto de chaves de valor por script (uma por widget e por conta, na prática).
pub const MAX_VALUES: usize = 4_000;
pub const EMIT_EVERY: Duration = Duration::from_millis(250);

const WIDGET_TYPES: &[&str] = &["stat", "progress", "text", "badge", "table", "accounts", "line", "log"];

#[derive(Default)]
struct Inner {
    sections: BTreeMap<String, DashboardSection>,
    pending_emit: HashSet<String>,
    last_emit: HashMap<String, Instant>,
    dirty: bool,
}

pub struct DashboardStore {
    path: Option<PathBuf>,
    inner: Mutex<Inner>,
}

/// Corta texto em `MAX_TEXT_BYTES` (na fronteira de caractere) e lista em
/// `MAX_TABLE_ROWS` (ficam as últimas), em qualquer profundidade.
fn clamp_value(value: Value) -> Value {
    match value {
        Value::String(s) if s.len() > MAX_TEXT_BYTES => {
            let mut end = MAX_TEXT_BYTES;
            while !s.is_char_boundary(end) {
                end -= 1;
            }
            Value::String(s[..end].to_string())
        }
        Value::Array(items) => {
            let skip = items.len().saturating_sub(MAX_TABLE_ROWS);
            Value::Array(items.into_iter().skip(skip).map(clamp_value).collect())
        }
        Value::Object(map) => Value::Object(map.into_iter().map(|(k, v)| (k, clamp_value(v))).collect()),
        other => other,
    }
}

fn widget_id(op: &str, args: &Value) -> Result<String, PyError> {
    let id = args
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| PyError::bad_args(op, "id must be a string"))?;
    if id.is_empty() || id.len() > 100 || id.contains('@') {
        return Err(PyError::bad_args(op, "id must be 1-100 characters, without '@'"));
    }
    Ok(id.to_string())
}

impl DashboardStore {
    pub fn new(path: Option<PathBuf>) -> Self {
        let sections = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str::<Vec<DashboardSection>>(&t).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|s| (s.script.clone(), s))
            .collect();
        Self {
            path,
            inner: Mutex::new(Inner {
                sections,
                ..Default::default()
            }),
        }
    }

    /// `dashboard.define|set|push|clear` de um script.
    pub fn apply(&self, script: &str, op: &str, args: &Value, now_ms: u64) -> Result<Value, PyError> {
        let mut inner = self.inner.lock().unwrap();
        let section = inner.sections.entry(script.to_string()).or_insert_with(|| DashboardSection {
            script: script.to_string(),
            ..Default::default()
        });
        match op {
            "dashboard.define" => {
                let layout = args
                    .get("layout")
                    .and_then(Value::as_array)
                    .ok_or_else(|| PyError::bad_args(op, "layout must be a list of widgets"))?;
                if layout.len() > MAX_WIDGETS {
                    return Err(PyError::bad_args(
                        op,
                        &format!("at most {MAX_WIDGETS} widgets per script, got {}", layout.len()),
                    ));
                }
                let mut seen = HashSet::new();
                for widget in layout {
                    let id = widget_id(op, widget)?;
                    if !seen.insert(id.clone()) {
                        return Err(PyError::bad_args(op, &format!("widget id \"{id}\" is used twice")));
                    }
                    let kind = widget.get("type").and_then(Value::as_str).unwrap_or("");
                    if !WIDGET_TYPES.contains(&kind) {
                        return Err(PyError::bad_args(
                            op,
                            &format!("widget \"{id}\" has unknown type \"{kind}\" (use {})", WIDGET_TYPES.join(", ")),
                        ));
                    }
                }
                section.layout = layout.iter().cloned().map(clamp_value).collect();
            }
            "dashboard.set" => {
                let id = widget_id(op, args)?;
                let value = clamp_value(args.get("value").cloned().unwrap_or(Value::Null));
                let key = match args.get("user_id").and_then(Value::as_i64) {
                    Some(user) => format!("{id}@{user}"),
                    None => id,
                };
                if !section.values.contains_key(&key) && section.values.len() >= MAX_VALUES {
                    return Err(PyError::bad_args(op, &format!("at most {MAX_VALUES} values per script")));
                }
                section.values.insert(key, value);
            }
            "dashboard.push" => {
                let id = widget_id(op, args)?;
                let value = clamp_value(args.get("value").cloned().unwrap_or(Value::Null));
                if !section.series.contains_key(&id) && section.series.len() >= MAX_WIDGETS {
                    return Err(PyError::bad_args(op, &format!("at most {MAX_WIDGETS} series per script")));
                }
                let series = section.series.entry(id).or_insert_with(|| Value::Array(Vec::new()));
                if let Value::Array(points) = series {
                    points.push(json!([now_ms, value]));
                    let extra = points.len().saturating_sub(MAX_POINTS);
                    if extra > 0 {
                        points.drain(..extra);
                    }
                }
            }
            "dashboard.clear" => {
                section.layout.clear();
                section.values = Map::new();
                section.series = Map::new();
            }
            _ => return Err(PyError::forbidden()),
        }
        section.updated_ms = now_ms;
        inner.pending_emit.insert(script.to_string());
        inner.dirty = true;
        Ok(json!(true))
    }

    pub fn sections(&self) -> Vec<DashboardSection> {
        self.inner.lock().unwrap().sections.values().cloned().collect()
    }

    /// O "Clear" da seção (a tela). Some do arquivo também.
    pub fn clear_section(&self, script: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.sections.remove(script);
        inner.pending_emit.remove(script);
        inner.dirty = true;
    }

    /// As seções que mudaram e cuja última atualização na tela foi há mais de
    /// 250 ms. As outras esperam o próximo tique.
    pub fn take_due_emits(&self, now: Instant) -> Vec<DashboardSection> {
        let mut inner = self.inner.lock().unwrap();
        let due: Vec<String> = inner
            .pending_emit
            .iter()
            .filter(|s| inner.last_emit.get(*s).map(|t| now.saturating_duration_since(*t) >= EMIT_EVERY).unwrap_or(true))
            .cloned()
            .collect();
        let mut out = Vec::new();
        for script in due {
            inner.pending_emit.remove(&script);
            inner.last_emit.insert(script.clone(), now);
            if let Some(section) = inner.sections.get(&script) {
                out.push(section.clone());
            }
        }
        out
    }

    /// Grava se algo mudou desde a última gravação.
    pub fn save_if_dirty(&self) -> Result<bool, String> {
        let (data, path) = {
            let mut inner = self.inner.lock().unwrap();
            if !inner.dirty {
                return Ok(false);
            }
            inner.dirty = false;
            let Some(path) = self.path.clone() else {
                return Ok(false);
            };
            let all: Vec<&DashboardSection> = inner.sections.values().collect();
            (serde_json::to_string(&all).map_err(|e| e.to_string())?, path)
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&path, data).map_err(|e| e.to_string())?;
        Ok(true)
    }
}

#[cfg(test)]
mod python_dashboard_tests {
    use super::*;

    fn widgets(n: usize) -> Value {
        Value::Array((0..n).map(|i| json!({"id": format!("w{i}"), "type": "stat", "title": "T"})).collect())
    }

    #[test]
    fn limits_are_enforced() {
        let store = DashboardStore::new(None);
        assert!(store.apply("farm.py", "dashboard.define", &json!({"layout": widgets(40)}), 1).is_ok());
        let error = store.apply("farm.py", "dashboard.define", &json!({"layout": widgets(41)}), 1).unwrap_err();
        assert_eq!(error.code.as_str(), "bad_args");
        let bad_type = json!({"layout": [{"id": "x", "type": "html"}]});
        assert!(store.apply("farm.py", "dashboard.define", &bad_type, 1).is_err());

        let long = "é".repeat(5_000); // 10 KB
        store.apply("farm.py", "dashboard.set", &json!({"id": "w0", "value": long}), 2).unwrap();
        let rows: Vec<Value> = (0..250).map(|i| json!([i, "row"])).collect();
        store.apply("farm.py", "dashboard.set", &json!({"id": "w1", "value": rows}), 2).unwrap();
        store.apply("farm.py", "dashboard.set", &json!({"id": "w2", "value": "ok", "user_id": 7}), 2).unwrap();

        let section = store.sections().into_iter().find(|s| s.script == "farm.py").unwrap();
        assert_eq!(section.layout.len(), 40, "a refused define keeps the previous layout");
        let text = section.values["w0"].as_str().unwrap();
        assert!(text.len() <= MAX_TEXT_BYTES && text.len() > MAX_TEXT_BYTES - 4);
        let table = section.values["w1"].as_array().unwrap();
        assert_eq!(table.len(), MAX_TABLE_ROWS);
        assert_eq!(table[0], json!([50, "row"]), "the newest rows are kept");
        assert_eq!(section.values["w2@7"], json!("ok"));
        assert_eq!(section.updated_ms, 2);
        assert!(store.apply("farm.py", "dashboard.set", &json!({"id": "a@b", "value": 1}), 3).is_err());
    }

    #[test]
    fn series_keeps_last_500() {
        let store = DashboardStore::new(None);
        for i in 0..600u64 {
            store.apply("farm.py", "dashboard.push", &json!({"id": "loops", "value": i}), 1_000 + i).unwrap();
        }
        let section = &store.sections()[0];
        let points = section.series["loops"].as_array().unwrap();
        assert_eq!(points.len(), MAX_POINTS);
        assert_eq!(points[0], json!([1_100, 100]));
        assert_eq!(points[499], json!([1_599, 599]));
    }

    #[test]
    fn sections_persist_and_clear() {
        let path = std::env::temp_dir().join(format!(
            "ram-pydash-{}.json",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        {
            let store = DashboardStore::new(Some(path.clone()));
            store.apply("a.py", "dashboard.set", &json!({"id": "x", "value": 1}), 5).unwrap();
            store.apply("b.py", "dashboard.push", &json!({"id": "y", "value": 2}), 6).unwrap();
            assert!(store.save_if_dirty().unwrap());
            assert!(!store.save_if_dirty().unwrap(), "nothing changed since");
        }
        let reopened = DashboardStore::new(Some(path.clone()));
        let scripts: Vec<String> = reopened.sections().into_iter().map(|s| s.script).collect();
        assert_eq!(scripts, vec!["a.py".to_string(), "b.py".to_string()]);
        reopened.clear_section("a.py");
        reopened.apply("b.py", "dashboard.clear", &json!({}), 7).unwrap();
        reopened.save_if_dirty().unwrap();
        let again = DashboardStore::new(Some(path.clone()));
        let sections = again.sections();
        assert_eq!(sections.len(), 1);
        assert!(sections[0].series.is_empty() && sections[0].values.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn screen_updates_are_throttled_per_script() {
        let store = DashboardStore::new(None);
        let t0 = Instant::now();
        store.apply("a.py", "dashboard.set", &json!({"id": "x", "value": 1}), 1).unwrap();
        assert_eq!(store.take_due_emits(t0).len(), 1);
        store.apply("a.py", "dashboard.set", &json!({"id": "x", "value": 2}), 2).unwrap();
        store.apply("a.py", "dashboard.set", &json!({"id": "x", "value": 3}), 3).unwrap();
        assert!(store.take_due_emits(t0 + Duration::from_millis(100)).is_empty(), "grouped");
        let later = store.take_due_emits(t0 + Duration::from_millis(260));
        assert_eq!(later.len(), 1);
        assert_eq!(later[0].values["x"], json!(3));
    }
}
