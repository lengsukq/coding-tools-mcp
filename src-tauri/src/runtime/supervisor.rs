use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tauri::async_runtime::JoinHandle;

use crate::error::AppResult;
use crate::mcp;
use crate::platform::platform;
use crate::runtime::port::{
    is_own_process, port_busy_message, try_reclaim_previous_macos_app_port,
    wait_for_port_free_blocking,
};
use crate::secret::SecretStore;
use crate::tunnel::{append_profile_log, cleanup_orphan_for_runtime};
use crate::usage::{ServiceUsage, ServiceUsageStats};
use crate::workspace::{RuntimeStatusDto, WorkspaceProfile};

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimePhase {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
}

struct RuntimeEntry {
    phase: RuntimePhase,
    shutdown: Option<mcp::ShutdownSender>,
    handle: Option<JoinHandle<()>>,
    error_message: Option<String>,
    started_at: Option<std::time::Instant>,
    missing_port_checks: u8,
}

#[derive(Default)]
pub struct RuntimeSupervisor {
    entries: HashMap<String, RuntimeEntry>,
    usage: HashMap<String, Arc<ServiceUsage>>,
}

impl RuntimeSupervisor {
    pub fn mcp_status(&self, profile: &WorkspaceProfile) -> RuntimeStatusDto {
        self.status(profile)
    }

    pub fn running_workspace_ids(&self) -> Vec<String> {
        let mut ids = self
            .entries
            .iter()
            .filter(|&(_workspace_id, entry)| {
                matches!(entry.phase, RuntimePhase::Running | RuntimePhase::Starting)
            })
            .map(|(workspace_id, _entry)| workspace_id.clone())
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        ids
    }

    pub fn usage_stats(&self, workspace_id: &str) -> ServiceUsageStats {
        self.usage
            .get(workspace_id)
            .map(|usage| usage.snapshot(workspace_id, "mcp"))
            .unwrap_or_else(|| ServiceUsage::empty(workspace_id, "mcp"))
    }

    pub fn start_mcp(&mut self, profile: &WorkspaceProfile) -> AppResult<RuntimeStatusDto> {
        self.start(profile)
    }

    /// True when the service for this workspace is currently running.
    pub fn is_running(&self, workspace_id: &str) -> bool {
        matches!(
            self.entries.get(workspace_id).map(|entry| &entry.phase),
            Some(RuntimePhase::Running)
        )
    }

    pub fn refresh_mcp(&mut self, profile: &WorkspaceProfile) {
        self.refresh(profile);
    }

    pub fn drop_workspace(&mut self, profile: &WorkspaceProfile) {
        self.sync_stop_and_wait(profile);
    }

