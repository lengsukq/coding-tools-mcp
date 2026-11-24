mod context;
mod lifecycle;
mod markdown;
mod model;
mod query;
mod storage;
mod validation;

use serde_json::{json, Value};

use crate::tools::context::ToolContext;
use crate::tools::workspace::{
    relative_display, tool_ok, Workspace, WorkspaceError, WorkspaceResult,
};
use crate::util::json::serialized_json_bytes;

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

/// Keep bootstrap responses within the remote MCP budget by trimming optional state arrays.
fn bound_bootstrap_result(result: &mut Value) {
    if serialized_json_bytes(result).unwrap_or(0) <= BOOTSTRAP_RESPONSE_BUDGET {
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
    if serialized_json_bytes(result).unwrap_or(0) > BOOTSTRAP_RESPONSE_BUDGET {
        if let Some(object) = result.as_object_mut() {
            object.insert("assistant_instructions".into(), Value::String("Use history_session_search and history_session_read for exact archive context; checkpoint raw_user_input before final response.".into()));
            object.insert(
                "warnings".into(),
                json!(["Bootstrap state was reduced to preserve a bounded remote response."]),
            );
        }
    }
}

fn derived_file_warnings(history_dir: &std::path::Path) -> (Vec<String>, u64) {
    let mut warnings = Vec::new();
    let state_result = storage::read_state(history_dir);
    let previous_state_revision = state_result
        .as_ref()
        .ok()
        .and_then(|state| state.as_ref())
        .map(|state| state.state_revision)
        .unwrap_or(0);
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
            state_result.map(|value| value.is_some()),
        ),
    ] {
        match result {
            Ok(true) => {}
            Ok(false) => warnings.push(format!("{label} 缺失，已根据 Markdown 档案重建。")),
            Err(_) => warnings.push(format!("{label} 损坏，已根据 Markdown 档案重建。")),
        }
    }
    (warnings, previous_state_revision)
}

fn derived_status<T>(result: &WorkspaceResult<Option<T>>) -> &'static str {
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

fn now_timestamp() -> String {
    format!("unix:{}", crate::util::time::unix_now_secs())
}
