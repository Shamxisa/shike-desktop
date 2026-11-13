mod ai;
mod audio;
mod commands;
mod models;
mod scheduler;
mod storage;
mod windows;

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{mpsc::Sender, Arc, Mutex},
    time::Duration,
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};
use tokio_util::sync::CancellationToken;

pub type AppResult<T> = Result<T, String>;

pub struct AppState {
    pub store: Mutex<storage::Store>,
    pub media: PathBuf,
    pub audio: Sender<audio::AudioCommand>,
    pub client: reqwest::Client,
    pub cancellations: Arc<Mutex<HashMap<String, CancellationToken>>>,
    pub exit_waiting: Mutex<Option<HashSet<String>>>,
}

fn request_exit(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        app.exit(0);
        return;
    };
    let windows = app
        .webview_windows()
        .keys()
        .cloned()
        .collect::<HashSet<_>>();
    if windows.is_empty() {
        app.exit(0);
        return;
    }
    if let Ok(mut waiting) = state.exit_waiting.lock() {
        if waiting.is_some() {
            return;
        }
        *waiting = Some(windows);
    }
    let _ = app.emit("app-exiting", ());
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if args.iter().any(|argument| argument == "--quit") {
                request_exit(app);
                return;
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if std::env::args().any(|argument| argument == "--quit") {
                app.handle().exit(0);
                return Ok(());
            }
            let path = app.path().app_data_dir()?;
            let media = path.join("media");
            std::fs::create_dir_all(&media)?;
            let store = storage::Store::open(&path).map_err(std::io::Error::other)?;
            let settings = store.settings.clone();
            let open_notes: Vec<String> = store
                .notes
                .iter()
                .filter(|note| note.open)
                .map(|note| note.id.clone())
                .collect();
            let client = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .build()?;
            let audio = audio::start(app.handle().clone());
            app.manage(AppState {
                store: Mutex::new(store),
                media,
                audio,
                client,
                cancellations: Arc::new(Mutex::new(HashMap::new())),
                exit_waiting: Mutex::new(None),
            });
            let show = MenuItem::with_id(app, "show", "显示时刻", true, None::<&str>)?;
            let pin = MenuItem::with_id(app, "pin", "切换置顶", true, None::<&str>)?;
            let exit = MenuItem::with_id(app, "exit", "退出时刻", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &pin, &exit])?;
            let mut tray = TrayIconBuilder::new()
                .tooltip("时刻 · 闹钟、倒计时和便签")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "pin" => {
                        let state = app.state::<AppState>();
                        if let Ok(mut store) = state.store.lock() {
                            let pinned = !store.settings.pinned;
                            if let Some(window) = app.get_webview_window("main") {
                                if window.set_always_on_top(pinned).is_ok() {
                                    store.settings.pinned = pinned;
                                    if let Err(error) = storage::save(
                                        &store.db,
                                        "settings",
                                        "main",
                                        &store.settings,
                                    ) {
                                        let _ = app.emit("app-error", error);
                                    }
                                    let _ = app.emit("state-changed", store.snapshot());
                                }
                            }
                        };
                    }
                    "exit" => {
                        request_exit(app);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            if let Some(window) = app.get_webview_window("main") {
                window.set_always_on_top(settings.pinned)?;
                if let Some(geometry) = &settings.geometry {
                    windows::restore(&window, geometry);
                }
                let compact_geometry = if settings.compact {
                    settings
                        .compact_geometry
                        .as_ref()
                        .or(settings.geometry.as_ref())
                } else {
                    None
                };
                windows::mode(&window, settings.compact, compact_geometry)
                    .map_err(std::io::Error::other)?;
                if !std::env::args().any(|argument| argument == "--autostart") {
                    window.show()?;
                }
            }
            for id in open_notes {
                if let Err(error) = windows::open_note(app.handle(), &id) {
                    let _ = app.emit("app-error", error);
                }
            }
            scheduler::start(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            let app = window.app_handle();
            match event {
                WindowEvent::Moved(_) => windows::track(app, window.label(), false),
                WindowEvent::Resized(_) => {
                    windows::track(app, window.label(), false);
                    if window.label() == "main" {
                        if let Some(window) = app.get_webview_window("main") {
                            if let Err(error) = windows::refresh_shape(&window) {
                                let _ = app.emit("app-error", error);
                            }
                        }
                    }
                }
                WindowEvent::ScaleFactorChanged { .. } if window.label() == "main" => {
                    let compact = app
                        .state::<AppState>()
                        .store
                        .lock()
                        .map(|store| store.settings.compact)
                        .unwrap_or(false);
                    if let Some(window) = app.get_webview_window("main") {
                        if let Err(error) = if compact {
                            windows::shape(&window, true)
                        } else {
                            Ok(())
                        } {
                            let _ = app.emit("app-error", error);
                        }
                    }
                }
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                }
                WindowEvent::Destroyed => {
                    windows::closed(app, window.label());
                    if let Some(state) = app.try_state::<AppState>() {
                        if let Ok(mut waiting) = state.exit_waiting.lock() {
                            if let Some(windows) = waiting.as_mut() {
                                windows.remove(window.label());
                                if windows.is_empty() {
                                    drop(waiting);
                                    app.exit(0);
                                }
                            }
                        };
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::save_alarm,
            commands::delete_alarm,
            commands::timer_action,
            commands::reminder_action,
            commands::dismiss_reminders,
            commands::save_settings,
            commands::set_pinned,
            commands::set_compact,
            commands::create_note,
            commands::save_note,
            commands::delete_note,
            commands::open_note,
            commands::import_media,
            commands::image_data,
            commands::preview_sound,
            commands::stop_preview,
            commands::save_profile,
            commands::delete_profile,
            ai::key_status,
            ai::set_api_key,
            ai::remove_api_key,
            ai::start_chat,
            ai::cancel_chat,
            commands::complete_exit,
            commands::close_window
        ])
        .build(tauri::generate_context!())
        .expect("时刻启动失败")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                let Some(state) = app.try_state::<AppState>() else {
                    return;
                };
                if let Ok(mut store) = state.store.lock() {
                    let _ = scheduler::update_timer(
                        &mut store,
                        chrono::Utc::now().timestamp_millis(),
                        scheduler::uptime_ms(),
                    );
                    let _ = storage::save(&store.db, "timer", "main", &store.timer);
                    let _ = storage::save(&store.db, "settings", "main", &store.settings);
                    for note in &store.notes {
                        let _ = storage::save(&store.db, "note", &note.id, note);
                    }
                };
            }
        });
}
