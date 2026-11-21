use std::collections::HashMap;
use std::future::Future;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::task::JoinSet;

use crate::mcp::gateway::{GatewayError, WorkspaceDescriptor};
use crate::review::ChangeSet;
use crate::settings::{AppSettings, PairedPeerConfig};

use super::crypto::{sign_peer_request, unix_time_ms};
use super::identity::{
    format_remote_workspace_scope, get_peer_secret, identity_from_settings, local_device_summary,
    PeerDeviceMetaDto,
};
use super::server::{PeerCatalogResponse, PeerReviewTokenDto, PeerRpcRequest, PeerRpcResponse};
use super::{HOP_HEADER, NONCE_HEADER, PEER_ID_HEADER, SIGNATURE_HEADER, TIMESTAMP_HEADER};

const CATALOG_CACHE_TTL_MS: u64 = 8_000;
const CONNECT_TIMEOUT_MS: u64 = 1_500;
const CATALOG_TIMEOUT_MS: u64 = 2_500;
const RPC_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedPeerStatusDto {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
    pub os: String,
    pub arch: String,
    pub version: String,
    pub host: String,
    pub port: u16,
    pub addresses: Vec<String>,
    pub enabled: bool,
    pub paired_at_ms: u64,
    pub online: bool,
    pub latency_ms: Option<u64>,
    pub trust_level: String,
    pub workspace_count: usize,
    pub computer_use_supported: bool,
    pub computer_use_enabled: bool,
    pub last_error: Option<String>,
}

impl PairedPeerStatusDto {
    fn offline(peer: PairedPeerConfig, reason: impl Into<String>) -> Self {
        Self {
            device_id: peer.device_id,
            device_name: peer.device_name,
            fingerprint: peer.fingerprint,
            os: peer.os,
            arch: peer.arch,
            version: peer.version,
            host: peer.host,
            port: peer.port,
            addresses: peer.addresses,
            enabled: peer.enabled,
            paired_at_ms: peer.paired_at_ms,
            online: false,
            latency_ms: None,
            trust_level: "unknown".into(),
            workspace_count: 0,
            computer_use_supported: false,
            computer_use_enabled: false,
            last_error: Some(reason.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FederatedDeviceDto {
    pub device_id: String,
    pub device_name: String,
    pub is_local: bool,
    pub os: String,
    pub arch: String,
    pub version: String,
    pub online: bool,
    pub latency_ms: Option<u64>,
    pub trust_level: String,
    pub workspace_count: usize,
    pub computer_use_supported: bool,
    pub computer_use_enabled: bool,
}

#[derive(Debug, Clone)]
struct CachedPeerCatalog {
    fetched_at_ms: u64,
    online: bool,
    latency_ms: Option<u64>,
    catalog: Option<PeerCatalogResponse>,
    last_error: Option<String>,
}

static PEER_CACHE: OnceLock<Mutex<HashMap<String, CachedPeerCatalog>>> = OnceLock::new();
static PREFERRED_HOST: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
static LAN_HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// Shared no-proxy HTTP client for all LAN peer traffic, avoiding per-request
/// connection-pool and TLS root-store allocations.
pub(super) fn shared_lan_http_client() -> &'static reqwest::Client {
    LAN_HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .no_proxy()
            .connect_timeout(Duration::from_millis(CONNECT_TIMEOUT_MS))
            .pool_idle_timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(4)
            .build()
            .expect("shared LAN HTTP client")
    })
}

pub(super) fn prune_peer_cache() {
    let now = unix_time_ms();
    let stale_ttl_ms = CATALOG_CACHE_TTL_MS * 10;
    let live_ids: Option<std::collections::HashSet<String>> = PEER_CACHE.get().map(|cache| {
        let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
        guard.retain(|_, entry| now.saturating_sub(entry.fetched_at_ms) <= stale_ttl_ms);
        if guard.capacity() > 32 && guard.len() * 4 < guard.capacity() {
            guard.shrink_to_fit();
        }
        guard.keys().cloned().collect()
    });
    if let Some(hosts) = PREFERRED_HOST.get() {
        let mut guard = hosts.lock().unwrap_or_else(|e| e.into_inner());
        match live_ids {
            Some(ref ids) => guard.retain(|device_id, _| ids.contains(device_id)),
            None => guard.clear(),
        }
        if guard.capacity() > 32 && guard.len() * 4 < guard.capacity() {
            guard.shrink_to_fit();
        }
    }
}

fn peer_cache() -> &'static Mutex<HashMap<String, CachedPeerCatalog>> {
    PEER_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn preferred_hosts() -> &'static Mutex<HashMap<String, String>> {
    PREFERRED_HOST.get_or_init(|| Mutex::new(HashMap::new()))
}

