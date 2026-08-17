use std::sync::LazyLock;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::{Path as AxumPath, State};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::{Json, Router};
use serde::Serialize;
use serde_json::json;
use tokio::process::Child;
use tokio::sync::{oneshot, Mutex};

use crate::data::DataStore;
use crate::error::{AppError, AppResult};
use crate::local_network;
use crate::settings::{AppSettings, GlobalGatewayConfig};
use crate::tunnel::{cloudflare, frp, TunnelServiceKind};
use crate::workspace::WorkspaceProfile;

const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalGatewayStatusDto {
    pub state: String,
    pub local_url: String,
    pub public_url: String,
    pub detail: String,
}

async fn proxy_mcp_protected_resource_metadata(
    State(state): State<ProxyState>,
    AxumPath(workspace_id): AxumPath<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    proxy(
        state,
        workspace_id,
        ".well-known/oauth-protected-resource".into(),
        Method::GET,
        uri,
        headers,
        Bytes::new(),
    )
    .await
}

async fn proxy_mcp_authorization_server_metadata(
    State(state): State<ProxyState>,
    AxumPath(workspace_id): AxumPath<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    proxy(
        state,
        workspace_id,
        ".well-known/oauth-authorization-server".into(),
        Method::GET,
        uri,
        headers,
        Bytes::new(),
    )
    .await
}

async fn proxy_actions_authorization_server_metadata(
    State(state): State<ProxyState>,
    AxumPath(workspace_id): AxumPath<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    proxy(
        state,
        workspace_id,
        "actions/.well-known/oauth-authorization-server".into(),
        Method::GET,
        uri,
        headers,
        Bytes::new(),
    )
    .await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayHealthItem {
    pub label: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Clone)]
struct ProxyState {
    client: reqwest::Client,
}

enum TunnelChild {
    Cloudflare { child: Child, pid: Option<u32> },
    Frp { child: Child, pid: Option<u32> },
}

struct GatewayRuntime {
    config: GlobalGatewayConfig,
    public_url: String,
    shutdown: Option<oneshot::Sender<()>>,
    handle: tauri::async_runtime::JoinHandle<()>,
    running: bool,
    tunnel: Option<TunnelChild>,
}

static RUNTIME: LazyLock<Mutex<Option<GatewayRuntime>>> = LazyLock::new(|| Mutex::new(None));

#[allow(dead_code)]
pub fn workspace_public_base(public_url: &str, workspace_id: &str, actions: bool) -> String {
    let base = public_url.trim_end_matches('/');
    if base.is_empty() {
        return String::new();
    }
    if actions {
        format!("{base}/w/{workspace_id}/actions")
    } else {
        format!("{base}/w/{workspace_id}")
    }
}

pub async fn ensure_started() -> AppResult<GlobalGatewayStatusDto> {
    let settings = AppSettings::load_or_default();
    let config = settings.global_gateway.clone();
    if !config.enabled {
        return Err(AppError::Message("全局共享公网入口尚未启用。".into()));
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

    let listener = bind_listener(config.local_port, settings.allow_lan_access)?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let port = config.local_port;
    let handle = tauri::async_runtime::spawn(async move {
        if let Err(error) = serve(listener, shutdown_rx).await {
            eprintln!("global gateway stopped: {error}");
        }
    });

    let tunnel_result = start_tunnel(&config, &settings).await;
    let (public_url, tunnel) = match tunnel_result {
        Ok(result) => result,
        Err(error) => {
            let _ = shutdown_tx.send(());
            let _ = handle.await;
            return Err(error);
        }
    };

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
        shutdown: Some(shutdown_tx),
        handle,
        tunnel,
        running: true,
    };
    let status = status_from_runtime(&runtime);
    *guard = Some(runtime);
    eprintln!("global gateway listening on http://127.0.0.1:{port}");
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
        local_url: format!("http://127.0.0.1:{}", settings.global_gateway.local_port),
        public_url: settings.global_gateway.public_url,
        detail: if settings.global_gateway.enabled {
            "已配置，当前未运行".into()
        } else {
            "未启用".into()
        },
    }
}

