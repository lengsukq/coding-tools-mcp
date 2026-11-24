use std::sync::LazyLock;
use std::time::Duration;

use serde::Serialize;
use tokio::process::Child;
use tokio::sync::Mutex;

use crate::data::DataStore;
use crate::error::{AppError, AppResult};
use crate::settings::{AppSettings, GlobalGatewayConfig};
use crate::tunnel::{cloudflare, frp};

const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);
const OFFLINE_DEBOUNCE_THRESHOLD: u8 = 2;

#[derive(Debug, Default)]
struct TunnelHealthTracker {
    consecutive_offline: u8,
    was_offline: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HealDecision {
    NoOp,
    RestartTunnel,
}

impl TunnelHealthTracker {
    /// Debounce transient network blips: require `OFFLINE_DEBOUNCE_THRESHOLD`
    /// consecutive offline observations before entering outage state, never
    /// kill a live process during an outage (so `loginFailExit = false` can
    /// self-recover), and trigger a heal once connectivity returns if the
    /// child exited or remained unhealthy.
    fn evaluate(
        &mut self,
        child_alive: bool,
        network_online: bool,
        public_healthy: bool,
    ) -> HealDecision {
        if !network_online {
            self.consecutive_offline = self.consecutive_offline.saturating_add(1);
            if self.consecutive_offline >= OFFLINE_DEBOUNCE_THRESHOLD {
                self.was_offline = true;
            }
            return HealDecision::NoOp;
        }

        self.consecutive_offline = 0;
        let recovering = self.was_offline;
        self.was_offline = false;

        if !child_alive {
            return HealDecision::RestartTunnel;
        }
        if recovering && !public_healthy {
            return HealDecision::RestartTunnel;
        }
        HealDecision::NoOp
    }
}

static HEALTH_TRACKER: LazyLock<std::sync::Mutex<TunnelHealthTracker>> =
    LazyLock::new(|| std::sync::Mutex::new(TunnelHealthTracker::default()));

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalGatewayStatusDto {
    pub state: String,
    pub local_url: String,
    pub public_url: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayHealthItem {
    pub label: String,
    pub ok: bool,
    pub detail: String,
}

enum TunnelChild {
    Cloudflare { child: Child, pid: Option<u32> },
    Frp { child: Child, pid: Option<u32> },
}

struct GatewayRuntime {
    config: GlobalGatewayConfig,
    public_url: String,
    tunnel: Option<TunnelChild>,
}

static RUNTIME: LazyLock<Mutex<Option<GatewayRuntime>>> = LazyLock::new(|| Mutex::new(None));
static HEALTH_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(HEALTH_TIMEOUT)
        .pool_idle_timeout(Duration::from_secs(15))
        .pool_max_idle_per_host(2)
        .build()
        .expect("health client")
});

/// Start only the public transport for the single Global MCP endpoint.
///
/// Cloudflare/FRP point directly at the one Global MCP local port; workspace
/// selection happens inside the MCP session rather than in the public URL.
pub async fn ensure_started() -> AppResult<GlobalGatewayStatusDto> {
    let settings = AppSettings::load_or_default();
    let config = settings.global_gateway.clone();
    if !config.enabled {
        return Err(AppError::Message("全局 MCP 公网入口尚未启用。".into()));
    }

    let mut guard = RUNTIME.lock().await;
    if let Some(runtime) = guard.as_ref() {
        if runtime.config == config {
            return Ok(status_from_runtime(runtime));
        }
    }
    if let Some(runtime) = guard.take() {
        stop_runtime(runtime, false).await?;
    }

    let tunnel_result = start_tunnel(&config, &settings).await;
    let (public_url, tunnel) = tunnel_result?;

    let mut effective_config = config.clone();
    if !public_url.is_empty() && effective_config.public_url != public_url {
        effective_config.public_url = public_url.clone();
        let resolved = public_url.clone();
        DataStore::update_file(|data| {
            data.global_gateway.public_url = resolved;
            Ok(())
        })?;
    }

    let runtime = GatewayRuntime {
        config: effective_config,
        public_url,
        tunnel,
    };
    let status = status_from_runtime(&runtime);
    *guard = Some(runtime);
    Ok(status)
}

