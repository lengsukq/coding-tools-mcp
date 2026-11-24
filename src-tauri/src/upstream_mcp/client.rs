use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::mcp::LATEST_PROTOCOL_VERSION;
use crate::settings::ProxyConfig;

use super::model::{CachedUpstreamTool, UpstreamTransportConfig};
use super::transport_http::{HttpRpcError, HttpTransportConfig, StreamableHttpTransport};
use super::transport_sse::LegacySseTransport;
use super::transport_stdio::{StdioTransport, StdioTransportConfig};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);
const LIST_TOOLS_TIMEOUT: Duration = Duration::from_secs(25);
const MAX_TOOL_PAGES: usize = 20;

#[derive(Debug, Clone)]
pub struct HandshakeInfo {
    pub protocol_version: String,
    pub server_name: String,
    pub server_version: String,
    pub latency_ms: u64,
}

pub struct ResolvedUpstreamSecrets {
    pub env: BTreeMap<String, String>,
    pub headers: BTreeMap<String, String>,
    pub bearer_token: Option<String>,
}

enum ClientTransport {
    Stdio(StdioTransport),
    StreamableHttp(StreamableHttpTransport),
    Sse(LegacySseTransport),
}

pub struct UpstreamClient {
    transport: ClientTransport,
    pub handshake: HandshakeInfo,
}

impl UpstreamClient {
    pub async fn connect(
        transport_cfg: &UpstreamTransportConfig,
        secrets: ResolvedUpstreamSecrets,
        extra_paths: Vec<PathBuf>,
        proxy: ProxyConfig,
        origin_device_id: String,
        on_tools_changed: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, String> {
        let started = Instant::now();
        let transport = match transport_cfg {
            UpstreamTransportConfig::Stdio {
                command,
                args,
                cwd,
                env,
                ..
            } => {
                let mut merged_env = env.clone();
                for (k, v) in secrets.env {
                    merged_env.insert(k, v);
                }
                let stdio = StdioTransport::spawn(
                    StdioTransportConfig {
                        command: command.clone(),
                        args: args.clone(),
                        cwd: cwd.clone(),
                        env: merged_env,
                        extra_paths,
                    },
                    on_tools_changed,
                )
                .await?;
                ClientTransport::Stdio(stdio)
            }
            UpstreamTransportConfig::StreamableHttp {
                url,
                headers,
                use_proxy,
                ..
            } => {
                let mut merged_headers = headers.clone();
                for (k, v) in secrets.headers {
                    merged_headers.insert(k, v);
                }
                let http = StreamableHttpTransport::new(HttpTransportConfig {
                    url: url.clone(),
                    headers: merged_headers,
                    bearer_token: secrets.bearer_token,
                    use_proxy: *use_proxy,
                    proxy,
                    origin_device_id,
                })?;
                ClientTransport::StreamableHttp(http)
            }
            UpstreamTransportConfig::Sse {
                url,
                headers,
                use_proxy,
                ..
            } => {
                let mut merged_headers = headers.clone();
                for (k, v) in secrets.headers {
                    merged_headers.insert(k, v);
                }
                let sse = LegacySseTransport::connect(
                    HttpTransportConfig {
                        url: url.clone(),
                        headers: merged_headers,
                        bearer_token: secrets.bearer_token,
                        use_proxy: *use_proxy,
                        proxy,
                        origin_device_id,
                    },
                    on_tools_changed,
                )
                .await?;
                ClientTransport::Sse(sse)
            }
        };

        let mut client = Self {
            transport,
            handshake: HandshakeInfo {
                protocol_version: LATEST_PROTOCOL_VERSION.to_string(),
                server_name: String::new(),
                server_version: String::new(),
                latency_ms: 0,
            },
        };

        let init_result = client.initialize_handshake(started).await;
        match init_result {
            Ok(info) => {
                client.handshake = info;
                Ok(client)
            }
            Err(err) => {
                client.shutdown().await;
                Err(err)
            }
        }
    }

    async fn initialize_handshake(&self, started: Instant) -> Result<HandshakeInfo, String> {
        let params = json!({
            "protocolVersion": LATEST_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {
                "name": "coding-tools-mcp",
                "title": "Coding Tools MCP Hub",
                "version": env!("CARGO_PKG_VERSION")
            }
        });
        let raw = self
            .rpc_request_raw("initialize", params, HANDSHAKE_TIMEOUT)
            .await
            .map_err(RpcTransportError::into_message)?;
        let result = extract_rpc_result(&raw, "initialize")?;

        let protocol_version = result
            .get("protocolVersion")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or(LATEST_PROTOCOL_VERSION)
            .to_string();
        let server_name = result
            .pointer("/serverInfo/name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let server_version = result
            .pointer("/serverInfo/version")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        if let ClientTransport::StreamableHttp(http) = &self.transport {
            http.set_protocol_version(protocol_version.clone()).await;
        }

        // Send notifications/initialized (best-effort on servers that ignore notifications).
        let _ = self
            .rpc_notification("notifications/initialized", json!({}))
            .await;

        Ok(HandshakeInfo {
            protocol_version,
            server_name,
            server_version,
            latency_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        })
    }

    pub fn is_alive(&self) -> bool {
        match &self.transport {
            ClientTransport::Stdio(t) => t.is_alive(),
            ClientTransport::StreamableHttp(_) => true,
            ClientTransport::Sse(t) => t.is_alive(),
        }
    }

    pub fn is_stdio(&self) -> bool {
        matches!(self.transport, ClientTransport::Stdio(_))
    }

    pub fn stdio_pid(&self) -> Option<u32> {
        match &self.transport {
            ClientTransport::Stdio(t) => t.pid(),
            _ => None,
        }
    }

    pub fn stderr_tail(&self) -> Vec<String> {
        match &self.transport {
            ClientTransport::Stdio(t) => t.stderr_tail(),
            _ => Vec::new(),
        }
    }

    pub async fn list_tools(&self) -> Result<Vec<CachedUpstreamTool>, String> {
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;

        for _page in 0..MAX_TOOL_PAGES {
            let params = match cursor.as_deref() {
                Some(c) => json!({ "cursor": c }),
                None => json!({}),
            };
            let raw = self
                .rpc_request("tools/list", params, LIST_TOOLS_TIMEOUT)
                .await?;
            let result = extract_rpc_result(&raw, "tools/list")?;
            if let Some(items) = result.get("tools").and_then(Value::as_array) {
                for item in items {
                    if let Some(tool) = CachedUpstreamTool::from_mcp_tool(item) {
                        all.push(tool);
                    }
                }
            }
            let next = result
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string);
            if next.is_none() || next == cursor {
                break;
            }
            cursor = next;
        }

        Ok(all)
    }

    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let args = if arguments.is_object() {
            arguments
        } else {
            json!({})
        };
        let params = json!({
            "name": tool_name,
            "arguments": args
        });
        let raw = self.rpc_request("tools/call", params, timeout).await?;
        if let Some(err) = raw.get("error") {
            let msg = err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Upstream MCP returned JSON-RPC error");
            let code = err.get("code").and_then(Value::as_i64).unwrap_or(-32000);
            return Ok(json!({
                "isError": true,
                "content": [{
                    "type": "text",
                    "text": format!("UPSTREAM_RPC_ERROR ({code}): {msg}")
                }]
            }));
        }
        let mut result = raw.get("result").cloned().unwrap_or_else(|| json!({}));
        if !result.is_object() {
            result = json!({
                "content": [{ "type": "text", "text": result.to_string() }],
                "isError": false
            });
        }
        if result.get("isError").is_none() {
            if let Some(obj) = result.as_object_mut() {
                obj.insert("isError".into(), Value::Bool(false));
            }
        }
        if result.get("content").is_none() {
            if let Some(obj) = result.as_object_mut() {
                obj.insert("content".into(), json!([]));
            }
        }
        Ok(result)
    }

