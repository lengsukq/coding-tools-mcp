use tauri::State;

use crate::app_state::AppState;
use crate::error::AppResult;
use crate::health::{run_global_health_checks as execute_health_checks, HealthItem};

#[tauri::command]
pub async fn run_global_health_checks(
    state: State<'_, AppState>,
) -> AppResult<Vec<HealthItem>> {
    let runtime = state.with_runtime(|runtime| {
        runtime.refresh_mcp();
        Ok(runtime.mcp_status())
    })?;
    let gateway = crate::global_gateway::health().await;
    Ok(execute_health_checks(&runtime, gateway))
}
