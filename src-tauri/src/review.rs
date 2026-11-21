//! Frozen workspace, session, and operation change-review snapshots.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use uuid::Uuid;

mod diff;
mod git_scan;
mod store;
#[cfg(test)]
mod tests;

pub use diff::snapshot_text_files;
pub use git_scan::direct_git_repositories;
pub(crate) use store::review_summary;
pub use store::{
    attach_operation, attach_session, cleanup_expired, delete_review, find_review, issue_token,
    list_review_summaries, load_review, storage_bytes, ChangeFile, ChangeSet, ReviewLink,
};

use diff::change_file_from_bytes;
use git_scan::{
    git_blobs_at_head, git_head, git_repo_changes, is_git_repository_root, prefixed_review_path,
};
use store::{fresh_token, persist_change_set, workspace_key, PersistChangeSetInput};

const MAX_WORKSPACE_REVIEW_FILES: usize = 500;

#[cfg(test)]
pub fn create_snapshot_review(
    root: &Path,
    change_id: &str,
    operation_id: Option<&str>,
    summary: &str,
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> Result<Option<ReviewLink>, String> {
    create_snapshot_review_with_session(root, change_id, operation_id, None, summary, before, after)
}

pub fn create_snapshot_review_with_session(
    root: &Path,
    change_id: &str,
    operation_id: Option<&str>,
    session_id: Option<&str>,
    summary: &str,
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> Result<Option<ReviewLink>, String> {
    let paths = before
        .keys()
        .chain(after.keys())
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let affected = paths
        .into_iter()
        .filter_map(|path| {
            let old = before.get(path).map(String::as_str);
            let new = after.get(path).map(String::as_str);
            if old == new {
                return None;
            }
            let status = match (old, new) {
                (None, Some(_)) => "add",
                (Some(_), None) => "delete",
                _ => "update",
            };
            Some((
                path.to_string(),
                status.to_string(),
                old.unwrap_or_default().to_string(),
                new.unwrap_or_default().to_string(),
            ))
        })
        .collect::<Vec<_>>();
    if affected.is_empty() {
        return Ok(None);
    }
    create_patch_review_with_metadata(
        root,
        change_id,
        operation_id,
        session_id,
        summary,
        &affected,
    )
    .map(Some)
}

#[cfg(test)]
pub fn create_patch_review(
    workspace_root: &Path,
    change_id: &str,
    operation_id: Option<&str>,
    summary: &str,
    affected: &[(String, String, String, String)],
) -> Result<ReviewLink, String> {
    create_patch_review_with_metadata(
        workspace_root,
        change_id,
        operation_id,
        None,
        summary,
        affected,
    )
}

pub fn create_patch_review_with_metadata(
    workspace_root: &Path,
    change_id: &str,
    operation_id: Option<&str>,
    session_id: Option<&str>,
    summary: &str,
    affected: &[(String, String, String, String)],
) -> Result<ReviewLink, String> {
    let (token, token_hash) = fresh_token();
    let files = affected
        .iter()
        .map(|(path, status, before, after)| {
            change_file_from_bytes(path, status, before.as_bytes(), after.as_bytes())
        })
        .collect::<Vec<_>>();
    persist_change_set(PersistChangeSetInput {
        workspace_root,
        change_id,
        token_hash,
        workspace_key: workspace_key(workspace_root),
        scope: "operation",
        session_id: session_id.map(str::to_string),
        operation_ids: operation_id.into_iter().map(str::to_string).collect(),
        base_revision: git_head(workspace_root),
        summary,
        files,
    })?;
    Ok(ReviewLink {
        change_id: change_id.into(),
        token,
    })
}

pub fn create_workspace_review(
    workspace_root: &Path,
    summary: &str,
) -> Result<Option<ReviewLink>, String> {
    let mut repositories = Vec::<(String, PathBuf)>::new();
    if is_git_repository_root(workspace_root) {
        repositories.push((String::new(), workspace_root.to_path_buf()));
    }
    let nested_repositories = direct_git_repositories(workspace_root);
    let nested_repo_prefixes = nested_repositories
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    repositories.extend(nested_repositories);
    if repositories.is_empty() {
        return Err(
            "workspace diff requires a Git repository or a first-level Git repository".into(),
        );
    }

    let mut files = Vec::new();
    let mut base_revisions = Vec::new();
    for (prefix, repo_root) in repositories {
        let Some(head) = git_head(&repo_root) else {
            continue;
        };
        base_revisions.push(if prefix.is_empty() {
            head.clone()
        } else {
            format!("{prefix}:{head}")
        });
        let ignored = if repo_root == workspace_root {
            nested_repo_prefixes.as_slice()
        } else {
            &[]
        };
        let changes = git_repo_changes(&repo_root, ignored)?;
        if files.len() + changes.len() > MAX_WORKSPACE_REVIEW_FILES {
            return Err(format!(
                "workspace review has more than {MAX_WORKSPACE_REVIEW_FILES} changed files"
            ));
        }

        let head_paths = changes
            .iter()
            .filter(|(status, _, _)| status.chars().next().unwrap_or('M') != 'A')
            .map(|(_, old_path, _)| old_path.as_str())
            .collect::<Vec<_>>();
        let head_blobs = git_blobs_at_head(&repo_root, &head_paths);

        for (status, old_path, new_path) in changes {
            let code = status.chars().next().unwrap_or('M');
            let before = if code == 'A' {
                &[][..]
            } else {
                head_blobs
                    .get(old_path.as_str())
                    .map(Vec::as_slice)
                    .unwrap_or(&[])
            };
            let after = if code == 'D' {
                Vec::new()
            } else {
                fs::read(repo_root.join(&new_path)).unwrap_or_default()
            };
            let display_old = prefixed_review_path(&prefix, &old_path);
            let display_new = prefixed_review_path(&prefix, &new_path);
            let (display_path, file_status) = match code {
                'A' => (display_new, "add"),
                'D' => (display_old, "delete"),
                'R' | 'C' => (format!("{display_old} → {display_new}"), "rename"),
                _ => (display_new, "update"),
            };
            files.push(change_file_from_bytes(
                &display_path,
                file_status,
                before,
                &after,
            ));
        }
    }
    if files.is_empty() {
        return Ok(None);
    }

    let change_id = Uuid::new_v4().simple().to_string();
    let (token, token_hash) = fresh_token();
    persist_change_set(PersistChangeSetInput {
        workspace_root,
        change_id: &change_id,
        token_hash,
        workspace_key: workspace_key(workspace_root),
        scope: "workspace",
        session_id: None,
        operation_ids: Vec::new(),
        base_revision: Some(base_revisions.join(" | ")),
        summary,
        files,
    })?;
    Ok(Some(ReviewLink { change_id, token }))
}

pub fn create_session_review(
    workspace_root: &Path,
    session_id: &str,
    summary: &str,
) -> Result<Option<ReviewLink>, String> {
    let mut matching_headers = list_review_summaries(workspace_root)
        .into_iter()
        .filter(|review| {
            review.scope == "operation" && review.session_id.as_deref() == Some(session_id)
        })
        .collect::<Vec<_>>();
    matching_headers.sort_by_key(|review| review.created_at);
    if matching_headers.is_empty() {
        return Ok(None);
    }
    let reviews = matching_headers
        .into_iter()
        .filter_map(|header| store::read_change_set_by_id(workspace_root, &header.id).ok())
        .collect::<Vec<_>>();
    if reviews.is_empty() {
        return Ok(None);
    }

    let mut by_path = BTreeMap::<String, ChangeFile>::new();
    let mut operations = Vec::new();
    for review in reviews {
        operations.extend(review.operation_ids);
        for file in review.files {
            by_path
                .entry(file.path.clone())
                .and_modify(|merged| {
                    if !merged.patch.is_empty() && !file.patch.is_empty() {
                        merged.patch.push('\n');
                    }
                    merged.patch.push_str(&file.patch);
                    merged.additions += file.additions;
                    merged.deletions += file.deletions;
                    merged.redacted |= file.redacted;
                    merged.status = file.status.clone();
                })
                .or_insert(file);
        }
    }
    let change_id = Uuid::new_v4().simple().to_string();
    let (token, token_hash) = fresh_token();
    persist_change_set(PersistChangeSetInput {
        workspace_root,
        change_id: &change_id,
        token_hash,
        workspace_key: workspace_key(workspace_root),
        scope: "session",
        session_id: Some(session_id.to_string()),
        operation_ids: operations,
        base_revision: git_head(workspace_root),
        summary,
        files: by_path.into_values().collect(),
    })?;
    Ok(Some(ReviewLink { change_id, token }))
}
