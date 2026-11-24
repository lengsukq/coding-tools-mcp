use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
#[cfg(windows)]
use std::sync::{Mutex, OnceLock};

use axum::http::HeaderMap;

pub const PROXY_FORWARDED_HEADERS: &[&str] = &[
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

pub fn has_proxy_forwarded_headers(headers: &HeaderMap) -> bool {
    PROXY_FORWARDED_HEADERS
        .iter()
        .any(|name| headers.contains_key(*name))
}

pub struct PeerConnectAddr(pub Option<SocketAddr>);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ipv4Cidr {
    pub network: u32,
    pub prefix_len: u8,
}

impl Ipv4Cidr {
    pub fn new(addr: Ipv4Addr, prefix_len: u8) -> Option<Self> {
        if prefix_len > 32 {
            return None;
        }
        let mask = Self::mask(prefix_len);
        Some(Self {
            network: u32::from(addr) & mask,
            prefix_len,
        })
    }

    pub fn mask(prefix_len: u8) -> u32 {
        if prefix_len == 0 {
            0
        } else {
            !0u32 << (32 - prefix_len)
        }
    }

    pub fn contains(&self, ip: Ipv4Addr) -> bool {
        (u32::from(ip) & Self::mask(self.prefix_len)) == self.network
    }

    /// Returns a representative probe IPv4 inside this subnet (e.g. `.1` for `/24`)
    /// so zero-packet UDP route queries reveal the local TUN/VLAN source IP.
    pub fn probe_addr(&self) -> Ipv4Addr {
        if self.prefix_len >= 31 {
            Ipv4Addr::from(self.network)
        } else {
            Ipv4Addr::from(self.network | 1)
        }
    }

    /// Expand host addresses inside this CIDR up to `max_hosts` for active
    /// unicast discovery across L3 TUN overlays (EasyTier, WireGuard, Tailscale)
    /// and routed cross-subnet VLANs.
    pub fn host_addrs(&self, max_hosts: usize) -> Vec<Ipv4Addr> {
        if max_hosts == 0 {
            return Vec::new();
        }
        if self.prefix_len == 32 {
            return vec![Ipv4Addr::from(self.network)];
        }
        if self.prefix_len == 31 {
            return vec![
                Ipv4Addr::from(self.network),
                Ipv4Addr::from(self.network | 1),
            ]
            .into_iter()
            .take(max_hosts)
            .collect();
        }
        let host_bits = 32 - self.prefix_len;
        // Cap expansion to /22 (1022 hosts) to avoid accidental /8 sweeps.
        if host_bits > 10 {
            return Vec::new();
        }
        let total_hosts = (1u32 << host_bits).saturating_sub(2) as usize;
        let count = total_hosts.min(max_hosts);
        (1..=count)
            .map(|offset| Ipv4Addr::from(self.network + offset as u32))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubnetOrSeedEntry {
    Cidr(Ipv4Cidr),
    SeedHost { host: String, port: Option<u16> },
}

/// Parse a user-configured subnet CIDR (`10.144.144.0/24`, `192.168.2.0/24`, `11.0.0.0/24`)
/// or explicit seed node (`10.144.144.2`, `10.144.144.2:28765`, `easytier-node:28765`).
///
/// Rejects overly broad public prefixes (`0.0.0.0/0` or non-private prefixes shorter than `/16`)
/// so a typo can never expose LAN peer endpoints to the entire public internet.
pub fn parse_subnet_or_seed(raw: &str) -> Result<SubnetOrSeedEntry, String> {
    let trimmed = raw
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("空子网条目".into());
    }
    if trimmed.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(format!("无效的子网或节点格式: {trimmed}"));
    }

    if let Some((ip_part, prefix_part)) = trimmed.split_once('/') {
        let ip: Ipv4Addr = ip_part
            .trim()
            .parse()
            .map_err(|_| format!("无效的 IPv4 网段地址: {ip_part}"))?;
        let prefix: u8 = prefix_part
            .trim()
            .parse()
            .map_err(|_| format!("无效的子网掩码前缀长度: /{prefix_part}"))?;
        if ip.is_unspecified() {
            return Err("禁止配置 0.0.0.0 通配网段".into());
        }
        let is_private_or_overlay = is_builtin_overlay_or_private_v4(ip);
        let min_prefix = if is_private_or_overlay { 8 } else { 16 };
        if prefix == 0 || prefix > 32 {
            return Err(format!(
                "子网前缀长度必须在 /{min_prefix} 到 /32 之间: {trimmed}"
            ));
        }
        if prefix < min_prefix {
            return if is_private_or_overlay {
                Err(format!(
                    "私网/内置组网网段 {trimmed} 掩码过宽（至少需要 /{min_prefix}，例如 /24）"
                ))
            } else {
                Err(format!(
                    "自定义非标准组网网段 {trimmed} 掩码过宽（至少需要 /{min_prefix}，例如 /24）"
                ))
            };
        }
        let cidr =
            Ipv4Cidr::new(ip, prefix).ok_or_else(|| format!("无效的 CIDR 网段: {trimmed}"))?;
        return Ok(SubnetOrSeedEntry::Cidr(cidr));
    }

    // Check `host:port` (IPv4 or hostname)
    if let Some((host_part, port_part)) = trimmed.rsplit_once(':') {
        if !host_part.contains(':') && !host_part.is_empty() {
            let port: u16 = port_part
                .parse()
                .map_err(|_| format!("无效的端口号: {port_part}"))?;
            if port == 0 {
                return Err("端口号不能为 0".into());
            }
            if let Ok(ip) = host_part.parse::<Ipv4Addr>() {
                if ip.is_unspecified() || ip.is_broadcast() {
                    return Err(format!("无效的节点 IP: {host_part}"));
                }
            }
            return Ok(SubnetOrSeedEntry::SeedHost {
                host: host_part.to_string(),
                port: Some(port),
            });
        }
    }

    if let Ok(ip) = trimmed.parse::<Ipv4Addr>() {
        if ip.is_unspecified() || ip.is_broadcast() {
            return Err(format!("无效的节点 IP: {trimmed}"));
        }
        return Ok(SubnetOrSeedEntry::SeedHost {
            host: ip.to_string(),
            port: None,
        });
    }

    // Hostname seed (e.g. EasyTier MagicDNS / Tailscale MagicDNS / LAN hostname)
    if trimmed
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_'))
    {
        return Ok(SubnetOrSeedEntry::SeedHost {
            host: trimmed.to_string(),
            port: None,
        });
    }

    Err(format!("无法识别的子网或组网节点地址: {trimmed}"))
}

/// Validate and normalize user-entered extra subnet/seed strings (comma, semicolon, or newline separated).
pub fn normalize_extra_subnets(entries: &[String]) -> Result<Vec<String>, String> {
    let mut normalized = Vec::new();
    for raw in entries {
        for token in raw.split([',', ';', '\n', '\r']) {
            let clean = token.trim();
            if clean.is_empty() {
                continue;
            }
            let _ = parse_subnet_or_seed(clean)?;
            if !normalized.iter().any(|existing| existing == clean) {
                normalized.push(clean.to_string());
            }
        }
    }
    if normalized.len() > 64 {
        return Err("自定义子网与种子节点数量最多支持 64 条".into());
    }
    Ok(normalized)
}

pub fn bind_addr(port: u16, allow_lan_access: bool) -> SocketAddr {
    let host = if allow_lan_access {
        [0, 0, 0, 0]
    } else {
        [127, 0, 0, 1]
    };
    SocketAddr::from((host, port))
}

pub fn bind_host(allow_lan_access: bool) -> &'static str {
    if allow_lan_access {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    }
}

