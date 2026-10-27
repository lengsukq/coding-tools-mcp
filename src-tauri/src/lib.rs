#![cfg_attr(target_os = "windows", allow(linker_messages))]

mod agent_context;
mod app_state;
mod auth;
mod commands;
mod computer;
mod computer_geometry;
mod data;
mod error;
mod global_gateway;
pub mod harness;
mod health;
mod local_network;
mod mcp;
pub mod planning;
mod platform;
mod review;
mod runtime;
mod secret;
mod settings;
mod tool_audit;
pub mod tools;
mod tunnel;
mod update;
pub(crate) mod usage;
mod workspace;

use app_state::AppState;
use commands::{
    accept_goal_review, accept_plan_review, approve_computer_screen_request, check_app_update,
    check_global_gateway_health, clear_tool_audit_records, create_session_review_url,
    create_workspace, create_workspace_review_url, decide_computer_sensitive_action,
    delete_frp_profile, delete_plan, delete_workspace, delete_workspace_review,
    detect_installed_ides, enable_computer_access, enter_compact_mode, get_app_settings,
    get_computer_screen_status, get_download_config, get_global_gateway_config,
    get_global_gateway_status, get_global_mcp_overview, get_global_runtime_settings,
    get_last_workspace_id, get_planning_state, get_proxy, get_runtime_status,
    get_service_usage_stats, get_shared_secret, get_webview_memory_sample,
    get_workspace_activity_metrics, get_workspace_git_summary, hide_to_tray, install_software,
    issue_workspace_review_url, list_computer_available_apps, list_frp_profiles,
    list_history_sessions, list_software, list_tool_audit_records, list_workspace_reviews,
    list_workspaces, open_url, open_workspace_directory, open_workspace_in_ide,
    pause_computer_access, quit_app, read_workspace_logs, recreate_ui_webview,
    regenerate_shared_secret, reject_goal_review, reject_plan_review,
    request_computer_accessibility_permission, request_computer_screen_permission,
    reset_planning_state, restart_global_cloudflare_tunnel, restart_runtime, restore_runtime_state,
    resume_computer_access, revoke_computer_screen_access, run_global_health_checks,
    save_frp_profile, scan_agent_context, scan_global_agent_context, set_download_config,
    set_global_gateway_config, set_global_runtime_settings, set_last_workspace, set_planning_mode,
    set_proxy, set_shared_secret, set_shared_secrets, set_tool_audit_retention_days,
    show_main_window, start_global_gateway, start_runtime, stop_global_gateway, stop_runtime,
    uninstall_software, update_workspace,
};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

struct TrayDashboard {
    status: MenuItem<tauri::Wry>,
    ui_status: MenuItem<tauri::Wry>,
    memory: MenuItem<tauri::Wry>,
    processes: MenuItem<tauri::Wry>,
    compact_action: MenuItem<tauri::Wry>,
    sessions: MenuItem<tauri::Wry>,
}

/// Update native menu items without ever instantiating another WebView.
fn update_tray_dashboard(app: &AppHandle) {
    let Some(menu) = app.try_state::<TrayDashboard>() else {
        return;
    };
    let compact = commands::window_chrome::is_compact_mode();
    // Check Tauri's window registry, not a cached compact-mode flag.
    let main_webview_exists = app.get_webview_window("main").is_some();
    let ui_status = if compact && !main_webview_exists {
        "主界面：已卸载（无活动主 WebView）"
    } else if compact {
        "主界面：正在卸载…"
    } else if main_webview_exists {
        "主界面：已加载"
    } else {
        "主界面：未找到窗口"
    };
    let _ = menu.ui_status.set_text(ui_status);
    let runtime = app.state::<AppState>().with_runtime(|runtime| {
        runtime.refresh_mcp();
        Ok(runtime.mcp_status().state)
    });
    let status = match runtime.as_deref() {
        Ok("running") => "运行中",
        Ok("starting") => "启动中",
        Ok("stopped") => "已停止",
        Ok("error") => "异常",
        _ => "未知",
    };
    let _ = menu.status.set_text(format!("MCP 服务：{status}"));
    let workspaces = app
        .state::<AppState>()
        .gateway
        .registry
        .descriptors()
        .map(|items| items.len())
        .unwrap_or_default();
    let session_store = &app.state::<AppState>().gateway.sessions;
    let active_connections = session_store.active_session_count();
    let reclaimed = session_store.maintenance_stats();
    let _ = menu.sessions.set_text(format!(
        "工作区：{workspaces} · 会话：{active_connections} · 定时回收：{}",
        reclaimed.reclaimed_sessions
    ));

    if let Ok(sample) = commands::ui_memory::get_webview_memory_sample() {
        if sample.supported {
            let total = sample.main_mb + sample.webview_mb;
            let _ = menu.memory.set_text(format!(
                "内存估算：{total:.0} MB（主进程 {:.0} MB）",
                sample.main_mb
            ));
            let label = if compact && !main_webview_exists {
                "WebKit 辅助进程（可能滞留）"
            } else {
                "WebKit 辅助进程"
            };
            let _ = menu.processes.set_text(format!(
                "{label}：{} 个 / {:.0} MB",
                sample.webview_process_count, sample.webview_mb
            ));
        } else {
            let _ = menu.memory.set_text("当前平台暂不支持内存采样");
        }
    } else {
        let _ = menu.memory.set_text("内存采样暂不可用，可点击刷新重试");
    }
    let _ = menu.compact_action.set_text(if compact {
        "已启用轻量驻留"
    } else if cfg!(target_os = "macos") {
        "轻量驻留（释放界面内存）"
    } else {
        "轻量驻留（仅 macOS）"
    });
    let _ = menu
        .compact_action
        .set_enabled(cfg!(target_os = "macos") && !compact);
}

