use tauri::{AppHandle, Emitter, Listener, Manager, WebviewUrl, WebviewWindowBuilder};

const WALLPAPER_LABEL: &str = "wallpaper";

#[tauri::command]
pub fn toggle_wallpaper_mode(app: AppHandle, enabled: bool, payload: serde_json::Value) -> Result<(), String> {
    if enabled {
        create_wallpaper_window(&app, &payload)?;
    } else {
        if let Some(window) = app.get_webview_window(WALLPAPER_LABEL) {
            let _ = window.close();
        }
    }
    Ok(())
}

#[tauri::command]
pub fn update_wallpaper_mode(app: AppHandle, payload: serde_json::Value) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WALLPAPER_LABEL) {
        let _ = window.emit("set-wallpaper-state", payload.clone());
    } else {
        if payload.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            create_wallpaper_window(&app, &payload)?;
        }
    }
    Ok(())
}

fn create_wallpaper_window(app: &AppHandle, payload: &serde_json::Value) -> Result<(), String> {
    if app.get_webview_window(WALLPAPER_LABEL).is_some() {
        return Ok(());
    }

    // Get primary monitor size for fullscreen
    let (mon_w, mon_h) = if let Some(monitor) = {
        app.get_webview_window("main")
            .and_then(|w| w.primary_monitor().ok())
            .flatten()
    } {
        let size = monitor.size();
        (size.width as f64, size.height as f64)
    } else {
        (1920.0, 1080.0)
    };

    let window = WebviewWindowBuilder::new(app, WALLPAPER_LABEL, WebviewUrl::App("wallpaper.html".into()))
        .inner_size(mon_w, mon_h)
        .position(0.0, 0.0)
        .resizable(false)
        .decorations(false)
        .transparent(false)
        .skip_taskbar(true)
        .build()
        .map_err(|e| format!("failed to create wallpaper window: {}", e))?;

    let _ = window.set_background_color(Some(tauri::window::Color(5, 6, 8, 255)));

    let app_clone = app.clone();
    let payload_clone = payload.clone();
    window.once("tauri://created", move |_| {
        // TODO: WorkerW attachment for Windows
        let _ = app_clone.emit("set-wallpaper-state", payload_clone);
    });

    Ok(())
}