/// Built-in private LAN + common SD-WAN / overlay virtual network ranges:
/// - RFC1918 (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16` — incl. EasyTier `10.144.144.0/24` & `10.126.126.0/24`)
/// - RFC6598 CGNAT (`100.64.0.0/10` — Tailscale, EasyTier, ZeroTier, WireGuard)
/// - RFC2544 Virtual/Overlay (`198.18.0.0/15` — EasyTier / TUN virtual networks)
fn is_builtin_overlay_or_private_v4(v4: Ipv4Addr) -> bool {
    let octets = v4.octets();
    v4.is_private()
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
}

/// Likely virtual SD-WAN / TUN overlay IPv4 (`10.144.144.x`, `10.126.126.x`, `100.64/10`, `198.18/15`).
pub fn is_likely_overlay_ipv4(v4: Ipv4Addr) -> bool {
    let o = v4.octets();
    (o[0] == 10 && (o[1] == 144 || o[1] == 126))
        || (o[0] == 100 && (64..=127).contains(&o[1]))
        || (o[0] == 198 && (o[1] == 18 || o[1] == 19))
}

/// Returns true only for loopback, RFC1918 private LAN, link-local, IPv6 ULA (`fc00::/7`),
/// `100.64.0.0/10`, or `198.18.0.0/15` overlay addresses. Public internet addresses return false.
pub fn is_lan_or_loopback_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_lan_or_loopback_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_lan_or_loopback_v4(mapped);
            }
            let first = v6.segments()[0];
            v6.is_loopback() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
        }
    }
}

