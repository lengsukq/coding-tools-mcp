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
    #[serde(default)]
    pub instruction_sources: Vec<String>,
    #[serde(default)]
    pub skill_sources: Vec<String>,
    #[serde(default)]
    pub custom_instruction_paths: String,
    #[serde(default)]
    pub custom_skill_paths: String,
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
            instruction_sources: settings.global_instruction_sources,
            skill_sources: settings.global_skill_sources,
            custom_instruction_paths: settings.global_custom_instruction_paths,
            custom_skill_paths: settings.global_custom_skill_paths,
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
        settings.global_instruction_sources = runtime.instruction_sources;
        settings.global_skill_sources = runtime.skill_sources;
        settings.global_custom_instruction_paths = runtime.custom_instruction_paths.trim().to_string();
        settings.global_custom_skill_paths = runtime.custom_skill_paths.trim().to_string();
        store.update_settings(settings)
    })
}
