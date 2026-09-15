use axum::http::HeaderMap;
use serde_json::{json, Value};

#[cfg(test)]
pub use super::gateway_state::GatewaySessionStore;
pub use super::gateway_state::{
    GatewayError, GatewayState, WorkspaceDescriptor, WorkspaceRequestContext,
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
            "workspace_list",
            "List workspaces",
            "List workspaces registered in Coding Tools MCP. Use this before selecting a project context.",
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
            "Select the active Workspace for this MCP session. This never changes another Chat session.",
            json!({
                "type": "object",
                "properties": {
                    "workspace_id": { "type": "string", "description": "Workspace id returned by workspace_list." }
                },
                "required": ["workspace_id"],
                "additionalProperties": false
            }),
            false,
        ),
        gateway_tool(
            "workspace_invoke",
            "Invoke in workspace",
            "Run one ordinary Coding Tools MCP tool against an explicitly selected registered Workspace without changing the session's active Workspace.",
            json!({
                "type": "object",
                "properties": {
                    "workspace_id": { "type": "string", "description": "Workspace id returned by workspace_list." },
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
        "workspace_list" | "workspace_current" | "workspace_select" | "workspace_invoke"
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
mod tests {
    use super::*;
    use crate::mcp::server::handle_gateway_request;
    use crate::settings::AppSettings;
    use crate::tools::call_tool;
    use crate::workspace::WorkspaceProfile;
    use std::fs;

    fn gateway_fixture() -> (tempfile::TempDir, tempfile::TempDir, GatewayState) {
        let first = tempfile::tempdir().expect("workspace a");
        let second = tempfile::tempdir().expect("workspace b");
        fs::write(first.path().join("marker.txt"), "ONLY-A").expect("marker a");
        fs::write(second.path().join("marker.txt"), "ONLY-B").expect("marker b");

        let mut a = WorkspaceProfile::new(
            first.path().display().to_string(),
            Some("Workspace A".into()),
        );
        a.id = "workspace-a".into();
        let mut b = WorkspaceProfile::new(
            second.path().display().to_string(),
            Some("Workspace B".into()),
        );
        b.id = "workspace-b".into();

        let settings = AppSettings {
            global_permission_mode: "dangerous".into(),
            global_allowed_commands: "git".into(),
            ..AppSettings::default()
        };
        let state = GatewayState::for_test_profiles(vec![a, b], settings);
        (first, second, state)
    }

    fn rpc_tool(state: &GatewayState, session: &str, name: &str, arguments: Value) -> Value {
        state.sessions.register_for_test(session);
        handle_gateway_request(
            state,
            Some(session),
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": { "name": name, "arguments": arguments }
            }),
        )
        .body
    }

    fn structured(response: &Value) -> &Value {
        &response["result"]["structuredContent"]
    }

    #[test]
    fn session_scopes_do_not_cross_talk() {
        let sessions = GatewaySessionStore::default();
        sessions.register_for_test("chat-a");
        sessions.register_for_test("chat-b");
        sessions.select("chat-a", "workspace-a".into());
        sessions.select("chat-b", "workspace-b".into());

        assert_eq!(
            sessions.current("chat-a").active_workspace_id.as_deref(),
            Some("workspace-a")
        );
        assert_eq!(
            sessions.current("chat-b").active_workspace_id.as_deref(),
            Some("workspace-b")
        );
    }

    #[test]
    fn stale_workspace_selection_is_cleared_without_invalidating_session() {
        let (_a, _b, state) = gateway_fixture();
        state.sessions.register_for_test("chat-a");
        assert!(state.sessions.select("chat-a", "deleted-workspace".into()));

        let error = match state.active_workspace("chat-a") {
            Ok(_) => panic!("missing workspace must fail"),
            Err(error) => error,
        };
        assert_eq!(error.code, "WORKSPACE_NOT_FOUND");
        assert!(state.sessions.contains("chat-a"));
        assert!(state
            .sessions
            .current("chat-a")
            .active_workspace_id
            .is_none());

        let current = rpc_tool(&state, "chat-a", "workspace_current", json!({}));
        assert_eq!(structured(&current)["selected"], false);
        assert!(state.sessions.contains("chat-a"));
    }

    #[test]
    fn request_scoped_workspace_id_survives_transport_session_churn() {
        let (_a, _b, state) = gateway_fixture();
        let first = rpc_tool(
            &state,
            "transport-a",
            "read_file",
            json!({ "workspace_id": "workspace-a", "path": "marker.txt" }),
        );
        let second = rpc_tool(
            &state,
            "transport-b",
            "read_file",
            json!({ "workspace_id": "workspace-a", "path": "marker.txt" }),
        );

        assert_eq!(structured(&first)["content"], "ONLY-A");
        assert_eq!(structured(&second)["content"], "ONLY-A");
        assert!(state
            .sessions
            .current("transport-a")
            .active_workspace_id
            .is_none());
        assert!(state
            .sessions
            .current("transport-b")
            .active_workspace_id
            .is_none());
    }

    #[test]
    fn request_scoped_workspace_id_cannot_be_redirected_by_absolute_path() {
        let (_a, b, state) = gateway_fixture();
        let response = rpc_tool(
            &state,
            "transport-a",
            "read_file",
            json!({
                "workspace_id": "workspace-a",
                "path": b.path().join("marker.txt").display().to_string()
            }),
        );

        assert_eq!(structured(&response)["ok"], false);
    }

    #[test]
    fn invalid_session_ids_are_rejected_instead_of_persisted() {
        let headers = HeaderMap::new();
        let too_long = "s".repeat(MAX_SESSION_ID_BYTES + 1);
        let long_body = json!({
            "params": { "_meta": { "openai/session": too_long } }
        });
        assert_eq!(session_id_from_request(&headers, &long_body), None);

        let control_body = json!({
            "params": { "_meta": { "openai/session": "chat\nunsafe" } }
        });
        assert_eq!(session_id_from_request(&headers, &control_body), None);
    }

    #[test]
    fn unknown_session_ids_fail_closed_without_creating_a_scope() {
        let (_a, _b, state) = gateway_fixture();
        let response = handle_gateway_request(
            &state,
            Some("forged-session"),
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": { "name": "workspace_current", "arguments": {} }
            }),
        );

        assert_eq!(
            response.body["error"]["data"]["reason"],
            "MCP_SESSION_INVALID"
        );
        assert_eq!(state.sessions.active_session_count(), 0);
    }

    #[test]
    fn session_id_prefers_mcp_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            MCP_SESSION_HEADER,
            "header-session".parse().expect("header"),
        );
        let body = json!({
            "params": { "_meta": { "openai/session": "meta-session" } }
        });

        assert_eq!(
            session_id_from_request(&headers, &body).as_deref(),
            Some("header-session")
        );
    }

    #[test]
    fn project_tool_fails_closed_without_workspace_selection() {
        let (_a, _b, state) = gateway_fixture();
        let response = rpc_tool(
            &state,
            "chat-a",
            "read_file",
            json!({ "path": "marker.txt" }),
        );

        assert_eq!(
            response["error"]["data"]["reason"],
            "WORKSPACE_NOT_SELECTED"
        );
    }

    #[test]
    fn two_chat_sessions_read_only_their_selected_workspaces() {
        let (_a, _b, state) = gateway_fixture();
        rpc_tool(
            &state,
            "chat-a",
            "workspace_select",
            json!({ "workspace_id": "workspace-a" }),
        );
        rpc_tool(
            &state,
            "chat-b",
            "workspace_select",
            json!({ "workspace_id": "workspace-b" }),
        );

        let read_a = rpc_tool(
            &state,
            "chat-a",
            "read_file",
            json!({ "path": "marker.txt" }),
        );
        let read_b = rpc_tool(
            &state,
            "chat-b",
            "read_file",
            json!({ "path": "marker.txt" }),
        );
        assert_eq!(structured(&read_a)["content"], "ONLY-A");
        assert_eq!(structured(&read_b)["content"], "ONLY-B");
        assert_eq!(
            state
                .sessions
                .current("chat-a")
                .active_workspace_id
                .as_deref(),
            Some("workspace-a")
        );
        assert_eq!(
            state
                .sessions
                .current("chat-b")
                .active_workspace_id
                .as_deref(),
            Some("workspace-b")
        );
    }

    #[test]
    fn captured_request_context_does_not_change_when_session_switches_workspace() {
        let (_a, _b, state) = gateway_fixture();
        state.sessions.register_for_test("chat-a");
        let captured = state
            .select_workspace("chat-a", "workspace-a")
            .expect("capture workspace a");
        state
            .select_workspace("chat-a", "workspace-b")
            .expect("switch session to workspace b");

        let read = call_tool(
            captured.tools.as_ref(),
            "read_file",
            &json!({ "path": "marker.txt" }),
        );
        assert_eq!(read["content"], "ONLY-A");
        assert_eq!(captured.workspace_id, "workspace-a");
        assert_eq!(captured.session_id, "chat-a");
        assert_eq!(
            state
                .sessions
                .current("chat-a")
                .active_workspace_id
                .as_deref(),
            Some("workspace-b")
        );
    }

    #[test]
    fn workspace_invoke_is_one_off_and_does_not_mutate_active_workspace() {
        let (_a, _b, state) = gateway_fixture();
        rpc_tool(
            &state,
            "chat-a",
            "workspace_select",
            json!({ "workspace_id": "workspace-a" }),
        );

        let invoked = rpc_tool(
            &state,
            "chat-a",
            "workspace_invoke",
            json!({
                "workspace_id": "workspace-b",
                "tool": "read_file",
                "arguments": { "path": "marker.txt" }
            }),
        );
        assert_eq!(structured(&invoked)["content"], "ONLY-B");
        assert_eq!(
            structured(&invoked)["gateway_workspace"]["active_workspace_changed"],
            false
        );

        let current = rpc_tool(&state, "chat-a", "workspace_current", json!({}));
        assert_eq!(structured(&current)["workspace"]["id"], "workspace-a");
    }

    #[test]
    fn absolute_path_cannot_escape_selected_workspace() {
        let (a, b, state) = gateway_fixture();
        rpc_tool(
            &state,
            "chat-a",
            "workspace_select",
            json!({ "workspace_id": "workspace-a" }),
        );
        let outside = b.path().join("marker.txt");
        let response = rpc_tool(
            &state,
            "chat-a",
            "read_file",
            json!({ "path": outside.display().to_string() }),
        );

        assert_eq!(structured(&response)["ok"], false);
        assert_ne!(a.path(), b.path());
    }

    #[test]
    fn command_output_ref_is_scoped_to_workspace_session_store() {
        let (_a, _b, state) = gateway_fixture();
        state.sessions.register_for_test("chat-a");
        state.sessions.register_for_test("chat-b");
        let a = state
            .workspace_by_id("chat-a", "workspace-a")
            .expect("workspace a context");
        let b = state
            .workspace_by_id("chat-b", "workspace-b")
            .expect("workspace b context");

        let command = call_tool(
            a.tools.as_ref(),
            "exec_command",
            &json!({
                "cmd": "git --version",
                "yield_time_ms": 5000,
                "max_output_bytes": 4096
            }),
        );
        assert_eq!(command["ok"], true, "{command}");
        let output_ref = command["output_refs"]["stdout"]
            .as_str()
            .expect("stdout output_ref");

        let cross_read = call_tool(
            b.tools.as_ref(),
            "read_output",
            &json!({ "output_ref": output_ref }),
        );
        assert_eq!(cross_read["ok"], false, "{cross_read}");
        assert_eq!(
            cross_read["error"]["code"], "SESSION_NOT_FOUND",
            "{cross_read}"
        );
    }

    #[test]
    fn server_info_reports_gateway_session_and_active_workspace() {
        let (_a, _b, state) = gateway_fixture();
        rpc_tool(
            &state,
            "chat-a",
            "workspace_select",
            json!({ "workspace_id": "workspace-a" }),
        );

        let response = rpc_tool(&state, "chat-a", "server_info", json!({}));
        let gateway = &structured(&response)["gateway"];
        assert_eq!(gateway["mode"], "global-multi-workspace");
        assert_eq!(gateway["session_id"], "chat-a");
        assert_eq!(gateway["active_workspace"]["id"], "workspace-a");
        assert_eq!(gateway["active_session_count"], 1);
    }
}
