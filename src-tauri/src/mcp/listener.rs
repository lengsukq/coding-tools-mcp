use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{
    header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE, HOST, WWW_AUTHENTICATE},
    HeaderMap, StatusCode,
};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use tokio::sync::oneshot;
use tower_http::cors::CorsLayer;

use crate::auth::{
    authenticate_oauth_bearer_header, external_base_url, protected_resource_metadata_url,
    verify_bearer_header, OAuthRuntime,
};
use crate::local_network::{self, has_proxy_forwarded_headers, PeerConnectAddr};
use crate::mcp::audit::{log_request, record_response, RpcRequestMeta};
use crate::mcp::gateway::{
    session_id_from_metadata, session_id_from_request, GatewayState, MCP_SESSION_HEADER,
};
use crate::mcp::server::{
    audit_workspace_hint, handle_gateway_request, handle_gateway_upstream_tool_call_async,
};
use crate::secret::SecretStore;
use crate::settings::AppSettings;
use crate::tool_audit::{record_tool_call, ToolAuditCall};
use crate::tunnel::append_profile_log;
use crate::workspace::AuthConfig;

pub type ShutdownSender = oneshot::Sender<()>;

mod oauth_http;
mod review_http;

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
    } else if trimmed.parse::<std::net::IpAddr>().is_ok() {
        trimmed.as_str()
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
    remote.is_some_and(|addr| addr.ip().is_loopback())
        && !has_proxy_forwarded_headers(headers)
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
                    "message": "当前主机未开启局域网直连 /mcp；请在本机通过 127.0.0.1 连接以跨设备访问配对工作区，或在「设置 → 连接」中开启「允许局域网未配对客户端直连 /mcp」。"
                })),
            )
                .into_response(),
        );
    }
    if !auth_enabled
        && (remote.is_some_and(|addr| !addr.ip().is_loopback())
            || has_proxy_forwarded_headers(headers)
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

// Shared by OAuth endpoints and bearer token verification using an already-cloned policy snapshot.
pub(super) fn resolve_oauth_base_with_policy(
    policy: &LiveListenerPolicy,
    bind_port: u16,
    headers: &HeaderMap,
) -> String {
    external_base_url(headers, bind_port, &policy.configured_public_url)
}

/// The OAuth issuer may be public, but a loopback MCP client must see its own
/// exact resource URL in RFC9728 metadata. Never derive this from untrusted
/// forwarded headers; the comparison is deliberately limited to the listener.
pub(super) fn resolve_mcp_resource_base_with_policy(
    policy: &LiveListenerPolicy,
    bind_port: u16,
    headers: &HeaderMap,
) -> String {
    let loopback_host = format!("127.0.0.1:{bind_port}");
    if headers.get(HOST).and_then(|h| h.to_str().ok()) == Some(loopback_host.as_str())
        && !headers.contains_key("x-forwarded-host")
        && !headers.contains_key("forwarded")
    {
        return format!("http://{loopback_host}");
    }
    resolve_oauth_base_with_policy(policy, bind_port, headers)
}

#[cfg(test)]
fn resolve_mcp_resource_base(state: &ListenerState, headers: &HeaderMap) -> String {
    let policy = state.policy_snapshot();
    resolve_mcp_resource_base_with_policy(&policy, state.bind_port, headers)
}

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

/// Hot-reloadable security and endpoint policy shared by a running listener.
/// Swapping this behind `Arc<RwLock<...>>` applies auth, secret, loopback,
/// LAN-MCP, and public URL changes immediately without dropping connections
/// or restarting tunnels.
#[derive(Clone)]
pub struct LiveListenerPolicy {
    pub auth: AuthConfig,
    pub allow_lan_mcp: bool,
    pub allow_local_noauth: bool,
    pub configured_public_url: String,
    pub bearer_token: Option<String>,
    pub oauth: Option<Arc<OAuthRuntime>>,
}

pub struct SpawnedGatewayListener {
    pub shutdown: ShutdownSender,
    pub handle: crate::async_rt::JoinHandle<()>,
    pub policy: Arc<RwLock<LiveListenerPolicy>>,
    pub bound_port: u16,
    pub bound_all_interfaces: bool,
}

#[derive(Clone)]
struct ListenerState {
    gateway: Arc<GatewayState>,
    policy: Arc<RwLock<LiveListenerPolicy>>,
    scope_id: String,
    scope_label: String,
    bind_port: u16,
}

impl ListenerState {
    pub(super) fn policy_snapshot(&self) -> LiveListenerPolicy {
        self.policy
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

struct AuthenticatedRequest {
    method: &'static str,
    client_id: Option<String>,
    computer_principal: Option<String>,
    local_agent_trusted: bool,
}

struct ComputerSessionResolution {
    session_id: Option<String>,
    newly_issued_capability: Option<(String, String)>,
}

pub fn build_live_policy(
    config: GatewayListenerConfig,
    secrets: ListenerSecrets,
    allow_lan_mcp: bool,
    allow_local_noauth: bool,
) -> Result<LiveListenerPolicy, String> {
    let GatewayListenerConfig {
        port: _,
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
        let oauth_runtime = OAuthRuntime::new_app_persistent(
            auth.oauth_client_id.clone(),
            oauth_client_secret,
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
    Ok(LiveListenerPolicy {
        auth,
        allow_lan_mcp,
        allow_local_noauth,
        configured_public_url,
        bearer_token,
        oauth,
    })
}

pub fn spawn_gateway_listener(
    config: GatewayListenerConfig,
    secrets: ListenerSecrets,
    gateway: Arc<GatewayState>,
) -> Result<SpawnedGatewayListener, String> {
    let port = config.port;
    let settings = AppSettings::load_or_default();
    let allow_lan_access = settings.allow_lan_access || settings.lan_federation.enabled;
    let live_policy = build_live_policy(
        config,
        secrets,
        settings.allow_lan_access,
        settings.allow_local_noauth,
    )?;
    let policy = Arc::new(RwLock::new(live_policy));
    let state = ListenerState {
        gateway,
        policy: Arc::clone(&policy),
        scope_id: "global-mcp".into(),
        scope_label: "Global MCP Gateway".into(),
        bind_port: port,
    };
    let listener = bind_listener(port, allow_lan_access)?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let handle = crate::async_rt::spawn(async move {
        if let Err(error) = serve(listener, port, allow_lan_access, state, shutdown_rx).await {
            eprintln!("global mcp listener stopped: {error}");
        }
    });
    Ok(SpawnedGatewayListener {
        shutdown: shutdown_tx,
        handle,
        policy,
        bound_port: port,
        bound_all_interfaces: allow_lan_access,
    })
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

async fn mcp_discovery(State(state): State<ListenerState>) -> Response {
    let configured_name = state
        .gateway
        .settings_snapshot()
        .unwrap_or_else(AppSettings::load_or_default)
        .global_mcp_server_name;
    (
        [(CACHE_CONTROL, "no-store")],
        Json(mcp_discovery_payload_for(&configured_name)),
    )
        .into_response()
}

fn mcp_discovery_payload_for(configured_name: &str) -> Value {
    json!({
        "name": crate::settings::resolved_mcp_server_name(configured_name),
        "title": crate::settings::resolved_mcp_server_title(configured_name),
        "version": env!("CARGO_PKG_VERSION"),
        "protocolVersion": crate::mcp::LATEST_PROTOCOL_VERSION,
        "supportedProtocolVersions": crate::mcp::SUPPORTED_PROTOCOL_VERSIONS
    })
}

#[cfg(test)]
fn mcp_discovery_payload() -> Value {
    mcp_discovery_payload_for("")
}

async fn mcp_post(
    State(state): State<ListenerState>,
    PeerConnectAddr(remote): PeerConnectAddr,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let policy = state.policy_snapshot();
    let auth = match authenticate_mcp_request(&policy, state.bind_port, &headers, remote) {
        Ok(auth) => auth,
        Err(response) => return *response,
    };

    // Anti-self-loop guard: reject upstream requests originating from this exact instance.
    if let Some(origin) = headers
        .get(crate::upstream_mcp::UPSTREAM_ORIGIN_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        let own_device_id = state
            .gateway
            .settings_snapshot()
            .unwrap_or_else(AppSettings::load_or_default)
            .lan_federation
            .device_id;
        if !own_device_id.is_empty() && origin == own_device_id {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "jsonrpc": "2.0",
                    "id": body.get("id").cloned().unwrap_or(Value::Null),
                    "error": {
                        "code": -32602,
                        "message": "UPSTREAM_SELF_LOOP_BLOCKED: 检测到上游 MCP 聚合自环调用当前 Coding Tools MCP 实例，已强制拦截。",
                        "data": { "reason": "UPSTREAM_SELF_LOOP_BLOCKED", "retryable": false }
                    }
                })),
            )
                .into_response();
        }
    }

    let meta = RpcRequestMeta::from_body(&body);
    let computer_tool = meta.is_tool_call()
        && matches!(
            meta.tool_name.as_str(),
            "computer_session" | "computer_observe" | "computer_action"
        );
    let computer_resolution = if computer_tool {
        resolve_computer_session(&state, &policy, &auth, &headers, &meta, &body)
    } else {
        ComputerSessionResolution {
            session_id: None,
            newly_issued_capability: None,
        }
    };
    if computer_tool && computer_resolution.session_id.is_none() {
        return reject_missing_computer_session(&meta, &auth, body).await;
    }

    let (effective_session_id, response_session_id) = resolve_effective_session(
        &state,
        &auth,
        &headers,
        &meta,
        &body,
        computer_tool,
        computer_resolution.session_id,
    );
    // The session must survive the entire tool request, not merely the
    // initial touch. This guard releases after the worker returns.
    let _active_session = effective_session_id
        .as_deref()
        .and_then(|id| state.gateway.sessions.begin_request(id));

    let is_computer_stop = meta.tool_name == "computer_session"
        && body
            .pointer("/params/arguments/action")
            .and_then(Value::as_str)
            == Some("stop");

    // Warm any unsynced direct-mode upstream servers concurrently (bounded 4s) before `tools/list`.
    if meta.method == "tools/list" {
        let override_snap = state.gateway.settings_snapshot();
        state
            .gateway
            .upstream
            .warm_unsynced_direct_servers_arc(override_snap)
            .await;
    }

    // Pure async path for aggregated upstream MCP calls so slow external servers
    // never starve the bounded `spawn_blocking` worker pool.
    let is_upstream_call = meta.is_tool_call()
        && state
            .gateway
            .upstream
            .is_upstream_tool_name(&meta.tool_name, state.gateway.settings_snapshot().as_ref());
    if is_upstream_call {
        let gateway = Arc::clone(&state.gateway);
        let scope_id = state.scope_id.clone();
        log_request(&scope_id, effective_session_id.as_deref(), &meta);
        let audit_call = ToolAuditCall::new(
            &meta.request_id,
            &meta.tool_name,
            effective_session_id.as_deref(),
            auth.method,
            auth.client_id.as_deref(),
            meta.request_bytes,
            &body,
        );
        let gateway_response = handle_gateway_upstream_tool_call_async(
            &gateway,
            effective_session_id.as_deref(),
            &body,
        )
        .await;
        let response = gateway_response.body;
        let bytes = Bytes::from(serde_json::to_vec(&response).unwrap_or_else(|_| b"{}".to_vec()));
        let mut call = audit_call;
        call.set_execution_host(gateway_response.execution_host);
        record_tool_call(call, None, &response);
        return build_json_bytes_response(bytes, response_session_id);
    }

    let gateway = Arc::clone(&state.gateway);
    let scope_id = state.scope_id.clone();
    let meta_for_err = meta.clone();
    let scope_for_err = state.scope_id.clone();
    let result = tokio::task::spawn_blocking(move || {
        execute_and_finalize_rpc(
            &gateway,
            &scope_id,
            effective_session_id.as_deref(),
            &meta,
            &auth,
            &body,
            computer_resolution.newly_issued_capability,
            is_computer_stop,
        )
    })
    .await;

    match result {
        Ok(bytes) => build_json_bytes_response(bytes, response_session_id),
        Err(error) => handle_worker_failure(&scope_for_err, &meta_for_err, error).await,
    }
}

fn authenticate_mcp_request(
    policy: &LiveListenerPolicy,
    bind_port: u16,
    headers: &HeaderMap,
    remote: Option<SocketAddr>,
) -> Result<AuthenticatedRequest, Box<Response>> {
    if let Some(response) = check_mcp_surface_scope(
        policy.allow_lan_mcp,
        policy.auth.auth_enabled(),
        headers,
        remote,
    ) {
        return Err(Box::new(response));
    }
    if policy.allow_local_noauth
        && !headers.contains_key(AUTHORIZATION)
        && is_direct_loopback_agent(headers, remote)
    {
        return Ok(AuthenticatedRequest {
            method: "local_loopback",
            client_id: Some("loopback".into()),
            computer_principal: Some("local_loopback".into()),
            local_agent_trusted: true,
        });
    }
    authenticate_with_policy(policy, bind_port, headers)
}

/// Perform credential verification once and return the verified principal identity.
fn authenticate_with_policy(
    policy: &LiveListenerPolicy,
    bind_port: u16,
    headers: &HeaderMap,
) -> Result<AuthenticatedRequest, Box<Response>> {
    if policy.auth.bearer_enabled() {
        let Some(expected) = policy
            .bearer_token
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        else {
            return Err(Box::new(
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Bearer authentication is enabled but no token is configured",
                )
                    .into_response(),
            ));
        };
        if let Some(response) = verify_bearer_header(headers, expected) {
            return Err(Box::new(response));
        }
        return Ok(AuthenticatedRequest {
            method: "shared_bearer",
            client_id: None,
            computer_principal: Some("shared_bearer".into()),
            local_agent_trusted: false,
        });
    }
    if policy.auth.oauth_enabled() {
        if let Some(oauth) = policy.oauth.as_ref() {
            let server_url = resolve_oauth_base_with_policy(policy, bind_port, headers);
            return match authenticate_oauth_bearer_header(headers, oauth, &server_url) {
                Ok(client_id) => Ok(AuthenticatedRequest {
                    method: "oauth",
                    computer_principal: Some(format!("oauth:{client_id}")),
                    client_id: Some(client_id),
                    local_agent_trusted: false,
                }),
                Err(mut response) => {
                    if response.status() == StatusCode::UNAUTHORIZED {
                        let resource_base =
                            resolve_mcp_resource_base_with_policy(policy, bind_port, headers);
                        let metadata_url = protected_resource_metadata_url(&resource_base);
                        if let Ok(value) =
                            format!("Bearer resource_metadata=\"{metadata_url}\"").parse()
                        {
                            response.headers_mut().insert(WWW_AUTHENTICATE, value);
                        }
                    }
                    Err(response)
                }
            };
        }
    }
    if policy.auth.auth_enabled() {
        return Err(Box::new(
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "Unsupported or incomplete MCP authentication configuration",
            )
                .into_response(),
        ));
    }
    // Preserve legacy per-session request flow for local/no-auth clients.
    // An anonymous identity is NEVER eligible for local auto mode.
    Ok(AuthenticatedRequest {
        method: "none",
        client_id: None,
        computer_principal: Some("local_unauthenticated".into()),
        local_agent_trusted: false,
    })
}

