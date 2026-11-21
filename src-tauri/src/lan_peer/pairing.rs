use std::net::IpAddr;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::data::DataStore;
use crate::local_network;
use crate::settings::{AppSettings, PairedPeerConfig};

use super::crypto::{
    compute_pair_ack_proof, compute_pair_request_proof, constant_time_eq,
    derive_peer_shared_secret, random_hex_32, random_pin_6, unix_time_ms,
};
use super::discovery::probe_peer_hello;
use super::identity::{
    ensure_local_identity, identity_from_settings, remove_peer_secret, set_peer_secret,
    valid_device_id,
};
use super::LAN_PROTOCOL_VERSION;

const PIN_TTL_MS: u64 = 120_000;
const MAX_PIN_ATTEMPTS: u8 = 5;
const PAIR_CLOCK_SKEW_MS: u64 = 120_000;

#[derive(Debug, Clone)]
struct ActivePinSession {
    pin: String,
    created_at_ms: u64,
    expires_at_ms: u64,
    failed_attempts: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingPinDto {
    pub pin: String,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
    pub remaining_seconds: u64,
    pub remaining_attempts: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairPeerSummary {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
    pub os: String,
    pub arch: String,
    pub version: String,
    pub mcp_port: u16,
    pub addresses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairCompleteRequest {
    pub protocol: String,
    pub target_device_id: String,
    pub initiator: PairPeerSummary,
    pub timestamp_ms: u64,
    pub nonce: String,
    pub secret_part_a: String,
    pub pin_proof: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairCompleteResponse {
    pub ok: bool,
    pub responder: PairPeerSummary,
    pub secret_part_b: String,
    pub response_proof: String,
}

static ACTIVE_PIN: OnceLock<Mutex<Option<ActivePinSession>>> = OnceLock::new();

fn pin_slot() -> &'static Mutex<Option<ActivePinSession>> {
    ACTIVE_PIN.get_or_init(|| Mutex::new(None))
}

fn to_pin_dto(session: &ActivePinSession, now_ms: u64) -> PairingPinDto {
    PairingPinDto {
        pin: session.pin.clone(),
        created_at_ms: session.created_at_ms,
        expires_at_ms: session.expires_at_ms,
        remaining_seconds: session.expires_at_ms.saturating_sub(now_ms) / 1000,
        remaining_attempts: MAX_PIN_ATTEMPTS.saturating_sub(session.failed_attempts),
    }
}

pub fn generate_pairing_pin() -> PairingPinDto {
    let now = unix_time_ms();
    let session = ActivePinSession {
        pin: random_pin_6(),
        created_at_ms: now,
        expires_at_ms: now + PIN_TTL_MS,
        failed_attempts: 0,
    };
    let dto = to_pin_dto(&session, now);
    *pin_slot().lock().unwrap_or_else(|e| e.into_inner()) = Some(session);
    dto
}

pub fn cancel_pairing_pin() {
    *pin_slot().lock().unwrap_or_else(|e| e.into_inner()) = None;
}

pub fn current_pairing_pin() -> Option<PairingPinDto> {
    let now = unix_time_ms();
    let mut guard = pin_slot().lock().unwrap_or_else(|e| e.into_inner());
    if guard
        .as_ref()
        .is_some_and(|s| now >= s.expires_at_ms || s.failed_attempts >= MAX_PIN_ATTEMPTS)
    {
        *guard = None;
    }
    guard.as_ref().map(|s| to_pin_dto(s, now))
}

pub fn is_pairing_pin_active() -> bool {
    current_pairing_pin().is_some()
}

/// Server-side handler for `POST /api/lan/v1/pair/complete`.
/// Verifies the initiator's PIN HMAC proof in constant time, derives a shared
/// 256-bit secret, persists the peer locally (when not running in test-override mode),
/// and returns a mutual acknowledgement proof so both machines trust each other.
pub(super) fn accept_pair_complete(
    req: &PairCompleteRequest,
    caller_ip: Option<IpAddr>,
    settings_override: Option<&AppSettings>,
) -> Result<(PairCompleteResponse, PairedPeerConfig, String), (u16, &'static str, String)> {
    if req.protocol != LAN_PROTOCOL_VERSION {
        return Err((
            400,
            "PROTOCOL_MISMATCH",
            format!("配对协议版本不兼容：{}", req.protocol),
        ));
    }
    if !valid_device_id(&req.initiator.device_id)
        || req.initiator.device_name.trim().is_empty()
        || req.secret_part_a.len() < 32
        || req.nonce.len() < 8
    {
        return Err((
            400,
            "INVALID_PAIR_REQUEST",
            "配对请求参数无效或不完整".into(),
        ));
    }

    let (local_cfg, local_fp) = identity_from_settings(settings_override);
    let local_port = settings_override
        .map(|s| s.global_gateway.local_port)
        .unwrap_or_else(|| AppSettings::load_or_default().global_gateway.local_port);
    if req.target_device_id != local_cfg.device_id {
        return Err((
            400,
            "TARGET_DEVICE_MISMATCH",
            "目标设备标识与当前主机不匹配".into(),
        ));
    }
    if req.initiator.device_id == local_cfg.device_id {
        return Err((400, "SELF_PAIR_DENIED", "不能与当前本机自身进行配对".into()));
    }

    let now = unix_time_ms();
    if now.abs_diff(req.timestamp_ms) > PAIR_CLOCK_SKEW_MS {
        return Err((
            400,
            "CLOCK_SKEW_TOO_LARGE",
            "两台电脑系统时间相差超过 2 分钟，请校准系统时间后重试".into(),
        ));
    }

    let pin = {
        let mut guard = pin_slot().lock().unwrap_or_else(|e| e.into_inner());
        let Some(active) = guard.as_mut() else {
            return Err((
                403,
                "PAIRING_PIN_INACTIVE",
                "目标电脑当前未开启配对码或配对码已过期，请在目标电脑点击“生成配对码”".into(),
            ));
        };
        if now >= active.expires_at_ms || active.failed_attempts >= MAX_PIN_ATTEMPTS {
            *guard = None;
            return Err((
                403,
                "PAIRING_PIN_EXPIRED",
                "配对码已过期或错误次数过多，请在目标电脑重新生成配对码".into(),
            ));
        }

        let expected_proof = compute_pair_request_proof(
            &active.pin,
            &req.initiator.device_id,
            &local_cfg.device_id,
            req.timestamp_ms,
            &req.nonce,
            &req.secret_part_a,
        );
        if !constant_time_eq(
            expected_proof.as_bytes(),
            req.pin_proof.trim().to_ascii_lowercase().as_bytes(),
        ) {
            active.failed_attempts = active.failed_attempts.saturating_add(1);
            let left = MAX_PIN_ATTEMPTS.saturating_sub(active.failed_attempts);
            if left == 0 {
                *guard = None;
                return Err((
                    403,
                    "PAIRING_PIN_LOCKED",
                    "配对码连续输入错误已作废，请在目标电脑重新生成".into(),
                ));
            }
            return Err((
                403,
                "INVALID_PAIRING_PIN",
                format!("配对码不正确（剩余可重试 {left} 次）"),
            ));
        }
        let verified_pin = active.pin.clone();
        // Single-use: consume the PIN immediately upon verified proof.
        *guard = None;
        verified_pin
    };

    let secret_part_b = random_hex_32();
    let shared_secret = derive_peer_shared_secret(
        &req.initiator.device_id,
        &local_cfg.device_id,
        &req.secret_part_a,
        &secret_part_b,
    );
    let response_proof = compute_pair_ack_proof(
        &pin,
        &local_cfg.device_id,
        &req.initiator.device_id,
        &req.nonce,
        &secret_part_b,
    );

    let caller_host = caller_ip
        .map(|ip| match ip {
            IpAddr::V4(v4) => v4.to_string(),
            IpAddr::V6(v6) => v6
                .to_ipv4_mapped()
                .map(|v4| v4.to_string())
                .unwrap_or_else(|| v6.to_string()),
        })
        .or_else(|| req.initiator.addresses.first().cloned())
        .unwrap_or_else(|| "127.0.0.1".to_string());

    let addresses = merge_peer_addresses(&caller_host, &req.initiator.addresses, 8);
    let peer_record = build_paired_peer_record(&req.initiator, caller_host, addresses, now);

    if settings_override.is_none() {
        persist_paired_peer(&peer_record, &shared_secret)
            .map_err(|e| (500, "PAIR_PERSIST_FAILED", e))?;
    }

    let extra_subnets = settings_override
        .map(|s| s.lan_federation.extra_subnets.clone())
        .unwrap_or_else(|| AppSettings::load_or_default().lan_federation.extra_subnets);
    let responder = build_local_peer_summary(&local_cfg, local_fp, local_port, &extra_subnets);

    Ok((
        PairCompleteResponse {
            ok: true,
            responder,
            secret_part_b,
            response_proof,
        },
        peer_record,
        shared_secret,
    ))
}

/// Client-side initiator: pairs this machine with `host:port` using the 6-digit PIN
/// displayed on that peer. Upon success, both machines hold the mutual `PairedPeerConfig`.
pub async fn pair_with_peer(host: &str, port: u16, pin: &str) -> Result<PairedPeerConfig, String> {
    let clean_pin: String = pin.chars().filter(char::is_ascii_digit).collect();
    if clean_pin.len() != 6 {
        return Err("请输入完整的 6 位数字配对码".into());
    }

    let target = probe_peer_hello(host, port).await?;
    let (local_cfg, local_fp) = ensure_local_identity()?;
    let settings = AppSettings::load_or_default();

    let timestamp_ms = unix_time_ms();
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let secret_part_a = random_hex_32();
    let pin_proof = compute_pair_request_proof(
        &clean_pin,
        &local_cfg.device_id,
        &target.device_id,
        timestamp_ms,
        &nonce,
        &secret_part_a,
    );

    let initiator = build_local_peer_summary(
        &local_cfg,
        local_fp,
        settings.global_gateway.local_port,
        &settings.lan_federation.extra_subnets,
    );
    let payload = PairCompleteRequest {
        protocol: LAN_PROTOCOL_VERSION.to_string(),
        target_device_id: target.device_id.clone(),
        initiator,
        timestamp_ms,
        nonce: nonce.clone(),
        secret_part_a: secret_part_a.clone(),
        pin_proof,
    };

    let url = format!(
        "http://{}:{}/api/lan/v1/pair/complete",
        target.host, target.port
    );
    let response = super::client::shared_lan_http_client()
        .post(&url)
        .timeout(Duration::from_secs(5))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("发送配对握手失败：{e}"))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body: serde_json::Value = response.json().await.unwrap_or_default();
        let msg = body
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("配对验证未通过");
        return Err(format!("{msg} (HTTP {status})"));
    }

    let ack: PairCompleteResponse = response
        .json()
        .await
        .map_err(|e| format!("解析配对确认响应失败：{e}"))?;
    if ack.responder.device_id != target.device_id {
        return Err("配对响应中的设备 ID 与探测目标不一致，已中止配对".into());
    }
    let expected_ack = compute_pair_ack_proof(
        &clean_pin,
        &ack.responder.device_id,
        &local_cfg.device_id,
        &nonce,
        &ack.secret_part_b,
    );
    if !constant_time_eq(
        expected_ack.as_bytes(),
        ack.response_proof.trim().to_ascii_lowercase().as_bytes(),
    ) {
        return Err("对方返回的双向配对签名校验失败，已拒绝保存".into());
    }

    let shared_secret = derive_peer_shared_secret(
        &local_cfg.device_id,
        &ack.responder.device_id,
        &secret_part_a,
        &ack.secret_part_b,
    );

    let addresses = merge_peer_addresses(&target.host, &ack.responder.addresses, 8);
    let peer_record =
        build_paired_peer_record(&ack.responder, target.host, addresses, unix_time_ms());

    persist_paired_peer(&peer_record, &shared_secret)?;
    super::client::invalidate_peer_cache(&peer_record.device_id);
    Ok(peer_record)
}

fn merge_peer_addresses(primary: &str, extra: &[String], max: usize) -> Vec<String> {
    let mut addresses = vec![primary.to_string()];
    for addr in extra {
        let trimmed = addr.trim();
        if !trimmed.is_empty() && !addresses.iter().any(|existing| existing == trimmed) {
            addresses.push(trimmed.to_string());
        }
    }
    addresses.truncate(max);
    addresses
}

fn build_local_peer_summary(
    local_cfg: &crate::settings::LanFederationConfig,
    fingerprint: String,
    mcp_port: u16,
    extra_subnets: &[String],
) -> PairPeerSummary {
    let addresses = local_network::local_lan_ipv4_addrs_with_subnets(extra_subnets)
        .into_iter()
        .map(|ip| ip.to_string())
        .collect();
    PairPeerSummary {
        device_id: local_cfg.device_id.clone(),
        device_name: local_cfg.device_name.clone(),
        fingerprint,
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        mcp_port,
        addresses,
    }
}

fn build_paired_peer_record(
    summary: &PairPeerSummary,
    host: String,
    addresses: Vec<String>,
    paired_at_ms: u64,
) -> PairedPeerConfig {
    PairedPeerConfig {
        device_id: summary.device_id.clone(),
        device_name: sanitize_device_name(&summary.device_name),
        fingerprint: summary.fingerprint.clone(),
        os: sanitize_short(&summary.os, 24),
        arch: sanitize_short(&summary.arch, 24),
        version: sanitize_short(&summary.version, 32),
        host,
        port: summary.mcp_port,
        addresses,
        enabled: true,
        paired_at_ms,
    }
}

pub(super) fn persist_paired_peer(
    peer: &PairedPeerConfig,
    shared_secret: &str,
) -> Result<(), String> {
    set_peer_secret(&peer.device_id, shared_secret)?;
    DataStore::update_file(|data| {
        data.lan_federation.enabled = true;
        if let Some(existing) = data
            .lan_federation
            .paired_peers
            .iter_mut()
            .find(|p| p.device_id == peer.device_id)
        {
            *existing = peer.clone();
        } else {
            data.lan_federation.paired_peers.push(peer.clone());
        }
        Ok(())
    })
    .map_err(|e| e.to_string())
}

pub fn remove_local_paired_peer(device_id: &str) -> Result<Option<PairedPeerConfig>, String> {
    let removed = DataStore::update_file(|data| {
        let pos = data
            .lan_federation
            .paired_peers
            .iter()
            .position(|p| p.device_id == device_id);
        Ok(pos.map(|idx| data.lan_federation.paired_peers.remove(idx)))
    })
    .map_err(|e| e.to_string())?;
    let _ = remove_peer_secret(device_id);
    super::client::invalidate_peer_cache(device_id);
    Ok(removed)
}

pub fn update_paired_peer_enabled(device_id: &str, enabled: bool) -> Result<(), String> {
    DataStore::update_file(|data| {
        let peer = data
            .lan_federation
            .paired_peers
            .iter_mut()
            .find(|p| p.device_id == device_id)
            .ok_or_else(|| crate::error::AppError::Message("未找到该配对设备".into()))?;
        peer.enabled = enabled;
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    super::client::invalidate_peer_cache(device_id);
    Ok(())
}

pub fn rename_paired_peer(device_id: &str, alias: &str) -> Result<(), String> {
    let cleaned = sanitize_device_name(alias);
    if cleaned.is_empty() {
        return Err("设备备注名称不能为空".into());
    }
    DataStore::update_file(|data| {
        let peer = data
            .lan_federation
            .paired_peers
            .iter_mut()
            .find(|p| p.device_id == device_id)
            .ok_or_else(|| crate::error::AppError::Message("未找到该配对设备".into()))?;
        peer.device_name = cleaned.clone();
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    super::client::invalidate_peer_cache(device_id);
    Ok(())
}

fn sanitize_device_name(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(64)
        .collect();
    if cleaned.is_empty() {
        "未命名局域网设备".into()
    } else {
        cleaned
    }
}

fn sanitize_short(raw: &str, max: usize) -> String {
    raw.trim()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(max)
        .collect()
}
