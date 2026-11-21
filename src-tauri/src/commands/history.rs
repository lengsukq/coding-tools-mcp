use serde_json::Value;
use tauri::State;

use crate::app_state::AppState;
use crate::error::{AppError, AppResult};
use crate::tools::{history, Workspace};

#[tauri::command]
pub async fn list_history_sessions(state: State<'_, AppState>, id: String) -> AppResult<Value> {
    let root = state.workspace_path(&id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let workspace = Workspace::new(root).map_err(|error| AppError::Message(error.message()))?;
        history::list_sessions_for_workspace(&workspace)
            .map_err(|error| AppError::Message(error.message()))
    })
    .await
    .map_err(|err| AppError::Message(format!("读取历史会话任务失败: {err}")))?
}
