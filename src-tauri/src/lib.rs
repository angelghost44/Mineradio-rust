use tauri::Manager;

mod commands;
mod extractor;
mod scanner;
mod sidecar_manager;
pub mod state;

use std::sync::Mutex;

pub struct SidecarState(pub Mutex<sidecar_manager::SidecarProcess>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(SidecarState(Mutex::new(
            sidecar_manager::SidecarProcess::new("node", "sidecar/index.js"),
        )))
        .invoke_handler(tauri::generate_handler![
            commands::scan_folder,
            commands::extract_cover,
            commands::sidecar_call,
            crate::state::load_state,
            crate::state::save_state,
        ])
        .setup(|app| {
            let state = app.state::<SidecarState>();
            if let Ok(manager) = state.0.lock() {
                if let Err(e) = manager.start() {
                    eprintln!("[Sidecar] failed to auto-start: {}", e);
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
