use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::tools::policy::PolicySettings;
use crate::tools::workspace::WorkspaceError;

pub(super) fn parse_and_resolve(
    cmd: &str,
    cwd: &Path,
    workspace_root: &Path,
    policy: &PolicySettings,
    search_path: Option<&OsStr>,
) -> Result<(String, Vec<String>), WorkspaceError> {
    let parts = shell_words::split(cmd)
        .map_err(|_| WorkspaceError::invalid_argument("Invalid command syntax"))?;
    if parts.is_empty() {
        return Err(WorkspaceError::invalid_argument("Empty command"));
    }

    let program = resolve_program(&parts[0], cwd, workspace_root, policy, search_path)?;
    Ok((program, parts[1..].to_vec()))
}

pub(super) fn resolve_program(
    raw: &str,
    cwd: &Path,
    workspace_root: &Path,
    policy: &PolicySettings,
    search_path: Option<&OsStr>,
) -> Result<String, WorkspaceError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(WorkspaceError::invalid_argument("Empty program"));
    }

    let explicit_path = trimmed.contains(['/', '\\']);
    let candidate = if Path::new(trimmed).is_absolute() {
        Path::new(trimmed).to_path_buf()
    } else {
        cwd.join(trimmed)
    };
    if candidate.is_file() {
        let resolved = candidate.canonicalize().map_err(|_| WorkspaceError::Tool {
            code: "COMMAND_REJECTED",
            message: format!("Program not found: {trimmed}"),
            category: "runtime",
            retryable: false,
        })?;
        let canonical_workspace =
            workspace_root
                .canonicalize()
                .map_err(|_| WorkspaceError::Tool {
                    code: "COMMAND_REJECTED",
                    message: "Workspace root is unavailable".into(),
                    category: "runtime",
                    retryable: true,
                })?;
        if !resolved.starts_with(&canonical_workspace) {
            return Err(WorkspaceError::Tool {
                code: "EXECUTABLE_OUTSIDE_WORKSPACE",
                message: format!("Workspace 外可执行文件被拒绝: {trimmed}"),
                category: "security",
                retryable: false,
            });
        }
        let extension = resolved
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| format!(".{}", value.to_ascii_lowercase()))
            .unwrap_or_default();
        if policy.workspace_local_entries
            && (extension.is_empty() || policy.workspace_script_extensions.contains(&extension))
        {
            return Ok(resolved.to_string_lossy().into_owned());
        }
        return Err(WorkspaceError::Tool {
            code: "COMMAND_REJECTED",
            message: format!("Workspace 本地入口未获允许: {trimmed}"),
            category: "policy",
            retryable: false,
        });
    }

    if explicit_path {
        return Err(WorkspaceError::Tool {
            code: "COMMAND_REJECTED",
            message: format!("Program not found: {trimmed}"),
            category: "runtime",
            retryable: false,
        });
    }

    which_on_path(trimmed, cwd, search_path)
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| WorkspaceError::Tool {
            code: "COMMAND_REJECTED",
            message: format!("Program not found on PATH: {trimmed}"),
            category: "runtime",
            retryable: false,
        })
}

pub(super) fn which_on_path(
    program: &str,
    cwd: &Path,
    search_path: Option<&OsStr>,
) -> Option<PathBuf> {
    let Some(paths) = search_path else {
        return which::which(program).ok();
    };

    for directory in std::env::split_paths(paths) {
        let directory = if directory.is_absolute() {
            directory
        } else {
            cwd.join(directory)
        };
        for candidate in executable_candidates(directory.join(program)) {
            if is_executable_file(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(not(windows))]
fn executable_candidates(candidate: PathBuf) -> Vec<PathBuf> {
    vec![candidate]
}

#[cfg(windows)]
fn executable_candidates(candidate: PathBuf) -> Vec<PathBuf> {
    if candidate.extension().is_some() {
        return vec![candidate];
    }
    let extensions = std::env::var_os("PATHEXT")
        .map(|value| {
            value
                .to_string_lossy()
                .split(';')
                .filter_map(|item| {
                    let extension = item.trim().trim_start_matches('.');
                    (!extension.is_empty()).then(|| extension.to_ascii_lowercase())
                })
                .collect::<Vec<_>>()
        })
        .filter(|items| !items.is_empty())
        .unwrap_or_else(|| vec!["com".into(), "exe".into(), "bat".into(), "cmd".into()]);

    extensions
        .into_iter()
        .map(|extension| candidate.with_extension(extension))
        .collect()
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(windows)]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}