/// Extended peer origin check that accepts built-in LAN/overlay subnets PLUS any
/// user-configured `extra_subnets` CIDRs/seeds (e.g. custom EasyTier virtual subnets)
/// or already-paired peer addresses.
pub fn is_allowed_peer_ip(ip: IpAddr, extra_subnets: &[String], paired_hosts: &[String]) -> bool {
    if is_lan_or_loopback_ip(ip) {
        return true;
    }
    let v4 = match ip {
        IpAddr::V4(v4) => Some(v4),
        IpAddr::V6(v6) => v6.to_ipv4_mapped(),
    };
    if let Some(v4) = v4 {
        let v4_str = v4.to_string();
        if paired_hosts.iter().any(|h| h.trim() == v4_str) {
            return true;
        }
        for raw in extra_subnets {
            for token in raw.split([',', ';', '\n', '\r']) {
                match parse_subnet_or_seed(token) {
                    Ok(SubnetOrSeedEntry::Cidr(cidr)) if cidr.contains(v4) => return true,
                    Ok(SubnetOrSeedEntry::SeedHost { host, .. }) if host == v4_str => return true,
                    _ => {}
                }
            }
        }
    } else {
        let ip_str = ip.to_string();
        if paired_hosts.iter().any(|h| h.trim() == ip_str) {
            return true;
        }
    }
    false
}

fn is_routable_or_custom_ipv4(v4: Ipv4Addr, custom_cidrs: &[Ipv4Cidr]) -> bool {
    if v4.is_loopback()
        || v4.is_unspecified()
        || v4.is_broadcast()
        || v4.is_multicast()
        || v4.is_link_local()
    {
        return false;
    }
    is_builtin_overlay_or_private_v4(v4) || custom_cidrs.iter().any(|c| c.contains(v4))
}

fn is_lan_or_loopback_v4(v4: Ipv4Addr) -> bool {
    v4.is_loopback() || v4.is_link_local() || is_builtin_overlay_or_private_v4(v4)
}

fn probe_route_ipv4(target: &str) -> Option<Ipv4Addr> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect(target).ok()?;
    match sock.local_addr().ok()? {
        SocketAddr::V4(local) => Some(*local.ip()),
        SocketAddr::V6(_) => None,
    }
}

const DEFAULT_ROUTE_PROBES: &[&str] = &[
    "239.255.77.67:28767",
    "192.168.1.1:80",
    "10.144.144.1:80",    // EasyTier CLI default virtual network
    "10.126.126.1:80",    // EasyTier GUI default virtual network
    "100.100.100.100:80", // Tailscale / CGNAT virtual network
    "198.18.0.1:80",      // EasyTier / TUN 198.18.0.0/15 range
    "192.168.192.1:80",   // ZeroTier common virtual network
    "10.0.0.1:80",
    "172.16.0.1:80",
];

