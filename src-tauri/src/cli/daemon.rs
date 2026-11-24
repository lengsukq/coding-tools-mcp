use std::sync::Arc;

use tokio::sync::Notify;

use crate::app_state::AppState;
use crate::data::DataStore;
use crate::platform::local_ipc::{
    default_daemon_socket_path, send_daemon_request_at, LocalDaemonRequest, LocalDaemonResponse,
};
use crate::runtime::service::{
    remember_runtime_state_sync, spawn_gateway_maintenance, start_global_mcp, stop_global_mcp,
};
use crate::settings::AppSettings;
use crate::workspace::auto_discover::{add_auto_root, sync_auto_discovered_workspaces};

#[derive(Default)]
struct ServeOptions {
    port: Option<u16>,
    enable_lan: bool,
    allow_lan_mcp: bool,
    issue_pair_pin: bool,
    auto_roots: Vec<String>,
}

fn parse_serve_options(args: &[String]) -> Result<ServeOptions, String> {
    let mut opts = ServeOptions::default();
    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--port" | "-p" => {
                let raw = args
                    .get(idx + 1)
                    .ok_or_else(|| "`--port` 需要提供端口号（例如 28765）".to_string())?;
                let port: u16 = raw.parse().map_err(|_| format!("无效端口号: {raw}"))?;
                if port == 0 {
                    return Err("端口号不能为 0".into());
                }
                opts.port = Some(port);
                idx += 2;
            }
            "--lan" => {
                opts.enable_lan = true;
                idx += 1;
            }
            "--allow-lan-mcp" => {
                opts.allow_lan_mcp = true;
                idx += 1;
            }
            "--pair-pin" => {
                opts.enable_lan = true;
                opts.issue_pair_pin = true;
                idx += 1;
            }
            "--auto-root" => {
                let dir = args
                    .get(idx + 1)
                    .ok_or_else(|| "`--auto-root` 需要指定父目录路径".to_string())?;
                opts.auto_roots.push(dir.clone());
                idx += 2;
            }
            "-h" | "--help" => {
                super::print_usage();
                std::process::exit(0);
            }
            other => {
                return Err(format!("`serve` 不支持参数 `{other}`"));
            }
        }
    }
    Ok(opts)
}

pub async fn cmd_serve(args: &[String]) -> Result<(), String> {
    let opts = parse_serve_options(args)?;
    for root in &opts.auto_roots {
        add_auto_root(root).map_err(|e| e.to_string())?;
    }
    let _ = sync_auto_discovered_workspaces();

    let state = Arc::new(AppState::new().map_err(|e| format!("初始化应用状态失败: {e}"))?);
    if opts.port.is_some() || opts.enable_lan || opts.allow_lan_mcp {
        state
            .with_settings(|store| {
                let mut settings = store.settings();
                if let Some(port) = opts.port {
                    settings.global_gateway.local_port = port;
                }
                if opts.enable_lan {
                    settings.lan_federation.enabled = true;
                }
                if opts.allow_lan_mcp {
                    settings.allow_lan_access = true;
                }
                store.update_settings(settings)
            })
            .map_err(|e| format!("保存启动参数失败: {e}"))?;
        crate::lan_peer::sync_discovery_daemon();
    }

    let shutdown_notify = Arc::new(Notify::new());
    #[cfg(unix)]
    let _ipc_guard = {
        let sock = default_daemon_socket_path()?;
        crate::platform::local_ipc::start_daemon_ipc_server(
            sock,
            Arc::clone(&state),
            Arc::clone(&shutdown_notify),
        )
        .await?
    };

    let status = start_global_mcp(&state)
        .await
        .map_err(|e| format!("启动 Global MCP 失败: {e}"))?;
    let _ = remember_runtime_state_sync(&state, true);
    spawn_gateway_maintenance(state.gateway.clone());

    let settings = state
        .with_settings(|store| Ok(store.settings()))
        .unwrap_or_default();
    let workspaces = state
        .gateway
        .registry
        .descriptors()
        .map(|v| v.len())
        .unwrap_or(0);
    let (lan_cfg, _) = crate::lan_peer::ensure_local_identity().unwrap_or_default();
    let ips: Vec<String> = crate::local_network::local_lan_ipv4_addrs_with_subnets(
        &settings.lan_federation.extra_subnets,
    )
    .into_iter()
    .map(|ip| ip.to_string())
    .collect();

    println!(
        "Coding Tools MCP v{} (Headless Daemon)",
        env!("CARGO_PKG_VERSION")
    );
    println!("  状态       : {}", status.state);
    println!("  本地端点   : {}", status.local_endpoint);
    if !status.public_endpoint.is_empty() {
        println!("  公网端点   : {}", status.public_endpoint);
    }
    println!(
        "  认证模式   : {}（本机免授权直连: {}）",
        settings.global_mcp_auth_type,
        if settings.allow_local_noauth {
            "开启"
        } else {
            "关闭"
        }
    );
    println!("  纳管工作区 : {workspaces} 个");
    if settings.lan_federation.enabled {
        let port = settings.global_gateway.local_port;
        let addr_list = if ips.is_empty() {
            "未探测到局域网/组网 IPv4".to_string()
        } else {
            ips.iter()
                .map(|ip| format!("{ip}:{port}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        println!(
            "  局域网联邦 : 已开启（设备: {} [{}] · 节点地址: {}）",
            lan_cfg.device_name, lan_cfg.device_id, addr_list
        );
    } else {
        println!("  局域网联邦 : 未开启（加 `--lan` 或执行 `coding-tools-mcp lan enable` 开启）");
    }

    if opts.issue_pair_pin {
        let pin = crate::lan_peer::generate_pairing_pin();
        println!("  ──────────────────────────────────────────────");
        println!(
            "  一次性配对码 : {} （{} 秒内有效，可在另一台电脑输入完成双向配对）",
            pin.pin, pin.remaining_seconds
        );
        println!("  ──────────────────────────────────────────────");
    }

    wait_for_shutdown_signal(&shutdown_notify).await;
    println!("\n正在优雅停止 Global MCP 服务…");
    let _ = stop_global_mcp(&state).await;
    Ok(())
}

async fn wait_for_shutdown_signal(shutdown_notify: &Notify) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = shutdown_notify.notified() => {}
            _ = async {
                match sigterm.as_mut() {
                    Some(s) => { let _ = s.recv().await; }
                    None => std::future::pending::<()>().await,
                }
            } => {}
        }
    }
    #[cfg(not(unix))]
    {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = shutdown_notify.notified() => {}
        }
    }
}

