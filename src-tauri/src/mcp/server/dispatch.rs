use super::*;
use crate::lan_peer::{
    federated_device_list, federated_workspace_descriptors, format_remote_workspace_scope,
    forward_computer_tool, forward_workspace_select, forward_workspace_tool,
    identity_from_settings, parse_workspace_scope, PeerDeviceMetaDto, WorkspaceTarget,
};

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

struct DispatchOutcome {
    result: Value,
    workspace: Option<WorkspaceRequestContext>,
    federated_workspace_id: Option<String>,
    execution_host: Option<String>,
}

/// Handle one request for the 0.3 global MCP endpoint.
///
/// Workspace resolution happens before ordinary tool dispatch and returns an
/// immutable request-scoped ToolContext (or routes across a single authenticated
/// LAN hop to a paired peer). A concurrent workspace_select call can therefore
/// only affect the *next* request from that MCP session.
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
                federated_workspace_id: None,
                execution_host: None,
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
            federated_workspace_id: None,
            execution_host: None,
        };
    }

    let mut workspace = None;
    let mut federated_workspace_id = None;
    let mut execution_host = None;
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
        "tools/call" => match handle_gateway_tools_call(state, session_id, &id, &params) {
            Ok(outcome) => {
                workspace = outcome.workspace;
                federated_workspace_id = outcome.federated_workspace_id;
                execution_host = outcome.execution_host;
                Ok(outcome.result)
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
    GatewayResponse {
        body,
        workspace,
        federated_workspace_id,
        execution_host,
    }
}

fn gateway_initialize_result(state: &GatewayState, protocol_version: &str) -> Value {
    let registry_revision = state.registry.revision().unwrap_or_default();
    let instructions = "Coding Tools MCP is a local-first coding runtime exposed through one Global MCP connection with optional LAN peer federation across multiple computers (macOS / Windows / Linux). The user-facing workflow is Device/Workspace → Goal/Plan → Execution → History. Workspace data is isolated per device and per workspace: call workspace_list (and device_list when checking connected computers) when the target is unknown, then workspace_select for stateful clients that preserve the same MCP transport session. Remote paired workspaces use composite IDs of the form {device_id}:{workspace_id}; always verify the device metadata before mutating when multiple computers share identical project names. Do not infer a filesystem root or use the desktop UI selection as routing authority. If the client does not preserve MCP session state, or an ordinary tool reports WORKSPACE_NOT_SELECTED after selection, use workspace_invoke or pass explicit workspace_id for that operation instead of guessing or repeatedly selecting. For Computer Use tools (computer_session, computer_observe, computer_action) in a multi-device LAN setup, pass device_id from device_list to target a specific computer's screen. Keep Planning as the durable source of intent and progress, Execution Ledger/checkpoints as the source of runtime progress and failures, Verification evidence as the completion gate, and History as the cross-session recovery record. When resuming existing work, continue the active Goal/Plan and its current execution checkpoint before creating duplicate work. workspace_invoke is one-off and never changes another request or Chat session. When a successful mutation returns review_url, surface that exact URL as the preferred inspection link; never reconstruct it or expose review_token separately. When the user asks for all current workspace changes, a PR-style overall diff, or all changes from the current chat/session, call change_review with scope=workspace or scope=session and surface the returned review_url.";
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
            "gatewayMode": "multi-workspace-lan-federated"
        }
    })
}

