//! Local `0700`/`0600` Unix Domain Socket IPC for:
//! 1. Linux desktop single-instance coordination & window wakeup (`desktop.sock`).
//! 2. Headless `coding-tools-mcp serve` live control (`daemon.sock`: status,
//!    in-memory 6-digit LAN pairing PIN generation, live policy reload, stop).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

use crate::app_state::AppState;
use crate::lan_peer::PairingPinDto;
use crate::runtime::service::GlobalMcpOverviewDto;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum LocalDaemonRequest {
    Ping,
    Status,
    GeneratePin,
    CancelPin,
    Reload {
        #[serde(default)]
        tunnel_changed: bool,
    },
    ReloadUpstream,
    Stop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonStatusPayload {
    pub pid: u32,
    pub version: String,
    pub overview: GlobalMcpOverviewDto,
    pub lan_enabled: bool,
    pub device_id: String,
    pub device_name: String,
    pub addresses: Vec<String>,
    pub paired_peers_count: usize,
    #[serde(default)]
    pub upstream_enabled_count: usize,
    #[serde(default)]
    pub upstream_direct_tools_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonPinPayload {
    pub pin: PairingPinDto,
    pub mcp_port: u16,
    pub device_id: String,
    pub device_name: String,
    pub addresses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LocalDaemonResponse {
    Pong {
        pid: u32,
        version: String,
        mcp_port: u16,
    },
    Status {
        status: Box<DaemonStatusPayload>,
    },
    PinIssued {
        payload: DaemonPinPayload,
    },
    Ack {
        message: String,
    },
    Error {
        message: String,
    },
}

#[cfg(unix)]
fn is_owned_real_dir(dir: &Path, expected_uid: u32) -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(dir)
        .map(|m| m.is_dir() && !m.file_type().is_symlink() && m.uid() == expected_uid)
        .unwrap_or(false)
}

#[cfg(unix)]
fn candidate_ipc_dirs_for_uid(
    euid: u32,
    xdg_runtime_dir: Option<PathBuf>,
    temp_dir: PathBuf,
) -> Vec<PathBuf> {
    let mut dirs = Vec::with_capacity(3);
    #[cfg(target_os = "linux")]
    if euid == 0 {
        let run_root = PathBuf::from("/run");
        if is_owned_real_dir(&run_root, 0) {
            dirs.push(run_root.join("coding-tools-mcp"));
        }
    }
    if let Some(xdg) = xdg_runtime_dir.filter(|dir| is_owned_real_dir(dir, euid)) {
        let candidate = xdg.join("coding-tools-mcp");
        if !dirs.contains(&candidate) {
            dirs.push(candidate);
        }
    }
    let fallback = temp_dir.join(format!("coding-tools-mcp-{euid}"));
    if !dirs.contains(&fallback) {
        dirs.push(fallback);
    }
    dirs
}

#[cfg(unix)]
fn is_live_owned_daemon_socket(sock_path: &Path, expected_uid: u32) -> bool {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::os::unix::net::UnixStream;

    let Some(parent) = sock_path.parent() else {
        return false;
    };
    let Ok(parent_meta) = std::fs::symlink_metadata(parent) else {
        return false;
    };
    if parent_meta.file_type().is_symlink()
        || !parent_meta.is_dir()
        || parent_meta.uid() != expected_uid
        || (parent_meta.permissions().mode() & 0o077 != 0)
    {
        return false;
    }
    let Ok(sock_meta) = std::fs::symlink_metadata(sock_path) else {
        return false;
    };
    if sock_meta.file_type().is_symlink() || sock_meta.uid() != expected_uid {
        return false;
    }
    UnixStream::connect(sock_path).is_ok()
}

#[cfg(unix)]
pub fn private_ipc_dir() -> Result<PathBuf, String> {
    let euid = unsafe { libc::geteuid() };
    let candidates = candidate_ipc_dirs_for_uid(
        euid,
        std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
        std::env::temp_dir(),
    );
    let candidate = candidates
        .into_iter()
        .next()
        .unwrap_or_else(|| std::env::temp_dir().join(format!("coding-tools-mcp-{euid}")));

    ensure_private_dir_at(&candidate, euid)?;
    Ok(candidate)
}

#[cfg(unix)]
fn ensure_private_dir_at(dir: &Path, expected_uid: u32) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};

    if !dir.exists() {
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder
            .create(dir)
            .map_err(|err| format!("创建本地 IPC 目录失败 ({}): {err}", dir.display()))?;
    }
    let meta = std::fs::symlink_metadata(dir)
        .map_err(|err| format!("检查本地 IPC 目录失败 ({}): {err}", dir.display()))?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(format!("本地 IPC 路径不是安全目录: {}", dir.display()));
    }
    if meta.uid() != expected_uid {
        return Err(format!(
            "本地 IPC 目录所有者异常 ({}): 期望 uid={expected_uid}，实际 uid={}",
            dir.display(),
            meta.uid()
        ));
    }
    if meta.permissions().mode() & 0o077 != 0 {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|err| format!("收紧本地 IPC 目录权限失败 ({}): {err}", dir.display()))?;
    }
    Ok(())
}