fn preferred_peer_host(device_id: &str) -> Option<String> {
    preferred_hosts()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(device_id)
        .cloned()
}

fn remember_preferred_peer_host(device_id: &str, host: &str) {
    preferred_hosts()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(device_id.to_string(), host.to_string());
}

pub fn invalidate_peer_cache(device_id: &str) {
    let mut guard = peer_cache().lock().unwrap_or_else(|e| e.into_inner());
    let mut hosts = preferred_hosts().lock().unwrap_or_else(|e| e.into_inner());
    if device_id.is_empty() {
        guard.clear();
        hosts.clear();
    } else {
        guard.remove(device_id);
        hosts.remove(device_id);
    }
}

/// Execute an async LAN network call safely from synchronous contexts (including
/// `tokio::task::spawn_blocking` workers, multi-thread runtime workers, single-thread
/// `#[tokio::test]` runtimes, and plain `#[test]` OS threads).
pub(crate) fn run_lan_future<T, F>(future: F) -> Result<T, String>
where
    T: Send + 'static,
    F: Future<Output = Result<T, String>> + Send + 'static,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread {
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            handle.spawn(async move {
                let _ = tx.send(future.await);
            });
            return rx
                .recv()
                .map_err(|_| "LAN 异步任务通道意外关闭".to_string())?;
        }
    }
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("初始化 LAN 运行时失败: {e}"))?;
        rt.block_on(future)
    })
    .join()
    .map_err(|_| "LAN 异步工作线程异常终止".to_string())?
}

fn load_effective_settings(settings_override: Option<&AppSettings>) -> AppSettings {
    settings_override
        .cloned()
        .unwrap_or_else(AppSettings::load_or_default)
}

fn find_enabled_peer(
    settings: &AppSettings,
    device_id: &str,
) -> Result<(PairedPeerConfig, String), GatewayError> {
    if !settings.lan_federation.enabled {
        return Err(GatewayError::invalid(
            "LAN_FEDERATION_DISABLED",
            "当前电脑尚未开启局域网配对互联功能。",
        ));
    }
    let peer = settings
        .lan_federation
        .paired_peers
        .iter()
        .find(|p| p.device_id == device_id)
        .ok_or_else(|| {
            GatewayError::not_found(
                "PEER_DEVICE_NOT_FOUND",
                format!("未找到已配对设备: {device_id}。请先调用 workspace_list 或 device_list 查看可用设备。"),
            )
        })?;
    if !peer.enabled {
        return Err(GatewayError::invalid(
            "PEER_DEVICE_DISABLED",
            format!(
                "配对设备 {} ({device_id}) 当前已在设置中停用。",
                peer.device_name
            ),
        ));
    }
    let secret = get_peer_secret(device_id, Some(settings))
        .or_else(|| get_peer_secret(device_id, None))
        .ok_or_else(|| {
            GatewayError::invalid(
                "PEER_SECRET_MISSING",
                format!("配对设备 {} 缺少共享密钥，请重新配对。", peer.device_name),
            )
        })?;
    Ok((peer.clone(), secret))
}

