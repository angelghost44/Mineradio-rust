use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Listener, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri::window::Color;

const LYRICS_LABEL: &str = "desktop-lyrics";
const LYRICS_WIDTH: f64 = 920.0;
const LYRICS_HEIGHT: f64 = 190.0;

/// Stored hot bounds for the lyrics window (in physical pixels relative to
/// the lyrics window's top-left corner).  Used to decide whether mouse events
/// should pass through to windows below.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct LyricsHotBounds {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

/// Shared state for the lyrics overlay: hot bounds + current drag state.
pub struct LyricsState {
    hot_bounds: Mutex<LyricsHotBounds>,
    dragging: Mutex<bool>,
}

impl Default for LyricsState {
    fn default() -> Self {
        Self {
            hot_bounds: Mutex::new(LyricsHotBounds::default()),
            dragging: Mutex::new(false),
        }
    }
}

// ── Windows click-through FFI ─────────────────────────────────────
//
// Tauri v2 doesn't expose setIgnoreCursorEvents.  On Windows we toggle
// the WS_EX_TRANSPARENT extended style on the lyrics window to control
// whether mouse events pass through to windows below.
//
// When locked (click-through):   WS_EX_TRANSPARENT | WS_EX_LAYERED  → events pass through.
// When unlocked / dragging:      remove WS_EX_TRANSPARENT            → events received.

#[cfg(target_os = "windows")]
mod win_clickthrough {
    use std::ffi::c_void;

    type HWND = *mut c_void;
    type LongPtr = isize;
    type DWORD = u32;

    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_TRANSPARENT: DWORD = 0x00000020;
    const WS_EX_LAYERED: DWORD = 0x00080000;

    #[link(name = "user32")]
    extern "system" {
        fn GetWindowLongPtrW(hwnd: HWND, nIndex: i32) -> LongPtr;
        fn SetWindowLongPtrW(hwnd: HWND, nIndex: i32, dwNewLong: LongPtr) -> LongPtr;
    }

    /// Enable or disable click-through (mouse pass-through) on the window.
    ///
    /// When `transparent` is true, mouse events pass through the window.
    /// When false, the window receives mouse events normally.
    pub fn set_click_through(hwnd: HWND, transparent: bool) {
        unsafe {
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as DWORD;
            let new_style = if transparent {
                ex_style | WS_EX_TRANSPARENT
            } else {
                ex_style & !WS_EX_TRANSPARENT
            };
            // Ensure WS_EX_LAYERED is always set (required for transparent windows)
            let new_style = new_style | WS_EX_LAYERED;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_style as LongPtr);
        }
    }

    /// Get the HWND from a Tauri WebviewWindow.
    pub fn window_hwnd(
        window: &tauri::WebviewWindow,
    ) -> Result<HWND, String> {
        let hwnd = window.hwnd().map_err(|e| e.to_string())?;
        Ok(hwnd.0)
    }
}

#[tauri::command]
pub fn toggle_desktop_lyrics(app: AppHandle, enabled: bool, payload: serde_json::Value) -> Result<(), String> {
    if enabled {
        create_lyrics_window(&app, &payload)?;
    } else {
        if let Some(window) = app.get_webview_window(LYRICS_LABEL) {
            let _ = window.close();
        }
    }
    Ok(())
}

