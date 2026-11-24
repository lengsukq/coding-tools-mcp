use std::collections::HashMap;
use std::fs;
use std::io;

use serde_json::{json, Value};
use uuid::Uuid;

use crate::tools::context::ToolContext;
use crate::tools::workspace::{tool_ok, WorkspaceError};

mod hunk;
mod parser;
mod tx;

use hunk::apply_hunks;
use parser::{parse_unified_diff, FilePatch};
use tx::commit_staged;
#[cfg(test)]
pub(crate) use tx::commit_staged_bytes;

pub fn apply_patch(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    apply_patch_with_operation(ctx, args, None)
}

pub(crate) fn apply_patch_with_operation(
    ctx: &ToolContext,
    args: &Value,
    operation_id: Option<&str>,
) -> Result<Value, WorkspaceError> {
    let patch = args
        .get("patch")
        .and_then(Value::as_str)
        .ok_or_else(|| WorkspaceError::invalid_argument("patch is required"))?;
    let dry_run = args
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let confirm = args
        .get("confirm")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let session_id = args
        .get("_review_session_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let file_patches = parse_unified_diff(patch)?;
    validate_patch_targets(ctx, &file_patches, confirm)?;
    let prepared = stage_file_patches(ctx, &file_patches)?;

    let files_created = affected_paths(&prepared.affected, "add");
    let files_modified = affected_paths(&prepared.affected, "update");
    let files_deleted = affected_paths(&prepared.affected, "delete");
    let summary = prepared.summaries.join("\n");

    if dry_run {
        return Ok(tool_ok(json!({
            "dry_run": true,
            "preflight": true,
            "clean": true,
            "summary": summary,
            "affected_files": prepared.affected,
            "would_create": files_created,
            "would_modify": files_modified,
            "would_delete": files_deleted,
            "warnings": []
        })));
    }

    commit_staged(&ctx.workspace, &prepared.staged)?;
    let change_id = Uuid::new_v4().simple().to_string();
    let mut warnings = Vec::new();
    let review = match crate::review::create_patch_review_with_metadata(
        ctx.workspace.root(),
        &change_id,
        operation_id,
        session_id,
        &summary,
        &prepared.review_files,
    ) {
        Ok(link) => Some(link),
        Err(err) => {
            warnings.push(format!("变更快照持久化失败，但不影响已写入的文件: {err}"));
            None
        }
    };
    let review_url = review
        .as_ref()
        .and_then(|link| build_review_url(&link.change_id, &link.token));

    Ok(tool_ok(json!({
        "dry_run": false,
        "clean": true,
        "change_id": change_id,
        "review_url": review_url,
        "summary": summary,
        "affected_files": prepared.affected,
        "files_created": files_created,
        "files_modified": files_modified,
        "files_deleted": files_deleted,
        "recovery": "git",
        "warnings": warnings
    })))
}

pub fn patch_check(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    let mut check_args = args.clone();
    check_args["dry_run"] = Value::Bool(true);
    let mut result = apply_patch(ctx, &check_args)?;
    if let Some(object) = result.as_object_mut() {
        object.insert("preflight".into(), Value::Bool(true));
    }
    Ok(result)
}

struct PreparedPatch {
    affected: Vec<Value>,
    summaries: Vec<String>,
    staged: HashMap<String, Option<String>>,
    review_files: Vec<(String, String, String, String)>,
}

fn validate_patch_targets(
    ctx: &ToolContext,
    file_patches: &[FilePatch],
    confirm: bool,
) -> Result<(), WorkspaceError> {
    if file_patches.is_empty() {
        return Err(patch_failed("No files were modified."));
    }
    if let Some(path) = file_patches
        .iter()
        .find(|file| is_git_repository_asset(&file.path))
        .map(|file| file.path.as_str())
    {
        return Err(protected_repository_asset(format!(
            "禁止修改 .git 仓库保护资产: {path}"
        )));
    }
    if !ctx.policy.allow_high_risk_writes {
        if let Some(path) = file_patches
            .iter()
            .find(|file| is_high_risk_file(&file.path))
            .map(|file| file.path.as_str())
        {
            return Err(protected_repository_asset(format!(
                "当前 Workspace 未开启高风险文件写入权限: {path}"
            )));
        }
    }
    if !confirm {
        if let Some(path) = file_patches
            .iter()
            .find(|file| file.is_deleted && is_high_risk_file(&file.path))
            .map(|file| file.path.as_str())
        {
            return Err(dangerous_operation(format!(
                "删除关键项目文件需要 confirm=true: {path}"
            )));
        }
    }
    Ok(())
}

fn stage_file_patches(
    ctx: &ToolContext,
    file_patches: &[FilePatch],
) -> Result<PreparedPatch, WorkspaceError> {
    let ws = &ctx.workspace;
    let mut prepared = PreparedPatch {
        affected: Vec::with_capacity(file_patches.len()),
        summaries: Vec::with_capacity(file_patches.len()),
        staged: HashMap::with_capacity(file_patches.len()),
        review_files: Vec::with_capacity(file_patches.len()),
    };

    for fp in file_patches {
        ws.reject_unsafe_text(&fp.path)?;
        let resolved = if fp.is_new_file {
            ws.resolve_for_write(&fp.path)?
        } else {
            ws.resolve_existing(&fp.path)?
        };
        ws.reject_write_symlink(&fp.path)?;

        let original = if fp.is_new_file {
            // An Add File envelope is replacement content even when an earlier
            // Delete File for the same path exists in this transaction.
            String::new()
        } else {
            fs::read_to_string(&resolved.path)
                .map_err(|err| read_patch_target_error(&fp.path, err))?
        };

        if fp.is_deleted {
            prepared.review_files.push((
                resolved.display.clone(),
                "delete".into(),
                original,
                String::new(),
            ));
            prepared.staged.insert(resolved.display.clone(), None);
            prepared
                .affected
                .push(json!({ "path": resolved.display, "operation": "delete" }));
            prepared.summaries.push(format!("D {}", resolved.display));
            continue;
        }

        let updated = apply_hunks(&original, &fp.hunks)?;
        let op = if resolved.existed { "update" } else { "add" };
        prepared.review_files.push((
            resolved.display.clone(),
            op.into(),
            original,
            updated.clone(),
        ));
        prepared
            .staged
            .insert(resolved.display.clone(), Some(updated));
        prepared
            .affected
            .push(json!({ "path": resolved.display, "operation": op }));
        prepared.summaries.push(format!(
            "{} {}",
            if op == "add" { "A" } else { "M" },
            resolved.display
        ));
    }

    Ok(prepared)
}

fn read_patch_target_error(path: &str, err: io::Error) -> WorkspaceError {
    if err.kind() == io::ErrorKind::NotFound {
        WorkspaceError::not_found(format!("File not found: {path}"))
    } else {
        patch_failed(format!("Failed to read {path}: {err}"))
    }
}

pub(crate) fn build_review_url(change_id: &str, token: &str) -> Option<String> {
    let settings = crate::settings::AppSettings::load_or_default();
    let base = settings
        .global_gateway
        .public_url
        .trim()
        .trim_end_matches('/');
    (!base.is_empty()).then(|| format!("{base}/review/{change_id}?t={token}"))
}

fn affected_paths(affected: &[Value], operation: &str) -> Vec<String> {
    affected
        .iter()
        .filter(|file| file["operation"] == operation)
        .filter_map(|file| file["path"].as_str().map(str::to_string))
        .collect()
}

fn is_high_risk_file(path: &str) -> bool {
    if matches!(
        crate::tools::workspace::Workspace::first_normal_component(path).as_deref(),
        Some(".git" | ".github")
    ) {
        return true;
    }
    let normalized = path.replace('\\', "/");
    let name = normalized.rsplit('/').next().unwrap_or(normalized.as_str());
    name == ".gitignore"
        || name == "Cargo.toml"
        || name == "Cargo.lock"
        || name == "package.json"
        || name == "package-lock.json"
        || name == "pnpm-lock.yaml"
        || name == "tauri.conf.json"
        || name.starts_with("README")
        || name.starts_with("LICENSE")
        || name.starts_with("vite.config.")
        || name == "pyproject.toml"
}

fn is_git_repository_asset(path: &str) -> bool {
    crate::tools::workspace::Workspace::first_normal_component(path).as_deref() == Some(".git")
}

fn dangerous_operation(message: impl Into<String>) -> WorkspaceError {
    WorkspaceError::Tool {
        code: "DANGEROUS_OPERATION_REQUIRES_CONFIRMATION",
        message: message.into(),
        category: "permission",
        retryable: false,
    }
}

fn protected_repository_asset(message: impl Into<String>) -> WorkspaceError {
    WorkspaceError::Tool {
        code: "PROTECTED_REPOSITORY_ASSET",
        message: message.into(),
        category: "security",
        retryable: false,
    }
}

pub(super) fn patch_failed(message: impl Into<String>) -> WorkspaceError {
    WorkspaceError::Tool {
        code: "PATCH_FAILED",
        message: message.into(),
        category: "validation",
        retryable: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::context::ToolContext;
    use parser::{Hunk, HunkLine};
    use serde_json::json;
    use tempfile::tempdir;

    fn context_with_file() -> (tempfile::TempDir, tempfile::TempDir, ToolContext) {
        let workspace = tempdir().expect("workspace");
        let harness = tempdir().expect("harness");
        std::fs::write(workspace.path().join("main.rs"), "old\n").expect("file");
        let context =
            ToolContext::for_test(workspace.path().to_path_buf(), harness.path().to_path_buf())
                .expect("context");
        (workspace, harness, context)
    }

    fn patch() -> Value {
        json!({
            "patch": "--- a/main.rs\n+++ b/main.rs\n@@\n-old\n+new\n"
        })
    }

    #[test]
    fn patch_check_does_not_modify_workspace() {
        let (_workspace, _harness, context) = context_with_file();
        let result = patch_check(&context, &patch()).expect("patch check");
        assert_eq!(result["preflight"], true);
        assert_eq!(
            std::fs::read_to_string(context.workspace.root().join("main.rs")).unwrap(),
            "old\n"
        );
    }

    #[test]
    fn preserves_lf_when_inserting_multiple_lines() {
        let input = "one\ntwo\n";
        let hunk = Hunk {
            lines: vec![
                HunkLine::Context("one".into()),
                HunkLine::Add("insert-a".into()),
                HunkLine::Add("insert-b".into()),
                HunkLine::Context("two".into()),
            ],
        };
        assert_eq!(
            apply_hunks(input, &[hunk]).expect("patch"),
            "one\ninsert-a\ninsert-b\ntwo\n"
        );
    }

    #[test]
    fn preserves_crlf_when_inserting_multiple_lines() {
        let input = "one\r\ntwo\r\n";
        let hunk = Hunk {
            lines: vec![
                HunkLine::Context("one".into()),
                HunkLine::Add("insert-a".into()),
                HunkLine::Add("insert-b".into()),
                HunkLine::Context("two".into()),
            ],
        };
        assert_eq!(
            apply_hunks(input, &[hunk]).expect("patch"),
            "one\r\ninsert-a\r\ninsert-b\r\ntwo\r\n"
        );
    }

    #[test]
    fn delete_then_add_same_path_replaces_instead_of_concatenating_old_content() {
        let (_workspace, _harness, context) = context_with_file();
        let result = apply_patch(
            &context,
            &json!({
                "patch": "*** Begin Patch\n*** Delete File: main.rs\n*** Add File: main.rs\n+fresh\n*** End Patch\n"
            }),
        )
        .expect("replace file");
        assert_eq!(result["files_modified"], json!(["main.rs"]));
        assert_eq!(
            std::fs::read_to_string(context.workspace.root().join("main.rs")).unwrap(),
            "fresh\n"
        );
    }

    #[test]
    fn validation_failure_in_later_file_keeps_all_files_unchanged() {
        let (_workspace, _harness, context) = context_with_file();
        let error = apply_patch(
            &context,
            &json!({
                "patch": "--- a/main.rs\n+++ b/main.rs\n@@\n-old\n+new\n--- a/missing.rs\n+++ b/missing.rs\n@@\n-old\n+new\n"
            }),
        )
        .expect_err("later file fails preflight");
        assert_eq!(error.to_error_value()["code"], "NOT_FOUND");
        assert_eq!(
            std::fs::read_to_string(context.workspace.root().join("main.rs")).unwrap(),
            "old\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn commit_staged_bytes_does_not_overwrite_file_when_backup_read_fails() {
        use std::os::unix::fs::PermissionsExt;

        let (_workspace, _harness, context) = context_with_file();
        let target = context.workspace.root().join("main.rs");
        fs::set_permissions(&target, fs::Permissions::from_mode(0o000)).expect("chmod 000");

        // Skip assertion if running as root where mode 000 remains readable.
        if fs::read(&target).is_ok() {
            fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).expect("restore");
            return;
        }

        let mut staged = HashMap::new();
        staged.insert("main.rs".to_string(), Some(b"new\n".to_vec()));
        let error = commit_staged_bytes(&context.workspace, &staged)
            .expect_err("unreadable target must fail backup step");
        assert_eq!(error.to_error_value()["code"], "PATCH_FAILED");

        fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).expect("restore");
        assert_eq!(fs::read_to_string(&target).expect("read original"), "old\n");
    }
}
