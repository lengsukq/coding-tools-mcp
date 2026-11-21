use std::path::Path;

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

#[derive(Debug, Default, PartialEq, Eq)]
struct ParsedGitStatus {
    branch: Option<String>,
    ahead: usize,
    behind: usize,
    changed_files: usize,
    is_initial: bool,
}

#[tauri::command]
pub async fn get_workspace_git_summary(
    state: State<'_, AppState>,
    workspace_id: String,
) -> AppResult<WorkspaceGitSummary> {
    let root = state.workspace_path(&workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || compute_workspace_git_summary(root))
        .await
        .map_err(|err| AppError::Message(format!("读取 Git 状态失败: {err}")))
}

fn run_git(cwd: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let output = background_command("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    output.status.success().then_some(output.stdout)
}

fn run_git_trimmed(cwd: &Path, args: &[&str]) -> Option<String> {
    let bytes = run_git(cwd, args)?;
    Some(String::from_utf8_lossy(&bytes).trim().to_owned())
}

fn is_excluded_git_path(path: &str, ignored_prefixes: &[String]) -> bool {
    let normalized = path.replace('\\', "/");
    let trimmed = normalized.trim_end_matches('/');
    if trimmed.split('/').any(|part| {
        matches!(
            part,
            ".git"
                | ".coding-tools"
                | ".local-build"
                | ".scratch"
                | "node_modules"
                | "target"
                | "build"
                | "dist"
                | ".venv"
        )
    }) {
        return true;
    }
    ignored_prefixes.iter().any(|prefix| {
        trimmed == prefix
            || trimmed
                .strip_prefix(prefix.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// Parse `git status --porcelain=v2 --branch -z --untracked-files=all` output
/// in a single pass, replacing separate `branch`, `rev-list`, `diff`, and `ls-files` calls.
fn parse_porcelain_v2_status(stdout: &[u8], ignored_prefixes: &[String]) -> ParsedGitStatus {
    let mut parsed = ParsedGitStatus::default();
    let tokens: Vec<&[u8]> = stdout
        .split(|&byte| byte == 0)
        .filter(|token| !token.is_empty())
        .collect();

    let mut i = 0;
    while i < tokens.len() {
        let entry = String::from_utf8_lossy(tokens[i]);
        i += 1;

        if let Some(rest) = entry.strip_prefix("# ") {
            if let Some(oid) = rest.strip_prefix("branch.oid ") {
                parsed.is_initial = oid.trim() == "(initial)";
            } else if let Some(head) = rest.strip_prefix("branch.head ") {
                let head = head.trim();
                if !head.is_empty() && head != "(detached)" {
                    parsed.branch = Some(head.to_string());
                }
            } else if let Some(ab) = rest.strip_prefix("branch.ab ") {
                let mut parts = ab.split_whitespace();
                if let (Some(ahead), Some(behind)) = (parts.next(), parts.next()) {
                    parsed.ahead = ahead.trim_start_matches('+').parse::<usize>().unwrap_or(0);
                    parsed.behind = behind.trim_start_matches('-').parse::<usize>().unwrap_or(0);
                }
            }
            continue;
        }

        if let Some(path) = entry.strip_prefix("? ") {
            if !is_excluded_git_path(path, ignored_prefixes) {
                parsed.changed_files += 1;
            }
        } else if entry.starts_with("1 ") {
            if let Some(path) = entry.splitn(9, ' ').nth(8) {
                if !is_excluded_git_path(path, ignored_prefixes) {
                    parsed.changed_files += 1;
                }
            }
        } else if entry.starts_with("2 ") {
            let new_path = entry.splitn(10, ' ').nth(9);
            let orig_path = tokens.get(i).map(|raw| String::from_utf8_lossy(raw));
            if i < tokens.len() {
                i += 1;
            }
            if let Some(new_path) = new_path {
                let orig_excluded = orig_path
                    .as_deref()
                    .is_some_and(|orig| is_excluded_git_path(orig, ignored_prefixes));
                if !orig_excluded && !is_excluded_git_path(new_path, ignored_prefixes) {
                    parsed.changed_files += 1;
                }
            }
        } else if entry.starts_with("u ") {
            if let Some(path) = entry.splitn(11, ' ').nth(10) {
                if !is_excluded_git_path(path, ignored_prefixes) {
                    parsed.changed_files += 1;
                }
            }
        }
    }

    parsed
}

fn inspect_repository_status(
    cwd: &Path,
    ignored_prefixes: &[String],
) -> (ParsedGitStatus, Option<String>) {
    let status = run_git(
        cwd,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=all",
        ],
    )
    .map(|bytes| parse_porcelain_v2_status(&bytes, ignored_prefixes))
    .unwrap_or_default();

    let last_commit = if status.is_initial {
        None
    } else {
        run_git_trimmed(cwd, &["log", "-1", "--pretty=%h · %s"])
    };
    (status, last_commit)
}

fn compute_workspace_git_summary(root: PathBuf) -> WorkspaceGitSummary {
    let sub_repositories = crate::review::direct_git_repositories(&root);
    let sub_prefixes = sub_repositories
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    let sub_repositories = sub_repositories
        .into_iter()
        .map(|(name, path)| {
            let (status, last_commit) = inspect_repository_status(&path, &[]);
            WorkspaceSubGitSummary {
                name,
                path: path.to_string_lossy().to_string(),
                branch: status.branch,
                changed_files: status.changed_files,
                ahead: status.ahead,
                behind: status.behind,
                last_commit,
            }
        })
        .collect::<Vec<_>>();

    let root_is_git = run_git_trimmed(&root, &["rev-parse", "--show-toplevel"])
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

    let (status, last_commit) = inspect_repository_status(&root, &sub_prefixes);
    WorkspaceGitSummary {
        available: true,
        branch: status.branch,
        changed_files: status.changed_files,
        ahead: status.ahead,
        behind: status.behind,
        last_commit,
        sub_repositories,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_porcelain_v2_extracts_branch_ab_and_counts_entries_with_exclusions() {
        let payload = concat!(
            "# branch.oid 0123456789abcdef\0",
            "# branch.head main\0",
            "# branch.upstream origin/main\0",
            "# branch.ab +3 -2\0",
            "1 .M N... 100644 100644 100644 aaa bbb src/lib.rs\0",
            "2 R. N... 100644 100644 100644 aaa bbb R100 src/new name.rs\0src/old name.rs\0",
            "? docs/new.md\0",
            "? node_modules/pkg/index.js\0",
            "? subrepo/file.rs\0",
            "u UU N... 100644 100644 100644 100644 a b c conflict.rs\0",
        );

        let parsed = parse_porcelain_v2_status(payload.as_bytes(), &["subrepo".to_string()]);
        assert_eq!(parsed.branch.as_deref(), Some("main"));
        assert_eq!(parsed.ahead, 3);
        assert_eq!(parsed.behind, 2);
        assert_eq!(parsed.changed_files, 4);
        assert!(!parsed.is_initial);
    }

    #[test]
    fn parse_porcelain_v2_handles_detached_and_initial_branch() {
        let detached = b"# branch.oid abc\0# branch.head (detached)\0";
        let parsed = parse_porcelain_v2_status(detached, &[]);
        assert_eq!(parsed.branch, None);
        assert!(!parsed.is_initial);

        let initial = b"# branch.oid (initial)\0# branch.head main\0? README.md\0";
        let parsed = parse_porcelain_v2_status(initial, &[]);
        assert_eq!(parsed.branch.as_deref(), Some("main"));
        assert!(parsed.is_initial);
        assert_eq!(parsed.changed_files, 1);
    }
}
