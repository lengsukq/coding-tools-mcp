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
    running: bool,
    tunnel: Option<TunnelChild>,
}

static RUNTIME: LazyLock<Mutex<Option<GatewayRuntime>>> = LazyLock::new(|| Mutex::new(None));

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
        if runtime.config == config && runtime.running {
            return Ok(status_from_runtime(runtime));
        }
    }
    if let Some(runtime) = guard.take() {
        stop_runtime(runtime).await;
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
        running: true,
        tunnel,
    };
    let status = status_from_runtime(&runtime);
    *guard = Some(runtime);
    Ok(status)
}

pub async fn stop() -> AppResult<()> {
    let mut guard = RUNTIME.lock().await;
    if let Some(runtime) = guard.take() {
        stop_runtime(runtime).await;
    }
    Ok(())
}

pub async fn status() -> GlobalGatewayStatusDto {
    let guard = RUNTIME.lock().await;
    if let Some(runtime) = guard.as_ref() {
        if runtime.running {
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
    let status = status().await;
    let client = reqwest::Client::builder()
        .timeout(HEALTH_TIMEOUT)
        .build()
        .expect("health client");
    let local = check_url(&client, &status.local_url).await;
    let public = if status.public_url.trim().is_empty() {
        (false, "公网 URL 未配置或尚未获取".into())
    } else {
        check_url(&client, &public_mcp_url(&status.public_url)).await
    };
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

async fn stop_runtime(mut runtime: GatewayRuntime) {
    if let Some(tunnel) = runtime.tunnel.take() {
        match tunnel {
            TunnelChild::Cloudflare { child, pid } => {
                let _ = cloudflare::stop_child(child, pid).await;
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
    }
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
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let log = crate::platform::platform()
                .app_config_dir()?
                .join("global-mcp-cloudflared.log");
            let handle = cloudflare::spawn_cloudflare_tunnel(
                config.local_port,
                &cwd,
                &log,
                "quick",
                "",
                "",
                config.use_proxy,
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

    #[test]
    fn gateway_urls_point_directly_to_single_mcp_endpoint() {
        assert_eq!(local_mcp_url(28765), "http://127.0.0.1:28765/mcp");
        assert_eq!(
            public_mcp_url("https://example.com/"),
            "https://example.com/mcp"
        );
    }
}