#[cfg(unix)]
pub fn default_daemon_socket_path() -> Result<PathBuf, String> {
    let euid = unsafe { libc::geteuid() };
    let candidates = candidate_ipc_dirs_for_uid(
        euid,
        std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
        std::env::temp_dir(),
    );
    for dir in &candidates {
        let sock = dir.join("daemon.sock");
        if is_live_owned_daemon_socket(&sock, euid) {
            return Ok(sock);
        }
    }
    Ok(private_ipc_dir()?.join("daemon.sock"))
}

#[cfg(not(unix))]
pub fn default_daemon_socket_path() -> Result<PathBuf, String> {
    Ok(std::env::temp_dir().join("coding-tools-mcp-daemon.sock"))
}

#[cfg(target_os = "linux")]
pub fn default_desktop_socket_path() -> Result<PathBuf, String> {
    Ok(private_ipc_dir()?.join("desktop.sock"))
}

/// Acquire a single-instance Unix socket lock for the desktop shell (`desktop.sock`).
/// If another desktop instance is already listening, sends a wakeup signal so its
/// main window comes to the front and returns `false`.
#[cfg(all(unix, any(target_os = "linux", test)))]
pub fn acquire_unix_desktop_instance_at(
    sock_path: &Path,
    on_show_window: Arc<dyn Fn() + Send + Sync + 'static>,
) -> Result<bool, String> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};

    if let Some(parent) = sock_path.parent() {
        ensure_private_dir_at(parent, unsafe { libc::geteuid() })?;
    }

    if let Ok(mut stream) = UnixStream::connect(sock_path) {
        let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
        let _ = stream.set_write_timeout(Some(Duration::from_millis(800)));
        if stream.write_all(b"show_window\n").is_ok() && stream.flush().is_ok() {
            let mut line = String::new();
            let mut reader = BufReader::new(stream);
            if reader.read_line(&mut line).is_ok() && line.trim() == "ok" {
                return Ok(false);
            }
        }
    }

    if sock_path.exists()
        || std::fs::symlink_metadata(sock_path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
    {
        let _ = std::fs::remove_file(sock_path);
    }

    let listener = UnixListener::bind(sock_path).map_err(|err| {
        format!(
            "绑定桌面单实例 Socket 失败 ({}): {err}",
            sock_path.display()
        )
    })?;
    let _ = std::fs::set_permissions(sock_path, std::fs::Permissions::from_mode(0o600));

    std::thread::spawn(move || {
        for incoming in listener.incoming().flatten() {
            let _ = incoming.set_read_timeout(Some(Duration::from_millis(500)));
            let _ = incoming.set_write_timeout(Some(Duration::from_millis(500)));
            let mut reader = BufReader::new(&incoming);
            let mut line = String::new();
            if reader.read_line(&mut line).is_ok() && line.trim() == "show_window" {
                on_show_window();
                let mut writer = &incoming;
                let _ = writer.write_all(b"ok\n");
                let _ = writer.flush();
            }
        }
    });

    Ok(true)
}

