use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

mod commands;
mod extractor;
mod hotkeys;
mod login;
mod lyrics;
mod online_api;
mod scanner;
pub mod state;
mod wallpaper;
mod wallpaper_pkg;
mod we_control;

pub struct OnlineApiState(pub online_api::OnlineApiState);

// ── Windows work area (excludes taskbar) via raw FFI ──────────────

#[cfg(target_os = "windows")]
mod win_workarea {
    use std::mem;
    use std::ptr;

    const MONITOR_DEFAULTTOPRIMARY: u32 = 0x00000001;

    #[repr(C)]
    struct RECT {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    struct MONITORINFO {
        cb_size: u32,
        rc_monitor: RECT,
        rc_work: RECT,
        dw_flags: u32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn MonitorFromWindow(hwnd: *mut std::ffi::c_void, dw_flags: u32) -> isize;
        fn GetMonitorInfoW(hmonitor: isize, lpmi: *mut MONITORINFO) -> i32;
    }

    /// Returns work area (x, y, width, height) of the primary monitor,
    /// i.e. the desktop area excluding the taskbar.
    pub fn primary_work_area() -> Option<(i32, i32, i32, i32)> {
        unsafe {
            let hmonitor = MonitorFromWindow(ptr::null_mut(), MONITOR_DEFAULTTOPRIMARY);
            if hmonitor == 0 {
                return None;
            }
            let mut info = MONITORINFO {
                cb_size: mem::size_of::<MONITORINFO>() as u32,
                rc_monitor: RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                },
                rc_work: RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                },
                dw_flags: 0,
            };
            if GetMonitorInfoW(hmonitor, &mut info) != 0 {
                let w = info.rc_work;
                Some((w.left, w.top, w.right - w.left, w.bottom - w.top))
            } else {
                None
            }
        }
    }
}

/// Get effective desktop area (work area if available, fallback to monitor size).
fn get_effective_area(monitor: &tauri::window::Monitor) -> (i32, i32, i32, i32) {
    #[cfg(target_os = "windows")]
    if let Some(area) = win_workarea::primary_work_area() {
        return area;
    }

    let mon_w = monitor.size().width as i32;
    let mon_h = monitor.size().height as i32;
    let mon_x = monitor.position().x;
    let mon_y = monitor.position().y;
    (mon_x, mon_y, mon_w, mon_h)
}

/// Match Electron's getWindowedBounds algorithm.
/// Takes explicit area dimensions (work area or full monitor).
fn get_windowed_bounds(area_w: i32, area_h: i32, area_x: i32, area_y: i32) -> (i32, i32, u32, u32) {
    const SCALE: f64 = 3.0 / 4.0;
    const ASPECT: f64 = 16.0 / 9.0;
    const MARGIN: i32 = 32;
    const MIN_W: i32 = 960;
    const MIN_H: i32 = 540;

    let max_w = (area_w - MARGIN).max(640) as f64;
    let max_h = (area_h - MARGIN).max(360) as f64;

    let mut width = (area_w as f64 * SCALE).round();
    let mut height = (width / ASPECT).round();
    let scaled_h = (area_h as f64 * SCALE).round();

    if height > scaled_h {
        height = scaled_h;
        width = (height * ASPECT).round();
    }

    if width < MIN_W as f64 && max_w >= MIN_W as f64 && max_h >= MIN_H as f64 {
        width = MIN_W as f64;
        height = MIN_H as f64;
    }

    if width > max_w {
        width = max_w;
        height = (width / ASPECT).round();
    }
    if height > max_h {
        height = max_h;
        width = (height * ASPECT).round();
    }

    let w = width.round() as u32;
    let h = height.round() as u32;
    let x = area_x + (area_w - w as i32) / 2;
    let y = area_y + (area_h - h as i32) / 2;

    (x, y, w, h)
}

fn resolve_cookie_dir() -> PathBuf {
    // In dev, use the project root's sidecar directory (for backward compat)
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sidecar/");
    if dev.exists() {
        return std::fs::canonicalize(&dev).unwrap_or(dev);
    }
    // In production, use the exe directory
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            return dir.to_path_buf();
        }
    }
    PathBuf::from(".")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::scan_folder,
            commands::extract_cover,
            commands::sidecar_call,
            crate::state::load_state,
            crate::state::save_state,
            crate::lyrics::toggle_desktop_lyrics,
            crate::lyrics::update_desktop_lyrics,
            crate::lyrics::move_lyrics_by,
            crate::lyrics::set_lyrics_lock_state,
            crate::lyrics::set_lyrics_drag,
            crate::lyrics::set_lyrics_hot_bounds,
            crate::wallpaper::toggle_wallpaper_mode,
            crate::wallpaper::update_wallpaper_mode,
            crate::wallpaper_pkg::import_wallpaper_pkg,
            crate::we_control::find_we,
            crate::we_control::list_we_wallpapers,
            crate::we_control::open_we_wallpaper,
            crate::we_control::sync_we_window,
            crate::we_control::control_we,
            crate::we_control::get_we_preview,
            crate::we_control::get_we_video_path,
            crate::we_control::close_we_wallpaper,
            crate::login::open_netease_music_login,
            crate::login::open_qq_music_login,
            crate::login::clear_netease_music_login,
            crate::login::clear_qq_music_login,
            commands::pick_folder,
            commands::export_json_file,
            commands::import_json_file,
            hotkeys::configure_global_hotkeys,
        ])
        .setup(|app| {
            let cookie_dir = resolve_cookie_dir();
            app.manage(OnlineApiState(online_api::OnlineApiState::new(cookie_dir)));

            app.manage(crate::lyrics::LyricsState::default());
            app.manage(crate::login::LoginWindowState::default());
            app.manage(crate::login::LoginPending(Mutex::new(None)));
            crate::lyrics::setup_listeners(app.handle());

            // Dynamic window sizing matching Electron's getWindowedBounds
            if let Some(window) = app.get_webview_window("main") {
                if let Ok(Some(monitor)) = window.primary_monitor() {
                    let (ax, ay, aw, ah) = get_effective_area(&monitor);
                    let (x, y, w, h) = get_windowed_bounds(aw, ah, ax, ay);
                    let _ = window.set_min_size(Some(tauri::PhysicalSize::new(960, 540)));
                    let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(w, h)));
                    let _ = window.set_position(tauri::Position::Physical(
                        tauri::PhysicalPosition::new(x, y),
                    ));
                }
            // 主窗口透明由 tauri.conf.json 的 transparent:true 控制，
            // 不再设不透明底色，否则会盖掉透明、无法透出 WE 窗口。
            let _ = window.show();

            // 原生窗口事件监听：替代前端轮询 sync_we_window，减少 IPC 延迟
            let win2 = window.clone();
            let _ = window.on_window_event(move |event| {
                if let tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) = event {
                    let _ = win2.inner_size().map(|sz| {
                        let _ = win2.hwnd().map(|h| {
                            crate::we_control::sync_bg(h.0, sz.width, sz.height);
                        });
                    });
                }
            });
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            #[cfg(target_os = "windows")] { crate::we_control::hide_bg_window(); }
            crate::we_control::kill_we_child_processes();
        }
    });
}
