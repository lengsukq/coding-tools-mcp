use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::error::{AppError, AppResult};
pub use crate::runtime::service::{
    apply_live_runtime_changes, collect_global_mcp_overview, remember_runtime_state_sync,
    restart_global_mcp, restore_runtime_state_inner, start_global_mcp, stop_global_mcp,
    GlobalMcpOverviewDto,
};
use crate::workspace::RuntimeStatusDto;

#[tauri::command]
pub async fn get_global_mcp_overview(app: AppHandle) -> AppResult<GlobalMcpOverviewDto> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        collect_global_mcp_overview(&state)
    })
    .await
    .map_err(|err| AppError::Message(format!("读取 Global MCP 概览失败: {err}")))?
}

async fn remember_runtime_state(app: &AppHandle, running: bool) -> AppResult<()> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        remember_runtime_state_sync(&state, running)
    })
    .await
    .map_err(|err| AppError::Message(format!("保存运行状态失败: {err}")))?
}

#[tauri::command]
pub async fn start_runtime(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<RuntimeStatusDto> {
    let status = start_global_mcp(&state).await?;
    if matches!(status.state.as_str(), "running" | "starting") {
        remember_runtime_state(&app, true).await?;
    }
    Ok(status)
}

#[tauri::command]
pub async fn stop_runtime(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<RuntimeStatusDto> {
    let status = stop_global_mcp(&state).await?;
    remember_runtime_state(&app, false).await?;
    Ok(status)
}

#[tauri::command]
pub async fn get_runtime_status(app: AppHandle) -> AppResult<RuntimeStatusDto> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>().with_runtime(|runtime| {
            runtime.refresh();
            Ok(runtime.status())
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("查询运行状态失败: {err}")))?
}

#[tauri::command]
pub async fn restore_runtime_state(state: State<'_, AppState>) -> AppResult<()> {
    restore_runtime_state_inner(&state).await
}
