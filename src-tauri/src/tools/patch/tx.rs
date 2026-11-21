use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use uuid::Uuid;

use crate::tools::workspace::{Workspace, WorkspaceError};

use super::patch_failed;

pub(super) fn commit_staged(
    ws: &Workspace,
    staged: &HashMap<String, Option<String>>,
) -> Result<HashMap<PathBuf, Option<Vec<u8>>>, WorkspaceError> {
    let staged_bytes = staged
        .iter()
        .map(|(path, content)| {
            (
                path.clone(),
                content.as_ref().map(|value| value.as_bytes().to_vec()),
            )
        })
        .collect::<HashMap<_, _>>();
    commit_staged_bytes(ws, &staged_bytes)
}

pub(crate) fn commit_staged_bytes(
    ws: &Workspace,
    staged: &HashMap<String, Option<Vec<u8>>>,
) -> Result<HashMap<PathBuf, Option<Vec<u8>>>, WorkspaceError> {
    let mut backups: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
    let mut temporary_files: HashMap<PathBuf, PathBuf> = HashMap::new();
    let mut resolved_targets: Vec<(PathBuf, bool)> = Vec::with_capacity(staged.len());

    // Phase 1: capture lossless backups and write sibling stage files before
    // mutating any destination path. Any error here aborts cleanly without
    // touching existing workspace files.
    for (rel, content) in staged {
        ws.reject_protected_write_path(rel)?;
        let resolved = if content.is_none() {
            ws.resolve_existing(rel)?
        } else {
            ws.resolve_for_write(rel)?
        };
        let path = resolved.path;
        let backup = if path.exists() && path.is_file() {
            let bytes = fs::read(&path).map_err(|err| {
                cleanup_temporary_files(temporary_files.values());
                patch_failed(format!("Failed to read backup for {rel}: {err}"))
            })?;
            Some(bytes)
        } else {
            None
        };
        backups.insert(path.clone(), backup);

        if let Some(bytes) = content {
            if let Some(parent) = path.parent() {
                if let Err(err) = fs::create_dir_all(parent) {
                    cleanup_temporary_files(temporary_files.values());
                    return Err(patch_failed(err.to_string()));
                }
            }
            let temp = path.with_file_name(format!(
                ".{}.harness-stage-{}",
                path.file_name().and_then(|v| v.to_str()).unwrap_or("file"),
                Uuid::new_v4().simple()
            ));
            if let Err(err) = fs::write(&temp, bytes) {
                cleanup_temporary_files(temporary_files.values());
                return Err(patch_failed(format!("Failed to stage file: {err}")));
            }
            temporary_files.insert(path.clone(), temp);
        }
        resolved_targets.push((path, content.is_some()));
    }

    // Phase 2: commit staged replacements/deletions; roll back from verified backups on error.
    for (path, has_content) in resolved_targets {
        let result = if has_content {
            match temporary_files.get(&path) {
                Some(temp) => replace_file(temp, &path),
                None => Err(std::io::Error::other("Staged file is missing")),
            }
        } else if path.exists() && path.is_file() {
            fs::remove_file(&path)
        } else {
            Ok(())
        };
        if let Err(err) = result {
            cleanup_temporary_files(temporary_files.values());
            restore_backups(&backups);
            return Err(patch_failed(format!("Failed to write file: {err}")));
        }
    }

    cleanup_temporary_files(temporary_files.values());
    Ok(backups)
}

fn restore_backups(backups: &HashMap<PathBuf, Option<Vec<u8>>>) {
    for (path, data) in backups {
        match data {
            None => {
                let _ = fs::remove_file(path);
            }
            Some(bytes) => {
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(path, bytes);
            }
        }
    }
}

fn replace_file(temp: &PathBuf, path: &PathBuf) -> Result<(), std::io::Error> {
    #[cfg(windows)]
    {
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    fs::rename(temp, path)
}

fn cleanup_temporary_files<'a>(paths: impl Iterator<Item = &'a PathBuf>) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}
