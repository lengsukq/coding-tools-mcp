use tauri::State;

use crate::app_state::AppState;
use crate::error::AppResult;
use crate::usage::ServiceUsageStats;

#[tauri::command]
pub fn get_service_usage_stats(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<Vec<ServiceUsageStats>> {
    let _ = state.workspace_path(&id)?;
    state.with_runtime(|runtime| Ok(vec![runtime.usage_stats(&id)]))
}