/// Force-restart the managed Cloudflare Quick Tunnel without touching the MCP runtime.
pub async fn force_restart_cloudflare_tunnel() -> AppResult<GlobalGatewayStatusDto> {
    let settings = AppSettings::load_or_default();
    let config = settings.global_gateway.clone();
    validate_cloudflare_quick_restart(&config)?;

    let mut guard = RUNTIME.lock().await;
    if let Some(runtime) = guard.take() {
        stop_runtime(runtime, true).await?;
    }

    let (public_url, tunnel) = start_tunnel(&config, &settings).await?;
    if public_url.trim().is_empty() {
        let error = "Cloudflare Quick Tunnel 启动后没有返回公网 URL。";
        if let Some(tunnel) = tunnel {
            if let Err(cleanup_error) = stop_tunnel_child(tunnel, true).await {
                return Err(AppError::Message(format!(
                    "{error} 停止新 Tunnel 也失败：{cleanup_error}"
                )));
            }
        }
        return Err(AppError::Message(error.into()));
    }

    let mut effective_config = config.clone();
    effective_config.public_url = public_url.clone();
    let resolved = public_url.clone();
    if let Err(error) = DataStore::update_file(|data| {
        data.global_gateway.public_url = resolved;
        Ok(())
    }) {
        if let Some(tunnel) = tunnel {
            if let Err(cleanup_error) = stop_tunnel_child(tunnel, true).await {
                return Err(AppError::Message(format!(
                    "保存 Cloudflare Quick URL 失败：{error}；停止新 Tunnel 也失败：{cleanup_error}"
                )));
            }
        }
        return Err(error);
    }

    let runtime = GatewayRuntime {
        config: effective_config,
        public_url,
        tunnel,
    };
    let status = status_from_runtime(&runtime);
    *guard = Some(runtime);
    Ok(status)
}

fn validate_cloudflare_quick_restart(config: &GlobalGatewayConfig) -> AppResult<()> {
    if !config.enabled {
        return Err(AppError::Message("请先启用全局 MCP 公网连接。".into()));
    }
    if config.tunnel_type != "cloudflare" {
        return Err(AppError::Message(
            "强制重启仅适用于 Cloudflare Tunnel。".into(),
        ));
    }
    if config.cloudflare_mode != "quick" {
        return Err(AppError::Message(
            "强制重启目前仅支持 Cloudflare Quick Tunnel。".into(),
        ));
    }
    Ok(())
}

pub async fn stop() -> AppResult<()> {
    let mut guard = RUNTIME.lock().await;
    if let Some(runtime) = guard.take() {
        stop_runtime(runtime, false).await?;
    }
    Ok(())
}

/// Synchronous last-resort cleanup invoked from Tauri exit hooks when Tokio may
/// already be shutting down. Prevents orphaned `frpc` / `cloudflared` processes.
pub fn terminate_children_sync() {
    if let Ok(mut guard) = RUNTIME.try_lock() {
        if let Some(mut runtime) = guard.take() {
            if let Some(tunnel) = runtime.tunnel.take() {
                match tunnel {
                    TunnelChild::Cloudflare { mut child, pid } => {
                        if let Some(pid) = pid {
                            let _ = crate::platform::platform().terminate_process_tree(pid);
                        }
                        let _ = child.start_kill();
                        cloudflare::clear_managed_cloudflared_pid("global-mcp");
                    }
                    TunnelChild::Frp { mut child, pid } => {
                        if let Some(pid) = pid {
                            let _ = crate::platform::platform().terminate_process_tree(pid);
                        }
                        let _ = child.start_kill();
                        frp::clear_managed_frpc_pid("global-mcp");
                    }
                }
            }
        }
    }
}

/// Clean up any tunnel child left behind by a previous abnormal exit.
pub async fn cleanup_orphaned_tunnels_on_startup() {
    let _ = frp::stop_recorded_frpc_instance("global-mcp").await;
    let _ = cloudflare::stop_recorded_cloudflared_instance("global-mcp").await;
}

/// Periodic self-healing check for the managed global tunnel (especially FRP
/// overnight router outages). Never kills a live tunnel while the host is offline.
pub async fn heal_tunnel_if_needed() {
    let (config, child_alive, public_url) = {
        let mut guard = RUNTIME.lock().await;
        let Some(runtime) = guard.as_mut() else {
            return;
        };
        if !runtime.config.enabled || runtime.config.tunnel_type != "frp" {
            return;
        }
        let child_alive = match runtime.tunnel.as_mut() {
            Some(TunnelChild::Frp { child, .. }) => child.try_wait().ok().flatten().is_none(),
            _ => false,
        };
        (
            runtime.config.clone(),
            child_alive,
            runtime.public_url.clone(),
        )
    };

    let settings = AppSettings::load_or_default();
    let frp_cfg = frp::global_frp_server_config(&config, &settings, None);
    let network_online =
        probe_frp_server_reachable(&frp_cfg.server_addr, frp_cfg.server_port).await;
    let public_healthy = if network_online && !public_url.trim().is_empty() {
        check_url(&HEALTH_CLIENT, &public_mcp_url(&public_url))
            .await
            .0
    } else {
        false
    };

    let decision = HEALTH_TRACKER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .evaluate(child_alive, network_online, public_healthy);
    if decision != HealDecision::RestartTunnel {
        return;
    }

    let mut guard = RUNTIME.lock().await;
    let Some(runtime) = guard.as_ref() else {
        return;
    };
    if runtime.config != config {
        return;
    }
    if let Some(old_runtime) = guard.take() {
        let _ = stop_runtime(old_runtime, false).await;
    }
    match start_tunnel(&config, &settings).await {
        Ok((next_url, tunnel)) => {
            *guard = Some(GatewayRuntime {
                config,
                public_url: next_url,
                tunnel,
            });
            eprintln!("[global-gateway] auto-healed FRP tunnel after outage/exit");
        }
        Err(error) => {
            eprintln!("[global-gateway] FRP auto-heal failed (will retry): {error}");
            *guard = Some(GatewayRuntime {
                config,
                public_url,
                tunnel: None,
            });
        }
    }
}

