use base64::Engine;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager};

const WE_WINDOW_NAME: &str = "MusicPlayerBG";
const WORKSHOP_APPID: &str = "431960";

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct WeWallpaper {
    pub title: String,
    pub r#type: String,
    pub preview: Option<String>,
    pub project_json: String,
    pub file: Option<String>,
}

#[derive(serde::Serialize)]
pub struct WeStatus { pub available: bool, pub running: bool, pub exe: Option<String> }

#[cfg(target_os = "windows")]
mod win {
    use std::ffi::c_void;
    use std::ptr;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    use crate::we_control::WE_WINDOW_NAME;

    const SW_HIDE: i32 = 0;
    const SW_SHOWNA: i32 = 8;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_SHOWWINDOW: u32 = 0x0040;
    const SWP_NOSENDCHANGING: u32 = 0x0400;
    const HWND_TOP: *mut c_void = 0 as *mut c_void;

    #[link(name = "user32")]
    extern "system" {
        fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> *mut c_void;
        fn ShowWindow(hwnd: *mut c_void, nCmdShow: i32) -> i32;
        fn GetWindowTextLengthW(hwnd: *mut c_void) -> i32;
        fn GetWindowTextW(hwnd: *mut c_void, lpString: *mut u16, nMaxCount: i32) -> i32;
        fn GetClassNameW(hwnd: *mut c_void, lpClassName: *mut u16, nMaxCount: i32) -> i32;
        fn EnumWindows(lpEnumFunc: *const c_void, lParam: isize) -> i32;
        fn EnumChildWindows(hWndParent: *mut c_void, lpEnumFunc: *const c_void, lParam: isize) -> i32;
        fn GetWindowThreadProcessId(hwnd: *mut c_void, lpdwProcessId: *mut u32) -> u32;
        fn IsWindowVisible(hwnd: *mut c_void) -> i32;
        fn GetWindowRect(hwnd: *mut c_void, lpRect: *mut windows::Win32::Foundation::RECT) -> i32;
        fn GetClientRect(hwnd: *mut c_void, lpRect: *mut windows::Win32::Foundation::RECT) -> i32;
        fn ClientToScreen(hwnd: *mut c_void, lpPoint: *mut windows::Win32::Foundation::POINT) -> i32;
        fn SetWindowPos(hwnd: *mut c_void, hWndInsertAfter: *mut c_void, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
        fn SetWindowLongPtrW(hwnd: *mut c_void, nIndex: i32, dwNewLong: isize) -> isize;
    }

    pub fn to_wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0u16)).collect() }

    pub fn find_we_window() -> Option<*mut c_void> {
        let name = to_wide(WE_WINDOW_NAME);
        unsafe { let h = FindWindowW(ptr::null(), name.as_ptr()); if !h.is_null() { return Some(h); } }
        let pids = find_we_pids();
        if pids.is_empty() { return None; }
        struct PidState { pids: Vec<u32>, found: Option<*mut c_void>, fallback: Option<*mut c_void> }
        unsafe extern "system" fn pid_cb(hwnd: *mut c_void, lparam: isize) -> i32 {
            let s = &mut *(lparam as *mut PidState);
            if s.found.is_some() { return 0; }
            let mut pid: u32 = 0; GetWindowThreadProcessId(hwnd, &mut pid);
            if s.pids.contains(&pid) {
                let sub = WE_WINDOW_NAME.to_lowercase();
                let len = GetWindowTextLengthW(hwnd);
                if len > 0 { let mut buf = vec![0u16; (len+1) as usize]; let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), (len+1) as i32); if n > 0 { let t = String::from_utf16_lossy(&buf[..n as usize]).to_lowercase(); if t.contains(&sub) { s.found = Some(hwnd); return 0; } } }
                let mut cbuf = [0u16; 256]; let cl = GetClassNameW(hwnd, cbuf.as_mut_ptr(), 256);
                if cl > 0 { let c = String::from_utf16_lossy(&cbuf[..cl as usize]).to_lowercase(); if c.contains(&sub) { s.found = Some(hwnd); return 0; } }
                if s.fallback.is_none() && IsWindowVisible(hwnd) != 0 { let mut r = windows::Win32::Foundation::RECT { left:0,top:0,right:0,bottom:0 }; if GetWindowRect(hwnd,&mut r)!=0 && r.right-r.left>10 && r.bottom-r.top>10 { s.fallback = Some(hwnd); } }
            }
            EnumChildWindows(hwnd, pid_cb as *const c_void, lparam);
            if s.found.is_some() { return 0; } 1
        }
        let mut st = PidState { pids, found: None, fallback: None };
        unsafe { EnumWindows(pid_cb as *const c_void, &mut st as *mut PidState as isize); }
        st.found.or(st.fallback)
    }

    fn find_we_pids() -> Vec<u32> {
        use sysinfo::System; let sys = System::new_all(); let sub = WE_WINDOW_NAME.to_lowercase();
        let mut pids = Vec::new();
        for (pid, process) in sys.processes() {
            let name = process.name().to_string_lossy().to_lowercase();
            let ne = name.trim_end_matches(".exe");
            if name == "wallpaper64.exe" || name == "wallpaper32.exe" || ne == sub { pids.push(pid.as_u32()); }
        }
        pids
    }

    // ── 直接定位 WE 渲染窗口到主窗口背后 ──────────────────────────
    // 不创建 bg 窗口、不用 DWM 缩略图。
    // WE 的 `-playInWindow MusicPlayerBG` 已创建渲染窗口，
    // 我们只需把它移到主窗口背后、加到 WS_EX_TRANSPARENT 让点击穿透。

    static EMBED_INITIALIZED: AtomicBool = AtomicBool::new(false);
    static LAST_X: Mutex<Option<i32>> = Mutex::new(None);
    static LAST_Y: Mutex<Option<i32>> = Mutex::new(None);
    static LAST_W: Mutex<Option<u32>> = Mutex::new(None);
    static LAST_H: Mutex<Option<u32>> = Mutex::new(None);

    pub fn embed_we_window(main_hwnd: *mut c_void, _width: u32, _height: u32) -> Result<(), String> {
        use windows::Win32::Foundation::{POINT, RECT};

        let we_hwnd = find_we_window().ok_or("未找到 WE 渲染窗口")?;

        if !EMBED_INITIALIZED.load(Ordering::Acquire) {
            eprintln!("[we-bg] 首次初始化 we={:p} main={:p}", we_hwnd, main_hwnd);
            unsafe { ShowWindow(we_hwnd, SW_SHOWNA); }

            unsafe {
                let old = SetWindowLongPtrW(we_hwnd, -20, 0); // GWL_EXSTYLE
                SetWindowLongPtrW(we_hwnd, -20, old | 0x00000020 | 0x08000000);
                // 圆角适配主窗口
                let pref: u32 = 2; // DWMWCP_ROUND
                let _ = windows::Win32::Graphics::Dwm::DwmSetWindowAttribute(
                    windows::Win32::Foundation::HWND(we_hwnd),
                    windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE(33),
                    &pref as *const u32 as *const std::ffi::c_void,
                    std::mem::size_of::<u32>() as u32,
                );
            }

            let mut cr = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            let mut pt = POINT { x: 0, y: 0 };
            if unsafe { GetClientRect(main_hwnd, &mut cr) != 0 && ClientToScreen(main_hwnd, &mut pt) != 0 } {
                let cw = (cr.right - cr.left) as u32;
                let ch = (cr.bottom - cr.top) as u32;
                unsafe {
                    SetWindowPos(main_hwnd, HWND_TOP, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                    SetWindowPos(we_hwnd, main_hwnd, pt.x, pt.y, cw as i32, ch as i32, SWP_NOACTIVATE | SWP_NOSENDCHANGING | SWP_SHOWWINDOW);
                }
                *LAST_X.lock().unwrap() = Some(pt.x);
                *LAST_Y.lock().unwrap() = Some(pt.y);
                *LAST_W.lock().unwrap() = Some(cw);
                *LAST_H.lock().unwrap() = Some(ch);
            }
            EMBED_INITIALIZED.store(true, Ordering::Release);
        }

        let mut cr = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        let mut pt = POINT { x: 0, y: 0 };
        unsafe {
            if GetClientRect(main_hwnd, &mut cr) != 0 && ClientToScreen(main_hwnd, &mut pt) != 0 {
                let cw = (cr.right - cr.left) as u32;
                let ch = (cr.bottom - cr.top) as u32;
                SetWindowPos(we_hwnd, main_hwnd, pt.x, pt.y, cw as i32, ch as i32, SWP_NOACTIVATE | SWP_NOSENDCHANGING);
                *LAST_X.lock().unwrap() = Some(pt.x);
                *LAST_Y.lock().unwrap() = Some(pt.y);
                *LAST_W.lock().unwrap() = Some(cw);
                *LAST_H.lock().unwrap() = Some(ch);
            }
        }

        Ok(())
    }

    pub fn sync_bg(main_hwnd: *mut c_void, _width: u32, _height: u32) {
        use windows::Win32::Foundation::{POINT, RECT};
        if !EMBED_INITIALIZED.load(Ordering::Acquire) { return; }
        let we_hwnd = match find_we_window() { Some(h) => h, None => return };

        unsafe {
            let mut cr = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            let mut pt = POINT { x: 0, y: 0 };
            if GetClientRect(main_hwnd, &mut cr) != 0 && ClientToScreen(main_hwnd, &mut pt) != 0 {
                let cw = (cr.right - cr.left) as u32;
                let ch = (cr.bottom - cr.top) as u32;
                let mut lx = LAST_X.lock().unwrap();
                let mut ly = LAST_Y.lock().unwrap();
                let mut lw = LAST_W.lock().unwrap();
                let mut lh = LAST_H.lock().unwrap();
                let px = lx.map(|v| v != pt.x).unwrap_or(true);
                let py = ly.map(|v| v != pt.y).unwrap_or(true);
                let sx = lw.map(|v| v != cw).unwrap_or(true);
                let sy = lh.map(|v| v != ch).unwrap_or(true);
                if px || py || sx || sy {
                    SetWindowPos(we_hwnd, main_hwnd, pt.x, pt.y, cw as i32, ch as i32, SWP_NOACTIVATE | SWP_NOSENDCHANGING);
                    *lx = Some(pt.x); *ly = Some(pt.y);
                    *lw = Some(cw); *lh = Some(ch);
                }
            }
        }
    }

    pub fn hide_we_window() {
        if let Some(hwnd) = find_we_window() {
            unsafe { ShowWindow(hwnd, SW_HIDE); }
        }
        EMBED_INITIALIZED.store(false, Ordering::Release);
        *LAST_X.lock().unwrap() = None;
        *LAST_Y.lock().unwrap() = None;
        *LAST_W.lock().unwrap() = None;
        *LAST_H.lock().unwrap() = None;
    }
}

