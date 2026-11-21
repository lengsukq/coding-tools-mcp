use serde_json::{json, Value};

use crate::tools::context::ToolContext;
use crate::tools::workspace::{tool_ok, WorkspaceError, WorkspaceResult};

pub fn change_review(ctx: &ToolContext, args: &Value) -> WorkspaceResult<Value> {
    let scope = args
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("workspace");
    if !matches!(scope, "workspace" | "session") {
        return Err(WorkspaceError::invalid_argument(
            "scope must be workspace or session",
        ));
    }
    let session_id = if scope == "session" {
        Some(
            args.get("session_id")
                .and_then(Value::as_str)
                .or_else(|| args.get("_review_session_id").and_then(Value::as_str))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    WorkspaceError::invalid_argument(
                        "Session Review requires an active or explicit session_id",
                    )
                })?,
        )
    } else {
        None
    };

    // Validate public_url before persisting any Review artifact to avoid creating
    // unreachable orphan files when the Global Gateway URL is unconfigured.
    let base = configured_public_url()?;
    let summary = args
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or(match scope {
            "session" => "Session aggregate changes",
            _ => "Workspace changes vs HEAD",
        });

    let link = match scope {
        "workspace" => crate::review::create_workspace_review(ctx.workspace.root(), summary),
        "session" => crate::review::create_session_review(
            ctx.workspace.root(),
            session_id.expect("validated session_id"),
            summary,
        ),
        _ => unreachable!("scope validated above"),
    }
    .map_err(map_review_error)?;

    let Some(link) = link else {
        return Ok(tool_ok(json!({
            "scope": scope,
            "empty": true,
            "message": if scope == "workspace" {
                "Workspace has no changes relative to HEAD."
            } else {
                "No operation reviews were found for this session."
            }
        })));
    };

    let review = crate::review::load_review(ctx.workspace.root(), &link.change_id, &link.token)
        .map_err(map_review_error)?;
    Ok(tool_ok(json!({
        "scope": scope,
        "empty": false,
        "change_id": link.change_id,
        "review_url": format!("{base}/review/{}?t={}", link.change_id, link.token),
        "stats": review.stats,
        "base_revision": review.base_revision,
        "summary": review.summary
    })))
}

fn configured_public_url() -> WorkspaceResult<String> {
    let settings = crate::settings::AppSettings::load_or_default();
    let base = settings
        .global_gateway
        .public_url
        .trim()
        .trim_end_matches('/');
    if base.is_empty() {
        return Err(WorkspaceError::invalid_argument(
            "Global Gateway Public URL is not configured",
        ));
    }
    Ok(base.to_string())
}

fn map_review_error(message: String) -> WorkspaceError {
    let is_validation = message.contains("requires a Git repository")
        || message.contains("more than")
        || message.contains("invalid review");
    if is_validation {
        WorkspaceError::invalid_argument(message)
    } else {
        WorkspaceError::Tool {
            code: "REVIEW_STORAGE_ERROR",
            message,
            category: "storage",
            retryable: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_review_error_distinguishes_validation_from_storage_failures() {
        let validation = map_review_error(
            "workspace diff requires a Git repository or a first-level Git repository".into(),
        );
        assert_eq!(validation.to_error_value()["code"], "INVALID_ARGUMENT");
        assert_eq!(validation.to_error_value()["category"], "validation");

        let storage = map_review_error("Permission denied (os error 13)".into());
        assert_eq!(storage.to_error_value()["code"], "REVIEW_STORAGE_ERROR");
        assert_eq!(storage.to_error_value()["category"], "storage");
    }
}