fn handle_gateway_tools_call(
    state: &GatewayState,
    session_id: Option<&str>,
    request_id: &Value,
    params: &Value,
) -> Result<DispatchOutcome, Value> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| json!({ "code": -32602, "message": "Missing tool name" }))?;
    let mut args = tool_arguments(name, params);

    if matches!(
        name,
        "computer_session" | "computer_observe" | "computer_action"
    ) {
        return handle_computer_tool(state, session_id, request_id, name, &mut args);
    }

    if is_gateway_tool(name) {
        return handle_workspace_tool(state, session_id, request_id, name, &args);
    }

    let resolved = resolve_tool_workspace(state, session_id, &mut args)?;
    let canonical_name = crate::tools::registry::canonical_tool_name(name);
    let logical_session_id = params
        .get("_meta")
        .and_then(|meta| meta.get("openai/session"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| session_id.map(str::to_string));

    match resolved {
        ResolvedToolWorkspace::Local(request) => {
            ensure_gateway_tool_allowed(&request, canonical_name)?;
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
            Ok(DispatchOutcome {
                result,
                workspace: Some(request),
                federated_workspace_id: None,
                execution_host: Some("local".into()),
            })
        }
        ResolvedToolWorkspace::Remote {
            session_id: fallback_session,
            device_id,
            workspace_id,
            qualified_id,
        } => {
            let gateway_surface = crate::tools::registry::exposed_tool_names("compact");
            if !gateway_surface.contains(&canonical_name) {
                return Err(GatewayError::invalid(
                    "TOOL_NOT_AVAILABLE_IN_WORKSPACE",
                    format!("工具 {canonical_name} 不在 Global MCP 可用工具列表中。"),
                )
                .to_rpc_error());
            }
            let override_snap = state.settings_snapshot();
            let effective_session = logical_session_id
                .as_deref()
                .unwrap_or(fallback_session.as_str());
            let (peer, result) = forward_workspace_tool(
                override_snap.as_ref(),
                &device_id,
                &workspace_id,
                canonical_name,
                &args,
                Some(effective_session),
                request_id,
            )?;
            Ok(DispatchOutcome {
                result,
                workspace: None,
                federated_workspace_id: Some(qualified_id),
                execution_host: Some(format!("peer:{} ({})", peer.device_id, peer.device_name)),
            })
        }
    }
}

