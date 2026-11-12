use super::*;

// Both ordinary tool calls and workspace_invoke produce operation reviews.
// Keep the attachment behavior identical and bind it to the resolved workspace.
fn attach_review_session(
    request: &WorkspaceRequestContext,
    structured: &Value,
    session: Option<&str>,
) {
    if let (Some(session), Some(change)) =
        (session, structured.get("change_id").and_then(Value::as_str))
    {
        let _ = crate::review::attach_session(request.tools.workspace.root(), change, session);
    }
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
            tools.extend(crate::computer::tool_definitions());
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
    let instructions = "Coding Tools MCP is a local-first coding runtime exposed through one Global MCP connection. The user-facing workflow is Workspace → Goal/Plan → Execution → History. Workspace data is isolated: call workspace_list when the target is unknown, then workspace_select for stateful clients that preserve the same MCP transport session. Do not infer a filesystem root or use the desktop UI selection as routing authority. If the client does not preserve MCP session state, or an ordinary tool reports WORKSPACE_NOT_SELECTED after selection, use workspace_invoke with the explicit workspace_id for that operation instead of guessing or repeatedly selecting. Keep Planning as the durable source of intent and progress, Execution Ledger/checkpoints as the source of runtime progress and failures, Verification evidence as the completion gate, and History as the cross-session recovery record. When resuming existing work, continue the active Goal/Plan and its current execution checkpoint before creating duplicate work. workspace_invoke is one-off and never changes another request or Chat session. When a successful mutation returns review_url, surface that exact URL as the preferred inspection link; never reconstruct it or expose review_token separately. When the user asks for all current workspace changes, a PR-style overall diff, or all changes from the current chat/session, call change_review with scope=workspace or scope=session and surface the returned review_url.";
    let instructions = format!("{instructions} For a workspace where server_info reports history_recording=true, call history_manage with action=checkpoint at the end of every user-requested development task. Include the user's verbatim current request as raw_user_input, plus actual findings, changed files and tests. The first checkpoint can lazily create the history archive; bootstrap is optional. Do not claim a task was archived unless the tool confirms recorded=true. Never checkpoint when history recording is disabled; the server cannot recover text that was never sent to it.");
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

    if matches!(
        name,
        "computer_session" | "computer_observe" | "computer_action"
    ) {
        if !session_id.is_some_and(|id| state.sessions.is_server_issued(id)) {
            return Ok((
                json!({
                    "isError": true,
                    "content": [{ "type": "text", "text": "COMPUTER_SESSION_REQUIRED: Computer Use 仅接受服务器签发的 MCP 会话。请重新 initialize 并保留 mcp-session-id 请求头。" }],
                    "structuredContent": { "ok": false, "error": "COMPUTER_SESSION_REQUIRED" }
                }),
                None,
            ));
        }
        // The listener has already validated the optional stateless bearer
        // credential. Keep it out of the native Computer Use action payload.
        if let Some(object) = args.as_object_mut() {
            object.remove("session_token");
        }
        return Ok((crate::computer::call_tool(name, session_id, &args), None));
    }

    if is_gateway_tool(name) {
        return handle_workspace_tool(state, session_id, name, &args);
    }

    let request = resolve_tool_workspace(state, session_id, &mut args)?;
    let canonical_name = crate::tools::registry::canonical_tool_name(name);
    ensure_gateway_tool_allowed(&request, canonical_name)?;
    let logical_session_id = params
        .get("_meta")
        .and_then(|meta| meta.get("openai/session"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| session_id.map(str::to_string));
    if canonical_name == "history_manage" && args.get("_host_session_key").is_none() {
        if let Some(value) = logical_session_id.as_deref() {
            args["_host_session_key"] = Value::String(value.to_string());
        }
    }
    if canonical_name == "change_review"
        && args.get("scope").and_then(Value::as_str) == Some("session")
        && args.get("session_id").is_none()
    {
        if !args.is_object() {
            args = json!({});
        }
        if let Some(value) = logical_session_id.as_deref() {
            args["_review_session_id"] = Value::String(value.to_string());
        }
    }
    let structured = call_tool(request.tools.as_ref(), canonical_name, &args);
    attach_review_session(&request, &structured, logical_session_id.as_deref());
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
            let logical_session_id = args
                .get("_host_session_key")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(session_id);
            if canonical_name == "history_manage" {
                nested_args["_host_session_key"] = Value::String(logical_session_id.to_string());
            }
            if canonical_name == "change_review"
                && nested_args.get("scope").and_then(Value::as_str) == Some("session")
                && nested_args.get("session_id").is_none()
            {
                nested_args["_review_session_id"] = Value::String(logical_session_id.to_string());
            }
            let structured = call_tool(request.tools.as_ref(), canonical_name, &nested_args);
            attach_review_session(&request, &structured, Some(logical_session_id));
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
