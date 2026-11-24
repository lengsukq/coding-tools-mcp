use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};
use tokio::sync::RwLock;

use crate::mcp::gateway::MCP_SESSION_HEADER;
use crate::settings::ProxyConfig;

use super::model::UPSTREAM_ORIGIN_HEADER;

const MCP_PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";

pub struct HttpTransportConfig {
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub bearer_token: Option<String>,
    pub use_proxy: bool,
    pub proxy: ProxyConfig,
    pub origin_device_id: String,
}

pub struct StreamableHttpTransport {
    client: reqwest::Client,
    url: String,
    headers: HeaderMap,
    session_id: RwLock<Option<String>>,
    protocol_version: RwLock<Option<String>>,
    next_id: AtomicU64,
}

impl StreamableHttpTransport {
    pub fn new(config: HttpTransportConfig) -> Result<Self, String> {
        let client = build_http_client(config.use_proxy, &config.proxy)?;
        let headers = build_static_headers(
            &config.headers,
            config.bearer_token.as_deref(),
            &config.origin_device_id,
        )?;
        Ok(Self {
            client,
            url: config.url,
            headers,
            session_id: RwLock::new(None),
            protocol_version: RwLock::new(None),
            next_id: AtomicU64::new(1),
        })
    }

    pub async fn set_protocol_version(&self, version: String) {
        *self.protocol_version.write().await = Some(version);
    }

    pub async fn clear_session(&self) {
        *self.session_id.write().await = None;
    }

    pub async fn send_request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, HttpRpcError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        let messages = self.post_message(&payload, timeout).await?;
        if let Some(matched) = messages
            .iter()
            .find(|msg| msg.get("id").and_then(Value::as_u64) == Some(id))
        {
            return Ok(matched.clone());
        }
        messages
            .into_iter()
            .find(|msg| msg.get("result").is_some() || msg.get("error").is_some())
            .ok_or_else(|| {
                HttpRpcError::Other(format!(
                    "Streamable HTTP 上游对 `{method}` 返回空响应体（未找到 JSON-RPC 响应）"
                ))
            })
    }

    pub async fn send_notification(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<(), HttpRpcError> {
        let payload = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        let _ = self.post_message(&payload, timeout).await?;
        Ok(())
    }

    pub async fn shutdown(&self) {
        let session = self.session_id.write().await.take();
        let Some(session_id) = session else {
            return;
        };
        let mut req = self
            .client
            .delete(&self.url)
            .headers(self.headers.clone())
            .header(MCP_SESSION_HEADER, session_id)
            .timeout(Duration::from_secs(3));
        if let Some(proto) = self.protocol_version.read().await.clone() {
            req = req.header(MCP_PROTOCOL_VERSION_HEADER, proto);
        }
        let _ = req.send().await;
    }

    async fn post_message(
        &self,
        payload: &Value,
        timeout: Duration,
    ) -> Result<Vec<Value>, HttpRpcError> {
        let mut req = self
            .client
            .post(&self.url)
            .headers(self.headers.clone())
            .header(ACCEPT, "application/json, text/event-stream")
            .header(CONTENT_TYPE, "application/json")
            .timeout(timeout)
            .json(payload);

        if let Some(session_id) = self.session_id.read().await.clone() {
            req = req.header(MCP_SESSION_HEADER, session_id);
        }
        if let Some(proto) = self.protocol_version.read().await.clone() {
            req = req.header(MCP_PROTOCOL_VERSION_HEADER, proto);
        }

        let response = req.send().await.map_err(|err| {
            if err.is_timeout() {
                HttpRpcError::Other(format!(
                    "请求 Streamable HTTP MCP `{}` 超时（>{} 秒）",
                    self.url,
                    timeout.as_secs()
                ))
            } else {
                HttpRpcError::Other(format!(
                    "连接 Streamable HTTP MCP `{}` 失败: {err}",
                    self.url
                ))
            }
        })?;

        if let Some(new_session) = response
            .headers()
            .get(MCP_SESSION_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            *self.session_id.write().await = Some(new_session.to_string());
        }

        let status = response.status();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();

        if status == reqwest::StatusCode::ACCEPTED || status == reqwest::StatusCode::NO_CONTENT {
            return Ok(Vec::new());
        }

        let body = response
            .text()
            .await
            .map_err(|err| HttpRpcError::Other(format!("读取 HTTP MCP 响应体失败: {err}")))?;

        if status == reqwest::StatusCode::NOT_FOUND && self.session_id.read().await.is_some() {
            return Err(HttpRpcError::SessionExpired(format!(
                "上游 MCP 会话已过期（HTTP 404）: {}",
                body.trim()
            )));
        }

        if !status.is_success() {
            let preview = crate::util::text::truncate_utf8(body.trim(), 300);
            let detail = if preview.is_empty() {
                status.to_string()
            } else {
                format!("{status}: {preview}")
            };
            return Err(HttpRpcError::Other(format!(
                "Streamable HTTP MCP 返回异常状态 {detail}"
            )));
        }

        parse_http_jsonrpc_body(&content_type, &body).map_err(HttpRpcError::Other)
    }
}

