use serde_json::{json, Value};

use super::{
    action_lock, locked, native_action, native_capture, native_ui_tree, native_windows, now_ms,
    COMPUTER_AUTO_SESSION_PREFIX,
};

fn tool_error(message: &str) -> Value {
    json!({
        "isError": true,
        "content": [{ "type": "text", "text": message }],
        "structuredContent": { "ok": false, "error": message }
    })
}

fn tool_ok(value: Value) -> Value {
    json!({
        "isError": false,
        "content": [{ "type": "text", "text": value.to_string() }],
        "structuredContent": value
    })
}

/// The caller identity is the *server-generated transport session*, never the
/// caller-supplied `_meta` field or an arbitrary workspace_id.
pub fn call_tool(name: &str, session_id: Option<&str>, args: &Value) -> Value {
    let Some(session) = session_id.filter(|s| !s.is_empty()) else {
        return tool_error(
            "COMPUTER_SESSION_REQUIRED: 必须建立持久 MCP transport session 才能申请截图授权",
        );
    };
    // Only verified principals received a server-created computer-auto-*
    // session in listener.rs. The local policy controls the permission.
    locked().attach_manual_grant(session);
    let action = args.get("action").and_then(Value::as_str).unwrap_or("");
    match name {
        "computer_session" => handle_session_tool(session, action, args),
        "computer_observe" => handle_observe_tool(session, action, args),
        "computer_action" => handle_action_tool(session, args),
        _ => tool_error("不支持的 computer action"),
    }
}

fn handle_session_tool(session: &str, action: &str, args: &Value) -> Value {
    match action {
        "request" => {
            let scope = args
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or("observe");
            if !matches!(scope, "observe" | "control") {
                return tool_error("INVALID_SCOPE: only observe or control allowed");
            }
            match locked().request(session, scope) {
                Ok(result) => tool_ok(result),
                Err(error) => tool_error(&error),
            }
        }
        "status" => {
            let mut state = locked();
            state.expire();
            let approved = state.allowed(session);
            let pending = state.pending.as_ref().is_some_and(|p| p.session == session);
            let paused = state
                .grant
                .as_ref()
                .is_some_and(|g| g.session == session && g.paused)
                || state.manual.as_ref().is_some_and(|policy| {
                    policy.paused && session.starts_with(COMPUTER_AUTO_SESSION_PREFIX)
                });
            let status = if paused {
                "paused"
            } else if approved {
                "approved"
            } else if pending {
                "pending"
            } else {
                "disabled"
            };
            tool_ok(json!({
                "status": status,
                "scope": state.grant.as_ref().filter(|g| g.session == session).map(|g| g.scope.as_str()),
                "authorization": if session.starts_with(COMPUTER_AUTO_SESSION_PREFIX) { "local_policy" } else { "session" }
            }))
        }
        "stop" => {
            let _serialized = action_lock();
            match locked().stop_session(session) {
                Ok(result) => tool_ok(result),
                Err(error) => tool_error(&error),
            }
        }
        _ => tool_error("不支持的 computer action"),
    }
}

fn handle_observe_tool(session: &str, action: &str, args: &Value) -> Value {
    if !matches!(action, "windows" | "screenshot" | "ui_tree") {
        return tool_error("不支持的 computer action");
    }
    let Some((lease, target_app)) = locked().observation_context(session) else {
        return tool_error("COMPUTER_NOT_APPROVED: 请先通过 computer_session(request) 申请，并在本机桌面应用中批准");
    };
    match action {
        "windows" => handle_observe_windows(session, &lease, target_app.as_deref()),
        "ui_tree" => handle_observe_ui_tree(session, &lease, target_app.as_deref()),
        "screenshot" => handle_observe_screenshot(session, &lease, args, target_app.as_deref()),
        _ => tool_error("不支持的 computer action"),
    }
}

fn handle_observe_windows(session: &str, lease: &str, target_app: Option<&str>) -> Value {
    match native_windows() {
        Ok(mut value) if locked().lease_matches(session, lease) => {
            if let Some(target) = target_app {
                if let Some(windows) = value["windows"].as_array_mut() {
                    windows.retain(|w| {
                        w["bundle_id"].as_str() == Some(target)
                            && w["on_screen"] == true
                            && w["layer"] == 0
                    });
                }
            }
            tool_ok(value)
        }
        Ok(_) => tool_error("COMPUTER_REVOKED: 授权在读取窗口期间被撤销"),
        Err(error) => tool_error(&error),
    }
}

fn handle_observe_ui_tree(session: &str, lease: &str, target_app: Option<&str>) -> Value {
    match native_ui_tree(target_app) {
        Ok(value) if locked().lease_matches(session, lease) => tool_ok(value),
        Ok(_) => tool_error("COMPUTER_REVOKED: 授权在读取控件期间被撤销"),
        Err(error) => tool_error(&error),
    }
}

fn handle_observe_screenshot(
    session: &str,
    lease: &str,
    args: &Value,
    target_app: Option<&str>,
) -> Value {
    match native_capture(args, target_app) {
        Ok(mut value) => {
            if !locked().lease_matches(session, lease) {
                return tool_error("COMPUTER_REVOKED: 授权在截图期间被撤销，拒绝返回屏幕内容");
            }
            let encoded = value
                .as_object_mut()
                .and_then(|map| map.remove("base64"))
                .and_then(|v| v.as_str().map(str::to_string));
            let Some(encoded) = encoded else {
                return tool_error("SCREEN_CAPTURE_FAILED: native capture omitted image");
            };
            let snapshot_id = match locked().record_snapshot(session, lease, &value) {
                Ok(id) => id,
                Err(error) => return tool_error(&error),
            };
            value["snapshot_id"] = json!(snapshot_id);
            value["captured_at_unix_ms"] = json!(now_ms());
            let metadata = value;
            json!({
                "isError": false,
                "content": [
                    { "type": "text", "text": metadata.to_string() },
                    { "type": "image", "data": encoded, "mimeType": "image/jpeg" }
                ],
                "structuredContent": metadata
            })
        }
        Err(error) => tool_error(&error),
    }
}

fn handle_action_tool(session: &str, args: &Value) -> Value {
    let _serialized = action_lock();
    let (target, prepared) = match locked().prepare_or_request_sensitive_action(session, args) {
        Ok(prepared) => prepared,
        Err(error) => return tool_error(&error),
    };
    match native_action(&target, &prepared) {
        Ok(result) => tool_ok(result),
        Err(error) => tool_error(&error),
    }
}
