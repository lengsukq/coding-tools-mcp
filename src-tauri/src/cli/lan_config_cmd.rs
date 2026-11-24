use std::time::Duration;

use crate::data::DataStore;
use crate::lan_peer::{
    ensure_local_identity, get_peer_secret, list_discovered_peers, list_paired_peer_statuses_async,
    notify_peer_unpair, pair_with_peer, probe_configured_overlay_targets, remove_local_paired_peer,
    trigger_discovery_scan,
};
use crate::local_network::{local_lan_ipv4_addrs_with_subnets, normalize_extra_subnets};
use crate::platform::local_ipc::{
    default_daemon_socket_path, send_daemon_request_at, LocalDaemonRequest, LocalDaemonResponse,
};
use crate::settings::AppSettings;

use super::daemon::try_send_daemon;

pub async fn cmd_lan(args: &[String]) -> Result<(), String> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    match sub {
        "status" | "peers" => cmd_lan_status().await,
        "enable" | "disable" => cmd_lan_set_enabled(sub == "enable").await,
        "pin" => cmd_lan_pin().await,
        "cancel-pin" => cmd_lan_cancel_pin().await,
        "scan" => cmd_lan_scan().await,
        "pair" => cmd_lan_pair(args).await,
        "unpair" | "rm" => cmd_lan_unpair(args).await,
        other => Err(format!(
            "未知 `lan` 子命令 `{other}`（支持: status, enable, disable, pin, scan, pair, unpair）"
        )),
    }
}

async fn cmd_lan_status() -> Result<(), String> {
    let settings = AppSettings::load_or_default();
    let (lan, fp) = ensure_local_identity()?;
    let ips: Vec<String> =
        local_lan_ipv4_addrs_with_subnets(&settings.lan_federation.extra_subnets)
            .into_iter()
            .map(|ip| ip.to_string())
            .collect();
    println!(
        "本机节点   : {} ({}) · 指纹 {}",
        lan.device_name, lan.device_id, fp
    );
    println!(
        "联邦开关   : {} · 信任级别 {} · 共享模式 {}",
        if settings.lan_federation.enabled {
            "已开启"
        } else {
            "未开启"
        },
        settings.lan_federation.trust_level,
        settings.lan_federation.share_mode
    );
    println!(
        "本机组网IP : {}",
        if ips.is_empty() {
            "无".into()
        } else {
            ips.join(", ")
        }
    );
    let statuses = list_paired_peer_statuses_async(Some(&settings), true).await;
    if statuses.is_empty() {
        println!("已配对设备 : 暂无");
    } else {
        println!("已配对设备 ({} 台)：", statuses.len());
        for peer in statuses {
            println!(
                "  - {} [{}] ({}) {}:{} · {} · 工作区 {} 个",
                peer.device_name,
                peer.device_id,
                peer.os,
                peer.host,
                peer.port,
                if peer.online { "在线" } else { "离线" },
                peer.workspace_count
            );
        }
    }
    Ok(())
}

