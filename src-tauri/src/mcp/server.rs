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

fn gateway_scoped_tool_catalog() -> Vec<Value> {
    list_tools_for_profile("compact")
        .into_iter()
        .map(add_workspace_scope_to_tool)
        .collect()
}

fn add_workspace_scope_to_tool(mut tool: Value) -> Value {
    let Some(schema) = tool.get_mut("inputSchema").and_then(Value::as_object_mut) else {
        return tool;
    };
    let properties = schema.entry("properties").or_insert_with(|| json!({}));
    let Some(properties) = properties.as_object_mut() else {
        return tool;
    };
    properties.insert(
        "workspace_id".into(),
        json!({
            "type": "string",
            "description": "Registered Workspace id from workspace_list. Pass this on each ordinary tool call when the MCP client does not preserve transport sessions."
        }),
    );
    tool
}

fn resolve_tool_workspace(
    state: &GatewayState,
    session_id: Option<&str>,
    args: &mut Value,
) -> Result<WorkspaceRequestContext, Value> {
    if let Some(workspace_id) = remove_workspace_scope(args)? {
        return state
            .workspace_by_id(session_id.unwrap_or("request-scoped"), &workspace_id)
            .map_err(|error| error.to_rpc_error());
    }

    if let Some(session_id) = session_id {
        match state.active_workspace(session_id) {
            Ok(request) => return Ok(request),
            Err(error) if error.code != "WORKSPACE_NOT_SELECTED" => {
                return Err(error.to_rpc_error())
            }
            Err(_) => {}
        }
    }

    let workspaces = state.registry.list().map_err(|message| {
        GatewayError::internal("WORKSPACE_REGISTRY_UNAVAILABLE", message).to_rpc_error()
    })?;
    if let [profile] = workspaces.as_slice() {
        return state
            .workspace_by_id(session_id.unwrap_or("single-workspace"), &profile.id)
            .map_err(|error| error.to_rpc_error());
    }

    Err(GatewayError::invalid(
        "WORKSPACE_NOT_SELECTED",
        "当前请求没有明确 Workspace。请先调用 workspace_list，并在普通工具调用中传 workspace_id；支持持久 MCP session 的客户端也可先调用 workspace_select。",
    )
    .to_rpc_error())
}

fn remove_workspace_scope(args: &mut Value) -> Result<Option<String>, Value> {
    let Some(object) = args.as_object_mut() else {
        return Ok(None);
    };
    let Some(value) = object.remove("workspace_id") else {
        return Ok(None);
    };
    let Value::String(value) = value else {
        return Err(GatewayError::invalid(
            "INVALID_WORKSPACE_SCOPE",
            "workspace_id 必须是 workspace_list 返回的字符串 id。",
        )
        .to_rpc_error());
    };
    let value = value.trim();
    if value.is_empty() {
        return Err(
            GatewayError::invalid("INVALID_WORKSPACE_SCOPE", "workspace_id 不能为空。")
                .to_rpc_error(),
        );
    }
    Ok(Some(value.to_string()))
}

fn attach_gateway_server_info(
    result: &mut Value,
    state: &GatewayState,
    request: &WorkspaceRequestContext,
) {
    let Some(structured) = result
        .get_mut("structuredContent")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    structured.insert(
        "gateway".into(),
        json!({
            "mode": "global-multi-workspace",
            "session_id": request.session_id,
            "active_workspace": {
                "id": request.profile.id,
                "name": request.profile.name,
                "path": request.profile.path,
            },
            "workspace_registry_revision": state.registry.revision().unwrap_or_default(),
            "active_session_count": state.sessions.active_session_count(),
        }),
    );
}