fn collect_local_lan_ipv4s_with_interfaces(
    custom_cidrs: &[Ipv4Cidr],
    custom_probes: &[String],
) -> (Vec<Ipv4Addr>, Vec<(Ipv4Addr, bool)>) {
    let iface_entries = platform_interface_ipv4s(custom_cidrs);
    let mut found = Vec::<Ipv4Addr>::new();
    let mut seen = HashSet::<Ipv4Addr>::new();
    let mut push_unique = |ip: Ipv4Addr| {
        if is_routable_or_custom_ipv4(ip, custom_cidrs) && seen.insert(ip) {
            found.push(ip);
        }
    };

    // Zero-packet OS kernel routing table probes across physical LAN + EasyTier/Tailscale/ZeroTier ranges.
    for probe_target in DEFAULT_ROUTE_PROBES {
        if let Some(ip) = probe_route_ipv4(probe_target) {
            push_unique(ip);
        }
    }

    for custom_target in custom_probes {
        if let Some(ip) = probe_route_ipv4(custom_target) {
            push_unique(ip);
        }
    }

    for &(ip, _is_overlay) in &iface_entries {
        push_unique(ip);
    }

    (found, iface_entries)
}

/// Enumerate active private LAN + virtual SD-WAN (EasyTier, Tailscale, ZeroTier, WireGuard)
/// IPv4 addresses across macOS, Windows, and Linux.
pub fn local_lan_ipv4_addrs_with_subnets(extra_subnets: &[String]) -> Vec<Ipv4Addr> {
    let mut custom_cidrs = Vec::new();
    let mut custom_probes = Vec::new();
    for raw in extra_subnets {
        for token in raw.split([',', ';', '\n', '\r']) {
            match parse_subnet_or_seed(token) {
                Ok(SubnetOrSeedEntry::Cidr(cidr)) => {
                    custom_probes.push(format!("{}:80", cidr.probe_addr()));
                    custom_cidrs.push(cidr);
                }
                Ok(SubnetOrSeedEntry::SeedHost { host, port }) => {
                    if let Ok(ip) = host.parse::<Ipv4Addr>() {
                        if let Some(cidr) = Ipv4Cidr::new(ip, 24) {
                            custom_cidrs.push(cidr);
                        }
                        custom_probes.push(format!("{ip}:{}", port.unwrap_or(28765)));
                    }
                }
                Err(_) => {}
            }
        }
    }

    collect_local_lan_ipv4s_with_interfaces(&custom_cidrs, &custom_probes).0
}

/// Collect `/24` or user-specified unicast UDP discovery targets and explicit HTTP seed targets
/// for EasyTier / Tailscale / WireGuard virtual TUN interfaces and cross-subnet configurations.
pub fn collect_overlay_discovery_targets(
    extra_subnets: &[String],
    default_port: u16,
) -> (Vec<Ipv4Addr>, Vec<(String, u16)>) {
    const MAX_UNICAST_HOSTS: usize = 1024;
    const MAX_HTTP_SEEDS: usize = 256;

    let mut custom_cidrs = Vec::new();
    let mut custom_cidr_set = HashSet::new();
    let mut probe_cidrs = Vec::new();
    let mut custom_probes = Vec::new();
    let mut seeds = Vec::<(String, u16)>::new();
    let mut seen_seeds = HashSet::<(String, u16)>::new();

    for raw in extra_subnets {
        for token in raw.split([',', ';', '\n', '\r']) {
            match parse_subnet_or_seed(token) {
                Ok(SubnetOrSeedEntry::Cidr(cidr)) => {
                    custom_probes.push(format!("{}:80", cidr.probe_addr()));
                    probe_cidrs.push(cidr);
                    if custom_cidr_set.insert(cidr) {
                        custom_cidrs.push(cidr);
                    }
                }
                Ok(SubnetOrSeedEntry::SeedHost { host, port }) => {
                    if let Ok(ip) = host.parse::<Ipv4Addr>() {
                        if let Some(cidr) = Ipv4Cidr::new(ip, 24) {
                            probe_cidrs.push(cidr);
                        }
                        custom_probes.push(format!("{ip}:{}", port.unwrap_or(default_port)));
                    }
                    let entry = (host, port.unwrap_or(default_port));
                    if seen_seeds.insert(entry.clone()) {
                        seeds.push(entry);
                    }
                }
                Err(_) => {}
            }
        }
    }

    let (local_ips, iface_entries) =
        collect_local_lan_ipv4s_with_interfaces(&probe_cidrs, &custom_probes);
    let local_ip_set: HashSet<Ipv4Addr> = local_ips.iter().copied().collect();

    let mut unicast_cidrs = custom_cidrs.clone();
    let mut unicast_cidr_set = custom_cidr_set;
    // Automatically add `/24` neighborhood sweep for active virtual TUN/overlay interfaces
    // (such as macOS `utun*` or Windows Wintun `10.144.144.x` / `10.126.126.x` / `100.64.x.x`).
    for (ip, is_overlay_iface) in iface_entries {
        if is_overlay_iface || is_likely_overlay_ipv4(ip) {
            if let Some(cidr24) = Ipv4Cidr::new(ip, 24) {
                if unicast_cidr_set.insert(cidr24) {
                    unicast_cidrs.push(cidr24);
                }
            }
        }
    }
    for &ip in &local_ips {
        if is_likely_overlay_ipv4(ip) {
            if let Some(cidr24) = Ipv4Cidr::new(ip, 24) {
                if unicast_cidr_set.insert(cidr24) {
                    unicast_cidrs.push(cidr24);
                }
            }
        }
    }

    let mut unicast_hosts = Vec::new();
    let mut seen_unicast = HashSet::<Ipv4Addr>::new();
    for cidr in &unicast_cidrs {
        for host_ip in cidr.host_addrs(256) {
            if !local_ip_set.contains(&host_ip) && seen_unicast.insert(host_ip) {
                unicast_hosts.push(host_ip);
                if unicast_hosts.len() >= MAX_UNICAST_HOSTS {
                    break;
                }
            }
        }
        if unicast_hosts.len() >= MAX_UNICAST_HOSTS {
            break;
        }
    }

    // Also include explicit user-configured CIDRs (up to 254 hosts total) in HTTP fallback seeds
    // so EasyTier `no-tun` subnet proxy or UDP-firewalled Windows hosts are discovered on scan.
    for cidr in &custom_cidrs {
        for host_ip in cidr.host_addrs(254) {
            if seeds.len() >= MAX_HTTP_SEEDS {
                break;
            }
            if !local_ip_set.contains(&host_ip) {
                let target = (host_ip.to_string(), default_port);
                if seen_seeds.insert(target.clone()) {
                    seeds.push(target);
                }
            }
        }
    }

    (unicast_hosts, seeds)
}