async fn signed_http_request(
    caller_device_id: &str,
    peer: &PairedPeerConfig,
    peer_secret: &str,
    method: reqwest::Method,
    path_and_query: &str,
    body: Vec<u8>,
    timeout: Duration,
) -> Result<(u16, Vec<u8>, u64), String> {
    let client = shared_lan_http_client();

    let mut candidate_hosts = Vec::new();
    if let Some(preferred) = preferred_peer_host(&peer.device_id) {
        candidate_hosts.push(preferred);
    }
    if !peer.host.trim().is_empty() && !candidate_hosts.contains(&peer.host) {
        candidate_hosts.push(peer.host.clone());
    }
    for addr in &peer.addresses {
        let trimmed = addr.trim();
        if !trimmed.is_empty() && !candidate_hosts.iter().any(|h| h == trimmed) {
            candidate_hosts.push(trimmed.to_string());
        }
    }

    let mut last_err = String::new();
    for host in candidate_hosts {
        let timestamp_ms = unix_time_ms();
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let signature = sign_peer_request(
            peer_secret,
            caller_device_id,
            method.as_str(),
            path_and_query,
            timestamp_ms,
            &nonce,
            &body,
        );
        let url = format!("http://{host}:{}{path_and_query}", peer.port);
        let started = Instant::now();
        let mut req = client
            .request(method.clone(), &url)
            .timeout(timeout)
            .header(PEER_ID_HEADER, caller_device_id)
            .header(TIMESTAMP_HEADER, timestamp_ms.to_string())
            .header(NONCE_HEADER, &nonce)
            .header(HOP_HEADER, "1")
            .header(SIGNATURE_HEADER, &signature);
        if !body.is_empty() {
            req = req
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.clone());
        }
        match req.send().await {
            Ok(resp) => {
                remember_preferred_peer_host(&peer.device_id, &host);
                let rtt = started.elapsed().as_millis() as u64;
                let status = resp.status().as_u16();
                let bytes = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
                return Ok((status, bytes, rtt));
            }
            Err(err) => {
                last_err = err.to_string();
            }
        }
    }
    Err(if last_err.is_empty() {
        format!(
            "无法连接配对设备 {} ({}:{})",
            peer.device_name, peer.host, peer.port
        )
    } else {
        last_err
    })
}

async fn fetch_or_cached_catalog(
    caller_device_id: &str,
    peer: &PairedPeerConfig,
    peer_secret: &str,
    force_refresh: bool,
) -> CachedPeerCatalog {
    let now = unix_time_ms();
    if !force_refresh {
        let cached = peer_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&peer.device_id)
            .cloned();
        if let Some(entry) = cached {
            if now.saturating_sub(entry.fetched_at_ms) <= CATALOG_CACHE_TTL_MS {
                return entry;
            }
        }
    }

    let entry = match signed_http_request(
        caller_device_id,
        peer,
        peer_secret,
        reqwest::Method::GET,
        "/api/lan/v1/catalog",
        Vec::new(),
        Duration::from_millis(CATALOG_TIMEOUT_MS),
    )
    .await
    {
        Ok((200, bytes, rtt)) => match serde_json::from_slice::<PeerCatalogResponse>(&bytes) {
            Ok(catalog) => CachedPeerCatalog {
                fetched_at_ms: unix_time_ms(),
                online: true,
                latency_ms: Some(rtt),
                catalog: Some(catalog),
                last_error: None,
            },
            Err(err) => CachedPeerCatalog {
                fetched_at_ms: unix_time_ms(),
                online: false,
                latency_ms: Some(rtt),
                catalog: None,
                last_error: Some(format!("解析目录响应失败: {err}")),
            },
        },
        Ok((status, bytes, rtt)) => {
            let detail = serde_json::from_slice::<Value>(&bytes)
                .ok()
                .and_then(|v| v.get("message").and_then(Value::as_str).map(str::to_string))
                .unwrap_or_else(|| format!("HTTP {status}"));
            CachedPeerCatalog {
                fetched_at_ms: unix_time_ms(),
                online: false,
                latency_ms: Some(rtt),
                catalog: None,
                last_error: Some(detail),
            }
        }
        Err(err) => {
            let prev_catalog = peer_cache()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&peer.device_id)
                .and_then(|c| c.catalog.clone());
            CachedPeerCatalog {
                fetched_at_ms: unix_time_ms(),
                online: false,
                latency_ms: None,
                catalog: prev_catalog,
                last_error: Some(err),
            }
        }
    };

    peer_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(peer.device_id.clone(), entry.clone());
    entry
}

