use crate::extractor::CoverData;
use crate::scanner::ScannedFile;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub fn scan_folder(folder: String) -> Result<Vec<ScannedFile>, String> {
    crate::scanner::scan_folder(&folder)
}

#[tauri::command]
pub fn extract_cover(path: String) -> Result<Option<CoverData>, String> {
    crate::extractor::extract_cover_cached(&path)
}

#[tauri::command]
pub fn sidecar_call(
    state: tauri::State<'_, crate::SidecarState>,
    method: String,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let manager = state.0.lock().map_err(|e| e.to_string())?;
    manager.call(&method, params)
}

#[tauri::command]
pub async fn pick_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let window = app.get_webview_window("main").ok_or("no main window")?;
    let (tx, rx) = std::sync::mpsc::channel::<Option<tauri_plugin_dialog::FilePath>>();
    app.dialog()
        .file()
        .set_parent(&window)
        .pick_folder(move |file_path| {
            let _ = tx.send(file_path);
        });
    let result = rx.recv().map_err(|e| format!("dialog error: {}", e))?;
    match result {
        Some(path) => Ok(Some(path.to_string())),
        None => Ok(None),
    }
}
