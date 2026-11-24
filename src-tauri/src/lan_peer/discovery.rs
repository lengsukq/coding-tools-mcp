use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket as StdUdpSocket};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;

use crate::data::DataStore;
use crate::local_network;
use crate::settings::AppSettings;
use crate::util::time::unix_now_ms;

use super::identity::ensure_local_identity;
use super::LAN_PROTOCOL_VERSION;

pub const LAN_DISCOVERY_UDP_PORT: u16 = 28767;
const MULTICAST_GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 77, 67);
const BEACON_INTERVAL_SECS: u64 = 8;
const DISCOVERED_TTL_MS: u64 = 35_000;
const MAX_DISCOVERED_PEERS: usize = 256;
const OVERLAY_PROBE_CONCURRENCY: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeaconPacket {
    pub protocol: String,
    /// "announce" | "discover"
    pub kind: String,
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
    pub os: String,
    pub arch: String,
    pub version: String,
    pub mcp_port: u16,
    pub computer_use_supported: bool,
    pub sent_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredPeerDto {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
    pub os: String,
    pub arch: String,
    pub version: String,
    pub host: String,
    pub port: u16,
    pub addresses: Vec<String>,
    pub computer_use_supported: bool,
    pub already_paired: bool,
    pub last_seen_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerHelloDto {
    pub protocol: String,
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
    pub os: String,
    pub arch: String,
    pub version: String,
    pub mcp_port: u16,
    pub pairing_accepting: bool,
    pub computer_use_supported: bool,
    pub computer_use_enabled: bool,
}

struct DaemonHandle {
    stop_tx: Option<oneshot::Sender<()>>,
    nudge_tx: mpsc::UnboundedSender<()>,
}

static DISCOVERED: OnceLock<Mutex<HashMap<String, DiscoveredPeerDto>>> = OnceLock::new();
static DAEMON: OnceLock<Mutex<Option<DaemonHandle>>> = OnceLock::new();

fn discovered_map() -> &'static Mutex<HashMap<String, DiscoveredPeerDto>> {
    DISCOVERED.get_or_init(|| Mutex::new(HashMap::new()))
}

fn daemon_slot() -> &'static Mutex<Option<DaemonHandle>> {
    DAEMON.get_or_init(|| Mutex::new(None))
}

/// Start or stop the background cross-platform UDP discovery beacon daemon
/// according to `lan_federation.enabled`.
pub fn sync_discovery_daemon() {
    let settings = AppSettings::load_or_default();
    let should_run = settings.lan_federation.enabled;
    let mut guard = daemon_slot().lock().unwrap_or_else(|e| e.into_inner());
    if !should_run {
        if let Some(mut handle) = guard.take() {
            if let Some(stop) = handle.stop_tx.take() {
                let _ = stop.send(());
            }
        }
        discovered_map()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        return;
    }

    if let Some(existing) = guard.as_ref() {
        let _ = existing.nudge_tx.send(());
        return;
    }

    let (stop_tx, stop_rx) = oneshot::channel();
    let (nudge_tx, nudge_rx) = mpsc::unbounded_channel();
    crate::async_rt::spawn(run_discovery_loop(stop_rx, nudge_rx));
    *guard = Some(DaemonHandle {
        stop_tx: Some(stop_tx),
        nudge_tx,
    });
}

/// Trigger an immediate active LAN discovery broadcast and prune expired entries.
pub fn trigger_discovery_scan() {
    sync_discovery_daemon();
    if let Some(handle) = daemon_slot()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
    {
        let _ = handle.nudge_tx.send(());
    }
}

