use crate::{runtime::View, AppState};
use serde::Deserialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

pub static AVAILABLE: AtomicBool = AtomicBool::new(false);
struct TrayState {
    play: MenuItem<tauri::Wry>,
    stop: MenuItem<tauri::Wry>,
    status: MenuItem<tauri::Wry>,
    last: Mutex<Option<(bool, bool, bool)>>,
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Show,
    Hide,
    Overlay,
    PlayPause,
    Stop,
    Settings,
    Quit,
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "tray_show", "Показать окно", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "tray_hide", "Свернуть в трей", true, None::<&str>)?;
    let overlay = MenuItem::with_id(
        app,
        "tray_overlay",
        "Показать / скрыть субтитры",
        true,
        None::<&str>,
    )?;
    let play = MenuItem::with_id(app, "tray_play", "Начать перевод", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "tray_stop", "Завершить сессию", false, None::<&str>)?;
    let settings = MenuItem::with_id(app, "tray_settings", "Настройки", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray_quit", "Выйти из приложения", true, None::<&str>)?;
    let status = MenuItem::with_id(app, "tray_status", "Готов к работе", false, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &status, &sep1, &show, &hide, &overlay, &play, &stop, &sep2, &settings, &quit,
        ],
    )?;
    let mut builder = TrayIconBuilder::with_id("babelhack")
        .menu(&menu)
        .tooltip("Babel Hack")
        // Linux only supports the menu; Windows left click restores the window.
        .show_menu_on_left_click(!cfg!(target_os = "windows"))
        .on_menu_event(|app, event| {
            let action = match event.id.as_ref() {
                "tray_show" => Action::Show,
                "tray_hide" => Action::Hide,
                "tray_overlay" => Action::Overlay,
                "tray_play" => Action::PlayPause,
                "tray_stop" => Action::Stop,
                "tray_settings" => Action::Settings,
                "tray_quit" => Action::Quit,
                _ => return,
            };
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = tray_action(app.clone(), action).await {
                    let _ = show_main(&app);
                    let _ = app.emit("tray-error", e);
                }
            });
        })
        .on_tray_icon_event(|tray, event| {
            if cfg!(target_os = "windows")
                && matches!(
                    event,
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    }
                )
            {
                let _ = show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    app.manage(TrayState {
        play,
        stop,
        status,
        last: Mutex::new(None),
    });
    AVAILABLE.store(true, Ordering::SeqCst);
    refresh(app, &View::default());
    Ok(())
}

// Called on session changes. Only menu state transitions touch native widgets;
// streaming words and audio levels do not rebuild the tray menu.
pub fn refresh(app: &AppHandle, view: &View) {
    let Some(tray) = app.try_state::<TrayState>() else {
        return;
    };
    let flags = (view.active, view.paused, view.status == "stopping");
    let mut last = tray.last.lock().unwrap();
    if *last == Some(flags) {
        return;
    }
    *last = Some(flags);
    drop(last);
    let (play, status, can_stop) = menu_state(flags.0, flags.1, flags.2);
    let _ = tray.play.set_text(play);
    let _ = tray.play.set_enabled(!flags.2);
    let _ = tray.stop.set_enabled(can_stop);
    let _ = tray.status.set_text(status);
}
fn menu_state(active: bool, paused: bool, stopping: bool) -> (&'static str, &'static str, bool) {
    if stopping {
        ("Завершение…", "Завершение сессии…", false)
    } else if !active {
        ("Начать перевод", "Готов к работе", false)
    } else if paused {
        ("Продолжить перевод", "Перевод на паузе", true)
    } else {
        ("Приостановить перевод", "Перевод работает", true)
    }
}

pub fn show_main(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("Главное окно недоступно")?;
    #[cfg(target_os = "macos")]
    app.set_dock_visibility(true).map_err(|e| e.to_string())?;
    window.unminimize().map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    app.state::<AppState>()
        .tray_hidden
        .store(false, Ordering::SeqCst);
    Ok(())
}
pub fn hide_main(app: &AppHandle) -> Result<(), String> {
    if !AVAILABLE.load(Ordering::SeqCst) {
        return Err("Системный трей недоступен. Окно оставлено открытым.".into());
    }
    let state = app.state::<AppState>();
    state.tray_hidden.store(true, Ordering::SeqCst);
    let result = (|| {
        let window = app
            .get_webview_window("main")
            .ok_or("Главное окно недоступно")?;
        window.hide().map_err(|e| e.to_string())?;
        #[cfg(target_os = "macos")]
        app.set_dock_visibility(false).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = show_main(app);
    }
    result
}

#[tauri::command]
pub async fn tray_action(app: AppHandle, action: Action) -> Result<(), String> {
    if app.state::<AppState>().closing.load(Ordering::SeqCst) {
        return Ok(());
    }
    match action {
        Action::Show => show_main(&app),
        Action::Hide => hide_main(&app),
        Action::Overlay => crate::toggle_overlay(app),
        Action::Settings => {
            show_main(&app)?;
            app.emit("navigate", "settings").map_err(|e| e.to_string())
        }
        Action::PlayPause => {
            let v = crate::get_session_state(app.state::<AppState>());
            if !v.active {
                crate::start_session(app.clone(), app.state::<AppState>(), None, None).await
            } else if v.paused {
                crate::resume_session(app.state::<AppState>()).await
            } else {
                crate::pause_session(app.state::<AppState>()).await
            }
        }
        Action::Stop => crate::stop_session(app.clone(), app.state::<AppState>()).await,
        Action::Quit => {
            crate::begin_exit(&app);
            Ok(())
        }
    }
}

#[tauri::command]
pub fn close_main_window(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    if state.close_to_tray.load(Ordering::SeqCst) && AVAILABLE.load(Ordering::SeqCst) {
        hide_main(&app)
    } else {
        crate::begin_exit(&app);
        Ok(())
    }
}
#[tauri::command]
pub fn minimize_main_window(app: AppHandle) -> Result<(), String> {
    if app
        .state::<AppState>()
        .minimize_to_tray
        .load(Ordering::SeqCst)
        && AVAILABLE.load(Ordering::SeqCst)
    {
        hide_main(&app)
    } else {
        app.get_webview_window("main")
            .ok_or("Главное окно недоступно")?
            .minimize()
            .map_err(|e| e.to_string())
    }
}
