use crate::config;
use serde::{Deserialize, Serialize};

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    pub timestamp_ms: u64,
    pub source: String,
    pub translation: String,
    pub error: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub title: String,
    pub started_at: u64,
    pub ended_at: Option<u64>,
    pub source_language: String,
    pub target_language: String,
    pub entries: Vec<Entry>,
}

impl Record {
    pub fn new(cfg: &config::Config, title: String) -> Self {
        let now = now_ms();
        Self {
            id: format!("{now}-{}", std::process::id()),
            title,
            started_at: now,
            ended_at: None,
            source_language: cfg.source_language.clone(),
            target_language: cfg.target_language.clone(),
            entries: vec![],
        }
    }
    pub fn save(&self) -> Result<(), String> {
        config::write_json(&path(&self.id)?, self)
            .map_err(|e| format!("Не удалось сохранить историю: {e}"))
    }
}

pub fn path(id: &str) -> Result<std::path::PathBuf, String> {
    if id.is_empty() || id.len() > 80 || !id.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
        return Err("Некорректный ID сессии".into());
    }
    Ok(config::config_path()
        .with_file_name("sessions")
        .join(format!("{id}.json")))
}

#[derive(Serialize)]
pub struct Summary {
    pub id: String,
    pub title: String,
    pub started_at: u64,
    pub ended_at: Option<u64>,
    pub source_language: String,
    pub target_language: String,
    pub entry_count: usize,
}

pub fn list() -> Result<Vec<Summary>, String> {
    let dir = config::config_path().with_file_name("sessions");
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut records = Vec::new();
    for file in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let file = file.map_err(|e| e.to_string())?;
        if file.path().extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        let r: Record = match std::fs::read(file.path())
            .ok()
            .and_then(|data| serde_json::from_slice(&data).ok())
        {
            Some(r) => r,
            None => {
                log::warn!("Unreadable history file: {:?}", file.path());
                continue;
            }
        };
        records.push(Summary {
            id: r.id,
            title: r.title,
            started_at: r.started_at,
            ended_at: r.ended_at,
            source_language: r.source_language,
            target_language: r.target_language,
            entry_count: r.entries.len(),
        });
    }
    records.sort_by_key(|r| std::cmp::Reverse(r.started_at));
    Ok(records)
}

pub fn read(id: &str) -> Result<Record, String> {
    let mut record: Record =
        serde_json::from_slice(&std::fs::read(path(id)?).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    mark_interrupted(&mut record);
    Ok(record)
}
fn mark_interrupted(record: &mut Record) {
    for entry in &mut record.entries {
        if entry.translation.is_empty() && entry.error.is_none() {
            entry.error = Some("Перевод был прерван. Исходный текст сохранён.".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crashed_session_preserves_sources_and_marks_pending_entries() {
        let mut r = Record::new(&config::Config::default(), "Test".into());
        r.entries.push(Entry {
            id: 1,
            timestamp_ms: 0,
            source: "Untranslated".into(),
            translation: String::new(),
            error: None,
        });
        mark_interrupted(&mut r);
        assert_eq!(r.entries[0].source, "Untranslated");
        assert!(r.entries[0].error.is_some());
    }
    #[test]
    fn rejects_path_traversal() {
        for id in ["../config", "C:\\test", "", "1/2", ".."] {
            assert!(path(id).is_err());
        }
        assert!(path("123-456").is_ok());
    }
}
