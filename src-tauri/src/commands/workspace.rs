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

#[tauri::command]
pub fn issue_workspace_review_url(state: State<'_, AppState>, workspace_id: String, review_id: String) -> AppResult<String> {
    let profile = state.with_workspaces(|store| store.get(&workspace_id).cloned().ok_or_else(|| AppError::Message(format!("workspace not found: {workspace_id}"))))?;
    let link = crate::review::issue_token(&PathBuf::from(profile.path), &review_id).map_err(AppError::Message)?;
    let settings = crate::settings::AppSettings::load_or_default();
    let base = settings.global_gateway.public_url.trim().trim_end_matches('/');
    if base.is_empty() { return Err(AppError::Message("请先配置 Global Gateway Public URL".into())); }
    Ok(format!("{base}/review/{}?t={}", link.change_id, link.token))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewLaunch {
    change_id: String,
    url: String,
    scope: String,
    files: usize,
    additions: usize,
    deletions: usize,
}

fn review_launch(root: &std::path::Path, link: crate::review::ReviewLink, scope: &str) -> AppResult<ReviewLaunch> {
    let settings = crate::settings::AppSettings::load_or_default();
    let base = settings.global_gateway.public_url.trim().trim_end_matches('/');
    if base.is_empty() { return Err(AppError::Message("请先配置 Global Gateway Public URL".into())); }
    let review = crate::review::load_review(root, &link.change_id, &link.token).map_err(AppError::Message)?;
    Ok(ReviewLaunch {
        change_id: link.change_id.clone(),
        url: format!("{base}/review/{}?t={}", link.change_id, link.token),
        scope: scope.into(),
        files: review.stats.files,
        additions: review.stats.additions,
        deletions: review.stats.deletions,
    })
}

#[tauri::command]
pub fn create_workspace_review_url(state: State<'_, AppState>, workspace_id: String) -> AppResult<Option<ReviewLaunch>> {
    let profile = state.with_workspaces(|store| store.get(&workspace_id).cloned()
        .ok_or_else(|| AppError::Message(format!("workspace not found: {workspace_id}"))))?;
    let root = PathBuf::from(profile.path);
    let Some(link) = crate::review::create_workspace_review(&root, "Workspace changes vs HEAD").map_err(AppError::Message)? else {
        return Ok(None);
    };
    review_launch(&root, link, "workspace").map(Some)
}

#[tauri::command]
pub fn create_session_review_url(
    state: State<'_, AppState>,
    workspace_id: String,
    session_id: String,
) -> AppResult<Option<ReviewLaunch>> {
    let profile = state.with_workspaces(|store| store.get(&workspace_id).cloned()
        .ok_or_else(|| AppError::Message(format!("workspace not found: {workspace_id}"))))?;
    let root = PathBuf::from(profile.path);
    let Some(link) = crate::review::create_session_review(&root, &session_id, "Session aggregate changes").map_err(AppError::Message)? else {
        return Ok(None);
    };
    review_launch(&root, link, "session").map(Some)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceReviewSummary {
    id: String,
    created_at: u64,
    expires_at: u64,
    summary: String,
    files: usize,
    additions: usize,
    deletions: usize,
    operation_ids: Vec<String>,
    storage_bytes: u64,
    session_id: Option<String>,
    scope: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceActivityEvent {
    created_at: u64,
    operations: usize,
    files: usize,
    additions: usize,
    deletions: usize,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceActivityMetrics {
    events: Vec<WorkspaceActivityEvent>,
    total_operations: usize,
    total_files: usize,
    total_additions: usize,
    total_deletions: usize,
}

#[tauri::command]
pub fn get_workspace_activity_metrics(
    state: State<'_, AppState>,
    workspace_id: String,
) -> AppResult<WorkspaceActivityMetrics> {
    let profile = state.with_workspaces(|store| store.get(&workspace_id).cloned()
        .ok_or_else(|| AppError::Message(format!("workspace not found: {workspace_id}"))))?;
    let root = PathBuf::from(profile.path);
    crate::review::cleanup_expired(&root);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let since = now.saturating_sub(8 * 24 * 60 * 60);
    let mut events = crate::review::list_reviews(&root).into_iter()
        .filter(|review| review.scope == "operation" && review.created_at >= since)
        .map(|review| WorkspaceActivityEvent {
            created_at: review.created_at,
            operations: review.operation_ids.len().max(1),
            files: review.stats.files,
            additions: review.stats.additions,
            deletions: review.stats.deletions,
        })
        .collect::<Vec<_>>();
    events.sort_by_key(|event| event.created_at);
    Ok(WorkspaceActivityMetrics {
        total_operations: events.iter().map(|event| event.operations).sum(),
        total_files: events.iter().map(|event| event.files).sum(),
        total_additions: events.iter().map(|event| event.additions).sum(),
        total_deletions: events.iter().map(|event| event.deletions).sum(),
        events,
    })
}

#[tauri::command]
pub fn list_workspace_reviews(state: State<'_, AppState>, workspace_id: String) -> AppResult<Vec<WorkspaceReviewSummary>> {
    let profile = state.with_workspaces(|store| store.get(&workspace_id).cloned()
        .ok_or_else(|| AppError::Message(format!("workspace not found: {workspace_id}"))))?;
    let root = PathBuf::from(profile.path);
    crate::review::cleanup_expired(&root);
    Ok(crate::review::list_reviews(&root).into_iter().take(20).map(|review| WorkspaceReviewSummary {
        id: review.id, created_at: review.created_at, expires_at: review.expires_at, summary: review.summary,
        files: review.stats.files, additions: review.stats.additions, deletions: review.stats.deletions,
        operation_ids: review.operation_ids,
        storage_bytes: crate::review::storage_bytes(&root),
        session_id: review.session_id,
        scope: review.scope,
    }).collect())
}

#[tauri::command]
pub fn delete_workspace_review(state: State<'_, AppState>, workspace_id: String, review_id: String) -> AppResult<()> {
    let profile = state.with_workspaces(|store| store.get(&workspace_id).cloned()
        .ok_or_else(|| AppError::Message(format!("workspace not found: {workspace_id}"))))?;
    crate::review::delete_review(&PathBuf::from(profile.path), &review_id).map_err(AppError::Message)
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
pub struct WorkspaceSubGitSummary {
    name: String,
    path: String,
    branch: Option<String>,
    changed_files: usize,
    ahead: usize,
    behind: usize,
    last_commit: Option<String>,
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
    sub_repositories: Vec<WorkspaceSubGitSummary>,
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
    let git_at = |cwd: &std::path::Path, args: &[&str]| -> Option<String> {
        let output = Command::new("git").args(args).current_dir(cwd).output().ok()?;
        output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let sync_counts = |cwd: &std::path::Path| {
        git_at(cwd, &["rev-list", "--left-right", "--count", "@{upstream}...HEAD"])
        .and_then(|value| {
            let mut parts = value.split_whitespace();
            Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
        })
        .unwrap_or((0, 0))
    };
    let sub_repositories = crate::review::direct_git_repositories(&root);
    let sub_prefixes = sub_repositories.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>();
    let sub_repositories = sub_repositories.into_iter().map(|(name, path)| {
        let (behind, ahead) = sync_counts(&path);
        WorkspaceSubGitSummary {
            name,
            path: path.to_string_lossy().to_string(),
            branch: git_at(&path, &["branch", "--show-current"]).filter(|value| !value.is_empty()),
            changed_files: crate::review::git_changed_file_count(&path, &[]),
            ahead,
            behind,
            last_commit: git_at(&path, &["log", "-1", "--pretty=%h · %s"]),
        }
    }).collect::<Vec<_>>();

    let root_is_git = git_at(&root, &["rev-parse", "--show-toplevel"])
        .and_then(|path| PathBuf::from(path).canonicalize().ok())
        .zip(root.canonicalize().ok())
        .is_some_and(|(reported, actual)| reported == actual);
    if !root_is_git {
        return Ok(WorkspaceGitSummary {
            available: false,
            branch: None,
            changed_files: 0,
            ahead: 0,
            behind: 0,
            last_commit: None,
            sub_repositories,
        });
    }
    let branch = git_at(&root, &["branch", "--show-current"]).filter(|value| !value.is_empty());
    let changed_files = crate::review::git_changed_file_count(&root, &sub_prefixes);
    let (behind, ahead) = sync_counts(&root);
    Ok(WorkspaceGitSummary {
        available: true,
        branch,
        changed_files,
        ahead,
        behind,
        last_commit: git_at(&root, &["log", "-1", "--pretty=%h · %s"]),
        sub_repositories,
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
