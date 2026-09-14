mod context;
mod lifecycle;
mod markdown;
mod model;
mod query;
mod storage;
mod validation;

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::tools::context::ToolContext;
use crate::tools::workspace::{
    relative_display, tool_ok, Workspace, WorkspaceError, WorkspaceResult,
};

use self::model::{InitialInputRecord, SearchHit};

pub use context::{context_snapshot, list_sessions_for_workspace};
pub use lifecycle::{bootstrap, checkpoint};
pub use query::{read, search};
pub use validation::validate;

const BOOTSTRAP_RESPONSE_BUDGET: usize = 64 * 1024;
const DEFAULT_SEARCH_LIMIT: usize = 5;
const MAX_SEARCH_LIMIT: usize = 50;
const DEFAULT_READ_MAX_BYTES: usize = 16 * 1024;
const MAX_READ_MAX_BYTES: usize = 64 * 1024;
const CONTEXT_SESSION_SNIPPET_LIMIT: usize = 3;
const CONTEXT_SNIPPET_MAX_CHARS: usize = 512;

/// Return the small, UI-safe session catalog used by the desktop history picker.
/// The catalog contains metadata and bounded focus text, never the archive body.
fn bound_bootstrap_result(result: &mut Value) {
    let mut encoded = serde_json::to_vec(result).unwrap_or_default();
    if encoded.len() <= BOOTSTRAP_RESPONSE_BUDGET {
        return;
    }
    if let Some(state) = result.get_mut("state").and_then(Value::as_object_mut) {
        state.insert("recent_changes".into(), json!([]));
        state.insert("open_items".into(), json!([]));
        state.insert("references".into(), json!([]));
        state.insert(
            "current_focus".into(),
            Value::String("当前状态已收紧；请用 history_session_search 定位档案。".into()),
        );
    }
    if let Some(object) = result.as_object_mut() {
        object.insert("state_truncated".into(), Value::Bool(true));
    }
    encoded = serde_json::to_vec(result).unwrap_or_default();
    if encoded.len() > BOOTSTRAP_RESPONSE_BUDGET {
        if let Some(object) = result.as_object_mut() {
            object.insert("assistant_instructions".into(), Value::String("Use history_session_search and history_session_read for exact archive context; checkpoint raw_user_input before final response.".into()));
            object.insert(
                "warnings".into(),
                json!(["Bootstrap state was reduced to preserve a bounded remote response."]),
            );
        }
    }
}

fn derived_file_warnings(history_dir: &std::path::Path) -> Vec<String> {
    let mut warnings = Vec::new();
    for (label, result) in [
        (
            "历史索引",
            storage::read_index(history_dir).map(|value| value.is_some()),
        ),
        (
            "memory/manifest.json",
            storage::read_manifest(history_dir).map(|value| value.is_some()),
        ),
        (
            "memory/state.json",
            storage::read_state(history_dir).map(|value| value.is_some()),
        ),
    ] {
        match result {
            Ok(true) => {}
            Ok(false) => warnings.push(format!("{label} 缺失，已根据 Markdown 档案重建。")),
            Err(_) => warnings.push(format!("{label} 损坏，已根据 Markdown 档案重建。")),
        }
    }
    let readme = history_dir.join("README.md");
    if readme.exists() {
        let _ = fs::read_to_string(readme);
    }
    warnings
}

fn derived_status<T>(result: WorkspaceResult<Option<T>>) -> &'static str {
    match result {
        Ok(Some(_)) => "valid",
        Ok(None) => "missing",
        Err(_) => "invalid",
    }
}

fn host_session_key(args: &Value) -> Option<&str> {
    args.get("_host_session_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn resolve_dir(ctx: &ToolContext, args: &Value) -> WorkspaceResult<std::path::PathBuf> {
    storage::resolve_history_dir(
        &ctx.workspace,
        args.get("workspace_root").and_then(Value::as_str),
        args.get("history_dir").and_then(Value::as_str),
    )
}

fn resolve_session_key(ctx: &ToolContext, args: &Value) -> WorkspaceResult<(String, &'static str)> {
    if let Some(value) = host_session_key(args) {
        return Ok((value.to_string(), "platform_conversation_id"));
    }
    if let Some(value) = args
        .get("session_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok((value.to_string(), "explicit_session_key"));
    }
    let fingerprint = storage::sha256(ctx.workspace.root_display().as_bytes());
    Ok((
        format!("workspace-session-{}", &fingerprint[..16]),
        "workspace_fallback",
    ))
}

fn reject_ambiguous_history(report: &model::ScanReport) -> WorkspaceResult<()> {
    if report.duplicate_session_keys.is_empty() {
        return Ok(());
    }
    Err(history_error(
        "HISTORY_INDEX_CONFLICT",
        "Multiple history files declare the same session_key.",
        "validation",
        false,
        json!({"duplicate_session_keys": report.duplicate_session_keys}),
    ))
}

fn session_not_bootstrapped() -> WorkspaceError {
    history_error(
        "SESSION_NOT_BOOTSTRAPPED",
        "The session_key has not been bootstrapped.",
        "not_found",
        false,
        json!({}),
    )
}

fn history_error(
    code: &'static str,
    message: &str,
    category: &'static str,
    retryable: bool,
    details: Value,
) -> WorkspaceError {
    WorkspaceError::ToolDetails {
        code,
        message: message.into(),
        category,
        retryable,
        details,
    }
}

fn history_dir_display(ctx: &ToolContext, path: &std::path::Path) -> String {
    crate::tools::workspace::relative_display(ctx.workspace.root(), path)
}

fn now_timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("unix:{seconds}")
}
