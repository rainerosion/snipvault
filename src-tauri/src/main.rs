#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::Manager;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    let _ = snipvault::commands::BOOT_START.set(std::time::Instant::now());
    snipvault::commands::boot_log("main_start", "process_begin");

    // Check if started with --minimized flag (from autostart)
    let start_minimized = std::env::args().any(|arg| arg == "--minimized");
    if start_minimized {
        log::info!("Started with --minimized flag, will not show main window");
    }

    log::info!("Starting 灵藏 SnipVault");
    snipvault::commands::boot_log("builder_start", "tauri_builder_default");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            snipvault::tray::reveal_main_window(app);
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(move |app| {
            snipvault::commands::boot_log("setup_enter", "start");
            log::info!("App mode: {}", snipvault::paths::get_app_mode());

            // Warm-up DB/settings in background so first paint is not blocked
            std::thread::spawn(|| {
                if let Err(_error) = snipvault::db::init_db() {
                    log::error!("Background database initialization failed");
                } else {
                    log::info!("Background database init completed");
                }
                snipvault::settings::init_settings();
                log::info!("Background settings init completed");
            });

            snipvault::commands::boot_log("setup_before_build_tray", "start");
            snipvault::tray::initialize(app.handle())?;
            snipvault::commands::boot_log("setup_after_build_tray", "done");

            if start_minimized {
                snipvault::commands::WINDOW_SHOWN.store(true, Ordering::SeqCst);
                snipvault::commands::boot_log("startup_mode", "minimized");
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            } else {
                snipvault::commands::boot_log("startup_mode", "deferred_show_wait_frontend_ready");
                let app_handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(2500));
                    if !snipvault::commands::WINDOW_SHOWN.load(Ordering::SeqCst) {
                        snipvault::commands::boot_log("fallback_show_fired", "timeout_ms=2500");
                        snipvault::commands::show_main_window_if_needed(
                            &app_handle,
                            "fallback_timeout",
                        );
                    }
                });
            }

            // Handle main window close -> minimize to tray using the latest
            // persisted setting so changes take effect without restarting.
            let app_handle = app.handle().clone();
            let main_window = app.get_webview_window("main").unwrap();
            main_window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    if snipvault::settings::get_settings().minimize_to_tray {
                        api.prevent_close();
                        if let Some(window) = app_handle.get_webview_window("main") {
                            let _ = window.hide();
                        }
                    }
                }
            });

            snipvault::sync::start_auto_sync_worker(app.handle().clone());

            snipvault::commands::boot_log("setup_exit", "done");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            snipvault::commands::get_snippets,
            snipvault::commands::query_snippets,
            snipvault::commands::get_snippet,
            snipvault::commands::get_snippet_tags,
            snipvault::commands::create_snippet,
            snipvault::commands::update_snippet,
            snipvault::commands::delete_snippet,
            snipvault::commands::search_snippets,
            snipvault::commands::toggle_favorite,
            snipvault::commands::export_snippets,
            snipvault::commands::export_snippets_to_file,
            snipvault::commands::open_project_repository,
            snipvault::commands::open_trusted_directory,
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
            snipvault::commands::frontend_ready,
            snipvault::commands::boot_mark,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    snipvault::commands::boot_log("run_exit", "tauri_run_returned");
}
