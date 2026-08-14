use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

const QUICK_CAPTURE_EVENT: &str = "quick-capture-complete";
const QUICK_CAPTURE_FALLBACK_TITLE: &str = "Clipboard capture";

static LATEST_COMPLETION: OnceCell<Mutex<Option<QuickCaptureCompletion>>> = OnceCell::new();
static QUICK_CAPTURE_IN_FLIGHT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, serde::Serialize)]
pub struct QuickCaptureCompletion {
    pub source: QuickCaptureSource,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet_id: Option<String>,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickCaptureSource {
    GlobalShortcut,
    Tray,
}

fn capture_title(content: &str) -> String {
    let first_line = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    let sanitized: String = first_line
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    let trimmed = sanitized.trim();
    if trimmed.is_empty() {
        return QUICK_CAPTURE_FALLBACK_TITLE.to_string();
    }
    let mut title: String = trimmed.chars().take(512).collect();
    if title.is_empty() {
        title = QUICK_CAPTURE_FALLBACK_TITLE.to_string();
    }
    title
}

fn store_and_emit(app: &AppHandle, completion: QuickCaptureCompletion) {
    let store = LATEST_COMPLETION.get_or_init(|| Mutex::new(None));
    *store.lock().unwrap_or_else(|error| error.into_inner()) = Some(completion.clone());

    if let Some(window) = app.get_webview_window("main") {
        if window.emit(QUICK_CAPTURE_EVENT, completion).is_err() {
            log::warn!("Quick capture completion could not be delivered to the WebView");
        }
    }
}

pub fn take_completion() -> Option<QuickCaptureCompletion> {
    LATEST_COMPLETION
        .get()
        .and_then(|store| store.lock().ok()?.take())
}

struct QuickCaptureInFlightGuard;

impl Drop for QuickCaptureInFlightGuard {
    fn drop(&mut self) {
        QUICK_CAPTURE_IN_FLIGHT.fetch_sub(1, Ordering::AcqRel);
    }
}

pub fn run_quick_capture(app: AppHandle, source: QuickCaptureSource) {
    if QUICK_CAPTURE_IN_FLIGHT.fetch_add(1, Ordering::AcqRel) > 0 {
        QUICK_CAPTURE_IN_FLIGHT.fetch_sub(1, Ordering::AcqRel);
        return;
    }

    std::thread::spawn(move || {
        let _in_flight = QuickCaptureInFlightGuard;
        let content = match app.clipboard().read_text() {
            Ok(value) if !value.trim().is_empty() => value,
            Ok(_) | Err(_) => {
                store_and_emit(
                    &app,
                    QuickCaptureCompletion {
                        source,
                        success: false,
                        snippet_id: None,
                    },
                );
                return;
            }
        };

        let now = chrono::Utc::now().to_rfc3339();
        let snippet = crate::db::Snippet {
            id: uuid::Uuid::new_v4().to_string(),
            title: capture_title(&content),
            content,
            language: "plaintext".to_string(),
            description: String::new(),
            tags: Vec::new(),
            is_favorite: false,
            created_at: now.clone(),
            updated_at: now,
            revision_id: String::new(),
        };

        let completion = match crate::db::validate_snippet(&snippet).and_then(|()| {
            crate::db::create_snippet_and_record_usage(&snippet)
                .map(|created| created.id)
                .map_err(|_| "quick capture write failed".to_string())
        }) {
            Ok(snippet_id) => QuickCaptureCompletion {
                source,
                success: true,
                snippet_id: Some(snippet_id),
            },
            Err(_) => QuickCaptureCompletion {
                source,
                success: false,
                snippet_id: None,
            },
        };
        store_and_emit(&app, completion);
    });
}

fn default_shortcut() -> Shortcut {
    #[cfg(target_os = "macos")]
    let modifier = Modifiers::SUPER;
    #[cfg(not(target_os = "macos"))]
    let modifier = Modifiers::CONTROL;

    Shortcut::new(Some(modifier | Modifiers::SHIFT), Code::KeyV)
}

pub fn register_global_shortcut(app: &AppHandle) {
    let shortcut = default_shortcut();
    if let Err(_error) = app.global_shortcut().register(shortcut) {
        log::warn!("Quick capture global shortcut could not be registered");
    }
}

pub fn global_shortcut_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                run_quick_capture(app.clone(), QuickCaptureSource::GlobalShortcut);
            }
        })
        .build()
}