#[cfg(unix)]
pub async fn send_daemon_request_at(
    sock_path: &Path,
    request: &LocalDaemonRequest,
) -> Result<LocalDaemonResponse, String> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixStream;

    let stream = tokio::time::timeout(Duration::from_secs(2), UnixStream::connect(sock_path))
        .await
        .map_err(|_| "连接本地守护进程超时".to_string())?
        .map_err(|err| format!("未检测到正在运行的后台守护进程 ({err})"))?;

    let (read_half, mut write_half) = stream.into_split();
    let mut payload =
        serde_json::to_vec(request).map_err(|err| format!("序列化本地控制指令失败: {err}"))?;
    payload.push(b'\n');
    write_half
        .write_all(&payload)
        .await
        .map_err(|err| format!("发送本地控制指令失败: {err}"))?;
    write_half
        .flush()
        .await
        .map_err(|err| format!("刷新本地控制通道失败: {err}"))?;

    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut line))
        .await
        .map_err(|_| "等待守护进程响应超时".to_string())?
        .map_err(|err| format!("读取守护进程响应失败: {err}"))?;

    serde_json::from_str(line.trim()).map_err(|err| format!("解析守护进程响应失败: {err}"))
}

#[cfg(not(unix))]
pub async fn send_daemon_request_at(
    _sock_path: &Path,
    _request: &LocalDaemonRequest,
) -> Result<LocalDaemonResponse, String> {
    Err("当前系统暂不支持 Unix Socket 本地守护控制通道".into())
}

#[cfg(unix)]
pub struct DaemonIpcGuard {
    sock_path: PathBuf,
    task: tokio::task::JoinHandle<()>,
}

#[cfg(unix)]
impl Drop for DaemonIpcGuard {
    fn drop(&mut self) {
        self.task.abort();
        let _ = std::fs::remove_file(&self.sock_path);
    }
}

#[cfg(unix)]
pub async fn start_daemon_ipc_server(
    sock_path: PathBuf,
    state: Arc<AppState>,
    shutdown_notify: Arc<Notify>,
) -> Result<DaemonIpcGuard, String> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixListener;

    if let Some(parent) = sock_path.parent() {
        ensure_private_dir_at(parent, unsafe { libc::geteuid() })?;
    }

    if let Ok(LocalDaemonResponse::Pong { pid, mcp_port, .. }) =
        send_daemon_request_at(&sock_path, &LocalDaemonRequest::Ping).await
    {
        return Err(format!(
            "后台守护进程已在运行中（PID {pid}，端口 {mcp_port}）；可使用 `coding-tools-mcp status` 查看状态"
        ));
    }

    if sock_path.exists() {
        let _ = std::fs::remove_file(&sock_path);
    }

    let listener = UnixListener::bind(&sock_path)
        .map_err(|err| format!("绑定本地控制 Socket 失败 ({}): {err}", sock_path.display()))?;
    let _ = std::fs::set_permissions(&sock_path, std::fs::Permissions::from_mode(0o600));

    let task = tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let state = Arc::clone(&state);
            let shutdown_notify = Arc::clone(&shutdown_notify);
            tokio::spawn(async move {
                use tokio::io::AsyncReadExt as _;
                const MAX_IPC_REQUEST_BYTES: u64 = 1024 * 1024;
                const IPC_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

                let (read_half, mut write_half) = stream.into_split();
                let mut reader = BufReader::new(read_half.take(MAX_IPC_REQUEST_BYTES));
                let mut line = String::new();
                let read_result =
                    tokio::time::timeout(IPC_READ_TIMEOUT, reader.read_line(&mut line)).await;
                let Ok(Ok(bytes_read)) = read_result else {
                    return;
                };
                if bytes_read == 0 {
                    return;
                }
                let response = match serde_json::from_str::<LocalDaemonRequest>(line.trim()) {
                    Ok(req) => handle_daemon_request(req, &state, &shutdown_notify).await,
                    Err(err) => LocalDaemonResponse::Error {
                        message: format!("无效的控制请求: {err}"),
                    },
                };
                if let Ok(mut out) = serde_json::to_vec(&response) {
                    out.push(b'\n');
                    let _ = write_half.write_all(&out).await;
                    let _ = write_half.flush().await;
                }
            });
        }
    });

    Ok(DaemonIpcGuard { sock_path, task })
}

