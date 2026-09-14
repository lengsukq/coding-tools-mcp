use tauri::State;

use std::sync::LazyLock;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio::sync::Mutex as AsyncMutex;

use crate::app_state::AppState;
use crate::error::{AppError, AppResult};
use crate::global_gateway;
use crate::platform::platform;
use crate::runtime::{
    await_listener_shutdown, port_busy_message, try_reclaim_previous_macos_app_port,
    wait_for_port_free,
};
use crate::tunnel::{maybe_start_for_runtime, stop_for_runtime, sync_managed_runtime_routes};
use crate::workspace::resources::validate_service_start;
use crate::workspace::RuntimeStatusDto;

/// Serialize MCP restarts so secret-save and form-save cannot tear down
/// the same listener concurrently (that race could abort the process on Windows).
static RESTART_GATE: LazyLock<AsyncMutex<()>> = LazyLock::new(|| AsyncMutex::new(()));

fn remember_runtime_state(state: &AppState, id: &str, running: bool) -> AppResult<()> {
    state.with_settings(|store| {
        let mut settings = store.settings();
        let ids = &mut settings.restore_mcp_workspace_ids;

        if running {
            if !ids.iter().any(|workspace_id| workspace_id == id) {
                ids.push(id.to_string());
                ids.sort();
            }
        } else {
            ids.retain(|workspace_id| workspace_id != id);
        }
        store.update_settings(settings)
    })
}

fn profile_by_id(state: &AppState, id: &str) -> AppResult<crate::workspace::WorkspaceProfile> {
    state.with_workspaces(|store| {
        store
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::Message(format!("workspace not found: {id}")))
    })
}

fn validate_start_resources(state: &AppState, id: &str) -> AppResult<()> {
    state.with_workspaces(|store| validate_service_start(store.list(), id))
}

fn persist_tunnel_url(
    state: &AppState,
    id: &str,
    url: &str,
) -> AppResult<()> {
    if url.is_empty() {
        return Ok(());
    }

    state.with_workspaces(|store| {
        let Some(mut profile) = store.get(id).cloned() else {
            return Ok(());
        };

        profile.tunnel.public_url = url.to_string();

        store.update(profile)?;
        Ok(())
    })
}

async fn sync_tunnel_routes_from_runtime(state: &AppState) -> AppResult<()> {
    let active_keys = state.with_runtime(|runtime| Ok(runtime.active_tunnel_workspace_ids()))?;
    sync_managed_runtime_routes(active_keys).await
}

#[allow(clippy::collapsible_if)]
async fn ensure_port_available(port: u16, service_label: &str) -> AppResult<()> {
    let Some(pid) = platform().find_pid_listening_on_port(port)? else {
        return Ok(());
    };

    if crate::runtime::is_own_process(pid) {
        if wait_for_port_free(port, Duration::from_secs(3)).await {
            return Ok(());
        }
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

async fn stop_mcp_service(state: &AppState, id: &str) -> AppResult<RuntimeStatusDto> {
    let profile = profile_by_id(state, id)?;
    let port = profile.runtime.local_port;
    let handle = state.with_runtime(|runtime| Ok(runtime.begin_stop(id)))?;
    await_listener_shutdown(handle, port).await;
    state.with_runtime(|runtime| {
        runtime.finish_stop(id);
        Ok(runtime.mcp_status(&profile))
    })?;
    stop_for_runtime(&profile).await?;
    sync_tunnel_routes_from_runtime(state).await?;
    state.with_runtime(|runtime| Ok(runtime.mcp_status(&profile)))
}

async fn start_mcp_service(state: &AppState, id: &str) -> AppResult<RuntimeStatusDto> {
    validate_start_resources(state, id)?;
    let profile = profile_by_id(state, id)?;
    ensure_port_available(profile.runtime.local_port, "本地 MCP").await?;
    if profile.tunnel.use_global_gateway {
        global_gateway::ensure_started().await?;
    }
    let profile = profile_by_id(state, id)?;
    state.with_runtime(|runtime| runtime.start_mcp(&profile))?;
    sync_tunnel_routes_from_runtime(state).await?;

    match maybe_start_for_runtime(&profile).await {
        Ok(Some(url)) => {
            persist_tunnel_url(state, id, &url)?;
        }
        Ok(None) => {}
        Err(error) => {
            eprintln!("mcp tunnel auto-start failed for {id}: {error}");
        }
    }

    let profile = profile_by_id(state, id)?;
    tokio::time::sleep(Duration::from_millis(250)).await;
    state.with_runtime(|runtime| {
        runtime.refresh_mcp(&profile);
        Ok(runtime.mcp_status(&profile))
    })
}

/// Async stop→start for MCP. Used by the Tauri command and secret-change hooks.
pub(crate) async fn restart_mcp_by_id(
    state: &AppState,
    id: &str,
) -> AppResult<RuntimeStatusDto> {
    let _guard = RESTART_GATE.lock().await;
    let was_running = state.with_runtime(|runtime| Ok(runtime.is_running(id)))?;
    if was_running {
        let _ = stop_mcp_service(state, id).await?;
    }
    start_mcp_service(state, id).await
}

#[tauri::command]
pub async fn start_runtime(state: State<'_, AppState>, id: String) -> AppResult<RuntimeStatusDto> {
    let status = start_mcp_service(&state, &id).await?;
    if status.state == "running" || status.state == "starting" {
        remember_runtime_state(&state, &id, true)?;
    }
    Ok(status)
}

#[tauri::command]
pub async fn stop_runtime(state: State<'_, AppState>, id: String) -> AppResult<RuntimeStatusDto> {
    let status = stop_mcp_service(&state, &id).await?;
    remember_runtime_state(&state, &id, false)?;
    Ok(status)
}

#[tauri::command]
pub fn get_runtime_status(state: State<'_, AppState>, id: String) -> AppResult<RuntimeStatusDto> {
    let profile = profile_by_id(&state, &id)?;
    state.with_runtime(|runtime| {
        runtime.refresh_mcp(&profile);
        Ok(runtime.mcp_status(&profile))
    })
}

#[tauri::command]
pub async fn restart_runtime(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<RuntimeStatusDto> {
    restart_mcp_by_id(&state, &id).await
}

#[tauri::command]
pub async fn restore_runtime_state(state: State<'_, AppState>) -> AppResult<()> {
    if state
        .startup_restore_attempted
        .swap(true, Ordering::SeqCst)
    {
        return Ok(());
    }

    let settings = state.with_settings(|store| Ok(store.settings()))?;
    if !settings.restore_runtime_state_on_launch {
        return Ok(());
    }

    for id in settings.restore_mcp_workspace_ids {
        if profile_by_id(&state, &id).is_err() {
            continue;
        }
        if let Err(error) = start_mcp_service(&state, &id).await {
            eprintln!("failed to restore MCP runtime for {id}: {error}");
        }
    }

    Ok(())
}
