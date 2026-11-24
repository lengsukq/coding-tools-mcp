use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::extract::State;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{broadcast, RwLock};

use crate::mcp::gateway::{GatewayState, MCP_SESSION_HEADER};
use crate::mcp::server::{handle_gateway_request, handle_gateway_upstream_tool_call_async};
use crate::settings::AppSettings;
use crate::tool_audit::ToolAuditCall;

use super::*;

#[derive(Clone)]
struct MockHttpState {
    expected_bearer: String,
    expected_header_val: String,
    active_session: Arc<RwLock<String>>,
    expire_once: Arc<AtomicUsize>,
    use_sse_response: bool,
}

async fn mock_streamable_http_handler(
    State(state): State<MockHttpState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let auth = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if auth != format!("Bearer {}", state.expected_bearer) {
        return (StatusCode::UNAUTHORIZED, "missing bearer").into_response();
    }
    let tenant = headers
        .get("x-tenant")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if tenant != state.expected_header_val {
        return (StatusCode::FORBIDDEN, "missing custom header").into_response();
    }
    if headers.get(UPSTREAM_ORIGIN_HEADER).is_none() {
        return (StatusCode::BAD_REQUEST, "missing anti-loop origin header").into_response();
    }

    let method = body.get("method").and_then(Value::as_str).unwrap_or("");
    let id = body.get("id").cloned().unwrap_or(Value::Null);

    if method == "notifications/initialized" {
        return StatusCode::ACCEPTED.into_response();
    }

    if method == "initialize" {
        let new_session = format!("sess-{}", uuid::Uuid::new_v4());
        *state.active_session.write().await = new_session.clone();
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2025-11-25",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "mock-http-mcp", "version": "1.0.0" }
            }
        });
        return (
            StatusCode::OK,
            [
                (CONTENT_TYPE, "application/json"),
                (
                    axum::http::header::HeaderName::from_static(MCP_SESSION_HEADER),
                    new_session.leak(),
                ),
            ],
            payload.to_string(),
        )
            .into_response();
    }

    // Simulate one session expiration (HTTP 404) to verify automatic re-handshake!
    if method == "tools/call" && state.expire_once.swap(0, Ordering::SeqCst) == 1 {
        return (StatusCode::NOT_FOUND, "session expired").into_response();
    }

    let resp = match method {
        "tools/list" => {
            let cursor = body.pointer("/params/cursor").and_then(Value::as_str);
            if cursor == Some("page2") {
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "tools": [
                            {
                                "name": "secret_admin",
                                "description": "Dangerous admin tool",
                                "inputSchema": { "type": "object", "properties": {} }
                            }
                        ]
                    }
                })
            } else {
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "tools": [
                            {
                                "name": "echo.text",
                                "description": "Echo input text back",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": { "msg": { "type": "string" } },
                                    "required": ["msg"]
                                }
                            }
                        ],
                        "nextCursor": "page2"
                    }
                })
            }
        }
        "tools/call" => {
            let tool = body
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let msg = body
                .pointer("/params/arguments/msg")
                .and_then(Value::as_str)
                .unwrap_or("");
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "isError": false,
                    "content": [{ "type": "text", "text": format!("http:{tool}:{msg}") }]
                }
            })
        }
        _ => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": "not found" }
        }),
    };

    if state.use_sse_response {
        let sse_body = format!("event: message\ndata: {}\n\n", resp);
        (
            StatusCode::OK,
            [(CONTENT_TYPE, "text/event-stream")],
            sse_body,
        )
            .into_response()
    } else {
        (StatusCode::OK, Json(resp)).into_response()
    }
}

async fn spawn_mock_http_server(
    use_sse_response: bool,
    expire_once: bool,
) -> (String, tokio::task::JoinHandle<()>) {
    let state = MockHttpState {
        expected_bearer: "tok-secret-123".into(),
        expected_header_val: "acme".into(),
        active_session: Arc::new(RwLock::new(String::new())),
        expire_once: Arc::new(AtomicUsize::new(usize::from(expire_once))),
        use_sse_response,
    };
    let app = Router::new()
        .route("/mcp", post(mock_streamable_http_handler))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}/mcp"), handle)
}

