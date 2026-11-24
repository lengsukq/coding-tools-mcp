//! Workspace-contained workdir and argument-path checks for `exec_command`.

use std::path::{Component, Path};
use std::sync::OnceLock;

use crate::tools::workspace::{ResolvedPath, Workspace, WorkspaceError};

pub(crate) fn resolve_workdir(
    workspace: &Workspace,
    raw: &str,
) -> Result<ResolvedPath, WorkspaceError> {
    let trimmed = if raw.is_empty() { "." } else { raw };
    let path = Path::new(trimmed);
    if path.components().any(|part| part == Component::ParentDir) {
        return Err(WorkspaceError::path_outside_workspace());
    }
    if !path.is_absolute() {
        return workspace.resolve_existing(trimmed);
    }
    let resolved = path
        .canonicalize()
        .map_err(|_| WorkspaceError::not_found(format!("Path not found: {trimmed}")))?;
    if !resolved.starts_with(workspace.root()) {
        return Err(WorkspaceError::path_outside_workspace());
    }
    Ok(ResolvedPath {
        display: resolved.display().to_string(),
        path: resolved,
        existed: true,
    })
}

/// Literal-path policy guard for interpreter mutation arguments (excludes `args[0]` executable).
pub(crate) fn contains_external_path(args: &[String], workspace: Option<&Workspace>) -> bool {
    static TOKENS: OnceLock<regex::Regex> = OnceLock::new();
    let tokens =
        TOKENS.get_or_init(|| regex::Regex::new(r#""([^"]*)"|'([^']*)'|([^\s"'(),;=]+)"#).unwrap());
    args.iter().any(|arg| {
        let normalized = arg.replace('\\', "/");
        if normalized.contains("../") {
            return true;
        }
        if looks_absolute(&normalized) {
            return outside(&normalized, workspace);
        }
        tokens.captures_iter(&normalized).any(|capture| {
            let token = capture.iter().skip(1).flatten().next().unwrap().as_str();
            looks_absolute(token) && outside(token, workspace)
        })
    })
}

fn looks_absolute(raw: &str) -> bool {
    raw.starts_with('/')
        || (raw.len() >= 3
            && raw.as_bytes()[0].is_ascii_alphabetic()
            && raw.as_bytes()[1] == b':'
            && raw.as_bytes()[2] == b'/')
}

fn outside(raw: &str, workspace: Option<&Workspace>) -> bool {
    let Some(workspace) = workspace else {
        return true;
    };
    let path = Path::new(raw);
    if !path.is_absolute() {
        return true;
    }
    // A write target may not exist yet; its closest existing ancestor must resolve
    // inside the workspace so an outside symlink cannot authorize a write.
    let Some(ancestor) = path.ancestors().find(|p| p.exists() || p.is_symlink()) else {
        return true;
    };
    ancestor
        .canonicalize()
        .map(|p| !p.starts_with(workspace.root()))
        .unwrap_or(true)
}
