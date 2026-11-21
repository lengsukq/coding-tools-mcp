use axum::http::HeaderMap;
use serde_json::{json, Value};

#[cfg(test)]
pub use super::gateway_state::GatewaySessionStore;
pub use super::gateway_state::{
    GatewayError, GatewayState, WorkspaceDescriptor, WorkspaceRegistry, WorkspaceRequestContext,
};

pub const MCP_SESSION_HEADER: &str = "mcp-session-id";
const MAX_SESSION_ID_BYTES: usize = 256;

pub fn session_id_from_request(headers: &HeaderMap, body: &Value) -> Option<String> {
    if let Some(value) = headers
        .get(MCP_SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return normalize_session_id(value);
    }

    session_id_from_metadata(body)
}

pub fn session_id_from_metadata(body: &Value) -> Option<String> {
    let params = body.get("params");
    let metadata = params.and_then(|params| params.get("_meta"));
    ["openai/session", "openai/session_id", "session_id"]
        .into_iter()
        .find_map(|key| {
            metadata
                .and_then(|meta| meta.get(key))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .and_then(normalize_session_id)
        })
}

fn normalize_session_id(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_SESSION_ID_BYTES || value.chars().any(char::is_control)
    {
        return None;
    }
    Some(value.to_string())
}

pub fn gateway_tool_definitions() -> Vec<Value> {
    vec![
        gateway_tool(
            "device_list",
            "List LAN devices",
            "List this host and all paired LAN devices (macOS / Windows / Linux), including online status, OS, workspace counts, and Computer Use availability.",
            json!({ "type": "object", "properties": {}, "additionalProperties": false }),
            true,
        ),
        gateway_tool(
            "workspace_list",
            "List workspaces",
            "List workspaces registered in Coding Tools MCP (including paired LAN peers scoped as {device_id}:{workspace_id}). Use this before selecting a project context.",
            json!({ "type": "object", "properties": {}, "additionalProperties": false }),
            true,
        ),
        gateway_tool(
            "workspace_current",
            "Current workspace",
            "Return the active Workspace for this MCP session. Sessions are isolated from each other.",
            json!({ "type": "object", "properties": {}, "additionalProperties": false }),
            true,
        ),
        gateway_tool(
            "workspace_select",
            "Select workspace",
            "Select the active Workspace (local or paired LAN {device_id}:{workspace_id}) for this MCP session. This never changes another Chat session.",
            json!({
                "type": "object",
                "properties": {
                    "workspace_id": { "type": "string", "description": "Workspace id returned by workspace_list (local id or {device_id}:{workspace_id} for paired LAN computers)." }
                },
                "required": ["workspace_id"],
                "additionalProperties": false
            }),
            false,
        ),
        gateway_tool(
            "workspace_invoke",
            "Invoke in workspace",
            "Run one ordinary Coding Tools MCP tool against an explicitly selected registered Workspace (local or paired LAN {device_id}:{workspace_id}) without changing the session's active Workspace.",
            json!({
                "type": "object",
                "properties": {
                    "workspace_id": { "type": "string", "description": "Workspace id returned by workspace_list (local id or {device_id}:{workspace_id})." },
                    "tool": { "type": "string", "description": "Ordinary Coding Tools MCP tool name, for example read_file or git_status." },
                    "arguments": { "type": "object", "description": "Arguments passed to the nested tool." }
                },
                "required": ["workspace_id", "tool"],
                "additionalProperties": false
            }),
            false,
        ),
    ]
}

pub fn is_gateway_tool(name: &str) -> bool {
    matches!(
        name,
        "device_list"
            | "workspace_list"
            | "workspace_current"
            | "workspace_select"
            | "workspace_invoke"
    )
}

fn gateway_tool(
    name: &str,
    title: &str,
    description: &str,
    input_schema: Value,
    read_only: bool,
) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": input_schema,
        "annotations": {
            "title": title,
            "readOnlyHint": read_only,
            "destructiveHint": false,
            "idempotentHint": read_only,
            "openWorldHint": false
        }
    })
}

#[cfg(test)]
#[path = "gateway_handlers_tests.rs"]
mod tests;