#[cfg(unix)]
async fn handle_daemon_request(
    request: LocalDaemonRequest,
    state: &Arc<AppState>,
    shutdown_notify: &Arc<Notify>,
) -> LocalDaemonResponse {
    match request {
        LocalDaemonRequest::Ping => {
            let port = state
                .with_settings(|store| Ok(store.settings().global_gateway.local_port))
                .unwrap_or(28765);
            LocalDaemonResponse::Pong {
                pid: std::process::id(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                mcp_port: port,
            }
        }
        LocalDaemonRequest::Status => {
            let overview = match crate::runtime::service::collect_global_mcp_overview(state) {
                Ok(ov) => ov,
                Err(err) => {
                    return LocalDaemonResponse::Error {
                        message: err.to_string(),
                    }
                }
            };
            let settings = state
                .with_settings(|store| Ok(store.settings()))
                .unwrap_or_default();
            let (lan_cfg, _) = crate::lan_peer::ensure_local_identity().unwrap_or_default();
            let addresses = crate::local_network::local_lan_ipv4_addrs_with_subnets(
                &settings.lan_federation.extra_subnets,
            )
            .into_iter()
            .map(|ip| ip.to_string())
            .collect();
            let upstream_overview = state.gateway.upstream.overview(None);
            let upstream_enabled_count = if upstream_overview.enabled {
                upstream_overview
                    .servers
                    .iter()
                    .filter(|s| s.config.enabled)
                    .count()
            } else {
                0
            };
            LocalDaemonResponse::Status {
                status: Box::new(DaemonStatusPayload {
                    pid: std::process::id(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    overview,
                    lan_enabled: settings.lan_federation.enabled,
                    device_id: lan_cfg.device_id,
                    device_name: lan_cfg.device_name,
                    addresses,
                    paired_peers_count: settings.lan_federation.paired_peers.len(),
                    upstream_enabled_count,
                    upstream_direct_tools_count: upstream_overview.direct_exposed_tool_count,
                }),
            }
        }
        LocalDaemonRequest::GeneratePin => {
            let mut enabled_changed = false;
            if let Err(err) = state.with_settings(|store| {
                let mut settings = store.settings();
                if !settings.lan_federation.enabled {
                    settings.lan_federation.enabled = true;
                    enabled_changed = true;
                    store.update_settings(settings)?;
                }
                Ok(())
            }) {
                return LocalDaemonResponse::Error {
                    message: format!("启用局域网联邦失败: {err}"),
                };
            }
            crate::lan_peer::sync_discovery_daemon();
            if enabled_changed {
                if let Err(err) =
                    crate::runtime::service::apply_live_runtime_changes(state, false).await
                {
                    return LocalDaemonResponse::Error {
                        message: format!("已开启局域网联邦，但重载监听地址失败: {err}"),
                    };
                }
            }
            let settings = state
                .with_settings(|store| Ok(store.settings()))
                .unwrap_or_default();
            let (lan_cfg, _) = crate::lan_peer::ensure_local_identity().unwrap_or_default();
            let addresses = crate::local_network::local_lan_ipv4_addrs_with_subnets(
                &settings.lan_federation.extra_subnets,
            )
            .into_iter()
            .map(|ip| ip.to_string())
            .collect();
            let pin = crate::lan_peer::generate_pairing_pin();
            LocalDaemonResponse::PinIssued {
                payload: DaemonPinPayload {
                    pin,
                    mcp_port: settings.global_gateway.local_port,
                    device_id: lan_cfg.device_id,
                    device_name: lan_cfg.device_name,
                    addresses,
                },
            }
        }
        LocalDaemonRequest::CancelPin => {
            crate::lan_peer::cancel_pairing_pin();
            LocalDaemonResponse::Ack {
                message: "已取消当前配对码".into(),
            }
        }
        LocalDaemonRequest::Reload { tunnel_changed } => {
            crate::lan_peer::sync_discovery_daemon();
            state.gateway.upstream.reload_config(None).await;
            match crate::runtime::service::apply_live_runtime_changes(state, tunnel_changed).await {
                Ok(rebound) => LocalDaemonResponse::Ack {
                    message: if rebound {
                        "已重绑并更新运行中 MCP 服务监听配置".into()
                    } else {
                        "已热更新运行中 MCP 服务配置".into()
                    },
                },
                Err(err) => LocalDaemonResponse::Error {
                    message: err.to_string(),
                },
            }
        }
        LocalDaemonRequest::ReloadUpstream => {
            state.gateway.upstream.reload_config(None).await;
            LocalDaemonResponse::Ack {
                message: "已热更新运行中聚合 MCP 配置".into(),
            }
        }
        LocalDaemonRequest::Stop => {
            shutdown_notify.notify_waiters();
            LocalDaemonResponse::Ack {
                message: "正在停止后台守护进程".into(),
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn desktop_unix_socket_wakes_primary_and_blocks_duplicate_instance() {
        let temp = tempfile::tempdir().expect("tempdir");
        let sock = temp.path().join("ipc").join("desktop.sock");
        let wakeups = Arc::new(AtomicUsize::new(0));
        let wakeups_cb = Arc::clone(&wakeups);

        let first = acquire_unix_desktop_instance_at(
            &sock,
            Arc::new(move || {
                wakeups_cb.fetch_add(1, Ordering::SeqCst);
            }),
        )
        .expect("first instance");
        assert!(first, "first instance must acquire lock");

        let second = acquire_unix_desktop_instance_at(&sock, Arc::new(|| {}))
            .expect("second instance check");
        assert!(!second, "duplicate instance must yield to primary");
        assert_eq!(wakeups.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn stale_desktop_socket_is_reclaimed_cleanly() {
        let temp = tempfile::tempdir().expect("tempdir");
        let sock_dir = temp.path().join("stale-ipc");
        ensure_private_dir_at(&sock_dir, unsafe { libc::geteuid() }).expect("private dir");
        let sock = sock_dir.join("desktop.sock");
        {
            let _stale = std::os::unix::net::UnixListener::bind(&sock).expect("bind stale");
        }
        assert!(sock.exists());

        let acquired =
            acquire_unix_desktop_instance_at(&sock, Arc::new(|| {})).expect("reclaim stale");
        assert!(acquired);
    }

    #[test]
    fn live_daemon_socket_in_fallback_dir_is_discovered_across_xdg_mismatch() {
        let temp = tempfile::tempdir().expect("tempdir");
        let euid = unsafe { libc::geteuid() };
        let xdg_root = temp.path().join("xdg");
        ensure_private_dir_at(&xdg_root, euid).expect("xdg root");
        let tmp_root = temp.path().join("tmp");
        std::fs::create_dir_all(&tmp_root).expect("tmp root");

        let candidates = candidate_ipc_dirs_for_uid(euid, Some(xdg_root), tmp_root.clone());
        let fallback_dir = tmp_root.join(format!("coding-tools-mcp-{euid}"));
        assert!(candidates.contains(&fallback_dir));

        ensure_private_dir_at(&fallback_dir, euid).expect("fallback private dir");
        let fallback_sock = fallback_dir.join("daemon.sock");
        let _listener =
            std::os::unix::net::UnixListener::bind(&fallback_sock).expect("bind fallback daemon");
        assert!(is_live_owned_daemon_socket(&fallback_sock, euid));
    }
}
