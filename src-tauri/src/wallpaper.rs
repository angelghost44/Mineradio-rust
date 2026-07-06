use tauri::{AppHandle, Emitter, Listener, Manager, WebviewUrl, WebviewWindowBuilder};

const WALLPAPER_LABEL: &str = "wallpaper";

// ── Windows WorkerW attachment (raw FFI) ──────────────────────────
//
// On Windows, to display a window behind desktop icons (as a live wallpaper),
// we attach it as a child of the WorkerW window that sits between the
// desktop background and the icon layer.
//
// Steps:
//   1. Find Progman (the Program Manager window).
//   2. Send WM_SPAWN_WORKER (0x052C) so Progman creates a WorkerW.
//   3. Enumerate top-level windows to find that WorkerW (its class is
//      "WorkerW" and it comes immediately after Progman in z-order).
//   4. SetParent(wallpaper_hwnd, workerw_hwnd).

#[cfg(target_os = "windows")]
mod win_wallpaper {
    use std::ffi::c_void;
    use std::ptr;

    type HWND = *mut c_void;
    type BOOL = i32;
    type LPARAM = isize;
    type WPARAM = usize;
    type LRESULT = isize;

    const WM_SPAWN_WORKER: u32 = 0x052C;
    const GWL_HWNDPARENT: i32 = -8;

    #[link(name = "user32")]
    extern "system" {
        fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> HWND;
        fn SendMessageTimeoutW(
            hwnd: HWND,
            msg: u32,
            wParam: WPARAM,
            lParam: LPARAM,
            fuFlags: u32,
            uTimeout: u32,
            lpdwResult: *mut usize,
        ) -> LRESULT;
        fn EnumWindows(lpEnumFunc: *const c_void, lParam: LPARAM) -> BOOL;
        fn GetClassNameW(hwnd: HWND, lpClassName: *mut u16, nMaxCount: i32) -> i32;
        fn GetWindowLongPtrW(hwnd: HWND, nIndex: i32) -> isize;
        fn SetParent(hwndChild: HWND, hwndNewParent: HWND) -> HWND;
        fn SetWindowPos(
            hwnd: HWND,
            hwndInsertAfter: HWND,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> BOOL;
    }

    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_SHOWWINDOW: u32 = 0x0040;
    const HWND_TOP: HWND = 0 as HWND;

    // Convert a Rust &str to a UTF-16 null-terminated Vec for Win32 wide APIs.
    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0u16)).collect()
    }

    /// State passed into the EnumWindows callback.
    struct EnumState {
        progman: HWND,
        found: Option<HWND>,
    }

    /// EnumWindows callback: find the WorkerW whose parent is Progman.
    /// WorkerW is identified by class name == "WorkerW" and appearing after
    /// Progman in z-order (i.e. its parent handle equals Progman).
    unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = &mut *(lparam as *mut EnumState);

        // Check class name
        let mut class_name = [0u16; 32];
        let len = GetClassNameW(hwnd, class_name.as_mut_ptr(), class_name.len() as i32);
        if len > 0 {
            let name = String::from_utf16_lossy(&class_name[..len as usize]);
            if name == "WorkerW" {
                // Check if this WorkerW's parent is Progman
                let parent = GetWindowLongPtrW(hwnd, GWL_HWNDPARENT) as HWND;
                if parent == state.progman {
                    state.found = Some(hwnd);
                    return 0; // FALSE — stop enumeration
                }
            }
        }

        1 // TRUE — continue
    }

    /// Attach `wallpaper_hwnd` as a child of WorkerW so it renders behind
    /// desktop icons.  Returns `Ok(())` on success or an error message.
    pub fn attach_to_desktop(wallpaper_hwnd: HWND) -> Result<(), String> {
        unsafe {
            // 1. Find Progman
            let progman_class = to_wide("Progman");
            let progman = FindWindowW(progman_class.as_ptr(), ptr::null());
            if progman.is_null() {
                return Err("Progman window not found".into());
            }

            // 2. Ask Progman to spawn a WorkerW
            let mut result: usize = 0;
            SendMessageTimeoutW(
                progman,
                WM_SPAWN_WORKER,
                0,
                0,
                0x0008, // SMTO_NORMAL (we won't block long)
                1000,
                &mut result,
            );

            // 3. Enumerate to find the WorkerW whose parent is Progman
            let mut state = EnumState {
                progman,
                found: None,
            };
            EnumWindows(
                enum_callback as *const c_void,
                &mut state as *mut EnumState as LPARAM,
            );

            let workerw = match state.found {
                Some(w) => w,
                None => return Err("WorkerW window not found".into()),
            };

            // 4. Reparent the wallpaper window to WorkerW
            let prev = SetParent(wallpaper_hwnd, workerw);
            if prev.is_null() {
                return Err("SetParent failed".into());
            }

            // Position it at (0,0) covering the full WorkerW area and show it
            SetWindowPos(
                wallpaper_hwnd,
                HWND_TOP,
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );

            Ok(())
        }
    }
}

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
        // Attach wallpaper window to Windows WorkerW so it renders behind desktop icons
        #[cfg(target_os = "windows")]
        {
            match window_hwnd(&app_clone) {
                Ok(hwnd) => {
                    if let Err(e) = win_wallpaper::attach_to_desktop(hwnd) {
                        eprintln!("[Wallpaper] WorkerW attach failed: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("[Wallpaper] cannot get HWND: {}", e);
                }
            }
        }
        let _ = app_clone.emit("set-wallpaper-state", payload_clone);
    });

    Ok(())
}

/// Retrieve the raw HWND of the wallpaper window as `*mut c_void`.
#[cfg(target_os = "windows")]
fn window_hwnd(app: &AppHandle) -> Result<*mut std::ffi::c_void, String> {
    let window = app
        .get_webview_window(WALLPAPER_LABEL)
        .ok_or("wallpaper window not found")?;
    let hwnd = window
        .hwnd()
        .map_err(|e| format!("hwnd error: {}", e))?;
    Ok(hwnd.0)
}