#[cfg(unix)]
fn platform_interface_ipv4s(custom_cidrs: &[Ipv4Cidr]) -> Vec<(Ipv4Addr, bool)> {
    let mut primary = Vec::new();
    let mut overlays = Vec::new();
    let mut secondary = Vec::new();
    unsafe {
        let mut addrs: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut addrs) != 0 || addrs.is_null() {
            return primary;
        }
        let mut cursor = addrs;
        while !cursor.is_null() {
            let entry = &*cursor;
            let flags = entry.ifa_flags as i32;
            let is_up = (flags & libc::IFF_UP) != 0;
            let is_loopback = (flags & libc::IFF_LOOPBACK) != 0;
            let is_p2p = (flags & libc::IFF_POINTOPOINT) != 0;
            if is_up
                && !is_loopback
                && !entry.ifa_addr.is_null()
                && (*entry.ifa_addr).sa_family as i32 == libc::AF_INET
            {
                let sin = &*(entry.ifa_addr as *const libc::sockaddr_in);
                let ip = Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
                if is_routable_or_custom_ipv4(ip, custom_cidrs) {
                    let name = if entry.ifa_name.is_null() {
                        String::new()
                    } else {
                        std::ffi::CStr::from_ptr(entry.ifa_name)
                            .to_string_lossy()
                            .into_owned()
                    };
                    let lower = name.to_ascii_lowercase();
                    let is_container_bridge = lower.starts_with("docker")
                        || lower.starts_with("br-")
                        || lower.starts_with("veth")
                        || lower.starts_with("virbr")
                        || lower.starts_with("cni")
                        || lower.starts_with("flannel");
                    if is_container_bridge && !custom_cidrs.iter().any(|c| c.contains(ip)) {
                        cursor = entry.ifa_next;
                        continue;
                    }
                    let is_physical = lower.starts_with("en")
                        || lower.starts_with("eth")
                        || lower.starts_with("wl");
                    let is_overlay = is_p2p
                        || lower.starts_with("utun")
                        || lower.starts_with("tun")
                        || lower.starts_with("tap")
                        || lower.starts_with("wg")
                        || lower.starts_with("zt")
                        || lower.contains("easytier")
                        || lower.contains("tailscale")
                        || is_likely_overlay_ipv4(ip);
                    if is_physical {
                        primary.push((ip, false));
                    } else if is_overlay {
                        overlays.push((ip, true));
                    } else {
                        secondary.push((ip, false));
                    }
                }
            }
            cursor = entry.ifa_next;
        }
        libc::freeifaddrs(addrs);
    }
    primary.extend(overlays);
    primary.extend(secondary);
    primary
}

