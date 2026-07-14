use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri::webview::PageLoadEvent;

const LOGIN_LABEL: &str = "web-login";

/// Which login provider the current login window is for.
#[derive(Debug, Clone, Copy, PartialEq)]
enum LoginProvider {
    Netease,
    QQ,
}

/// Pending login result sender — used to communicate the extracted cookie
/// from the page-load callback back to the awaiting command.
pub struct LoginPending(pub Mutex<Option<tokio::sync::oneshot::Sender<serde_json::Value>>>);

/// Shared state to track the active login window's provider.
pub struct LoginWindowState {
    provider: Mutex<Option<LoginProvider>>,
}

impl Default for LoginWindowState {
    fn default() -> Self {
        Self {
            provider: Mutex::new(None),
        }
    }
}

/// Open a web login window for Netease Cloud Music.
///
/// Opens `https://music.163.com/#/login` in a new window. When the user
/// logs in and the page navigates to the user home, we inject JS to read
/// `document.cookie` and resolve the promise with `{ ok, cookie }`.
#[tauri::command]
pub async fn open_netease_music_login(app: AppHandle) -> Result<serde_json::Value, String> {
    open_login_window(&app, LoginProvider::Netease, "https://music.163.com/#/login").await
}

/// Open a web login window for QQ Music.
/// Uses the QQ Connect/xlogin page directly so the QR code is for account
/// login, not the "download QQ" prompt shown on the y.qq.com profile page.
#[tauri::command]
pub async fn open_qq_music_login(app: AppHandle) -> Result<serde_json::Value, String> {
    open_login_window(
        &app,
        LoginProvider::QQ,
        "https://xui.ptlogin2.qq.com/cgi-bin/xlogin?appid=716027609&daid=383&pt_skey_valid=0&style=40&s_url=https%3A%2F%2Fy.qq.com%2Fportal%2Fprofile.html",
    )
    .await
}

/// Clear the Netease cookie.
#[tauri::command]
pub fn clear_netease_music_login(app: AppHandle) -> Result<(), String> {
    let state = app.state::<crate::OnlineApiState>();
    state.0.set_cookie("");
    Ok(())
}

/// Clear the QQ Music cookie.
#[tauri::command]
pub fn clear_qq_music_login(app: AppHandle) -> Result<(), String> {
    let state = app.state::<crate::OnlineApiState>();
    state.0.set_qq_cookie("");
    Ok(())
}

