use std::fs;
use std::path::{Path, PathBuf};

use crate::platform::background_command;

use super::diff::review_path_excluded;

pub fn direct_git_repositories(workspace_root: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(workspace_root) else {
        return Vec::new();
    };
    let mut repositories = entries
        .flatten()
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_dir() {
                return None;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if review_path_excluded(&name) {
                return None;
            }
            let path = entry.path();
            is_git_repository_root(&path).then_some((name, path))
        })
        .collect::<Vec<_>>();
    repositories.sort_by(|left, right| left.0.cmp(&right.0));
    repositories
}

pub fn git_changed_file_count(repo_root: &Path, ignored_prefixes: &[String]) -> usize {
    git_repo_changes(repo_root, ignored_prefixes)
        .map(|changes| changes.len())
        .unwrap_or(0)
}

pub(super) fn git_repo_changes(
    repo_root: &Path,
    ignored_prefixes: &[String],
) -> Result<Vec<(String, String, String)>, String> {
    let output = background_command("git")
        .args(["diff", "--name-status", "-z", "-M", "HEAD", "--", "."])
        .current_dir(repo_root)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let fields = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let mut changes = Vec::<(String, String, String)>::new();
    let mut i = 0;
    while i < fields.len() {
        let status = String::from_utf8_lossy(fields[i]).to_string();
        i += 1;
        if status.starts_with('R') || status.starts_with('C') {
            if i + 1 >= fields.len() {
                break;
            }
            let old = String::from_utf8_lossy(fields[i]).to_string();
            let new = String::from_utf8_lossy(fields[i + 1]).to_string();
            i += 2;
            changes.push((status, old, new));
        } else {
            if i >= fields.len() {
                break;
            }
            let path = String::from_utf8_lossy(fields[i]).to_string();
            i += 1;
            changes.push((status, path.clone(), path));
        }
    }
    let untracked = background_command("git")
        .args(["ls-files", "--others", "--exclude-standard", "-z"])
        .current_dir(repo_root)
        .output()
        .map_err(|e| e.to_string())?;
    if untracked.status.success() {
        for raw in untracked
            .stdout
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
        {
            let path = String::from_utf8_lossy(raw).to_string();
            changes.push(("A".into(), path.clone(), path));
        }
    }
    Ok(changes
        .into_iter()
        .filter(|(_, old_path, new_path)| {
            !review_path_excluded(old_path)
                && !review_path_excluded(new_path)
                && !ignored_prefixes.iter().any(|prefix| {
                    old_path == prefix
                        || old_path.starts_with(&format!("{prefix}/"))
                        || new_path == prefix
                        || new_path.starts_with(&format!("{prefix}/"))
                })
        })
        .collect())
}

pub(super) fn prefixed_review_path(prefix: &str, path: &str) -> String {
    if prefix.is_empty() {
        path.to_string()
    } else {
        format!("{prefix}/{path}")
    }
}

pub(super) fn is_git_repository_root(path: &Path) -> bool {
    // Most first-level directories are NOT Git repositories. Avoid spawning a
    // Git process for each of them on every workspace status refresh.
    // Linked worktrees use a .git file, and bare repositories have HEAD and
    // objects directly at the root. Both should remain discoverable.
    if !(path.join(".git").exists()
        || (path.join("HEAD").is_file() && path.join("objects").is_dir()))
    {
        return false;
    }
    let output = background_command("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(path)
        .output();
    let Ok(output) = output else { return false };
    if !output.status.success() {
        return false;
    }
    let reported = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string());
    match (reported.canonicalize(), path.canonicalize()) {
        (Ok(reported), Ok(actual)) => reported == actual,
        _ => false,
    }
}

pub(super) fn git_blob_at_head(root: &Path, path: &str) -> Vec<u8> {
    background_command("git")
        .args(["show", &format!("HEAD:{path}")])
        .current_dir(root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| output.stdout)
        .unwrap_or_default()
}

pub(super) fn git_head(root: &Path) -> Option<String> {
    background_command("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
}
