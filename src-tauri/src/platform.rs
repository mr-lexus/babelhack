use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct PlatformInfo {
    pub os: &'static str,
    pub label: &'static str,
    pub credential_store: &'static str,
    pub audio_backend: &'static str,
    pub audio_help: &'static str,
    pub overlay_shortcut: &'static str,
    pub wayland: bool,
    pub overlay_transparency: bool,
}

pub fn info() -> PlatformInfo {
    let wayland = cfg!(target_os = "linux")
        && (std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s == "wayland"));
    let (label, credential_store, audio_backend, audio_help) = if cfg!(target_os = "macos") {
        ("macOS", "macOS Keychain", "CoreAudio Process Tap",
         "Нужна macOS 14.6 или новее. Разрешите приложению запись системного аудио в Системных настройках → Конфиденциальность и безопасность. При отказе разрешите доступ и перезапустите приложение. Микрофон не захватывается.")
    } else if cfg!(target_os = "linux") {
        ("Linux", "Secret Service (GNOME Keyring / KWallet)", "PulseAudio / PipeWire monitor",
         "Нужен работающий PulseAudio или PipeWire с pipewire-pulse. Захватывается monitor выбранного выхода, не микрофон. Для ключей нужен разблокированный Secret Service в пользовательской D-Bus-сессии.")
    } else {
        ("Windows", "Windows Credential Manager", "WASAPI loopback",
         "Выберите устройство, через которое слышите звонок. Захватывается системный звук этого выхода, не микрофон.")
    };
    PlatformInfo {
        os: std::env::consts::OS,
        label,
        credential_store,
        audio_backend,
        audio_help,
        overlay_shortcut: if cfg!(target_os = "macos") {
            "⌘ ⌥ T"
        } else {
            "Ctrl Alt T"
        },
        wayland,
        // macOS transparent WebViews require private APIs. Keep a solid readable overlay there.
        overlay_transparency: !cfg!(target_os = "macos"),
    }
}

pub fn shortcut() -> &'static str {
    if cfg!(target_os = "macos") {
        "super+alt+t"
    } else {
        "ctrl+alt+t"
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    #[test]
    fn glib_string_iterator_backport() {
        use glib::variant::ToVariant;
        let variant = vec!["alpha", "beta", "gamma"].to_variant();
        let mut iter = variant.array_iter_str().unwrap();
        assert_eq!(iter.next(), Some("alpha"));
        assert_eq!(iter.next_back(), Some("gamma"));
        assert_eq!(iter.next(), Some("beta"));
        assert_eq!(iter.next(), None);
        assert_eq!(variant.array_iter_str().unwrap().last(), Some("gamma"));
    }
}
