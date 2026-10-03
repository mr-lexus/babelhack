mod audio;
mod config;
mod coordinator;
mod dg;
mod history;
#[cfg(target_os = "macos")]
mod macos_tap;
mod metrics;
mod platform;
mod runtime;
mod secure;
mod transcript;
mod translate;
mod tray;

use runtime::{Runtime, View};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{mpsc, watch};

pub struct AppState {
    session: tokio::sync::Mutex<Option<Session>>,
    current: Mutex<Option<Arc<Runtime>>>,
    closing: AtomicBool,
    close_to_tray: AtomicBool,
    minimize_to_tray: AtomicBool,
    tray_hidden: AtomicBool,
    shutdown_complete: AtomicBool,
}
struct Session {
    audio: Option<audio::AudioCapture>,
    audio_tx: mpsc::Sender<Vec<u8>>,
    stop: watch::Sender<bool>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    audio_task: Option<tokio::task::JoinHandle<()>>,
    rt: Arc<Runtime>,
    demo: bool,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.audio.take();
        let _ = self.stop.send(true);
        for task in &self.tasks {
            task.abort();
        }
        if let Some(task) = &self.audio_task {
            task.abort();
        }
    }
}
impl AppState {
    fn new() -> Self {
        Self {
            session: tokio::sync::Mutex::new(None),
            current: Mutex::new(None),
            closing: AtomicBool::new(false),
            close_to_tray: AtomicBool::new(true),
            minimize_to_tray: AtomicBool::new(false),
            tray_hidden: AtomicBool::new(false),
            shutdown_complete: AtomicBool::new(false),
        }
    }
}

#[derive(Serialize)]
struct ConfigStatus {
    #[serde(flatten)]
    config: config::Config,
    has_deepgram: bool,
    has_openai: bool,
    credential_error: Option<String>,
    platform: platform::PlatformInfo,
    tray_available: bool,
}
#[tauri::command]
async fn get_config() -> Result<ConfigStatus, String> {
    tauri::async_runtime::spawn_blocking(read_config)
        .await
        .map_err(|e| e.to_string())
}
fn read_config() -> ConfigStatus {
    let _guard = config::CONFIG_LOCK.lock().unwrap();
    let mut cfg = config::load();
    cfg.deepgram_key = None;
    cfg.openai_key = None;
    let (has_deepgram, has_openai, credential_error) = secure::status();
    ConfigStatus {
        config: cfg,
        has_deepgram,
        has_openai,
        credential_error,
        platform: platform::info(),
        tray_available: tray::AVAILABLE.load(Ordering::SeqCst),
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetConfigArgs {
    config: config::Config,
    deepgram_key: Option<String>,
    openai_key: Option<String>,
}
#[tauri::command]
async fn set_config(app: AppHandle, args: SetConfigArgs) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || save_config(app, args))
        .await
        .map_err(|e| e.to_string())?
}
fn save_config(app: AppHandle, args: SetConfigArgs) -> Result<(), String> {
    let _guard = config::CONFIG_LOCK.lock().unwrap();
    let old = config::load();
    let mut cfg = args.config;
    cfg.openai_model = cfg.openai_model.trim().into();
    cfg.validate()?;
    // Geometry belongs to the overlay; a stale settings form must not overwrite it.
    cfg.overlay_x = old.overlay_x;
    cfg.overlay_y = old.overlay_y;
    cfg.overlay_width = old.overlay_width;
    cfg.overlay_height = old.overlay_height;
    cfg.deepgram_key = old.deepgram_key;
    cfg.openai_key = old.openai_key;
    if let Some(key) = args.deepgram_key.filter(|s| !s.trim().is_empty()) {
        secure::set_deepgram_key(key.trim())?;
        cfg.deepgram_key = None;
    }
    if let Some(key) = args.openai_key.filter(|s| !s.trim().is_empty()) {
        secure::set_openai_key(key.trim())?;
        cfg.openai_key = None;
    }
    config::save(&cfg).map_err(|e| format!("Не удалось сохранить настройки: {e}"))?;
    app.state::<AppState>()
        .close_to_tray
        .store(cfg.close_to_tray, Ordering::SeqCst);
    app.state::<AppState>()
        .minimize_to_tray
        .store(cfg.minimize_to_tray, Ordering::SeqCst);
    let _ = app.emit("config-changed", ());
    Ok(())
}
#[tauri::command]
async fn list_output_devices() -> Result<Vec<audio::OutputDevice>, String> {
    tauri::async_runtime::spawn_blocking(audio::list_output_devices)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}
