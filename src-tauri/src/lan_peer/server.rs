use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{header::CACHE_CONTROL, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::local_network;
use crate::mcp::gateway::{GatewayError, GatewayState};
use crate::settings::{AppSettings, PairedPeerConfig};
use crate::tool_audit::{record_tool_call, ToolAuditCall};
use crate::tools::workspace::{tool_err, WorkspaceError};
use crate::tools::{call_tool, wrap_mcp_tool_result};

use super::crypto::{unix_time_ms, verify_peer_request_signature, ReplayGuard};
use super::discovery::PeerHelloDto;
use super::identity::{
    get_peer_secret, identity_from_settings, is_workspace_shared, remove_peer_secret,
    valid_device_id,
};
use super::pairing::{
    accept_pair_complete, is_pairing_pin_active, remove_local_paired_peer, PairCompleteRequest,
};
use super::{
    HOP_HEADER, LAN_PROTOCOL_VERSION, NONCE_HEADER, PEER_ID_HEADER, SIGNATURE_HEADER,
    TIMESTAMP_HEADER,
};

const PROXY_FORBIDDEN_HEADERS: &[&str] = &[
    "cf-connecting-ip",
    "cf-ray",
    "x-forwarded-for",
    "x-forwarded-host",
    "x-forwarded-proto",
    "x-real-ip",
    "forwarded",
];

#[derive(Clone)]
struct LanServerState {
    gateway: Arc<GatewayState>,
}

struct MaybeConnectInfo(Option<SocketAddr>);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for MaybeConnectInfo {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerWorkspaceWireDto {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerCatalogResponse {
    pub protocol: String,
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
    pub os: String,
    pub arch: String,
    pub version: String,
    pub trust_level: String,
    pub allow_computer_use: bool,
    pub computer_use_supported: bool,
    pub computer_use_enabled: bool,
    pub registry_revision: u64,
    pub workspaces: Vec<PeerWorkspaceWireDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PeerRpcRequest {
    SelectProbe {
        workspace_id: String,
        caller_session_id: String,
    },
    WorkspaceTool {
        workspace_id: String,
        tool: String,
        arguments: Value,
        caller_session_id: Option<String>,
        request_id: Value,
    },
    ComputerTool {
        tool: String,
        arguments: Value,
        caller_session_id: String,
        request_id: Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerReviewTokenDto {
    pub change_id: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerRpcResponse {
    pub result: Option<Value>,
    pub rpc_error: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_review: Option<PeerReviewTokenDto>,
}

#[derive(Deserialize)]
struct PeerReviewQuery {
    t: String,
}

static REPLAY_GUARD: OnceLock<ReplayGuard> = OnceLock::new();

fn replay_guard() -> &'static ReplayGuard {
    REPLAY_GUARD.get_or_init(ReplayGuard::default)
}

pub fn lan_router(gateway: Arc<GatewayState>) -> Router {
    Router::new()
        .route("/api/lan/v1/hello", get(lan_hello))
        .route("/api/lan/v1/pair/complete", post(lan_pair_complete))
        .route("/api/lan/v1/catalog", get(lan_catalog))
        .route("/api/lan/v1/rpc", post(lan_rpc))
        .route("/api/lan/v1/review/{id}", get(lan_review))
        .route("/api/lan/v1/unpair", post(lan_unpair))
        .with_state(LanServerState { gateway })
}

fn json_err(status: StatusCode, code: &str, message: impl Into<String>) -> Response {
    (
        status,
        [(CACHE_CONTROL, "no-store")],
        Json(json!({
            "ok": false,
            "code": code,
            "message": message.into(),
        })),
    )
        .into_response()
}

fn effective_settings(state: &LanServerState) -> AppSettings {
    state
        .gateway
        .settings_snapshot()
        .unwrap_or_else(AppSettings::load_or_default)
}

/// Hard isolation boundary: reject any request arriving via Cloudflare/FRP/reverse-proxy
/// headers or from a non-LAN public IP address, or when LAN Federation is disabled.
fn verify_lan_surface(
    state: &LanServerState,
    headers: &HeaderMap,
    remote: Option<SocketAddr>,
) -> Result<AppSettings, Response> {
    for forbidden in PROXY_FORBIDDEN_HEADERS {
        if headers.contains_key(*forbidden) {
            return Err(json_err(
                StatusCode::FORBIDDEN,
                "PUBLIC_TUNNEL_FORBIDDEN",
                "局域网配对与联邦接口禁止通过公网隧道或反向代理访问",
            ));
        }
    }
    let settings = effective_settings(state);
    if let Some(addr) = remote {
        let paired_hosts: Vec<String> = settings
            .lan_federation
            .paired_peers
            .iter()
            .flat_map(|p| std::iter::once(p.host.clone()).chain(p.addresses.iter().cloned()))
            .collect();
        if !local_network::is_allowed_peer_ip(
            addr.ip(),
            &settings.lan_federation.extra_subnets,
            &paired_hosts,
        ) {
            return Err(json_err(
                StatusCode::FORBIDDEN,
                "NON_LAN_ORIGIN_FORBIDDEN",
                "仅允许局域网私网、EasyTier/Tailscale 等组网地址或已配置的自定义子网访问此接口",
            ));
        }
    }
    if !settings.lan_federation.enabled {
        return Err(json_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "LAN_FEDERATION_DISABLED",
            "目标设备尚未开启局域网配对互联功能",
        ));
    }
    Ok(settings)
}

fn authenticate_peer_request(
    settings: &AppSettings,
    headers: &HeaderMap,
    method: &str,
    path_and_query: &str,
    body: &[u8],
) -> Result<PairedPeerConfig, Response> {
    let hop = headers
        .get(HOP_HEADER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u8>().ok())
        .unwrap_or(0);
    if hop != 1 {
        return Err(json_err(
            StatusCode::BAD_REQUEST,
            "FEDERATION_LOOP_DENIED",
            "局域网联邦仅允许单跳直连调用（hop=1），拒绝多跳转发",
        ));
    }

    let caller_id = headers
        .get(PEER_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| valid_device_id(s))
        .ok_or_else(|| json_err(StatusCode::UNAUTHORIZED, "MISSING_PEER_ID", "缺少有效的对等设备标识"))?;
    let timestamp_ms = headers
        .get(TIMESTAMP_HEADER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .ok_or_else(|| json_err(StatusCode::UNAUTHORIZED, "MISSING_TIMESTAMP", "缺少请求时间戳"))?;
    let nonce = headers
        .get(NONCE_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .ok_or_else(|| json_err(StatusCode::UNAUTHORIZED, "MISSING_NONCE", "缺少防重放 Nonce"))?;
    let signature = headers
        .get(SIGNATURE_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .ok_or_else(|| json_err(StatusCode::UNAUTHORIZED, "MISSING_SIGNATURE", "缺少请求签名"))?;

    let peer = settings
        .lan_federation
        .paired_peers
        .iter()
        .find(|p| p.device_id == caller_id)
        .ok_or_else(|| {
            json_err(
                StatusCode::UNAUTHORIZED,
                "UNKNOWN_PEER",
                format!("设备 {caller_id} 未与当前主机配对"),
            )
        })?;
    if !peer.enabled {
        return Err(json_err(
            StatusCode::FORBIDDEN,
            "PEER_DISABLED",
            "当前主机已停用来自该配对设备的访问",
        ));
    }
    let secret = get_peer_secret(caller_id, Some(settings))
        .or_else(|| get_peer_secret(caller_id, None))
        .ok_or_else(|| {
            json_err(
                StatusCode::UNAUTHORIZED,
                "PEER_SECRET_MISSING",
                "本地缺少该配对设备的共享密钥，请重新配对",
            )
        })?;

    let now_ms = unix_time_ms();
    if let Err(reason) = verify_peer_request_signature(
        &secret,
        caller_id,
        method,
        path_and_query,
        timestamp_ms,
        nonce,
        body,
        signature,
        now_ms,
    ) {
        return Err(json_err(
            StatusCode::UNAUTHORIZED,
            reason,
            format!("对等请求签名校验未通过: {reason}"),
        ));
    }

    if !replay_guard().check_and_record(caller_id, nonce, now_ms) {
        return Err(json_err(
            StatusCode::UNAUTHORIZED,
            "REPLAY_DETECTED",
            "检测到重复的 Nonce 请求，已拒绝重放",
        ));
    }

    Ok(peer.clone())
}

async fn lan_hello(
    State(state): State<LanServerState>,
    MaybeConnectInfo(remote): MaybeConnectInfo,
    headers: HeaderMap,
) -> Response {
    let settings = match verify_lan_surface(&state, &headers, remote) {
        Ok(s) => s,
        Err(resp) => return resp,
    };
    let override_snap = state.gateway.settings_snapshot();
    let (lan, fingerprint) = identity_from_settings(override_snap.as_ref());
    let payload = PeerHelloDto {
        protocol: LAN_PROTOCOL_VERSION.to_string(),
        device_id: lan.device_id,
        device_name: lan.device_name,
        fingerprint,
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        mcp_port: settings.global_gateway.local_port,
        pairing_accepting: is_pairing_pin_active(),
        computer_use_supported: cfg!(target_os = "macos"),
        computer_use_enabled: cfg!(target_os = "macos")
            && lan.allow_computer_use
            && crate::computer::local_policy_present(),
    };
    ([(CACHE_CONTROL, "no-store")], Json(payload)).into_response()
}

async fn lan_pair_complete(
    State(state): State<LanServerState>,
    MaybeConnectInfo(remote): MaybeConnectInfo,
    headers: HeaderMap,
    Json(req): Json<PairCompleteRequest>,
) -> Response {
    if let Err(resp) = verify_lan_surface(&state, &headers, remote) {
        return resp;
    }
    let override_snap = state.gateway.settings_snapshot();
    match accept_pair_complete(&req, remote.map(|s| s.ip()), override_snap.as_ref()) {
        Ok((ack, peer_record, shared_secret)) => {
            if state.gateway.is_test_override() {
                state.gateway.register_test_paired_peer(peer_record, shared_secret);
            }
            ([(CACHE_CONTROL, "no-store")], Json(ack)).into_response()
        }
        Err((status, code, message)) => json_err(
            StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST),
            code,
            message,
        ),
    }
}

async fn lan_catalog(
    State(state): State<LanServerState>,
    MaybeConnectInfo(remote): MaybeConnectInfo,
    headers: HeaderMap,
) -> Response {
    let settings = match verify_lan_surface(&state, &headers, remote) {
        Ok(s) => s,
        Err(resp) => return resp,
    };
    if let Err(resp) =
        authenticate_peer_request(&settings, &headers, "GET", "/api/lan/v1/catalog", &[])
    {
        return resp;
    }

    let override_snap = state.gateway.settings_snapshot();
    let (lan, fingerprint) = identity_from_settings(override_snap.as_ref());
    let descriptors = state
        .gateway
        .registry
        .local_descriptors()
        .unwrap_or_default()
        .into_iter()
        .filter(|ws| is_workspace_shared(&lan, &ws.id))
        .map(|ws| PeerWorkspaceWireDto {
            id: ws.id,
            name: ws.name,
            path: ws.path,
        })
        .collect();
    let revision = state.gateway.registry.revision().unwrap_or_default();

    let payload = PeerCatalogResponse {
        protocol: LAN_PROTOCOL_VERSION.to_string(),
        device_id: lan.device_id,
        device_name: lan.device_name,
        fingerprint,
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        trust_level: lan.trust_level,
        allow_computer_use: lan.allow_computer_use,
        computer_use_supported: cfg!(target_os = "macos"),
        computer_use_enabled: cfg!(target_os = "macos")
            && lan.allow_computer_use
            && crate::computer::local_policy_present(),
        registry_revision: revision,
        workspaces: descriptors,
    };
    ([(CACHE_CONTROL, "no-store")], Json(payload)).into_response()
}

async fn lan_rpc(
    State(state): State<LanServerState>,
    MaybeConnectInfo(remote): MaybeConnectInfo,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let settings = match verify_lan_surface(&state, &headers, remote) {
        Ok(s) => s,
        Err(resp) => return resp,
    };
    let caller_peer =
        match authenticate_peer_request(&settings, &headers, "POST", "/api/lan/v1/rpc", &body) {
            Ok(peer) => peer,
            Err(resp) => return resp,
        };

    let rpc_req: PeerRpcRequest = match serde_json::from_slice(&body) {
        Ok(req) => req,
        Err(err) => {
            return json_err(
                StatusCode::BAD_REQUEST,
                "INVALID_RPC_PAYLOAD",
                format!("无法解析局域网 RPC 请求体: {err}"),
            )
        }
    };

    let gateway = state.gateway.clone();
    let request_len = body.len();
    let response = tokio::task::spawn_blocking(move || {
        execute_local_peer_rpc(&gateway, &settings, &caller_peer, rpc_req, request_len)
    })
    .await;

    match response {
        Ok(payload) => ([(CACHE_CONTROL, "no-store")], Json(payload)).into_response(),
        Err(err) => json_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "PEER_WORKER_FAILED",
            format!("执行远端工具请求失败: {err}"),
        ),
    }
}

fn execute_local_peer_rpc(
    gateway: &GatewayState,
    settings: &AppSettings,
    caller_peer: &PairedPeerConfig,
    rpc_req: PeerRpcRequest,
    request_bytes: usize,
) -> PeerRpcResponse {
    let lan = &settings.lan_federation;
    let source_label = format!("{} ({})", caller_peer.device_name, caller_peer.device_id);

    match rpc_req {
        PeerRpcRequest::SelectProbe {
            workspace_id,
            caller_session_id,
        } => {
            if !is_workspace_shared(lan, &workspace_id) {
                return PeerRpcResponse {
                    result: None,
                    rpc_error: Some(
                        GatewayError::not_found(
                            "WORKSPACE_NOT_SHARED",
                            format!("目标设备未向局域网共享该工作区: {workspace_id}"),
                        )
                        .to_rpc_error(),
                    ),
                    peer_review: None,
                };
            }
            let scoped_session = format!("lan:{}:{caller_session_id}", caller_peer.device_id);
            match gateway.workspace_by_id(&scoped_session, &workspace_id) {
                Ok(ctx) => {
                    let instructions =
                        truncate_utf8(&ctx.tools.current_ai_instructions(), 16 * 1024);
                    PeerRpcResponse {
                        result: Some(json!({
                            "workspace": {
                                "id": ctx.profile.id,
                                "name": ctx.profile.name,
                                "path": ctx.profile.path,
                            },
                            "tool_profile": ctx.tools.tool_profile.as_str(),
                            "permission_mode": if lan.trust_level == "readonly" {
                                "readonly"
                            } else {
                                ctx.tools.policy.permission_mode.as_str()
                            },
                            "trust_level": lan.trust_level,
                            "history_recording": ctx.tools.history_recording,
                            "agent_instructions": instructions,
                        })),
                        rpc_error: None,
                        peer_review: None,
                    }
                }
                Err(err) => PeerRpcResponse {
                    result: None,
                    rpc_error: Some(err.to_rpc_error()),
                    peer_review: None,
                },
            }
        }
        PeerRpcRequest::WorkspaceTool {
            workspace_id,
            tool,
            mut arguments,
            caller_session_id,
            request_id,
        } => {
            if !is_workspace_shared(lan, &workspace_id) {
                return PeerRpcResponse {
                    result: None,
                    rpc_error: Some(
                        GatewayError::not_found(
                            "WORKSPACE_NOT_SHARED",
                            format!("目标设备未向局域网共享该工作区: {workspace_id}"),
                        )
                        .to_rpc_error(),
                    ),
                    peer_review: None,
                };
            }
            let canonical_name = crate::tools::registry::canonical_tool_name(&tool);
            let logical_session = format!(
                "lan:{}:{}",
                caller_peer.device_id,
                caller_session_id.as_deref().unwrap_or("stateless")
            );
            let audit_body = json!({
                "params": { "name": canonical_name, "arguments": &arguments }
            });
            let audit_call = ToolAuditCall::new_with_peer_context(
                &request_id,
                canonical_name,
                Some(&logical_session),
                "lan_peer",
                Some(&caller_peer.device_id),
                request_bytes,
                &audit_body,
                Some("local".into()),
                Some(source_label),
            );

            let ctx = match gateway.workspace_by_id(&logical_session, &workspace_id) {
                Ok(ctx) => ctx,
                Err(err) => {
                    let rpc_err = err.to_rpc_error();
                    record_tool_call(
                        audit_call,
                        Some(workspace_id),
                        &json!({ "error": &rpc_err }),
                    );
                    return PeerRpcResponse {
                        result: None,
                        rpc_error: Some(rpc_err),
                        peer_review: None,
                    };
                }
            };

            let gateway_surface = crate::tools::registry::exposed_tool_names("compact");
            let workspace_surface =
                crate::tools::registry::exposed_tool_names(ctx.tools.tool_profile.as_str());
            if !gateway_surface.contains(&canonical_name)
                || !workspace_surface.contains(&canonical_name)
            {
                let rpc_err = GatewayError::invalid(
                    "TOOL_NOT_AVAILABLE_IN_WORKSPACE",
                    format!(
                        "工具 {canonical_name} 不在目标设备工作区的可用档位 {} 中。",
                        ctx.tools.tool_profile.as_str()
                    ),
                )
                .to_rpc_error();
                record_tool_call(
                    audit_call,
                    Some(ctx.workspace_id),
                    &json!({ "error": &rpc_err }),
                );
                return PeerRpcResponse {
                    result: None,
                    rpc_error: Some(rpc_err),
                    peer_review: None,
                };
            }

            if lan.trust_level == "readonly" && !is_readonly_peer_tool(canonical_name, &arguments) {
                let denied = wrap_mcp_tool_result(
                    canonical_name,
                    &arguments,
                    tool_err(WorkspaceError::Tool {
                        code: "PEER_READONLY_POLICY",
                        message: format!(
                            "目标设备 {} 已将局域网共享权限设为只读，拒绝执行写入或命令工具 {canonical_name}",
                            lan.device_name
                        ),
                        category: "policy",
                        retryable: false,
                    }),
                );
                record_tool_call(
                    audit_call,
                    Some(ctx.workspace_id),
                    &json!({ "result": &denied }),
                );
                return PeerRpcResponse {
                    result: Some(denied),
                    rpc_error: None,
                    peer_review: None,
                };
            }

            if let Some(obj) = arguments.as_object_mut() {
                obj.remove("workspace_id");
                obj.remove("device_id");
            }
            if canonical_name == "history_manage" && arguments.get("_host_session_key").is_none() {
                arguments["_host_session_key"] = Value::String(logical_session.clone());
            }
            if canonical_name == "change_review"
                && arguments.get("scope").and_then(Value::as_str) == Some("session")
                && arguments.get("session_id").is_none()
            {
                if !arguments.is_object() {
                    arguments = json!({});
                }
                arguments["_review_session_id"] = Value::String(logical_session.clone());
            }

            let structured = call_tool(ctx.tools.as_ref(), canonical_name, &arguments);
            let mut peer_review = None;
            if let Some(change_id) = structured.get("change_id").and_then(Value::as_str) {
                let _ = crate::review::attach_session(
                    ctx.tools.workspace.root(),
                    change_id,
                    &logical_session,
                );
                if let Ok(link) = crate::review::issue_token(ctx.tools.workspace.root(), change_id)
                {
                    peer_review = Some(PeerReviewTokenDto {
                        change_id: link.change_id,
                        token: link.token,
                    });
                }
            }
            let result = wrap_mcp_tool_result(canonical_name, &arguments, structured);
            ctx.tools.record_context_block("tool_return", &result);
            record_tool_call(
                audit_call,
                Some(ctx.workspace_id),
                &json!({ "result": &result }),
            );
            PeerRpcResponse {
                result: Some(result),
                rpc_error: None,
                peer_review,
            }
        }
        PeerRpcRequest::ComputerTool {
            tool,
            mut arguments,
            caller_session_id,
            request_id,
        } => {
            let owner = format!("lan_peer:{}:{caller_session_id}", caller_peer.device_id);
            let session = gateway.sessions.computer_auto_session(&owner);
            if let Some(obj) = arguments.as_object_mut() {
                obj.remove("device_id");
                obj.remove("session_token");
            }
            let audit_body = json!({
                "params": { "name": &tool, "arguments": &arguments }
            });
            let audit_call = ToolAuditCall::new_with_peer_context(
                &request_id,
                &tool,
                Some(&session),
                "lan_peer",
                Some(&caller_peer.device_id),
                request_bytes,
                &audit_body,
                Some("local".into()),
                Some(source_label),
            );

            if !lan.allow_computer_use {
                let denied = json!({
                    "isError": true,
                    "content": [{ "type": "text", "text": "COMPUTER_LAN_DISABLED: 目标电脑已在局域网互联设置中关闭跨设备屏幕访问" }],
                    "structuredContent": { "ok": false, "error": "COMPUTER_LAN_DISABLED" }
                });
                record_tool_call(audit_call, None, &json!({ "result": &denied }));
                return PeerRpcResponse {
                    result: Some(denied),
                    rpc_error: None,
                    peer_review: None,
                };
            }

            let result = crate::computer::call_tool(&tool, Some(&session), &arguments);
            record_tool_call(audit_call, None, &json!({ "result": &result }));
            PeerRpcResponse {
                result: Some(result),
                rpc_error: None,
                peer_review: None,
            }
        }
    }
}

fn is_readonly_peer_tool(canonical_name: &str, args: &Value) -> bool {
    match canonical_name {
        "server_info"
        | "read_file"
        | "list_dir"
        | "list_files"
        | "search_text"
        | "grep_text"
        | "patch_check"
        | "git_status"
        | "git_diff"
        | "git_log"
        | "git_show"
        | "git_blame"
        | "view_image"
        | "change_review" => true,
        "planning_manage" => matches!(
            args.get("action").and_then(Value::as_str),
            Some("get" | "status" | "list")
        ),
        "history_manage" => matches!(
            args.get("action").and_then(Value::as_str),
            Some("search" | "read" | "list")
        ),
        _ => false,
    }
}

async fn lan_review(
    State(state): State<LanServerState>,
    MaybeConnectInfo(remote): MaybeConnectInfo,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<PeerReviewQuery>,
) -> Response {
    let settings = match verify_lan_surface(&state, &headers, remote) {
        Ok(s) => s,
        Err(resp) => return resp,
    };
    let path_and_query = format!("/api/lan/v1/review/{id}?t={}", query.t);
    if let Err(resp) = authenticate_peer_request(&settings, &headers, "GET", &path_and_query, &[]) {
        return resp;
    }

    let lan = &settings.lan_federation;
    let roots = match state.gateway.registry.list() {
        Ok(profiles) => profiles
            .into_iter()
            .filter(|p| is_workspace_shared(lan, &p.id))
            .map(|p| std::path::PathBuf::from(p.path)),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    match crate::review::find_review(roots, &id, &query.t) {
        Ok(review) => ([(CACHE_CONTROL, "no-store")], Json(review)).into_response(),
        Err(msg) => match msg.as_str() {
            "invalid review token" => StatusCode::UNAUTHORIZED.into_response(),
            "review expired" => StatusCode::GONE.into_response(),
            _ => StatusCode::NOT_FOUND.into_response(),
        },
    }
}

async fn lan_unpair(
    State(state): State<LanServerState>,
    MaybeConnectInfo(remote): MaybeConnectInfo,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let settings = match verify_lan_surface(&state, &headers, remote) {
        Ok(s) => s,
        Err(resp) => return resp,
    };
    let caller =
        match authenticate_peer_request(&settings, &headers, "POST", "/api/lan/v1/unpair", &body) {
            Ok(peer) => peer,
            Err(resp) => return resp,
        };
    if !state.gateway.is_test_override() {
        let _ = remove_local_paired_peer(&caller.device_id);
        let _ = remove_peer_secret(&caller.device_id);
    }
    ([(CACHE_CONTROL, "no-store")], Json(json!({ "ok": true }))).into_response()
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n…[truncated]", &value[..end])
}
