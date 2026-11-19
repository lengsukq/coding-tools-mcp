use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{
    header::{AUTHORIZATION, CACHE_CONTROL, HOST, WWW_AUTHENTICATE},
    HeaderMap, StatusCode,
};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use tokio::sync::oneshot;
use tower_http::cors::CorsLayer;

use crate::auth::{
    external_base_url, protected_resource_metadata_url, verify_bearer_header,
    verify_oauth_bearer_header, OAuthRuntime,
};
use crate::local_network;
use crate::mcp::audit::{log_request, record_response, RpcRequestMeta};
use crate::mcp::gateway::{
    session_id_from_metadata, session_id_from_request, GatewayState, MCP_SESSION_HEADER,
};
use crate::mcp::server::{audit_workspace_hint, handle_gateway_request};
use crate::secret::SecretStore;
use crate::settings::AppSettings;
use crate::tool_audit::{record_tool_call, ToolAuditCall};
use crate::tunnel::append_profile_log;
use crate::workspace::AuthConfig;

pub type ShutdownSender = oneshot::Sender<()>;

mod oauth_http;
mod review_http;

const PROXY_FORWARDED_HEADERS: &[&str] = &[
    "cf-connecting-ip",
    "cf-ray",
    "x-forwarded-for",
    "x-forwarded-host",
    "x-forwarded-proto",
    "x-real-ip",
    "forwarded",
    "via",
    "true-client-ip",
];

struct PeerConnectAddr(Option<SocketAddr>);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for PeerConnectAddr {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let addr = parts
            .extensions
            .get::<axum::extract::ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0);
        Ok(Self(addr))
    }
}

fn is_loopback_host(headers: &HeaderMap) -> bool {
    let Some(raw) = headers.get(HOST).and_then(|v| v.to_str().ok()) else {
        return true;
    };
    let trimmed = raw.trim().to_ascii_lowercase();
    let host_only = if let Some(rest) = trimmed.strip_prefix('[') {
        match rest.split_once(']') {
            Some((ipv6, _)) => ipv6,
            None => return false,
        }
    } else if let Some((h, _)) = trimmed.rsplit_once(':') {
        if h.contains(':') {
            return false;
        }
        h
    } else {
        trimmed.as_str()
    };
    matches!(host_only, "127.0.0.1" | "localhost" | "::1")
}

fn is_direct_loopback_agent(headers: &HeaderMap, remote: Option<SocketAddr>) -> bool {
    let via_proxy = PROXY_FORWARDED_HEADERS
        .iter()
        .any(|name| headers.contains_key(*name));
    remote.is_some_and(|addr| addr.ip().is_loopback())
        && !via_proxy
        && is_loopback_host(headers)
        && headers.get("origin").is_none()
}

fn check_mcp_surface_scope(
    allow_lan_mcp: bool,
    auth_enabled: bool,
    headers: &HeaderMap,
    remote: Option<SocketAddr>,
) -> Option<Response> {
    if remote.is_some_and(|addr| !addr.ip().is_loopback()) && !allow_lan_mcp {
        return Some(
            (
                StatusCode::FORBIDDEN,
                Json(json!({
                    "ok": false,
                    "code": "LAN_MCP_DISABLED",
                    "message": "当前主机未开启局域网直连 /mcp；请在本机通过 127.0.0.1 连接以跨设备访问配对工作区，或在设置中开启「允许局域网访问」。"
                })),
            )
                .into_response(),
        );
    }
    let via_proxy = PROXY_FORWARDED_HEADERS
        .iter()
        .any(|name| headers.contains_key(*name));
    if !auth_enabled
        && (remote.is_some_and(|addr| !addr.ip().is_loopback())
            || via_proxy
            || !is_loopback_host(headers)
            || headers.get("origin").is_some())
    {
        return Some(
            (
                StatusCode::FORBIDDEN,
                Json(json!({
                    "ok": false,
                    "code": "NOAUTH_LOOPBACK_ONLY",
                    "message": "无认证模式仅允许本机 127.0.0.1 非浏览器客户端直接访问，禁止通过局域网、组网网卡或公网隧道无认证调用。"
                })),
            )
                .into_response(),
        );
    }
    None
}

// Shared by OAuth endpoints and bearer token verification.
fn resolve_oauth_base(state: &ListenerState, headers: &HeaderMap) -> String {
    external_base_url(headers, state.bind_port, &state.configured_public_url)
}