fn resolve_computer_session(
    state: &ListenerState,
    policy: &LiveListenerPolicy,
    auth: &AuthenticatedRequest,
    headers: &HeaderMap,
    meta: &RpcRequestMeta,
    body: &Value,
) -> ComputerSessionResolution {
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
    let can_use_local_auto = auth.method != "none" && policy.auth.auth_enabled();
    let can_use_remote_auto = auth.local_agent_trusted || auth.method != "none";

    let session_id = match (auth.computer_principal.as_deref(), capability_arg) {
        (Some(owner), Some(token)) => state
            .gateway
            .sessions
            .resolve_computer_capability(token, owner),
        (Some(owner), None) if can_use_local_auto && crate::computer::local_policy_present() => {
            Some(state.gateway.sessions.computer_auto_session(owner))
        }
        (Some(owner), None) if can_use_remote_auto && remote_computer_target => {
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
                match state
                    .gateway
                    .sessions
                    .issue_computer_capability(&session, owner)
                {
                    Some(token) => {
                        newly_issued_capability = Some((session.clone(), token));
                        Some(session)
                    }
                    None => None,
                }
            } else {
                None
            }
        }
        _ => None,
    };

    ComputerSessionResolution {
        session_id,
        newly_issued_capability,
    }
}

async fn reject_missing_computer_session(
    meta: &RpcRequestMeta,
    auth: &AuthenticatedRequest,
    body: Value,
) -> Response {
    let denied = json!({
        "jsonrpc": "2.0",
        "id": meta.request_id,
        "result": {
            "isError": true,
            "content": [{ "type": "text", "text": "COMPUTER_SESSION_REQUIRED: 请先通过 computer_session(request) 获取短期 session_token 并在后续调用中传入，或保留服务端 MCP Session Header；_meta 不能用作电脑操作身份" }],
            "structuredContent": { "ok": false, "error": "COMPUTER_SESSION_REQUIRED" }
        }
    });
    let audit_call = ToolAuditCall::new(
        &meta.request_id,
        &meta.tool_name,
        None,
        auth.method,
        auth.client_id.as_deref(),
        meta.request_bytes,
        &body,
    );
    let denied_for_audit = denied.clone();
    let _ = tokio::task::spawn_blocking(move || {
        record_tool_call(audit_call, None, &denied_for_audit);
    })
    .await;
    Json(denied).into_response()
}

