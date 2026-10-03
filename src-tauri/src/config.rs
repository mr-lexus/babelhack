use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

pub static CONFIG_LOCK: Mutex<()> = Mutex::new(());
pub const OPENAI_MODEL_DEFAULT: &str = "gpt-4.1-mini";
pub const DEEPGRAM_MODEL_DEFAULT: &str = "nova-3";
pub const LANGUAGES: &[&str] = &[
    "en", "ru", "uk", "de", "fr", "es", "pt", "it", "nl", "pl", "ro", "ja", "ko", "zh",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deepgram_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai_key: Option<String>,
    pub openai_model: String,
    pub deepgram_model: String,
    pub source_language: String,
    pub target_language: String,
    pub glossary: String,
    pub save_history: bool,
    pub close_to_tray: bool,
    pub minimize_to_tray: bool,
    pub overlay_lines: usize,
    pub overlay_opacity: f64,
    pub overlay_font_size: f64,
    pub overlay_history_line_height: f64,
    pub overlay_history_spacing: f64,
    pub history_font_size: f64,
    pub history_line_height: f64,
    pub history_entry_spacing: f64,

    pub overlay_line_height: f64,
    pub overlay_realtime_font_size: f64,
    pub overlay_bg_color: String,
    pub overlay_text_color: String,
    pub overlay_history_color: String,
    pub overlay_show_source: bool,
    pub audio_device: Option<String>,
    pub audio_device_name: Option<String>,
    pub overlay_x: Option<f64>,
    pub overlay_y: Option<f64>,
    pub overlay_width: Option<f64>,
    pub overlay_height: Option<f64>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            deepgram_key: None,
            openai_key: None,
            openai_model: OPENAI_MODEL_DEFAULT.into(),
            deepgram_model: DEEPGRAM_MODEL_DEFAULT.into(),
            source_language: "en".into(),
            target_language: "ru".into(),
            glossary: String::new(),
            save_history: true,
            close_to_tray: !cfg!(target_os = "linux"),
            minimize_to_tray: false,
            overlay_lines: 3,
            overlay_opacity: 0.88,
            overlay_font_size: 16.0,
            overlay_history_line_height: 1.2,
            overlay_history_spacing: 6.0,
            history_font_size: 13.0,
            history_line_height: 1.25,
            history_entry_spacing: 8.0,

            overlay_line_height: 1.25,
            overlay_realtime_font_size: 26.0,
            overlay_bg_color: "#111827".into(),
            overlay_text_color: "#f8fafc".into(),
            overlay_history_color: "#a9b7cc".into(),
            overlay_show_source: true,
            audio_device: None,
            audio_device_name: None,
            overlay_x: None,
            overlay_y: None,
            overlay_width: None,
            overlay_height: None,
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if !LANGUAGES.contains(&self.source_language.as_str())
            || !LANGUAGES.contains(&self.target_language.as_str())
        {
            return Err("Неподдерживаемый язык".into());
        }
        if self.openai_model.trim().is_empty() || self.openai_model.len() > 120 {
            return Err("Укажите корректную модель OpenAI".into());
        }
        if !["nova-2", "nova-3"].contains(&self.deepgram_model.as_str()) {
            return Err("Неподдерживаемая модель Deepgram".into());
        }
        if self.glossary.chars().count() > 4000 {
            return Err("Словарь: не более 4000 символов".into());
        }
        for (value, min, max) in [
            (self.overlay_opacity, 0.15, 1.0),
            (self.overlay_font_size, 12.0, 48.0),
            (self.overlay_history_line_height, 1.0, 2.0),
            (self.overlay_history_spacing, 0.0, 24.0),
            (self.history_font_size, 11.0, 24.0),
            (self.history_line_height, 1.0, 2.0),
            (self.history_entry_spacing, 2.0, 24.0),
            (self.overlay_line_height, 1.0, 2.0),
            (self.overlay_realtime_font_size, 14.0, 64.0),
        ] {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err("Настройки субтитров вне допустимого диапазона".into());
            }
        }
        if !(1..=8).contains(&self.overlay_lines) {
            return Err("Допустимо от 1 до 8 строк".into());
        }
        for c in [
            &self.overlay_bg_color,
            &self.overlay_text_color,
            &self.overlay_history_color,
        ] {
            if c.len() != 7 || !c.starts_with('#') || !c[1..].bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err("Цвет должен иметь формат #RRGGBB".into());
            }
        }
        Ok(())
    }
}

pub fn config_path() -> std::path::PathBuf {
    // Optional portable/test profile; affects settings and transcripts, not the OS credential store.
    if let Some(dir) = std::env::var_os("BABELHACK_DATA_DIR")
        .or_else(|| std::env::var_os("INTERVIEW_TRANSLATOR_DATA_DIR"))
    {
        let dir = std::path::PathBuf::from(dir);
        if dir.is_absolute() {
            return dir.join("config.json");
        }
    }
    // Keep the pre-Babel Hack profile so upgrades retain settings and history.
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("interview-translator")
        .join("config.json")
}

fn decode_config(data: &[u8]) -> Result<Config, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_slice(data)?;
    let mut cfg: Config = serde_json::from_value(value.clone())?;
    // Migrate the old stock size once; preserve user-selected sizes.
    if value.get("overlay_history_line_height").is_none() && cfg.overlay_font_size == 20.0 {
        cfg.overlay_font_size = 16.0;
    }
    Ok(cfg)
}

pub fn load() -> Config {
    match std::fs::read(config_path()) {
        Ok(data) => decode_config(&data).unwrap_or_else(|e| {
            log::error!("Invalid config: {e}");
            Config::default()
        }),
        Err(_) => Config::default(),
    }
}

pub fn write_json(path: &Path, value: &impl Serialize) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)?;
        }
        #[cfg(not(unix))]
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = std::fs::File::create(&tmp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(tmp, path)
}

pub fn save(cfg: &Config) -> std::io::Result<()> {
    write_json(&config_path(), cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_config_gets_new_defaults() {
        let c: Config = serde_json::from_str(r#"{"overlay_lines":5}"#).unwrap();
        assert_eq!(c.target_language, "ru");
        assert_eq!(c.overlay_lines, 5);
        assert_eq!(c.overlay_font_size, 16.0);
        assert_eq!(c.history_line_height, 1.25);
        assert_eq!(
            decode_config(br#"{"overlay_font_size":20}"#)
                .unwrap()
                .overlay_font_size,
            16.0
        );
        assert_eq!(
            decode_config(br#"{"overlay_font_size":24}"#)
                .unwrap()
                .overlay_font_size,
            24.0
        );
        assert_eq!(
            decode_config(br#"{"overlay_font_size":20,"overlay_history_line_height":1.2}"#)
                .unwrap()
                .overlay_font_size,
            20.0
        );
        assert!(c.validate().is_ok());
    }
    #[test]
    fn rejects_invalid_settings() {
        for c in [
            Config {
                overlay_opacity: f64::NAN,
                ..Default::default()
            },
            Config {
                overlay_bg_color: "red".into(),
                ..Default::default()
            },
            Config {
                source_language: "xx".into(),
                ..Default::default()
            },
        ] {
            assert!(c.validate().is_err());
        }
    }
    #[test]
    fn atomic_write_replaces_existing_file() {
        let dir = std::env::temp_dir().join(format!("translator-config-{}", std::process::id()));
        let path = dir.join("config.json");
        write_json(&path, &vec![1]).unwrap();
        write_json(&path, &vec![2]).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[\n  2\n]");
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
