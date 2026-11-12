use super::*;

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
pub async fn get_workspace_git_summary(
    state: State<'_, AppState>,
    workspace_id: String,
) -> AppResult<WorkspaceGitSummary> {
    let profile = state.with_workspaces(|store| {
        store
            .get(&workspace_id)
            .cloned()
            .ok_or_else(|| AppError::Message(format!("workspace not found: {workspace_id}")))
    })?;
    let root = PathBuf::from(profile.path);
    tauri::async_runtime::spawn_blocking(move || compute_workspace_git_summary(root))
        .await
        .map_err(|err| AppError::Message(format!("读取 Git 状态失败: {err}")))
}

fn compute_workspace_git_summary(root: PathBuf) -> WorkspaceGitSummary {
    let git_at = |cwd: &std::path::Path, args: &[&str]| -> Option<String> {
        let output = background_command("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let sync_counts = |cwd: &std::path::Path| {
        git_at(
            cwd,
            &["rev-list", "--left-right", "--count", "@{upstream}...HEAD"],
        )
        .and_then(|value| {
            let mut parts = value.split_whitespace();
            Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
        })
        .unwrap_or((0, 0))
    };
    let sub_repositories = crate::review::direct_git_repositories(&root);
    let sub_prefixes = sub_repositories
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    let sub_repositories = sub_repositories
        .into_iter()
        .map(|(name, path)| {
            let (behind, ahead) = sync_counts(&path);
            WorkspaceSubGitSummary {
                name,
                path: path.to_string_lossy().to_string(),
                branch: git_at(&path, &["branch", "--show-current"])
                    .filter(|value| !value.is_empty()),
                changed_files: crate::review::git_changed_file_count(&path, &[]),
                ahead,
                behind,
                last_commit: git_at(&path, &["log", "-1", "--pretty=%h · %s"]),
            }
        })
        .collect::<Vec<_>>();

    let root_is_git = git_at(&root, &["rev-parse", "--show-toplevel"])
        .and_then(|path| PathBuf::from(path).canonicalize().ok())
        .zip(root.canonicalize().ok())
        .is_some_and(|(reported, actual)| reported == actual);
    if !root_is_git {
        return WorkspaceGitSummary {
            available: false,
            branch: None,
            changed_files: 0,
            ahead: 0,
            behind: 0,
            last_commit: None,
            sub_repositories,
        };
    }
    let branch = git_at(&root, &["branch", "--show-current"]).filter(|value| !value.is_empty());
    let changed_files = crate::review::git_changed_file_count(&root, &sub_prefixes);
    let (behind, ahead) = sync_counts(&root);
    WorkspaceGitSummary {
        available: true,
        branch,
        changed_files,
        ahead,
        behind,
        last_commit: git_at(&root, &["log", "-1", "--pretty=%h · %s"]),
        sub_repositories,
    }
}