pub async fn list_paired_peer_statuses_async(
    settings_override: Option<&AppSettings>,
    force_refresh: bool,
) -> Vec<PairedPeerStatusDto> {
    let settings = load_effective_settings(settings_override);
    let (local_cfg, _) = identity_from_settings(Some(&settings));
    let peers = settings.lan_federation.paired_peers.clone();
    if peers.is_empty() {
        return Vec::new();
    }

    let mut join_set = JoinSet::new();
    let total_peers = peers.len();
    for (index, peer) in peers.into_iter().enumerate() {
        let caller_id = local_cfg.device_id.clone();
        let secret = get_peer_secret(&peer.device_id, Some(&settings))
            .or_else(|| get_peer_secret(&peer.device_id, None));
        let lan_enabled = settings.lan_federation.enabled;
        join_set.spawn(async move {
            let dto = if !lan_enabled {
                PairedPeerStatusDto::offline(peer, "局域网互联未启用")
            } else if !peer.enabled {
                PairedPeerStatusDto::offline(peer, "已手动停用")
            } else if let Some(secret) = secret {
                let snap = fetch_or_cached_catalog(&caller_id, &peer, &secret, force_refresh).await;
                let cat = snap.catalog.as_ref();
                PairedPeerStatusDto {
                    device_id: peer.device_id,
                    device_name: cat
                        .map(|c| c.device_name.clone())
                        .unwrap_or(peer.device_name),
                    fingerprint: peer.fingerprint,
                    os: cat.map(|c| c.os.clone()).unwrap_or(peer.os),
                    arch: cat.map(|c| c.arch.clone()).unwrap_or(peer.arch),
                    version: cat.map(|c| c.version.clone()).unwrap_or(peer.version),
                    host: peer.host,
                    port: peer.port,
                    addresses: peer.addresses,
                    enabled: peer.enabled,
                    paired_at_ms: peer.paired_at_ms,
                    online: snap.online,
                    latency_ms: snap.latency_ms,
                    trust_level: cat
                        .map(|c| c.trust_level.clone())
                        .unwrap_or_else(|| "full".into()),
                    workspace_count: cat.map(|c| c.workspaces.len()).unwrap_or(0),
                    computer_use_supported: cat.map(|c| c.computer_use_supported).unwrap_or(false),
                    computer_use_enabled: snap.online
                        && cat.map(|c| c.computer_use_enabled).unwrap_or(false),
                    last_error: snap.last_error,
                }
            } else {
                PairedPeerStatusDto::offline(peer, "缺少配对密钥，请重新配对")
            };
            (index, dto)
        });
    }

    let mut indexed = Vec::with_capacity(total_peers);
    while let Some(res) = join_set.join_next().await {
        if let Ok(item) = res {
            indexed.push(item);
        }
    }
    indexed.sort_by_key(|(index, _)| *index);
    indexed.into_iter().map(|(_, dto)| dto).collect()
}