fn audio_monitor(
    mut errors: mpsc::UnboundedReceiver<String>,
    rt: Arc<Runtime>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Some(error) = errors.recv().await {
            rt.paused.store(true, Ordering::Relaxed);
            rt.level.store(0, Ordering::Relaxed);
            rt.status("error", Some(format!("Аудиоустройство отключено: {error}. Восстановите подключение и нажмите «Продолжить».")));
        }
    })
}
#[tauri::command]
async fn start_session(
    app: AppHandle,
    state: State<'_, AppState>,
    title: Option<String>,
    demo: Option<bool>,
) -> Result<(), String> {
    let mut guard = state.session.lock().await;
    if state.closing.load(Ordering::SeqCst) {
        return Err("Приложение завершает работу".into());
    }
    if guard.is_some() {
        return Err("Сессия уже запущена".into());
    }
    let mut cfg = config::load();
    let demo = demo.unwrap_or(false);
    if demo {
        cfg.source_language = "en".into();
        cfg.target_language = "ru".into();
        cfg.save_history = false;
    }
    cfg.validate()?;
    let keys = if demo {
        None
    } else {
        Some((
            secure::get_deepgram_key().ok_or("Добавьте ключ Deepgram в настройках")?,
            secure::get_openai_key().ok_or("Добавьте ключ OpenAI в настройках")?,
        ))
    };
    let title = title
        .unwrap_or_default()
        .trim()
        .chars()
        .take(120)
        .collect::<String>();
    let title = if title.is_empty() {
        if demo {
            "Знакомство с переводчиком".into()
        } else {
            "Новая сессия".into()
        }
    } else {
        title
    };
    let rt = Arc::new(Runtime::new(app.clone(), cfg, title));
    let (audio_tx, audio_rx) = mpsc::channel(50);
    let (stop, stop_rx) = watch::channel(false);
    let mut session = Session {
        audio: None,
        audio_tx: audio_tx.clone(),
        stop,
        tasks: vec![],
        audio_task: None,
        rt: rt.clone(),
        demo,
    };
    if let Some((dg_key, oa_key)) = keys {
        let (capture, errors) = audio::AudioCapture::start(
            audio_tx,
            rt.cfg.audio_device.as_deref(),
            rt.level.clone(),
            rt.dropped.clone(),
        )
        .map_err(|e| e.to_string())?;
        session.audio = Some(capture);
        session.audio_task = Some(audio_monitor(errors, rt.clone()));
        let (jobs_tx, jobs_rx) = mpsc::channel(32);
        let (previews_tx, previews_rx) = watch::channel(None);
        session.tasks.push(tokio::spawn(dg::run(
            dg_key,
            audio_rx,
            jobs_tx,
            previews_tx,
            rt.clone(),
            stop_rx,
        )));
        session.tasks.push(tokio::spawn(coordinator::run(
            jobs_rx,
            previews_rx,
            oa_key,
            rt.clone(),
        )));
    } else {
        session
            .tasks
            .push(tokio::spawn(run_demo(rt.clone(), stop_rx)));
    }
    rt.persist(&rt.record.lock().unwrap());
    *state.current.lock().unwrap() = Some(rt.clone());
    *guard = Some(session);
    rt.update(|v| v.demo = demo);
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.show();
    }
    Ok(())
}
async fn run_demo(rt: Arc<Runtime>, mut stop: watch::Receiver<bool>) {
    rt.status("listening", None);
    let phrases = [
        (
            "Thanks for joining today. Could you tell me about yourself?",
            "Спасибо, что присоединились. Расскажите немного о себе.",
        ),
        (
            "I build reliable products and enjoy solving difficult problems.",
            "Я создаю надёжные продукты и люблю решать сложные задачи.",
        ),
        (
            "How do you approach a project with a tight deadline?",
            "Как вы подходите к проекту с жёсткими сроками?",
        ),
        (
            "First, I identify the most important outcome and break the work into small steps.",
            "Сначала определяю главный результат и разбиваю работу на небольшие шаги.",
        ),
        (
            "Clear communication helps the team make better decisions.",
            "Понятная коммуникация помогает команде принимать лучшие решения.",
        ),
    ];
    for (source, translated) in phrases {
        let source_words: Vec<_> = source.split_whitespace().collect();
        let target_words: Vec<_> = translated.split_whitespace().collect();
        for step in 0..source_words.len() + 4 {
            loop {
                if *stop.borrow() {
                    return;
                }
                if !rt.paused.load(Ordering::Relaxed) {
                    break;
                }
                tokio::select! { _ = tokio::time::sleep(Duration::from_millis(100)) => {}, _ = stop.changed() => return }
            }
            let spoken = (step + 1).min(source_words.len());
            let translated_count = (step.saturating_sub(1) * target_words.len()
                / source_words.len())
            .min(target_words.len());
            rt.update(|v| {
                v.source = source_words[..spoken].join(" ");
                v.current = target_words[..translated_count].join(" ");
                v.translation_source = v.source.clone();
                v.provisional = true;
            });
            tokio::select! { _ = tokio::time::sleep(Duration::from_millis(210)) => {}, _ = stop.changed() => return }
        }
        let id = rt.add_source(source);
        rt.complete(id, Ok(translated.into()));
        tokio::select! { _ = tokio::time::sleep(Duration::from_secs(2)) => {}, _ = stop.changed() => return }
        rt.update(|v| v.source.clear());
    }
    rt.status(
        "listening",
        Some("Демонстрация завершена. Нажмите «Завершить», чтобы начать свою сессию.".into()),
    );
}
#[tauri::command]
async fn pause_session(state: State<'_, AppState>) -> Result<(), String> {
    let mut guard = state.session.lock().await;
    let s = guard.as_mut().ok_or("Нет активной сессии")?;
    s.rt.paused.store(true, Ordering::Relaxed);
    s.audio.take();
    if let Some(task) = s.audio_task.take() {
        task.abort();
        let _ = task.await;
    }
    s.rt.level.store(0, Ordering::Relaxed);
    s.rt.update(|_| {});
    Ok(())
}
#[tauri::command]
async fn resume_session(state: State<'_, AppState>) -> Result<(), String> {
    let mut guard = state.session.lock().await;
    let s = guard.as_mut().ok_or("Нет активной сессии")?;
    if !s.rt.paused.load(Ordering::Relaxed) {
        return Ok(());
    }
    if s.rt.snapshot().status == "error" && s.tasks.first().is_some_and(|h| h.is_finished()) {
        return Err("Остановите сессию и запустите заново после исправления настроек".into());
    }
    if !s.demo {
        s.audio.take();
        if let Some(task) = s.audio_task.take() {
            task.abort();
            let _ = task.await;
        }
        let (capture, errors) = audio::AudioCapture::start(
            s.audio_tx.clone(),
            s.rt.cfg.audio_device.as_deref(),
            s.rt.level.clone(),
            s.rt.dropped.clone(),
        )
        .map_err(|e| e.to_string())?;
        s.audio = Some(capture);
        s.audio_task = Some(audio_monitor(errors, s.rt.clone()));
    }
    s.rt.paused.store(false, Ordering::Relaxed);
    s.rt.status("listening", None);
    Ok(())
}
#[tauri::command]
async fn stop_session(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let mut guard = state.session.lock().await;
    if let Some(mut s) = guard.take() {
        s.audio.take();
        s.rt.status("stopping", None);
        if let Some(task) = s.audio_task.take() {
            task.abort();
            let _ = task.await;
        }
        let _ = s.stop.send(true);
        // Drain final recognition + queued translations within a bounded shutdown.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        for mut task in s.tasks.drain(..) {
            if tokio::time::timeout_at(deadline, &mut task).await.is_err() {
                task.abort();
                let _ = task.await;
            }
        }
        s.rt.finish();
    }
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.hide();
    }
    Ok(())
}
#[tauri::command]
fn get_session_state(state: State<'_, AppState>) -> View {
    state
        .current
        .lock()
        .unwrap()
        .as_ref()
        .map(|r| r.snapshot())
        .unwrap_or_default()
}
#[tauri::command]
fn get_metrics(state: State<'_, AppState>) -> metrics::MetricsSnapshot {
    state
        .current
        .lock()
        .unwrap()
        .as_ref()
        .map(|r| r.metrics.snapshot())
        .unwrap_or_else(|| metrics::Metrics::new().snapshot())
}
#[tauri::command]
fn list_sessions() -> Result<Vec<history::Summary>, String> {
    history::list()
}
#[tauri::command]
fn get_session(id: String, state: State<'_, AppState>) -> Result<history::Record, String> {
    if let Some(rt) = state.current.lock().unwrap().as_ref() {
        let record = rt.record.lock().unwrap();
        if record.id == id {
            return Ok(record.clone());
        }
    }
    history::read(&id)
}
#[tauri::command]
fn delete_session(id: String, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(rt) = state.current.lock().unwrap().as_ref() {
        if rt.snapshot().active && rt.record.lock().unwrap().id == id {
            return Err("Сначала завершите сессию".into());
        }
    }
    std::fs::remove_file(history::path(&id)?).map_err(|e| e.to_string())
}
#[derive(Deserialize)]
struct Geometry {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
#[tauri::command]
fn save_overlay_geometry(args: Geometry) -> Result<(), String> {
    if ![args.x, args.y, args.width, args.height]
        .iter()
        .all(|v| v.is_finite())
        || args.width < 280.0
        || args.height < 120.0
    {
        return Err("Некорректная геометрия окна".into());
    }
    let _guard = config::CONFIG_LOCK.lock().unwrap();
    let mut cfg = config::load();
    cfg.overlay_x = Some(args.x);
    cfg.overlay_y = Some(args.y);
    cfg.overlay_width = Some(args.width);
    cfg.overlay_height = Some(args.height);
    config::save(&cfg).map_err(|e| e.to_string())
}
#[tauri::command]
fn toggle_overlay(app: AppHandle) -> Result<(), String> {
    let w = app
        .get_webview_window("overlay")
        .ok_or("Окно субтитров недоступно")?;
    if w.is_visible().map_err(|e| e.to_string())? {
        w.hide()
    } else {
        w.show()
    }
    .map_err(|e| e.to_string())
}
#[tauri::command]
fn reset_overlay(app: AppHandle) -> Result<(), String> {
    let w = app
        .get_webview_window("overlay")
        .ok_or("Окно субтитров недоступно")?;
    w.set_size(tauri::LogicalSize::new(640.0, 260.0))
        .map_err(|e| e.to_string())?;
    if !platform::info().wayland {
        w.center().map_err(|e| e.to_string())?;
    }
    w.show().map_err(|e| e.to_string())
}
#[tauri::command]
async fn export_session(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    format: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let record = get_session(id, state)?;
    let content = export_text(&record, &format)?;
    let name = format!("babelhack-{}.{}", record.id, format);
    let ext = format.clone();
    let file = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("Transcript", &[&ext])
            .set_file_name(&name)
            .blocking_save_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(file) = file else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(Some(path.display().to_string()))
}
fn export_text(r: &history::Record, format: &str) -> Result<String, String> {
    match format {
        "json" => serde_json::to_string_pretty(r).map_err(|e| e.to_string()),
        "txt" | "md" => Ok(format!(
            "{}\n{} → {}\n\n{}",
            r.title,
            r.source_language,
            r.target_language,
            r.entries
                .iter()
                .map(|e| format!(
                    "[{}:{:02}]\n{}\n{}\n",
                    e.timestamp_ms / 60000,
                    e.timestamp_ms / 1000 % 60,
                    e.source,
                    if e.translation.is_empty() {
                        e.error.as_deref().unwrap_or("Перевод не завершён")
                    } else {
                        &e.translation
                    }
                ))
                .collect::<Vec<_>>()
                .join("\n")
        )),
        _ => Err("Допустимые форматы: txt, md, json".into()),
    }
}
pub fn run() {
    init_logging();
    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            tray::tray_action,
            tray::close_main_window,
            tray::minimize_main_window,
            get_config,
            set_config,
            list_output_devices,
            start_session,
            pause_session,
            resume_session,
            stop_session,
            get_session_state,
            get_metrics,
            list_sessions,
            get_session,
            delete_session,
            save_overlay_geometry,
            toggle_overlay,
            reset_overlay,
            export_session
        ])
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let cfg = config::load();
            app.state::<AppState>()
                .close_to_tray
                .store(cfg.close_to_tray, Ordering::SeqCst);
            app.state::<AppState>()
                .minimize_to_tray
                .store(cfg.minimize_to_tray, Ordering::SeqCst);
            if let Err(e) = tray::setup(app.handle()) {
                log::warn!("Tray unavailable; normal window close/minimize retained: {e}");
            }
            let migration_app = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                secure::migrate_legacy_keys();
                let _ = migration_app.emit("config-changed", ());
            });
            use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
            if !platform::info().wayland
                && app
                    .handle()
                    .plugin(tauri_plugin_global_shortcut::Builder::new().build())
                    .map_err(|e| log::warn!("Global shortcuts unavailable: {e}"))
                    .is_ok()
            {
                if let Err(e) =
                    app.global_shortcut()
                        .on_shortcut(platform::shortcut(), |app, _, event| {
                            if event.state == ShortcutState::Pressed {
                                let _ = toggle_overlay(app.clone());
                            }
                        })
                {
                    log::warn!("Hotkey unavailable: {e}");
                }
            }
            let mut builder = tauri::WebviewWindowBuilder::new(
                app,
                "overlay",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("Babel Hack · Субтитры")
            .inner_size(640.0, 260.0)
            .min_inner_size(280.0, 120.0)
            .decorations(false)
            // Native shadows add a white frame/second corner on Windows.
            // The overlay owns its rounded outline in CSS.
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(true)
            .visible(false);
            #[cfg(not(target_os = "macos"))]
            {
                builder = builder.transparent(true);
            }
            if let (Some(w), Some(h)) = (cfg.overlay_width, cfg.overlay_height) {
                if w.is_finite() && h.is_finite() {
                    builder = builder.inner_size(w.clamp(280.0, 2000.0), h.clamp(120.0, 1200.0));
                }
            }
            let overlay = builder.build()?;
            if !platform::info().wayland {
                if let (Some(x), Some(y)) = (cfg.overlay_x, cfg.overlay_y) {
                    // Restore only positions that intersect a connected monitor (logical coordinates).
                    let visible = overlay.available_monitors()?.iter().any(|m| {
                        let pos = m.position().to_logical::<f64>(m.scale_factor());
                        let size = m.size().to_logical::<f64>(m.scale_factor());
                        x >= pos.x
                            && y >= pos.y
                            && x + 80.0 < pos.x + size.width
                            && y + 40.0 < pos.y + size.height
                    });
                    if visible {
                        let _ = overlay.set_position(tauri::LogicalPosition::new(x, y));
                    } else {
                        let _ = overlay.center();
                    }
                } else {
                    let _ = overlay.center();
                }
            }
            let main = app.get_webview_window("main").expect("main window");
            let handle = app.handle().clone();
            main.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if let Err(e) = tray::close_main_window(handle.clone()) {
                        let _ = handle.emit("tray-error", e);
                    }
                }
                if let tauri::WindowEvent::Resized(_) = event {
                    let state = handle.state::<AppState>();
                    if state.minimize_to_tray.load(Ordering::SeqCst)
                        && !state.tray_hidden.load(Ordering::SeqCst)
                        && handle
                            .get_webview_window("main")
                            .is_some_and(|w| w.is_minimized().unwrap_or(false))
                    {
                        let _ = tray::hide_main(&handle);
                    }
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building application")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if matches!(event, tauri::RunEvent::Reopen { .. }) {
                let _ = tray::show_main(app);
            }
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let state = app.state::<AppState>();
                // Cmd+Q and OS Quit must drain owned requests just like closing the main window.
                if state.shutdown_complete.load(Ordering::SeqCst) {
                    return;
                }
                api.prevent_exit();
                begin_exit(app);
            }
        });
}
fn begin_exit(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.closing.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = app.emit("app-exiting", ());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = stop_session(app.clone(), app.state::<AppState>()).await;
        app.state::<AppState>()
            .shutdown_complete
            .store(true, Ordering::SeqCst);
        app.exit(0);
    });
}

fn init_logging() {
    let path = config::config_path().with_file_name("app.log");
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Keep diagnostic logs bounded; transcript content and credentials are never logged.
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 2_000_000) {
        let _ = std::fs::rename(&path, path.with_extension("previous.log"));
    }
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        builder.target(env_logger::Target::Pipe(Box::new(file)));
    }
    let _ = builder.try_init();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn export_preserves_unicode_and_failed_source() {
        let mut r = history::Record::new(&config::Config::default(), "Тест".into());
        r.entries.push(history::Entry {
            id: 1,
            timestamp_ms: 65000,
            source: "Hello".into(),
            translation: "Привет".into(),
            error: None,
        });
        let text = export_text(&r, "txt").unwrap();
        assert!(text.contains("[1:05]"));
        assert!(text.contains("Привет"));
        assert!(export_text(&r, "exe").is_err());
        assert!(serde_json::from_str::<history::Record>(&export_text(&r, "json").unwrap()).is_ok());
    }
}