pub fn list_discovered_peers() -> Vec<DiscoveredPeerDto> {
    let now = unix_now_ms();
    let settings = AppSettings::load_or_default();
    let paired_ids: std::collections::HashSet<&str> = settings
        .lan_federation
        .paired_peers
        .iter()
        .map(|p| p.device_id.as_str())
        .collect();
    let mut map = discovered_map().lock().unwrap_or_else(|e| e.into_inner());
    map.retain(|_, peer| now.saturating_sub(peer.last_seen_ms) <= DISCOVERED_TTL_MS);
    let mut list: Vec<DiscoveredPeerDto> = map
        .values()
        .cloned()
        .map(|mut item| {
            item.already_paired = paired_ids.contains(item.device_id.as_str());
            item
        })
        .collect();
    list.sort_by(|a, b| {
        a.already_paired
            .cmp(&b.already_paired)
            .then_with(|| a.device_name.cmp(&b.device_name))
    });
    list
}

pub(super) fn prune_discovered_peers() {
    let now = unix_now_ms();
    if let Some(map) = DISCOVERED.get() {
        let mut guard = map.lock().unwrap_or_else(|e| e.into_inner());
        guard.retain(|_, peer| now.saturating_sub(peer.last_seen_ms) <= DISCOVERED_TTL_MS);
        if guard.capacity() > 32 && guard.len() * 4 < guard.capacity() {
            guard.shrink_to_fit();
        }
    }
}

/// Direct HTTP probe (`GET http://{host}:{port}/api/lan/v1/hello`) as a firewall-safe
/// fallback when Windows Defender Firewall or router AP isolation blocks UDP broadcast.
pub async fn probe_peer_hello(host: &str, port: u16) -> Result<DiscoveredPeerDto, String> {
    let trimmed_host = host
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/');
    let clean_host = match trimmed_host.rsplit_once(':') {
        Some((h, _)) if !h.contains(':') => h.trim(),
        _ => trimmed_host,
    };
    if clean_host.is_empty() || clean_host.chars().any(char::is_control) {
        return Err("请输入有效的局域网 IP 地址或主机名".into());
    }
    let settings_for_check = AppSettings::load_or_default();
    let paired_hosts: Vec<String> = settings_for_check
        .lan_federation
        .paired_peers
        .iter()
        .flat_map(|p| std::iter::once(p.host.clone()).chain(p.addresses.iter().cloned()))
        .collect();
    if let Ok(ip) = clean_host.parse::<IpAddr>() {
        if !local_network::is_allowed_peer_ip(
            ip,
            &settings_for_check.lan_federation.extra_subnets,
            &paired_hosts,
        ) {
            return Err(
                "仅支持局域网私网、EasyTier/Tailscale 等组网地址或已在「自定义组网子网」中添加的网段，不支持未知公网 IP"
                    .into(),
            );
        }
    }

    let url = format!("http://{clean_host}:{port}/api/lan/v1/hello");
    let resp = super::client::shared_lan_http_client()
        .get(&url)
        .timeout(Duration::from_millis(3000))
        .send()
        .await
        .map_err(|e| format!("无法连接到 {clean_host}:{port}（请确认对方已开启局域网配对且 MCP 服务正在运行）：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "目标节点 {clean_host}:{port} 拒绝局域网握手（HTTP {}）",
            resp.status().as_u16()
        ));
    }
    let hello: PeerHelloDto = resp
        .json()
        .await
        .map_err(|e| format!("解析节点握手响应失败：{e}"))?;
    if hello.protocol != LAN_PROTOCOL_VERSION {
        return Err(format!(
            "节点协议版本不兼容：期望 {LAN_PROTOCOL_VERSION}，实际 {}",
            hello.protocol
        ));
    }
    let (local_cfg, _) = ensure_local_identity()?;
    if hello.device_id == local_cfg.device_id {
        return Err("该地址指向当前本机，无需与本机自己配对".into());
    }

    let already_paired = settings_for_check
        .lan_federation
        .paired_peers
        .iter()
        .any(|p| p.device_id == hello.device_id);
    let now = unix_now_ms();
    let dto = DiscoveredPeerDto {
        device_id: hello.device_id.clone(),
        device_name: hello.device_name,
        fingerprint: hello.fingerprint,
        os: hello.os,
        arch: hello.arch,
        version: hello.version,
        host: clean_host.to_string(),
        port: hello.mcp_port,
        addresses: vec![clean_host.to_string()],
        computer_use_supported: hello.computer_use_supported,
        already_paired,
        last_seen_ms: now,
    };
    record_discovered_peer(dto.clone());
    Ok(dto)
}