fn handle_computer_tool(
    state: &GatewayState,
    session_id: Option<&str>,
    request_id: &Value,
    name: &str,
    args: &mut Value,
) -> Result<DispatchOutcome, Value> {
    if !session_id.is_some_and(|id| state.sessions.is_server_issued(id)) {
        return Ok(DispatchOutcome {
            result: json!({
                "isError": true,
                "content": [{ "type": "text", "text": "COMPUTER_SESSION_REQUIRED: Computer Use 仅接受服务器签发的 MCP 会话。请重新 initialize 并保留 mcp-session-id 请求头。" }],
                "structuredContent": { "ok": false, "error": "COMPUTER_SESSION_REQUIRED" }
            }),
            workspace: None,
            federated_workspace_id: None,
            execution_host: None,
        });
    }
    let session = session_id.expect("verified server-issued session");
    let explicit_device = if let Some(object) = args.as_object_mut() {
        object.remove("session_token");
        object
            .remove("device_id")
            .and_then(|v| v.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
    } else {
        None
    };

    let override_snap = state.settings_snapshot();
    let (lan, _) = identity_from_settings(override_snap.as_ref());

    if let Some(target_dev) = explicit_device {
        if target_dev != "local" && target_dev != "self" && target_dev != lan.device_id {
            let (peer, result) = forward_computer_tool(
                override_snap.as_ref(),
                &target_dev,
                name,
                args,
                session,
                request_id,
            )?;
            return Ok(DispatchOutcome {
                result,
                workspace: None,
                federated_workspace_id: None,
                execution_host: Some(format!("peer:{} ({})", peer.device_id, peer.device_name)),
            });
        }
    } else if lan.enabled
        && lan
            .paired_peers
            .iter()
            .any(|p| p.enabled)
    {
        let local_ws_count = state.registry.list().map(|v| v.len()).unwrap_or_default();
        let devices = federated_device_list(local_ws_count, override_snap.as_ref());
        let remote_cu: Vec<_> = devices
            .iter()
            .filter(|d| !d.is_local && d.online && d.computer_use_enabled)
            .collect();
        let local_cu = cfg!(target_os = "macos") && crate::computer::local_policy_present();
        if !remote_cu.is_empty() && (local_cu || !cfg!(target_os = "macos")) {
            let mut candidates = Vec::new();
            if cfg!(target_os = "macos") {
                candidates.push(format!("local ({})", lan.device_name));
            }
            for peer in remote_cu {
                candidates.push(format!("{} ({})", peer.device_id, peer.device_name));
            }
            let message = format!(
                "COMPUTER_DEVICE_AMBIGUOUS: 检测到局域网存在可用的远端屏幕控制设备，请在调用 {name} 时显式传入 device_id（可选设备: {}；也可先调用 device_list 查看）。",
                candidates.join(", ")
            );
            return Ok(DispatchOutcome {
                result: json!({
                    "isError": true,
                    "content": [{ "type": "text", "text": message }],
                    "structuredContent": {
                        "ok": false,
                        "error": "COMPUTER_DEVICE_AMBIGUOUS",
                        "candidates": candidates,
                    }
                }),
                workspace: None,
                federated_workspace_id: None,
                execution_host: None,
            });
        }
    }

    Ok(DispatchOutcome {
        result: crate::computer::call_tool(name, Some(session), args),
        workspace: None,
        federated_workspace_id: None,
        execution_host: Some("local".into()),
    })
}

fn handle_workspace_tool(
    state: &GatewayState,
    session_id: Option<&str>,
    request_id: &Value,
    name: &str,
    args: &Value,
) -> Result<DispatchOutcome, Value> {
    use crate::tools::workspace::tool_ok;

    let override_snap = state.settings_snapshot();
    let (lan, _) = identity_from_settings(override_snap.as_ref());

    match name {
        "device_list" => {
            let local_count = state
                .registry
                .local_descriptors()
                .map(|v| v.len())
                .unwrap_or_default();
            let devices = federated_device_list(local_count, override_snap.as_ref());
            let structured = tool_ok(json!({
                "local_device_id": lan.device_id,
                "count": devices.len(),
                "devices": devices,
            }));
            Ok(DispatchOutcome {
                result: wrap_mcp_tool_result(name, args, structured),
                workspace: None,
                federated_workspace_id: None,
                execution_host: Some("local".into()),
            })
        }
        "workspace_list" => {
            let local_workspaces = state.registry.local_descriptors().map_err(|message| {
                GatewayError::internal("WORKSPACE_REGISTRY_UNAVAILABLE", message).to_rpc_error()
            })?;
            let revision = state.registry.revision().unwrap_or_default();
            let has_enabled_peers = lan.enabled && lan.paired_peers.iter().any(|p| p.enabled);
            let structured = if has_enabled_peers {
                let (workspaces, devices) = federated_workspace_descriptors(
                    local_workspaces,
                    override_snap.as_ref(),
                );
                tool_ok(json!({
                    "workspaces": workspaces,
                    "count": workspaces.len(),
                    "registry_revision": revision,
                    "devices": devices,
                }))
            } else {
                tool_ok(json!({
                    "workspaces": local_workspaces,
                    "count": local_workspaces.len(),
                    "registry_revision": revision
                }))
            };
            Ok(DispatchOutcome {
                result: wrap_mcp_tool_result(name, args, structured),
                workspace: None,
                federated_workspace_id: None,
                execution_host: Some("local".into()),
            })
        }
        "workspace_current" => {
            let session_id = require_gateway_session(session_id)?;
            let scope = state.sessions.current(session_id);
            let workspace = match scope.active_workspace_id {
                Some(id) => match parse_workspace_scope(&id, &lan.device_id) {
                    Ok(WorkspaceTarget::Local(local_id)) => match state.registry.resolve(&local_id) {
                        Ok(profile) => {
                            Some(crate::mcp::gateway::WorkspaceDescriptor::from(&profile))
                        }
                        Err(_) => {
                            state.sessions.clear_selection(session_id);
                            None
                        }
                    },
                    Ok(WorkspaceTarget::Remote {
                        device_id,
                        workspace_id,
                    }) => match lan.paired_peers.iter().find(|p| p.device_id == device_id && p.enabled) {
                        Some(peer) => {
                            let (all_ws, _) = federated_workspace_descriptors(
                                Vec::new(),
                                override_snap.as_ref(),
                            );
                            let qualified = format_remote_workspace_scope(&device_id, &workspace_id);
                            Some(
                                all_ws
                                    .into_iter()
                                    .find(|w| w.id == qualified)
                                    .unwrap_or_else(|| crate::mcp::gateway::WorkspaceDescriptor {
                                        id: qualified,
                                        name: workspace_id.clone(),
                                        path: String::new(),
                                        qualified_name: Some(format!(
                                            "[远端 · {}] {workspace_id}",
                                            peer.device_name
                                        )),
                                        device: Some(PeerDeviceMetaDto {
                                            device_id: peer.device_id.clone(),
                                            device_name: peer.device_name.clone(),
                                            is_local: false,
                                            os: peer.os.clone(),
                                            arch: peer.arch.clone(),
                                            online: false,
                                            access_mode: "full".into(),
                                        }),
                                    }),
                            )
                        }
                        None => {
                            state.sessions.clear_selection(session_id);
                            None
                        }
                    },
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
            Ok(DispatchOutcome {
                result: wrap_mcp_tool_result(name, args, structured),
                workspace: None,
                federated_workspace_id: None,
                execution_host: Some("local".into()),
            })
        }
        "workspace_select" => {
            let session_id = require_gateway_session(session_id)?;
            let raw_workspace_id = required_string(args, "workspace_id")?;
            let target = parse_workspace_scope(raw_workspace_id, &lan.device_id)
                .map_err(|e| e.to_rpc_error())?;

            match target {
                WorkspaceTarget::Local(local_id) => {
                    let request = state
                        .select_workspace(session_id, &local_id)
                        .map_err(|error| error.to_rpc_error())?;
                    let instructions =
                        truncate_utf8(&request.tools.current_ai_instructions(), 16 * 1024);
                    let structured = tool_ok(json!({
                        "session_id": session_id,
                        "workspace": crate::mcp::gateway::WorkspaceDescriptor::from(&request.profile),
                        "tool_profile": request.tools.tool_profile.as_str(),
                        "permission_mode": request.tools.policy.permission_mode.as_str(),
                        "history_recording": request.tools.history_recording,
                        "agent_instructions": instructions,
                        "message": "Workspace 已绑定到当前 MCP session；后续普通工具调用将使用该 Workspace。"
                    }));
                    Ok(DispatchOutcome {
                        result: wrap_mcp_tool_result(name, args, structured),
                        workspace: Some(request),
                        federated_workspace_id: None,
                        execution_host: Some("local".into()),
                    })
                }
                WorkspaceTarget::Remote {
                    device_id,
                    workspace_id,
                } => {
                    if !state.sessions.contains(session_id) {
                        return Err(GatewayError::invalid(
                            "MCP_SESSION_INVALID",
                            "MCP session 不存在或已过期。请重新建立 MCP 连接并使用服务端返回的 session 标识。",
                        )
                        .to_rpc_error());
                    }
                    let (descriptor, probe) = forward_workspace_select(
                        override_snap.as_ref(),
                        session_id,
                        &device_id,
                        &workspace_id,
                    )?;
                    let qualified_id = descriptor.id.clone();
                    if !state.sessions.select(session_id, qualified_id.clone()) {
                        return Err(GatewayError::invalid(
                            "MCP_SESSION_INVALID",
                            "MCP session 不存在或已过期。请重新建立 MCP 连接并使用服务端返回的 session 标识。",
                        )
                        .to_rpc_error());
                    }
                    let peer_name = descriptor
                        .device
                        .as_ref()
                        .map(|d| d.device_name.clone())
                        .unwrap_or_else(|| device_id.clone());
                    let structured = tool_ok(json!({
                        "session_id": session_id,
                        "workspace": descriptor,
                        "tool_profile": probe["tool_profile"].as_str().unwrap_or("compact"),
                        "permission_mode": probe["permission_mode"].as_str().unwrap_or("trusted"),
                        "history_recording": probe["history_recording"].as_bool().unwrap_or(false),
                        "agent_instructions": probe["agent_instructions"].as_str().unwrap_or(""),
                        "message": format!("远端配对设备 {peer_name} 的 Workspace 已绑定到当前 MCP session；后续普通工具调用将自动单跳路由至该设备。")
                    }));
                    Ok(DispatchOutcome {
                        result: wrap_mcp_tool_result(name, args, structured),
                        workspace: None,
                        federated_workspace_id: Some(qualified_id),
                        execution_host: Some(format!("peer:{device_id} ({peer_name})")),
                    })
                }
            }
        }
        "workspace_invoke" => {
            let session_id = require_gateway_session(session_id)?;
            let raw_workspace_id = required_string(args, "workspace_id")?;
            let nested_name = required_string(args, "tool")?;
            if is_gateway_tool(nested_name) {
                return Err(GatewayError::invalid(
                    "NESTED_GATEWAY_TOOL_NOT_ALLOWED",
                    "workspace_invoke 只能调用普通 Workspace 工具，不能递归调用 workspace_* 或 device_list 工具。",
                )
                .to_rpc_error());
            }
            let target = parse_workspace_scope(raw_workspace_id, &lan.device_id)
                .map_err(|e| e.to_rpc_error())?;
            let canonical_name = crate::tools::registry::canonical_tool_name(nested_name);
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

            match target {
                WorkspaceTarget::Local(local_id) => {
                    let request = state
                        .workspace_by_id(session_id, &local_id)
                        .map_err(|error| error.to_rpc_error())?;
                    ensure_gateway_tool_allowed(&request, canonical_name)?;
                    if canonical_name == "history_manage" {
                        nested_args["_host_session_key"] =
                            Value::String(logical_session_id.to_string());
                    }
                    if canonical_name == "change_review"
                        && nested_args.get("scope").and_then(Value::as_str) == Some("session")
                        && nested_args.get("session_id").is_none()
                    {
                        nested_args["_review_session_id"] =
                            Value::String(logical_session_id.to_string());
                    }
                    let structured =
                        call_tool(request.tools.as_ref(), canonical_name, &nested_args);
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
                    Ok(DispatchOutcome {
                        result,
                        workspace: Some(request),
                        federated_workspace_id: None,
                        execution_host: Some("local".into()),
                    })
                }
                WorkspaceTarget::Remote {
                    device_id,
                    workspace_id,
                } => {
                    let gateway_surface = crate::tools::registry::exposed_tool_names("compact");
                    if !gateway_surface.contains(&canonical_name) {
                        return Err(GatewayError::invalid(
                            "TOOL_NOT_AVAILABLE_IN_WORKSPACE",
                            format!("工具 {canonical_name} 不在 Global MCP 可用工具列表中。"),
                        )
                        .to_rpc_error());
                    }
                    let qualified_id = format_remote_workspace_scope(&device_id, &workspace_id);
                    let (peer, mut result) = forward_workspace_tool(
                        override_snap.as_ref(),
                        &device_id,
                        &workspace_id,
                        canonical_name,
                        &nested_args,
                        Some(logical_session_id),
                        request_id,
                    )?;
                    if let Some(object) = result
                        .get_mut("structuredContent")
                        .and_then(Value::as_object_mut)
                    {
                        object.insert(
                            "gateway_workspace".into(),
                            json!({
                                "id": qualified_id,
                                "deviceId": peer.device_id,
                                "deviceName": peer.device_name,
                                "active_workspace_changed": false
                            }),
                        );
                    }
                    Ok(DispatchOutcome {
                        result,
                        workspace: None,
                        federated_workspace_id: Some(qualified_id),
                        execution_host: Some(format!(
                            "peer:{} ({})",
                            peer.device_id, peer.device_name
                        )),
                    })
                }
            }
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
