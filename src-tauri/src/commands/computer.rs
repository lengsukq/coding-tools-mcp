use serde_json::Value;

use crate::computer;
use crate::error::{AppError, AppResult};

/// These commands are available to the local Tauri webview only.
#[tauri::command]
pub fn get_computer_screen_status() -> Value {
    computer::local_status()
}

#[tauri::command]
pub fn approve_computer_screen_request(
    request_id: String,
    target_bundle: Option<String>,
) -> AppResult<Value> {
    computer::local_approve(&request_id, target_bundle.as_deref()).map_err(AppError::Message)
}

#[tauri::command]
pub fn list_computer_available_apps() -> Value {
    computer::local_apps()
}

#[tauri::command]
pub fn revoke_computer_screen_access() -> AppResult<Value> {
    computer::local_revoke().map_err(AppError::Message)
}

#[tauri::command]
pub fn pause_computer_access() -> AppResult<Value> {
    computer::local_pause().map_err(AppError::Message)
}

/// Local webview only. MCP tools cannot enable Computer Use.
#[tauri::command]
pub fn enable_computer_access(
    scope: String,
    duration_minutes: u64,
    target_bundle: Option<String>,
) -> AppResult<Value> {
    computer::local_enable(&scope, duration_minutes, target_bundle.as_deref())
        .map_err(AppError::Message)
}

#[tauri::command]
pub fn resume_computer_access() -> AppResult<Value> {
    computer::local_resume().map_err(AppError::Message)
}

#[tauri::command]
pub fn decide_computer_sensitive_action(
    confirmation_id: String,
    approve: bool,
) -> AppResult<Value> {
    computer::local_decide_sensitive_action(&confirmation_id, approve).map_err(AppError::Message)
}

#[tauri::command]
pub fn request_computer_screen_permission() -> Value {
    computer::local_request_screen_permission()
}

#[tauri::command]
pub fn request_computer_accessibility_permission() -> Value {
    computer::local_request_accessibility_permission()
}
