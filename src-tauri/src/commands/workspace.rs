use std::path::PathBuf;
use std::process::Command;

use tauri::State;

use crate::app_state::{teardown_workspace, AppState};
use crate::error::{AppError, AppResult};
use crate::platform::open_path_in_file_manager;
use crate::workspace::WorkspaceProfile;

#[tauri::command]
pub fn list_workspaces(state: State<'_, AppState>) -> AppResult<Vec<WorkspaceProfile>> {
    state.with_workspaces(|store| Ok(store.list().to_vec()))
}

#[derive(Clone, serde::Serialize)]
pub struct DetectedIde {
    id: &'static str,
    name: &'static str,
}

struct IdeDefinition {
    id: &'static str,
    name: &'static str,
    mac_app: &'static str,
    cli: &'static str,
}

const IDE_DEFINITIONS: &[IdeDefinition] = &[
    IdeDefinition { id: "vscode", name: "Visual Studio Code", mac_app: "Visual Studio Code", cli: "code" },
    IdeDefinition { id: "cursor", name: "Cursor", mac_app: "Cursor", cli: "cursor" },
    IdeDefinition { id: "windsurf", name: "Windsurf", mac_app: "Windsurf", cli: "windsurf" },
    IdeDefinition { id: "webstorm", name: "WebStorm", mac_app: "WebStorm", cli: "webstorm" },
    IdeDefinition { id: "idea", name: "IntelliJ IDEA", mac_app: "IntelliJ IDEA", cli: "idea" },
    IdeDefinition { id: "pycharm", name: "PyCharm", mac_app: "PyCharm", cli: "pycharm" },
    IdeDefinition { id: "android-studio", name: "Android Studio", mac_app: "Android Studio", cli: "studio" },
    IdeDefinition { id: "zed", name: "Zed", mac_app: "Zed", cli: "zed" },
];

fn command_exists(command: &str) -> bool {
    #[cfg(target_os = "windows")]
    let probe = Command::new("where").arg(command).output();
    #[cfg(not(target_os = "windows"))]
    let probe = Command::new("sh").args(["-lc", &format!("command -v {command}")]).output();
    probe.map(|output| output.status.success()).unwrap_or(false)
}

fn ide_installed(ide: &IdeDefinition) -> bool {
    #[cfg(target_os = "macos")]
    {
        let app = format!("{}.app", ide.mac_app);
        if PathBuf::from("/Applications").join(&app).exists()
            || dirs::home_dir().is_some_and(|home| home.join("Applications").join(&app).exists())
        { return true; }
    }
    command_exists(ide.cli)
}

#[tauri::command]
pub fn detect_installed_ides() -> Vec<DetectedIde> {
    IDE_DEFINITIONS.iter().filter(|ide| ide_installed(ide))
        .map(|ide| DetectedIde { id: ide.id, name: ide.name }).collect()
}

#[tauri::command]
pub fn open_workspace_in_ide(
    state: State<'_, AppState>,
    workspace_id: String,
    ide_id: String,
) -> AppResult<()> {
    let profile = state.with_workspaces(|store| {
        store.get(&workspace_id).cloned().ok_or_else(|| AppError::Message(format!("workspace not found: {workspace_id}")))
    })?;
    let root = PathBuf::from(profile.path);
    if !root.is_dir() { return Err(AppError::Message(format!("工作区目录不存在: {}", root.display()))); }
    let ide = IDE_DEFINITIONS.iter().find(|ide| ide.id == ide_id)
        .ok_or_else(|| AppError::Message(format!("不支持的 IDE: {ide_id}")))?;
    if !ide_installed(ide) { return Err(AppError::Message(format!("未检测到 {}", ide.name))); }

    #[cfg(target_os = "macos")]
    if PathBuf::from("/Applications").join(format!("{}.app", ide.mac_app)).exists()
        || dirs::home_dir().is_some_and(|home| home.join("Applications").join(format!("{}.app", ide.mac_app)).exists())
    {
        return Command::new("open").args(["-a", ide.mac_app]).arg(&root).spawn()
            .map(|_| ()).map_err(|err| AppError::Message(format!("无法使用 {} 打开工作区: {err}", ide.name)));
    }
    Command::new(ide.cli).arg(&root).spawn()
        .map(|_| ()).map_err(|err| AppError::Message(format!("无法使用 {} 打开工作区: {err}", ide.name)))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitSummary {
    available: bool,
    branch: Option<String>,
    changed_files: usize,
    ahead: usize,
    behind: usize,
    last_commit: Option<String>,
}

#[tauri::command]
pub fn get_workspace_git_summary(
    state: State<'_, AppState>,
    workspace_id: String,
) -> AppResult<WorkspaceGitSummary> {
    let profile = state.with_workspaces(|store| {
        store.get(&workspace_id).cloned().ok_or_else(|| {
            AppError::Message(format!("workspace not found: {workspace_id}"))
        })
    })?;
    let root = PathBuf::from(profile.path);
    let git = |args: &[&str]| -> Option<String> {
        let output = Command::new("git").args(args).current_dir(&root).output().ok()?;
        output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let Some(branch) = git(&["branch", "--show-current"]) else {
        return Ok(WorkspaceGitSummary {
            available: false, branch: None, changed_files: 0, ahead: 0, behind: 0, last_commit: None,
        });
    };
    let changed_files = git(&["status", "--porcelain"])
        .map(|value| value.lines().count())
        .unwrap_or(0);
    let (behind, ahead) = git(&["rev-list", "--left-right", "--count", "@{upstream}...HEAD"])
        .and_then(|value| {
            let mut parts = value.split_whitespace();
            Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
        })
        .unwrap_or((0, 0));
    Ok(WorkspaceGitSummary {
        available: true,
        branch: Some(branch),
        changed_files,
        ahead,
        behind,
        last_commit: git(&["log", "-1", "--pretty=%h · %s"]),
    })
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
