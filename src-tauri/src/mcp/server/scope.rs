use super::*;

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
        "workspace_list" | "computer_session" | "computer_observe" | "computer_action"
    ) {
        return None;
    }

    let explicit_workspace = params
        .get("arguments")
        .and_then(|arguments| arguments.get("workspace_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|workspace_id| !workspace_id.is_empty());
    if let Some(workspace_id) = explicit_workspace {
        return state
            .registry
            .resolve(workspace_id)
            .ok()
            .map(|profile| profile.id);
    }

    if let Some(workspace_id) =
        session_id.and_then(|session_id| state.sessions.current(session_id).active_workspace_id)
    {
        return state
            .registry
            .resolve(&workspace_id)
            .ok()
            .map(|profile| profile.id);
    }

    if !is_gateway_tool(tool_name) {
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
            "description": "Registered Workspace id from workspace_list. Pass this on each ordinary tool call when the MCP client does not preserve transport sessions."
        }),
    );
    tool
}

pub(super) fn resolve_tool_workspace(
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