async fn open_login_window(
    app: &AppHandle,
    provider: LoginProvider,
    url: &str,
) -> Result<serde_json::Value, String> {
    // If a login window is already open, reject
    if app.get_webview_window(LOGIN_LABEL).is_some() {
        return Err("登录窗口已打开".into());
    }

    // Set the provider in shared state
    {
        let state = app.state::<LoginWindowState>();
        let lock_result = state.provider.lock();
        if let Ok(mut guard) = lock_result {
            *guard = Some(provider);
        }
    }

    let title = match provider {
        LoginProvider::Netease => "网易云音乐登录",
        LoginProvider::QQ => "QQ 音乐登录",
    };

    // Create a oneshot channel to receive the result
    let (tx, rx) = tokio::sync::oneshot::channel::<serde_json::Value>();
    {
        let pending = app.state::<LoginPending>();
        let lock_result = pending.0.lock();
        if let Ok(mut guard) = lock_result {
            *guard = Some(tx);
        }
    }

    let parsed_url: tauri::Url = url
        .parse()
        .map_err(|e: <tauri::Url as std::str::FromStr>::Err| e.to_string())?;

    let app_for_load = app.clone();

    let window = WebviewWindowBuilder::new(app, LOGIN_LABEL, WebviewUrl::External(parsed_url))
        .title(title)
        .inner_size(1000.0, 700.0)
        .resizable(true)
        .decorations(true)
        .on_page_load(move |win, payload| {
            if payload.event() != PageLoadEvent::Finished {
                return;
            }

            let state = app_for_load.state::<LoginWindowState>();
            let current_provider = state
                .provider
                .lock()
                .ok()
                .and_then(|guard| *guard)
                .unwrap_or(LoginProvider::Netease);

            let url_str = payload.url().to_string();

            match current_provider {
                LoginProvider::Netease => {
                    // After Netease login, URL contains "user" or "home"
                    if url_str.contains("music.163.com")
                        && (url_str.contains("user") || url_str.contains("home"))
                    {
                        let app_clone = app_for_load.clone();
                        let win_clone = win.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(2000));
                            let _ = try_extract_cookie(&app_clone, &win_clone, LoginProvider::Netease);
                        });
                    }
                }
                LoginProvider::QQ => {
                    if url_str.contains("y.qq.com") {
                        // For QQ, poll periodically since the URL doesn't change on login
                        let app_clone = app_for_load.clone();
                        let win_clone = win.clone();
                        std::thread::spawn(move || {
                            for _ in 0..100 {
                                std::thread::sleep(std::time::Duration::from_secs(3));
                                if app_clone.get_webview_window(LOGIN_LABEL).is_none() {
                                    return;
                                }
                                match try_extract_cookie(&app_clone, &win_clone, LoginProvider::QQ) {
                                    Ok(()) => return,
                                    Err(_) => continue,
                                }
                            }
                        });
                    }
                }
            }
        })
        .build()
        .map_err(|e| format!("failed to create login window: {}", e))?;

    // Clean up on manual window close
    let app_for_close = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Destroyed = event {
            // Clear provider state
            let state = app_for_close.state::<LoginWindowState>();
            let lock_result = state.provider.lock();
            if let Ok(mut guard) = lock_result {
                *guard = None;
            }
            // Send a "cancelled" result if the pending sender is still there
            let pending = app_for_close.state::<LoginPending>();
            let lock_result = pending.0.lock();
            if let Ok(mut guard) = lock_result {
                if let Some(tx) = guard.take() {
                    let _ = tx.send(serde_json::json!({
                        "ok": false,
                        "message": "登录窗口已关闭"
                    }));
                }
            }
        }
    });

    // Wait for the result (with a 5-minute timeout)
    let result = match tokio::time::timeout(std::time::Duration::from_secs(300), rx).await {
        Ok(Ok(val)) => val,
        Ok(Err(_)) => serde_json::json!({ "ok": false, "message": "登录通道已关闭" }),
        Err(_) => {
            // Timeout — close the window and return error
            let _ = window.close();
            serde_json::json!({ "ok": false, "message": "登录超时（5分钟）" })
        }
    };

    Ok(result)
}

/// Inject JavaScript to read `document.cookie`, send it to the sidecar,
/// emit the result via the pending channel, and close the window.
fn try_extract_cookie(
    app: &AppHandle,
    window: &tauri::WebviewWindow,
    provider: LoginProvider,
) -> Result<(), String> {
    // Use eval_with_callback to get document.cookie
    let (cookie_tx, cookie_rx) = std::sync::mpsc::channel::<String>();

    window
        .eval_with_callback("document.cookie", move |cookie_value: String| {
            let _ = cookie_tx.send(cookie_value);
        })
        .map_err(|e| format!("eval error: {}", e))?;

    let cookie_value = cookie_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| format!("cookie read timeout: {}", e))?;

    if cookie_value.is_empty() {
        return Err("cookie is empty".into());
    }

    // For QQ Music, check if we actually have a login cookie
    if provider == LoginProvider::QQ {
        if !cookie_value.contains("uin") && !cookie_value.contains("qqmusic_key") {
            return Err("QQ login cookie not found yet".into());
        }
    }

    // Save the cookie directly to the OnlineApiState
    let state = app.state::<crate::OnlineApiState>();
    match provider {
        LoginProvider::Netease => state.0.set_cookie(&cookie_value),
        LoginProvider::QQ => state.0.set_qq_cookie(&cookie_value),
    }

    // Send the result via the pending channel
    let pending = app.state::<LoginPending>();
    let lock_result = pending.0.lock();
    if let Ok(mut guard) = lock_result {
        if let Some(tx) = guard.take() {
            let _ = tx.send(serde_json::json!({
                "ok": true,
                "cookie": cookie_value,
            }));
        }
    }

    // Also emit an event for any listeners
    let _ = app.emit(
        "web-login-result",
        serde_json::json!({
            "ok": true,
            "provider": match provider {
                LoginProvider::Netease => "netease",
                LoginProvider::QQ => "qq",
            },
        }),
    );

    // Close the login window
    let _ = window.close();

    // Clear provider state
    let login_state = app.state::<LoginWindowState>();
    let lock_result = login_state.provider.lock();
    if let Ok(mut guard) = lock_result {
        *guard = None;
    }

    Ok(())
}