async fn spawn_mock_legacy_sse_server() -> (String, tokio::task::JoinHandle<()>) {
    let (tx, _) = broadcast::channel::<String>(32);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = match socket.read(&mut buf).await {
                    Ok(n) if n > 0 => n,
                    _ => return,
                };
                let req = String::from_utf8_lossy(&buf[..n]);
                let first_line = req.lines().next().unwrap_or("");
                if first_line.starts_with("GET /sse") {
                    let mut rx = tx.subscribe();
                    let header = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n";
                    if socket.write_all(header.as_bytes()).await.is_err() {
                        return;
                    }
                    let ep = "event: endpoint\ndata: /messages?sid=abc\n\n";
                    if socket.write_all(ep.as_bytes()).await.is_err() {
                        return;
                    }
                    let _ = socket.flush().await;
                    while let Ok(payload) = rx.recv().await {
                        let chunk = format!("event: message\ndata: {payload}\n\n");
                        if socket.write_all(chunk.as_bytes()).await.is_err() {
                            break;
                        }
                        let _ = socket.flush().await;
                    }
                } else if first_line.starts_with("POST /messages") {
                    let body_str = req.split("\r\n\r\n").nth(1).unwrap_or("");
                    if let Ok(body) = serde_json::from_str::<Value>(body_str.trim()) {
                        let method = body.get("method").and_then(Value::as_str).unwrap_or("");
                        let id = body.get("id").cloned().unwrap_or(Value::Null);
                        if method != "notifications/initialized" {
                            let resp = match method {
                                "initialize" => json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "protocolVersion": "2024-11-05",
                                        "capabilities": { "tools": {} },
                                        "serverInfo": { "name": "mock-sse-mcp", "version": "0.9.0" }
                                    }
                                }),
                                "tools/list" => json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "tools": [
                                            {
                                                "name": "lookup_doc",
                                                "description": "Search legacy docs",
                                                "inputSchema": { "type": "object", "properties": { "q": { "type": "string" } } }
                                            }
                                        ]
                                    }
                                }),
                                "tools/call" => {
                                    let q = body
                                        .pointer("/params/arguments/q")
                                        .and_then(Value::as_str)
                                        .unwrap_or("");
                                    json!({
                                        "jsonrpc": "2.0",
                                        "id": id,
                                        "result": {
                                            "isError": false,
                                            "content": [{ "type": "text", "text": format!("sse-result:{q}") }]
                                        }
                                    })
                                }
                                _ => json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "error": { "code": -32601, "message": "not found" }
                                }),
                            };
                            let _ = tx.send(resp.to_string());
                        }
                    }
                    let _ = socket
                        .write_all(b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .await;
                }
            });
        }
    });
    (format!("http://{addr}/sse"), handle)
}

#[tokio::test]
async fn streamable_http_direct_and_session_recovery_end_to_end() {
    let (url, server_handle) = spawn_mock_http_server(true, true).await;

    let mut settings = AppSettings::default();
    settings.lan_federation.device_id = "dev-test-local".into();
    settings.global_gateway.local_port = 28765;
    settings.upstream_mcp.enabled = true;
    settings.upstream_mcp.servers.push(UpstreamServerConfig {
        id: "srv-http-1".into(),
        alias: "docs".into(),
        name: "Docs Server".into(),
        description: "Mock HTTP docs".into(),
        enabled: true,
        expose_mode: UpstreamExposeMode::Direct,
        transport: UpstreamTransportConfig::StreamableHttp {
            url,
            headers: BTreeMap::from([("x-tenant".into(), "acme".into())]),
            secret_header_keys: Vec::new(),
            has_bearer_token: true,
            use_proxy: false,
        },
        disabled_tools: vec!["secret_admin".into()],
        call_timeout_secs: 15,
        idle_timeout_secs: 300,
        source: "test".into(),
    });
    settings
        .app_secrets
        .entry(UPSTREAM_BEARER_SECRETS_SCOPE.into())
        .or_default()
        .insert("srv-http-1".into(), "tok-secret-123".into());

    let gateway = GatewayState::for_test_profiles(Vec::new(), settings.clone());
    let sync = gateway
        .upstream
        .test_and_sync_server("srv-http-1", Some(&settings))
        .await;
    assert!(sync.ok, "sync failed: {:?}", sync.error);
    assert_eq!(sync.tool_count, 2, "should follow nextCursor pagination");

    // 1. tools/list exposes sanitized direct tool `docs__echo_text`, excludes disabled `secret_admin`,
    //    and does NOT expose proxy meta-tools since no proxy server is configured.
    let list_resp = handle_gateway_request(
        &gateway,
        None,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} }),
    );
    let tools = list_resp.body["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(names.contains(&"docs__echo_text"));
    assert!(!names.contains(&"docs__secret_admin"));
    assert!(!names.contains(&TOOL_UPSTREAM_LIST));

    // Verify workspace_id was NOT injected into upstream direct tool schema.
    let echo_tool = tools
        .iter()
        .find(|t| t["name"] == "docs__echo_text")
        .unwrap();
    assert!(echo_tool["inputSchema"]["properties"]
        .get("workspace_id")
        .is_none());

    // 2. Direct tool call succeeds even across simulated HTTP 404 session expiration!
    let call_resp = handle_gateway_upstream_tool_call_async(
        &gateway,
        None,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "docs__echo_text",
                "arguments": { "msg": "hello-hub" }
            }
        }),
    )
    .await;
    assert_eq!(call_resp.body["result"]["isError"], false);
    assert_eq!(
        call_resp.body["result"]["content"][0]["text"],
        "http:echo.text:hello-hub"
    );
    assert_eq!(
        call_resp.execution_host.as_deref(),
        Some("upstream:docs (Docs Server)")
    );

    gateway.upstream.shutdown_all().await;
    server_handle.abort();
}

