use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::Manager;

#[derive(Debug, Serialize, Deserialize)]
pub struct AppData {
    pub version: u32,
    pub local_music_folder: Option<String>,
    pub preferences: serde_json::Value,
}

impl Default for AppData {
    fn default() -> Self {
        Self {
            version: 1,
            local_music_folder: None,
            preferences: serde_json::Value::Object(serde_json::Map::new()),
        }
    }
}

fn state_path(_app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let test = dir.join(".state_test");
            if std::fs::write(&test, "").is_ok() {
                let _ = std::fs::remove_file(&test);
                return Ok(dir.join("state.json"));
            }
        }
    }
    let dir = _app_handle.path().app_data_dir().map_err(|e| format!("failed to get app data dir: {}", e))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("failed to create state dir: {}", e))?;
    Ok(dir.join("state.json"))
}

#[tauri::command]
pub fn load_state(app_handle: tauri::AppHandle) -> Result<AppData, String> {
    let path = state_path(&app_handle)?;

    if !path.exists() {
        return Ok(AppData::default());
    }

    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("failed to read state file: {}", e))?;

    serde_json::from_str(&content).map_err(|e| format!("failed to parse state: {}", e))
}

#[tauri::command]
pub fn save_state(app_handle: tauri::AppHandle, data: AppData) -> Result<(), String> {
    let path = state_path(&app_handle)?;

    let content =
        serde_json::to_string_pretty(&data).map_err(|e| format!("failed to serialize: {}", e))?;

    std::fs::write(&path, content).map_err(|e| format!("failed to write state: {}", e))?;

    Ok(())
}
