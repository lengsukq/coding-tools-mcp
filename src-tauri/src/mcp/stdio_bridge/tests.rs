use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use reqwest::header::AUTHORIZATION;
use serde_json::{json, Value};
use tokio::sync::{mpsc, RwLock};

use super::auth::load_bearer_token;
use super::compact::adapt_gemini_response;
use super::transport::{extract_jsonrpc_messages, forward_line_channel, BridgeState};
use super::{
    format_mcp_stdio_command, read_auth_file, remove_auth_file_if_exists,
    validate_loopback_mcp_endpoint, write_private_auth_file, GeminiStdioAuthFile,
    MCP_SESSION_HEADER,
};

#[test]
fn validate_loopback_endpoint_accepts_and_normalizes_loopback_urls() {
    assert_eq!(
        validate_loopback_mcp_endpoint(" http://localhost:28765/mcp ").unwrap(),
        "http://127.0.0.1:28765/mcp"
    );
    assert_eq!(
        validate_loopback_mcp_endpoint("http://127.0.0.1:29000/mcp").unwrap(),
        "http://127.0.0.1:29000/mcp"
    );
    assert_eq!(
        validate_loopback_mcp_endpoint("http://[::1]:28765/mcp").unwrap(),
        "http://[::1]:28765/mcp"
    );
}

#[test]
fn validate_loopback_endpoint_rejects_remote_or_malformed_urls() {
    for bad in [
        "https://127.0.0.1:28765/mcp",
        "http://example.com:28765/mcp",
        "http://127.0.0.1/mcp",
        "http://127.0.0.1:28765/other",
        "http://127.0.0.1:28765/mcp?foo=bar",
        "http://user:pass@127.0.0.1:28765/mcp",
    ] {
        assert!(
            validate_loopback_mcp_endpoint(bad).is_err(),
            "expected error for {bad}"
        );
    }
}

#[test]
fn format_command_quotes_only_when_binary_has_whitespace() {
    assert_eq!(
        format_mcp_stdio_command(
            Path::new("/Users/leo/.local/bin/coding-tools-mcp"),
            "http://127.0.0.1:28765/mcp"
        ),
        "/Users/leo/.local/bin/coding-tools-mcp mcp-stdio http://127.0.0.1:28765/mcp"
    );
    assert_eq!(
        format_mcp_stdio_command(
            Path::new("C:\\Program Files\\Coding Tools\\coding-tools-mcp.exe"),
            "http://127.0.0.1:28765/mcp"
        ),
        "\"C:\\Program Files\\Coding Tools\\coding-tools-mcp.exe\" mcp-stdio http://127.0.0.1:28765/mcp"
    );
}

