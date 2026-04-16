#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use once_cell::sync::OnceCell;
use std::sync::{Arc, Mutex};
use tauri::{
    image::Image,
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder},
    tray::{TrayIcon, TrayIconBuilder},
    AppHandle, Emitter, Manager,
};

/// Stores the tray icon so we can rebuild it when autostart state changes.
static TRAY: OnceCell<Arc<Mutex<Option<TrayIcon>>>> = OnceCell::new();

fn build_tray_menu(app: &AppHandle, auto_start: bool) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let show_item = MenuItemBuilder::with_id("show", "打开 灵藏 SnipVault").build(app)?;
    let sync_item = MenuItemBuilder::with_id("sync", "立即同步").build(app)?;
    let settings_item = MenuItemBuilder::with_id("settings", "设置").build(app)?;
    let autostart_item = CheckMenuItemBuilder::with_id("autostart", "开机自启")
        .checked(auto_start)
        .build(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", "退出").build(app)?;

    MenuBuilder::new(app)
        .item(&show_item)
        .separator()
        .item(&sync_item)
        .item(&settings_item)
        .separator()
        .item(&autostart_item)
        .separator()
        .item(&quit_item)
        .build()
}

fn reveal_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn build_tray(app: &AppHandle, auto_start: bool) -> tauri::Result<TrayIcon> {
    let menu = build_tray_menu(app, auto_start)?;

    TrayIconBuilder::new()
        .icon(Image::from_path("icons/32x32.png").unwrap_or_else(|_| {
            Image::from_bytes(include_bytes!("../icons/32x32.png")).expect("invalid tray icon")
        }))
        .menu(&menu)
        .tooltip("灵藏 SnipVault")
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                ..
            } = event
            {
                reveal_main_window(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| {
            match event.id().as_ref() {
                "show" => {
                    reveal_main_window(app);
                }
                "sync" => {
                    reveal_main_window(app);
                    let app_handle = app.clone();
                    std::thread::spawn(move || {
                        match snipvault::webdav::sync_merge() {
                            Ok(result) => {
                                log::info!("Tray sync result: {}", result.message);
                                if let Some(window) = app_handle.get_webview_window("main") {
                                    let _ = window.emit("sync-complete", &result);
                                }
                            }
                            Err(e) => {
                                log::error!("Tray sync error: {}", e);
                                if let Some(window) = app_handle.get_webview_window("main") {
                                    let _ = window.emit("sync-complete", serde_json::json!({
                                        "success": false,
                                        "message": e
                                    }));
                                }
                            }
                        }
                    });
                }
                "settings" => {
                    // Directly open settings panel in the frontend
                    reveal_main_window(app);
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.eval("if(window.__openSettings)window.__openSettings()");
                    }
                }
                "autostart" => {
                    // Toggle auto-start via proper IPC so the plugin + settings both update
                    let app_handle = app.clone();
                    std::thread::spawn(move || {
                        let current = snipvault::settings::get_settings();
                        let new_val = !current.auto_start;
                        match snipvault::commands::set_auto_start(new_val, app_handle.clone()) {
                            Ok(()) => {
                                log::info!("Autostart toggled to {}", new_val);
                                // Update tray menu checkbox to match new state
                                if let Some(tray_cell) = TRAY.get() {
                                    if let Ok(guard) = tray_cell.lock() {
                                        if let Some(tray) = guard.as_ref() {
                                            if let Ok(menu) = build_tray_menu(&app_handle, new_val) {
                                                let _ = tray.set_menu(Some(menu));
                                            }
                                        }
                                    }
                                }
                                reveal_main_window(&app_handle);
                                if let Some(window) = app_handle.get_webview_window("main") {
                                    let _ = window.emit("autostart-toggled", new_val);
                                }
                            }
                            Err(e) => {
                                log::error!("set_auto_start failed: {}", e);
                            }
                        }
                    });
                }
                "quit" => {
                    std::process::exit(0);
                }
                _ => {}
            }
        })
        .build(app)
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    // Check if started with --minimized flag (from autostart)
    let start_minimized = std::env::args().any(|arg| arg == "--minimized");
    if start_minimized {
        log::info!("Started with --minimized flag, will not show main window");
    }

    log::info!("Starting 灵藏 SnipVault");

    if let Err(e) = snipvault::db::init_db() {
        log::error!("Failed to initialize database: {e}");
        std::process::exit(1);
    }
    log::info!("Database initialized");

    snipvault::settings::init_settings();
    log::info!("Settings initialized");

    log::info!(
        "App mode: {} | Data dir: {:?}",
        snipvault::paths::get_app_mode(),
        snipvault::paths::get_data_dir()
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(move |app| {
            // Initialize global tray storage
            let tray_store = Arc::new(Mutex::new(None::<TrayIcon>));
            TRAY.set(tray_store.clone()).ok();

            let settings = snipvault::settings::get_settings();
            let tray = build_tray(app.handle(), settings.auto_start)?;
            *tray_store.lock().unwrap() = Some(tray);

            // Hide window on startup if started with --minimized flag
            if start_minimized {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }

            // Handle main window close -> minimize to tray
            let minimize_to_tray = settings.minimize_to_tray;
            let app_handle = app.handle().clone();
            let main_window = app.get_webview_window("main").unwrap();
            main_window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    if minimize_to_tray {
                        api.prevent_close();
                        if let Some(w) = app_handle.get_webview_window("main") {
                            let _ = w.hide();
                        }
                    }
                }
            });

            // Auto-sync timer
            let auto_sync_settings = snipvault::settings::get_settings();
            if auto_sync_settings.auto_sync && auto_sync_settings.sync_interval_minutes > 0 {
                let interval_secs = auto_sync_settings.sync_interval_minutes as u64 * 60;
                let _app_handle_sync = app.handle().clone();
                log::info!(
                    "Starting auto-sync timer (every {} minutes)",
                    auto_sync_settings.sync_interval_minutes
                );
                std::thread::spawn(move || {
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(interval_secs));
                        let s = snipvault::settings::get_settings();
                        if !s.auto_sync || s.webdav_url.is_empty() {
                            log::info!(
                                "Auto-sync disabled or WebDAV not configured, stopping timer"
                            );
                            break;
                        }
                        log::info!("Running scheduled auto-sync");
                        match snipvault::webdav::sync_merge() {
                            Ok(result) => {
                                log::info!("Auto-sync result: {}", result.message);
                            }
                            Err(e) => {
                                log::error!("Auto-sync error: {}", e);
                            }
                        }
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            snipvault::commands::get_snippets,
            snipvault::commands::create_snippet,
            snipvault::commands::update_snippet,
            snipvault::commands::delete_snippet,
            snipvault::commands::search_snippets,
            snipvault::commands::toggle_favorite,
            snipvault::commands::export_snippets,
            snipvault::commands::import_snippets,
            snipvault::commands::get_settings,
            snipvault::commands::save_settings,
            snipvault::commands::set_auto_start,
            snipvault::commands::is_auto_start_enabled,
            snipvault::commands::sync_upload,
            snipvault::commands::sync_download,
            snipvault::commands::get_sync_versions,
            snipvault::commands::get_system_theme,
            snipvault::commands::get_system_locale,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