pub(super) fn record_discovered_peer(peer: DiscoveredPeerDto) {
    let now = unix_now_ms();
    {
        let mut map = discovered_map().lock().unwrap_or_else(|e| e.into_inner());
        map.retain(|_, entry| now.saturating_sub(entry.last_seen_ms) <= DISCOVERED_TTL_MS);
        if map.len() >= MAX_DISCOVERED_PEERS && !map.contains_key(&peer.device_id) {
            if let Some(oldest_id) = map
                .iter()
                .min_by_key(|(_, entry)| entry.last_seen_ms)
                .map(|(id, _)| id.clone())
            {
                map.remove(&oldest_id);
            }
        }
        map.insert(peer.device_id.clone(), peer.clone());
    }

    // Fast-path cached read: skip disk write & DATA_FILE_LOCK unless a paired peer
    // with matching pinned fingerprint actually experienced host/port/name drift.
    let needs_persist = DataStore::read_file(|data| {
        Ok(data
            .lan_federation
            .paired_peers
            .iter()
            .find(|p| p.device_id == peer.device_id && p.fingerprint == peer.fingerprint)
            .is_some_and(|existing| {
                existing.host != peer.host
                    || existing.port != peer.port
                    || existing.device_name != peer.device_name
            }))
    })
    .unwrap_or(false);
    if !needs_persist {
        return;
    }

    // Auto-heal DHCP IP or port drift for an already paired peer ONLY if its
    // cryptographic fingerprint matches the pinned fingerprint recorded at pairing time.
    let _ = DataStore::update_file(|data| {
        if let Some(existing) = data
            .lan_federation
            .paired_peers
            .iter_mut()
            .find(|p| p.device_id == peer.device_id && p.fingerprint == peer.fingerprint)
        {
            if existing.host != peer.host
                || existing.port != peer.port
                || existing.device_name != peer.device_name
            {
                existing.host = peer.host.clone();
                existing.port = peer.port;
                existing.device_name = peer.device_name.clone();
                if !existing.addresses.contains(&peer.host) {
                    existing.addresses.insert(0, peer.host.clone());
                    existing.addresses.truncate(6);
                }
            }
        }
        Ok(())
    });
}

fn bind_discovery_socket() -> Option<UdpSocket> {
    let std_sock = StdUdpSocket::bind(("0.0.0.0", LAN_DISCOVERY_UDP_PORT))
        .or_else(|_| StdUdpSocket::bind(("0.0.0.0", 0)))
        .ok()?;
    let _ = std_sock.set_broadcast(true);
    let _ = std_sock.set_multicast_loop_v4(true);
    let _ = std_sock.set_nonblocking(true);

    let settings = AppSettings::load_or_default();
    let _ = std_sock.join_multicast_v4(&MULTICAST_GROUP, &Ipv4Addr::UNSPECIFIED);
    for iface in
        local_network::local_lan_ipv4_addrs_with_subnets(&settings.lan_federation.extra_subnets)
    {
        let _ = std_sock.join_multicast_v4(&MULTICAST_GROUP, &iface);
    }

    UdpSocket::from_std(std_sock).ok()
}

fn build_local_beacon_with_settings(settings: &AppSettings, kind: &str) -> Option<Vec<u8>> {
    if !settings.lan_federation.enabled {
        return None;
    }
    let (lan, fingerprint) = ensure_local_identity().ok()?;
    let packet = BeaconPacket {
        protocol: LAN_PROTOCOL_VERSION.to_string(),
        kind: kind.to_string(),
        device_id: lan.device_id,
        device_name: lan.device_name,
        fingerprint,
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        mcp_port: settings.global_gateway.local_port,
        computer_use_supported: cfg!(target_os = "macos"),
        sent_at_ms: unix_now_ms(),
    };
    serde_json::to_vec(&packet).ok()
}