/// Handle one request for the 0.3 global MCP endpoint.
///
/// Workspace resolution happens before ordinary tool dispatch and returns an
/// immutable request-scoped ToolContext. A concurrent workspace_select call can
/// therefore only affect the *next* request from that MCP session.
pub fn handle_gateway_request(
    state: &GatewayState,
    session_id: Option<&str>,
    body: &Value,
) -> GatewayResponse {
    if let Some(session_id) = session_id {
        if !state.sessions.touch_existing(session_id) {
            return GatewayResponse {
                body: json!({
                    "jsonrpc": "2.0",
                    "id": body.get("id").cloned().unwrap_or(Value::Null),
                    "error": GatewayError::invalid(
                        "MCP_SESSION_INVALID",
                        "MCP session 不存在或已过期。请重新建立 MCP 连接并使用服务端返回的 session 标识。",
                    )
                    .to_rpc_error()
                }),
                workspace: None,
            };
        }
    }
    let method = body.get("method").and_then(Value::as_str).unwrap_or("");
    let id = body.get("id").cloned().unwrap_or(Value::Null);
    let params = body.get("params").cloned().unwrap_or(Value::Null);

    if id.is_null() && method.starts_with("notifications/") {
        return GatewayResponse {
            body: Value::Null,
            workspace: None,
        };
    }

    let mut workspace = None;
    let result = match method {
        "initialize" => {
            let requested = params.get("protocolVersion").and_then(Value::as_str);
            Ok(gateway_initialize_result(
                state,
                negotiate_protocol_version(requested),
            ))
        }
        "ping" => Ok(json!({})),
        "tools/list" => {
            let mut tools = gateway_scoped_tool_catalog();
            tools.extend(gateway_tool_definitions());
            Ok(json!({ "tools": tools }))
        }
        "tools/call" => match handle_gateway_tools_call(state, session_id, &params) {
            Ok((result, request_workspace)) => {
                workspace = request_workspace;
                Ok(result)
            }
            Err(error) => Err(error),
        },
        _ => Err(json!({
            "code": -32601,
            "message": format!("Method not found: {method}")
        })),
    };

    let body = match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
    };
    GatewayResponse { body, workspace }
}

fn gateway_initialize_result(state: &GatewayState, protocol_version: &str) -> Value {
    let registry_revision = state.registry.revision().unwrap_or_default();
    let instructions = "Coding Tools MCP is a local-first coding runtime exposed through one Global MCP connection. The user-facing workflow is Workspace → Goal/Plan → Execution → History. Workspace data is isolated: call workspace_list when the target is unknown, then workspace_select for stateful clients that preserve the same MCP transport session. Do not infer a filesystem root or use the desktop UI selection as routing authority. If the client does not preserve MCP session state, or an ordinary tool reports WORKSPACE_NOT_SELECTED after selection, use workspace_invoke with the explicit workspace_id for that operation instead of guessing or repeatedly selecting. Keep Planning as the durable source of intent and progress, Execution Ledger/checkpoints as the source of runtime progress and failures, Verification evidence as the completion gate, and History as the cross-session recovery record. When resuming existing work, continue the active Goal/Plan and its current execution checkpoint before creating duplicate work. workspace_invoke is one-off and never changes another request or Chat session.";
    json!({
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
        "instructions": instructions,
        "_meta": {
            "workspaceRegistryRevision": registry_revision,
            "gatewayMode": "multi-workspace"
        }
    })
}

fn handle_gateway_tools_call(
    state: &GatewayState,
    session_id: Option<&str>,
    params: &Value,
) -> Result<(Value, Option<WorkspaceRequestContext>), Value> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| json!({ "code": -32602, "message": "Missing tool name" }))?;
    let mut args = tool_arguments(name, params);

    if is_gateway_tool(name) {
        return handle_workspace_tool(state, session_id, name, &args);
    }

    let request = resolve_tool_workspace(state, session_id, &mut args)?;
    let canonical_name = crate::tools::registry::canonical_tool_name(name);
    ensure_gateway_tool_allowed(&request, canonical_name)?;
    let structured = call_tool(request.tools.as_ref(), canonical_name, &args);
    let mut result = wrap_mcp_tool_result(canonical_name, &args, structured);
    if canonical_name == "server_info" {
        attach_gateway_server_info(&mut result, state, &request);
    }
    request.tools.record_context_block("tool_return", &result);
    Ok((result, Some(request)))
}

