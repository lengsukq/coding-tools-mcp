use std::sync::{Arc, RwLock};
use std::time::Duration;

use crate::async_rt::JoinHandle;

use crate::error::AppResult;
use crate::mcp;
use crate::mcp::gateway::GatewayState;
use crate::platform::platform;
use crate::runtime::port::{
    is_own_process, port_busy_message, try_reclaim_previous_macos_app_port,
    wait_for_port_free_blocking,
};
use crate::secret::SecretStore;
use crate::settings::AppSettings;
use crate::tunnel::append_profile_log;
use crate::usage::ServiceUsageStats;
use crate::workspace::{AuthConfig, RuntimeStatusDto};

const GLOBAL_RUNTIME_LOG_SCOPE: &str = "global-mcp";

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimePhase {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListenerRebindNeed {
    None,
    BindHostOnly,
    PortChanged,
}

struct RuntimeEntry {
    phase: RuntimePhase,
    shutdown: Option<mcp::ShutdownSender>,
    handle: Option<JoinHandle<()>>,
    live_policy: Option<Arc<RwLock<mcp::LiveListenerPolicy>>>,
    bound_port: u16,
    bound_all_interfaces: bool,
    error_message: Option<String>,
    started_at: Option<std::time::Instant>,
    missing_port_checks: u8,
}

/// 0.3 runtime supervisor: exactly one MCP listener for the whole application.
///
/// Workspace lifecycle no longer owns network listeners. Workspaces are routed
/// inside `GatewayState`; deleting one only invalidates session selections that
/// referenced it.
pub struct RuntimeSupervisor {
    entry: Option<RuntimeEntry>,
    gateway: Arc<GatewayState>,
}

impl Default for RuntimeSupervisor {
    fn default() -> Self {
        Self::new(Arc::new(GatewayState::default()))
    }
}

impl RuntimeSupervisor {
    pub fn new(gateway: Arc<GatewayState>) -> Self {
        Self {
            entry: None,
            gateway,
        }
    }

    pub fn mcp_status(&self) -> RuntimeStatusDto {
        self.status()
    }

    pub fn usage_stats(&self, workspace_id: &str) -> ServiceUsageStats {
        self.gateway
            .usage_for(workspace_id)
            .snapshot(workspace_id, "mcp")
    }

    pub fn start_mcp(&mut self) -> AppResult<RuntimeStatusDto> {
        self.start()
    }

    pub fn is_running(&self) -> bool {
        matches!(
            self.entry.as_ref().map(|entry| &entry.phase),
            Some(RuntimePhase::Running)
        )
    }

    pub fn bound_port(&self) -> Option<u16> {
        self.entry
            .as_ref()
            .filter(|entry| entry.phase == RuntimePhase::Running)
            .map(|entry| entry.bound_port)
    }

    pub fn rebind_need(&self, settings: &AppSettings) -> ListenerRebindNeed {
        let Some(entry) = self
            .entry
            .as_ref()
            .filter(|e| e.phase == RuntimePhase::Running)
        else {
            return ListenerRebindNeed::None;
        };
        if entry.bound_port != settings.global_gateway.local_port {
            return ListenerRebindNeed::PortChanged;
        }
        let want_all = settings.allow_lan_access || settings.lan_federation.enabled;
        if entry.bound_all_interfaces != want_all {
            return ListenerRebindNeed::BindHostOnly;
        }
        ListenerRebindNeed::None
    }

    pub fn reload_live_policy(&self) -> AppResult<bool> {
        let Some(policy_lock) = self
            .entry
            .as_ref()
            .filter(|entry| entry.phase == RuntimePhase::Running)
            .and_then(|entry| entry.live_policy.as_ref())
        else {
            return Ok(false);
        };
        let settings = AppSettings::load_or_default();
        let (config, secrets) = build_listener_config_and_secrets(&settings)?;
        let next_policy = mcp::build_live_policy(
            config,
            secrets,
            settings.allow_lan_access,
            settings.allow_local_noauth,
        )
        .map_err(crate::error::AppError::Message)?;
        {
            let mut guard = policy_lock
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *guard = next_policy;
        }
        append_profile_log(
            GLOBAL_RUNTIME_LOG_SCOPE,
            "stdout.log",
            &format!(
                "[mcp] hot-reloaded listener policy (auth={}, allow_lan_mcp={}, allow_local_noauth={})",
                settings.global_mcp_auth_type,
                settings.allow_lan_access,
                settings.allow_local_noauth
            ),
        );
        Ok(true)
    }

    pub fn refresh_mcp(&mut self) {
        self.refresh();
    }

    pub fn begin_stop(&mut self) -> Option<JoinHandle<()>> {
        let entry = self.entry.as_mut()?;
        entry.phase = RuntimePhase::Stopping;
        if let Some(shutdown) = entry.shutdown.take() {
            let _ = shutdown.send(());
        }
        entry.handle.take()
    }

    pub fn finish_stop(&mut self) {
        self.entry = None;
    }

    fn status(&self) -> RuntimeStatusDto {
        let settings = AppSettings::load_or_default();
        let port = settings.global_gateway.local_port;
        let local_endpoint = format!("http://127.0.0.1:{port}/mcp");
        let public_base = settings.global_gateway.public_url.trim_end_matches('/');
        let public_endpoint = if public_base.is_empty() {
            String::new()
        } else {
            format!("{public_base}/mcp")
        };
        let phase = self
            .entry
            .as_ref()
            .map(|entry| entry.phase.clone())
            .unwrap_or(RuntimePhase::Stopped);

        match phase {
            RuntimePhase::Running => RuntimeStatusDto {
                state: "running".into(),
                pid: None,
                local_message: format!("Global MCP 正在监听 127.0.0.1:{port}"),
                public_message: settings.global_gateway.public_url,
                local_endpoint,
                public_endpoint,
            },
            RuntimePhase::Starting => RuntimeStatusDto {
                state: "starting".into(),
                pid: None,
                local_message: format!("正在启动 Global MCP 端口 {port}"),
                public_message: "等待服务就绪".into(),
                local_endpoint,
                public_endpoint,
            },
            RuntimePhase::Stopping => RuntimeStatusDto {
                state: "stopping".into(),
                pid: None,
                local_message: "正在停止 Global MCP".into(),
                public_message: "正在停止".into(),
                local_endpoint,
                public_endpoint,
            },
            RuntimePhase::Error => {
                let message = self
                    .entry
                    .as_ref()
                    .and_then(|entry| entry.error_message.clone())
                    .unwrap_or_else(|| "Global MCP 运行失败".into());
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
                local_message: "Global MCP 未启动".into(),
                public_message: "未知".into(),
                local_endpoint,
                public_endpoint,
            },
        }
    }

    fn start(&mut self) -> AppResult<RuntimeStatusDto> {
        if matches!(
            self.entry.as_ref().map(|entry| &entry.phase),
            Some(RuntimePhase::Running | RuntimePhase::Starting)
        ) {
            return Ok(self.status());
        }
        if matches!(
            self.entry.as_ref().map(|entry| &entry.phase),
            Some(RuntimePhase::Stopping)
        ) {
            return Err(crate::error::AppError::Message(
                "Global MCP 正在停止，请稍后再试".into(),
            ));
        }

        let settings = AppSettings::load_or_default();
        let port = settings.global_gateway.local_port;
        self.entry = Some(RuntimeEntry {
            phase: RuntimePhase::Starting,
            shutdown: None,
            handle: None,
            live_policy: None,
            bound_port: port,
            bound_all_interfaces: false,
            error_message: None,
            started_at: Some(std::time::Instant::now()),
            missing_port_checks: 0,
        });

        if let Some(pid) = platform().find_pid_listening_on_port(port)? {
            if is_own_process(pid) {
                wait_for_port_free_blocking(port, Duration::from_secs(3));
            }
            if try_reclaim_previous_macos_app_port(port) {
                // Previous app instance released the port.
            }
            if let Some(pid) = platform().find_pid_listening_on_port(port)? {
                self.entry = None;
                let message = port_busy_message(port, "Global MCP", pid);
                append_profile_log(
                    GLOBAL_RUNTIME_LOG_SCOPE,
                    "stderr.log",
                    &format!("[start] {message}"),
                );
                return Err(crate::error::AppError::Message(message));
            }
        }

        let (config, secrets) = build_listener_config_and_secrets(&settings)?;
        let spawn_result = mcp::spawn_gateway_listener(config, secrets, self.gateway.clone());

        match spawn_result {
            Ok(spawned) => {
                self.entry = Some(RuntimeEntry {
                    phase: RuntimePhase::Running,
                    shutdown: Some(spawned.shutdown),
                    handle: Some(spawned.handle),
                    live_policy: Some(spawned.policy),
                    bound_port: spawned.bound_port,
                    bound_all_interfaces: spawned.bound_all_interfaces,
                    error_message: None,
                    started_at: Some(std::time::Instant::now()),
                    missing_port_checks: 0,
                });
            }
            Err(error) => {
                append_profile_log(
                    GLOBAL_RUNTIME_LOG_SCOPE,
                    "stderr.log",
                    &format!("[start] Global MCP 启动失败：{error}"),
                );
                self.entry = Some(RuntimeEntry {
                    phase: RuntimePhase::Error,
                    shutdown: None,
                    handle: None,
                    live_policy: None,
                    bound_port: port,
                    bound_all_interfaces: false,
                    error_message: Some(error),
                    started_at: None,
                    missing_port_checks: 0,
                });
            }
        }
        Ok(self.status())
    }

    fn refresh(&mut self) {
        let Some(entry) = self.entry.as_mut() else {
            return;
        };
        if entry.phase != RuntimePhase::Running {
            return;
        }
        // Inspect the port this listener is actually bound to, not an unapplied
        // port saved in settings before the next restart.
        let port = entry.bound_port;
        let listening = match platform().find_pid_listening_on_port(port) {
            Ok(pid) => pid.is_some(),
            Err(error) => {
                append_profile_log(
                    GLOBAL_RUNTIME_LOG_SCOPE,
                    "stderr.log",
                    &format!("[refresh] 检查 Global MCP 端口 {port} 失败：{error}"),
                );
                return;
            }
        };
        if should_mark_runtime_error(entry, listening) {
            if let Some(handle) = entry.handle.take() {
                handle.abort();
                crate::async_rt::spawn(async move {
                    let _ = handle.await;
                });
            }
            entry.shutdown.take();
            entry.phase = RuntimePhase::Error;
            entry.error_message = Some(format!(
                "Global MCP 端口 {port} 未能成功监听，请检查端口占用后重试"
            ));
            entry.started_at = None;
        }
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

fn build_listener_config_and_secrets(
    settings: &AppSettings,
) -> AppResult<(mcp::GatewayListenerConfig, mcp::ListenerSecrets)> {
    let oauth_client_id = SecretStore::get_shared("oauth_client_id")?
        .unwrap_or_else(|| format!("chatgpt-client-{}", &uuid::Uuid::new_v4().to_string()[..12]));
    let auth = AuthConfig {
        auth_type: settings.global_mcp_auth_type.as_str().into(),
        oauth_client_id,
        use_shared_secrets: true,
    };
    let secrets = mcp::ListenerSecrets {
        oauth_client_secret: SecretStore::get_shared("oauth_client_secret")?,
        oauth_password: SecretStore::get_shared("oauth_password")?,
        oauth_token_secret: SecretStore::get_shared("oauth_token_secret")?,
    };
    Ok((
        mcp::GatewayListenerConfig {
            port: settings.global_gateway.local_port,
            auth,
            public_base_url: settings.global_gateway.public_url.clone(),
        },
        secrets,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(phase: RuntimePhase, started_at: Option<std::time::Instant>) -> RuntimeEntry {
        RuntimeEntry {
            phase,
            shutdown: None,
            handle: None,
            live_policy: None,
            bound_port: 28765,
            bound_all_interfaces: false,
            error_message: None,
            started_at,
            missing_port_checks: 0,
        }
    }

    #[test]
    fn refresh_keeps_a_listening_global_runtime() {
        let mut runtime = entry(RuntimePhase::Running, Some(std::time::Instant::now()));
        assert!(!should_mark_runtime_error(&mut runtime, true));
    }

    #[test]
    fn refresh_ignores_starting_global_runtime() {
        let mut runtime = entry(RuntimePhase::Starting, None);
        assert!(!should_mark_runtime_error(&mut runtime, false));
    }

    #[test]
    fn refresh_marks_global_runtime_only_after_repeated_missing_port_checks() {
        let mut runtime = entry(
            RuntimePhase::Running,
            Some(std::time::Instant::now() - Duration::from_secs(1)),
        );
        assert!(!should_mark_runtime_error(&mut runtime, false));
        assert!(!should_mark_runtime_error(&mut runtime, false));
        assert!(should_mark_runtime_error(&mut runtime, false));
    }
}