async fn probe_frp_server_reachable(server_addr: &str, server_port: u16) -> bool {
    let host = server_addr.trim();
    if host.is_empty() || server_port == 0 {
        return false;
    }
    let target = format!("{host}:{server_port}");
    matches!(
        tokio::time::timeout(
            Duration::from_secs(3),
            tokio::net::TcpStream::connect(&target)
        )
        .await,
        Ok(Ok(_))
    )
}

pub async fn status() -> GlobalGatewayStatusDto {
    {
        let guard = RUNTIME.lock().await;
        if let Some(runtime) = guard.as_ref() {
            return status_from_runtime(runtime);
        }
    }
    let settings = AppSettings::load_or_default();
    GlobalGatewayStatusDto {
        state: "stopped".into(),
        local_url: local_mcp_url(settings.global_gateway.local_port),
        public_url: settings.global_gateway.public_url,
        detail: if settings.global_gateway.enabled {
            "公网入口已配置，当前未运行".into()
        } else {
            "公网入口未启用；本地 Global MCP 可独立运行".into()
        },
    }
}

pub async fn health() -> Vec<GatewayHealthItem> {
    let settings = AppSettings::load_or_default();
    let status = status().await;
    let client = &*HEALTH_CLIENT;
    let public_fut = async {
        if !settings.global_gateway.enabled {
            (true, "公网入口未启用（仅本地模式）".into())
        } else if status.public_url.trim().is_empty() {
            (false, "公网 URL 未配置或尚未获取".into())
        } else {
            let url = public_mcp_url(&status.public_url);
            check_url(client, &url).await
        }
    };
    let (local, public) = tokio::join!(check_url(client, &status.local_url), public_fut);
    vec![
        GatewayHealthItem {
            label: "Global MCP 本地入口".into(),
            ok: local.0,
            detail: local.1,
        },
        GatewayHealthItem {
            label: "Global MCP 公网入口".into(),
            ok: public.0,
            detail: public.1,
        },
    ]
}

async fn check_url(client: &reqwest::Client, url: &str) -> (bool, String) {
    match client.get(url).send().await {
        Ok(response) => {
            let status = response.status();
            (
                status.is_success(),
                format!("HTTP {} · {url}", status.as_u16()),
            )
        }
        Err(error) => (false, error.to_string()),
    }
}

fn status_from_runtime(runtime: &GatewayRuntime) -> GlobalGatewayStatusDto {
    GlobalGatewayStatusDto {
        state: "running".into(),
        local_url: local_mcp_url(runtime.config.local_port),
        public_url: runtime.public_url.clone(),
        detail: format!(
            "{} · 直接连接唯一 Global MCP Endpoint",
            runtime.config.tunnel_type
        ),
    }
}

async fn stop_runtime(mut runtime: GatewayRuntime, confirm_cloudflare_stop: bool) -> AppResult<()> {
    if let Some(tunnel) = runtime.tunnel.take() {
        stop_tunnel_child(tunnel, confirm_cloudflare_stop).await?;
    }
    Ok(())
}

async fn stop_tunnel_child(tunnel: TunnelChild, confirm_cloudflare_stop: bool) -> AppResult<()> {
    match tunnel {
        TunnelChild::Cloudflare { child, pid } => {
            let result = if confirm_cloudflare_stop {
                cloudflare::force_stop_child(child, pid).await
            } else {
                let _ = cloudflare::stop_child(child, pid).await;
                Ok(())
            };
            cloudflare::clear_managed_cloudflared_pid("global-mcp");
            result?;
        }
        TunnelChild::Frp { mut child, pid } => {
            if let Some(pid) = pid {
                let _ = crate::platform::platform().terminate_process_tree(pid);
            }
            let _ = child.kill().await;
            let _ = child.wait().await;
            frp::clear_managed_frpc_pid("global-mcp");
        }
    }
    Ok(())
}

