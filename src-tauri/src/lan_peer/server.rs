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

use crate::local_network::{self, has_proxy_forwarded_headers, PeerConnectAddr};
use crate::mcp::gateway::{GatewayError, GatewayState, WorkspaceRegistry};
use crate::mcp::server::{
    inject_workspace_session_args, review_error_status, truncate_utf8_with_ellipsis,
};
use crate::settings::{AppSettings, PairedPeerConfig};
use crate::tool_audit::{record_tool_call, ToolAuditCall};
use crate::tools::workspace::{tool_err, WorkspaceError};
use crate::tools::{call_tool, wrap_mcp_tool_result};

use super::crypto::{unix_time_ms, verify_peer_request_signature, ReplayGuard};
use super::discovery::PeerHelloDto;
use super::identity::{
    get_peer_secret, identity_from_settings, is_workspace_shared, valid_device_id,
};
use super::pairing::{
    accept_pair_complete, is_pairing_pin_active, remove_local_paired_peer, PairCompleteRequest,
};
use super::{
    HOP_HEADER, LAN_PROTOCOL_VERSION, NONCE_HEADER, PEER_ID_HEADER, SIGNATURE_HEADER,
    TIMESTAMP_HEADER,
};

const MAX_SELECT_PROBE_INSTRUCTIONS_BYTES: usize = 16 * 1024;

#[derive(Clone)]
struct LanServerState {
    gateway: Arc<GatewayState>,
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

impl PeerRpcResponse {
    fn ok(result: Value, peer_review: Option<PeerReviewTokenDto>) -> Self {
        Self {
            result: Some(result),
            rpc_error: None,
            peer_review,
        }
    }

    fn err(rpc_error: Value) -> Self {
        Self {
            result: None,
            rpc_error: Some(rpc_error),
            peer_review: None,
        }
    }
}

#[derive(Deserialize)]
struct PeerReviewQuery {
    t: String,
}

static REPLAY_GUARD: OnceLock<ReplayGuard> = OnceLock::new();

fn replay_guard() -> &'static ReplayGuard {
    REPLAY_GUARD.get_or_init(ReplayGuard::default)
}

pub(super) fn prune_replay_cache() {
    if let Some(guard) = REPLAY_GUARD.get() {
        guard.prune_expired(unix_time_ms());
    }
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
) -> Result<AppSettings, Box<Response>> {
    if has_proxy_forwarded_headers(headers) {
        return Err(Box::new(json_err(
            StatusCode::FORBIDDEN,
            "PUBLIC_TUNNEL_FORBIDDEN",
            "局域网配对与联邦接口禁止通过公网隧道或反向代理访问",
        )));
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
            return Err(Box::new(json_err(
                StatusCode::FORBIDDEN,
                "NON_LAN_ORIGIN_FORBIDDEN",
                "仅允许局域网私网、EasyTier/Tailscale 等组网地址或已配置的自定义子网访问此接口",
            )));
        }
    }
    if !settings.lan_federation.enabled {
        return Err(Box::new(json_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "LAN_FEDERATION_DISABLED",
            "目标设备尚未开启局域网配对互联功能",
        )));
    }
    Ok(settings)
}