#[cfg(windows)]
fn platform_interface_ipv4s(custom_cidrs: &[Ipv4Cidr]) -> Vec<(Ipv4Addr, bool)> {
    use std::net::ToSocketAddrs;

    let mut addrs = Vec::new();
    let host = raw_os_hostname();
    if !host.is_empty() {
        if let Ok(resolved) = (host.as_str(), 0u16).to_socket_addrs() {
            for sock in resolved {
                if let SocketAddr::V4(v4) = sock {
                    let ip = *v4.ip();
                    if is_routable_or_custom_ipv4(ip, custom_cidrs)
                        && !addrs.iter().any(|(existing, _)| *existing == ip)
                    {
                        addrs.push((ip, is_likely_overlay_ipv4(ip)));
                    }
                }
            }
        }
    }

    // Fallback to cached silent `ipconfig` parsing so Wintun/TAP virtual adapters
    // (EasyTier, Tailscale, WireGuard, ZeroTier) without DNS registration are enumerated.
    for (ip, is_overlay) in windows_ipconfig_ipv4s() {
        if is_routable_or_custom_ipv4(ip, custom_cidrs)
            && !addrs.iter().any(|(existing, _)| *existing == ip)
        {
            addrs.push((ip, is_overlay));
        }
    }

    addrs
}

#[cfg(windows)]
fn windows_ipconfig_ipv4s() -> Vec<(Ipv4Addr, bool)> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CACHE_TTL_MS: u64 = 15_000;

    static CACHE: OnceLock<Mutex<(u64, Vec<(Ipv4Addr, bool)>)>> = OnceLock::new();
    let now_ms = crate::util::time::unix_now_ms();
    let slot = CACHE.get_or_init(|| Mutex::new((0, Vec::new())));
    if let Ok(guard) = slot.lock() {
        if guard.0 > 0 && now_ms.saturating_sub(guard.0) <= CACHE_TTL_MS {
            return guard.1.clone();
        }
    }

    let mut parsed = Vec::new();
    if let Ok(output) = std::process::Command::new("ipconfig")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        let mut current_adapter_overlay = false;
        for line in text.lines() {
            let trimmed = line.trim();
            if !line.starts_with(' ') && !line.starts_with('\t') && trimmed.ends_with(':') {
                let lower = trimmed.to_ascii_lowercase();
                current_adapter_overlay = lower.contains("easytier")
                    || lower.contains("tailscale")
                    || lower.contains("wireguard")
                    || lower.contains("zerotier")
                    || lower.contains("wintun")
                    || lower.contains("tun")
                    || lower.contains("tap");
                continue;
            }
            if let Some((_, rhs)) = trimmed.split_once(':') {
                let candidate = rhs
                    .trim()
                    .trim_end_matches(|c: char| !c.is_ascii_digit() && c != '.');
                if let Ok(ip) = candidate.parse::<Ipv4Addr>() {
                    if !ip.is_loopback() && !ip.is_unspecified() && ip.octets()[0] != 255 {
                        // Skip subnet masks like 255.255.255.0
                        parsed.push((ip, current_adapter_overlay || is_likely_overlay_ipv4(ip)));
                    }
                }
            }
        }
    }

    if let Ok(mut guard) = slot.lock() {
        *guard = (now_ms.max(1), parsed.clone());
    }
    parsed
}

#[cfg(not(any(unix, windows)))]
fn platform_interface_ipv4s(_custom_cidrs: &[Ipv4Cidr]) -> Vec<(Ipv4Addr, bool)> {
    Vec::new()
}

