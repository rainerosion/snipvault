use once_cell::sync::OnceCell;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::{
    image::Image,
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder},
    tray::{TrayIcon, TrayIconBuilder},
    AppHandle, Emitter, Manager,
};

static TRAY: OnceCell<Arc<Mutex<Option<TrayIcon>>>> = OnceCell::new();

fn autostart_checked_state(persisted: bool, plugin_state: Option<bool>) -> bool {
    plugin_state.unwrap_or(persisted)
}

fn current_autostart_checked(app: &AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;

    let persisted = crate::settings::get_settings().auto_start;
    let plugin_state = app.autolaunch().is_enabled().map_err(|error| {
        log::warn!("Reading autostart state for tray menu failed: {error}");
        error
    });
    autostart_checked_state(persisted, plugin_state.ok())
}

fn build_tray_menu(
    app: &AppHandle,
    auto_start: bool,
) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let show_item = MenuItemBuilder::with_id("show", "打开 灵藏 SnipVault").build(app)?;
    let capture_item = MenuItemBuilder::with_id("quick-capture", "从剪贴板快速捕获").build(app)?;
    let sync_item = MenuItemBuilder::with_id("sync", "立即同步").build(app)?;
    let settings_item = MenuItemBuilder::with_id("settings", "设置").build(app)?;
    let autostart_item = CheckMenuItemBuilder::with_id("autostart", "开机自启")
        .checked(auto_start)
        .build(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", "退出").build(app)?;

    MenuBuilder::new(app)
        .item(&show_item)
        .separator()
        .item(&capture_item)
        .item(&sync_item)
        .item(&settings_item)
        .separator()
        .item(&autostart_item)
        .separator()
        .item(&quit_item)
        .build()
}

pub fn reveal_main_window(app: &AppHandle) {
    crate::commands::WINDOW_SHOWN.store(true, Ordering::SeqCst);
    crate::commands::boot_log("reveal_main_window", "manual_reveal");
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn emit_window_event<T: serde::Serialize + Clone>(app: &AppHandle, name: &str, payload: T) {
    let Some(window) = app.get_webview_window("main") else {
        log::warn!("Could not emit {name} because the main window is unavailable");
        return;
    };
    if let Err(error) = window.emit(name, payload) {
        log::error!("Emitting {name} failed: {error}");
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let menu = build_tray_menu(app, current_autostart_checked(app))?;

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
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => reveal_main_window(app),
            "quick-capture" => {
                crate::capture::run_quick_capture(app.clone(), crate::capture::QuickCaptureSource::Tray);
            }
            "sync" => {
                reveal_main_window(app);
                let app_handle = app.clone();
                std::thread::spawn(move || {
                    crate::sync::run_and_emit(&app_handle, crate::sync::SyncSource::Tray);
                });
            }
            "settings" => {
                reveal_main_window(app);
                emit_window_event(app, "open-settings", ());
            }
            "autostart" => {
                let app_handle = app.clone();
                std::thread::spawn(move || {
                    match crate::commands::toggle_auto_start_from_tray(&app_handle) {
                        Ok(enabled) => {
                            log::info!("Autostart toggled to {enabled}");
                            reveal_main_window(&app_handle);
                            emit_window_event(&app_handle, "autostart-toggled", enabled);
                        }
                        Err(error) => {
                            log::error!("set_auto_start failed: {error}");
                            if let Err(refresh_error) = refresh_menu(&app_handle) {
                                log::error!(
                                    "Refreshing tray menu after autostart failure failed: {refresh_error}"
                                );
                            }
                        }
                    }
                });
            }
            "quit" => std::process::exit(0),
            _ => {}
        })
        .build(app)
}

pub fn initialize(app: &AppHandle) -> tauri::Result<()> {
    let tray_store = TRAY
        .get_or_init(|| Arc::new(Mutex::new(None::<TrayIcon>)))
        .clone();
    let tray = build_tray(app)?;
    let mut guard = tray_store.lock().unwrap_or_else(|error| error.into_inner());
    *guard = Some(tray);
    Ok(())
}

pub fn refresh_menu(app: &AppHandle) -> tauri::Result<()> {
    let Some(tray_store) = TRAY.get() else {
        log::warn!("Tray refresh requested before tray initialization");
        return Ok(());
    };
    let menu = build_tray_menu(app, current_autostart_checked(app))?;
    let guard = tray_store.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(tray) = guard.as_ref() {
        tray.set_menu(Some(menu))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::autostart_checked_state;

    #[test]
    fn tray_checked_state_prefers_plugin_and_falls_back_to_persisted_setting() {
        assert!(autostart_checked_state(false, Some(true)));
        assert!(!autostart_checked_state(true, Some(false)));
        assert!(autostart_checked_state(true, None));
        assert!(!autostart_checked_state(false, None));
    }
}
