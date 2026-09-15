use serde::Deserialize;
use tauri::{Manager, State};

use crate::app_state::AppState;
use crate::data::DataStore;
use crate::error::{AppError, AppResult};

const SHARED_KEYS: &[&str] = &[
    "oauth_client_id",
    "bearer_token",
    "oauth_client_secret",
    "oauth_password",
    "oauth_token_secret",
];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedSecretUpdate {
    pub key: String,
    pub value: String,
}

#[tauri::command]
pub fn get_shared_secret(state: State<'_, AppState>, key: String) -> AppResult<Option<String>> {
    if !SHARED_KEYS.contains(&key.as_str()) {
        return Err(AppError::Message(format!("invalid shared key: {key}")));
    }
    state.with_data(|store| Ok(store.get_shared_secret(&key)))
}

#[tauri::command]
pub fn set_shared_secret(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> AppResult<()> {
    if !SHARED_KEYS.contains(&key.as_str()) {
        return Err(AppError::Message(format!("invalid shared key: {key}")));
    }
    if value.is_empty() {
        return Err(AppError::Message("密钥不能为空。".into()));
    }
    let changed = state.with_data(|store| {
        if store.get_shared_secret(&key).as_deref() == Some(value.as_str()) {
            return Ok(false);
        }
        store.set_shared_secret(&key, &value)?;
        Ok(true)
    })?;
    if changed {
        schedule_global_runtime_restart(app, key);
    }
    Ok(())
}

#[tauri::command]
pub fn set_shared_secrets(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    updates: Vec<SharedSecretUpdate>,
) -> AppResult<()> {
    let mut normalized = Vec::with_capacity(updates.len());
    for update in updates {
        let key = update.key.trim().to_string();
        if !SHARED_KEYS.contains(&key.as_str()) {
            return Err(AppError::Message(format!("invalid shared key: {key}")));
        }
        if update.value.is_empty() {
            return Err(AppError::Message(format!("密钥 {key} 不能为空。")));
        }
        normalized.push((key, update.value));
    }

    let first_changed_key = state.with_data(|store| {
        let changed = normalized
            .iter()
            .filter(|(key, value)| store.get_shared_secret(key).as_deref() != Some(value.as_str()))
            .map(|(key, _)| key.clone())
            .next();
        if changed.is_some() {
            store.set_shared_secrets(&normalized)?;
        }
        Ok(changed)
    })?;

    if let Some(key) = first_changed_key {
        schedule_global_runtime_restart(app, key);
    }
    Ok(())
}

#[tauri::command]
pub fn regenerate_shared_secret(
    _app: tauri::AppHandle,
    _state: State<'_, AppState>,
    key: String,
) -> AppResult<String> {
    if !SHARED_KEYS.contains(&key.as_str()) {
        return Err(AppError::Message(format!("invalid shared key: {key}")));
    }
    Ok(DataStore::generate_shared_secret_value(&key))
}

fn schedule_global_runtime_restart(app: tauri::AppHandle, key: String) {
    // Must stay on the async runtime: sync restart_mcp while a listener is
    // shutting down previously raced with form-save restart and could abort.
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        restart_global_runtime_async(state.inner(), &key).await;
    });
}

/// 仅重启当前确实在运行、且使用了这组密钥的服务。
///
/// 密钥命令是桌面端和设置页共用的入口，因此重启必须放在后端统一处理。
/// 前端不再额外调用 restart_*，避免同一次密钥变更触发两次停止/启动竞态。
async fn restart_global_runtime_async(state: &AppState, key: &str) {
    let should_restart_mcp = SHARED_KEYS.contains(&key)
        && state
            .with_runtime(|runtime| Ok(runtime.is_running()))
            .unwrap_or(false);
    if should_restart_mcp {
        if let Err(error) = crate::commands::runtime::restart_global_mcp(state).await {
            eprintln!("Global MCP restart after shared secret change failed: {error}");
        }
    }
}
