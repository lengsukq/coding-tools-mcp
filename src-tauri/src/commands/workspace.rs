use std::path::PathBuf;

use tauri::State;

use crate::app_state::{teardown_workspace, AppState};
use crate::error::{AppError, AppResult};
use crate::platform::open_path_in_file_manager;
use crate::workspace::WorkspaceProfile;

#[tauri::command]
pub fn list_workspaces(state: State<'_, AppState>) -> AppResult<Vec<WorkspaceProfile>> {
    state.with_workspaces(|store| Ok(store.list().to_vec()))
}

#[tauri::command]
pub fn create_workspace(
    state: State<'_, AppState>,
    path: String,
    name: Option<String>,
) -> AppResult<WorkspaceProfile> {
    state.with_workspaces(|store| {
        let profile = WorkspaceProfile::new(path, name);
        store.add(profile.clone())?;
        Ok(profile)
    })
}

#[tauri::command]
pub fn update_workspace(state: State<'_, AppState>, profile: WorkspaceProfile) -> AppResult<()> {
    state.with_workspaces(|store| {
        if store.get(&profile.id).is_none() {
            return Err(AppError::Message(format!(
                "workspace not found: {}",
                profile.id
            )));
        }
        store.update(profile)
    })
}

#[tauri::command]
pub fn open_workspace_directory(path: String) -> AppResult<()> {
    let path = PathBuf::from(path.trim());
    open_path_in_file_manager(&path)
}

#[tauri::command]
pub fn delete_workspace(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let profile = state.with_workspaces(|store| {
        store
            .get(&id)
            .cloned()
            .ok_or_else(|| AppError::Message(format!("workspace not found: {id}")))
    })?;
    let workspace_path = PathBuf::from(&profile.path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&profile.path));
    crate::tools::session::kill_workspace_sessions(&workspace_path);
    state.with_runtime(|runtime| {
        runtime.drop_workspace(&profile.id);
        Ok(())
    })?;
    state.with_workspaces(|store| {
        if store.remove(&id)?.is_some() {
            teardown_workspace(store, &id)?;
        }
        let mut settings = store.settings();
        settings.restore_mcp_workspace_ids.clear();
        store.update_settings(settings)?;
        Ok(())
    })
}
