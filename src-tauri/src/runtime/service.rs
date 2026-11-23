//! Shared headless-compatible Global MCP lifecycle and maintenance service.

use std::sync::atomic::Ordering;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex as AsyncMutex, MutexGuard as AsyncMutexGuard};

use crate::app_state::AppState;
use crate::error::{AppError, AppResult};
use crate::global_gateway;
use crate::platform::platform;
use crate::runtime::{
    await_listener_shutdown, is_own_process, port_busy_message,
    try_reclaim_previous_macos_app_port, wait_for_port_free, ListenerRebindNeed,
};
use crate::secret::SecretStore;
use crate::workspace::RuntimeStatusDto;

/// Serialize all Global MCP lifecycle transitions (start/stop/restart/rebind/reload)
/// so concurrent UI actions, CLI control commands, and settings saves cannot race.
static RESTART_GATE: LazyLock<AsyncMutex<()>> = LazyLock::new(|| AsyncMutex::new(()));

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalMcpSessionDto {
    pub session_id: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub last_seen_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalMcpOverviewDto {
    pub state: String,
    pub local_endpoint: String,
    pub public_endpoint: String,
    pub workspace_count: usize,
    pub session_count: usize,
    pub registry_revision: u64,
    pub sessions: Vec<GlobalMcpSessionDto>,
}

pub fn collect_global_mcp_overview(state: &AppState) -> AppResult<GlobalMcpOverviewDto> {
    let runtime = state.with_runtime(|runtime| {
        runtime.refresh_mcp();
        Ok(runtime.mcp_status())
    })?;
    let workspaces = state
        .gateway
        .registry
        .descriptors()
        .map_err(AppError::Message)?;
    let names = workspaces
        .iter()
        .map(|workspace| (workspace.id.clone(), workspace.name.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    let peer_names = state
        .with_settings(|store| Ok(store.settings().lan_federation.paired_peers))
        .unwrap_or_default()
        .into_iter()
        .map(|p| (p.device_id, p.device_name))
        .collect::<std::collections::HashMap<_, _>>();
    let sessions = state
        .gateway
        .sessions
        .snapshots()
        .into_iter()
        .map(|(session_id, scope)| {
            let workspace_id = scope.active_workspace_id.unwrap_or_default();
            let workspace_name = if workspace_id.is_empty() {
                "未选择 Workspace".into()
            } else if let Some(local_name) = names.get(&workspace_id) {
                local_name.clone()
            } else if let Some((dev_id, remote_ws)) = workspace_id.split_once(':') {
                if let Some(peer_name) = peer_names.get(dev_id) {
                    format!("[{peer_name}] {remote_ws}")
                } else {
                    "已删除或未配对 Workspace".into()
                }
            } else {
                "已删除 Workspace".into()
            };
            GlobalMcpSessionDto {
                session_id,
                workspace_name,
                workspace_id,
                last_seen_at: scope.last_seen_at,
            }
        })
        .collect::<Vec<_>>();
    Ok(GlobalMcpOverviewDto {
        state: runtime.state,
        local_endpoint: runtime.local_endpoint,
        public_endpoint: runtime.public_endpoint,
        workspace_count: workspaces.len(),
        session_count: state.gateway.sessions.active_session_count(),
        registry_revision: state.gateway.registry.revision().unwrap_or_default(),
        sessions,
    })
}

pub fn validate_global_auth(auth_type: &str, bearer_token: Option<&str>) -> AppResult<()> {
    match auth_type.trim() {
        "oauth" | "noauth" => Ok(()),
        "bearer" if bearer_token.is_some_and(|token| !token.trim().is_empty()) => Ok(()),
        "bearer" => Err(AppError::Message(
            "Global MCP 已启用 Bearer 认证，但 Bearer Token 为空。".into(),
        )),
        other => Err(AppError::Message(format!(
            "不支持的 Global MCP 认证模式: {other}"
        ))),
    }
}

pub fn remember_runtime_state_sync(state: &AppState, running: bool) -> AppResult<()> {
    state.with_settings(|store| {
        let mut settings = store.settings();
        if settings.global_mcp_was_running == running {
            return Ok(());
        }
        settings.global_mcp_was_running = running;
        store.update_settings(settings)
    })
}

async fn ensure_port_available(port: u16, service_label: &str) -> AppResult<()> {
    let pid = crate::async_rt::spawn_blocking(move || platform().find_pid_listening_on_port(port))
        .await
        .map_err(|err| AppError::Message(format!("端口探测任务失败: {err}")))??;

    let Some(pid) = pid else {
        return Ok(());
    };

    if is_own_process(pid) && wait_for_port_free(port, Duration::from_secs(3)).await {
        return Ok(());
    }

    let reclaimed =
        crate::async_rt::spawn_blocking(move || try_reclaim_previous_macos_app_port(port))
            .await
            .map_err(|err| AppError::Message(format!("端口回收任务失败: {err}")))?;
    if reclaimed {
        return Ok(());
    }

    let still_busy =
        crate::async_rt::spawn_blocking(move || platform().find_pid_listening_on_port(port))
            .await
            .map_err(|err| AppError::Message(format!("端口复检任务失败: {err}")))??;
    if let Some(pid) = still_busy {
        return Err(AppError::Message(port_busy_message(
            port,
            service_label,
            pid,
        )));
    }

    Ok(())
}

async fn stop_mcp_service_locked(
    state: &AppState,
    _gate: &AsyncMutexGuard<'_, ()>,
) -> AppResult<RuntimeStatusDto> {
    global_gateway::stop().await?;
    let port = state.with_settings(|store| Ok(store.settings().global_gateway.local_port))?;
    let handle = state.with_runtime(|runtime| Ok(runtime.begin_stop()))?;
    await_listener_shutdown(handle, port).await;
    state.with_runtime(|runtime| {
        runtime.finish_stop();
        Ok(runtime.mcp_status())
    })
}

async fn start_mcp_service_locked(
    state: &AppState,
    _gate: &AsyncMutexGuard<'_, ()>,
) -> AppResult<RuntimeStatusDto> {
    let settings = state.with_settings(|store| Ok(store.settings()))?;
    let bearer_token = if settings.global_mcp_auth_type.trim() == "bearer" {
        SecretStore::get_shared("bearer_token")?
    } else {
        None
    };
    validate_global_auth(&settings.global_mcp_auth_type, bearer_token.as_deref())?;
    ensure_port_available(settings.global_gateway.local_port, "Global MCP").await?;
    state.with_runtime(|runtime| runtime.start_mcp())?;

    if settings.global_gateway.enabled {
        if let Err(error) = global_gateway::ensure_started().await {
            eprintln!("Global MCP tunnel auto-start failed: {error}");
        }
    }

    tokio::time::sleep(Duration::from_millis(250)).await;
    state.with_runtime(|runtime| {
        runtime.refresh_mcp();
        Ok(runtime.mcp_status())
    })
}

async fn restart_global_mcp_locked(
    state: &AppState,
    gate: &AsyncMutexGuard<'_, ()>,
) -> AppResult<RuntimeStatusDto> {
    let was_running = state.with_runtime(|runtime| Ok(runtime.is_running()))?;
    if was_running {
        let _ = stop_mcp_service_locked(state, gate).await?;
    }
    start_mcp_service_locked(state, gate).await
}

pub async fn start_global_mcp(state: &AppState) -> AppResult<RuntimeStatusDto> {
    let gate = RESTART_GATE.lock().await;
    start_mcp_service_locked(state, &gate).await
}

pub async fn stop_global_mcp(state: &AppState) -> AppResult<RuntimeStatusDto> {
    let gate = RESTART_GATE.lock().await;
    stop_mcp_service_locked(state, &gate).await
}

pub async fn restart_global_mcp(state: &AppState) -> AppResult<RuntimeStatusDto> {
    let gate = RESTART_GATE.lock().await;
    restart_global_mcp_locked(state, &gate).await
}

async fn rebind_mcp_listener_only_locked(
    state: &AppState,
    _gate: &AsyncMutexGuard<'_, ()>,
) -> AppResult<()> {
    let bound_port = state.with_runtime(|runtime| Ok(runtime.bound_port()))?;
    let Some(old_port) = bound_port else {
        return Ok(());
    };
    let handle = state.with_runtime(|runtime| Ok(runtime.begin_stop()))?;
    await_listener_shutdown(handle, old_port).await;
    state.with_runtime(|runtime| {
        runtime.finish_stop();
        Ok(())
    })?;
    let settings = state.with_settings(|store| Ok(store.settings()))?;
    ensure_port_available(settings.global_gateway.local_port, "Global MCP").await?;
    state.with_runtime(|runtime| runtime.start_mcp())?;
    tokio::time::sleep(Duration::from_millis(120)).await;
    state.with_runtime(|runtime| {
        runtime.refresh_mcp();
        Ok(())
    })
}

pub async fn apply_live_runtime_changes(state: &AppState, tunnel_changed: bool) -> AppResult<bool> {
    let gate = RESTART_GATE.lock().await;
    let settings = state.with_settings(|store| Ok(store.settings()))?;
    let (is_running, rebind_need) =
        state.with_runtime(|runtime| Ok((runtime.is_running(), runtime.rebind_need(&settings))))?;
    if !is_running {
        return Ok(false);
    }

    match rebind_need {
        ListenerRebindNeed::PortChanged => {
            restart_global_mcp_locked(state, &gate).await?;
            return Ok(true);
        }
        ListenerRebindNeed::BindHostOnly => {
            rebind_mcp_listener_only_locked(state, &gate).await?;
        }
        ListenerRebindNeed::None => {
            let _ = state.with_runtime(|runtime| runtime.reload_live_policy())?;
        }
    }

    if tunnel_changed {
        global_gateway::stop().await?;
        if settings.global_gateway.enabled {
            global_gateway::ensure_started().await?;
        }
    }

    Ok(rebind_need != ListenerRebindNeed::None)
}

pub async fn restore_runtime_state_inner(state: &AppState) -> AppResult<()> {
    if state.startup_restore_attempted.swap(true, Ordering::SeqCst) {
        let _gate = RESTART_GATE.lock().await;
        return Ok(());
    }

    let settings = state.with_settings(|store| Ok(store.settings()))?;
    if !settings.restore_runtime_state_on_launch || !settings.global_mcp_was_running {
        return Ok(());
    }

    let gate = RESTART_GATE.lock().await;
    if let Err(error) = start_mcp_service_locked(state, &gate).await {
        eprintln!("failed to restore Global MCP runtime: {error}");
    }
    Ok(())
}

/// Run one periodic maintenance pass (session expiry, idle command reaping, LAN cache prune).
pub async fn run_maintenance_pass(gateway: &crate::mcp::gateway::GatewayState) {
    let (sessions, capabilities) = gateway.sessions.maintenance_sweep();
    let completed_commands = crate::tools::session::reap_idle_command_sessions().await;
    crate::lan_peer::prune_caches();
    #[cfg(target_os = "macos")]
    crate::platform::macos::process::trim_process_heap();
    if sessions > 0 || capabilities > 0 || completed_commands > 0 {
        eprintln!(
            "[memory-maintenance] expired_sessions={sessions} expired_capabilities={capabilities} completed_commands={completed_commands}"
        );
    }
}

pub fn spawn_gateway_maintenance(gateway: Arc<crate::mcp::gateway::GatewayState>) {
    crate::async_rt::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5 * 60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        loop {
            interval.tick().await;
            run_maintenance_pass(&gateway).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::validate_global_auth;

    #[test]
    fn global_auth_validation_fails_closed() {
        assert!(validate_global_auth("oauth", None).is_ok());
        assert!(validate_global_auth("noauth", None).is_ok());
        assert!(validate_global_auth("bearer", Some("token")).is_ok());
        assert!(validate_global_auth("bearer", None).is_err());
        assert!(validate_global_auth("bearer", Some("   ")).is_err());
        assert!(validate_global_auth("unexpected", Some("token")).is_err());
    }
}
