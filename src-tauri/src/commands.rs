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