async fn cmd_lan_set_enabled(enable: bool) -> Result<(), String> {
    let _ = ensure_local_identity()?;
    DataStore::update_file(|data| {
        data.lan_federation.enabled = enable;
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    let _ = try_send_daemon(&LocalDaemonRequest::Reload {
        tunnel_changed: false,
    })
    .await;
    println!(
        "局域网与组网联邦已{}。",
        if enable { "开启" } else { "关闭" }
    );
    Ok(())
}

async fn cmd_lan_pin() -> Result<(), String> {
    let sock = default_daemon_socket_path()?;
    match send_daemon_request_at(&sock, &LocalDaemonRequest::GeneratePin).await {
        Ok(LocalDaemonResponse::PinIssued { payload }) => {
            println!(
                "已生成 6 位一次性配对码（{} 秒内有效）：",
                payload.pin.remaining_seconds
            );
            println!("  配对码   : {}", payload.pin.pin);
            println!(
                "  本机名称 : {} [{}]",
                payload.device_name, payload.device_id
            );
            if !payload.addresses.is_empty() {
                let targets = payload
                    .addresses
                    .iter()
                    .map(|ip| format!("{ip}:{}", payload.mcp_port))
                    .collect::<Vec<_>>()
                    .join(" 或 ");
                println!("  连接地址 : {targets}");
            }
            Ok(())
        }
        Ok(LocalDaemonResponse::Error { message }) => Err(message),
        _ => Err(
            "未检测到正在运行的后台 `serve` 守护进程；请先运行 `coding-tools-mcp serve --lan --pair-pin` 或启动后台服务后再生成配对码。"
                .into(),
        ),
    }
}

async fn cmd_lan_cancel_pin() -> Result<(), String> {
    let sock = default_daemon_socket_path()?;
    let _ = send_daemon_request_at(&sock, &LocalDaemonRequest::CancelPin).await?;
    println!("已取消当前活跃配对码。");
    Ok(())
}

async fn cmd_lan_scan() -> Result<(), String> {
    let _ = ensure_local_identity()?;
    DataStore::update_file(|data| {
        data.lan_federation.enabled = true;
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    trigger_discovery_scan();
    probe_configured_overlay_targets().await;
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let found = list_discovered_peers();
    if found.is_empty() {
        println!("未发现其他在线节点（若跨网段使用 EasyTier/WireGuard，可通过 `config set extra-subnets 10.144.144.0/24` 配置网段，或直接用 `lan pair <IP:端口> <PIN>` 配对）。");
    } else {
        println!("发现 {} 台在线节点：", found.len());
        for p in found {
            println!(
                "  - {} [{}] ({}/{}) {}:{} {}",
                p.device_name,
                p.device_id,
                p.os,
                p.arch,
                p.host,
                p.port,
                if p.already_paired { "[已配对]" } else { "" }
            );
        }
    }
    Ok(())
}

async fn cmd_lan_pair(args: &[String]) -> Result<(), String> {
    let (Some(target), Some(pin)) = (args.get(1), args.get(2)) else {
        return Err("用法: coding-tools-mcp lan pair <IP[:端口]> <6位配对码>".into());
    };
    let (host, port) = parse_host_and_port(target, 28765)?;
    let record = pair_with_peer(&host, port, pin).await?;
    let _ = try_send_daemon(&LocalDaemonRequest::Reload {
        tunnel_changed: false,
    })
    .await;
    println!(
        "双向配对成功：{} [{}] ({}:{})",
        record.device_name, record.device_id, record.host, record.port
    );
    Ok(())
}

async fn cmd_lan_unpair(args: &[String]) -> Result<(), String> {
    let Some(device_id) = args.get(1) else {
        return Err("用法: coding-tools-mcp lan unpair <DEVICE_ID>".into());
    };
    let prior_secret = get_peer_secret(device_id, None);
    if let Some(peer) = remove_local_paired_peer(device_id)? {
        if let Some(secret) = prior_secret.as_deref() {
            notify_peer_unpair(&peer, secret).await;
        }
        let _ = try_send_daemon(&LocalDaemonRequest::Reload {
            tunnel_changed: false,
        })
        .await;
        println!("已解除配对：{} [{}]", peer.device_name, peer.device_id);
    } else {
        println!("未找到已配对设备 `{device_id}`");
    }
    Ok(())
}

fn parse_host_and_port(raw: &str, default_port: u16) -> Result<(String, u16), String> {
    let trimmed = raw
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/');
    if let Some((host, port_str)) = trimmed.rsplit_once(':') {
        if !host.contains(':') {
            let port: u16 = port_str
                .parse()
                .map_err(|_| format!("无效端口号: {port_str}"))?;
            return Ok((host.to_string(), port));
        }
    }
    Ok((trimmed.to_string(), default_port))
}

pub async fn cmd_config(args: &[String]) -> Result<(), String> {
    let sub = args.first().map(String::as_str).unwrap_or("show");
    match sub {
        "show" | "list" => {
            let s = AppSettings::load_or_default();
            println!("port               = {}", s.global_gateway.local_port);
            println!("auth               = {}", s.global_mcp_auth_type);
            println!(
                "server-name        = {}",
                crate::settings::resolved_mcp_server_name(&s.global_mcp_server_name)
            );
            println!("device-name        = {}", s.lan_federation.device_name);
            println!("allow-local-noauth = {}", s.allow_local_noauth);
            println!("allow-lan-mcp      = {}", s.allow_lan_access);
            println!("lan-enabled        = {}", s.lan_federation.enabled);
            println!("lan-trust          = {}", s.lan_federation.trust_level);
            println!(
                "extra-subnets      = {}",
                s.lan_federation.extra_subnets.join(",")
            );
            println!("proxy-mode         = {}", s.proxy.mode);
            println!("proxy-url          = {}", s.proxy.url);
            Ok(())
        }
        "set" => {
            let (Some(key), Some(val)) = (args.get(1), args.get(2)) else {
                return Err("用法: coding-tools-mcp config set <KEY> <VALUE>".into());
            };
            apply_config_kv(key, val)?;
            let _ = try_send_daemon(&LocalDaemonRequest::Reload {
                tunnel_changed: false,
            })
            .await;
            println!("已更新配置 `{key}` = `{val}`");
            Ok(())
        }
        other => Err(format!("未知 `config` 子命令 `{other}`（支持: show, set）")),
    }
}

fn parse_bool_flag(raw: &str) -> Result<bool, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "on" | "yes" => Ok(true),
        "false" | "0" | "off" | "no" => Ok(false),
        _ => Err(format!("布尔参数 `{raw}` 无效，请使用 true 或 false")),
    }
}

fn apply_config_kv(key: &str, val: &str) -> Result<(), String> {
    match key {
        "port" => {
            let port: u16 = val.parse().map_err(|_| format!("无效端口号: {val}"))?;
            if port == 0 {
                return Err("端口号不能为 0".into());
            }
            DataStore::update_file(|d| {
                d.global_gateway.local_port = port;
                Ok(())
            })
            .map_err(|e| e.to_string())
        }
        "auth" => {
            if !matches!(val, "oauth" | "bearer" | "noauth") {
                return Err("`auth` 仅支持 oauth、bearer 或 noauth".into());
            }
            DataStore::update_file(|d| {
                d.global_mcp_auth_type = val.to_string();
                Ok(())
            })
            .map_err(|e| e.to_string())
        }
        "server-name" => DataStore::update_file(|d| {
            d.global_mcp_server_name = val.trim().to_string();
            Ok(())
        })
        .map_err(|e| e.to_string()),
        "device-name" => {
            let cleaned: String = val
                .trim()
                .chars()
                .filter(|ch| !ch.is_control())
                .take(64)
                .collect();
            if cleaned.is_empty() {
                return Err("`device-name` 不能为空".into());
            }
            DataStore::update_file(|d| {
                d.lan_federation.device_name = cleaned;
                Ok(())
            })
            .map_err(|e| e.to_string())
        }
        "allow-local-noauth" => {
            let flag = parse_bool_flag(val)?;
            DataStore::update_file(|d| {
                d.allow_local_noauth = flag;
                Ok(())
            })
            .map_err(|e| e.to_string())
        }
        "allow-lan-mcp" => {
            let flag = parse_bool_flag(val)?;
            DataStore::update_file(|d| {
                d.allow_lan_access = flag;
                Ok(())
            })
            .map_err(|e| e.to_string())
        }
        "lan-trust" => {
            if !matches!(val, "full" | "readonly") {
                return Err("`lan-trust` 仅支持 full 或 readonly".into());
            }
            DataStore::update_file(|d| {
                d.lan_federation.trust_level = val.to_string();
                Ok(())
            })
            .map_err(|e| e.to_string())
        }
        "extra-subnets" => {
            let normalized = if val.trim().is_empty() || val.trim() == "none" {
                Vec::new()
            } else {
                normalize_extra_subnets(&[val.to_string()])?
            };
            DataStore::update_file(|d| {
                d.lan_federation.extra_subnets = normalized;
                Ok(())
            })
            .map_err(|e| e.to_string())
        }
        "proxy-mode" => {
            if !matches!(val, "none" | "system" | "manual") {
                return Err("`proxy-mode` 仅支持 none、system 或 manual".into());
            }
            DataStore::update_file(|d| {
                d.proxy.mode = val.to_string();
                Ok(())
            })
            .map_err(|e| e.to_string())
        }
        "proxy-url" => DataStore::update_file(|d| {
            d.proxy.url = val.trim().to_string();
            Ok(())
        })
        .map_err(|e| e.to_string()),
        _ => Err(format!("不支持的配置键 `{key}`")),
    }
}