    pub fn active_tunnel_workspace_ids(&self) -> HashSet<String> {
        self.entries
            .iter()
            .filter_map(|(workspace_id, entry)| match entry.phase {
                RuntimePhase::Running | RuntimePhase::Starting => Some(workspace_id.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn begin_stop(&mut self, workspace_id: &str) -> Option<JoinHandle<()>> {
        let entry = self.entries.get_mut(workspace_id)?;

        entry.phase = RuntimePhase::Stopping;
        let shutdown = entry.shutdown.take();
        let handle = entry.handle.take();
        if let Some(shutdown) = shutdown {
            let _ = shutdown.send(());
        }
        handle
    }

    pub fn finish_stop(&mut self, workspace_id: &str) {
        self.entries.remove(workspace_id);
    }

    fn status(&self, profile: &WorkspaceProfile) -> RuntimeStatusDto {
        let key = profile.id.as_str();
        let phase = self
            .entries
            .get(key)
            .map(|entry| entry.phase.clone())
            .unwrap_or(RuntimePhase::Stopped);

        let local_endpoint = profile.local_endpoint();
        let public_endpoint = profile.public_endpoint();
        let port = profile.runtime.local_port;
        let service_label = "本地 MCP ";

        match phase {
            RuntimePhase::Running => RuntimeStatusDto {
                state: "running".into(),
                pid: None,
                local_message: format!("{service_label}正在监听 127.0.0.1:{port}"),
                public_message: profile.effective_public_url(),
                local_endpoint,
                public_endpoint,
            },
            RuntimePhase::Starting => RuntimeStatusDto {
                state: "starting".into(),
                pid: None,
                local_message: format!("正在启动{service_label}端口 {port}"),
                public_message: "等待服务就绪".into(),
                local_endpoint,
                public_endpoint,
            },
            RuntimePhase::Stopping => RuntimeStatusDto {
                state: "stopping".into(),
                pid: None,
                local_message: "正在停止".into(),
                public_message: "正在停止".into(),
                local_endpoint,
                public_endpoint,
            },
            RuntimePhase::Error => {
                let message = self
                    .entries
                    .get(key)
                    .and_then(|entry| entry.error_message.clone())
                    .unwrap_or_else(|| "运行失败".into());
                RuntimeStatusDto {
                    state: "error".into(),
                    pid: None,
                    local_message: message.clone(),
                    public_message: message,
                    local_endpoint,
                    public_endpoint,
                }
            }
            RuntimePhase::Stopped => RuntimeStatusDto {
                state: "stopped".into(),
                pid: None,
                local_message: "未启动".into(),
                public_message: "未知".into(),
                local_endpoint,
                public_endpoint,
            },
        }
    }

    fn start(&mut self, profile: &WorkspaceProfile) -> AppResult<RuntimeStatusDto> {
        let key = profile.id.clone();
        if matches!(
            self.entries.get(&key).map(|e| &e.phase),
            Some(RuntimePhase::Running) | Some(RuntimePhase::Starting)
        ) {
            return Ok(self.status(profile));
        }
        if matches!(
            self.entries.get(&key).map(|e| &e.phase),
            Some(RuntimePhase::Stopping)
        ) {
            return Err(crate::error::AppError::Message(format!(
                "{}正在停止，请稍后再试",
                "本地 MCP"
            )));
        }

        let usage = self
            .usage
            .entry(key.clone())
            .or_insert_with(|| Arc::new(ServiceUsage::default()))
            .clone();

        self.entries.insert(
            key.clone(),
            RuntimeEntry {
                phase: RuntimePhase::Starting,
                shutdown: None,
                handle: None,
                error_message: None,
                started_at: Some(std::time::Instant::now()),
                missing_port_checks: 0,
            },
        );

        let port = profile.runtime.local_port;
        if let Some(pid) = platform().find_pid_listening_on_port(port)? {
            if is_own_process(pid) {
                wait_for_port_free_blocking(port, Duration::from_secs(3));
            }
            if try_reclaim_previous_macos_app_port(port) {
                // A previous source-built or installed instance of this macOS
                // app released the port; continue with the current listener.
            }
            if let Some(pid) = platform().find_pid_listening_on_port(port)? {
                self.entries.remove(&key);
                let message = port_busy_message(port, "本地 MCP", pid);
                append_profile_log(&profile.id, "stderr.log", &format!("[start] {message}"));
                return Err(crate::error::AppError::Message(message));
            }
        }

        let use_shared = profile.auth.use_shared_secrets;
        let mut auth = profile.auth.clone();
        if use_shared {
            if let Some(client_id) = SecretStore::get_shared("oauth_client_id")? {
                auth.oauth_client_id = client_id;
            }
        }
        // MCP OAuth matches legacy Python: client_secret is optional.
        // ChatGPT connectors use PKCE only and do not send client_secret.
        let oauth_client_secret = None;
        let oauth_password = if profile.auth.oauth_enabled() {
            resolve_secret(&profile.id, "oauth_password", use_shared)?
        } else {
            None
        };
        let oauth_token_secret = if profile.auth.oauth_enabled() {
            resolve_secret(&profile.id, "oauth_token_secret", use_shared)?
        } else {
            None
        };
        let spawn_result = mcp::spawn_listener(
            port,
            PathBuf::from(&profile.path),
            profile.id.clone(),
            auth,
            profile.effective_public_url(),
            oauth_client_secret,
            oauth_password,
            oauth_token_secret,
            profile.runtime.clone(),
            usage.clone(),
        );

        match spawn_result {
            Ok((shutdown, handle)) => {
                let started_at = self
                    .entries
                    .get(&key)
                    .and_then(|entry| entry.started_at)
                    .or_else(|| Some(std::time::Instant::now()));
                self.entries.insert(
                    key,
                    RuntimeEntry {
                        phase: RuntimePhase::Running,
                        shutdown: Some(shutdown),
                        handle: Some(handle),
                        error_message: None,
                        started_at,
                        missing_port_checks: 0,
                    },
                );
            }
            Err(err) => {
                // spawn_listener can fail synchronously before the server task is
                // ever created (e.g. missing API key / OAuth secret). In that case
                // serve() never runs, so nothing writes to the stderr log and the
                // failure was previously invisible in the log viewer. Record it here.
                append_profile_log(
                    &profile.id,
                    "stderr.log",
                    &format!("[start] 本地 MCP 启动失败：{err}"),
                );
                self.entries.insert(
                    key,
                    RuntimeEntry {
                        phase: RuntimePhase::Error,
                        shutdown: None,
                        handle: None,
                        error_message: Some(err.to_string()),
                        started_at: None,
                        missing_port_checks: 0,
                    },
                );
            }
        }

        Ok(self.status(profile))
    }

    fn sync_stop_and_wait(&mut self, profile: &WorkspaceProfile) {
        let port = profile.runtime.local_port;
        let handle = self.begin_stop(&profile.id);
        if handle.is_some() {
            crate::runtime::port::await_listener_shutdown_blocking(handle, port);
        } else if platform()
            .find_pid_listening_on_port(port)
            .ok()
            .flatten()
            .is_some()
        {
            wait_for_port_free_blocking(port, Duration::from_secs(3));
        }
        self.finish_stop(&profile.id);
    }

    fn refresh(&mut self, profile: &WorkspaceProfile) {
        let key = profile.id.clone();
        let port = profile.runtime.local_port;
        let mut should_cleanup_tunnel = false;
        if let Some(entry) = self.entries.get_mut(&key) {
            if entry.phase == RuntimePhase::Running {
                let listening = match platform().find_pid_listening_on_port(port) {
                    Ok(pid) => pid.is_some(),
                    Err(error) => {
                        append_profile_log(
                            &profile.id,
                            "stderr.log",
                            &format!("[refresh] 检查端口 {port} 失败，保留当前线路：{error}"),
                        );
                        return;
                    }
                };
                if should_mark_runtime_error(entry, listening) {
                    if let Some(handle) = entry.handle.take() {
                        handle.abort();
                        tauri::async_runtime::spawn(async move {
                            let _ = handle.await;
                        });
                    }
                    entry.shutdown.take();
                    let occupied_by_self = platform()
                        .find_pid_listening_on_port(port)
                        .ok()
                        .flatten()
                        .map(is_own_process)
                        .unwrap_or(false);
                    let message = if occupied_by_self {
                        format!(
                            "{}端口 {} 未能成功启动，可能仍被本应用上一次服务占用，请先停止后再试",
                            "本地 MCP", port
                        )
                    } else {
                        format!(
                            "{}端口 {} 未能成功启动，可能已被其他程序占用",
                            "本地 MCP", port
                        )
                    };
                    entry.phase = RuntimePhase::Error;
                    entry.error_message = Some(message);
                    entry.started_at = None;
                    should_cleanup_tunnel = true;
                }
            }
        }

        // 状态查询本身不能改变其他工作区的隧道集合。只有本次刷新确认了
        // 一个原本 Running 的 runtime 已经进入 Error，才清理它对应的孤儿线路。
        // 之前无条件调用 cleanup_orphan 会把启动时的瞬时端口检测失败误认为
        // 孤儿 runtime，删除 route 后重启唯一的 frpc，导致其他工作区公网线路消失。
        if !should_cleanup_tunnel {
            return;
        }

        let profile = profile.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = cleanup_orphan_for_runtime(&profile, false).await {
                append_profile_log(
                    &profile.id,
                    "stderr.log",
                    &format!("[refresh] 清理失效隧道失败：{error}"),
                );
            }
        });
    }
}

fn should_mark_runtime_error(entry: &mut RuntimeEntry, listening: bool) -> bool {
    if entry.phase != RuntimePhase::Running {
        return false;
    }
    if listening {
        entry.missing_port_checks = 0;
        return false;
    }

    entry.missing_port_checks = entry.missing_port_checks.saturating_add(1);
    entry.missing_port_checks >= 3
        && entry
            .started_at
            .map(|started| started.elapsed() > Duration::from_millis(200))
            .unwrap_or(true)
}

/// Resolve a secret from the shared pool or per-workspace keyring.
fn resolve_secret(profile_id: &str, key: &str, use_shared: bool) -> AppResult<Option<String>> {
    if use_shared {
        SecretStore::get_shared(key)
    } else {
        SecretStore::get(profile_id, key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(phase: RuntimePhase, started_at: Option<std::time::Instant>) -> RuntimeEntry {
        RuntimeEntry {
            phase,
            shutdown: None,
            handle: None,
            error_message: None,
            started_at,
            missing_port_checks: 0,
        }
    }

    #[test]
    fn refresh_does_not_cleanup_a_running_runtime_that_is_listening() {
        let mut runtime = entry(RuntimePhase::Running, Some(std::time::Instant::now()));
        assert!(!should_mark_runtime_error(&mut runtime, true));
    }

    #[test]
    fn refresh_does_not_cleanup_a_starting_runtime() {
        let mut runtime = entry(RuntimePhase::Starting, None);
        assert!(!should_mark_runtime_error(&mut runtime, false));
    }

    #[test]
    fn refresh_cleans_up_only_after_running_runtime_is_confirmed_missing() {
        let mut runtime = entry(
            RuntimePhase::Running,
            Some(std::time::Instant::now() - Duration::from_secs(1)),
        );
        assert!(!should_mark_runtime_error(&mut runtime, false));
        assert!(!should_mark_runtime_error(&mut runtime, false));
        assert!(should_mark_runtime_error(&mut runtime, false));
    }

    #[test]
    fn a_recovered_port_clears_missing_port_checks() {
        let mut runtime = entry(
            RuntimePhase::Running,
            Some(std::time::Instant::now() - Duration::from_secs(1)),
        );
        assert!(!should_mark_runtime_error(&mut runtime, false));
        assert!(!should_mark_runtime_error(&mut runtime, true));
        assert!(!should_mark_runtime_error(&mut runtime, false));
    }
}
