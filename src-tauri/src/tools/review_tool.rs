use serde_json::{json, Value};

use crate::tools::context::ToolContext;
use crate::tools::workspace::{tool_ok, WorkspaceError, WorkspaceResult};

pub fn change_review(ctx: &ToolContext, args: &Value) -> WorkspaceResult<Value> {
    let scope = args.get("scope").and_then(Value::as_str).unwrap_or("workspace");
    let summary = args.get("summary").and_then(Value::as_str).unwrap_or(match scope {
        "session" => "Session aggregate changes",
        _ => "Workspace changes vs HEAD",
    });
    let link = match scope {
        "workspace" => crate::review::create_workspace_review(ctx.workspace.root(), summary),
        "session" => {
            let session_id = args.get("session_id").and_then(Value::as_str)
                .or_else(|| args.get("_review_session_id").and_then(Value::as_str))
                .map(str::trim).filter(|value| !value.is_empty())
                .ok_or_else(|| WorkspaceError::invalid_argument("Session Review requires an active or explicit session_id"))?;
            crate::review::create_session_review(ctx.workspace.root(), session_id, summary)
        }
        _ => return Err(WorkspaceError::invalid_argument("scope must be workspace or session")),
    }.map_err(WorkspaceError::invalid_argument)?;

    let Some(link) = link else {
        return Ok(tool_ok(json!({
            "scope": scope,
            "empty": true,
            "message": if scope == "workspace" { "Workspace has no changes relative to HEAD." } else { "No operation reviews were found for this session." }
        })));
    };
    let settings = crate::settings::AppSettings::load_or_default();
    let base = settings.global_gateway.public_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(WorkspaceError::invalid_argument("Global Gateway Public URL is not configured"));
    }
    let review = crate::review::load_review(ctx.workspace.root(), &link.change_id, &link.token)
        .map_err(WorkspaceError::invalid_argument)?;
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