#[tokio::test]
async fn legacy_sse_proxy_mode_and_audit_unwrapping_end_to_end() {
    let (sse_url, server_handle) = spawn_mock_legacy_sse_server().await;

    let mut settings = AppSettings::default();
    settings.lan_federation.device_id = "dev-test-sse".into();
    settings.global_gateway.local_port = 28765;
    settings.upstream_mcp.enabled = true;
    settings.upstream_mcp.servers.push(UpstreamServerConfig {
        id: "srv-sse-1".into(),
        alias: "legacy".into(),
        name: "Legacy SSE".into(),
        description: "Legacy knowledge base".into(),
        enabled: true,
        expose_mode: UpstreamExposeMode::Proxy,
        transport: UpstreamTransportConfig::Sse {
            url: sse_url,
            headers: BTreeMap::new(),
            secret_header_keys: Vec::new(),
            has_bearer_token: false,
            use_proxy: false,
        },
        disabled_tools: Vec::new(),
        call_timeout_secs: 15,
        idle_timeout_secs: 300,
        source: "test".into(),
    });

    let gateway = GatewayState::for_test_profiles(Vec::new(), settings.clone());

    // tools/list should expose the 3 meta-tools and NOT flatten `legacy__lookup_doc`.
    let list_resp = handle_gateway_request(
        &gateway,
        None,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} }),
    );
    let names: Vec<&str> = list_resp.body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(names.contains(&TOOL_UPSTREAM_LIST));
    assert!(names.contains(&TOOL_UPSTREAM_TOOLS));
    assert!(names.contains(&TOOL_UPSTREAM_CALL));
    assert!(!names.contains(&"legacy__lookup_doc"));

    // 1. Discover servers via mcp_upstream_list
    let srv_list = handle_gateway_upstream_tool_call_async(
        &gateway,
        None,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": TOOL_UPSTREAM_LIST, "arguments": {} }
        }),
    )
    .await;
    assert_eq!(srv_list.body["result"]["structuredContent"]["count"], 1);

    // 2. Lazy-sync and search tool schema via mcp_upstream_tools
    let tool_search = handle_gateway_upstream_tool_call_async(
        &gateway,
        None,
        &json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": TOOL_UPSTREAM_TOOLS,
                "arguments": { "server": "legacy", "query": "lookup" }
            }
        }),
    )
    .await;
    let found = tool_search.body["result"]["structuredContent"]["tools"]
        .as_array()
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["tool"], "lookup_doc");
    assert_eq!(found[0]["inputSchema"]["properties"]["q"]["type"], "string");

    // 3. Call upstream via mcp_upstream_call + verify ToolAuditCall unwraps tool/wrapper name
    let call_body = json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": TOOL_UPSTREAM_CALL,
            "arguments": {
                "server": "legacy",
                "tool": "lookup_doc",
                "arguments": { "q": "rust-mcp" }
            }
        }
    });
    let call_resp = handle_gateway_upstream_tool_call_async(&gateway, None, &call_body).await;
    assert_eq!(call_resp.body["result"]["isError"], false);
    assert_eq!(
        call_resp.body["result"]["content"][0]["text"],
        "sse-result:rust-mcp"
    );
    assert_eq!(
        call_resp.execution_host.as_deref(),
        Some("upstream:legacy (Legacy SSE)")
    );

    let mut audit_call = ToolAuditCall::new(
        &json!(4),
        TOOL_UPSTREAM_CALL,
        None,
        "local_loopback",
        Some("loopback"),
        128,
        &call_body,
    );
    audit_call.set_execution_host(call_resp.execution_host);
    let record = audit_call.finish(None, &call_resp.body);
    assert_eq!(record.tool_name, "legacy__lookup_doc");
    assert_eq!(
        record.wrapper_tool_name.as_deref(),
        Some(TOOL_UPSTREAM_CALL)
    );
    assert_eq!(
        record.execution_host.as_deref(),
        Some("upstream:legacy (Legacy SSE)")
    );
    assert_eq!(record.outcome, "success");

    gateway.upstream.shutdown_all().await;
    server_handle.abort();
}

