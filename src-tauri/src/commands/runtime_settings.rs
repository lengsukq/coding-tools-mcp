use serde::{Deserialize, Serialize};
use tauri::State;

use crate::app_state::AppState;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalRuntimeSettingsDto {
    #[serde(default = "crate::settings::default_global_mcp_auth_type")]
    pub auth_type: String,
    #[serde(default)]
    pub executable_paths: String,
    #[serde(default = "crate::settings::default_global_permission_mode")]
    pub permission_mode: String,
    #[serde(default = "crate::settings::default_global_allowed_commands")]
    pub allowed_commands: String,
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
    #[serde(default)]
    pub migration_notice: String,
}

#[tauri::command]
pub fn get_global_runtime_settings(
    state: State<'_, AppState>,
) -> AppResult<GlobalRuntimeSettingsDto> {
    state.with_settings(|store| {
        let settings = store.settings();
        Ok(GlobalRuntimeSettingsDto {
            auth_type: settings.global_mcp_auth_type,
            executable_paths: settings.global_executable_paths,
            permission_mode: settings.global_permission_mode,
            allowed_commands: settings.global_allowed_commands,
            ai_instructions: settings.global_ai_instructions,
            instruction_sources: settings.global_instruction_sources,
            skill_sources: settings.global_skill_sources,
            custom_instruction_paths: settings.global_custom_instruction_paths,
            custom_skill_paths: settings.global_custom_skill_paths,
            allow_lan_access: settings.allow_lan_access,
            restore_runtime_state_on_launch: settings.restore_runtime_state_on_launch,
            migration_notice: settings.global_mcp_migration_notice,
        })
    })
}

#[tauri::command]
pub fn set_global_runtime_settings(
    state: State<'_, AppState>,
    runtime: GlobalRuntimeSettingsDto,
) -> AppResult<()> {
    let auth_type = runtime.auth_type.trim();
    if !matches!(auth_type, "oauth" | "bearer" | "noauth") {
        return Err(AppError::Message(format!(
            "不支持的 Global MCP 认证模式: {auth_type}"
        )));
    }

    let should_capture_running = state.with_settings(|store| {
        Ok(runtime.restore_runtime_state_on_launch
            && !store.settings().restore_runtime_state_on_launch)
    })?;
    let running_snapshot = if should_capture_running {
        Some(state.with_runtime(|supervisor| Ok(supervisor.is_running()))?)
    } else {
        None
    };

    state.with_settings(|store| {
        let mut settings = store.settings();
        settings.global_mcp_auth_type = auth_type.to_string();
        settings.global_executable_paths = runtime.executable_paths.trim().to_string();
        settings.global_permission_mode = runtime.permission_mode.trim().to_string();
        settings.global_allowed_commands = runtime.allowed_commands.trim().to_string();
        settings.global_ai_instructions = runtime.ai_instructions.trim().to_string();
        settings.global_instruction_sources = runtime.instruction_sources;
        settings.global_skill_sources = runtime.skill_sources;
        settings.global_custom_instruction_paths =
            runtime.custom_instruction_paths.trim().to_string();
        settings.global_custom_skill_paths = runtime.custom_skill_paths.trim().to_string();
        settings.allow_lan_access = runtime.allow_lan_access;
        settings.restore_runtime_state_on_launch = runtime.restore_runtime_state_on_launch;
        settings.global_mcp_migration_notice.clear();
        if let Some(running) = running_snapshot {
            settings.global_mcp_was_running = running;
            settings.restore_mcp_workspace_ids.clear();
        }
        store.update_settings(settings)
    })
}