async fn broadcast_beacon(socket: &UdpSocket, kind: &str) {
    let settings = AppSettings::load_or_default();
    let Some(bytes) = build_local_beacon_with_settings(&settings, kind) else {
        return;
    };
    let extra_subnets = settings.lan_federation.extra_subnets.clone();
    let local_port = settings.global_gateway.local_port;
    let Ok((local_ips, unicast_ips, seed_hosts)) = tokio::task::spawn_blocking(move || {
        let local_ips = local_network::local_lan_ipv4_addrs_with_subnets(&extra_subnets);
        let (unicast_ips, seed_hosts) =
            local_network::collect_overlay_discovery_targets(&extra_subnets, local_port);
        (local_ips, unicast_ips, seed_hosts)
    })
    .await
    else {
        return;
    };

    let mut targets = Vec::new();
    let mut seen_targets = HashSet::new();
    let mut push_target = |addr: SocketAddr| {
        if seen_targets.insert(addr) {
            targets.push(addr);
        }
    };
    push_target(SocketAddr::from((MULTICAST_GROUP, LAN_DISCOVERY_UDP_PORT)));
    push_target(SocketAddr::from((
        Ipv4Addr::BROADCAST,
        LAN_DISCOVERY_UDP_PORT,
    )));
    for ip in local_ips {
        let octets = ip.octets();
        let directed = Ipv4Addr::new(octets[0], octets[1], octets[2], 255);
        push_target(SocketAddr::from((directed, LAN_DISCOVERY_UDP_PORT)));
    }
    if kind == "discover" {
        for host_ip in unicast_ips {
            push_target(SocketAddr::from((host_ip, LAN_DISCOVERY_UDP_PORT)));
        }
    }
    for (seed_host, _) in seed_hosts {
        if let Ok(ip) = seed_host.parse::<Ipv4Addr>() {
            push_target(SocketAddr::from((ip, LAN_DISCOVERY_UDP_PORT)));
        }
    }
    for target in targets {
        let _ = socket.send_to(&bytes, target).await;
    }
}

/// Concurrently probe explicit seed hosts and configured cross-subnet / EasyTier CIDRs
/// via HTTP `/api/lan/v1/hello` so nodes running in EasyTier `no-tun` subnet proxy mode
/// or behind UDP-blocking firewalls are discovered during active scans.
pub async fn probe_configured_overlay_targets() {
    let settings = AppSettings::load_or_default();
    if !settings.lan_federation.enabled {
        return;
    }
    let extra_subnets = settings.lan_federation.extra_subnets.clone();
    let local_port = settings.global_gateway.local_port;
    let Ok((_, seeds)) = tokio::task::spawn_blocking(move || {
        local_network::collect_overlay_discovery_targets(&extra_subnets, local_port)
    })
    .await
    else {
        return;
    };
    if seeds.is_empty() {
        return;
    }
    let client = super::client::shared_lan_http_client();
    let Ok((local_cfg, _)) = ensure_local_identity() else {
        return;
    };
    let self_id: Arc<str> = Arc::from(local_cfg.device_id);
    let paired_ids: Arc<HashSet<String>> = Arc::new(
        settings
            .lan_federation
            .paired_peers
            .iter()
            .map(|p| p.device_id.clone())
            .collect(),
    );

    for chunk in seeds.chunks(OVERLAY_PROBE_CONCURRENCY) {
        let mut join_set = JoinSet::new();
        for (host, port) in chunk.iter().cloned() {
            let self_id = Arc::clone(&self_id);
            let paired_ids = Arc::clone(&paired_ids);
            join_set.spawn(async move {
                let url = format!("http://{host}:{port}/api/lan/v1/hello");
                let Ok(resp) = client
                    .get(&url)
                    .timeout(Duration::from_millis(1200))
                    .send()
                    .await
                else {
                    return;
                };
                if !resp.status().is_success() {
                    return;
                }
                let Ok(hello) = resp.json::<PeerHelloDto>().await else {
                    return;
                };
                if hello.protocol != LAN_PROTOCOL_VERSION || hello.device_id == self_id.as_ref() {
                    return;
                }
                let already_paired = paired_ids.contains(&hello.device_id);
                record_discovered_peer(DiscoveredPeerDto {
                    device_id: hello.device_id,
                    device_name: hello.device_name,
                    fingerprint: hello.fingerprint,
                    os: hello.os,
                    arch: hello.arch,
                    version: hello.version,
                    host: host.clone(),
                    port: hello.mcp_port,
                    addresses: vec![host],
                    computer_use_supported: hello.computer_use_supported,
                    already_paired,
                    last_seen_ms: unix_now_ms(),
                });
            });
        }
        while join_set.join_next().await.is_some() {}
    }
}

