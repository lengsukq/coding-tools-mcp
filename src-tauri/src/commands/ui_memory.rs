use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::command;
use tauri::webview::WebviewWindowBuilder;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl};

use crate::error::{AppError, AppResult};

/// When true, `RunEvent::ExitRequested` must call `prevent_exit` so destroying
/// the sole window during UI recreate does not kill MCP/FRP with the process.
static UI_RECREATING: AtomicBool = AtomicBool::new(false);

const KEEPALIVE_LABEL: &str = "__ui_recreate_keepalive__";

pub fn should_prevent_exit() -> bool {
    UI_RECREATING.load(Ordering::SeqCst)
}

struct RecreateGuard;

impl RecreateGuard {
    fn enter() -> Self {
        UI_RECREATING.store(true, Ordering::SeqCst);
        Self
    }
}

impl Drop for RecreateGuard {
    fn drop(&mut self) {
        UI_RECREATING.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebviewMemorySample {
    /// Desktop host process working set (MB).
    pub main_mb: f64,
    /// Sum of msedgewebview2 working sets under this app (MB).
    pub webview_mb: f64,
    pub webview_process_count: u32,
    /// Whether sampling is supported on this OS.
    pub supported: bool,
}

#[cfg(windows)]
fn bytes_to_mb(bytes: u64) -> f64 {
    (bytes as f64) / (1024.0 * 1024.0)
}

/// Windows returns sentinel coords (often around -32000) for minimized windows.
fn is_sane_position(pos: &PhysicalPosition<i32>) -> bool {
    pos.x > -10_000 && pos.y > -10_000 && pos.x < 50_000 && pos.y < 50_000
}

fn is_sane_size(size: &PhysicalSize<u32>) -> bool {
    size.width >= 200 && size.height >= 200 && size.width < 20_000 && size.height < 20_000
}

/// Sample UI-related memory. Does not touch MCP/Actions/FRP runtimes.
#[command]
pub fn get_webview_memory_sample() -> AppResult<WebviewMemorySample> {
    #[cfg(windows)]
    {
        let sample = crate::platform::windows::process::sample_process_tree_memory()?;
        return Ok(WebviewMemorySample {
            main_mb: (bytes_to_mb(sample.main_bytes) * 10.0).round() / 10.0,
            webview_mb: (bytes_to_mb(sample.webview_bytes) * 10.0).round() / 10.0,
            webview_process_count: sample.webview_process_count,
            supported: true,
        });
    }
    #[cfg(not(windows))]
    {
        Ok(WebviewMemorySample {
            main_mb: 0.0,
            webview_mb: 0.0,
            webview_process_count: 0,
            supported: false,
        })
    }
}

/// Destroy and recreate the main WebView window so Edge WebView2 processes are
/// replaced. Does **not** stop MCP / Actions / FRP (`AppState` stays alive).
///
/// Must be async on Windows — synchronous WebView creation deadlocks.
///
/// Important: destroying the only window would otherwise exit the whole Tauri
/// process (0.1.30 bug). We (1) set a prevent-exit flag and (2) open a hidden
/// keepalive window first so "last window closed" never fires for the main UI.
#[command]
pub async fn recreate_ui_webview(app: AppHandle) -> AppResult<()> {
    let _guard = RecreateGuard::enter();

    // Drop any leftover keepalive from a previous failed attempt.
    if let Some(stale) = app.get_webview_window(KEEPALIVE_LABEL) {
        let _ = stale.destroy();
    }

    let window = app
        .get_webview_window("main")
        .or_else(|| {
            app.webview_windows()
                .into_iter()
                .find(|(label, _)| label.as_str() != KEEPALIVE_LABEL)
                .map(|(_, w)| w)
        })
        .ok_or_else(|| AppError::Message("no webview window to recreate".into()))?;

    let label = window.label().to_string();
    let was_hidden = !window.is_visible().unwrap_or(true);
    let was_minimized = window.is_minimized().unwrap_or(false);
    let is_maximized = window.is_maximized().unwrap_or(false);

    // Unminimize BEFORE reading geometry. Minimized windows on Windows often
    // report outer_position ≈ (-32000, -32000); restoring that parks the new
    // window off-screen so the taskbar icon appears dead.
    let _ = window.unminimize();
    // Briefly show so geometry reads are sane; tray-hidden windows are re-hidden
    // after rebuild (see was_hidden below).
    let _ = window.show();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let outer_position = window
        .outer_position()
        .ok()
        .filter(|pos| is_sane_position(pos));
    let outer_size = window.outer_size().ok().filter(|size| is_sane_size(size));

    // Keepalive window: ensures destroy(main) is not "close last window → exit".
    let keepalive = WebviewWindowBuilder::new(
        &app,
        KEEPALIVE_LABEL,
        WebviewUrl::App("index.html".into()),
    )
    .visible(false)
    .skip_taskbar(true)
    .title(" ")
    .inner_size(1.0, 1.0)
    .build()
    .map_err(|err| AppError::Message(format!("keepalive window failed: {err}")))?;

    window
        .destroy()
        .map_err(|err| AppError::Message(format!("destroy webview failed: {err}")))?;

    // Allow msedgewebview2 children to exit before creating a replacement.
    tokio::time::sleep(std::time::Duration::from_millis(700)).await;

    let new_window = match app.config().app.windows.first() {
        Some(config) => match WebviewWindowBuilder::from_config(&app, config) {
            Ok(builder) => builder.build().map_err(|err| {
                AppError::Message(format!("rebuild webview from config failed: {err}"))
            }),
            Err(err) => Err(AppError::Message(format!(
                "webview builder from config failed: {err}"
            ))),
        },
        None => Err(AppError::Message("missing window config".into())),
    };

    let new_window = match new_window {
        Ok(w) => w,
        Err(config_err) => WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
            .title("Coding Tools MCP")
            .inner_size(1280.0, 800.0)
            .min_inner_size(960.0, 640.0)
            .build()
            .map_err(|err| {
                AppError::Message(format!(
                    "rebuild webview failed ({config_err}); fallback also failed: {err}"
                ))
            })?,
    };

    if let Some(size) = outer_size {
        let _ = new_window.set_size(tauri::Size::Physical(size));
    }
    if let Some(pos) = outer_position {
        let _ = new_window.set_position(tauri::Position::Physical(pos));
    } else {
        let _ = new_window.center();
    }

    // Establish a normal on-screen window first so later minimize/hide is restorable.
    let _ = new_window.unminimize();
    let _ = new_window.show();
    if is_maximized && !was_minimized && !was_hidden {
        let _ = new_window.maximize();
    }

    // Remove keepalive only after main is back.
    let _ = keepalive.destroy();

    if was_hidden {
        // Stay in tray: do not steal focus after a silent recreate.
        let _ = new_window.hide();
    } else {
        let _ = new_window.set_focus();
        // If the user had it minimized (silent memory refresh), put it back in the
        // taskbar — but only after geometry is sane, so restore from taskbar works.
        if was_minimized {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let _ = new_window.minimize();
        }
    }

    Ok(())
}