#[derive(Debug)]
pub enum HttpRpcError {
    SessionExpired(String),
    Other(String),
}

impl HttpRpcError {
    pub fn into_message(self) -> String {
        match self {
            Self::SessionExpired(msg) | Self::Other(msg) => msg,
        }
    }
}

pub(super) fn build_http_client(
    use_proxy: bool,
    proxy: &ProxyConfig,
) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .pool_idle_timeout(Duration::from_secs(60));

    if !use_proxy || proxy.mode == "none" {
        builder = builder.no_proxy();
    } else if proxy.mode == "manual" {
        let proxy_url = proxy.url.trim();
        if !proxy_url.is_empty() {
            match reqwest::Proxy::all(proxy_url) {
                Ok(p) => builder = builder.proxy(p),
                Err(err) => {
                    return Err(format!("无效的代理地址 `{proxy_url}`: {err}"));
                }
            }
        }
    }

    builder
        .build()
        .map_err(|err| format!("初始化 HTTP 客户端失败: {err}"))
}

pub(super) fn build_static_headers(
    headers: &BTreeMap<String, String>,
    bearer_token: Option<&str>,
    origin_device_id: &str,
) -> Result<HeaderMap, String> {
    let mut map = HeaderMap::new();
    for (name, value) in headers {
        let key = name.trim();
        if key.is_empty() {
            continue;
        }
        let header_name = HeaderName::from_bytes(key.as_bytes())
            .map_err(|err| format!("无效的 HTTP Header 名称 `{key}`: {err}"))?;
        let header_val = HeaderValue::from_str(value.trim())
            .map_err(|err| format!("HTTP Header `{key}` 包含非法字符: {err}"))?;
        map.insert(header_name, header_val);
    }
    if let Some(token) = bearer_token.map(str::trim).filter(|v| !v.is_empty()) {
        if !map.contains_key(AUTHORIZATION) {
            let val = HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|err| format!("Bearer Token 包含非法 Header 字符: {err}"))?;
            map.insert(AUTHORIZATION, val);
        }
    }
    if !origin_device_id.trim().is_empty() {
        if let (Ok(name), Ok(val)) = (
            HeaderName::from_bytes(UPSTREAM_ORIGIN_HEADER.as_bytes()),
            HeaderValue::from_str(origin_device_id.trim()),
        ) {
            map.insert(name, val);
        }
    }
    Ok(map)
}

pub(super) fn parse_http_jsonrpc_body(
    content_type: &str,
    body: &str,
) -> Result<Vec<Value>, String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if content_type.starts_with("text/event-stream") {
        let mut messages = Vec::new();
        for chunk in parse_sse_data_chunks(body) {
            if chunk.is_empty() || chunk == "[DONE]" {
                continue;
            }
            let val: Value = serde_json::from_str(&chunk)
                .map_err(|err| format!("解析上游 SSE JSON-RPC 数据失败: {err}"))?;
            if !val.is_null() {
                messages.push(val);
            }
        }
        return Ok(messages);
    }
    let val: Value = serde_json::from_str(trimmed)
        .map_err(|err| format!("解析上游 JSON-RPC 响应失败: {err}"))?;
    if val.is_null() {
        Ok(Vec::new())
    } else if let Some(arr) = val.as_array() {
        Ok(arr.iter().filter(|v| !v.is_null()).cloned().collect())
    } else {
        Ok(vec![val])
    }
}

pub(super) fn parse_sse_data_chunks(body: &str) -> Vec<String> {
    let mut events = Vec::new();
    let mut current = Vec::new();
    for raw_line in body.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() {
            if !current.is_empty() {
                events.push(current.join("\n"));
                current.clear();
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            current.push(rest.trim_start().to_string());
        }
    }
    if !current.is_empty() {
        events.push(current.join("\n"));
    }
    events
}