    pub async fn shutdown(&self) {
        match &self.transport {
            ClientTransport::Stdio(t) => t.shutdown().await,
            ClientTransport::StreamableHttp(t) => t.shutdown().await,
            ClientTransport::Sse(t) => t.shutdown(),
        }
    }

    async fn rpc_request_raw(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, RpcTransportError> {
        match &self.transport {
            ClientTransport::Stdio(t) => t
                .send_request(method, params, timeout)
                .await
                .map_err(RpcTransportError::Other),
            ClientTransport::StreamableHttp(t) => t
                .send_request(method, params, timeout)
                .await
                .map_err(|err| match err {
                    HttpRpcError::SessionExpired(msg) => RpcTransportError::SessionExpired(msg),
                    HttpRpcError::Other(msg) => RpcTransportError::Other(msg),
                }),
            ClientTransport::Sse(t) => t
                .send_request(method, params, timeout)
                .await
                .map_err(RpcTransportError::Other),
        }
    }

    async fn rpc_request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        match self.rpc_request_raw(method, params.clone(), timeout).await {
            Ok(v) => Ok(v),
            Err(RpcTransportError::SessionExpired(_)) => {
                if let ClientTransport::StreamableHttp(http) = &self.transport {
                    http.clear_session().await;
                    let _ = self.initialize_handshake(Instant::now()).await?;
                    self.rpc_request_raw(method, params, timeout)
                        .await
                        .map_err(RpcTransportError::into_message)
                } else {
                    Err("上游 MCP 会话已过期".into())
                }
            }
            Err(err) => Err(err.into_message()),
        }
    }

    async fn rpc_notification(&self, method: &str, params: Value) -> Result<(), String> {
        match &self.transport {
            ClientTransport::Stdio(t) => t.send_notification(method, params).await,
            ClientTransport::StreamableHttp(t) => t
                .send_notification(method, params, Duration::from_secs(10))
                .await
                .map_err(HttpRpcError::into_message),
            ClientTransport::Sse(t) => t.send_notification(method, params).await,
        }
    }
}

enum RpcTransportError {
    SessionExpired(String),
    Other(String),
}

impl RpcTransportError {
    fn into_message(self) -> String {
        match self {
            Self::SessionExpired(m) | Self::Other(m) => m,
        }
    }
}

fn extract_rpc_result(raw: &Value, method: &str) -> Result<Value, String> {
    if let Some(err) = raw.get("error") {
        let code = err.get("code").and_then(Value::as_i64).unwrap_or(-32000);
        let msg = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown upstream error");
        return Err(format!("上游 MCP `{method}` 返回错误 ({code}): {msg}"));
    }
    raw.get("result")
        .cloned()
        .ok_or_else(|| format!("上游 MCP `{method}` 响应缺少 result 字段"))
}
