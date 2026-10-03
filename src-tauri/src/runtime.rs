use crate::{
    config::Config,
    history::{self, Record},
    metrics::Metrics,
};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex,
};
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
pub struct CaptionCue {
    pub id: u64,
    pub text: String,
    pub untranslated: bool,
}

#[derive(Clone, Serialize)]
pub struct View {
    pub version: u64,
    pub active: bool,
    pub demo: bool,
    pub source_language: String,
    pub target_language: String,
    pub save_history: bool,
    pub paused: bool,
    pub status: String,
    pub message: Option<String>,
    pub session_id: Option<String>,
    pub started_at: Option<u64>,
    pub source: String,
    pub current: String,
    pub translation_source: String,
    pub provisional: bool,
    pub cues: Vec<CaptionCue>,
    pub lines: Vec<String>,
    pub pending: usize,
    pub audio_level: f32,
    pub dropped_chunks: u32,
}
impl Default for View {
    fn default() -> Self {
        Self {
            version: 0,
            active: false,
            demo: false,
            source_language: "en".into(),
            target_language: "ru".into(),
            save_history: false,
            paused: false,
            status: "idle".into(),
            message: None,
            session_id: None,
            started_at: None,
            source: String::new(),
            current: String::new(),
            translation_source: String::new(),
            provisional: false,
            cues: vec![],
            lines: vec![],
            pending: 0,
            audio_level: 0.0,
            dropped_chunks: 0,
        }
    }
}
impl View {
    fn finish_caption(&mut self, id: u64, source: &str, result: Result<&str, &str>) {
        let (text, untranslated) = match result {
            Ok(text) => {
                if self
                    .message
                    .as_deref()
                    .is_some_and(|m| m.starts_with("Перевод:"))
                {
                    self.message = None;
                }
                (text, false)
            }
            Err(_) => {
                self.message = Some("Перевод: не удалось перевести фразу после попытки восстановления. Оригинал сохранён; следующие фразы продолжают переводиться.".into());
                (source, true)
            }
        };
        self.cues.push(CaptionCue {
            id,
            text: text.into(),
            untranslated,
        });
        if self.cues.len() > 8 {
            self.cues.remove(0);
        }
        self.lines.push(text.into());
        if self.lines.len() > 8 {
            self.lines.remove(0);
        }
        self.current.clear();
        self.provisional = false;
        self.translation_source.clear();
        self.pending = self.pending.saturating_sub(1);
    }
}

pub struct Runtime {
    pub app: AppHandle,
    pub view: Mutex<View>,
    pub record: Mutex<Record>,
    pub cfg: Config,
    pub metrics: Arc<Metrics>,
    pub paused: Arc<AtomicBool>,
    pub level: Arc<AtomicU32>,
    pub dropped: Arc<AtomicU32>,
}
impl Runtime {
    pub fn new(app: AppHandle, cfg: Config, title: String) -> Self {
        let record = Record::new(&cfg, title);
        let view = View {
            active: true,
            source_language: cfg.source_language.clone(),
            target_language: cfg.target_language.clone(),
            save_history: cfg.save_history,
            status: "connecting".into(),
            session_id: Some(record.id.clone()),
            started_at: Some(record.started_at),
            ..View::default()
        };
        Self {
            app,
            view: Mutex::new(view),
            record: Mutex::new(record),
            cfg,
            metrics: Arc::new(Metrics::new()),
            paused: Arc::new(AtomicBool::new(false)),
            level: Arc::new(AtomicU32::new(0)),
            dropped: Arc::new(AtomicU32::new(0)),
        }
    }
    pub fn update(&self, f: impl FnOnce(&mut View)) {
        let mut v = self.view.lock().unwrap();
        f(&mut v);
        v.version += 1;
        v.paused = self.paused.load(Ordering::Relaxed);
        let _ = self.app.emit("session-state", &*v);
        crate::tray::refresh(&self.app, &v);
    }
    pub fn snapshot(&self) -> View {
        let mut v = self.view.lock().unwrap().clone();
        v.audio_level = f32::from_bits(self.level.load(Ordering::Relaxed));
        v.dropped_chunks = self.dropped.load(Ordering::Relaxed);
        v
    }
    pub fn status(&self, status: &str, message: Option<String>) {
        self.update(|v| {
            v.status = status.into();
            v.message = message;
        });
    }
    pub fn add_source(&self, text: &str) -> u64 {
        let mut r = self.record.lock().unwrap();
        let id = r.entries.len() as u64 + 1;
        let timestamp_ms = history::now_ms().saturating_sub(r.started_at);
        r.entries.push(history::Entry {
            id,
            timestamp_ms,
            source: text.into(),
            translation: String::new(),
            error: None,
        });
        self.persist(&r);
        self.update(|v| v.pending += 1);
        let _ = self.app.emit("history-changed", ());
        id
    }
    pub fn complete(&self, id: u64, result: Result<String, String>) {
        let mut r = self.record.lock().unwrap();
        if let Some(e) = r.entries.iter_mut().find(|e| e.id == id) {
            match &result {
                Ok(text) => {
                    e.translation = text.clone();
                    e.error = None;
                }
                Err(error) => {
                    e.error = Some(error.clone());
                }
            }
            self.update(|v| {
                v.finish_caption(id, &e.source, result.as_deref().map_err(String::as_str))
            });
        }
        self.persist(&r);
        let _ = self.app.emit("history-changed", ());
    }
    pub fn persist(&self, r: &Record) {
        if self.cfg.save_history {
            if let Err(e) = r.save() {
                self.update(|v| v.message = Some(e));
            }
        }
    }
    pub fn finish(&self) {
        self.level.store(0, Ordering::Relaxed);
        let mut r = self.record.lock().unwrap();
        r.ended_at = Some(history::now_ms());
        for e in &mut r.entries {
            if e.translation.is_empty() && e.error.is_none() {
                e.error = Some("Сессия завершена до окончания перевода".into());
            }
        }
        self.persist(&r);
        self.paused.store(false, Ordering::Relaxed);
        self.update(|v| {
            v.active = false;
            v.status = "idle".into();
            v.current.clear();
            v.provisional = false;
            v.translation_source.clear();
            v.pending = 0;
            v.audio_level = 0.0;
        });
        let _ = self.app.emit("history-changed", ());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_translation_keeps_source_in_reading_order_and_next_success_recovers() {
        let mut v = View {
            pending: 2,
            current: "an incomplete draft".into(),
            ..View::default()
        };
        v.finish_caption(1, "The whole source sentence.", Err("timeout"));
        assert_eq!(v.cues[0].text, "The whole source sentence.");
        assert!(v.cues[0].untranslated);
        assert!(v.current.is_empty());
        assert!(v.message.is_some());
        assert_eq!(v.pending, 1);
        v.finish_caption(2, "Next sentence.", Ok("Следующее предложение."));
        assert!(v.message.is_none());
        assert_eq!(v.pending, 0);
        assert_eq!(v.cues.iter().map(|c| c.id).collect::<Vec<_>>(), vec![1, 2]);
        assert!(!v.cues[1].untranslated);
    }
    #[test]
    fn translation_success_does_not_clear_unrelated_status() {
        let mut v = View {
            message: Some("Connection lost".into()),
            ..View::default()
        };
        v.finish_caption(1, "Source", Ok("Translation"));
        assert_eq!(v.message.as_deref(), Some("Connection lost"));
    }
}
