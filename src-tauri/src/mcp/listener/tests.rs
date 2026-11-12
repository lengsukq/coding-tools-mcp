use std::sync::Arc;
use std::{fs, path::Path};

use axum::http::header::{AUTHORIZATION, CACHE_CONTROL};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Serialize;
use serde_json::json;

use crate::auth::OAuthRuntime;
use crate::mcp::gateway::GatewayState;
use crate::settings::AppSettings;
use crate::workspace::{AuthConfig, WorkspaceProfile};

use super::{
    bind_listener, build_router, mcp_discovery, mcp_discovery_payload, require_mcp_auth,
    ListenerState,
};

#[test]
fn bind_listener_reports_port_conflict_synchronously() {
    let occupied = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("占用测试端口");
    let port = occupied.local_addr().expect("读取测试端口").port();

    assert!(bind_listener(port, false).is_err());
}

#[tokio::test]
async fn discovery_reports_the_current_package_version() {
    let discovery = mcp_discovery_payload();

    assert_eq!(discovery["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn discovery_prevents_stale_tool_catalog_caching() {
    let response = mcp_discovery().await.into_response();

    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
}

#[test]
fn mcp_auth_fails_closed_for_missing_bearer_and_unknown_mode() {
    let mut state = test_listener_state();
    state.auth.auth_type = "bearer".into();
    let missing_bearer =
        require_mcp_auth(&state, &HeaderMap::new()).expect("missing bearer must fail");
    assert_eq!(missing_bearer.status(), StatusCode::SERVICE_UNAVAILABLE);

    state.auth.auth_type = "unexpected".into();
    let invalid_mode =
        require_mcp_auth(&state, &HeaderMap::new()).expect("unknown auth must fail closed");
    assert_eq!(invalid_mode.status(), StatusCode::SERVICE_UNAVAILABLE);

    state.auth.auth_type = "noauth".into();
    assert!(require_mcp_auth(&state, &HeaderMap::new()).is_none());
}

#[test]
fn mcp_auth_allows_a_valid_oauth_bearer() {
    #[derive(Serialize)]
    struct Claims {
        iss: String,
        aud: String,
        iat: i64,
        exp: i64,
        scope: String,
        client_id: String,
        token_use: String,
    }

    let mut state = test_listener_state();
    state.auth.auth_type = "oauth".into();
    state.configured_public_url = "https://coding-mcp.lengsu.top".into();
    let token_secret = "oauth-token-secret";
    state.oauth = Some(Arc::new(OAuthRuntime::new(
        "https://coding-mcp.lengsu.top".into(),
        "test-client".into(),
        None,
        "test-password".into(),
        token_secret.into(),
    )));

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock")
        .as_secs() as i64;
    let token = encode(
        &Header::new(Algorithm::HS256),
        &Claims {
            iss: "https://coding-mcp.lengsu.top".into(),
            aud: "https://coding-mcp.lengsu.top".into(),
            iat: now,
            exp: now + 60,
            scope: "mcp".into(),
            client_id: "test-client".into(),
            token_use: "access".into(),
        },
        &EncodingKey::from_secret(token_secret.as_bytes()),
    )
    .expect("oauth access token");

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        format!("Bearer {token}")
            .parse()
            .expect("authorization header"),
    );

    assert!(require_mcp_auth(&state, &headers).is_none());
}

fn listener_state(profiles: Vec<WorkspaceProfile>) -> ListenerState {
    let auth = AuthConfig {
        auth_type: "noauth".into(),
        ..AuthConfig::default()
    };
    ListenerState {
        gateway: Arc::new(GatewayState::for_test_profiles(
            profiles,
            AppSettings::default(),
        )),
        auth,
        scope_id: "global-mcp".into(),
        scope_label: "Global MCP Gateway".into(),
        bind_port: 0,
        configured_public_url: String::new(),
        bearer_token: None,
        oauth: None,
        oauth_client_secret: None,
    }
}

fn test_listener_state() -> ListenerState {
    let mut profile = WorkspaceProfile::new(
        std::env::temp_dir().display().to_string(),
        Some("HTTP Test Workspace".into()),
    );
    profile.id = "http-test-workspace".into();
    listener_state(vec![profile])
}

#[tokio::test]
async fn review_routes_keep_token_isolation_csp_and_non_cors_boundary() {
    let root = tempfile::tempdir().expect("review fixture workspace");
    let link = crate::review::create_patch_review(
        root.path(),
        "review-render-fixture",
        None,
        "</script><script>alert('unsafe')</script>",
        &[(
            "src/example.rs".into(),
            "update".into(),
            "old\n".into(),
            "new\n".into(),
        )],
    )
    .expect("create review");
    let profile = WorkspaceProfile::new(
        root.path().display().to_string(),
        Some("Review HTTP Fixture".into()),
    );
    let app = build_router(listener_state(vec![profile]));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("listen");
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();
    let url = format!("{base}/review/{}?t={}", link.change_id, link.token);

    let forbidden = client
        .get(format!("{base}/review/{}?t=invalid", link.change_id))
        .send()
        .await
        .unwrap();
    assert_eq!(forbidden.status(), reqwest::StatusCode::UNAUTHORIZED);

    let page = client
        .get(&url)
        .header("Origin", "https://untrusted.example")
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), reqwest::StatusCode::OK);
    assert_eq!(page.headers()["cache-control"], "no-store");
    assert_eq!(page.headers()["x-content-type-options"], "nosniff");
    assert!(page.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .contains("frame-ancestors 'none'"));
    assert!(page.headers().get("access-control-allow-origin").is_none());
    let body = page.text().await.unwrap();
    assert!(body.contains("<!doctype html>"));
    assert!(body.contains("const r={"));
    assert!(!body.contains("__REVIEW_DATA_JSON__"));
    assert!(!body.contains("</script><script>alert('unsafe')"));
    assert!(body.contains(r"\u003c/script>"));

    let api = client
        .get(format!(
            "{base}/api/reviews/{}?t={}",
            link.change_id, link.token
        ))
        .header("Origin", "https://untrusted.example")
        .send()
        .await
        .unwrap();
    assert_eq!(api.status(), reqwest::StatusCode::OK);
    assert!(api.headers().get("access-control-allow-origin").is_none());
    assert_eq!(api.headers()["cache-control"], "no-store");
    assert_eq!(
        api.json::<serde_json::Value>().await.unwrap()["id"],
        link.change_id
    );
    server.abort();
}