pub async fn health() -> Vec<GatewayHealthItem> {
    let status = status().await;
    let client = reqwest::Client::builder().timeout(HEALTH_TIMEOUT).build().expect("health client");
    let local = check_url(&client, &format!("{}/health", status.local_url.trim_end_matches('/'))).await;
    let public = if status.public_url.trim().is_empty() {
        (false, "公网 URL 未配置或尚未获取".into())
    } else {
        check_url(&client, &format!("{}/health", status.public_url.trim_end_matches('/'))).await
    };
    vec![
        GatewayHealthItem { label: "全局 Gateway 本地入口".into(), ok: local.0, detail: local.1 },
        GatewayHealthItem { label: "全局 Gateway 公网入口".into(), ok: public.0, detail: public.1 },
    ]
}

async fn check_url(client: &reqwest::Client, url: &str) -> (bool, String) {
    match client.get(url).send().await {
        Ok(response) => {
            let status = response.status();
            (status.is_success(), format!("HTTP {} · {url}", status.as_u16()))
        }
        Err(error) => (false, error.to_string()),
    }
}

fn status_from_runtime(runtime: &GatewayRuntime) -> GlobalGatewayStatusDto {
    GlobalGatewayStatusDto {
        state: "running".into(),
        local_url: format!("http://127.0.0.1:{}", runtime.config.local_port),
        public_url: runtime.public_url.clone(),
        detail: format!("{} · path prefix /w/<workspace-id>", runtime.config.tunnel_type),
    }
}

async fn stop_runtime(mut runtime: GatewayRuntime) {
    if let Some(shutdown) = runtime.shutdown.take() {
        let _ = shutdown.send(());
    }
    let _ = runtime.handle.await;
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
                    "全局 Gateway 当前优先支持 Cloudflare Quick；固定域名请使用 FRP 或外部反向代理。独立 Workspace Tunnel 仍保留 Named Cloudflare。".into(),
                ));
            }
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let log = crate::platform::platform().app_config_dir()?.join("global-gateway-cloudflared.log");
            let handle = cloudflare::spawn_cloudflare_tunnel(
                config.local_port,
                &cwd,
                &log,
                "quick",
                "",
                "",
                config.use_proxy,
            ).await?;
            let public_url = handle.public_url.clone();
            Ok((public_url, Some(TunnelChild::Cloudflare { child: handle.child, pid: handle.pid })))
        }
        "frp" => {
            let mut profile = WorkspaceProfile::new(
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")).display().to_string(),
                Some("Global Gateway".into()),
            );
            profile.id = "global-gateway".into();
            profile.runtime.local_port = config.local_port;
            profile.tunnel.tunnel_type = "frp".into();
            profile.tunnel.frp_profile_id = config.frp_profile_id.clone();
            profile.tunnel.frp_server = config.frp_server.clone();
            profile.tunnel.frp_server_port = config.frp_server_port;
            profile.tunnel.frp_subdomain = config.frp_subdomain.clone();
            profile.tunnel.use_proxy = config.use_proxy;
            if profile.tunnel.frp_subdomain.trim().is_empty() {
                return Err(AppError::Message("全局 Gateway FRP 子域名不能为空。".into()));
            }
            let handle = frp::spawn_frpc(
                "global-gateway",
                &[(&profile, TunnelServiceKind::Mcp)],
                settings,
            ).await?;
            let public_url = frp::frp_public_url(&profile, TunnelServiceKind::Mcp, settings);
            Ok((public_url, Some(TunnelChild::Frp { child: handle.child, pid: handle.pid })))
        }
        other => Err(AppError::Message(format!("不支持的全局 Gateway tunnel_type: {other}"))),
    }
}

fn bind_listener(port: u16, allow_lan_access: bool) -> AppResult<tokio::net::TcpListener> {
    let addr = local_network::bind_addr(port, allow_lan_access);
    let listener = std::net::TcpListener::bind(addr)
        .map_err(|error| AppError::Message(format!("全局 Gateway 端口 {port} 绑定失败: {error}")))?;
    listener.set_nonblocking(true)?;
    tokio::net::TcpListener::from_std(listener).map_err(AppError::from)
}

