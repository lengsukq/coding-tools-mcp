use std::sync::Arc;

use axum::extract::{Form, Query, State};
use axum::http::{
    header::{CACHE_CONTROL, WWW_AUTHENTICATE},
    HeaderMap, StatusCode,
};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use tokio::sync::oneshot;
use tower_http::cors::CorsLayer;

use crate::auth::{
    authorization_server_metadata, authorize_get, authorize_post, external_base_url,
    protected_resource_metadata, protected_resource_metadata_url, register_client, token_exchange,
    verify_bearer_header, verify_oauth_bearer_header, AuthorizeForm, AuthorizeParams,
    ClientRegistrationRequest, OAuthRuntime, TokenForm,
};
use crate::local_network;
use crate::mcp::audit::{log_request, record_response, RpcRequestMeta};
use crate::mcp::gateway::{
    session_id_from_metadata, session_id_from_request, GatewayState, MCP_SESSION_HEADER,
};
use crate::mcp::server::handle_gateway_request;
use crate::secret::SecretStore;
use crate::settings::AppSettings;
use crate::tunnel::append_profile_log;
use crate::workspace::AuthConfig;

pub type ShutdownSender = oneshot::Sender<()>;

#[derive(Debug, Clone)]
pub struct GatewayListenerConfig {
    pub port: u16,
    pub auth: AuthConfig,
    pub public_base_url: String,
}

#[derive(Debug, Clone, Default)]
pub struct ListenerSecrets {
    pub oauth_client_secret: Option<String>,
    pub oauth_password: Option<String>,
    pub oauth_token_secret: Option<String>,
}

async fn oauth_register_post(
    State(state): State<ListenerState>,
    Json(request): Json<ClientRegistrationRequest>,
) -> Response {
    let Some(oauth) = state.oauth.as_ref() else {
        return oauth_not_configured();
    };
    register_client(oauth, request)
}

#[derive(Clone)]
struct ListenerState {
    gateway: Arc<GatewayState>,
    auth: AuthConfig,
    scope_id: String,
    scope_label: String,
    bind_port: u16,
    configured_public_url: String,
    bearer_token: Option<String>,
    oauth: Option<Arc<OAuthRuntime>>,
    oauth_client_secret: Option<String>,
}

pub fn spawn_gateway_listener(
    config: GatewayListenerConfig,
    secrets: ListenerSecrets,
    gateway: Arc<GatewayState>,
) -> Result<(ShutdownSender, tauri::async_runtime::JoinHandle<()>), String> {
    let GatewayListenerConfig {
        port,
        auth,
        public_base_url,
    } = config;
    let ListenerSecrets {
        oauth_client_secret,
        oauth_password,
        oauth_token_secret,
    } = secrets;
    let bearer_token = if auth.bearer_enabled() {
        SecretStore::get_shared("bearer_token").map_err(|error| error.to_string())?
    } else {
        None
    };
    let configured_public_url = public_base_url.trim().to_string();
    let oauth = if auth.oauth_enabled() {
        let oauth_base = external_base_url(&HeaderMap::new(), port, &configured_public_url);
        Some(Arc::new(OAuthRuntime::new_app_persistent(
            oauth_base,
            auth.oauth_client_id.clone(),
            oauth_client_secret.clone(),
            oauth_password.unwrap_or_default(),
            oauth_token_secret.unwrap_or_default(),
            "global_mcp".into(),
            "oauth_dynamic_clients".into(),
        )?))
    } else {
        None
    };
    let state = ListenerState {
        gateway,
        auth,
        scope_id: "global-mcp".into(),
        scope_label: "Global MCP Gateway".into(),
        bind_port: port,
        configured_public_url,
        bearer_token,
        oauth,
        oauth_client_secret,
    };
    let allow_lan_access = AppSettings::load_or_default().allow_lan_access;
    let listener = bind_listener(port, allow_lan_access)?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let handle = tauri::async_runtime::spawn(async move {
        if let Err(error) = serve(listener, port, allow_lan_access, state, shutdown_rx).await {
            eprintln!("global mcp listener stopped: {error}");
        }
    });
    Ok((shutdown_tx, handle))
}

