use std::sync::atomic::Ordering;
use std::sync::LazyLock;
use std::time::Duration;

use serde::Serialize;
use tauri::State;
use tokio::sync::Mutex as AsyncMutex;

use crate::app_state::AppState;
use crate::error::{AppError, AppResult};
use crate::global_gateway;
use crate::platform::platform;
use crate::runtime::{
    await_listener_shutdown, port_busy_message, try_reclaim_previous_macos_app_port,
    wait_for_port_free,
};
use crate::secret::SecretStore;
use crate::workspace::RuntimeStatusDto;

/// Serialize Global MCP restarts so settings/secret saves cannot race teardown.
static RESTART_GATE: LazyLock<AsyncMutex<()>> = LazyLock::new(|| AsyncMutex::new(()));

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalMcpSessionDto {
    pub session_id: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub last_seen_at: u64,
}

#[tauri::command]
pub fn get_global_mcp_overview(state: State<'_, AppState>) -> AppResult<GlobalMcpOverviewDto> {
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
    let sessions = state
        .gateway
        .sessions
        .snapshots()
        .into_iter()
        .map(|(session_id, scope)| {
            let workspace_id = scope.active_workspace_id.unwrap_or_default();
            let workspace_name = if workspace_id.is_empty() {
                "未选择 Workspace".into()
            } else {
                names
                    .get(&workspace_id)
                    .cloned()
                    .unwrap_or_else(|| "已删除 Workspace".into())
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

fn validate_global_auth(auth_type: &str, bearer_token: Option<&str>) -> AppResult<()> {
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

#[derive(Debug, Clone, Serialize)]
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

fn remember_runtime_state(state: &AppState, running: bool) -> AppResult<()> {
    state.with_settings(|store| {
        let mut settings = store.settings();
        settings.global_mcp_was_running = running;
        // 0.2 restore ids are intentionally no longer authoritative in 0.3.
        settings.restore_mcp_workspace_ids.clear();
        store.update_settings(settings)
    })
}

async fn ensure_port_available(port: u16, service_label: &str) -> AppResult<()> {
    let Some(pid) = platform().find_pid_listening_on_port(port)? else {
        return Ok(());
    };

    if crate::runtime::is_own_process(pid) && wait_for_port_free(port, Duration::from_secs(3)).await
    {
        return Ok(());
    }

    if try_reclaim_previous_macos_app_port(port) {
        return Ok(());
    }

    if let Some(pid) = platform().find_pid_listening_on_port(port)? {
        return Err(AppError::Message(port_busy_message(
            port,
            service_label,
            pid,
        )));
    }

    Ok(())
}

async fn stop_mcp_service(state: &AppState) -> AppResult<RuntimeStatusDto> {
    // Stop the public transport first so it cannot forward traffic while the
    // local endpoint is shutting down.
    global_gateway::stop().await?;
    let port = state.with_settings(|store| Ok(store.settings().global_gateway.local_port))?;
    let handle = state.with_runtime(|runtime| Ok(runtime.begin_stop()))?;
    await_listener_shutdown(handle, port).await;
    state.with_runtime(|runtime| {
        runtime.finish_stop();
        Ok(runtime.mcp_status())
    })
}

async fn start_mcp_service(state: &AppState) -> AppResult<RuntimeStatusDto> {
    let settings = state.with_settings(|store| Ok(store.settings()))?;
    let bearer_token = if settings.global_mcp_auth_type.trim() == "bearer" {
        SecretStore::get_shared("bearer_token")?
    } else {
        None
    };
    validate_global_auth(&settings.global_mcp_auth_type, bearer_token.as_deref())?;
    ensure_port_available(settings.global_gateway.local_port, "Global MCP").await?;
    state.with_runtime(|runtime| runtime.start_mcp())?;

    // A public tunnel is optional. Local MCP stays running even if the tunnel
    // cannot be established, matching the previous best-effort tunnel behavior.
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

pub(crate) async fn restart_global_mcp(state: &AppState) -> AppResult<RuntimeStatusDto> {
    let _guard = RESTART_GATE.lock().await;
    let was_running = state.with_runtime(|runtime| Ok(runtime.is_running()))?;
    if was_running {
        let _ = stop_mcp_service(state).await?;
    }
    start_mcp_service(state).await
}

#[tauri::command]
pub async fn start_runtime(state: State<'_, AppState>) -> AppResult<RuntimeStatusDto> {
    let status = start_mcp_service(&state).await?;
    if matches!(status.state.as_str(), "running" | "starting") {
        remember_runtime_state(&state, true)?;
    }
    Ok(status)
}

#[tauri::command]
pub async fn stop_runtime(state: State<'_, AppState>) -> AppResult<RuntimeStatusDto> {
    let status = stop_mcp_service(&state).await?;
    remember_runtime_state(&state, false)?;
    Ok(status)
}

#[tauri::command]
pub fn get_runtime_status(state: State<'_, AppState>) -> AppResult<RuntimeStatusDto> {
    state.with_runtime(|runtime| {
        runtime.refresh_mcp();
        Ok(runtime.mcp_status())
    })
}

#[tauri::command]
pub async fn restart_runtime(state: State<'_, AppState>) -> AppResult<RuntimeStatusDto> {
    restart_global_mcp(&state).await
}

#[tauri::command]
pub async fn restore_runtime_state(state: State<'_, AppState>) -> AppResult<()> {
    if state.startup_restore_attempted.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    let settings = state.with_settings(|store| Ok(store.settings()))?;
    if !settings.restore_runtime_state_on_launch || !settings.global_mcp_was_running {
        return Ok(());
    }

    if let Err(error) = start_mcp_service(&state).await {
        eprintln!("failed to restore Global MCP runtime: {error}");
    }
    Ok(())
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