fn authenticate_peer_request(
    settings: &AppSettings,
    headers: &HeaderMap,
    method: &str,
    path_and_query: &str,
    body: &[u8],
) -> Result<PairedPeerConfig, Box<Response>> {
    let hop = headers
        .get(HOP_HEADER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u8>().ok())
        .unwrap_or(0);
    if hop != 1 {
        return Err(Box::new(json_err(
            StatusCode::BAD_REQUEST,
            "FEDERATION_LOOP_DENIED",
            "局域网联邦仅允许单跳直连调用（hop=1），拒绝多跳转发",
        )));
    }

    let caller_id = headers
        .get(PEER_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| valid_device_id(s))
        .ok_or_else(|| {
            Box::new(json_err(
                StatusCode::UNAUTHORIZED,
                "MISSING_PEER_ID",
                "缺少有效的对等设备标识",
            ))
        })?;
    let timestamp_ms = headers
        .get(TIMESTAMP_HEADER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .ok_or_else(|| {
            Box::new(json_err(
                StatusCode::UNAUTHORIZED,
                "MISSING_TIMESTAMP",
                "缺少请求时间戳",
            ))
        })?;
    let nonce = headers
        .get(NONCE_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .ok_or_else(|| {
            Box::new(json_err(
                StatusCode::UNAUTHORIZED,
                "MISSING_NONCE",
                "缺少防重放 Nonce",
            ))
        })?;
    let signature = headers
        .get(SIGNATURE_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .ok_or_else(|| {
            Box::new(json_err(
                StatusCode::UNAUTHORIZED,
                "MISSING_SIGNATURE",
                "缺少请求签名",
            ))
        })?;

    let peer = settings
        .lan_federation
        .paired_peers
        .iter()
        .find(|p| p.device_id == caller_id)
        .ok_or_else(|| {
            Box::new(json_err(
                StatusCode::UNAUTHORIZED,
                "UNKNOWN_PEER",
                format!("设备 {caller_id} 未与当前主机配对"),
            ))
        })?;
    if !peer.enabled {
        return Err(Box::new(json_err(
            StatusCode::FORBIDDEN,
            "PEER_DISABLED",
            "当前主机已停用来自该配对设备的访问",
        )));
    }
    let secret = get_peer_secret(caller_id, Some(settings))
        .or_else(|| get_peer_secret(caller_id, None))
        .ok_or_else(|| {
            Box::new(json_err(
                StatusCode::UNAUTHORIZED,
                "PEER_SECRET_MISSING",
                "本地缺少该配对设备的共享密钥，请重新配对",
            ))
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
        return Err(Box::new(json_err(
            StatusCode::UNAUTHORIZED,
            reason,
            format!("对等请求签名校验未通过: {reason}"),
        )));
    }

    if !replay_guard().check_and_record(caller_id, nonce, now_ms) {
        return Err(Box::new(json_err(
            StatusCode::UNAUTHORIZED,
            "REPLAY_DETECTED",
            "检测到重复的 Nonce 请求，已拒绝重放",
        )));
    }

    Ok(peer.clone())
}

async fn lan_hello(
    State(state): State<LanServerState>,
    PeerConnectAddr(remote): PeerConnectAddr,
    headers: HeaderMap,
) -> Response {
    let settings = match verify_lan_surface(&state, &headers, remote) {
        Ok(s) => s,
        Err(resp) => return *resp,
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
    PeerConnectAddr(remote): PeerConnectAddr,
    headers: HeaderMap,
    Json(req): Json<PairCompleteRequest>,
) -> Response {
    if let Err(resp) = verify_lan_surface(&state, &headers, remote) {
        return *resp;
    }
    let override_snap = state.gateway.settings_snapshot();
    let remote_ip = remote.map(|s| s.ip());
    let result = tokio::task::spawn_blocking(move || {
        accept_pair_complete(&req, remote_ip, override_snap.as_ref())
    })
    .await;
    match result {
        Ok(Ok((ack, _peer_record, _shared_secret))) => {
            #[cfg(test)]
            if state.gateway.is_test_override() {
                state
                    .gateway
                    .register_test_paired_peer(_peer_record, _shared_secret);
            }
            ([(CACHE_CONTROL, "no-store")], Json(ack)).into_response()
        }
        Ok(Err((status, code, message))) => json_err(
            StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST),
            code,
            message,
        ),
        Err(err) => json_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "PAIR_WORKER_FAILED",
            format!("处理配对请求失败: {err}"),
        ),
    }
}

async fn lan_catalog(
    State(state): State<LanServerState>,
    PeerConnectAddr(remote): PeerConnectAddr,
    headers: HeaderMap,
) -> Response {
    let settings = match verify_lan_surface(&state, &headers, remote) {
        Ok(s) => s,
        Err(resp) => return *resp,
    };
    if let Err(resp) =
        authenticate_peer_request(&settings, &headers, "GET", "/api/lan/v1/catalog", &[])
    {
        return *resp;
    }

    let override_snap = state.gateway.settings_snapshot();
    let (lan, fingerprint) = identity_from_settings(override_snap.as_ref());
    let local_descriptors = state
        .gateway
        .registry
        .local_descriptors()
        .unwrap_or_default();
    let revision = WorkspaceRegistry::revision_for(&local_descriptors);
    let workspaces = local_descriptors
        .into_iter()
        .filter(|ws| is_workspace_shared(&lan, &ws.id))
        .map(|ws| PeerWorkspaceWireDto {
            id: ws.id,
            name: ws.name,
            path: ws.path,
        })
        .collect();

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
        workspaces,
    };
    ([(CACHE_CONTROL, "no-store")], Json(payload)).into_response()
}

async fn lan_rpc(
    State(state): State<LanServerState>,
    PeerConnectAddr(remote): PeerConnectAddr,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let settings = match verify_lan_surface(&state, &headers, remote) {
        Ok(s) => s,
        Err(resp) => return *resp,
    };
    let caller_peer =
        match authenticate_peer_request(&settings, &headers, "POST", "/api/lan/v1/rpc", &body) {
            Ok(peer) => peer,
            Err(resp) => return *resp,
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
        } => execute_select_probe(gateway, lan, caller_peer, &workspace_id, &caller_session_id),
        PeerRpcRequest::WorkspaceTool {
            workspace_id,
            tool,
            arguments,
            caller_session_id,
            request_id,
        } => execute_workspace_tool_rpc(
            gateway,
            lan,
            caller_peer,
            source_label,
            workspace_id,
            &tool,
            arguments,
            caller_session_id.as_deref(),
            &request_id,
            request_bytes,
        ),
        PeerRpcRequest::ComputerTool {
            tool,
            arguments,
            caller_session_id,
            request_id,
        } => execute_computer_tool_rpc(
            gateway,
            lan,
            caller_peer,
            source_label,
            &tool,
            arguments,
            &caller_session_id,
            &request_id,
            request_bytes,
        ),
    }
}

fn workspace_not_shared_error(workspace_id: &str) -> PeerRpcResponse {
    PeerRpcResponse::err(
        GatewayError::not_found(
            "WORKSPACE_NOT_SHARED",
            format!("目标设备未向局域网共享该工作区: {workspace_id}"),
        )
        .to_rpc_error(),
    )
}

fn execute_select_probe(
    gateway: &GatewayState,
    lan: &crate::settings::LanFederationConfig,
    caller_peer: &PairedPeerConfig,
    workspace_id: &str,
    caller_session_id: &str,
) -> PeerRpcResponse {
    if !is_workspace_shared(lan, workspace_id) {
        return workspace_not_shared_error(workspace_id);
    }
    let scoped_session = format!("lan:{}:{caller_session_id}", caller_peer.device_id);
    match gateway.workspace_by_id(&scoped_session, workspace_id) {
        Ok(ctx) => {
            let instructions = truncate_utf8_with_ellipsis(
                &ctx.tools.current_ai_instructions(),
                MAX_SELECT_PROBE_INSTRUCTIONS_BYTES,
            );
            let skills = ctx
                .tools
                .current_skills()
                .into_iter()
                .take(50)
                .map(|entry| entry.descriptor)
                .collect::<Vec<_>>();
            PeerRpcResponse::ok(
                json!({
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
                    "skills": skills,
                }),
                None,
            )
        }
        Err(err) => PeerRpcResponse::err(err.to_rpc_error()),
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_workspace_tool_rpc(
    gateway: &GatewayState,
    lan: &crate::settings::LanFederationConfig,
    caller_peer: &PairedPeerConfig,
    source_label: String,
    workspace_id: String,
    tool: &str,
    mut arguments: Value,
    caller_session_id: Option<&str>,
    request_id: &Value,
    request_bytes: usize,
) -> PeerRpcResponse {
    if !is_workspace_shared(lan, &workspace_id) {
        return workspace_not_shared_error(&workspace_id);
    }
    let canonical_name = crate::tools::registry::canonical_tool_name(tool);
    let logical_session = format!(
        "lan:{}:{}",
        caller_peer.device_id,
        caller_session_id.unwrap_or("stateless")
    );
    let audit_body = json!({
        "params": { "name": canonical_name, "arguments": &arguments }
    });
    let audit_call = ToolAuditCall::new_with_peer_context(
        request_id,
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
            return PeerRpcResponse::err(rpc_err);
        }
    };

    let gateway_surface = crate::tools::registry::exposed_tool_names("compact");
    let workspace_surface =
        crate::tools::registry::exposed_tool_names(ctx.tools.tool_profile.as_str());
    if !gateway_surface.contains(&canonical_name) || !workspace_surface.contains(&canonical_name) {
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
        return PeerRpcResponse::err(rpc_err);
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
        return PeerRpcResponse::ok(denied, None);
    }

    if let Some(obj) = arguments.as_object_mut() {
        obj.remove("workspace_id");
        obj.remove("device_id");
    }
    inject_workspace_session_args(canonical_name, &mut arguments, Some(&logical_session));

    let structured = call_tool(ctx.tools.as_ref(), canonical_name, &arguments);
    let mut peer_review = None;
    if let Some(change_id) = structured.get("change_id").and_then(Value::as_str) {
        let _ =
            crate::review::attach_session(ctx.tools.workspace.root(), change_id, &logical_session);
        if let Ok(link) = crate::review::issue_token(ctx.tools.workspace.root(), change_id) {
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
    PeerRpcResponse::ok(result, peer_review)
}

#[allow(clippy::too_many_arguments)]
fn execute_computer_tool_rpc(
    gateway: &GatewayState,
    lan: &crate::settings::LanFederationConfig,
    caller_peer: &PairedPeerConfig,
    source_label: String,
    tool: &str,
    mut arguments: Value,
    caller_session_id: &str,
    request_id: &Value,
    request_bytes: usize,
) -> PeerRpcResponse {
    let owner = format!("lan_peer:{}:{caller_session_id}", caller_peer.device_id);
    let session = gateway.sessions.computer_auto_session(&owner);
    if let Some(obj) = arguments.as_object_mut() {
        obj.remove("device_id");
        obj.remove("session_token");
    }
    let audit_body = json!({
        "params": { "name": tool, "arguments": &arguments }
    });
    let audit_call = ToolAuditCall::new_with_peer_context(
        request_id,
        tool,
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
        return PeerRpcResponse::ok(denied, None);
    }

    let result = crate::computer::call_tool(tool, Some(&session), &arguments);
    record_tool_call(audit_call, None, &json!({ "result": &result }));
    PeerRpcResponse::ok(result, None)
}

fn is_readonly_peer_tool(canonical_name: &str, args: &Value) -> bool {
    match canonical_name {
        "server_info" | "read_file" | "list_dir" | "list_files" | "search_text" | "grep_text"
        | "patch_check" | "git_status" | "git_diff" | "git_log" | "git_show" | "git_blame"
        | "view_image" | "change_review" => true,
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
    PeerConnectAddr(remote): PeerConnectAddr,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<PeerReviewQuery>,
) -> Response {
    let settings = match verify_lan_surface(&state, &headers, remote) {
        Ok(s) => s,
        Err(resp) => return *resp,
    };
    let path_and_query = format!("/api/lan/v1/review/{id}?t={}", query.t);
    if let Err(resp) = authenticate_peer_request(&settings, &headers, "GET", &path_and_query, &[]) {
        return *resp;
    }

    let gateway = state.gateway.clone();
    let token = query.t;
    let found = tokio::task::spawn_blocking(move || {
        let lan = &settings.lan_federation;
        let roots = gateway
            .registry
            .list()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .into_iter()
            .filter(|p| is_workspace_shared(lan, &p.id))
            .map(|p| std::path::PathBuf::from(p.path));
        crate::review::find_review(roots, &id, &token).map_err(|msg| review_error_status(&msg))
    })
    .await;

    match found {
        Ok(Ok(review)) => ([(CACHE_CONTROL, "no-store")], Json(review)).into_response(),
        Ok(Err(status)) => status.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn lan_unpair(
    State(state): State<LanServerState>,
    PeerConnectAddr(remote): PeerConnectAddr,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let settings = match verify_lan_surface(&state, &headers, remote) {
        Ok(s) => s,
        Err(resp) => return *resp,
    };
    let caller =
        match authenticate_peer_request(&settings, &headers, "POST", "/api/lan/v1/unpair", &body) {
            Ok(peer) => peer,
            Err(resp) => return *resp,
        };
    #[cfg(test)]
    let skip_disk_unpair = state.gateway.is_test_override();
    #[cfg(not(test))]
    let skip_disk_unpair = false;

    if !skip_disk_unpair {
        let device_id = caller.device_id;
        let _ = tokio::task::spawn_blocking(move || remove_local_paired_peer(&device_id)).await;
    }
    ([(CACHE_CONTROL, "no-store")], Json(json!({ "ok": true }))).into_response()
}
