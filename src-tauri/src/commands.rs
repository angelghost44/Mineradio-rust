use crate::extractor::CoverData;
use crate::scanner::ScannedFile;

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
