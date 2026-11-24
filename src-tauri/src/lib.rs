#![cfg_attr(target_os = "windows", allow(linker_messages))]
#![cfg_attr(not(feature = "desktop-ui"), allow(dead_code, unused_imports))]

mod agent_context;
mod app_state;
pub mod async_rt;
mod auth;
pub mod cli;
#[cfg(feature = "desktop-ui")]
mod commands;
mod computer;
mod data;
mod error;
mod global_gateway;
pub mod harness;
mod health;
pub(crate) mod lan_peer;
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
pub mod util;
mod workspace;

/// Headless / CLI entry point shared by `coding-tools-mcp` binary and subcommands.
pub fn run_cli_entry(args: &[String]) -> i32 {
    cli::run_cli(args)
}

#[cfg(feature = "desktop-ui")]
mod desktop_shell {
    use super::*;
    use app_state::AppState;
    use commands::{
        accept_goal_review, accept_plan_review, approve_computer_screen_request,
        cancel_lan_pairing_pin, check_app_update, check_global_gateway_health,
        clear_tool_audit_records, create_lan_pairing_pin, create_session_review_url,
        create_workspace, create_workspace_review_url, decide_computer_sensitive_action,
        delete_frp_profile, delete_plan, delete_workspace, delete_workspace_review,
        detect_installed_ides, enable_computer_access, enter_compact_mode,
        get_computer_screen_status, get_download_config, get_gemini_local_bridge_status,
        get_global_gateway_config, get_global_gateway_status, get_global_mcp_overview,
        get_global_runtime_settings, get_lan_peers_overview, get_last_workspace_id,
        get_planning_state, get_proxy, get_runtime_status, get_service_usage_stats,
        get_shared_secret, get_webview_memory_sample, get_workspace_activity_metrics,
        get_workspace_git_summary, hide_to_tray, install_software, issue_workspace_review_url,
        list_computer_available_apps, list_frp_profiles, list_history_sessions, list_software,
        list_tool_audit_records, list_workspace_reviews, list_workspaces, open_url,
        open_workspace_directory, open_workspace_in_ide, pair_lan_peer, pause_computer_access,
        prepare_gemini_local_bridge, probe_lan_peer_address, quit_app, read_workspace_logs,
        recreate_ui_webview, regenerate_shared_secret, reject_goal_review, reject_plan_review,
        remove_lan_paired_peer, rename_lan_paired_peer, request_computer_accessibility_permission,
        request_computer_screen_permission, reset_planning_state, restart_global_cloudflare_tunnel,
        restart_runtime, restore_runtime_state, resume_computer_access,
        revoke_computer_screen_access, run_global_health_checks, save_frp_profile,
        save_lan_federation_settings, scan_agent_context, scan_global_agent_context,
        scan_lan_peers, set_download_config, set_global_gateway_config,
        set_global_runtime_settings, set_lan_paired_peer_enabled, set_last_workspace,
        set_planning_mode, set_proxy, set_shared_secret, set_shared_secrets,
        set_tool_audit_retention_days, start_global_gateway, start_runtime, stop_global_gateway,
        stop_runtime, uninstall_software, update_workspace,
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

    struct TraySnapshot {
        compact: bool,
        status_text: String,
        sessions_text: String,
        memory_sample: Result<commands::ui_memory::WebviewMemorySample, ()>,
    }

    fn collect_tray_snapshot(app: &AppHandle) -> TraySnapshot {
        let compact = commands::window_chrome::is_compact_mode();
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
        let memory_sample = commands::ui_memory::sample_webview_memory().map_err(|_| ());

        TraySnapshot {
            compact,
            status_text: format!("MCP 服务：{status}"),
            sessions_text: format!(
                "工作区：{workspaces} · 会话：{active_connections} · 定时回收：{}",
                reclaimed.reclaimed_sessions
            ),
            memory_sample,
        }
    }

    fn apply_tray_snapshot(app: &AppHandle, snapshot: TraySnapshot) {
        let Some(menu) = app.try_state::<TrayDashboard>() else {
            return;
        };
        let main_webview_exists = app.get_webview_window("main").is_some();
        let ui_status = if snapshot.compact && !main_webview_exists {
            "主界面：已卸载（无活动主 WebView）"
        } else if snapshot.compact {
            "主界面：正在卸载…"
        } else if main_webview_exists {
            "主界面：已加载"
        } else {
            "主界面：未找到窗口"
        };
        let _ = menu.ui_status.set_text(ui_status);
        let _ = menu.status.set_text(&snapshot.status_text);
        let _ = menu.sessions.set_text(&snapshot.sessions_text);

        match snapshot.memory_sample {
            Ok(sample) if sample.supported => {
                let total = sample.main_mb + sample.webview_mb;
                let _ = menu.memory.set_text(format!(
                    "内存估算：{total:.0} MB（主进程 {:.0} MB）",
                    sample.main_mb
                ));
                let label = if snapshot.compact && !main_webview_exists {
                    "WebKit 辅助进程（可能滞留）"
                } else {
                    "WebKit 辅助进程"
                };
                let _ = menu.processes.set_text(format!(
                    "{label}：{} 个 / {:.0} MB",
                    sample.webview_process_count, sample.webview_mb
                ));
            }
            Ok(_) => {
                let _ = menu.memory.set_text("当前平台暂不支持内存采样");
            }
            Err(()) => {
                let _ = menu.memory.set_text("内存采样暂不可用，可点击刷新重试");
            }
        }
        let _ = menu.compact_action.set_text(if snapshot.compact {
            "已启用轻量驻留"
        } else if cfg!(target_os = "macos") {
            "轻量驻留（释放界面内存）"
        } else {
            "轻量驻留（仅 macOS）"
        });
        let _ = menu
            .compact_action
            .set_enabled(cfg!(target_os = "macos") && !snapshot.compact);
    }

    fn update_tray_dashboard(app: &AppHandle) {
        let app_for_sample = app.clone();
        tauri::async_runtime::spawn(async move {
            let sample_handle = app_for_sample.clone();
            let Ok(snapshot) =
                tauri::async_runtime::spawn_blocking(move || collect_tray_snapshot(&sample_handle))
                    .await
            else {
                return;
            };
            let app_for_ui = app_for_sample.clone();
            let _ = app_for_sample.run_on_main_thread(move || {
                apply_tray_snapshot(&app_for_ui, snapshot);
            });
        });
    }

    fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
        let status_i = MenuItem::with_id(app, "status", "MCP 服务：检查中", false, None::<&str>)?;
        let ui_status_i =
            MenuItem::with_id(app, "ui-status", "主界面：检查中", false, None::<&str>)?;
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
        let refresh_i =
            MenuItem::with_id(app, "refresh", "刷新内存与服务状态", true, None::<&str>)?;
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
        #[cfg(target_os = "macos")]
        {
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(120));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                interval.tick().await;
                loop {
                    interval.tick().await;
                    let sample_handle = app_handle.clone();
                    let Ok(snapshot) = tauri::async_runtime::spawn_blocking(move || {
                        collect_tray_snapshot(&sample_handle)
                    })
                    .await
                    else {
                        break;
                    };
                    let app_for_ui = app_handle.clone();
                    if app_handle
                        .run_on_main_thread(move || apply_tray_snapshot(&app_for_ui, snapshot))
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
        Ok(())
    }

    #[cfg_attr(mobile, tauri::mobile_entry_point)]
    pub fn run() {
        let args: Vec<String> = std::env::args().collect();
        if args.get(1).is_some_and(|arg| cli::is_cli_subcommand(arg)) {
            let code = cli::run_cli(&args);
            std::process::exit(code);
        }
        if !platform::acquire_single_instance() {
            return;
        }
        let worker_threads = std::thread::available_parallelism()
            .map(|n| n.get().clamp(2, 4))
            .unwrap_or(2);
        let _tokio_runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(worker_threads)
            .max_blocking_threads(16)
            .enable_all()
            .build()
            .ok();
        if let Some(rt) = _tokio_runtime.as_ref() {
            async_rt::set(rt.handle().clone());
        }
        let startup_args = args.clone();
        tauri::Builder::default()
            .plugin(tauri_plugin_dialog::init())
            .setup(move |app| {
                let app_state = AppState::new().expect("failed to load app state");
                let gateway = app_state.gateway.clone();
                let launch_in_lightweight_mode = app_state
                    .with_settings(|store| Ok(store.settings().launch_in_lightweight_mode))
                    .unwrap_or(false);
                let start_lightweight = platform::autostart::should_start_lightweight(
                    &startup_args,
                    launch_in_lightweight_mode,
                );
                app.manage(app_state);
                if start_lightweight {
                    commands::window_chrome::enter_lightweight_startup(app.handle())
                        .map_err(|err| std::io::Error::other(err.to_string()))?;
                } else {
                    commands::window_chrome::create_main_window_from_config(app.handle(), true)
                        .map_err(|err| std::io::Error::other(err.to_string()))?;
                }
                setup_tray(app)?;
                runtime::service::spawn_gateway_maintenance(gateway);
                platform::register_show_window_handle(app.handle());
                let restore_app = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    crate::global_gateway::cleanup_orphaned_tunnels_on_startup().await;
                    let state = restore_app.state::<AppState>();
                    let _ = commands::runtime::restore_runtime_state_inner(&state).await;
                    update_tray_dashboard(&restore_app);
                });
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
                get_gemini_local_bridge_status,
                prepare_gemini_local_bridge,
                start_global_gateway,
                restart_global_cloudflare_tunnel,
                stop_global_gateway,
                get_global_gateway_status,
                check_global_gateway_health,
                get_global_runtime_settings,
                set_global_runtime_settings,
                get_lan_peers_overview,
                save_lan_federation_settings,
                create_lan_pairing_pin,
                cancel_lan_pairing_pin,
                scan_lan_peers,
                probe_lan_peer_address,
                pair_lan_peer,
                remove_lan_paired_peer,
                set_lan_paired_peer_enabled,
                rename_lan_paired_peer,
                get_webview_memory_sample,
                recreate_ui_webview,
                enter_compact_mode,
                hide_to_tray,
                quit_app,
            ])
            .build(tauri::generate_context!())
            .expect("error while building tauri application")
            .run(|app_handle, event| match event {
                tauri::RunEvent::ExitRequested { api, .. } => {
                    if commands::ui_memory::should_prevent_exit() {
                        api.prevent_exit();
                    }
                }
                tauri::RunEvent::Exit => {
                    crate::global_gateway::terminate_children_sync();
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
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen {
                    has_visible_windows: false,
                    ..
                } => {
                    let _ = commands::window_chrome::show_main_window(app_handle.clone());
                    update_tray_dashboard(app_handle);
                }
                _ => {}
            });
    }
}

#[cfg(feature = "desktop-ui")]
pub use desktop_shell::run;