/// The OAuth issuer may be public, but a loopback MCP client must see its own
/// exact resource URL in RFC9728 metadata. Never derive this from untrusted
/// forwarded headers; the comparison is deliberately limited to the listener.
fn resolve_mcp_resource_base(state: &ListenerState, headers: &HeaderMap) -> String {
    let loopback_host = format!("127.0.0.1:{}", state.bind_port);
    if headers.get(HOST).and_then(|h| h.to_str().ok()) == Some(loopback_host.as_str())
        && !headers.contains_key("x-forwarded-host")
        && !headers.contains_key("forwarded")
    {
        return format!("http://{loopback_host}");
    }
    resolve_oauth_base(state, headers)
}

#[derive(Debug, Clone)]
pub struct GatewayListenerConfig {
    pub port: u16,
    pub auth: AuthConfig,
    pub public_base_url: String,
}

/// Identity is sourced only from the verified auth layer, not RPC parameters.
/// A shared bearer token inherently represents one shared identity; OAuth is
/// bound to the authenticated client ID. Missing OAuth identity fails closed
/// for host-level Computer Use while ordinary Workspace tools still work.
fn computer_auth_principal(state: &ListenerState, headers: &HeaderMap) -> Option<String> {
    let (method, client_id) = audit_auth_identity(state, headers);
    match method {
        "oauth" => client_id.map(|id| format!("oauth:{id}")),
        "shared_bearer" => Some("shared_bearer".into()),
        // Preserve legacy per-session request flow for local/no-auth clients.
        // An anonymous identity is NEVER eligible for the new local auto mode.
        "none" => Some("local_unauthenticated".into()),
        _ => None,
    }
}

#[derive(Debug, Clone, Default)]
pub struct ListenerSecrets {
    pub oauth_client_secret: Option<String>,
    pub oauth_password: Option<String>,
    pub oauth_token_secret: Option<String>,
}

