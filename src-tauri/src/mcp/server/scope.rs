use super::*;
use crate::lan_peer::{
    federated_workspace_descriptors, format_remote_workspace_scope, identity_from_settings,
    parse_workspace_scope, WorkspaceTarget,
};
use crate::settings::AppSettings;

pub(super) enum ResolvedToolWorkspace {
    Local(WorkspaceRequestContext),
    Remote {
        session_id: String,
        device_id: String,
        workspace_id: String,
        qualified_id: String,
    },
}

fn self_device_id(state: &GatewayState) -> (AppSettings, String) {
    let override_snap = state.settings_snapshot();
    let settings = override_snap
        .clone()
        .unwrap_or_else(AppSettings::load_or_default);
    let (lan, _) = identity_from_settings(override_snap.as_ref());
    (settings, lan.device_id)
}

/// Best-effort routing attribution for a tools/call audit record. This helper
/// must not mutate session state or influence normal request routing.
pub fn audit_workspace_hint(
    state: &GatewayState,
    session_id: Option<&str>,
    body: &Value,
) -> Option<String> {
    if body.get("method").and_then(Value::as_str) != Some("tools/call") {
        return None;
    }
    let params = body.get("params")?;
    let tool_name = params.get("name").and_then(Value::as_str)?;
    if matches!(
        tool_name,
        "device_list"
            | "workspace_list"
            | "computer_session"
            | "computer_observe"
            | "computer_action"
    ) {
        return None;
    }

    let (settings, self_id) = self_device_id(state);
    let explicit_workspace = params
        .get("arguments")
        .and_then(|arguments| arguments.get("workspace_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|workspace_id| !workspace_id.is_empty());
    if let Some(workspace_id) = explicit_workspace {
        return match parse_workspace_scope(workspace_id, &self_id).ok()? {
            WorkspaceTarget::Local(local_id) => state
                .registry
                .resolve(&local_id)
                .ok()
                .map(|profile| profile.id),
            WorkspaceTarget::Remote {
                device_id,
                workspace_id,
            } => Some(format_remote_workspace_scope(&device_id, &workspace_id)),
        };
    }

    if let Some(workspace_id) =
        session_id.and_then(|session_id| state.sessions.current(session_id).active_workspace_id)
    {
        return match parse_workspace_scope(&workspace_id, &self_id).ok()? {
            WorkspaceTarget::Local(local_id) => state
                .registry
                .resolve(&local_id)
                .ok()
                .map(|profile| profile.id),
            WorkspaceTarget::Remote {
                device_id,
                workspace_id,
            } => Some(format_remote_workspace_scope(&device_id, &workspace_id)),
        };
    }

    let has_enabled_peers = settings.lan_federation.enabled
        && settings
            .lan_federation
            .paired_peers
            .iter()
            .any(|p| p.enabled);
    if !is_gateway_tool(tool_name) && !has_enabled_peers {
        return state
            .registry
            .list()
            .ok()
            .and_then(|profiles| match profiles.as_slice() {
                [profile] => Some(profile.id.clone()),
                _ => None,
            });
    }
    None
}

pub(super) fn gateway_scoped_tool_catalog() -> Vec<Value> {
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
            "description": "Registered Workspace id from workspace_list (local id or paired LAN {device_id}:{workspace_id}). Pass this on each ordinary tool call when the MCP client does not preserve transport sessions."
        }),
    );
    tool
}