/// Collect local + federated remote WorkspaceDescriptors for `workspace_list`.
pub fn federated_workspace_descriptors(
    local_items: Vec<WorkspaceDescriptor>,
    settings_override: Option<&AppSettings>,
) -> (Vec<WorkspaceDescriptor>, Vec<FederatedDeviceDto>) {
    let settings = load_effective_settings(settings_override);
    let local_summary = local_device_summary(Some(&settings), local_items.len());
    let local_meta = PeerDeviceMetaDto {
        device_id: local_summary.device_id.clone(),
        device_name: local_summary.device_name.clone(),
        is_local: true,
        os: local_summary.os.clone(),
        arch: local_summary.arch.clone(),
        online: true,
        access_mode: "full".into(),
    };

    let mut all_workspaces: Vec<WorkspaceDescriptor> = local_items
        .into_iter()
        .map(|mut ws| {
            ws.qualified_name = Some(format!(
                "[本机 · {}] {}",
                local_summary.device_name, ws.name
            ));
            ws.device = Some(local_meta.clone());
            ws
        })
        .collect();

    let mut devices = vec![FederatedDeviceDto {
        device_id: local_summary.device_id.clone(),
        device_name: local_summary.device_name.clone(),
        is_local: true,
        os: local_summary.os,
        arch: local_summary.arch,
        version: local_summary.version,
        online: true,
        latency_ms: Some(0),
        trust_level: "local".into(),
        workspace_count: local_summary.workspace_count,
        computer_use_supported: local_summary.computer_use_supported,
        computer_use_enabled: local_summary.computer_use_enabled,
    }];

    if !settings.lan_federation.enabled
        || !settings
            .lan_federation
            .paired_peers
            .iter()
            .any(|p| p.enabled)
    {
        return (all_workspaces, devices);
    }

    let settings_cloned = settings.clone();
    let caller_id = local_summary.device_id;
    let peer_entries = run_lan_future(async move {
        let mut join_set = JoinSet::new();
        for (index, peer) in settings_cloned
            .lan_federation
            .paired_peers
            .iter()
            .filter(|p| p.enabled)
            .enumerate()
        {
            let Some(secret) = get_peer_secret(&peer.device_id, Some(&settings_cloned))
                .or_else(|| get_peer_secret(&peer.device_id, None))
            else {
                continue;
            };
            let peer_owned = peer.clone();
            let caller = caller_id.clone();
            join_set.spawn(async move {
                let snap = fetch_or_cached_catalog(&caller, &peer_owned, &secret, false).await;
                (index, peer_owned, snap)
            });
        }
        let mut collected = Vec::new();
        while let Some(res) = join_set.join_next().await {
            if let Ok(item) = res {
                collected.push(item);
            }
        }
        collected.sort_by_key(|(index, _, _)| *index);
        Ok(collected
            .into_iter()
            .map(|(_, peer, snap)| (peer, snap))
            .collect::<Vec<_>>())
    })
    .unwrap_or_default();

    for (peer, snap) in peer_entries {
        let cat = snap.catalog.as_ref();
        let peer_name = cat
            .map(|c| c.device_name.clone())
            .unwrap_or_else(|| peer.device_name.clone());
        let peer_os = cat.map(|c| c.os.clone()).unwrap_or_else(|| peer.os.clone());
        let peer_arch = cat
            .map(|c| c.arch.clone())
            .unwrap_or_else(|| peer.arch.clone());
        let peer_ver = cat
            .map(|c| c.version.clone())
            .unwrap_or_else(|| peer.version.clone());
        let trust = cat
            .map(|c| c.trust_level.clone())
            .unwrap_or_else(|| "full".into());
        let ws_count = cat.map(|c| c.workspaces.len()).unwrap_or(0);

        devices.push(FederatedDeviceDto {
            device_id: peer.device_id.clone(),
            device_name: peer_name.clone(),
            is_local: false,
            os: peer_os.clone(),
            arch: peer_arch.clone(),
            version: peer_ver,
            online: snap.online,
            latency_ms: snap.latency_ms,
            trust_level: trust.clone(),
            workspace_count: ws_count,
            computer_use_supported: cat.map(|c| c.computer_use_supported).unwrap_or(false),
            computer_use_enabled: snap.online
                && cat.map(|c| c.computer_use_enabled).unwrap_or(false),
        });

        if let Some(catalog) = snap.catalog {
            let meta = PeerDeviceMetaDto {
                device_id: peer.device_id.clone(),
                device_name: peer_name.clone(),
                is_local: false,
                os: peer_os.clone(),
                arch: peer_arch,
                online: snap.online,
                access_mode: trust,
            };
            let status_tag = if snap.online { "" } else { " · 离线" };
            for remote_ws in catalog.workspaces {
                all_workspaces.push(WorkspaceDescriptor {
                    id: format_remote_workspace_scope(&peer.device_id, &remote_ws.id),
                    name: remote_ws.name.clone(),
                    path: remote_ws.path,
                    qualified_name: Some(format!(
                        "[远端 · {peer_name} ({peer_os}){status_tag}] {}",
                        remote_ws.name
                    )),
                    device: Some(meta.clone()),
                });
            }
        }
    }

    (all_workspaces, devices)
}