pub fn kill_we_child_processes() {
    use sysinfo::System; let sub = WE_WINDOW_NAME.to_lowercase(); let sys = System::new_all();
    for (pid, process) in sys.processes() {
        let name = process.name().to_string_lossy().to_lowercase();
        if name.trim_end_matches(".exe") == sub { eprintln!("[we-bg] 杀死残留进程 {} (PID {})", name, pid); process.kill(); }
    }
}

#[cfg(target_os = "windows")]
pub fn sync_bg(hwnd: *mut std::ffi::c_void, width: u32, height: u32) {
    win::sync_bg(hwnd, width, height);
}

#[cfg(target_os = "windows")]
pub fn hide_bg_window() {
    win::hide_we_window();
}

#[cfg(target_os = "windows")]
fn steam_install_path() -> Option<PathBuf> {
    use std::ffi::c_void; use std::ptr;
    #[link(name = "advapi32")] extern "system" {
        fn RegOpenKeyExW(hKey:*mut c_void,lpSubKey:*const u16,ulOptions:u32,samDesired:u32,phkResult:*mut*mut c_void)->i32;
        fn RegQueryValueExW(hKey:*mut c_void,lpValueName:*const u16,lpReserved:*mut u32,lpType:*mut u32,lpData:*mut u8,lpcbData:*mut u32)->i32;
        fn RegCloseKey(hKey:*mut c_void)->i32;
    }
    const HKLM: *mut c_void = 0x80000002 as *mut c_void; const KEY_READ: u32 = 0x20019;
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0u16)).collect::<Vec<u16>>();
    for subkey in ["SOFTWARE\\Valve\\Steam", "SOFTWARE\\WOW6432Node\\Valve\\Steam"] {
        let mut hkey = ptr::null_mut(); let sk = wide(subkey);
        if unsafe { RegOpenKeyExW(HKLM, sk.as_ptr(), 0, KEY_READ, &mut hkey) } != 0 || hkey.is_null() { continue; }
        let name = wide("InstallPath"); let mut buf = [0u8; 512]; let mut cb = buf.len() as u32;
        let q = unsafe { RegQueryValueExW(hkey, name.as_ptr(), ptr::null_mut(), ptr::null_mut(), buf.as_mut_ptr(), &mut cb) };
        unsafe { RegCloseKey(hkey); }
        if q == 0 && cb > 0 {
            let s = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u16, (cb as usize)/2) });
            let p = s.trim_end_matches('\0').to_string(); if !p.is_empty() { return Some(PathBuf::from(p)); }
        }
    }
    None
}

