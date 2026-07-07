use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// A single hotkey binding sent from the frontend.
/// `accelerator` uses Electron-style strings, e.g. `"Control+Shift+P"`, `"MediaPlayPause"`.
#[derive(Deserialize)]
pub struct HotkeyBinding {
    pub action: String,
    pub accelerator: String,
}

/// Conflict metadata returned when a shortcut cannot be registered.
#[derive(Serialize)]
pub struct HotkeyConflict {
    pub source_name: String,
    pub source_icon: String,
    pub reason: String,
}

/// Per-binding registration result.
#[derive(Serialize)]
pub struct HotkeyResult {
    pub action: String,
    pub accelerator: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict: Option<HotkeyConflict>,
}

/// Top-level response returned to the frontend.
#[derive(Serialize)]
pub struct ConfigureResult {
    pub ok: bool,
    pub results: Vec<HotkeyResult>,
}

fn make_conflict() -> HotkeyConflict {
    HotkeyConflict {
        source_name: "系统 / 其他软件".to_string(),
        source_icon: "warning".to_string(),
        reason: "该组合键已被占用或被系统保留".to_string(),
    }
}

/// Tauri command – mirrors Electron's `mineradio-hotkeys-configure-global` IPC handler.
///
/// 1. Unregisters all previously registered global shortcuts.
/// 2. Registers each binding; on success stores the handler that emits
///    `mineradio-global-hotkey` `{ action }` back to the frontend.
/// 3. Returns `{ ok: true, results: [...] }` with per-binding success/conflict info.
#[tauri::command]
pub fn configure_global_hotkeys<R: Runtime>(
    app: AppHandle<R>,
    bindings: Vec<HotkeyBinding>,
) -> ConfigureResult {
    // Clear everything from a previous call.
    let _ = app.global_shortcut().unregister_all();

    let mut results = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for binding in bindings {
        let action = binding.action.trim().to_string();
        let accelerator = binding.accelerator.trim().to_string();

        // Skip empty or duplicate accelerators (matches Electron behaviour).
        if action.is_empty() || accelerator.is_empty() || seen.contains(&accelerator) {
            continue;
        }
        seen.insert(accelerator.clone());

        // Each shortcut gets its own closure that captures the action string,
        // so no shared lookup map is needed.
        let action_for_handler = action.clone();
        let app_for_handler = app.clone();

        let register_result = app.global_shortcut().on_shortcut(
            accelerator.as_str(),
            move |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    let _ = app_for_handler.emit(
                        "mineradio-global-hotkey",
                        serde_json::json!({ "action": action_for_handler.clone() }),
                    );
                }
            },
        );

        match register_result {
            Ok(()) => results.push(HotkeyResult {
                action,
                accelerator,
                ok: true,
                conflict: None,
            }),
            Err(_) => results.push(HotkeyResult {
                action,
                accelerator,
                ok: false,
                conflict: Some(make_conflict()),
            }),
        }
    }

    ConfigureResult {
        ok: true,
        results,
    }
}