#[test]
fn auth_file_roundtrips_with_private_permissions() {
    let dir = std::env::temp_dir().join(format!("ctm-stdio-auth-{}", uuid::Uuid::new_v4()));
    let auth_path = dir.join("gemini-stdio-auth.json");
    let payload = GeminiStdioAuthFile::new_oauth(
        "http://127.0.0.1:28765/mcp".into(),
        "https://mcp.example.com".into(),
        "secret.jwt.token".into(),
        1_800_000_000,
    );
    write_private_auth_file(&auth_path, &payload).expect("write auth file");
    let loaded = read_auth_file(&auth_path)
        .expect("read auth file")
        .expect("auth file present");
    assert_eq!(loaded, payload);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&auth_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    remove_auth_file_if_exists(&auth_path).expect("remove auth file");
    assert!(!auth_path.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn extract_jsonrpc_handles_null_empty_json_and_sse() {
    assert!(extract_jsonrpc_messages("application/json", "null")
        .unwrap()
        .is_empty());
    assert!(extract_jsonrpc_messages("application/json", "")
        .unwrap()
        .is_empty());
    let json_msgs =
        extract_jsonrpc_messages("application/json", r#"{"jsonrpc":"2.0","id":1,"result":{}}"#)
            .unwrap();
    assert_eq!(json_msgs.len(), 1);

    let sse = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"tools\":[]}}\n\ndata: [DONE]\n\n";
    let sse_msgs = extract_jsonrpc_messages("text/event-stream; charset=utf-8", sse).unwrap();
    assert_eq!(sse_msgs.len(), 1);
    assert_eq!(sse_msgs[0]["id"], json!(2));
}

#[tokio::test]
async fn bridge_forwards_initialize_session_notifications_and_auth() {
    #[derive(Clone)]
    struct MockState {
        expected_token: String,
        session_id: String,
        seen_session_on_list: Arc<AtomicUsize>,
    }

    async fn mock_mcp(
        State(state): State<MockState>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> impl IntoResponse {
        let auth = headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if auth != format!("Bearer {}", state.expected_token) {
            return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
        }

        let method = body.get("method").and_then(Value::as_str).unwrap_or("");
        match method {
            "initialize" => {
                let mut resp = Json(json!({
                    "jsonrpc": "2.0",
                    "id": body["id"],
                    "result": { "protocolVersion": "2025-03-26" }
                }))
                .into_response();
                resp.headers_mut()
                    .insert(MCP_SESSION_HEADER, state.session_id.parse().unwrap());
                resp
            }
            "notifications/initialized" => Json(Value::Null).into_response(),
            "tools/list" => {
                if headers
                    .get(MCP_SESSION_HEADER)
                    .and_then(|v| v.to_str().ok())
                    == Some(state.session_id.as_str())
                {
                    state.seen_session_on_list.fetch_add(1, Ordering::SeqCst);
                }
                Json(json!({
                    "jsonrpc": "2.0",
                    "id": body["id"],
                    "result": { "tools": [{"name": "workspace_list"}] }
                }))
                .into_response()
            }
            _ => Json(json!({"jsonrpc": "2.0", "id": body["id"], "result": {}})).into_response(),
        }
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen_session_on_list = Arc::new(AtomicUsize::new(0));
    let mock_state = MockState {
        expected_token: "test-jwt-token".into(),
        session_id: "sess-123".into(),
        seen_session_on_list: Arc::clone(&seen_session_on_list),
    };
    let app = Router::new()
        .route("/mcp", post(mock_mcp))
        .with_state(mock_state);
    let server_task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let dir = std::env::temp_dir().join(format!("ctm-bridge-test-{}", uuid::Uuid::new_v4()));
    let auth_path = dir.join("gemini-stdio-auth.json");
    write_private_auth_file(
        &auth_path,
        &GeminiStdioAuthFile::new_oauth(
            format!("http://127.0.0.1:{port}/mcp"),
            format!("http://127.0.0.1:{port}"),
            "test-jwt-token".into(),
            1_900_000_000,
        ),
    )
    .unwrap();

    let (in_tx, in_rx) = mpsc::unbounded_channel();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel();

    let bridge_state = Arc::new(BridgeState {
        client: reqwest::Client::builder().no_proxy().build().unwrap(),
        endpoint: format!("http://127.0.0.1:{port}/mcp"),
        auth_file: Some(auth_path.clone()),
        bearer_token: RwLock::new(load_bearer_token(Some(&auth_path))),
        session_id: RwLock::new(None),
    });

    for line in [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
    ] {
        in_tx.send(Ok(line.to_string())).unwrap();
    }
    drop(in_tx);

    forward_line_channel(bridge_state, in_rx, out_tx)
        .await
        .unwrap();
    let mut output_lines = Vec::new();
    while let Some(line) = out_rx.recv().await {
        output_lines.push(serde_json::from_str::<Value>(&line).expect("valid json line"));
    }

    // Notification `Value::Null` response must NOT be printed to stdout.
    assert_eq!(output_lines.len(), 2);
    assert_eq!(output_lines[0]["id"], json!(1));
    assert_eq!(output_lines[1]["id"], json!(2));
    assert_eq!(
        output_lines[1]["result"]["tools"][0]["name"],
        json!("workspace_list")
    );
    assert_eq!(seen_session_on_list.load(Ordering::SeqCst), 1);

    server_task.abort();
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn bridge_returns_jsonrpc_error_on_unauthorized_without_hanging() {
    async fn always_unauthorized() -> impl IntoResponse {
        (StatusCode::UNAUTHORIZED, "Unauthorized")
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = Router::new().route("/mcp", post(always_unauthorized));
    let server_task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let (in_tx, in_rx) = mpsc::unbounded_channel();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel();

    let bridge_state = Arc::new(BridgeState {
        client: reqwest::Client::builder().no_proxy().build().unwrap(),
        endpoint: format!("http://127.0.0.1:{port}/mcp"),
        auth_file: None,
        bearer_token: RwLock::new(None),
        session_id: RwLock::new(None),
    });

    in_tx
        .send(Ok(
            r#"{"jsonrpc":"2.0","id":7,"method":"initialize"}"#.to_string(),
        ))
        .unwrap();
    drop(in_tx);

    forward_line_channel(bridge_state, in_rx, out_tx)
        .await
        .unwrap();
    let raw_out = out_rx.recv().await.expect("error line");
    let parsed: Value = serde_json::from_str(raw_out.trim()).unwrap();
    assert_eq!(parsed["id"], json!(7));
    assert_eq!(parsed["error"]["code"], json!(-32000));
    assert!(parsed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("HTTP 401"));

    server_task.abort();
}

#[test]
fn adapt_gemini_response_compacts_tools_list_without_dropping_schema() {
    let raw = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "result": {
            "tools": [
                {
                    "name": "exec_command",
                    "title": "Exec Command",
                    "description": "Execute a single whitelisted CLI command inside the active workspace without shell operators like && or | and inspect its stdout/stderr carefully before acting.",
                    "annotations": { "destructiveHint": true },
                    "inputSchema": {
                        "$schema": "http://json-schema.org/draft-07/schema#",
                        "title": "ExecParams",
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["cmd"],
                        "properties": {
                            "workspace_id": {
                                "type": "string",
                                "description": "Optional workspace ID returned by workspace_list or workspace_select when no session workspace has been selected yet or when targeting another workspace explicitly."
                            },
                            "cmd": {
                                "type": "string",
                                "minLength": 1,
                                "description": "Single command to run inside the workspace root directory without chaining operators"
                            },
                            "mode": {
                                "type": "string",
                                "enum": ["fast", "full"],
                                "default": "fast"
                            }
                        }
                    }
                }
            ]
        }
    });

    let raw_len = raw.to_string().len();
    let adapted = adapt_gemini_response(Some("tools/list"), raw);
    let adapted_len = adapted.to_string().len();
    assert!(adapted_len < raw_len);

    let tool = &adapted["result"]["tools"][0];
    assert!(tool.get("title").is_none());
    assert_eq!(tool["annotations"]["destructiveHint"], json!(true));
    assert!(tool["description"].as_str().unwrap().chars().count() <= 110);
    assert_eq!(tool["inputSchema"]["required"], json!(["cmd"]));
    assert_eq!(
        tool["inputSchema"]["properties"]["workspace_id"],
        json!({ "type": "string" })
    );
    assert_eq!(
        tool["inputSchema"]["properties"]["mode"],
        json!({ "type": "string", "enum": ["fast", "full"] })
    );
    assert!(tool["inputSchema"]["properties"]["cmd"]["description"]
        .as_str()
        .unwrap()
        .chars()
        .count()
        <= 48);
}

#[test]
fn adapt_gemini_response_converts_tool_call_rpc_error_into_mcp_is_error_result() {
    let rpc_err = json!({
        "jsonrpc": "2.0",
        "id": 9,
        "error": {
            "code": -32602,
            "message": "No active workspace selected for this session.",
            "data": { "reason": "WORKSPACE_NOT_SELECTED", "retryable": false }
        }
    });

    let adapted = adapt_gemini_response(Some("tools/call"), rpc_err.clone());
    assert!(adapted.get("error").is_none());
    assert_eq!(adapted["id"], json!(9));
    assert_eq!(adapted["result"]["isError"], json!(true));
    let text = adapted["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("WORKSPACE_NOT_SELECTED"));
    assert!(text.contains("workspace_list"));
    assert!(text.contains("workspace_select"));

    // Non-tools/call RPC errors stay standard JSON-RPC errors.
    let untouched = adapt_gemini_response(Some("resources/read"), rpc_err);
    assert_eq!(untouched["error"]["code"], json!(-32602));
}
