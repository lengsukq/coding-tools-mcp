use tauri::State;

use crate::agent_context::{
    discover, merge_source_lists, scan_global_agent_context as discover_global_agent_context,
    AgentContextSnapshot, GlobalAgentContextScan,
};
use crate::app_state::AppState;
use crate::error::{AppError, AppResult};

#[tauri::command]
pub async fn scan_agent_context(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<AgentContextSnapshot> {
    let profile = state.workspace_profile(&id)?;
    let settings = state.with_settings(|store| Ok(store.settings()))?;
    let instruction_sources = merge_source_lists(
        &settings.global_instruction_sources,
        &profile.runtime.instruction_sources,
    );
    let skill_sources = merge_source_lists(
        &settings.global_skill_sources,
        &profile.runtime.skill_sources,
    );
    let instruction_paths = merge_text(
        &settings.global_custom_instruction_paths,
        &profile.runtime.custom_instruction_paths,
    );
    let skill_paths = merge_text(
        &settings.global_custom_skill_paths,
        &profile.runtime.custom_skill_paths,
    );
    let workspace_path = profile.path;

    tauri::async_runtime::spawn_blocking(move || {
        discover(
            std::path::Path::new(&workspace_path),
            &instruction_sources,
            &skill_sources,
            &instruction_paths,
            &skill_paths,
        )
    })
    .await
    .map_err(|err| AppError::Message(format!("扫描工作区上下文任务失败: {err}")))
}

#[tauri::command]
pub async fn scan_global_agent_context() -> AppResult<GlobalAgentContextScan> {
    tauri::async_runtime::spawn_blocking(discover_global_agent_context)
        .await
        .map_err(|err| AppError::Message(format!("扫描全局上下文任务失败: {err}")))
}

fn merge_text(global: &str, workspace: &str) -> String {
    [global.trim(), workspace.trim()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