fn resolve_effective_session(
    state: &ListenerState,
    auth: &AuthenticatedRequest,
    headers: &HeaderMap,
    meta: &RpcRequestMeta,
    body: &Value,
    computer_tool: bool,
    computer_session: Option<String>,
) -> (Option<String>, Option<String>) {
    let requested_session_id = session_id_from_request(headers, body);
    let host_session_id = if headers.get(MCP_SESSION_HEADER).is_none() {
        session_id_from_metadata(body)
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
                if auth.computer_principal.as_deref().is_some_and(|owner| {
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
                    .issue_with_owner(auth.computer_principal.clone()),
            ),
        }
    } else {
        requested_session_id
    };

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

    (effective_session_id, response_session_id)
}

#[allow(clippy::too_many_arguments)]
fn execute_and_finalize_rpc(
    gateway: &GatewayState,
    scope_id: &str,
    session_id: Option<&str>,
    meta: &RpcRequestMeta,
    auth: &AuthenticatedRequest,
    body: &Value,
    newly_issued_capability: Option<(String, String)>,
    is_computer_stop: bool,
) -> Bytes {
    log_request(scope_id, session_id, meta);

    let audit_call = meta.is_tool_call().then(|| {
        ToolAuditCall::new(
            &meta.request_id,
            &meta.tool_name,
            session_id,
            auth.method,
            auth.client_id.as_deref(),
            meta.request_bytes,
            body,
        )
    });
    let audit_workspace_id = audit_call
        .as_ref()
        .and_then(|_| audit_workspace_hint(gateway, session_id, body));

    let gateway_response = handle_gateway_request(gateway, session_id, body);
    let mut response = gateway_response.body;
    let tool_succeeded = response["result"]["isError"] == false;

    if let Some((session, token)) = newly_issued_capability {
        if tool_succeeded {
            response["result"]["structuredContent"]["session_token"] = json!(token);
            let payload = response["result"]["structuredContent"].to_string();
            response["result"]["content"][0]["text"] = json!(payload);
        } else {
            gateway.sessions.revoke_computer_capabilities(&session);
        }
    }
    if tool_succeeded && is_computer_stop {
        if let Some(session) = session_id {
            gateway.sessions.revoke_computer_capabilities(session);
        }
    }

    // Serialize once into `Bytes`: used both to measure exact response size
    // and as the direct HTTP body without a second JSON serialization pass.
    let bytes = Bytes::from(serde_json::to_vec(&response).unwrap_or_else(|_| b"{}".to_vec()));
    let response_bytes = bytes.len();

    if let Some(workspace) = gateway_response.workspace.as_ref() {
        record_response(
            &workspace.tools,
            &workspace.workspace_id,
            session_id,
            meta,
            &response,
            response_bytes,
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
    bytes
}

fn build_json_bytes_response(bytes: Bytes, response_session_id: Option<String>) -> Response {
    let mut http_response =
        (StatusCode::OK, [(CONTENT_TYPE, "application/json")], bytes).into_response();
    if let Some(session_id) = response_session_id {
        if let Ok(value) = session_id.parse() {
            http_response
                .headers_mut()
                .insert(MCP_SESSION_HEADER, value);
        }
    }
    http_response
}

async fn handle_worker_failure(
    scope_id: &str,
    meta: &RpcRequestMeta,
    error: tokio::task::JoinError,
) -> Response {
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
    let scope_id = scope_id.to_string();
    let log_line = format!(
        "[rpc] worker_failed id={} method={} tool={} error={error}",
        meta.request_id, meta.method, meta.tool_name
    );
    let _ = tokio::task::spawn_blocking(move || {
        append_profile_log(&scope_id, "mcp-requests.log", &log_line);
    })
    .await;
    Json(error_response).into_response()
}

#[cfg(test)]
fn require_mcp_auth(state: &ListenerState, headers: &HeaderMap) -> Option<Response> {
    let policy = state.policy_snapshot();
    authenticate_with_policy(&policy, state.bind_port, headers)
        .err()
        .map(|resp| *resp)
}

#[cfg(test)]
mod tests;