#[cfg(not(target_os = "windows"))] fn steam_install_path() -> Option<PathBuf> { None }

fn locate_exe() -> Option<PathBuf> {
    for c in &[r"C:\Program Files (x86)\Steam\steamapps\common\WallpaperEngine\wallpaper64.exe", r"C:\Program Files\Steam\steamapps\common\WallpaperEngine\wallpaper64.exe", r"C:\Program Files\WallpaperEngine\wallpaper64.exe", r"C:\Program Files (x86)\WallpaperEngine\wallpaper64.exe"] { let p = PathBuf::from(c); if p.exists() { return Some(p); } }
    if let Some(sp) = steam_install_path() { let p = sp.join("steamapps").join("common").join("WallpaperEngine").join("wallpaper64.exe"); if p.exists() { return Some(p); } }
    for sr in steam_roots() { let p = sr.join("steamapps").join("common").join("WallpaperEngine").join("wallpaper64.exe"); if p.exists() { return Some(p); } }
    None
}

fn steam_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for base in [PathBuf::from(r"C:\Program Files (x86)\Steam"), PathBuf::from(r"C:\Program Files\Steam")] {
        if base.exists() { roots.push(base.clone()); }
        if let Ok(text) = std::fs::read_to_string(base.join("steamapps").join("libraryfolders.vdf")) { for p in regex_literal_path(&text) { roots.push(PathBuf::from(p)); } }
    }
    if let Some(sp) = steam_install_path() { if sp.exists() { roots.push(sp); } }
    roots
}

