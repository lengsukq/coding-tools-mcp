use serde_json::{json, Value};

#[cfg(test)]
use crate::agent_context::render_skill_catalog;
use crate::mcp::gateway::{
    gateway_tool_definitions, is_gateway_tool, GatewayError, GatewayState, WorkspaceRequestContext,
};
use crate::tools::{call_tool, list_tools_for_profile, wrap_mcp_tool_result, SharedToolContext};

pub type SharedState = SharedToolContext;

#[derive(Clone)]
pub struct GatewayResponse {
    pub body: Value,
    pub workspace: Option<WorkspaceRequestContext>,
}

mod dispatch;
mod scope;

pub use dispatch::handle_gateway_request;
pub use scope::audit_workspace_hint;
use scope::*;

/// 中转层支持协商的 legacy-era MCP 协议版本，按版本升序排列。
///
/// 不要仅为了“跟版本”把 2026-07-28 加到这里：该版本进入 modern/stateless era，
/// 需要 server/discover、每请求 _meta、HTTP 版本/方法头校验以及无 initialize/session 的
/// 完整 transport 语义。只有这些行为一起实现并验证后才能对外宣称支持。
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
    &["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];
/// 客户端未声明协议版本时的兼容默认值，保持历史行为。
pub const DEFAULT_PROTOCOL_VERSION: &str = "2025-06-18";
/// 服务端支持的最新协议版本。
pub const LATEST_PROTOCOL_VERSION: &str = "2025-11-25";

/// 按 MCP 规则协商协议版本：支持集合内直接回显；未知版本回退到服务端最新版本，
/// 由客户端自行决定是否继续；缺省时沿用历史默认版本。
pub fn negotiate_protocol_version(requested: Option<&str>) -> &'static str {
    let requested = requested.map(str::trim).filter(|value| !value.is_empty());
    let Some(requested) = requested else {
        return DEFAULT_PROTOCOL_VERSION;
    };
    SUPPORTED_PROTOCOL_VERSIONS
        .iter()
        .copied()
        .find(|version| *version == requested)
        .unwrap_or(LATEST_PROTOCOL_VERSION)
}

#[cfg(test)]
pub fn handle_request(state: &SharedState, body: &Value) -> Value {
    let method = body.get("method").and_then(Value::as_str).unwrap_or("");
    let id = body.get("id").cloned().unwrap_or(Value::Null);
    let params = body.get("params").cloned().unwrap_or(Value::Null);

    if id.is_null() && method.starts_with("notifications/") {
        return Value::Null;
    }

    let result = match method {
        "initialize" => {
            let requested = params.get("protocolVersion").and_then(Value::as_str);
            Ok(initialize_result(
                state,
                negotiate_protocol_version(requested),
            ))
        }
        "ping" => Ok(serde_json::json!({})),
        "tools/list" => {
            let tools = list_tools_for_profile(state.tool_profile.as_str());
            state.record_context_block("tool_definitions", &json!(tools));
            Ok(json!({ "tools": tools }))
        }
        "tools/call" => handle_tools_call(state, &params),
        _ => Err(serde_json::json!({
            "code": -32601,
            "message": format!("Method not found: {method}")
        })),
    };

    match result {
        Ok(result) => serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => serde_json::json!({ "jsonrpc": "2.0", "id": id, "error": error }),
    }
}

