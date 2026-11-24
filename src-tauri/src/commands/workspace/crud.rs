use super::*;

#[tauri::command]
pub async fn list_workspaces(app: AppHandle) -> AppResult<Vec<WorkspaceProfile>> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>()
            .with_data(|store| Ok(store.list().to_vec()))
    })
    .await
    .map_err(|err| AppError::Message(format!("读取工作区列表失败: {err}")))?
}

#[tauri::command]
pub async fn create_workspace(
    app: AppHandle,
    path: String,
    name: Option<String>,
) -> AppResult<WorkspaceProfile> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>().with_data(|store| {
            let profile = WorkspaceProfile::new(path, name);
            store.add(profile.clone())?;
            Ok(profile)
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("创建工作区失败: {err}")))?
}

#[tauri::command]
pub async fn update_workspace(app: AppHandle, profile: WorkspaceProfile) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>().with_data(|store| {
            if store.get(&profile.id).is_none() {
                return Err(AppError::Message(format!(
                    "workspace not found: {}",
                    profile.id
                )));
            }
            store.update(profile)
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("更新工作区失败: {err}")))?
}

#[tauri::command]
pub async fn open_workspace_directory(path: String) -> AppResult<()> {
    let path = PathBuf::from(path.trim());
    tauri::async_runtime::spawn_blocking(move || open_path_in_file_manager(&path))
        .await
        .map_err(|err| AppError::Message(format!("打开工作区目录失败: {err}")))?
}

#[tauri::command]
pub async fn delete_workspace(app: AppHandle, id: String) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let raw_path = state.workspace_path(&id)?;
        let workspace_path = raw_path.canonicalize().unwrap_or(raw_path);
        crate::tools::session::kill_workspace_sessions(&workspace_path);
        crate::tools::session::drop_workspace_session_store(&workspace_path);
        state.gateway.drop_workspace(&id);
        // Single atomic write: `DataStore::remove` deletes both the profile and its secrets.
        state.with_data(|store| {
            let _ = store.remove(&id)?;
            Ok(())
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("删除工作区失败: {err}")))?
}