fn handle_workspace_tool(
    state: &GatewayState,
    session_id: Option<&str>,
    name: &str,
    args: &Value,
) -> Result<(Value, Option<WorkspaceRequestContext>), Value> {
    use crate::tools::workspace::tool_ok;

    match name {
        "workspace_list" => {
            let workspaces = state.registry.descriptors().map_err(|message| {
                GatewayError::internal("WORKSPACE_REGISTRY_UNAVAILABLE", message).to_rpc_error()
            })?;
            let revision = state.registry.revision().unwrap_or_default();
            let structured = tool_ok(json!({
                "workspaces": workspaces,
                "count": workspaces.len(),
                "registry_revision": revision
            }));
            Ok((wrap_mcp_tool_result(name, args, structured), None))
        }
        "workspace_current" => {
            let session_id = require_gateway_session(session_id)?;
            let scope = state.sessions.current(session_id);
            let workspace = match scope.active_workspace_id {
                Some(id) => match state.registry.resolve(&id) {
                    Ok(profile) => Some(crate::mcp::gateway::WorkspaceDescriptor::from(&profile)),
                    Err(_) => {
                        state.sessions.clear_selection(session_id);
                        None
                    }
                },
                None => None,
            };
            let structured = tool_ok(json!({
                "session_id": session_id,
                "workspace": workspace,
                "selected": workspace.is_some()
            }));
            Ok((wrap_mcp_tool_result(name, args, structured), None))
        }
        "workspace_select" => {
            let session_id = require_gateway_session(session_id)?;
            let workspace_id = required_string(args, "workspace_id")?;
            let request = state
                .select_workspace(session_id, workspace_id)
                .map_err(|error| error.to_rpc_error())?;
            let instructions = truncate_utf8(&request.tools.current_ai_instructions(), 16 * 1024);
            let structured = tool_ok(json!({
                "session_id": session_id,
                "workspace": crate::mcp::gateway::WorkspaceDescriptor::from(&request.profile),
                "tool_profile": request.tools.tool_profile.as_str(),
                "permission_mode": request.tools.policy.permission_mode.as_str(),
                "history_recording": request.tools.history_recording,
                "agent_instructions": instructions,
                "message": "Workspace 已绑定到当前 MCP session；后续普通工具调用将使用该 Workspace。"
            }));
            Ok((wrap_mcp_tool_result(name, args, structured), Some(request)))
        }
        "workspace_invoke" => {
            let session_id = require_gateway_session(session_id)?;
            let workspace_id = required_string(args, "workspace_id")?;
            let nested_name = required_string(args, "tool")?;
            if is_gateway_tool(nested_name) {
                return Err(GatewayError::invalid(
                    "NESTED_GATEWAY_TOOL_NOT_ALLOWED",
                    "workspace_invoke 只能调用普通 Workspace 工具，不能递归调用 workspace_* 工具。",
                )
                .to_rpc_error());
            }
            let request = state
                .workspace_by_id(session_id, workspace_id)
                .map_err(|error| error.to_rpc_error())?;
            let canonical_name = crate::tools::registry::canonical_tool_name(nested_name);
            ensure_gateway_tool_allowed(&request, canonical_name)?;
            let mut nested_args = args
                .get("arguments")
                .cloned()
                .filter(Value::is_object)
                .unwrap_or_else(|| json!({}));
            // The outer workspace_id is the authoritative scope for workspace_invoke.
            // Ignore a nested scope field so it can never redirect the request.
            remove_workspace_scope(&mut nested_args)?;
            let structured = call_tool(request.tools.as_ref(), canonical_name, &nested_args);
            let mut result = wrap_mcp_tool_result(canonical_name, &nested_args, structured);
            if let Some(object) = result
                .get_mut("structuredContent")
                .and_then(Value::as_object_mut)
            {
                object.insert(
                    "gateway_workspace".into(),
                    json!({
                        "id": request.profile.id,
                        "name": request.profile.name,
                        "active_workspace_changed": false
                    }),
                );
            }
            request.tools.record_context_block("tool_return", &result);
            Ok((result, Some(request)))
        }
        _ => Err(GatewayError::invalid(
            "UNKNOWN_GATEWAY_TOOL",
            format!("Unknown gateway tool: {name}"),
        )
        .to_rpc_error()),
    }
}

fn ensure_gateway_tool_allowed(request: &WorkspaceRequestContext, name: &str) -> Result<(), Value> {
    let gateway_surface = crate::tools::registry::exposed_tool_names("compact");
    let workspace_surface =
        crate::tools::registry::exposed_tool_names(request.tools.tool_profile.as_str());
    if gateway_surface.contains(&name) && workspace_surface.contains(&name) {
        return Ok(());
    }
    Err(GatewayError::invalid(
        "TOOL_NOT_AVAILABLE_IN_WORKSPACE",
        format!(
            "工具 {name} 不在当前 Workspace 的可用工具档位 {} 中。",
            request.tools.tool_profile.as_str()
        ),
    )
    .to_rpc_error())
}

fn require_gateway_session(session_id: Option<&str>) -> Result<&str, Value> {
    session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            GatewayError::invalid(
                "MCP_SESSION_REQUIRED",
                "当前请求缺少 MCP session 标识，无法安全维护 Workspace 选择。请刷新 MCP 连接后重试。",
            )
            .to_rpc_error()
        })
}