#[cfg(test)]
fn initialize_result(state: &SharedState, protocol_version: &str) -> Value {
    let base_instructions = "Use these tools only for local coding operations inside the configured workspace. Planning mode is controlled exclusively by the desktop app: every tool response may contain planning_context with the authoritative current mode, revision, focused Goal, and focused Plan. Never assume or attempt to change the mode from chat. Goal and Plan records are AI-driven conversation artifacts: when a user request benefits from durable tracking, create_goal and create_plan may be called directly from the conversation without asking the user to fill desktop forms or approve a proposal first. Keep their criteria and steps updated as work progresses. When the work is ready for acceptance, call request_goal_review and/or request_plan_review with a concise verification summary. Never archive or claim final acceptance yourself; only the human desktop review action can accept and archive. If a review is rejected, continue from the reactivated Goal/Plan and incorporate the human feedback. In Plan mode, Goal/Plan metadata operations remain writable: create/update Goal or Plan records and request their review as needed. Plan mode blocks project/source file mutations, command execution, and task mutations; do not interpret it as blocking the planning tools themselves. In Goal mode, project mutations require an active focused Goal and must respect any focused Plan relationship/status. If the client reports missing tools while server authorization is still valid, treat it as a capability discovery mismatch rather than a permission loss: refresh the MCP session/tool list before requesting permissions. At the start of every new ChatGPT conversation, before answering the user's first request, call history_session_bootstrap exactly once and pass the user's verbatim first request as initial_user_input. Treat bootstrap as required conversation initialization: it creates or resumes a lossless Markdown archive and returns bounded current state, not all history. Use history_session_search followed by history_session_read only when exact earlier context is needed. history_session_read returns a bounded UTF-8-safe page; follow next_cursor with the returned content hash until the relevant archive is complete. Repeated successful bootstrap calls in the same conversation resume the same session and must not create duplicates. Preserve session_key and current_path returned by bootstrap, then pass them unchanged as session_key and expected_path to every history_session_checkpoint call. After completing each user-requested task in the conversation, call history_session_checkpoint before the final response and pass that user's verbatim request as raw_user_input. Only state that progress was saved after checkpoint returns ok=true with the same session_key and path. The server cannot access ChatGPT transcript text that was not provided as a tool argument; persistence is not automatic background persistence. If an operation returns DANGEROUS_OPERATION_REQUIRES_CONFIRMATION, do not request a separate permission grant. Only retry the same tool with confirm=true when the user's request already clearly authorizes that dangerous operation; otherwise ask the user for confirmation.";
    let base_instructions = if state.tool_profile.is_compact() {
        "Use these tools only for local coding operations inside the configured workspace. This profile uses Stable Tool API v2: use history_manage and planning_manage with their action field instead of relying on lifecycle-specific tool names. Goal → Plan → Steps is the default agent workflow; durable Task/Harness lifecycle remains an advanced compatibility capability and is not part of the compact tool surface. Treat Workspace Root as the stable path base: pass explicit relative paths and use exec_command workdir for subdirectory commands instead of relying on mutable default-cwd session state. The desktop app controls permissions and planning mode. In Plan mode, planning_manage remains writable for Goal/Plan create, update, and review actions; only project/source mutations and command execution are blocked. History recording is controlled by the workspace setting; history bootstrap is optional and is never required before the first response. Use history_manage action=search/read only when exact older context is needed. Selected history context below is a bounded snapshot; do not repeat it in tool responses. Checkpoints may omit session_key and expected_path because the server can lazily create the current workspace session. If a dangerous operation requires confirmation, retry only the original tool with confirm=true when the user's request clearly authorizes it."
    } else {
        base_instructions
    };
    let history_instructions = if state.tool_profile.is_compact() && state.history_recording {
        "History recording is enabled for this workspace. At the end of each user-requested development task, call history_manage with action=checkpoint and the user's verbatim current request as raw_user_input, along with the actual findings, modified files, and tests. Checkpoint lazily creates the session archive, so an initial bootstrap is optional. Do not claim a conversation was archived unless the tool confirms recorded=true. History persistence is model-mediated, not automatic background capture."
    } else {
        ""
    };
    let review_instructions = "Change Review is supported for eligible source mutations. When a successful write result contains review_url, surface that exact complete URL to the user as the preferred way to inspect what the AI changed. Never reconstruct, shorten, or modify review_url, and never expose review_token separately. If several writes return review URLs, keep the URLs relevant to the current user task. Do not claim that command-side filesystem changes are included unless their result explicitly provides a review_url.";
    let current_ai_instructions = state.current_ai_instructions();
    let configured = if current_ai_instructions.trim().is_empty() {
        String::new()
    } else {
        format!(
            "Configured agent instructions (global, workspace, repository sources):\n{}",
            current_ai_instructions.trim()
        )
    };
    let current_skills = state.current_skills();
    let skill_catalog = if state.tool_profile.is_compact() {
        String::new()
    } else {
        render_skill_catalog(&current_skills)
    };
    let history_context = crate::tools::history::context_snapshot(state)
        .ok()
        .flatten()
        .map(|value| {
            format!(
                "Selected workspace history context (revisioned snapshot; do not repeat in tool results):\n{}",
                serde_json::to_string(&value).unwrap_or_else(|_| "{}".into())
            )
        })
        .unwrap_or_default();
    state.record_context_block(
        "initialization_rules",
        &json!({
            "base": base_instructions,
            "history": history_instructions,
            "configured": configured,
            "skills": skill_catalog
        }),
    );
    if !history_context.is_empty() {
        state.record_context_block("history_snapshot", &Value::String(history_context.clone()));
    }
    let instructions = [
        base_instructions,
        history_instructions,
        review_instructions,
        configured.as_str(),
        skill_catalog.as_str(),
        history_context.as_str(),
    ]
    .into_iter()
    .filter(|value| !value.trim().is_empty())
    .collect::<Vec<_>>()
    .join("\n\n");
    serde_json::json!({
        "protocolVersion": protocol_version,
        "capabilities": {
            "tools": { "listChanged": false },
            "logging": {}
        },
        "serverInfo": {
            "name": "coding-tools-mcp",
            "title": "Coding Tools MCP",
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": instructions
    })
}

#[cfg(test)]
fn handle_tools_call(state: &SharedState, params: &Value) -> Result<Value, Value> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| serde_json::json!({ "code": -32602, "message": "Missing tool name" }))?;
    let args = tool_arguments(name, params);

    let canonical_name = crate::tools::registry::canonical_tool_name(name);
    let known = crate::tools::registry::exposed_tool_names(state.tool_profile.as_str());
    if !known.iter().any(|n| n == &canonical_name) {
        return Err(serde_json::json!({
            "code": -32602,
            "message": format!("Unknown tool: {name}"),
            "data": { "reason": "unknown_tool" }
        }));
    }

    let structured = call_tool(state.as_ref(), canonical_name, &args);
    let result = wrap_mcp_tool_result(canonical_name, &args, structured);
    state.record_context_block("tool_return", &result);
    Ok(result)
}

fn tool_arguments(name: &str, params: &Value) -> Value {
    let mut args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if name.starts_with("history_session_")
        || name == "history_manage"
        || name == "workspace_invoke"
    {
        if let Some(session_key) = params
            .get("_meta")
            .and_then(|meta| meta.get("openai/session"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if !args.is_object() {
                args = serde_json::json!({});
            }
            args["_host_session_key"] = Value::String(session_key.to_string());
        }
    }
    args
}

#[cfg(test)]
mod tests;
