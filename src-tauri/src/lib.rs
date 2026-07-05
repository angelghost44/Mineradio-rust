use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

mod commands;
mod extractor;
mod scanner;
mod sidecar_manager;
pub mod state;

pub struct SidecarState(pub Mutex<sidecar_manager::SidecarProcess>);

fn resolve_sidecar_path(app: &tauri::App) -> PathBuf {
    let res = app.path().resource_dir().unwrap_or_default();
    let dev = res.join("../sidecar/index.js");
    if dev.exists() {
        return std::fs::canonicalize(&dev).unwrap_or(dev);
    }
    let prod = res.join("sidecar/index.js");
    if prod.exists() {
        return prod;
    }
    PathBuf::from("sidecar/index.js")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::scan_folder,
            commands::extract_cover,
            commands::sidecar_call,
            crate::state::load_state,
            crate::state::save_state,
        ])
        .setup(|app| {
            let sidecar_path = resolve_sidecar_path(app);
            let script = sidecar_path.to_string_lossy().to_string();
            let process = sidecar_manager::SidecarProcess::new("node", &script);
            app.manage(SidecarState(Mutex::new(process)));

            let state = app.state::<SidecarState>();
            if let Ok(manager) = state.0.lock() {
                if let Err(e) = manager.start() {
                    eprintln!("[Sidecar] failed to start: {}", e);
                }
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            let state = app_handle.state::<SidecarState>();
            let _ = state.0.lock().map(|m| m.stop());
        }
    });
}