pub fn federated_device_list(
    local_workspace_count: usize,
    settings_override: Option<&AppSettings>,
) -> Vec<FederatedDeviceDto> {
    let (_, devices) = federated_workspace_descriptors(Vec::new(), settings_override);
    devices
        .into_iter()
        .map(|mut d| {
            if d.is_local {
                d.workspace_count = local_workspace_count;
            }
            d
        })
        .collect()
}

/// Query only the target paired peer's catalog to resolve a single remote workspace
/// descriptor (used by `workspace_current` to avoid probing all unrelated peers).
pub fn lookup_remote_workspace_descriptor(
    settings_override: Option<&AppSettings>,
    device_id: &str,
    workspace_id: &str,
) -> Option<WorkspaceDescriptor> {
    let settings = load_effective_settings(settings_override);
    let (peer, secret) = find_enabled_peer(&settings, device_id).ok()?;
    let (local_cfg, _) = identity_from_settings(Some(&settings));
    let caller_id = local_cfg.device_id;
    let peer_for_req = peer.clone();
    let snap = run_lan_future(async move {
        Ok(fetch_or_cached_catalog(&caller_id, &peer_for_req, &secret, false).await)
    })
    .ok()?;

    let catalog = snap.catalog?;
    let remote_ws = catalog
        .workspaces
        .into_iter()
        .find(|ws| ws.id == workspace_id)?;
    let status_tag = if snap.online { "" } else { " · 离线" };
    Some(WorkspaceDescriptor {
        id: format_remote_workspace_scope(&peer.device_id, &remote_ws.id),
        name: remote_ws.name.clone(),
        path: remote_ws.path,
        qualified_name: Some(format!(
            "[远端 · {} ({}){status_tag}] {}",
            catalog.device_name, catalog.os, remote_ws.name
        )),
        device: Some(PeerDeviceMetaDto {
            device_id: peer.device_id,
            device_name: catalog.device_name,
            is_local: false,
            os: catalog.os,
            arch: catalog.arch,
            online: snap.online,
            access_mode: catalog.trust_level,
        }),
    })
}

fn dispatch_peer_rpc_sync(
    settings_override: Option<&AppSettings>,
    target_device_id: &str,
    rpc_req: PeerRpcRequest,
) -> Result<(PairedPeerConfig, PeerRpcResponse), Value> {
    let settings = load_effective_settings(settings_override);
    let (peer, secret) =
        find_enabled_peer(&settings, target_device_id).map_err(|e| e.to_rpc_error())?;
    let (local_cfg, _) = identity_from_settings(Some(&settings));
    let caller_id = local_cfg.device_id;
    let body = serde_json::to_vec(&rpc_req).map_err(|e| {
        GatewayError::internal("SERIALIZE_RPC_FAILED", e.to_string()).to_rpc_error()
    })?;

    let peer_for_req = peer.clone();
    let (status, resp_bytes) = run_lan_future(async move {
        let (status, bytes, rtt) = signed_http_request(
            &caller_id,
            &peer_for_req,
            &secret,
            reqwest::Method::POST,
            "/api/lan/v1/rpc",
            body,
            Duration::from_secs(RPC_TIMEOUT_SECS),
        )
        .await?;
        if let Some(cached) = peer_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&peer_for_req.device_id)
        {
            cached.online = status < 500;
            cached.latency_ms = Some(rtt);
        }
        Ok((status, bytes))
    })
    .map_err(|err| {
        if let Some(cached) = peer_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&peer.device_id)
        {
            cached.online = false;
            cached.last_error = Some(err.clone());
        }
        GatewayError::internal(
            "PEER_DEVICE_OFFLINE",
            format!(
                "目标配对设备 {} ({}) 当前离线或局域网连接超时：{}",
                peer.device_name, peer.device_id, err
            ),
        )
        .to_rpc_error()
    })?;

    if status != 200 {
        let detail = serde_json::from_slice::<Value>(&resp_bytes)
            .ok()
            .and_then(|v| v.get("message").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_else(|| format!("HTTP {status}"));
        return Err(GatewayError::invalid(
            "PEER_RPC_REJECTED",
            format!("配对设备 {} 拒绝调用：{detail}", peer.device_name),
        )
        .to_rpc_error());
    }

    let decoded: PeerRpcResponse = serde_json::from_slice(&resp_bytes).map_err(|e| {
        GatewayError::internal(
            "PEER_INVALID_RESPONSE",
            format!("解析配对设备 {} 响应失败: {e}", peer.device_name),
        )
        .to_rpc_error()
    })?;
    if let Some(rpc_err) = decoded.rpc_error {
        return Err(rpc_err);
    }
    Ok((peer, decoded))
}

