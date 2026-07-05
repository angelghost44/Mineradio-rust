use tauri::{AppHandle, Emitter, Listener, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri::window::Color;

const LYRICS_LABEL: &str = "desktop-lyrics";
const LYRICS_WIDTH: f64 = 920.0;
const LYRICS_HEIGHT: f64 = 190.0;

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
        // Tauri v2 doesn't have setIgnoreCursorEvents, but we can emit an event
        let _ = window.emit("lyrics-lock-changed", serde_json::json!({ "locked": locked }));
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
