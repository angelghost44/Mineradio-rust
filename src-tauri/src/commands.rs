use crate::extractor::CoverData;
use crate::scanner::ScannedFile;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;
use std::fs;

#[tauri::command]
pub fn scan_folder(folder: String) -> Result<Vec<ScannedFile>, String> {
    crate::scanner::scan_folder(&folder)
}

#[tauri::command]
pub fn extract_cover(path: String) -> Result<Option<CoverData>, String> {
    crate::extractor::extract_cover_cached(&path)
}

#[tauri::command]
pub async fn sidecar_call(
    state: tauri::State<'_, crate::OnlineApiState>,
    method: String,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    state.0.call(&method, params).await
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

/// Sanitise a filename the same way Electron's handler does: replace
/// illegal characters (`\ / : * ? " < > |`) with `-`.
fn sanitize_file_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            other => other,
        })
        .collect()
}

/// Tauri command – mirrors Electron's `mineradio-export-json-file` IPC handler.
/// Shows a save dialog, then writes `text` (or serialised `data`) to the chosen path.
#[tauri::command]
pub async fn export_json_file(
    app: tauri::AppHandle,
    default_name: Option<String>,
    text: Option<String>,
    data: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let window = app.get_webview_window("main").ok_or("no main window")?;

    let raw_name = default_name.unwrap_or_else(|| "mineradio-export.json".to_string());
    let safe_name = sanitize_file_name(&raw_name);
    let file_name = if safe_name.to_lowercase().ends_with(".json") {
        safe_name
    } else {
        format!("{}.json", safe_name)
    };

    let (tx, rx) = std::sync::mpsc::channel::<Option<tauri_plugin_dialog::FilePath>>();
    app.dialog()
        .file()
        .set_title("导出 Mineradio 存档")
        .set_file_name(&file_name)
        .add_filter("JSON", &["json"])
        .set_parent(&window)
        .save_file(move |file_path| {
            let _ = tx.send(file_path);
        });

    let chosen = rx
        .recv()
        .map_err(|e| format!("dialog error: {}", e))?;

    let path = match chosen {
        Some(p) => p,
        None => return Ok(serde_json::json!({ "ok": false, "canceled": true })),
    };

    let file_path_str = path.to_string();
    let content = match text {
        Some(t) => t,
        None => serde_json::to_string_pretty(&data.unwrap_or(serde_json::json!({})))
            .unwrap_or_else(|_| "{}".to_string()),
    };

    fs::write(&file_path_str, content)
        .map_err(|e| serde_json::json!({ "ok": false, "error": e.to_string() }).to_string())?;

    Ok(serde_json::json!({ "ok": true, "filePath": file_path_str }))
}

/// Tauri command – mirrors Electron's `mineradio-import-json-file` IPC handler.
/// Shows an open-file dialog, then reads the chosen JSON file as UTF-8 text.
#[tauri::command]
pub async fn import_json_file(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let window = app.get_webview_window("main").ok_or("no main window")?;

    let (tx, rx) = std::sync::mpsc::channel::<Option<tauri_plugin_dialog::FilePath>>();
    app.dialog()
        .file()
        .set_title("导入 Mineradio 存档")
        .add_filter("JSON", &["json"])
        .set_parent(&window)
        .pick_file(move |file_path| {
            let _ = tx.send(file_path);
        });

    let chosen = rx
        .recv()
        .map_err(|e| format!("dialog error: {}", e))?;

    let path = match chosen {
        Some(p) => p,
        None => return Ok(serde_json::json!({ "ok": false, "canceled": true })),
    };

    let file_path_str = path.to_string();
    let text = fs::read_to_string(&file_path_str)
        .map_err(|e| serde_json::json!({ "ok": false, "error": e.to_string() }).to_string())?;

    Ok(serde_json::json!({ "ok": true, "filePath": file_path_str, "text": text }))
}
