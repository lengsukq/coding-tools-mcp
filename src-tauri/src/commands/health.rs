use tauri::{AppHandle, Manager};

use crate::app_state::AppState;
use crate::error::{AppError, AppResult};
use crate::health::{run_global_health_checks as execute_health_checks, HealthItem};

#[tauri::command]
pub async fn run_global_health_checks(app: AppHandle) -> AppResult<Vec<HealthItem>> {
    let runtime = tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>().with_runtime(|runtime| {
            runtime.refresh_mcp();
            Ok(runtime.mcp_status())
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("查询运行态健康检查失败: {err}")))??;
    let gateway = crate::global_gateway::health().await;
    Ok(execute_health_checks(&runtime, gateway))
}