#[tauri::command]
pub fn update_desktop_lyrics(app: AppHandle, payload: serde_json::Value) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(LYRICS_LABEL) {
        let _ = window.emit("set-lyrics-state", payload.clone());
    } else {
        if payload.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            create_lyrics_window(&app, &payload)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn move_lyrics_by(app: AppHandle, dx: f64, dy: f64) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(LYRICS_LABEL) {
        let pos = window.outer_position().map_err(|e| e.to_string())?;
        let new_x = (pos.x as f64 + dx).max(0.0);
        let new_y = (pos.y as f64 + dy).max(0.0);
        let _ = window.set_position(tauri::Position::Physical(
            tauri::PhysicalPosition::new(new_x as i32, new_y as i32),
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn set_lyrics_lock_state(app: AppHandle, locked: bool) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(LYRICS_LABEL) {
        // On Windows, toggle WS_EX_TRANSPARENT to control click-through
        #[cfg(target_os = "windows")]
        {
            match win_clickthrough::window_hwnd(&window) {
                Ok(hwnd) => {
                    // locked = click-through (events pass through)
                    // unlocked = events received
                    win_clickthrough::set_click_through(hwnd, locked);
                }
                Err(e) => {
                    eprintln!("[Lyrics] cannot get HWND for lock state: {}", e);
                }
            }
        }
        // Notify the frontend about the lock state change
        let _ = window.emit("lyrics-lock-changed", serde_json::json!({ "locked": locked }));
    }
    Ok(())
}

/// Called by the desktop lyrics overlay when the user starts or stops
/// dragging the window.  During drag we must disable click-through so the
/// window receives mouse events; after drag we restore the previous state.
#[tauri::command]
pub fn set_lyrics_drag(
    app: AppHandle,
    state: State<'_, LyricsState>,
    dragging: bool,
) -> Result<(), String> {
    if let Ok(mut d) = state.dragging.lock() {
        *d = dragging;
    }

    if let Some(window) = app.get_webview_window(LYRICS_LABEL) {
        #[cfg(target_os = "windows")]
        {
            match win_clickthrough::window_hwnd(&window) {
                Ok(hwnd) => {
                    // During drag: always disable click-through so we get mouse events.
                    // After drag: re-enable click-through (the frontend will call
                    // set_lyrics_lock_state if needed to restore the correct state).
                    win_clickthrough::set_click_through(hwnd, !dragging);
                }
                Err(e) => {
                    eprintln!("[Lyrics] cannot get HWND for drag: {}", e);
                }
            }
        }
    }
    Ok(())
}

/// Called by the desktop lyrics overlay to report the hot bounds (the area
/// where the lyrics text is actually visible).  This is stored for potential
/// future use in region-based click-through.
#[tauri::command]
pub fn set_lyrics_hot_bounds(
    state: State<'_, LyricsState>,
    bounds: serde_json::Value,
) -> Result<(), String> {
    let left = bounds.get("left").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let top = bounds.get("top").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let right = bounds.get("right").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let bottom = bounds.get("bottom").and_then(|v| v.as_f64()).unwrap_or(0.0);

    if let Ok(mut hb) = state.hot_bounds.lock() {
        *hb = LyricsHotBounds { left, top, right, bottom };
    }
    Ok(())
}

fn create_lyrics_window(app: &AppHandle, payload: &serde_json::Value) -> Result<(), String> {
    if app.get_webview_window(LYRICS_LABEL).is_some() {
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(app, LYRICS_LABEL, WebviewUrl::App("desktop-lyrics.html".into()))
        .inner_size(LYRICS_WIDTH, LYRICS_HEIGHT)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .build()
        .map_err(|e| format!("failed to create lyrics window: {}", e))?;

    let _ = window.set_background_color(Some(Color(0, 0, 0, 0)));

    // Start with click-through enabled (locked by default)
    #[cfg(target_os = "windows")]
    {
        if let Ok(hwnd) = win_clickthrough::window_hwnd(&window) {
            win_clickthrough::set_click_through(hwnd, true);
        }
    }

    let app_clone = app.clone();
    let payload_clone = payload.clone();
    window.once("tauri://created", move |_| {
        let _ = app_clone.emit("set-lyrics-state", payload_clone);
    });

    Ok(())
}

pub fn setup_listeners(app: &AppHandle) {
    let app_clone = app.clone();
    app.listen("lyrics-pointer-capture", move |event| {
        if let Some(window) = app_clone.get_webview_window("main") {
            let _ = window.emit("desktop-lyrics-pointer-capture", event.payload());
        }
    });

    let app_clone2 = app.clone();
    app.listen("lyrics-lock-state", move |event| {
        if let Some(window) = app_clone2.get_webview_window("main") {
            let _ = window.emit("desktop-lyrics-lock-state", event.payload());
        }
    });

    let app_clone3 = app.clone();
    app.listen("lyrics-enabled-state", move |event| {
        if let Some(window) = app_clone3.get_webview_window("main") {
            let _ = window.emit("desktop-lyrics-enabled-state", event.payload());
        }
    });
}
