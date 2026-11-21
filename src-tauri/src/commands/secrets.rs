use serde::Deserialize;
use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::data::{DataStore, SHARED_KEYS};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedSecretUpdate {
    pub key: String,
    pub value: String,
}

#[tauri::command]
pub async fn get_shared_secret(
    state: State<'_, AppState>,
    key: String,
) -> AppResult<Option<String>> {
    if !SHARED_KEYS.contains(&key.as_str()) {
        return Err(AppError::Message(format!("invalid shared key: {key}")));
    }
    state.with_data(|store| Ok(store.get_shared_secret(&key)))
}

#[tauri::command]
pub async fn set_shared_secret(app: AppHandle, key: String, value: String) -> AppResult<()> {
    if !SHARED_KEYS.contains(&key.as_str()) {
        return Err(AppError::Message(format!("invalid shared key: {key}")));
    }
    if value.is_empty() {
        return Err(AppError::Message("密钥不能为空。".into()));
    }
    let app_for_save = app.clone();
    let key_for_save = key.clone();
    let changed = tauri::async_runtime::spawn_blocking(move || {
        app_for_save.state::<AppState>().with_data(|store| {
            if store.get_shared_secret(&key_for_save).as_deref() == Some(value.as_str()) {
                return Ok(false);
            }
            store.set_shared_secret(&key_for_save, &value)?;
            Ok(true)
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("保存共享密钥任务失败: {err}")))??;

    if changed {
        schedule_global_runtime_restart(app, key);
    }
    Ok(())
}

#[tauri::command]
pub async fn set_shared_secrets(app: AppHandle, updates: Vec<SharedSecretUpdate>) -> AppResult<()> {
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

    let app_for_save = app.clone();
    let first_changed_key = tauri::async_runtime::spawn_blocking(move || {
        app_for_save.state::<AppState>().with_data(|store| {
            let changed = normalized
                .iter()
                .filter(|(key, value)| {
                    store.get_shared_secret(key).as_deref() != Some(value.as_str())
                })
                .map(|(key, _)| key.clone())
                .next();
            if changed.is_some() {
                store.set_shared_secrets(&normalized)?;
            }
            Ok(changed)
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("批量保存共享密钥任务失败: {err}")))??;

    if let Some(key) = first_changed_key {
        schedule_global_runtime_restart(app, key);
    }
    Ok(())
}

#[tauri::command]
pub async fn regenerate_shared_secret(key: String) -> AppResult<String> {
    if !SHARED_KEYS.contains(&key.as_str()) {
        return Err(AppError::Message(format!("invalid shared key: {key}")));
    }
    Ok(DataStore::generate_shared_secret_value(&key))
}

fn schedule_global_runtime_restart(app: AppHandle, key: String) {
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
    let should_reload_mcp = SHARED_KEYS.contains(&key)
        && state
            .with_runtime(|runtime| Ok(runtime.is_running()))
            .unwrap_or(false);
    if should_reload_mcp {
        match state.with_runtime(|runtime| runtime.reload_live_policy()) {
            Ok(true) => return,
            Ok(false) => {}
            Err(error) => {
                eprintln!(
                    "Global MCP live policy reload after shared secret change failed: {error}"
                );
            }
        }
        if let Err(error) = crate::commands::runtime::restart_global_mcp(state).await {
            eprintln!("Global MCP restart after shared secret change failed: {error}");
        }
    }
}
