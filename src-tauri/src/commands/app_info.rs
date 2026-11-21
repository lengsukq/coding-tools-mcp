use tauri::State;

use crate::app_state::AppState;
use crate::error::{AppError, AppResult};
use crate::platform::open_url as platform_open_url;
use crate::update::{check_app_update as check_update, UpdateCheckResult};

#[tauri::command]
pub async fn open_url(url: String) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || platform_open_url(&url))
        .await
        .map_err(|err| AppError::Message(format!("打开链接任务失败: {err}")))?
}

#[tauri::command]
pub async fn check_app_update(state: State<'_, AppState>) -> AppResult<UpdateCheckResult> {
    let settings = state.with_settings(|store| Ok(store.settings()))?;
    check_update(&settings).await
}