async fn initialize_session(client: &reqwest::Client, url: &str) -> String {
    let response = client
        .post(url)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "protocolVersion": "2025-11-25" }
        }))
        .send()
        .await
        .expect("initialize request");
    assert!(response.status().is_success());
    response
        .headers()
        .get(super::MCP_SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .expect("server-issued session header")
        .to_string()
}

async fn call_tool(
    client: &reqwest::Client,
    url: &str,
    session_id: &str,
    id: u64,
    name: &str,
    arguments: serde_json::Value,
) -> serde_json::Value {
    client
        .post(url)
        .header(super::MCP_SESSION_HEADER, session_id)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        }))
        .send()
        .await
        .expect("tool request")
        .json()
        .await
        .expect("tool response")
}

#[tokio::test]
async fn http_transport_issues_and_requires_server_owned_sessions() {
    let app = build_router(test_listener_state());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind test HTTP listener");
    let address = listener.local_addr().expect("test listener address");
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("test HTTP server");
    });
    let client = reqwest::Client::new();
    let url = format!("http://{address}/mcp");

    let initialize = client
        .post(&url)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "protocolVersion": "2025-11-25" }
        }))
        .send()
        .await
        .expect("initialize request");
    assert!(initialize.status().is_success());
    let session_id = initialize
        .headers()
        .get(super::MCP_SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .expect("server-issued session header")
        .to_string();
    assert!(!session_id.is_empty());

    let current = client
        .post(&url)
        .header(super::MCP_SESSION_HEADER, &session_id)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": "workspace_current", "arguments": {} }
        }))
        .send()
        .await
        .expect("workspace_current request");
    let current_body: serde_json::Value = current.json().await.expect("current response");
    assert_eq!(
        current_body["result"]["structuredContent"]["selected"],
        false
    );

    let forged = client
        .post(&url)
        .header(super::MCP_SESSION_HEADER, "client-chosen-session")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": { "name": "workspace_current", "arguments": {} }
        }))
        .send()
        .await
        .expect("forged session request");
    assert!(forged.headers().get(super::MCP_SESSION_HEADER).is_none());
    let forged_body: serde_json::Value = forged.json().await.expect("forged response");
    assert_eq!(
        forged_body["error"]["data"]["reason"],
        "MCP_SESSION_INVALID"
    );

    server.abort();
}