fn regex_literal_path(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some(idx) = line.find("\"path\"") {
            let rest = &line[idx + 6..];
            if let Some(start) = rest.find('"') { if let Some(end) = rest[start+1..].find('"') { let val = rest[start+1..][..end].to_string(); if !val.is_empty() { out.push(val); } } }
        }
    }
    out
}

fn running_exe() -> Option<PathBuf> {
    use sysinfo::System; let sys = System::new_all();
    for (_pid, process) in sys.processes() {
        let name = process.name().to_string_lossy().to_lowercase();
        if name == "wallpaper64.exe" || name == "wallpaper32.exe" { if let Some(exe) = process.exe() { return Some(exe.to_path_buf()); } }
    }
    None
}

fn find_we_exe() -> Option<PathBuf> { running_exe().or_else(locate_exe) }

fn ensure_we_running() -> Result<PathBuf, String> {
    if let Some(p) = running_exe() { return Ok(p); }
    let p = locate_exe().ok_or("未检测到 Wallpaper Engine，请先安装")?;
    run_control(&p, &[]).map_err(|e| format!("启动 Wallpaper Engine 失败: {}", e))?;
    Ok(p)
}

fn find_steam_root(exe: &Path) -> Option<PathBuf> {
    let mut cur = exe.parent();
    while let Some(dir) = cur { if dir.join("steamapps").is_dir() { return Some(dir.to_path_buf()); } cur = dir.parent(); }
    None
}

fn preview_data_url(json_dir: &Path, rel: &str) -> Option<String> {
    let path = json_dir.join(rel); if !path.is_file() { return None; }
    let mime = match path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()).as_deref() { Some("jpg")|Some("jpeg")=>"image/jpeg", Some("png")=>"image/png", Some("gif")=>"image/gif", Some("webp")=>"image/webp", Some("bmp")=>"image/bmp", _=>return None };
    let bytes = std::fs::read(&path).ok()?; let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Some(format!("data:{};base64,{}", mime, b64))
}