#[tokio::test]
async fn stdio_upstream_roundtrip_with_secret_env() {
    let Ok(node_bin) = which::which("node") else {
        // Skip only if node is absent in a minimal Rust-only container.
        return;
    };

    let script = r#"
const readline = require('readline');
const rl = readline.createInterface({ input: process.stdin });
rl.on('line', (line) => {
  if (!line.trim()) return;
  const msg = JSON.parse(line);
  if (msg.method === 'initialize') {
    process.stdout.write(JSON.stringify({
      jsonrpc: '2.0', id: msg.id,
      result: { protocolVersion: '2025-11-25', capabilities: { tools: {} }, serverInfo: { name: 'mock-stdio', version: '1.0' } }
    }) + '\n');
  } else if (msg.method === 'tools/list') {
    process.stdout.write(JSON.stringify({
      jsonrpc: '2.0', id: msg.id,
      result: { tools: [{ name: 'whoami', description: 'Check env', inputSchema: { type: 'object', properties: {} } }] }
    }) + '\n');
  } else if (msg.method === 'tools/call') {
    process.stdout.write(JSON.stringify({
      jsonrpc: '2.0', id: msg.id,
      result: { isError: false, content: [{ type: 'text', text: 'secret=' + (process.env.MY_API_TOKEN || '') }] }
    }) + '\n');
  }
});
"#;

    let mut settings = AppSettings::default();
    settings.upstream_mcp.enabled = true;
    settings.upstream_mcp.servers.push(UpstreamServerConfig {
        id: "srv-stdio-1".into(),
        alias: "stdio_demo".into(),
        name: "Stdio Demo".into(),
        description: String::new(),
        enabled: true,
        expose_mode: UpstreamExposeMode::Direct,
        transport: UpstreamTransportConfig::Stdio {
            command: node_bin.to_string_lossy().to_string(),
            args: vec!["-e".into(), script.into()],
            cwd: None,
            env: BTreeMap::new(),
            secret_env_keys: vec!["MY_API_TOKEN".into()],
        },
        disabled_tools: Vec::new(),
        call_timeout_secs: 15,
        idle_timeout_secs: 300,
        source: "test".into(),
    });
    settings
        .app_secrets
        .entry(UPSTREAM_ENV_SECRETS_SCOPE.into())
        .or_default()
        .insert(
            env_secret_item_id("srv-stdio-1", "MY_API_TOKEN"),
            "s3cr3t!".into(),
        );

    let hub = UpstreamHub::new_in_memory();
    let sync = hub
        .test_and_sync_server("srv-stdio-1", Some(&settings))
        .await;
    assert!(sync.ok, "stdio sync failed: {:?}", sync.error);
    assert_eq!(sync.tool_count, 1);

    let call = hub
        .handle_tool_call("stdio_demo__whoami", &json!({}), Some(&settings))
        .await
        .expect("stdio call");
    assert_eq!(call.result["content"][0]["text"], "secret=s3cr3t!");
    assert_eq!(call.execution_host, "upstream:stdio_demo (Stdio Demo)");

    hub.shutdown_all().await;
}

#[test]
fn self_loop_guard_blocks_own_port_and_stdio_bridge() {
    let ctx = SelfLoopContext {
        local_port: 28765,
        public_url: "https://hub.example.com".into(),
    };
    let own_http = UpstreamTransportConfig::StreamableHttp {
        url: "http://127.0.0.1:28765/mcp".into(),
        headers: BTreeMap::new(),
        secret_header_keys: Vec::new(),
        has_bearer_token: false,
        use_proxy: false,
    };
    assert!(detect_self_loop(&own_http, &ctx).is_some());

    let own_public = UpstreamTransportConfig::StreamableHttp {
        url: "https://hub.example.com/mcp".into(),
        headers: BTreeMap::new(),
        secret_header_keys: Vec::new(),
        has_bearer_token: false,
        use_proxy: false,
    };
    assert!(detect_self_loop(&own_public, &ctx).is_some());

    let own_stdio = UpstreamTransportConfig::Stdio {
        command: "/Users/leo/.local/bin/coding-tools-mcp".into(),
        args: vec!["mcp-stdio".into(), "http://127.0.0.1:28765/mcp".into()],
        cwd: None,
        env: BTreeMap::new(),
        secret_env_keys: Vec::new(),
    };
    assert!(detect_self_loop(&own_stdio, &ctx).is_some());

    let other_http = UpstreamTransportConfig::StreamableHttp {
        url: "http://127.0.0.1:3001/mcp".into(),
        headers: BTreeMap::new(),
        secret_header_keys: Vec::new(),
        has_bearer_token: false,
        use_proxy: false,
    };
    assert!(detect_self_loop(&other_http, &ctx).is_none());
}
