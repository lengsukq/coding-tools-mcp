use serde::{Deserialize, Serialize};

use crate::data::DataStore;
use crate::local_network;
use crate::mcp::gateway::GatewayError;
use crate::secret::SecretStore;
use crate::settings::{AppSettings, LanFederationConfig};

use super::crypto::{derive_fingerprint, random_hex_32};

pub const LAN_SECRET_SCOPE: &str = "lan_federation";
pub const LAN_NODE_SECRET_KEY: &str = "node_secret";
pub const LAN_PEER_SECRETS_SCOPE: &str = "lan_peers";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalDeviceDto {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
    pub os: String,
    pub arch: String,
    pub version: String,
    pub mcp_port: u16,
    pub auth_type: String,
    pub local_agent_endpoint: String,
    pub addresses: Vec<String>,
    pub allow_lan_access: bool,
    pub allow_local_noauth: bool,
    pub lan_enabled: bool,
    pub share_mode: String,
    pub shared_workspace_ids: Vec<String>,
    pub trust_level: String,
    pub allow_computer_use: bool,
    pub extra_subnets: Vec<String>,
    pub computer_use_supported: bool,
    pub computer_use_enabled: bool,
    pub workspace_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerDeviceMetaDto {
    pub device_id: String,
    pub device_name: String,
    pub is_local: bool,
    pub os: String,
    pub arch: String,
    pub online: bool,
    pub access_mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceTarget {
    Local(String),
    Remote {
        device_id: String,
        workspace_id: String,
    },
}

pub fn valid_device_id(id: &str) -> bool {
    let trimmed = id.trim();
    (4..=64).contains(&trimmed.len())
        && trimmed
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn valid_raw_workspace_id(id: &str) -> bool {
    let trimmed = id.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 128
        && !trimmed.contains(':')
        && !trimmed.contains('/')
        && !trimmed.contains('\\')
        && !trimmed.contains("..")
        && !trimmed.chars().any(char::is_control)
}

pub fn format_remote_workspace_scope(device_id: &str, workspace_id: &str) -> String {
    format!("{}:{}", device_id.trim(), workspace_id.trim())
}

/// Parse a `workspace_id` supplied by an MCP client into either a Local workspace
/// or a Remote `{device_id}:{workspace_id}` target.
///
/// Bare IDs (`"ws_123"`), `"local:ws_123"`, and `"{self_device_id}:ws_123"` all
/// resolve to `WorkspaceTarget::Local("ws_123")` for 100% backward compatibility.
pub fn parse_workspace_scope(
    raw_scope: &str,
    self_device_id: &str,
) -> Result<WorkspaceTarget, GatewayError> {
    let trimmed = raw_scope.trim();
    if trimmed.is_empty() {
        return Err(GatewayError::invalid(
            "INVALID_WORKSPACE_SCOPE",
            "workspace_id 不能为空。",
        ));
    }
    if let Some((prefix, rest)) = trimmed.split_once(':') {
        let prefix = prefix.trim();
        let rest = rest.trim();
        if !valid_raw_workspace_id(rest) {
            return Err(GatewayError::invalid(
                "INVALID_WORKSPACE_SCOPE",
                format!("无效的工作区标识格式: {trimmed}"),
            ));
        }
        if prefix == "local" || prefix == "self" || (!self_device_id.is_empty() && prefix == self_device_id) {
            return Ok(WorkspaceTarget::Local(rest.to_string()));
        }
        if !valid_device_id(prefix) {
            return Err(GatewayError::invalid(
                "INVALID_WORKSPACE_SCOPE",
                format!("无效的目标设备标识: {prefix}"),
            ));
        }
        return Ok(WorkspaceTarget::Remote {
            device_id: prefix.to_string(),
            workspace_id: rest.to_string(),
        });
    }

    if !valid_raw_workspace_id(trimmed) {
        return Err(GatewayError::invalid(
            "INVALID_WORKSPACE_SCOPE",
            format!("无效的工作区标识: {trimmed}"),
        ));
    }
    Ok(WorkspaceTarget::Local(trimmed.to_string()))
}

/// Ensure this installation has a persistent `device_id`, `device_name`, and
/// `node_secret` persisted on disk. Safe to call repeatedly.
pub fn ensure_local_identity() -> Result<(LanFederationConfig, String), String> {
    let mut node_secret = SecretStore::get_app(LAN_SECRET_SCOPE, LAN_NODE_SECRET_KEY)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    if node_secret.trim().len() < 32 {
        let generated = random_hex_32();
        SecretStore::set_app(LAN_SECRET_SCOPE, LAN_NODE_SECRET_KEY, &generated)
            .map_err(|e| e.to_string())?;
        node_secret = generated;
    }

    let config = DataStore::update_file(|data| {
        if !valid_device_id(&data.lan_federation.device_id) {
            let suffix = &random_hex_32()[..12];
            data.lan_federation.device_id = format!("dev_{suffix}");
        }
        if data.lan_federation.device_name.trim().is_empty() {
            data.lan_federation.device_name = local_network::default_device_name();
        }
        if !matches!(data.lan_federation.share_mode.as_str(), "all" | "selected") {
            data.lan_federation.share_mode = "all".into();
        }
        if !matches!(data.lan_federation.trust_level.as_str(), "full" | "readonly") {
            data.lan_federation.trust_level = "full".into();
        }
        Ok(data.lan_federation.clone())
    })
    .map_err(|e| e.to_string())?;

    let fingerprint = derive_fingerprint(&config.device_id, &node_secret);
    Ok((config, fingerprint))
}

/// Read-only identity helper that honors an optional `settings_override` (used by unit tests
/// so tests never mutate the real user configuration directory).
pub fn identity_from_settings(settings_override: Option<&AppSettings>) -> (LanFederationConfig, String) {
    if let Some(settings) = settings_override {
        let mut cfg = settings.lan_federation.clone();
        if !valid_device_id(&cfg.device_id) {
            cfg.device_id = "dev_localtest01".into();
        }
        if cfg.device_name.trim().is_empty() {
            cfg.device_name = local_network::default_device_name();
        }
        let secret = settings
            .app_secrets
            .get(LAN_SECRET_SCOPE)
            .and_then(|m| m.get(LAN_NODE_SECRET_KEY))
            .map(String::as_str)
            .unwrap_or("test-node-secret-0123456789abcdef");
        let fp = derive_fingerprint(&cfg.device_id, secret);
        return (cfg, fp);
    }
    ensure_local_identity().unwrap_or_else(|_| {
        let cfg = LanFederationConfig {
            device_id: "dev_fallback0001".into(),
            device_name: local_network::default_device_name(),
            ..LanFederationConfig::default()
        };
        let fp = derive_fingerprint(&cfg.device_id, "fallback-secret");
        (cfg, fp)
    })
}

pub fn local_device_summary(
    settings_override: Option<&AppSettings>,
    workspace_count: usize,
) -> LocalDeviceDto {
    let owned_settings;
    let settings = match settings_override {
        Some(s) => s,
        None => {
            owned_settings = AppSettings::load_or_default();
            &owned_settings
        }
    };
    let (lan, fingerprint) = identity_from_settings(settings_override);
    let addresses = local_network::local_lan_ipv4_addrs_with_subnets(&lan.extra_subnets)
        .into_iter()
        .map(|ip| ip.to_string())
        .collect();
    let local_port = settings.global_gateway.local_port;
    let auth_type = settings.global_mcp_auth_type.trim().to_string();
    let local_agent_endpoint = format!("http://127.0.0.1:{local_port}/mcp");
    LocalDeviceDto {
        device_id: lan.device_id,
        device_name: lan.device_name,
        fingerprint,
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        mcp_port: local_port,
        auth_type,
        local_agent_endpoint,
        addresses,
        allow_lan_access: settings.allow_lan_access,
        allow_local_noauth: settings.allow_local_noauth,
        lan_enabled: lan.enabled,
        share_mode: lan.share_mode,
        shared_workspace_ids: lan.shared_workspace_ids,
        trust_level: lan.trust_level,
        allow_computer_use: lan.allow_computer_use,
        extra_subnets: lan.extra_subnets,
        computer_use_supported: cfg!(target_os = "macos"),
        computer_use_enabled: cfg!(target_os = "macos") && crate::computer::local_policy_present(),
        workspace_count,
    }
}

pub fn is_workspace_shared(config: &LanFederationConfig, workspace_id: &str) -> bool {
    match config.share_mode.as_str() {
        "selected" => config
            .shared_workspace_ids
            .iter()
            .any(|id| id == workspace_id),
        _ => true,
    }
}

pub fn get_peer_secret(
    device_id: &str,
    settings_override: Option<&AppSettings>,
) -> Option<String> {
    if let Some(settings) = settings_override {
        return settings
            .app_secrets
            .get(LAN_PEER_SECRETS_SCOPE)
            .and_then(|m| m.get(device_id))
            .filter(|v| !v.is_empty())
            .cloned();
    }
    SecretStore::get_app(LAN_PEER_SECRETS_SCOPE, device_id)
        .ok()
        .flatten()
}

pub fn set_peer_secret(device_id: &str, secret: &str) -> Result<(), String> {
    SecretStore::set_app(LAN_PEER_SECRETS_SCOPE, device_id, secret).map_err(|e| e.to_string())
}

pub fn remove_peer_secret(device_id: &str) -> Result<(), String> {
    DataStore::update_file(|data| {
        if let Some(map) = data.app_secrets.get_mut(LAN_PEER_SECRETS_SCOPE) {
            map.remove(device_id);
        }
        Ok(())
    })
    .map_err(|e| e.to_string())
}
