use serde_json::Value;
use tauri::State;

use crate::app_state::AppState;
use crate::error::{AppError, AppResult};
use crate::tools::{history, Workspace};

#[tauri::command]
pub fn list_history_sessions(state: State<'_, AppState>, id: String) -> AppResult<Value> {
    let profile = state.with_workspaces(|store| {
        store
            .get(&id)
            .cloned()
            .ok_or_else(|| AppError::Message(format!("workspace not found: {id}")))
    })?;
    let workspace = Workspace::new(profile.path.into())
        .map_err(|error| AppError::Message(error.message()))?;
    history::list_sessions_for_workspace(&workspace)
        .map_err(|error| AppError::Message(error.message()))
}