#[tokio::test]
async fn stateless_computer_use_requires_capability_and_never_trusts_metadata() {
    let app = build_router(test_listener_state());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("listen");
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();
    let call = |id: u64, name: &str, arguments: serde_json::Value| {
        client.post(&url).json(&json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": arguments,
                "_meta": { "openai/session": "untrusted-client-supplied-id" } }
        }))
    };

    let request: serde_json::Value = call(
        1,
        "computer_session",
        json!({"action": "request", "scope": "observe"}),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(request["result"]["isError"], false);
    let token = request["result"]["structuredContent"]["session_token"]
        .as_str()
        .expect("secret returned to stateless client");
    assert!(token.starts_with("cu1_") && token.len() == 68);
    assert_ne!(token, "untrusted-client-supplied-id");

    let status: serde_json::Value = call(
        2,
        "computer_session",
        json!({"action":"status", "session_token":token}),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(status["result"]["structuredContent"]["status"], "pending");

    let denied: serde_json::Value = call(3, "computer_observe", json!({"action":"windows"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        denied["result"]["structuredContent"]["error"],
        "COMPUTER_SESSION_REQUIRED"
    );
    let unapproved: serde_json::Value = call(
        4,
        "computer_observe",
        json!({"action":"windows", "session_token":token}),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert!(unapproved["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("COMPUTER_NOT_APPROVED"));
    let invalid: serde_json::Value = call(
        5,
        "computer_session",
        json!({"action":"status", "session_token":"cu1_invalid"}),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(
        invalid["result"]["structuredContent"]["error"],
        "COMPUTER_SESSION_REQUIRED"
    );

    let stopped: serde_json::Value = call(
        6,
        "computer_session",
        json!({"action":"stop", "session_token":token}),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(stopped["result"]["structuredContent"]["status"], "stopped");
    let revoked: serde_json::Value = call(
        7,
        "computer_session",
        json!({"action":"status", "session_token":token}),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(
        revoked["result"]["structuredContent"]["error"],
        "COMPUTER_SESSION_REQUIRED"
    );

    // A rotating/stale client header cannot authenticate an observation,
    // but it must not prevent an explicit, newly approved request either.
    let forged_header: serde_json::Value = client
        .post(&url)
        .header(super::MCP_SESSION_HEADER, "stale-client-session")
        .json(&json!({
            "jsonrpc":"2.0", "id":8, "method":"tools/call",
            "params": { "name":"computer_session",
                "arguments":{"action":"request","scope":"observe"} }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = forged_header["result"]["structuredContent"]["session_token"]
        .as_str()
        .expect("stale header uses explicit stateless capability instead");
    let stopped_again: serde_json::Value = call(
        9,
        "computer_session",
        json!({"action":"stop", "session_token":token}),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(
        stopped_again["result"]["structuredContent"]["status"],
        "stopped"
    );
    server.abort();
}

#[tokio::test]
async fn http_host_metadata_session_keeps_workspace_selection_without_transport_header() {
    let first = tempfile::tempdir().expect("workspace a");
    let second = tempfile::tempdir().expect("workspace b");
    fs::write(first.path().join("marker.txt"), "ONLY-A").expect("marker a");
    let mut first_profile =
        WorkspaceProfile::new(path_string(first.path()), Some("Workspace A".into()));
    first_profile.id = "workspace-a".into();
    let mut second_profile =
        WorkspaceProfile::new(path_string(second.path()), Some("Workspace B".into()));
    second_profile.id = "workspace-b".into();

    let app = build_router(listener_state(vec![first_profile, second_profile]));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind test HTTP listener");
    let address = listener.local_addr().expect("test listener address");
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("test HTTP server");
    });
    let client = reqwest::Client::new();
    let url = format!("http://{address}/mcp");

    let call = |id: u64, name: &str, arguments: serde_json::Value| {
        client.post(&url).json(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments,
                "_meta": { "openai/session": "chatgpt-conversation-a" }
            }
        }))
    };

    let selected: serde_json::Value = call(
        1,
        "workspace_select",
        json!({ "workspace_id": "workspace-a" }),
    )
    .send()
    .await
    .expect("select request")
    .json()
    .await
    .expect("select response");
    assert_eq!(selected["result"]["structuredContent"]["ok"], true);

    let read: serde_json::Value = call(2, "read_file", json!({ "path": "marker.txt" }))
        .send()
        .await
        .expect("read request")
        .json()
        .await
        .expect("read response");
    assert_eq!(read["result"]["structuredContent"]["content"], "ONLY-A");
    server.abort();
}

#[tokio::test]
async fn initialize_replaces_client_supplied_session_id() {
    let app = build_router(test_listener_state());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind test HTTP listener");
    let address = listener.local_addr().expect("test listener address");
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("test HTTP server");
    });
    let response = reqwest::Client::new()
        .post(format!("http://{address}/mcp"))
        .header(super::MCP_SESSION_HEADER, "client-chosen-session")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        }))
        .send()
        .await
        .expect("initialize request");
    let session_id = response
        .headers()
        .get(super::MCP_SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .expect("server-issued session header");
    assert_ne!(session_id, "client-chosen-session");
    server.abort();
}

#[tokio::test]
async fn http_sessions_keep_workspace_selection_isolated() {
    let first = tempfile::tempdir().expect("workspace a");
    let second = tempfile::tempdir().expect("workspace b");
    fs::write(first.path().join("marker.txt"), "ONLY-A").expect("marker a");
    fs::write(second.path().join("marker.txt"), "ONLY-B").expect("marker b");
    let mut first_profile =
        WorkspaceProfile::new(path_string(first.path()), Some("Workspace A".into()));
    first_profile.id = "workspace-a".into();
    let mut second_profile =
        WorkspaceProfile::new(path_string(second.path()), Some("Workspace B".into()));
    second_profile.id = "workspace-b".into();

    let app = build_router(listener_state(vec![first_profile, second_profile]));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind test HTTP listener");
    let address = listener.local_addr().expect("test listener address");
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("test HTTP server");
    });
    let client = reqwest::Client::new();
    let url = format!("http://{address}/mcp");
    let session_a = initialize_session(&client, &url).await;
    let session_b = initialize_session(&client, &url).await;

    let selected_a = call_tool(
        &client,
        &url,
        &session_a,
        2,
        "workspace_select",
        json!({ "workspace_id": "workspace-a" }),
    )
    .await;
    let selected_b = call_tool(
        &client,
        &url,
        &session_b,
        3,
        "workspace_select",
        json!({ "workspace_id": "workspace-b" }),
    )
    .await;
    assert_eq!(selected_a["result"]["structuredContent"]["ok"], true);
    assert_eq!(selected_b["result"]["structuredContent"]["ok"], true);

    let read_a = call_tool(
        &client,
        &url,
        &session_a,
        4,
        "read_file",
        json!({ "path": "marker.txt" }),
    )
    .await;
    let read_b = call_tool(
        &client,
        &url,
        &session_b,
        5,
        "read_file",
        json!({ "path": "marker.txt" }),
    )
    .await;
    assert_eq!(read_a["result"]["structuredContent"]["content"], "ONLY-A");
    assert_eq!(read_b["result"]["structuredContent"]["content"], "ONLY-B");
    server.abort();
}

#[tokio::test]
async fn http_request_scoped_workspace_id_survives_transport_session_churn() {
    let first = tempfile::tempdir().expect("workspace a");
    let second = tempfile::tempdir().expect("workspace b");
    fs::write(first.path().join("marker.txt"), "ONLY-A").expect("marker a");
    fs::write(second.path().join("marker.txt"), "ONLY-B").expect("marker b");
    let mut first_profile =
        WorkspaceProfile::new(path_string(first.path()), Some("Workspace A".into()));
    first_profile.id = "workspace-a".into();
    let mut second_profile =
        WorkspaceProfile::new(path_string(second.path()), Some("Workspace B".into()));
    second_profile.id = "workspace-b".into();

    let app = build_router(listener_state(vec![first_profile, second_profile]));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind test HTTP listener");
    let address = listener.local_addr().expect("test listener address");
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("test HTTP server");
    });
    let client = reqwest::Client::new();
    let url = format!("http://{address}/mcp");
    let session_a = initialize_session(&client, &url).await;
    let session_b = initialize_session(&client, &url).await;

    let tools: serde_json::Value = client
        .post(&url)
        .header(super::MCP_SESSION_HEADER, &session_a)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }))
        .send()
        .await
        .expect("tools/list request")
        .json()
        .await
        .expect("tools/list response");
    let read_file = tools["result"]["tools"]
        .as_array()
        .expect("tool catalog")
        .iter()
        .find(|tool| tool["name"] == "read_file")
        .expect("read_file tool");
    assert!(read_file["inputSchema"]["properties"]
        .get("workspace_id")
        .is_some());

    let read_a = call_tool(
        &client,
        &url,
        &session_a,
        3,
        "read_file",
        json!({ "workspace_id": "workspace-a", "path": "marker.txt" }),
    )
    .await;
    let read_after_churn = call_tool(
        &client,
        &url,
        &session_b,
        4,
        "read_file",
        json!({ "workspace_id": "workspace-a", "path": "marker.txt" }),
    )
    .await;
    assert_eq!(read_a["result"]["structuredContent"]["content"], "ONLY-A");
    assert_eq!(
        read_after_churn["result"]["structuredContent"]["content"],
        "ONLY-A"
    );
    server.abort();
}

