use serde::{Deserialize, Serialize};
use tauri::State;

use crate::app_state::AppState;
use crate::error::AppResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalRuntimeSettingsDto {
    #[serde(default)]
    pub executable_paths: String,
    #[serde(default)]
    pub ai_instructions: String,
}

#[tauri::command]
pub fn get_global_runtime_settings(
    state: State<'_, AppState>,
) -> AppResult<GlobalRuntimeSettingsDto> {
    state.with_settings(|store| {
        let settings = store.settings();
        Ok(GlobalRuntimeSettingsDto {
            executable_paths: settings.global_executable_paths,
            ai_instructions: settings.global_ai_instructions,
        })
    })
}

#[tauri::command]
pub fn set_global_runtime_settings(
    state: State<'_, AppState>,
    runtime: GlobalRuntimeSettingsDto,
) -> AppResult<()> {
    state.with_settings(|store| {
        let mut settings = store.settings();
        settings.global_executable_paths = runtime.executable_paths.trim().to_string();
        settings.global_ai_instructions = runtime.ai_instructions.trim().to_string();
        store.update_settings(settings)
    })
}