#[cfg(target_os = "windows")]
fn signal_existing_instance() -> bool {
    use windows::core::w;
    use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
    use windows::Win32::System::Threading::{
        CreateEventW, CreateMutexW, OpenEventW, SetEvent, EVENT_MODIFY_STATE,
    };

    let Ok(mutex) = (unsafe {
        CreateMutexW(
            None,
            false,
            w!("Local\\CodingToolsMcpDesktop-SingleInstance"),
        )
    }) else {
        eprintln!("创建应用单实例锁失败，为避免误清理其他实例的 frpc，本次启动已取消");
        return false;
    };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        let _ = unsafe { CloseHandle(mutex) };
        if let Ok(event) = unsafe {
            OpenEventW(
                EVENT_MODIFY_STATE,
                false,
                w!("Local\\CodingToolsMcpDesktop-ShowWindow"),
            )
        } {
            let _ = unsafe { SetEvent(event) };
            let _ = unsafe { CloseHandle(event) };
        }
        return false;
    }

    // Keep mutex handle for process lifetime (do not CloseHandle).
    let _ = INSTANCE_MUTEX.set(mutex.0 as usize);

    let Ok(event) = (unsafe {
        CreateEventW(
            None,
            false,
            false,
            w!("Local\\CodingToolsMcpDesktop-ShowWindow"),
        )
    }) else {
        return true;
    };
    // Pass the handle as usize so the waiter thread is Send (HANDLE is !Send).
    let event_bits = event.0 as usize;
    std::thread::spawn(move || {
        use windows::Win32::System::Threading::{WaitForSingleObject, INFINITE};
        let event = HANDLE(event_bits as *mut std::ffi::c_void);
        loop {
            let _ = unsafe { WaitForSingleObject(event, INFINITE) };
            if let Some(app) = SHOW_APP_HANDLE.get() {
                let _ = commands::window_chrome::show_main_window(app.clone());
            }
        }
    });
    true
}

#[cfg(target_os = "windows")]
static SHOW_APP_HANDLE: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

#[cfg(target_os = "windows")]
static INSTANCE_MUTEX: std::sync::OnceLock<usize> = std::sync::OnceLock::new();

#[cfg(target_os = "windows")]
fn acquire_single_instance() -> bool {
    signal_existing_instance()
}

