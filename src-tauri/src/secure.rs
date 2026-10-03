use keyring::Entry;

// Compatibility namespace: renaming it would orphan existing OS credentials.
const SERVICE: &str = "interview-translator";
const DG_TARGET: &str = "deepgram_api_key";
const OA_TARGET: &str = "openai_api_key";

fn entry(target: &str) -> keyring::Result<Entry> {
    Entry::new(SERVICE, target)
}

pub fn get_deepgram_key() -> Option<String> {
    match entry(DG_TARGET).and_then(|e| e.get_password()) {
        Ok(k) => Some(k),
        Err(e) => {
            log::debug!("keyring: no deepgram key: {e}");
            None
        }
    }
}

pub fn set_deepgram_key(k: &str) -> Result<(), String> {
    entry(DG_TARGET)
        .and_then(|e| e.set_password(k))
        .map(|()| log::info!("keyring: deepgram key saved"))
        .map_err(|e| {
            log::error!("keyring: failed to save deepgram key: {e}");
            format!(
                "Не удалось сохранить ключ Deepgram в {}: {e}",
                crate::platform::info().credential_store
            )
        })
}

pub fn get_openai_key() -> Option<String> {
    match entry(OA_TARGET).and_then(|e| e.get_password()) {
        Ok(k) => Some(k),
        Err(e) => {
            log::debug!("keyring: no openai key: {e}");
            None
        }
    }
}

pub fn set_openai_key(k: &str) -> Result<(), String> {
    entry(OA_TARGET)
        .and_then(|e| e.set_password(k))
        .map(|()| log::info!("keyring: openai key saved"))
        .map_err(|e| {
            log::error!("keyring: failed to save openai key: {e}");
            format!(
                "Не удалось сохранить ключ OpenAI в {}: {e}",
                crate::platform::info().credential_store
            )
        })
}

pub fn status() -> (bool, bool, Option<String>) {
    fn available(target: &str) -> Result<bool, String> {
        match entry(target).and_then(|e| e.get_password()) {
            Ok(value) => Ok(!value.trim().is_empty()),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(format!(
                "Хранилище ключей недоступно ({}): {e}. Разблокируйте его и повторите сохранение.",
                crate::platform::info().credential_store
            )),
        }
    }
    let dg = available(DG_TARGET);
    let oa = available(OA_TARGET);
    let error = dg.as_ref().err().or_else(|| oa.as_ref().err()).cloned();
    (dg.unwrap_or(false), oa.unwrap_or(false), error)
}

fn migrated_config(
    cfg: &crate::config::Config,
    mut save_dg: impl FnMut(&str) -> Result<(), String>,
    mut save_oa: impl FnMut(&str) -> Result<(), String>,
) -> crate::config::Config {
    let mut migrated = cfg.clone();
    if let Some(k) = cfg.deepgram_key.as_deref().filter(|k| !k.is_empty()) {
        if save_dg(k).is_ok() {
            migrated.deepgram_key = None;
        }
    }
    if let Some(k) = cfg.openai_key.as_deref().filter(|k| !k.is_empty()) {
        if save_oa(k).is_ok() {
            migrated.openai_key = None;
        }
    }
    migrated
}
pub fn migrate_legacy_keys() {
    let _guard = crate::config::CONFIG_LOCK.lock().unwrap();
    let cfg = crate::config::load();
    let migrated = migrated_config(&cfg, set_deepgram_key, set_openai_key);
    if migrated.deepgram_key != cfg.deepgram_key || migrated.openai_key != cfg.openai_key {
        if let Err(e) = crate::config::save(&migrated) {
            log::error!("Credential migration config write failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_credential_migration_keeps_failed_key() {
        let cfg = crate::config::Config {
            deepgram_key: Some("dg".into()),
            openai_key: Some("oa".into()),
            ..Default::default()
        };
        let migrated = migrated_config(&cfg, |_| Ok(()), |_| Err("unavailable".into()));
        assert!(migrated.deepgram_key.is_none());
        assert_eq!(migrated.openai_key.as_deref(), Some("oa"));
        let migrated = migrated_config(&cfg, |_| Err("unavailable".into()), |_| Ok(()));
        assert_eq!(migrated.deepgram_key.as_deref(), Some("dg"));
        assert!(migrated.openai_key.is_none());
    }
    #[test]
    #[ignore = "requires an unlocked platform credential store"]
    fn keyring_round_trip() {
        let e = Entry::new(SERVICE, "test_round_trip").unwrap();
        e.set_password("test-value").unwrap();
        let v = e.get_password().unwrap();
        assert_eq!(v, "test-value");
        let _ = e.delete_credential();
    }
}
