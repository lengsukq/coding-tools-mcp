use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalRuntimeSettingsDto {
    #[serde(default = "crate::settings::default_global_mcp_auth_type")]
    pub auth_type: String,
    #[serde(default)]
    pub mcp_server_name: String,
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
    pub disabled_instruction_ids: Vec<String>,
    #[serde(default)]
    pub disabled_skill_ids: Vec<String>,
    #[serde(default)]
    pub allow_lan_access: bool,
    #[serde(default = "crate::settings::default_allow_local_noauth")]
    pub allow_local_noauth: bool,
    #[serde(default)]
    pub restore_runtime_state_on_launch: bool,
    #[serde(default)]
    pub launch_at_login: bool,
    #[serde(default)]
    pub launch_in_lightweight_mode: bool,
    #[serde(default)]
    pub migration_notice: String,
}

#[tauri::command]
pub async fn get_global_runtime_settings(app: AppHandle) -> AppResult<GlobalRuntimeSettingsDto> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>().with_settings(|store| {
            let settings = store.settings();
            let launch_at_login =
                settings.launch_at_login && crate::platform::autostart::is_enabled();
            Ok(GlobalRuntimeSettingsDto {
                auth_type: settings.global_mcp_auth_type,
                mcp_server_name: settings.global_mcp_server_name,
                executable_paths: settings.global_executable_paths,
                permission_mode: settings.global_permission_mode,
                allowed_commands: settings.global_allowed_commands,
                ai_instructions: settings.global_ai_instructions,
                instruction_sources: settings.global_instruction_sources,
                skill_sources: settings.global_skill_sources,
                custom_instruction_paths: settings.global_custom_instruction_paths,
                custom_skill_paths: settings.global_custom_skill_paths,
                disabled_instruction_ids: settings.global_disabled_instruction_ids,
                disabled_skill_ids: settings.global_disabled_skill_ids,
                allow_lan_access: settings.allow_lan_access,
                allow_local_noauth: settings.allow_local_noauth,
                restore_runtime_state_on_launch: settings.restore_runtime_state_on_launch,
                launch_at_login,
                launch_in_lightweight_mode: settings.launch_in_lightweight_mode,
                migration_notice: settings.global_mcp_migration_notice,
            })
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("读取全局运行设置失败: {err}")))?
}

#[tauri::command]
pub async fn set_global_runtime_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    runtime: GlobalRuntimeSettingsDto,
) -> AppResult<()> {
    let auth_type = runtime.auth_type.trim().to_string();
    if !matches!(auth_type.as_str(), "oauth" | "bearer" | "noauth") {
        return Err(AppError::Message(format!(
            "不支持的 Global MCP 认证模式: {auth_type}"
        )));
    }
    let mcp_server_name = runtime.mcp_server_name.trim().to_string();
    if mcp_server_name.chars().count() > 80 {
        return Err(AppError::Message(
            "MCP 服务名称不能超过 80 个字符".to_string(),
        ));
    }
    if mcp_server_name.chars().any(char::is_control) {
        return Err(AppError::Message(
            "MCP 服务名称不能包含换行或控制字符".to_string(),
        ));
    }

    let app_for_save = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_for_save.state::<AppState>();
        let (should_capture_running, previous_launch_at_login) = state.with_settings(|store| {
            let current = store.settings();
            Ok((
                runtime.restore_runtime_state_on_launch && !current.restore_runtime_state_on_launch,
                current.launch_at_login,
            ))
        })?;
        let should_sync_autostart = runtime.launch_at_login != previous_launch_at_login
            || (runtime.launch_at_login && !crate::platform::autostart::is_enabled());
        if should_sync_autostart {
            crate::platform::autostart::set_enabled(runtime.launch_at_login)?;
        }
        let running_snapshot = if should_capture_running {
            Some(state.with_runtime(|supervisor| Ok(supervisor.is_running()))?)
        } else {
            None
        };

        state.with_settings(|store| {
            let mut settings = store.settings();
            settings.global_mcp_auth_type = auth_type;
            settings.global_mcp_server_name = mcp_server_name;
            settings.global_executable_paths = runtime.executable_paths.trim().to_string();
            settings.global_permission_mode = runtime.permission_mode.trim().to_string();
            settings.global_allowed_commands = runtime.allowed_commands.trim().to_string();
            settings.global_ai_instructions = runtime.ai_instructions.trim().to_string();
            settings.global_instruction_sources = runtime.instruction_sources;
            settings.global_skill_sources = runtime.skill_sources;
            settings.global_custom_instruction_paths =
                runtime.custom_instruction_paths.trim().to_string();
            settings.global_custom_skill_paths = runtime.custom_skill_paths.trim().to_string();
            settings.global_disabled_instruction_ids =
                crate::agent_context::merge_id_lists(&runtime.disabled_instruction_ids, &[]);
            settings.global_disabled_skill_ids =
                crate::agent_context::merge_id_lists(&runtime.disabled_skill_ids, &[]);
            settings.allow_lan_access = runtime.allow_lan_access;
            settings.allow_local_noauth = runtime.allow_local_noauth;
            settings.restore_runtime_state_on_launch = runtime.restore_runtime_state_on_launch;
            settings.launch_at_login = runtime.launch_at_login;
            settings.launch_in_lightweight_mode = runtime.launch_in_lightweight_mode;
            settings.global_mcp_migration_notice.clear();
            if let Some(running) = running_snapshot {
                settings.global_mcp_was_running = running;
            }
            store.update_settings(settings)
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("保存全局运行设置任务失败: {err}")))??;

    if let Err(error) = crate::commands::runtime::apply_live_runtime_changes(&state, false).await {
        eprintln!("Global MCP live reload after runtime settings save failed: {error}");
        return Err(AppError::Message(format!(
            "全局运行设置已保存，但热重载 Global MCP 失败，请手动重启服务：{error}"
        )));
    }
    Ok(())
}