async fn serve(
    listener: tokio::net::TcpListener,
    port: u16,
    allow_lan_access: bool,
    state: ListenerState,
    shutdown: oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let profile_id = state.scope_id.clone();
    let app = build_router(state).layer(CorsLayer::permissive());

    append_profile_log(
        &profile_id,
        "stdout.log",
        &format!(
            "[mcp] listening on http://{}:{port}/mcp",
            local_network::bind_host(allow_lan_access)
        ),
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = shutdown.await;
        })
        .await?;
    Ok(())
}

fn build_router(state: ListenerState) -> Router {
    Router::new()
        .route("/mcp", get(mcp_discovery).post(mcp_post))
        .route(
            "/.well-known/oauth-authorization-server",
            get(oauth_authorization_server_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource",
            get(oauth_protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(oauth_protected_resource_metadata),
        )
        .route("/register", post(oauth_register_post))
        .route(
            "/oauth/authorize",
            get(oauth_authorize_get).post(oauth_authorize_post),
        )
        .route("/oauth/token", post(oauth_token_post))
        .with_state(state)
}

fn bind_listener(port: u16, allow_lan_access: bool) -> Result<tokio::net::TcpListener, String> {
    let addr = local_network::bind_addr(port, allow_lan_access);
    let listener = std::net::TcpListener::bind(addr)
        .map_err(|err| format!("MCP 本地端口 {port} 绑定失败: {err}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|err| format!("MCP 本地端口 {port} 设置非阻塞失败: {err}"))?;
    tokio::net::TcpListener::from_std(listener)
        .map_err(|err| format!("MCP 本地监听器初始化失败: {err}"))
}

async fn mcp_discovery() -> Response {
    ([(CACHE_CONTROL, "no-store")], Json(mcp_discovery_payload())).into_response()
}

fn mcp_discovery_payload() -> Value {
    json!({
        "name": "coding-tools-mcp",
        "version": env!("CARGO_PKG_VERSION"),
        "protocolVersion": crate::mcp::LATEST_PROTOCOL_VERSION,
        "supportedProtocolVersions": crate::mcp::SUPPORTED_PROTOCOL_VERSIONS
    })
}

fn resolve_oauth_base(state: &ListenerState, headers: &HeaderMap) -> String {
    external_base_url(headers, state.bind_port, &state.configured_public_url)
}

async fn mcp_post(
    State(state): State<ListenerState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Some(response) = require_mcp_auth(&state, &headers) {
        return response;
    }
    let meta = RpcRequestMeta::from_body(&body);

    let requested_session_id = session_id_from_request(&headers, &body);
    let host_session_id = if headers.get(MCP_SESSION_HEADER).is_none() {
        session_id_from_metadata(&body)
    } else {
        None
    };
    if meta.method != "initialize" {
        if let Some(session_id) = host_session_id.as_deref() {
            state.gateway.sessions.ensure_host_session(session_id);
        }
    }
    let effective_session_id = if meta.method == "initialize" {
        match requested_session_id.as_deref() {
            Some(session_id) if state.gateway.sessions.contains(session_id) => {
                Some(session_id.to_string())
            }
            _ => Some(state.gateway.sessions.issue()),
        }
    } else {
        requested_session_id
    };
    let response_session_id = effective_session_id
        .as_deref()
        .filter(|session_id| state.gateway.sessions.contains(session_id))
        .map(str::to_string);
    log_request(&state.scope_id, effective_session_id.as_deref(), &meta);

    let gateway = state.gateway.clone();
    let body_for_worker = body.clone();
    let session_for_worker = effective_session_id.clone();
    let result = tokio::task::spawn_blocking(move || {
        handle_gateway_request(
            gateway.as_ref(),
            session_for_worker.as_deref(),
            &body_for_worker,
        )
    })
    .await;
    match result {
        Ok(gateway_response) => {
            let response = gateway_response.body;
            if let Some(workspace) = gateway_response.workspace.as_ref() {
                record_response(
                    &workspace.tools,
                    &workspace.workspace_id,
                    effective_session_id.as_deref(),
                    &meta,
                    &response,
                );
            }
            let mut http_response = Json(response).into_response();
            if let Some(session_id) = response_session_id {
                if let Ok(value) = session_id.parse() {
                    http_response
                        .headers_mut()
                        .insert(MCP_SESSION_HEADER, value);
                }
            }
            http_response
        }
        Err(error) => {
            let error_response = json!({
                "jsonrpc": "2.0",
                "id": meta.request_id.clone(),
                "error": {
                    "code": -32603,
                    "message": "Exec RPC worker failed",
                    "data": {
                        "stage": "rpc_worker",
                        "reason": "worker_failed",
                        "retryable": true,
                        "suggestion": "重试请求或重启 MCP 运行时"
                    }
                }
            });
            append_profile_log(
                &state.scope_id,
                "mcp-requests.log",
                &format!(
                    "[rpc] worker_failed id={} method={} tool={} error={error}",
                    meta.request_id, meta.method, meta.tool_name
                ),
            );
            Json(error_response).into_response()
        }
    }
}

fn require_mcp_auth(state: &ListenerState, headers: &HeaderMap) -> Option<Response> {
    if state.auth.bearer_enabled() {
        let Some(expected) = state
            .bearer_token
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        else {
            return Some(
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Bearer authentication is enabled but no token is configured",
                )
                    .into_response(),
            );
        };
        return verify_bearer_header(headers, expected);
    }
    if state.auth.oauth_enabled() {
        if let Some(oauth) = state.oauth.as_ref() {
            let server_url = resolve_oauth_base(state, headers);
            if let Some(mut response) = verify_oauth_bearer_header(headers, oauth, &server_url) {
                if response.status() == StatusCode::UNAUTHORIZED {
                    let metadata_url = protected_resource_metadata_url(&server_url);
                    if let Ok(value) =
                        format!("Bearer resource_metadata=\"{metadata_url}\"").parse()
                    {
                        response.headers_mut().insert(WWW_AUTHENTICATE, value);
                    }
                }
                return Some(response);
            }
        }
    }
    if state.auth.auth_enabled() {
        return Some(
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "Unsupported or incomplete MCP authentication configuration",
            )
                .into_response(),
        );
    }
    None
}

async fn oauth_authorization_server_metadata(
    State(state): State<ListenerState>,
    headers: HeaderMap,
) -> Response {
    if !state.auth.oauth_enabled() {
        return oauth_not_configured();
    }
    let base = resolve_oauth_base(&state, &headers);
    Json(authorization_server_metadata(
        &base,
        state.oauth_client_secret.as_deref(),
    ))
    .into_response()
}

async fn oauth_protected_resource_metadata(
    State(state): State<ListenerState>,
    headers: HeaderMap,
) -> Response {
    if !state.auth.oauth_enabled() {
        return oauth_not_configured();
    }
    Json(protected_resource_metadata(&resolve_oauth_base(
        &state, &headers,
    )))
    .into_response()
}

async fn oauth_authorize_get(
    State(state): State<ListenerState>,
    headers: HeaderMap,
    Query(params): Query<AuthorizeParams>,
) -> Response {
    let Some(oauth) = state.oauth.as_ref() else {
        return oauth_not_configured();
    };
    authorize_get(
        oauth,
        params,
        Some(state.scope_label.as_str()),
        &resolve_oauth_base(&state, &headers),
    )
}

async fn oauth_authorize_post(
    State(state): State<ListenerState>,
    headers: HeaderMap,
    Form(form): Form<AuthorizeForm>,
) -> Response {
    let Some(oauth) = state.oauth.as_ref() else {
        return oauth_not_configured();
    };
    authorize_post(oauth, form, &resolve_oauth_base(&state, &headers))
}

async fn oauth_token_post(
    State(state): State<ListenerState>,
    headers: HeaderMap,
    Form(form): Form<TokenForm>,
) -> Response {
    let Some(oauth) = state.oauth.as_ref() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "unsupported_grant_type" })),
        )
            .into_response();
    };
    token_exchange(oauth, &headers, form, &resolve_oauth_base(&state, &headers))
}

fn oauth_not_configured() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "OAuth not configured" })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::{fs, path::Path};

    use axum::http::header::CACHE_CONTROL;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use serde_json::json;

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

    fn path_string(path: &Path) -> String {
        path.display().to_string()
    }
}
