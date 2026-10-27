use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "macos")]
use std::sync::Mutex;
#[cfg(target_os = "macos")]
use tauri::webview::WebviewWindowBuilder;
#[cfg(target_os = "macos")]
use tauri::ActivationPolicy;
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::error::{AppError, AppResult};

/// When true, CloseRequested may destroy the window / exit the process.
static ALLOW_EXIT: AtomicBool = AtomicBool::new(false);
/// A lightweight menubar session deliberately has no main WebView.
static COMPACT_MODE: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
struct SavedGeometry {
    position: Option<tauri::PhysicalPosition<i32>>,
    size: Option<tauri::PhysicalSize<u32>>,
    maximized: bool,
}

#[cfg(target_os = "macos")]
static SAVED_GEOMETRY: Mutex<Option<SavedGeometry>> = Mutex::new(None);

pub fn is_compact_mode() -> bool {
    COMPACT_MODE.load(Ordering::SeqCst)
}

pub fn is_exit_armed() -> bool {
    ALLOW_EXIT.load(Ordering::SeqCst)
}

/// Unload the macOS main WebView instead of merely hiding it. All backend
/// state, MCP listeners, tunnels and the tray icon remain in this process.
#[tauri::command]
pub fn enter_compact_mode(app: AppHandle) -> AppResult<()> {
    #[cfg(not(target_os = "macos"))]
    {
        return hide_to_tray(app);
    }
    #[cfg(target_os = "macos")]
    {
        if is_compact_mode() {
            return Ok(());
        }
        let window = main_window(&app)?;
        if let Ok(mut geometry) = SAVED_GEOMETRY.lock() {
            *geometry = Some(SavedGeometry {
                position: window.outer_position().ok(),
                size: window.outer_size().ok(),
                maximized: window.is_maximized().unwrap_or(false),
            });
        }
        COMPACT_MODE.store(true, Ordering::SeqCst);
        // Return the IPC response before destroying its originating WebView.
        // ExitRequested checks COMPACT_MODE so the last window cannot quit MCP.
        // Switch to Accessory *after* destroying the UI, so the Dock icon also
        // disappears and only the native menu bar status item remains.
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            let policy_app = app.clone();
            let result = app.run_on_main_thread(move || {
                // An immediate Restore click can cancel the queued unload.
                if is_compact_mode() {
                    match window.destroy() {
                        Ok(()) => {
                            if let Err(error) =
                                policy_app.set_activation_policy(ActivationPolicy::Accessory)
                            {
                                eprintln!("failed to hide Dock icon in compact mode: {error}");
                            }
                        }
                        Err(error) => {
                            eprintln!("failed to unload compact WebView: {error}");
                            COMPACT_MODE.store(false, Ordering::SeqCst);
                        }
                    }
                }
            });
            if let Err(error) = result {
                eprintln!("failed to schedule compact WebView unload: {error}");
                COMPACT_MODE.store(false, Ordering::SeqCst);
            }
        });
        Ok(())
    }
}

/// Intercept user close (show confirm / keep running) unless quitting or UI recreate.
pub fn should_intercept_close() -> bool {
    !ALLOW_EXIT.load(Ordering::SeqCst) && !crate::commands::ui_memory::should_prevent_exit()
}

fn main_window(app: &AppHandle) -> AppResult<WebviewWindow> {
    app.get_webview_window("main")
        .or_else(|| {
            app.webview_windows()
                .into_iter()
                .find(|(label, _)| !label.starts_with("__"))
                .map(|(_, w)| w)
        })
        .ok_or_else(|| AppError::Message("找不到主窗口".into()))
}

#[tauri::command]
pub fn hide_to_tray(app: AppHandle) -> AppResult<()> {
    let window = main_window(&app)?;
    window
        .hide()
        .map_err(|err| AppError::Message(format!("隐藏窗口失败: {err}")))?;
    Ok(())
}

#[tauri::command]
pub fn show_main_window(app: AppHandle) -> AppResult<()> {
    #[cfg(target_os = "macos")]
    if is_compact_mode() && app.get_webview_window("main").is_some() {
        // The 150 ms IPC drain period has not elapsed; reuse the existing UI.
        COMPACT_MODE.store(false, Ordering::SeqCst);
    }
    #[cfg(target_os = "macos")]
    let window = if is_compact_mode() && app.get_webview_window("main").is_none() {
        // Recreate lazily: no hidden WKWebView needs to remain resident.
        let config = app
            .config()
            .app
            .windows
            .first()
            .ok_or_else(|| AppError::Message("找不到主窗口配置，无法恢复界面".into()))?;
        let builder = WebviewWindowBuilder::from_config(&app, config)
            .map_err(|err| AppError::Message(format!("初始化界面失败: {err}")))?;
        let window = builder
            .visible(false)
            .build()
            .map_err(|err| AppError::Message(format!("恢复界面失败: {err}")))?;
        let geometry = SAVED_GEOMETRY.lock().ok().and_then(|slot| *slot);
        if let Some(geometry) = geometry {
            if let Some(size) = geometry.size {
                if size.width >= 960 && size.height >= 640 {
                    let _ = window.set_size(tauri::Size::Physical(size));
                }
            }
            if let Some(position) = geometry.position {
                if position.x > -10_000 && position.y > -10_000 {
                    let _ = window.set_position(tauri::Position::Physical(position));
                }
            }
            if geometry.maximized {
                let _ = window.maximize();
            }
        }
        // Switch back to a regular Dock app only after rebuilding succeeds.
        // Failed activation must not strand a hidden main WebView.
        if let Err(error) = app.set_activation_policy(ActivationPolicy::Regular) {
            let _ = window.destroy();
            return Err(AppError::Message(format!("恢复 Dock 图标失败: {error}")));
        }
        COMPACT_MODE.store(false, Ordering::SeqCst);
        window
    } else {
        main_window(&app)?
    };
    #[cfg(not(target_os = "macos"))]
    let window = main_window(&app)?;
    let _ = window.unminimize();
    window
        .show()
        .map_err(|err| AppError::Message(format!("显示窗口失败: {err}")))?;
    let _ = window.set_focus();
    Ok(())
}

#[tauri::command]
pub fn quit_app(app: AppHandle) -> AppResult<()> {
    arm_allow_exit();
    app.exit(0);
    Ok(())
}

pub fn arm_allow_exit() {
    ALLOW_EXIT.store(true, Ordering::SeqCst);
}
