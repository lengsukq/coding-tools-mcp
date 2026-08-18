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
    #[serde(default)]
    pub allow_lan_access: bool,
    #[serde(default)]
    pub restore_runtime_state_on_launch: bool,
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
            allow_lan_access: settings.allow_lan_access,
            restore_runtime_state_on_launch: settings.restore_runtime_state_on_launch,
        })
    })
}

#[tauri::command]
pub fn set_global_runtime_settings(
    state: State<'_, AppState>,
    runtime: GlobalRuntimeSettingsDto,
) -> AppResult<()> {
    let should_capture_running = state.with_settings(|store| {
        Ok(runtime.restore_runtime_state_on_launch && !store.settings().restore_runtime_state_on_launch)
    })?;
    let running_snapshot = if should_capture_running {
        Some(state.with_runtime(|supervisor| {
            Ok((
                supervisor.running_workspace_ids(crate::runtime::ServiceKind::Mcp),
                supervisor.running_workspace_ids(crate::runtime::ServiceKind::Actions),
            ))
        })?)
    } else {
        None
    };

    state.with_settings(|store| {
        let mut settings = store.settings();
        settings.global_executable_paths = runtime.executable_paths.trim().to_string();
        settings.global_ai_instructions = runtime.ai_instructions.trim().to_string();
        settings.global_instruction_sources = runtime.instruction_sources;
        settings.global_skill_sources = runtime.skill_sources;
        settings.global_custom_instruction_paths = runtime.custom_instruction_paths.trim().to_string();
        settings.global_custom_skill_paths = runtime.custom_skill_paths.trim().to_string();
        settings.allow_lan_access = runtime.allow_lan_access;
        settings.restore_runtime_state_on_launch = runtime.restore_runtime_state_on_launch;
        if let Some((mcp_ids, actions_ids)) = &running_snapshot {
            settings.restore_mcp_workspace_ids = mcp_ids.clone();
            settings.restore_actions_workspace_ids = actions_ids.clone();
        }
        store.update_settings(settings)
    })
}