#[tokio::test]
async fn computer_http_tool_catalog_and_default_deny() {
    let app = build_router(test_listener_state());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("listen");
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();
    let session = initialize_session(&client, &url).await;

    let tools: serde_json::Value = client
        .post(&url)
        .header(super::MCP_SESSION_HEADER, &session)
        .json(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let catalog = tools["result"]["tools"].as_array().unwrap();
    for name in ["computer_session", "computer_observe", "computer_action"] {
        assert!(catalog.iter().any(|tool| tool["name"] == name));
    }

    for (name, args) in [
        ("computer_observe", json!({"action":"screenshot"})),
        ("computer_action", json!({"action":"click","x":100,"y":100})),
    ] {
        let result = call_tool(&client, &url, &session, 3, name, args).await;
        assert_eq!(result["result"]["isError"], true);
        assert!(result["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains(if name == "computer_observe" {
                "COMPUTER_NOT_APPROVED"
            } else {
                "CONTROL_NOT_APPROVED"
            }));
    }
    let forged: serde_json::Value = client
        .post(&url)
        .json(&json!({
            "jsonrpc":"2.0","id":5,"method":"tools/call",
            "params":{"name":"computer_observe","arguments":{"action":"screenshot"},
                      "_meta":{"openai/session":"untrusted-client"}}
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        forged["result"]["structuredContent"]["error"],
        "COMPUTER_SESSION_REQUIRED"
    );
    server.abort();
}

fn path_string(path: &Path) -> String {
    path.display().to_string()
}