fn parse_wallpaper(json_path: &Path) -> Option<WeWallpaper> {
    let text = std::fs::read_to_string(json_path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let title = v.get("title").and_then(|t| t.as_str()).unwrap_or("未命名壁纸").to_string();
    let r#type = v.get("type").and_then(|t| t.as_str()).unwrap_or("").to_string();
    let file = v.get("file").and_then(|f| f.as_str()).map(|s| s.to_string());
    Some(WeWallpaper { title, r#type, preview: None, project_json: json_path.to_string_lossy().to_string(), file })
}

fn placeholder_wallpaper(json_path: &Path) -> WeWallpaper { WeWallpaper { title: "（壁纸数据缺失）".into(), r#type: String::new(), preview: None, project_json: json_path.to_string_lossy().to_string(), file: None } }

fn scan_dir(dir: &Path, out: &mut Vec<WeWallpaper>) {
    if !dir.is_dir() { return; }
    if let Ok(entries) = std::fs::read_dir(dir) { for entry in entries.flatten() { let p = entry.path(); if p.is_dir() { let json = p.join("project.json"); if json.is_file() { match parse_wallpaper(&json) { Some(w) => out.push(w), None => out.push(placeholder_wallpaper(&json)) } } } } }
}

fn list_wallpapers() -> Vec<WeWallpaper> {
    let mut out = Vec::new();
    if let Some(exe) = find_we_exe() {
        if let Some(projects) = exe.parent().map(|d| d.join("projects")) { scan_dir(&projects, &mut out); }
        if let Some(sr) = find_steam_root(&exe) { scan_dir(&sr.join("steamapps").join("workshop").join("content").join(WORKSHOP_APPID), &mut out); }
    }
    out
}

fn count_in_dir(dir: &Path) -> usize { if !dir.is_dir() { return 0; } let mut c = 0; if let Ok(entries) = std::fs::read_dir(dir) { for entry in entries.flatten() { if entry.path().is_dir() && entry.path().join("project.json").is_file() { c += 1; } } } c }

fn count_wallpaper_dirs() -> usize {
    let mut c = 0;
    if let Some(exe) = find_we_exe() {
        if let Some(projects) = exe.parent().map(|d| d.join("projects")) { c += count_in_dir(&projects); }
        if let Some(sr) = find_steam_root(&exe) { c += count_in_dir(&sr.join("steamapps").join("workshop").join("content").join(WORKSHOP_APPID)); }
    }
    c
}

fn cache_path(_app: &AppHandle) -> PathBuf {
    // 优先 exe 目录（卸载时随安装目录一起清理），失败则 temp
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("we_wallpapers_cache.json");
            // 检查目录是否可写（非 Program Files 等只读位置）
            let test = dir.join(".we_cache_test");
            if std::fs::write(&test, "").is_ok() { let _ = std::fs::remove_file(&test); return p; }
        }
    }
    std::env::temp_dir().join("we_wallpapers_cache.json")
}
fn read_cache(path: &Path) -> Option<Vec<WeWallpaper>> { let text = std::fs::read_to_string(path).ok()?; serde_json::from_str(&text).ok() }
fn write_cache(path: &Path, items: &[WeWallpaper]) { if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); } if let Ok(json) = serde_json::to_string(items) { let _ = std::fs::write(path, json); } }

#[cfg(target_os = "windows")]
fn run_control(exe: &Path, args: &[&str]) -> Result<(), String> {
    use std::os::windows::process::CommandExt; use std::process::Stdio;
    Command::new(exe).args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).creation_flags(0x0800_0000|0x0000_0008).spawn().map_err(|e| format!("调用 Wallpaper Engine CLI 失败: {}", e))?;
    Ok(())
}
#[cfg(not(target_os = "windows"))]
fn run_control(exe: &Path, args: &[&str]) -> Result<(), String> {
    use std::process::Stdio; Command::new(exe).args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e| format!("调用 Wallpaper Engine CLI 失败: {}", e))?;
    Ok(())
}

// ── Tauri 命令 ────────────────────────────────────────────────────