async fn start_tunnel(
    config: &GlobalGatewayConfig,
    settings: &AppSettings,
) -> AppResult<(String, Option<TunnelChild>)> {
    match config.tunnel_type.as_str() {
        "" | "none" => Ok((config.public_url.trim_end_matches('/').to_string(), None)),
        "cloudflare" => {
            if config.cloudflare_mode == "named" {
                return Err(AppError::Message(
                    "Global MCP 当前优先支持 Cloudflare Quick；固定域名请使用 FRP 或外部反向代理。"
                        .into(),
                ));
            }
            let _ = cloudflare::stop_recorded_cloudflared_instance("global-mcp").await;
            let cwd = cloudflare::managed_cloudflared_dir("global-mcp")?;
            std::fs::create_dir_all(&cwd)?;
            let log = crate::platform::platform()
                .app_config_dir()?
                .join("global-mcp-cloudflared.log");
            let handle = cloudflare::spawn_cloudflare_tunnel(
                config.local_port,
                &cwd,
                &log,
                config.use_proxy,
                config.cloudflare_http2,
            )
            .await?;
            let public_url = handle.public_url.clone();
            Ok((
                public_url,
                Some(TunnelChild::Cloudflare {
                    child: handle.child,
                    pid: handle.pid,
                }),
            ))
        }
        "frp" => {
            let _operation_lock = frp::acquire_frpc_operation_lock("global-mcp").await?;
            let _ = frp::stop_recorded_frpc_instance("global-mcp").await?;
            if config.frp_subdomain.trim().is_empty() {
                return Err(AppError::Message("Global MCP FRP 子域名不能为空。".into()));
            }
            let handle = frp::spawn_global_frpc(config, settings).await?;
            let public_url = frp::frp_public_url(config, settings);
            Ok((
                public_url,
                Some(TunnelChild::Frp {
                    child: handle.child,
                    pid: handle.pid,
                }),
            ))
        }
        other => Err(AppError::Message(format!(
            "不支持的 Global MCP tunnel_type: {other}"
        ))),
    }
}

fn local_mcp_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/mcp")
}

fn public_mcp_url(public_url: &str) -> String {
    format!("{}/mcp", public_url.trim_end_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quick_tunnel_config() -> GlobalGatewayConfig {
        GlobalGatewayConfig {
            enabled: true,
            tunnel_type: "cloudflare".into(),
            cloudflare_mode: "quick".into(),
            ..GlobalGatewayConfig::default()
        }
    }

    #[test]
    fn gateway_urls_point_directly_to_single_mcp_endpoint() {
        assert_eq!(local_mcp_url(28765), "http://127.0.0.1:28765/mcp");
        assert_eq!(
            public_mcp_url("https://example.com/"),
            "https://example.com/mcp"
        );
    }

    #[test]
    fn force_restart_requires_enabled_cloudflare_quick_config() {
        assert!(validate_cloudflare_quick_restart(&quick_tunnel_config()).is_ok());

        let mut disabled = quick_tunnel_config();
        disabled.enabled = false;
        assert!(validate_cloudflare_quick_restart(&disabled).is_err());

        let mut frp = quick_tunnel_config();
        frp.tunnel_type = "frp".into();
        assert!(validate_cloudflare_quick_restart(&frp).is_err());

        let mut named = quick_tunnel_config();
        named.cloudflare_mode = "named".into();
        assert!(validate_cloudflare_quick_restart(&named).is_err());
    }

    #[test]
    fn health_tracker_debounces_outages_and_heals_on_recovery() {
        let mut tracker = TunnelHealthTracker::default();
        // Single transient blip does not trigger restart or mark prolonged outage.
        assert_eq!(tracker.evaluate(true, false, false), HealDecision::NoOp);
        assert_eq!(tracker.evaluate(true, true, false), HealDecision::NoOp);

        // Two consecutive offline checks enter outage, never killing live process.
        assert_eq!(tracker.evaluate(true, false, false), HealDecision::NoOp);
        assert_eq!(tracker.evaluate(true, false, false), HealDecision::NoOp);
        // Once back online, unhealthy live process heals once; healthy process does not thrash.
        assert_eq!(
            tracker.evaluate(true, true, false),
            HealDecision::RestartTunnel
        );
        assert_eq!(tracker.evaluate(true, true, true), HealDecision::NoOp);

        // Dead child while online immediately triggers recovery.
        assert_eq!(
            tracker.evaluate(false, true, false),
            HealDecision::RestartTunnel
        );
        // Dead child while offline waits until network returns.
        assert_eq!(tracker.evaluate(false, false, false), HealDecision::NoOp);
    }
}
