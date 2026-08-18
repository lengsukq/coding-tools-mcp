use tauri::State;

use crate::app_state::AppState;
use crate::error::{AppError, AppResult};
use crate::runtime::ServiceKind;
use crate::usage::ServiceUsageStats;

#[tauri::command]
pub fn get_service_usage_stats(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<Vec<ServiceUsageStats>> {
    let exists = state.with_workspaces(|store| Ok(store.get(&id).is_some()))?;
    if !exists {
        return Err(AppError::Message(format!("workspace not found: {id}")));
    }

    state.with_runtime(|runtime| {
        Ok(vec![
            runtime.usage_stats(&id, ServiceKind::Mcp),
            runtime.usage_stats(&id, ServiceKind::Actions),
        ])
    })
}