pub fn forward_workspace_select(
    settings_override: Option<&AppSettings>,
    session_id: &str,
    device_id: &str,
    remote_workspace_id: &str,
) -> Result<(WorkspaceDescriptor, Value), Value> {
    let (peer, resp) = dispatch_peer_rpc_sync(
        settings_override,
        device_id,
        PeerRpcRequest::SelectProbe {
            workspace_id: remote_workspace_id.to_string(),
            caller_session_id: session_id.to_string(),
        },
    )?;
    let payload = resp.result.unwrap_or_else(|| json!({}));
    let raw_name = payload["workspace"]["name"]
        .as_str()
        .unwrap_or(remote_workspace_id)
        .to_string();
    let raw_path = payload["workspace"]["path"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let access_mode = payload["trust_level"]
        .as_str()
        .unwrap_or("full")
        .to_string();
    let qualified_id = format_remote_workspace_scope(&peer.device_id, remote_workspace_id);
    let descriptor = WorkspaceDescriptor {
        id: qualified_id,
        name: raw_name.clone(),
        path: raw_path,
        qualified_name: Some(format!(
            "[远端 · {} ({})] {raw_name}",
            peer.device_name, peer.os
        )),
        device: Some(PeerDeviceMetaDto {
            device_id: peer.device_id,
            device_name: peer.device_name,
            is_local: false,
            os: peer.os,
            arch: peer.arch,
            online: true,
            access_mode,
        }),
    };
    Ok((descriptor, payload))
}

pub fn forward_workspace_tool(
    settings_override: Option<&AppSettings>,
    device_id: &str,
    remote_workspace_id: &str,
    tool_name: &str,
    arguments: &Value,
    caller_session_id: Option<&str>,
    request_id: &Value,
) -> Result<(PairedPeerConfig, Value), Value> {
    let settings = load_effective_settings(settings_override);
    let (peer, resp) = dispatch_peer_rpc_sync(
        Some(&settings),
        device_id,
        PeerRpcRequest::WorkspaceTool {
            workspace_id: remote_workspace_id.to_string(),
            tool: tool_name.to_string(),
            arguments: arguments.clone(),
            caller_session_id: caller_session_id.map(str::to_string),
            request_id: request_id.clone(),
        },
    )?;
    let mut result = resp.result.unwrap_or_else(|| json!({}));
    rewrite_peer_result_metadata(
        &settings,
        &peer,
        remote_workspace_id,
        resp.peer_review.as_ref(),
        &mut result,
    );
    Ok((peer, result))
}

pub fn forward_computer_tool(
    settings_override: Option<&AppSettings>,
    device_id: &str,
    tool_name: &str,
    arguments: &Value,
    caller_session_id: &str,
    request_id: &Value,
) -> Result<(PairedPeerConfig, Value), Value> {
    let (peer, resp) = dispatch_peer_rpc_sync(
        settings_override,
        device_id,
        PeerRpcRequest::ComputerTool {
            tool: tool_name.to_string(),
            arguments: arguments.clone(),
            caller_session_id: caller_session_id.to_string(),
            request_id: request_id.clone(),
        },
    )?;
    let mut result = resp.result.unwrap_or_else(|| json!({}));
    if let Some(structured) = result
        .get_mut("structuredContent")
        .and_then(Value::as_object_mut)
    {
        structured.insert(
            "gateway_device".into(),
            json!({
                "deviceId": peer.device_id,
                "deviceName": peer.device_name,
                "isLocal": false,
                "os": peer.os,
            }),
        );
    }
    Ok((peer, result))
}

fn rewrite_peer_result_metadata(
    entry_settings: &AppSettings,
    peer: &PairedPeerConfig,
    remote_workspace_id: &str,
    peer_review: Option<&PeerReviewTokenDto>,
    result: &mut Value,
) {
    let Some(structured) = result
        .get_mut("structuredContent")
        .and_then(Value::as_object_mut)
    else {
        return;
    };

    structured.insert(
        "gateway_device".into(),
        json!({
            "deviceId": peer.device_id,
            "deviceName": peer.device_name,
            "isLocal": false,
            "os": peer.os,
            "qualifiedWorkspaceId": format_remote_workspace_scope(&peer.device_id, remote_workspace_id),
        }),
    );

    if let Some(link) = peer_review {
        let public_base = entry_settings
            .global_gateway
            .public_url
            .trim()
            .trim_end_matches('/');
        let entry_base = if !public_base.is_empty() {
            public_base.to_string()
        } else {
            format!(
                "http://127.0.0.1:{}",
                entry_settings.global_gateway.local_port
            )
        };
        let proxied_url = format!(
            "{entry_base}/review/peer/{}/{}?t={}",
            peer.device_id, link.change_id, link.token
        );
        structured.insert("review_url".into(), Value::String(proxied_url));
    }

    // Re-sync MCP text content representation when structuredContent was enriched,
    // without touching non-text items (such as base64 images).
    let serialized = Value::Object(structured.clone()).to_string();
    if let Some(first_text) = result
        .get_mut("content")
        .and_then(Value::as_array_mut)
        .and_then(|arr| {
            arr.iter_mut()
                .find(|item| item.get("type") == Some(&json!("text")))
        })
        .and_then(Value::as_object_mut)
    {
        first_text.insert("text".into(), Value::String(serialized));
    }
}

/// Fetch a ChangeSet from a paired peer so the entry node's `/review/peer/{device_id}/{id}`
/// endpoint can render the read-only diff review page over a single tunnel.
pub async fn fetch_peer_review(
    settings_override: Option<&AppSettings>,
    device_id: &str,
    review_id: &str,
    token: &str,
) -> Result<ChangeSet, u16> {
    if !review_id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || token
            .bytes()
            .any(|b| !b.is_ascii_alphanumeric() && b != b'-' && b != b'_')
    {
        return Err(400);
    }
    let settings = load_effective_settings(settings_override);
    let (peer, secret) = find_enabled_peer(&settings, device_id).map_err(|_| 404u16)?;
    let (local_cfg, _) = identity_from_settings(Some(&settings));
    let path_and_query = format!("/api/lan/v1/review/{review_id}?t={token}");
    let (status, bytes, _) = signed_http_request(
        &local_cfg.device_id,
        &peer,
        &secret,
        reqwest::Method::GET,
        &path_and_query,
        Vec::new(),
        Duration::from_secs(5),
    )
    .await
    .map_err(|_| 502u16)?;
    if status != 200 {
        return Err(status);
    }
    serde_json::from_slice::<ChangeSet>(&bytes).map_err(|_| 502u16)
}

/// Best-effort notification to peer when local user unpairs it, so both machines clean up.
pub async fn notify_peer_unpair(peer: &PairedPeerConfig, secret: &str) {
    let settings = AppSettings::load_or_default();
    let (local_cfg, _) = identity_from_settings(Some(&settings));
    let body = serde_json::to_vec(&json!({ "deviceId": local_cfg.device_id })).unwrap_or_default();
    let _ = signed_http_request(
        &local_cfg.device_id,
        peer,
        secret,
        reqwest::Method::POST,
        "/api/lan/v1/unpair",
        body,
        Duration::from_millis(1500),
    )
    .await;
}