#[tauri::command]
pub fn find_we() -> WeStatus {
    #[cfg(target_os = "windows")] { match find_we_exe() { Some(exe) => { let r = running_exe().is_some(); WeStatus { available: true, running: r, exe: exe.to_str().map(|s| s.to_string()) } }, None => WeStatus { available: false, running: false, exe: None } } }
    #[cfg(not(target_os = "windows"))] WeStatus { available: false, running: false, exe: None }
}

#[tauri::command]
pub fn list_we_wallpapers(app: AppHandle) -> Result<Vec<WeWallpaper>, String> {
    if find_we_exe().is_none() { return Err("未检测到 Wallpaper Engine".into()); }
    let cp = cache_path(&app);
    if let Some(cached) = read_cache(&cp) {
        if count_wallpaper_dirs() == cached.len() {
            let v: Vec<WeWallpaper> = cached.into_iter().map(|w| { let valid = Path::new(&w.project_json).parent().map(|d| d.is_dir() && d.join("project.json").is_file()).unwrap_or(false); if valid { w } else { placeholder_wallpaper(Path::new(&w.project_json)) } }).collect();
            return Ok(v);
        }
    }
    let scanned = list_wallpapers(); write_cache(&cp, &scanned); Ok(scanned)
}

#[tauri::command]
pub fn open_we_wallpaper(project_json: String, _app: tauri::AppHandle) -> Result<(), String> {
    let exe = ensure_we_running()?;
    run_control(&exe, &["-control", "openWallpaper", "-file", &project_json, "-playInWindow", WE_WINDOW_NAME, "-width", "1280", "-height", "720", "-borderless"])
}

#[tauri::command]
pub fn sync_we_window(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "windows")] {
        let win = app.get_webview_window("main").ok_or_else(|| "未找到主窗口 main".to_string())?;
        let sz = win.inner_size().map_err(|e| e.to_string())?;
        let par = win.hwnd().map_err(|e| e.to_string())?;
        crate::we_control::win::embed_we_window(par.0, sz.width, sz.height)
    }
    #[cfg(not(target_os = "windows"))] { let _ = app; Err("仅支持 Windows".into()) }
}

#[tauri::command]
pub fn control_we(action: String) -> Result<(), String> {
    let exe = ensure_we_running()?;
    let sub = match action.as_str() { "play"=>"play", "pause"=>"pause", "mute"=>"mute", "unmute"=>"unmute", "close"=>"closeWallpaper", other=>return Err(format!("不支持的 WE 控制动作: {}", other)) };
    run_control(&exe, &["-control", sub])
}

#[tauri::command]
pub fn get_we_preview(project_json: String) -> Option<String> {
    let path = std::path::Path::new(&project_json);
    let dir = path.parent()?;
    let text = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let rel = v.get("preview")?.as_str()?;
    preview_data_url(dir, rel)
}

#[tauri::command]
pub fn get_we_video_path(project_json: String) -> Option<String> {
    let path = std::path::Path::new(&project_json);
    let dir = path.parent()?;
    let text = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let rel = v.get("file")?.as_str()?;
    let abs = dir.join(rel);
    if abs.is_file() { Some(abs.to_string_lossy().to_string()) } else { None }
}

#[tauri::command]
pub fn close_we_wallpaper() -> Result<(), String> {
    #[cfg(target_os = "windows")] { crate::we_control::win::hide_we_window(); }
    kill_we_child_processes();
    if let Some(exe) = find_we_exe() { let _ = run_control(&exe, &["-control", "closeWallpaper"]); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*; use std::io::Write;
    #[test]
    fn parse_wallpaper_reads_metadata() {
        let dir = std::env::temp_dir().join("we_control_test"); let _ = std::fs::create_dir_all(&dir);
        let jp = dir.join("project.json"); let mut f = std::fs::File::create(&jp).unwrap();
        f.write_all(b"{\"title\":\"test\",\"type\":\"scene\",\"preview\":\"preview.jpg\"}").unwrap();
        std::fs::File::create(dir.join("preview.jpg")).unwrap().write_all(b"x").unwrap();
        let w = parse_wallpaper(&jp).expect("parse");
        assert_eq!(w.title, "测试壁纸"); assert_eq!(w.r#type, "scene"); assert!(w.preview.is_some()); assert_eq!(w.project_json, jp.to_string_lossy());
    }
}