fn required_string<'a>(args: &'a Value, key: &str) -> Result<&'a str, Value> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            GatewayError::invalid(
                "INVALID_ARGUMENT",
                format!("Missing or empty argument: {key}"),
            )
            .to_rpc_error()
        })
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n…[truncated]", &value[..end])
}

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
            "configured": configured,
            "skills": skill_catalog
        }),
    );
    if !history_context.is_empty() {
        state.record_context_block("history_snapshot", &Value::String(history_context.clone()));
    }
    let instructions = [
        base_instructions,
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
    if name.starts_with("history_session_") {
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
mod tests {
    use std::fs;
    use std::sync::Arc;

    use serde_json::json;

    use crate::tools::ToolContext;

    use super::{
        handle_request, initialize_result, negotiate_protocol_version, tool_arguments,
        DEFAULT_PROTOCOL_VERSION, LATEST_PROTOCOL_VERSION, SUPPORTED_PROTOCOL_VERSIONS,
    };

    fn test_context() -> ToolContext {
        let workspace = tempfile::tempdir().expect("workspace");
        let harness = tempfile::tempdir().expect("harness");
        ToolContext::for_test(workspace.keep(), harness.keep()).expect("context")
    }

    fn test_state() -> Arc<ToolContext> {
        Arc::new(test_context())
    }

    #[test]
    fn compact_initialize_does_not_require_history_bootstrap() {
        let state = Arc::new(
            test_context()
                .with_tool_profile("compact")
                .with_history_config(true, Vec::new())
                .with_agent_runtime(Vec::new(), String::new()),
        );
        let initialized = initialize_result(&state, DEFAULT_PROTOCOL_VERSION);
        let instructions = initialized["instructions"].as_str().expect("instructions");
        assert!(instructions.contains("history bootstrap is optional"));
        assert!(instructions.contains("Workspace Root as the stable path base"));
        assert!(instructions.contains("exec_command workdir"));
        assert!(!instructions.contains("task_manage"));
        assert!(!instructions.contains("exactly once"));
        assert!(!instructions.contains("required conversation initialization"));
    }

    #[test]
    fn legacy_initialize_keeps_the_history_persistence_workflow() {
        let state = test_state();
        let initialized = initialize_result(&state, DEFAULT_PROTOCOL_VERSION);
        let instructions = initialized["instructions"].as_str().expect("instructions");
        assert!(instructions.contains("history_session_bootstrap"));
        assert!(instructions.contains("At the start of every new ChatGPT conversation"));
        assert!(instructions.contains("before answering the user's first request"));
        assert!(instructions.contains("required conversation initialization"));
        assert!(instructions.contains("initial_user_input"));
        assert!(instructions.contains("must not create duplicates"));
        assert!(instructions.contains("history_session_checkpoint"));
        assert!(instructions.contains("raw_user_input"));
        assert!(instructions.contains("history_session_search"));
        assert!(instructions.contains("history_session_read"));
        assert!(instructions.contains("follow next_cursor"));
        assert!(instructions.contains("session_key and current_path returned by bootstrap"));
        assert!(instructions.contains("session_key and expected_path"));
        assert!(instructions.contains("After completing each user-requested task"));
        assert!(instructions.contains("before the final response"));
        assert!(instructions.contains("checkpoint returns ok=true"));
        assert!(instructions.contains("not automatic background persistence"));
        assert!(instructions.contains("do not request a separate permission grant"));
        assert!(instructions.contains("confirm=true"));
        assert!(instructions.contains("planning_context"));
        assert!(instructions.contains("controlled exclusively by the desktop app"));
    }

    #[test]
    fn initialize_does_not_claim_tool_catalog_notifications_without_a_stream() {
        let state = test_state();
        let initialized = initialize_result(&state, DEFAULT_PROTOCOL_VERSION);

        assert_eq!(initialized["capabilities"]["tools"]["listChanged"], false);
    }

    #[test]
    fn initialize_appends_configured_agent_instructions() {
        let state = Arc::new(
            test_context().with_agent_runtime(Vec::new(), "Global rule\n\nWorkspace rule".into()),
        );
        let initialized = initialize_result(&state, DEFAULT_PROTOCOL_VERSION);
        let instructions = initialized["instructions"].as_str().expect("instructions");

        assert!(instructions.contains("Configured agent instructions"));
        assert!(instructions.contains("Global rule"));
        assert!(instructions.contains("Workspace rule"));
    }

    #[test]
    fn protocol_version_negotiation_matrix() {
        assert!(!SUPPORTED_PROTOCOL_VERSIONS.contains(&"2026-07-28"));
        assert_eq!(LATEST_PROTOCOL_VERSION, "2025-11-25");
        assert_eq!(negotiate_protocol_version(Some("2024-11-05")), "2024-11-05");
        assert_eq!(negotiate_protocol_version(Some("2025-06-18")), "2025-06-18");
        assert_eq!(negotiate_protocol_version(Some("2025-11-25")), "2025-11-25");
        // 未知新版本：回退到服务端最新版本，由客户端决定是否继续。
        assert_eq!(
            negotiate_protocol_version(Some("2099-01-01")),
            LATEST_PROTOCOL_VERSION
        );
        // 未知旧版本：同样回退到服务端最新版本。
        assert_eq!(
            negotiate_protocol_version(Some("2000-01-01")),
            LATEST_PROTOCOL_VERSION
        );
        // 缺省或空值：沿用历史默认版本。
        assert_eq!(negotiate_protocol_version(None), DEFAULT_PROTOCOL_VERSION);
        assert_eq!(
            negotiate_protocol_version(Some("  ")),
            DEFAULT_PROTOCOL_VERSION
        );
    }

    #[test]
    fn initialize_echoes_the_negotiated_protocol_version() {
        let state = test_state();
        let response = handle_request(
            &state,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": "2025-11-25"}
            }),
        );
        assert_eq!(response["result"]["protocolVersion"], "2025-11-25");

        let without_version = handle_request(
            &state,
            &json!({"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {}}),
        );
        assert_eq!(
            without_version["result"]["protocolVersion"],
            DEFAULT_PROTOCOL_VERSION
        );
    }

    #[test]
    fn workspace_prompt_uses_lazy_history_workflow() {
        let component = include_str!("../../../src/components/workspace/ChatGptSessionPrompt.vue");

        assert!(component.contains("ChatGPT 新会话启动提示词"));
        assert!(component.contains("不需要强制调用 history_session_bootstrap"));
        assert!(component.contains("raw_user_input"));
        assert!(component.contains("history_session_search"));
        assert!(component.contains("history_session_checkpoint"));
        assert!(!component.contains("打开连接器设置"));
    }

    #[test]
    fn chatgpt_session_metadata_is_injected_only_for_history_tools() {
        let params = json!({
            "arguments": {"session_key": "explicit"},
            "_meta": {"openai/session": "chatgpt-conversation"}
        });
        let history = tool_arguments("history_session_bootstrap", &params);
        assert_eq!(history["session_key"], "explicit");
        assert_eq!(history["_host_session_key"], "chatgpt-conversation");

        let existing = tool_arguments("read_file", &params);
        assert_eq!(existing["session_key"], "explicit");
        assert!(existing.get("_host_session_key").is_none());
    }

    #[test]
    fn host_session_key_takes_precedence_over_explicit_session_key() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let harness = tempfile::tempdir().expect("harness tempdir");
        let state = Arc::new(
            ToolContext::for_test(workspace.path().to_path_buf(), harness.path().to_path_buf())
                .expect("tool context"),
        );
        let response = handle_request(
            &state,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "history_session_bootstrap",
                    "arguments": {
                        "session_key": "explicit-session",
                        "initial_user_input": "保存首轮原文"
                    },
                    "_meta": {"openai/session": "chatgpt-session"}
                }
            }),
        );
        let structured = &response["result"]["structuredContent"];
        assert_eq!(structured["ok"], true);
        assert_eq!(structured["session_key_source"], "platform_conversation_id");
        assert_eq!(structured["session_key"], "chatgpt-session");
        assert_eq!(structured["initial_input_captured"], true);
        let content = fs::read_to_string(workspace.path().join("docs/history-session/1.md"))
            .expect("read history file");
        assert!(content.contains("**Session key:** chatgpt-session"));
        assert!(!content.contains("**Session key:** explicit-session"));
    }

    #[test]
    fn legacy_grep_calls_are_mapped_to_the_public_grep_text_tool() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let harness = tempfile::tempdir().expect("harness tempdir");
        fs::write(workspace.path().join("sample.txt"), "catalog needle")
            .expect("write sample file");
        let state = Arc::new(
            ToolContext::for_test(workspace.path().to_path_buf(), harness.path().to_path_buf())
                .expect("tool context"),
        );

        let response = handle_request(
            &state,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "grep",
                    "arguments": {"query": "needle", "path": "."}
                }
            }),
        );

        assert!(response.get("error").is_none());
        assert_eq!(response["result"]["structuredContent"]["ok"], true);
    }
}