/// Cross-platform default device display name (macOS / Windows / Linux).
pub fn default_device_name() -> String {
    let raw = raw_os_hostname();
    let cleaned: String = raw
        .trim()
        .trim_end_matches(".local")
        .chars()
        .filter(|ch| !ch.is_control())
        .take(64)
        .collect();
    if cleaned.is_empty() {
        format!("Coding-Tools-{}", std::env::consts::OS)
    } else {
        cleaned
    }
}

fn raw_os_hostname() -> String {
    for key in ["COMPUTERNAME", "HOSTNAME"] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    #[cfg(unix)]
    unsafe {
        let mut buf = [0u8; 256];
        if libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) == 0 {
            let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            if let Ok(text) = std::str::from_utf8(&buf[..len]) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_access_is_the_default() {
        assert_eq!(
            bind_addr(28766, false),
            SocketAddr::from(([127, 0, 0, 1], 28766))
        );
    }

    #[test]
    fn lan_access_binds_all_interfaces() {
        assert_eq!(
            bind_addr(28766, true),
            SocketAddr::from(([0, 0, 0, 0], 28766))
        );
    }

    #[test]
    fn lan_ip_classifier_accepts_private_easytier_overlay_and_rejects_public_internet() {
        assert!(is_lan_or_loopback_ip("127.0.0.1".parse().unwrap()));
        assert!(is_lan_or_loopback_ip("192.168.1.42".parse().unwrap()));
        assert!(is_lan_or_loopback_ip("10.144.144.2".parse().unwrap()));
        assert!(is_lan_or_loopback_ip("10.126.126.5".parse().unwrap()));
        assert!(is_lan_or_loopback_ip("172.20.10.2".parse().unwrap()));
        assert!(is_lan_or_loopback_ip("100.100.10.5".parse().unwrap()));
        assert!(is_lan_or_loopback_ip("198.18.0.12".parse().unwrap()));
        assert!(is_lan_or_loopback_ip("198.19.255.1".parse().unwrap()));
        assert!(is_lan_or_loopback_ip("::1".parse().unwrap()));
        assert!(is_lan_or_loopback_ip("::ffff:192.168.1.8".parse().unwrap()));
        assert!(!is_lan_or_loopback_ip("8.8.8.8".parse().unwrap()));
        assert!(!is_lan_or_loopback_ip("1.1.1.1".parse().unwrap()));
        assert!(!is_lan_or_loopback_ip("172.32.0.1".parse().unwrap()));
        assert!(!is_routable_or_custom_ipv4(
            Ipv4Addr::new(169, 254, 1, 2),
            &[]
        ));
        assert!(!is_routable_or_custom_ipv4(Ipv4Addr::LOCALHOST, &[]));
    }

    #[test]
    fn private_subnet_too_broad_reports_private_error_not_custom_error() {
        let private_err = parse_subnet_or_seed("10.0.0.0/4").unwrap_err();
        assert!(private_err.contains("私网/内置组网网段"));
        assert!(private_err.contains("/8"));
        assert!(!private_err.contains("自定义非标准"));

        let public_err = parse_subnet_or_seed("11.0.0.0/8").unwrap_err();
        assert!(public_err.contains("自定义非标准组网网段"));
        assert!(public_err.contains("/16"));
    }

    #[test]
    fn custom_subnet_and_seed_parser_supports_easytier_cidrs_and_blocks_wildcard_internet() {
        let extra = normalize_extra_subnets(&[
            "10.144.144.0/24, 11.0.0.0/24\n10.144.144.8:28765".to_string()
        ])
        .expect("valid subnets");
        assert_eq!(extra.len(), 3);
        assert!(is_allowed_peer_ip(
            "11.0.0.42".parse().unwrap(),
            &extra,
            &[]
        ));
        assert!(!is_allowed_peer_ip("8.8.8.8".parse().unwrap(), &extra, &[]));

        // Wildcard or overly broad non-private ranges must be rejected
        assert!(parse_subnet_or_seed("0.0.0.0/0").is_err());
        assert!(parse_subnet_or_seed("11.0.0.0/8").is_err());
    }

    #[test]
    fn default_device_name_is_non_empty_and_clean() {
        let name = default_device_name();
        assert!(!name.trim().is_empty());
        assert!(!name.chars().any(char::is_control));
    }
}
