use tauri::State;

use crate::app_state::AppState;
use crate::error::AppResult;
use crate::global_gateway::{self, GatewayHealthItem, GlobalGatewayStatusDto};
use crate::settings::GlobalGatewayConfig;

#[tauri::command]
pub fn get_global_gateway_config(state: State<'_, AppState>) -> AppResult<GlobalGatewayConfig> {
    state.with_settings(|store| Ok(store.settings().global_gateway))
}

#[tauri::command]
pub fn set_global_gateway_config(
    state: State<'_, AppState>,
    config: GlobalGatewayConfig,
) -> AppResult<()> {
    state.with_settings(|store| {
        let mut settings = store.settings();
        settings.global_gateway = config;
        store.update_settings(settings)
    })
}

#[tauri::command]
pub async fn start_global_gateway() -> AppResult<GlobalGatewayStatusDto> {
    global_gateway::ensure_started().await
}

#[tauri::command]
pub async fn stop_global_gateway() -> AppResult<()> {
    global_gateway::stop().await
}

#[tauri::command]
pub async fn get_global_gateway_status() -> AppResult<GlobalGatewayStatusDto> {
    Ok(global_gateway::status().await)
}

#[tauri::command]
pub async fn check_global_gateway_health() -> AppResult<Vec<GatewayHealthItem>> {
    Ok(global_gateway::health().await)
}
