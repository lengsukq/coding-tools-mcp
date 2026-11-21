use serde_json::Value;

use crate::computer;
use crate::error::{AppError, AppResult};

async fn run_computer_blocking<F>(label: &'static str, f: F) -> AppResult<Value>
where
    F: FnOnce() -> AppResult<Value> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|err| AppError::Message(format!("{label}任务失败: {err}")))?
}

/// These commands are available to the local Tauri webview only.
#[tauri::command]
pub async fn get_computer_screen_status() -> AppResult<Value> {
    run_computer_blocking("读取桌面控制状态", || Ok(computer::local_status())).await
}

#[tauri::command]
pub async fn approve_computer_screen_request(
    request_id: String,
    target_bundle: Option<String>,
) -> AppResult<Value> {
    run_computer_blocking("审批桌面控制请求", move || {
        computer::local_approve(&request_id, target_bundle.as_deref()).map_err(AppError::Message)
    })
    .await
}

#[tauri::command]
pub async fn list_computer_available_apps() -> AppResult<Value> {
    run_computer_blocking("枚举可用桌面应用", || Ok(computer::local_apps())).await
}

#[tauri::command]
pub async fn revoke_computer_screen_access() -> AppResult<Value> {
    run_computer_blocking("撤销桌面控制授权", || {
        computer::local_revoke().map_err(AppError::Message)
    })
    .await
}

#[tauri::command]
pub async fn pause_computer_access() -> AppResult<Value> {
    run_computer_blocking("暂停桌面控制", || {
        computer::local_pause().map_err(AppError::Message)
    })
    .await
}

/// Local webview only. MCP tools cannot enable Computer Use.
#[tauri::command]
pub async fn enable_computer_access(
    scope: String,
    duration_minutes: u64,
    target_bundle: Option<String>,
) -> AppResult<Value> {
    run_computer_blocking("启用桌面控制", move || {
        computer::local_enable(&scope, duration_minutes, target_bundle.as_deref())
            .map_err(AppError::Message)
    })
    .await
}

#[tauri::command]
pub async fn resume_computer_access() -> AppResult<Value> {
    run_computer_blocking("恢复桌面控制", || {
        computer::local_resume().map_err(AppError::Message)
    })
    .await
}

#[tauri::command]
pub async fn decide_computer_sensitive_action(
    confirmation_id: String,
    approve: bool,
) -> AppResult<Value> {
    run_computer_blocking("审批高风险动作", move || {
        computer::local_decide_sensitive_action(&confirmation_id, approve)
            .map_err(AppError::Message)
    })
    .await
}

#[tauri::command]
pub async fn request_computer_screen_permission() -> AppResult<Value> {
    run_computer_blocking("请求屏幕录制权限", || {
        Ok(computer::local_request_screen_permission())
    })
    .await
}

#[tauri::command]
pub async fn request_computer_accessibility_permission() -> AppResult<Value> {
    run_computer_blocking("请求辅助功能权限", || {
        Ok(computer::local_request_accessibility_permission())
    })
    .await
}