async fn run_discovery_loop(
    mut stop_rx: oneshot::Receiver<()>,
    mut nudge_rx: mpsc::UnboundedReceiver<()>,
) {
    let Some(socket) = bind_discovery_socket() else {
        // Clear the global daemon handle on bind failure so subsequent scans can retry cleanly.
        let mut guard = daemon_slot().lock().unwrap_or_else(|e| e.into_inner());
        *guard = None;
        return;
    };
    broadcast_beacon(&socket, "discover").await;

    let mut ticker = tokio::time::interval(Duration::from_secs(BEACON_INTERVAL_SECS));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut buf = [0u8; 2048];

    loop {
        tokio::select! {
            _ = &mut stop_rx => {
                break;
            }
            Some(()) = nudge_rx.recv() => {
                broadcast_beacon(&socket, "discover").await;
            }
            _ = ticker.tick() => {
                broadcast_beacon(&socket, "announce").await;
            }
            recv = socket.recv_from(&mut buf) => {
                let Ok((len, src_addr)) = recv else {
                    continue;
                };
                let packet = buf[..len].to_vec();
                let reply = tokio::task::spawn_blocking(move || {
                    handle_received_beacon(&packet, src_addr)
                })
                .await
                .ok()
                .flatten();
                if let Some(reply_bytes) = reply {
                    let _ = socket.send_to(&reply_bytes, src_addr).await;
                }
            }
        }
    }
}

fn handle_received_beacon(packet: &[u8], src_addr: std::net::SocketAddr) -> Option<Vec<u8>> {
    let settings_check = AppSettings::load_or_default();
    let paired_hosts: Vec<String> = settings_check
        .lan_federation
        .paired_peers
        .iter()
        .flat_map(|p| std::iter::once(p.host.clone()).chain(p.addresses.iter().cloned()))
        .collect();
    if !local_network::is_allowed_peer_ip(
        src_addr.ip(),
        &settings_check.lan_federation.extra_subnets,
        &paired_hosts,
    ) {
        return None;
    }
    let beacon = serde_json::from_slice::<BeaconPacket>(packet).ok()?;
    if beacon.protocol != LAN_PROTOCOL_VERSION {
        return None;
    }
    let (local_cfg, _) = ensure_local_identity().ok()?;
    if beacon.device_id == local_cfg.device_id {
        return None;
    }
    let host = match src_addr.ip() {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(|v4| v4.to_string())
            .unwrap_or_else(|| v6.to_string()),
    };
    let already_paired = settings_check
        .lan_federation
        .paired_peers
        .iter()
        .any(|p| p.device_id == beacon.device_id);
    record_discovered_peer(DiscoveredPeerDto {
        device_id: beacon.device_id,
        device_name: beacon.device_name,
        fingerprint: beacon.fingerprint,
        os: beacon.os,
        arch: beacon.arch,
        version: beacon.version,
        host: host.clone(),
        port: beacon.mcp_port,
        addresses: vec![host],
        computer_use_supported: beacon.computer_use_supported,
        already_paired,
        last_seen_ms: unix_now_ms(),
    });

    if beacon.kind == "discover" {
        return build_local_beacon_with_settings(&settings_check, "announce");
    }
    None
}