pub(super) fn resolve_tool_workspace(
    state: &GatewayState,
    session_id: Option<&str>,
    args: &mut Value,
) -> Result<ResolvedToolWorkspace, Value> {
    let (settings, self_id) = self_device_id(state);
    if let Some(raw_scope) = remove_workspace_scope(args)? {
        let target = parse_workspace_scope(&raw_scope, &self_id).map_err(|e| e.to_rpc_error())?;
        return match target {
            WorkspaceTarget::Local(local_id) => state
                .workspace_by_id(session_id.unwrap_or("request-scoped"), &local_id)
                .map(ResolvedToolWorkspace::Local)
                .map_err(|error| error.to_rpc_error()),
            WorkspaceTarget::Remote {
                device_id,
                workspace_id,
            } => Ok(ResolvedToolWorkspace::Remote {
                session_id: session_id.unwrap_or("request-scoped").to_string(),
                qualified_id: format_remote_workspace_scope(&device_id, &workspace_id),
                device_id,
                workspace_id,
            }),
        };
    }

    if let Some(session_id) = session_id {
        if let Some(selected) = state.sessions.current(session_id).active_workspace_id {
            match parse_workspace_scope(&selected, &self_id) {
                Ok(WorkspaceTarget::Local(_)) => match state.active_workspace(session_id) {
                    Ok(request) => return Ok(ResolvedToolWorkspace::Local(request)),
                    Err(error) if error.code != "WORKSPACE_NOT_SELECTED" => {
                        return Err(error.to_rpc_error())
                    }
                    Err(_) => {}
                },
                Ok(WorkspaceTarget::Remote {
                    device_id,
                    workspace_id,
                }) => {
                    let peer_exists = settings.lan_federation.enabled
                        && settings
                            .lan_federation
                            .paired_peers
                            .iter()
                            .any(|p| p.device_id == device_id && p.enabled);
                    if !peer_exists {
                        state.sessions.clear_selection(session_id);
                        return Err(GatewayError::not_found(
                            "WORKSPACE_NOT_FOUND",
                            format!("配对设备不存在或已停用，工作区选择已失效: {selected}"),
                        )
                        .to_rpc_error());
                    }
                    return Ok(ResolvedToolWorkspace::Remote {
                        session_id: session_id.to_string(),
                        qualified_id: format_remote_workspace_scope(&device_id, &workspace_id),
                        device_id,
                        workspace_id,
                    });
                }
                Err(error) => {
                    state.sessions.clear_selection(session_id);
                    return Err(error.to_rpc_error());
                }
            }
        }
    }

    let workspaces = state.registry.list().map_err(|message| {
        GatewayError::internal("WORKSPACE_REGISTRY_UNAVAILABLE", message).to_rpc_error()
    })?;
    let has_enabled_peers = settings.lan_federation.enabled
        && settings
            .lan_federation
            .paired_peers
            .iter()
            .any(|p| p.enabled);
    if !has_enabled_peers {
        if let [profile] = workspaces.as_slice() {
            return state
                .workspace_by_id(session_id.unwrap_or("single-workspace"), &profile.id)
                .map(ResolvedToolWorkspace::Local)
                .map_err(|error| error.to_rpc_error());
        }
    } else {
        let local_desc = workspaces
            .iter()
            .map(crate::mcp::gateway::WorkspaceDescriptor::from)
            .collect();
        let override_snap = state.settings_snapshot();
        let (all_desc, _) =
            federated_workspace_descriptors(local_desc, override_snap.as_ref());
        if let [sole] = all_desc.as_slice() {
            if let Ok(target) = parse_workspace_scope(&sole.id, &self_id) {
                return match target {
                    WorkspaceTarget::Local(local_id) => state
                        .workspace_by_id(session_id.unwrap_or("single-workspace"), &local_id)
                        .map(ResolvedToolWorkspace::Local)
                        .map_err(|error| error.to_rpc_error()),
                    WorkspaceTarget::Remote {
                        device_id,
                        workspace_id,
                    } => Ok(ResolvedToolWorkspace::Remote {
                        session_id: session_id.unwrap_or("single-workspace").to_string(),
                        qualified_id: format_remote_workspace_scope(&device_id, &workspace_id),
                        device_id,
                        workspace_id,
                    }),
                };
            }
        }
    }

    Err(GatewayError::invalid(
        "WORKSPACE_NOT_SELECTED",
        "当前请求没有明确 Workspace。请先调用 workspace_list（多设备环境会同时返回本机与局域网配对电脑的工作区），并在普通工具调用中传 workspace_id；支持持久 MCP session 的客户端也可先调用 workspace_select。",
    )
    .to_rpc_error())
}

pub(super) fn remove_workspace_scope(args: &mut Value) -> Result<Option<String>, Value> {
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

pub(super) fn attach_gateway_server_info(
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
    let override_snap = state.settings_snapshot();
    let (lan, _) = identity_from_settings(override_snap.as_ref());
    structured.insert(
        "gateway".into(),
        json!({
            "mode": "global-multi-workspace",
            "session_id": request.session_id,
            "device": {
                "id": lan.device_id,
                "name": lan.device_name,
                "isLocal": true,
                "os": std::env::consts::OS,
            },
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
