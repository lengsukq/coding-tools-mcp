use tauri::State;

use crate::agent_context::{
    discover, merge_source_lists, scan_global_agent_context as discover_global_agent_context,
    AgentContextSnapshot, GlobalAgentContextScan,
};
use crate::app_state::AppState;
use crate::error::{AppError, AppResult};

#[tauri::command]
pub fn scan_agent_context(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<AgentContextSnapshot> {
    let profile = state.with_workspaces(|store| {
        store
            .get(&id)
            .cloned()
            .ok_or_else(|| AppError::Message(format!("workspace not found: {id}")))
    })?;
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
    Ok(discover(
        std::path::Path::new(&profile.path),
        &instruction_sources,
        &skill_sources,
        &instruction_paths,
        &skill_paths,
    ))
}

#[tauri::command]
pub fn scan_global_agent_context() -> GlobalAgentContextScan {
    discover_global_agent_context()
}

fn merge_text(global: &str, workspace: &str) -> String {
    [global.trim(), workspace.trim()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