#[derive(Clone)]
struct ListenerState {
    gateway: Arc<GatewayState>,
    auth: AuthConfig,
    scope_id: String,
    scope_label: String,
    bind_port: u16,
    allow_lan_mcp: bool,
    allow_local_noauth: bool,
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
        let oauth_runtime = OAuthRuntime::new_app_persistent(
            oauth_base,
            auth.oauth_client_id.clone(),
            oauth_client_secret.clone(),
            oauth_password.unwrap_or_default(),
            oauth_token_secret.unwrap_or_default(),
            "global_mcp".into(),
            "oauth_dynamic_clients".into(),
        )?;
        if let Some(notice) = oauth_runtime.recovery_notice() {
            append_profile_log("global-mcp", "stderr.log", notice);
        }
        Some(Arc::new(oauth_runtime))
    } else {
        None
    };
    let settings = AppSettings::load_or_default();
    let allow_lan_access = settings.allow_lan_access || settings.lan_federation.enabled;
    let state = ListenerState {
        gateway,
        auth,
        scope_id: "global-mcp".into(),
        scope_label: "Global MCP Gateway".into(),
        bind_port: port,
        allow_lan_mcp: settings.allow_lan_access,
        allow_local_noauth: settings.allow_local_noauth,
        configured_public_url,
        bearer_token,
        oauth,
        oauth_client_secret,
    };
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
    // CORS is intentionally applied only to the MCP/OAuth protocol router.
    // Review links and LAN peer endpoints must not inherit permissive CORS.
    let app = build_router(state).into_make_service_with_connect_info::<SocketAddr>();

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
    let lan_gateway = state.gateway.clone();
    let protocol = Router::new()
        .route("/mcp", get(mcp_discovery).post(mcp_post))
        .route(
            "/.well-known/oauth-authorization-server",
            get(oauth_http::oauth_authorization_server_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource",
            get(oauth_http::oauth_protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(oauth_http::oauth_protected_resource_metadata),
        )
        .route("/register", post(oauth_http::oauth_register_post))
        .route(
            "/oauth/authorize",
            get(oauth_http::oauth_authorize_get).post(oauth_http::oauth_authorize_post),
        )
        .route("/oauth/token", post(oauth_http::oauth_token_post))
        .layer(CorsLayer::permissive());
    let review = Router::new()
        .route("/review/{id}", get(review_http::review_page))
        .route("/api/reviews/{id}", get(review_http::review_api))
        .route("/api/reviews/{id}/diff", get(review_http::review_api))
        .route(
            "/review/peer/{device_id}/{id}",
            get(review_http::peer_review_page),
        )
        .route(
            "/api/reviews/peer/{device_id}/{id}",
            get(review_http::peer_review_api),
        )
        .route(
            "/api/reviews/peer/{device_id}/{id}/diff",
            get(review_http::peer_review_api),
        );
    protocol
        .merge(review)
        .with_state(state)
        .merge(crate::lan_peer::lan_router(lan_gateway))
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

async fn mcp_post(
    State(state): State<ListenerState>,
    PeerConnectAddr(remote): PeerConnectAddr,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Some(response) = check_mcp_surface_scope(
        state.allow_lan_mcp,
        state.auth.auth_enabled(),
        &headers,
        remote,
    ) {
        return response;
    }
    let local_agent_trusted = state.allow_local_noauth
        && !headers.contains_key(AUTHORIZATION)
        && is_direct_loopback_agent(&headers, remote);
    if !local_agent_trusted {
        if let Some(response) = require_mcp_auth(&state, &headers) {
            return response;
        }
    }
    let meta = RpcRequestMeta::from_body(&body);
    let (effective_auth_method, effective_client_id): (&'static str, Option<String>) =
        if local_agent_trusted {
            ("local_loopback", Some("loopback".into()))
        } else {
            audit_auth_identity(&state, &headers)
        };
    let computer_principal = if local_agent_trusted {
        Some("local_loopback".into())
    } else {
        computer_auth_principal(&state, &headers)
    };
    let computer_tool = meta.method == "tools/call"
        && matches!(
            meta.tool_name.as_str(),
            "computer_session" | "computer_observe" | "computer_action"
        );
    let capability_arg = body
        .pointer("/params/arguments/session_token")
        .and_then(Value::as_str);
    let remote_computer_target = body
        .pointer("/params/arguments/device_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|dev| !dev.is_empty() && dev != "local" && dev != "self");
    // A capability is an explicit, short-lived bearer secret for MCP clients
    // that do not retain the transport header. Neither workspace_id nor _meta
    // can become a Computer Use identity.
    let mut newly_issued_capability: Option<(String, String)> = None;
    let computer_session = if computer_tool {
        match (computer_principal.as_deref(), capability_arg) {
            (Some(owner), Some(token)) => state
                .gateway
                .sessions
                .resolve_computer_capability(token, owner),
            (Some(owner), None)
                if audit_auth_identity(&state, &headers).0 != "none"
                    && crate::computer::local_policy_present() =>
            {
                // Stable per-authenticated-principal identity: no ChatGPT
                // transport header, request _meta, or session_token required.
                // Only the local webview can enable this policy.
                Some(state.gateway.sessions.computer_auto_session(owner))
            }
            (Some(owner), None)
                if (local_agent_trusted || audit_auth_identity(&state, &headers).0 != "none")
                    && remote_computer_target =>
            {
                // Remote LAN Computer Use target: mint a transport-bound principal
                // session so dispatch can forward to the paired Mac, which enforces
                // its own local Computer Use policy and TCC independently.
                Some(state.gateway.sessions.computer_auto_session(owner))
            }
            (Some(owner), None) => {
                let header_session = headers
                    .get(MCP_SESSION_HEADER)
                    .and_then(|value| value.to_str().ok());
                if let Some(header_session) = header_session.filter(|session| {
                    state
                        .gateway
                        .sessions
                        .computer_owner_matches(session, owner)
                }) {
                    Some(header_session.to_string())
                } else if meta.tool_name == "computer_session"
                    && body
                        .pointer("/params/arguments/action")
                        .and_then(Value::as_str)
                        == Some("request")
                {
                    let session = state
                        .gateway
                        .sessions
                        .issue_with_owner(Some(owner.to_string()));
                    let token = state
                        .gateway
                        .sessions
                        .issue_computer_capability(&session, owner)
                        .expect("newly issued Computer Use session has matching owner");
                    newly_issued_capability = Some((session.clone(), token));
                    Some(session)
                } else {
                    None
                }
            }
            _ => None,
        }
    } else {
        None
    };
    if computer_tool && computer_session.is_none() {
        let denied = json!({
            "jsonrpc": "2.0",
            "id": meta.request_id,
            "result": {
                "isError": true,
                "content": [{ "type": "text", "text": "COMPUTER_SESSION_REQUIRED: 请先通过 computer_session(request) 获取短期 session_token 并在后续调用中传入，或保留服务端 MCP Session Header；_meta 不能用作电脑操作身份" }],
                "structuredContent": { "ok": false, "error": "COMPUTER_SESSION_REQUIRED" }
            }
        });
        record_tool_call(
            ToolAuditCall::new(
                &meta.request_id,
                &meta.tool_name,
                None,
                effective_auth_method,
                effective_client_id.as_deref(),
                serde_json::to_vec(&body)
                    .map(|v| v.len())
                    .unwrap_or_default(),
                &body,
            ),
            None,
            &denied,
        );
        return Json(denied).into_response();
    }

    let requested_session_id = session_id_from_request(&headers, &body);
    let host_session_id = if headers.get(MCP_SESSION_HEADER).is_none() {
        session_id_from_metadata(&body)
    } else {
        None
    };
    if meta.method != "initialize" && !computer_tool {
        if let Some(session_id) = host_session_id.as_deref() {
            state.gateway.sessions.ensure_host_session(session_id);
        }
    }
    let effective_session_id = if computer_tool {
        computer_session
    } else if meta.method == "initialize" {
        match requested_session_id.as_deref() {
            Some(session_id)
                if computer_principal.as_deref().is_some_and(|owner| {
                    state
                        .gateway
                        .sessions
                        .computer_owner_matches(session_id, owner)
                }) =>
            {
                Some(session_id.to_string())
            }
            _ => Some(
                state
                    .gateway
                    .sessions
                    .issue_with_owner(computer_principal.clone()),
            ),
        }
    } else {
        requested_session_id
    };
    // The session must survive the entire tool request, not merely the
    // initial touch. This guard releases after the worker returns.
    let _active_session = effective_session_id
        .as_deref()
        .and_then(|id| state.gateway.sessions.begin_request(id));
    let response_session_id = effective_session_id
        .as_deref()
        .filter(|session_id| {
            (meta.method == "initialize"
                || headers
                    .get(MCP_SESSION_HEADER)
                    .and_then(|header| header.to_str().ok())
                    == Some(*session_id))
                && state.gateway.sessions.contains(session_id)
        })
        .map(str::to_string);
    log_request(&state.scope_id, effective_session_id.as_deref(), &meta);

    let audit_call = if meta.method == "tools/call" {
        let request_bytes = serde_json::to_vec(&body)
            .map(|bytes| bytes.len())
            .unwrap_or_default();
        Some(ToolAuditCall::new(
            &meta.request_id,
            &meta.tool_name,
            effective_session_id.as_deref(),
            effective_auth_method,
            effective_client_id.as_deref(),
            request_bytes,
            &body,
        ))
    } else {
        None
    };
    let audit_workspace_id = audit_call
        .as_ref()
        .and_then(|_| audit_workspace_hint(&state.gateway, effective_session_id.as_deref(), &body));

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
            let mut response = gateway_response.body;
            let tool_succeeded = response["result"]["isError"] == false;
            if let Some((session, token)) = newly_issued_capability {
                if tool_succeeded {
                    response["result"]["structuredContent"]["session_token"] = json!(token);
                    let payload = response["result"]["structuredContent"].to_string();
                    response["result"]["content"][0]["text"] = json!(payload);
                } else {
                    state
                        .gateway
                        .sessions
                        .revoke_computer_capabilities(&session);
                }
            }
            if tool_succeeded
                && meta.tool_name == "computer_session"
                && body
                    .pointer("/params/arguments/action")
                    .and_then(Value::as_str)
                    == Some("stop")
            {
                if let Some(session) = effective_session_id.as_deref() {
                    state.gateway.sessions.revoke_computer_capabilities(session);
                }
            }
            if let Some(workspace) = gateway_response.workspace.as_ref() {
                record_response(
                    &workspace.tools,
                    &workspace.workspace_id,
                    effective_session_id.as_deref(),
                    &meta,
                    &response,
                );
            }
            if let Some(mut call) = audit_call {
                call.set_execution_host(gateway_response.execution_host);
                let workspace_id = gateway_response
                    .workspace
                    .as_ref()
                    .map(|workspace| workspace.workspace_id.clone())
                    .or(gateway_response.federated_workspace_id)
                    .or(audit_workspace_id);
                record_tool_call(call, workspace_id, &response);
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
            if let Some(call) = audit_call {
                record_tool_call(call, audit_workspace_id, &error_response);
            }
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

fn audit_auth_identity(
    state: &ListenerState,
    headers: &HeaderMap,
) -> (&'static str, Option<String>) {
    if state.auth.bearer_enabled() {
        return ("shared_bearer", None);
    }
    if state.auth.oauth_enabled() {
        let client_id = state.oauth.as_ref().and_then(|oauth| {
            let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
            let token = value.strip_prefix("Bearer ")?.trim();
            oauth.access_token_client_id(token, &resolve_oauth_base(state, headers))
        });
        return ("oauth", client_id);
    }
    ("none", None)
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
            let mut response = verify_oauth_bearer_header(headers, oauth, &server_url)?;
            if response.status() == StatusCode::UNAUTHORIZED {
                let metadata_url =
                    protected_resource_metadata_url(&resolve_mcp_resource_base(state, headers));
                if let Ok(value) = format!("Bearer resource_metadata=\"{metadata_url}\"").parse() {
                    response.headers_mut().insert(WWW_AUTHENTICATE, value);
                }
            }
            return Some(response);
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

#[cfg(test)]
mod tests;