#[cfg(not(target_os = "windows"))]
fn acquire_single_instance() -> bool {
    true
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let status_i = MenuItem::with_id(app, "status", "MCP 服务：检查中", false, None::<&str>)?;
    let ui_status_i = MenuItem::with_id(app, "ui-status", "主界面：检查中", false, None::<&str>)?;
    let memory_i = MenuItem::with_id(app, "memory", "当前内存：采样中", false, None::<&str>)?;
    let processes_i = MenuItem::with_id(app, "processes", "界面：采样中", false, None::<&str>)?;
    let sessions_i = MenuItem::with_id(app, "sessions", "工作区：检查中", false, None::<&str>)?;
    let version_i = MenuItem::with_id(
        app,
        "version",
        format!("Coding Tools MCP v{}", app.package_info().version),
        false,
        None::<&str>,
    )?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let show_i = MenuItem::with_id(app, "show", "恢复主窗口", true, None::<&str>)?;
    let compact_i = MenuItem::with_id(
        app,
        "compact",
        "轻量驻留（释放界面内存）",
        true,
        None::<&str>,
    )?;
    let refresh_i = MenuItem::with_id(app, "refresh", "刷新内存与服务状态", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "完全退出", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &version_i,
            &status_i,
            &ui_status_i,
            &sessions_i,
            &memory_i,
            &processes_i,
            &sep1,
            &show_i,
            &compact_i,
            &refresh_i,
            &sep2,
            &quit_i,
        ],
    )?;
    app.manage(TrayDashboard {
        status: status_i,
        ui_status: ui_status_i,
        memory: memory_i,
        processes: processes_i,
        sessions: sessions_i,
        compact_action: compact_i,
    });

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .tooltip("Coding Tools MCP")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                let _ = commands::window_chrome::show_main_window(app.clone());
                update_tray_dashboard(app);
            }
            "compact" => {
                // The Vue unsaved-changes guard must run before unloading the UI.
                if commands::window_chrome::show_main_window(app.clone()).is_ok() {
                    let _ = app.emit("compact-requested", ());
                }
                update_tray_dashboard(app);
            }
            "refresh" => {
                update_tray_dashboard(app);
            }
            "quit" => {
                commands::window_chrome::arm_allow_exit();
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                #[cfg(target_os = "macos")]
                update_tray_dashboard(tray.app_handle());
                #[cfg(not(target_os = "macos"))]
                let _ = commands::window_chrome::show_main_window(tray.app_handle().clone());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    update_tray_dashboard(app.handle());
    // The menu displays recent values even before the first click. No polling
    // or Vue timers survive in the compact state; sampling stays in Rust.
    #[cfg(target_os = "macos")]
    {
        let app_handle = app.handle().clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(15));
            let app_for_ui = app_handle.clone();
            if app_handle
                .run_on_main_thread(move || update_tray_dashboard(&app_for_ui))
                .is_err()
            {
                break;
            }
        });
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if !acquire_single_instance() {
        return;
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(AppState::new().expect("failed to load app state"));
            setup_tray(app)?;
            // Maintenance is owned by the desktop process rather than the
            // WebView or MCP listener, so compact mode / listener restarts do
            // not interrupt cleanup. It never kills a running command.
            let maintenance_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(5 * 60));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                interval.tick().await;
                loop {
                    interval.tick().await;
                    let (sessions, capabilities) = maintenance_app
                        .state::<AppState>()
                        .gateway.sessions.maintenance_sweep();
                    let completed_commands =
                        crate::tools::session::reap_idle_command_sessions().await;
                    if sessions > 0 || capabilities > 0 || completed_commands > 0 {
                        eprintln!(
                            "[memory-maintenance] expired_sessions={sessions} expired_capabilities={capabilities} completed_commands={completed_commands}"
                        );
                    }
                }
            });
            #[cfg(target_os = "windows")]
            {
                let _ = SHOW_APP_HANDLE.set(app.handle().clone());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_computer_screen_status,
            enable_computer_access,
            approve_computer_screen_request,
            decide_computer_sensitive_action,
            list_computer_available_apps,
            revoke_computer_screen_access,
            pause_computer_access,
            resume_computer_access,
            request_computer_screen_permission,
            request_computer_accessibility_permission,
            list_workspaces,
            get_workspace_activity_metrics,
            get_workspace_git_summary,
            list_workspace_reviews,
            delete_workspace_review,
            issue_workspace_review_url,
            create_workspace_review_url,
            create_session_review_url,
            detect_installed_ides,
            open_workspace_in_ide,
            get_planning_state,
            reset_planning_state,
            delete_plan,
            set_planning_mode,
            accept_goal_review,
            reject_goal_review,
            accept_plan_review,
            reject_plan_review,
            scan_agent_context,
            scan_global_agent_context,
            list_history_sessions,
            get_service_usage_stats,
            create_workspace,
            update_workspace,
            open_workspace_directory,
            open_url,
            check_app_update,
            delete_workspace,
            start_runtime,
            stop_runtime,
            get_runtime_status,
            get_global_mcp_overview,
            restart_runtime,
            restore_runtime_state,
            run_global_health_checks,
            get_shared_secret,
            set_shared_secret,
            set_shared_secrets,
            regenerate_shared_secret,
            read_workspace_logs,
            list_tool_audit_records,
            set_tool_audit_retention_days,
            clear_tool_audit_records,
            list_frp_profiles,
            save_frp_profile,
            delete_frp_profile,
            get_app_settings,
            set_last_workspace,
            get_last_workspace_id,
            list_software,
            install_software,
            uninstall_software,
            get_download_config,
            set_download_config,
            get_proxy,
            set_proxy,
            get_global_gateway_config,
            set_global_gateway_config,
            start_global_gateway,
            restart_global_cloudflare_tunnel,
            stop_global_gateway,
            get_global_gateway_status,
            check_global_gateway_health,
            get_global_runtime_settings,
            set_global_runtime_settings,
            get_webview_memory_sample,
            recreate_ui_webview,
            enter_compact_mode,
            hide_to_tray,
            show_main_window,
            quit_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| match event {
            tauri::RunEvent::ExitRequested { api, .. } => {
                // While recreating the UI WebView we temporarily destroy the main
                // window; without prevent_exit Tauri would quit the whole process
                // and take MCP/FRP down with it (0.1.30 regression).
                if commands::ui_memory::should_prevent_exit() {
                    api.prevent_exit();
                }
            }
            tauri::RunEvent::WindowEvent { label, event, .. } => {
                if label != "main" {
                    return;
                }
                if let WindowEvent::CloseRequested { api, .. } = event {
                    if commands::window_chrome::should_intercept_close() {
                        api.prevent_close();
                        let _ = app_handle.emit("close-requested", ());
                    }
                }
            }
            _ => {}
        });
}