async fn serve(
    listener: tokio::net::TcpListener,
    shutdown: oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let app = Router::new()
        .route("/health", get(|| async { Json(json!({ "ok": true, "service": "global-gateway" })) }))
        .route(
            "/.well-known/oauth-protected-resource/w/{workspace_id}/mcp",
            get(proxy_mcp_protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource/w/{workspace_id}",
            get(proxy_mcp_protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server/w/{workspace_id}",
            get(proxy_mcp_authorization_server_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server/w/{workspace_id}/mcp",
            get(proxy_mcp_authorization_server_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server/w/{workspace_id}/actions",
            get(proxy_actions_authorization_server_metadata),
        )
        .route("/w/{workspace_id}", any(proxy_root))
        .route("/w/{workspace_id}/{*path}", any(proxy_path))
        .with_state(ProxyState { client });
    axum::serve(listener, app)
        .with_graceful_shutdown(async { let _ = shutdown.await; })
        .await?;
    Ok(())
}

async fn proxy_root(
    State(state): State<ProxyState>,
    AxumPath(workspace_id): AxumPath<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    proxy(state, workspace_id, String::new(), method, uri, headers, body).await
}

async fn proxy_path(
    State(state): State<ProxyState>,
    AxumPath((workspace_id, path)): AxumPath<(String, String)>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    proxy(state, workspace_id, path, method, uri, headers, body).await
}

async fn proxy(
    state: ProxyState,
    workspace_id: String,
    path: String,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let profile = match DataStore::read_file(|data| {
        Ok(data.profiles.iter().find(|profile| profile.id == workspace_id).cloned())
    }) {
        Ok(Some(profile)) => profile,
        Ok(None) => return (StatusCode::NOT_FOUND, "workspace not found").into_response(),
        Err(error) => return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    };

    let actions_request = path == "actions" || path.starts_with("actions/");
    if actions_request && !profile.actions.use_global_gateway {
        return (StatusCode::NOT_FOUND, "actions is not routed through global gateway").into_response();
    }
    if !actions_request && !profile.tunnel.use_global_gateway {
        return (StatusCode::NOT_FOUND, "mcp is not routed through global gateway").into_response();
    }

    let (port, upstream_path) = if actions_request {
        let stripped = path.strip_prefix("actions").unwrap_or("").trim_start_matches('/');
        (profile.actions.local_port, format!("/{}", stripped))
    } else {
        (profile.runtime.local_port, format!("/{}", path.trim_start_matches('/')))
    };
    let upstream_path = if upstream_path == "/" { "/".to_string() } else { upstream_path };
    let query = uri.query().map(|value| format!("?{value}")).unwrap_or_default();
    let target = format!("http://127.0.0.1:{port}{upstream_path}{query}");

    let mut request = state.client.request(method, &target).body(body);
    for (name, value) in &headers {
        if !is_hop_header(name.as_str()) && name.as_str() != "host" {
            request = request.header(name, value);
        }
    }
    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => return (StatusCode::BAD_GATEWAY, format!("upstream unavailable: {error}")).into_response(),
    };
    let status = response.status();
    let response_headers = response.headers().clone();
    let mut builder = Response::builder().status(status);
    if let Some(target_headers) = builder.headers_mut() {
        for (name, value) in &response_headers {
            if !is_hop_header(name.as_str()) {
                target_headers.insert(name.clone(), value.clone());
            }
        }
    }
    builder
        .body(Body::from_stream(response.bytes_stream()))
        .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
}

fn is_hop_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection" | "keep-alive" | "proxy-authenticate" | "proxy-authorization"
            | "te" | "trailer" | "transfer-encoding" | "upgrade"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_prefixes_are_stable() {
        assert_eq!(
            workspace_public_base("https://mcp.example.com/", "abc", false),
            "https://mcp.example.com/w/abc"
        );
        assert_eq!(
            workspace_public_base("https://mcp.example.com", "abc", true),
            "https://mcp.example.com/w/abc/actions"
        );
    }
}