pub async fn try_send_daemon(req: &LocalDaemonRequest) -> Option<LocalDaemonResponse> {
    let sock = default_daemon_socket_path().ok()?;
    send_daemon_request_at(&sock, req).await.ok()
}

pub async fn cmd_status() -> Result<(), String> {
    if let Some(LocalDaemonResponse::Status { status }) =
        try_send_daemon(&LocalDaemonRequest::Status).await
    {
        println!(
            "守护进程   : 运行中 (PID {} · v{})",
            status.pid, status.version
        );
        println!("MCP 状态   : {}", status.overview.state);
        println!("本地端点   : {}", status.overview.local_endpoint);
        if !status.overview.public_endpoint.is_empty() {
            println!("公网端点   : {}", status.overview.public_endpoint);
        }
        println!(
            "工作区/会话: {} 个工作区 · {} 个活跃会话",
            status.overview.workspace_count, status.overview.session_count
        );
        println!(
            "MCP 聚合   : {} 个已启用上游（直出 {} 工具）",
            status.upstream_enabled_count, status.upstream_direct_tools_count
        );
        println!(
            "局域网联邦 : {}（{} [{}] · 已配对 {} 台 · IP: {}）",
            if status.lan_enabled {
                "开启"
            } else {
                "关闭"
            },
            status.device_name,
            status.device_id,
            status.paired_peers_count,
            if status.addresses.is_empty() {
                "-".into()
            } else {
                status.addresses.join(", ")
            }
        );
        return Ok(());
    }

    let settings = AppSettings::load_or_default();
    let profiles = DataStore::read_file(|d| Ok(d.profiles.len())).unwrap_or(0);
    println!("守护进程   : 未运行（可执行 `coding-tools-mcp serve` 启动）");
    println!(
        "配置端口   : 127.0.0.1:{}",
        settings.global_gateway.local_port
    );
    println!("认证模式   : {}", settings.global_mcp_auth_type);
    println!(
        "已存工作区 : {profiles} 个（自动扫描根目录 {} 个）",
        settings.workspace_auto_roots.len()
    );
    Ok(())
}

pub async fn cmd_stop() -> Result<(), String> {
    let sock = default_daemon_socket_path()?;
    match send_daemon_request_at(&sock, &LocalDaemonRequest::Stop).await? {
        LocalDaemonResponse::Ack { message } => {
            println!("{message}");
            Ok(())
        }
        LocalDaemonResponse::Error { message } => Err(message),
        _ => Ok(()),
    }
}

pub async fn cmd_reload() -> Result<(), String> {
    let sock = default_daemon_socket_path()?;
    match send_daemon_request_at(
        &sock,
        &LocalDaemonRequest::Reload {
            tunnel_changed: true,
        },
    )
    .await?
    {
        LocalDaemonResponse::Ack { message } => {
            println!("{message}");
            Ok(())
        }
        LocalDaemonResponse::Error { message } => Err(message),
        _ => Ok(()),
    }
}
