use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use reqwest::header::{HeaderMap, ACCEPT, CONTENT_TYPE};
use serde_json::{json, Value};
use tokio::sync::oneshot;

use super::transport_http::{build_http_client, build_static_headers, HttpTransportConfig};

const SSE_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(12);

type PendingMap = Arc<StdMutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

/// Legacy MCP SSE (`2024-11-05`) client:
/// opens a persistent `GET <sse_url>` stream, waits for `event: endpoint` to obtain
/// the message POST target, sends JSON-RPC requests via `POST`, and routes `event: message`
/// responses back by JSON-RPC `id`.
pub struct LegacySseTransport {
    client: reqwest::Client,
    message_endpoint: String,
    headers: HeaderMap,
    pending: PendingMap,
    next_id: AtomicU64,
    closed: Arc<AtomicBool>,
    stream_task: tokio::task::JoinHandle<()>,
}

impl LegacySseTransport {
    pub async fn connect(
        config: HttpTransportConfig,
        on_tools_changed: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, String> {
        let base_url = reqwest::Url::parse(config.url.trim())
            .map_err(|err| format!("无效的 SSE URL `{}`: {err}", config.url))?;
        let client = build_http_client(config.use_proxy, &config.proxy)?;
        let headers = build_static_headers(
            &config.headers,
            config.bearer_token.as_deref(),
            &config.origin_device_id,
        )?;

        let response = client
            .get(base_url.clone())
            .headers(headers.clone())
            .header(ACCEPT, "text/event-stream")
            .send()
            .await
            .map_err(|err| format!("连接 Legacy SSE MCP `{}` 失败: {err}", config.url))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let preview = crate::util::text::truncate_utf8(body.trim(), 240);
            return Err(format!("Legacy SSE 握手返回状态 {status}: {preview}"));
        }

        let pending: PendingMap = Arc::new(StdMutex::new(HashMap::new()));
        let closed = Arc::new(AtomicBool::new(false));
        let (endpoint_tx, endpoint_rx) = oneshot::channel::<Result<String, String>>();

        let pending_task = Arc::clone(&pending);
        let closed_task = Arc::clone(&closed);
        let base_for_task = base_url.clone();
        let stream_task = tokio::spawn(async move {
            run_sse_reader(
                response,
                base_for_task,
                endpoint_tx,
                pending_task,
                closed_task,
                on_tools_changed,
            )
            .await;
        });

        let endpoint_result = match tokio::time::timeout(SSE_HANDSHAKE_TIMEOUT, endpoint_rx).await {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => Err("Legacy SSE 流在返回 endpoint 事件前断开".into()),
            Err(_) => Err(format!(
                "等待 Legacy SSE `endpoint` 事件超时（>{} 秒）",
                SSE_HANDSHAKE_TIMEOUT.as_secs()
            )),
        };

        let message_endpoint = match endpoint_result {
            Ok(ep) => ep,
            Err(err) => {
                stream_task.abort();
                return Err(err);
            }
        };

        Ok(Self {
            client,
            message_endpoint,
            headers,
            pending,
            next_id: AtomicU64::new(1),
            closed,
            stream_task,
        })
    }

    pub fn is_alive(&self) -> bool {
        !self.closed.load(Ordering::SeqCst)
    }

    pub async fn send_request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        if !self.is_alive() {
            return Err("Legacy SSE 长连接已断开".into());
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, tx);

        let post_res = self
            .client
            .post(&self.message_endpoint)
            .headers(self.headers.clone())
            .header(CONTENT_TYPE, "application/json")
            .timeout(timeout)
            .json(&payload)
            .send()
            .await;

        let response = match post_res {
            Ok(resp) => resp,
            Err(err) => {
                self.pending
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&id);
                return Err(format!(
                    "Legacy SSE POST `{}` 失败: {err}",
                    self.message_endpoint
                ));
            }
        };

        let status = response.status();
        if !status.is_success() {
            self.pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id);
            let body = response.text().await.unwrap_or_default();
            let preview = crate::util::text::truncate_utf8(body.trim(), 240);
            return Err(format!("Legacy SSE POST 返回状态 {status}: {preview}"));
        }

        // Some hybrid servers reply synchronously with JSON on POST while others push on SSE.
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        if content_type.contains("application/json") {
            if let Ok(text) = response.text().await {
                if let Ok(val) = serde_json::from_str::<Value>(text.trim()) {
                    if val.get("id").and_then(Value::as_u64) == Some(id) {
                        self.pending
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .remove(&id);
                        return Ok(val);
                    }
                }
            }
        }

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("Legacy SSE 连接在收到响应前断开".into()),
            Err(_) => {
                self.pending
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&id);
                Err(format!(
                    "Legacy SSE 调用 `{method}` 超时（>{} 秒）",
                    timeout.as_secs()
                ))
            }
        }
    }

    pub async fn send_notification(&self, method: &str, params: Value) -> Result<(), String> {
        if !self.is_alive() {
            return Err("Legacy SSE 长连接已断开".into());
        }
        let payload = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        let resp = self
            .client
            .post(&self.message_endpoint)
            .headers(self.headers.clone())
            .header(CONTENT_TYPE, "application/json")
            .timeout(Duration::from_secs(10))
            .json(&payload)
            .send()
            .await
            .map_err(|err| format!("发送 Legacy SSE 通知失败: {err}"))?;
        if !resp.status().is_success() {
            return Err(format!("Legacy SSE 通知返回异常状态 {}", resp.status()));
        }
        Ok(())
    }

    pub fn shutdown(&self) {
        self.closed.store(true, Ordering::SeqCst);
        fail_all_pending(&self.pending, "Legacy SSE 连接已主动关闭");
        self.stream_task.abort();
    }
}

impl Drop for LegacySseTransport {
    fn drop(&mut self) {
        self.shutdown();
    }
}

async fn run_sse_reader(
    mut response: reqwest::Response,
    base_url: reqwest::Url,
    endpoint_tx: oneshot::Sender<Result<String, String>>,
    pending: PendingMap,
    closed: Arc<AtomicBool>,
    on_tools_changed: Arc<dyn Fn() + Send + Sync>,
) {
    let mut endpoint_tx = Some(endpoint_tx);
    let mut buffer = String::new();
    let mut current_event = String::new();
    let mut current_data: Vec<String> = Vec::new();

    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                let text = String::from_utf8_lossy(&chunk);
                buffer.push_str(&text);
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
                    buffer.drain(..=newline_pos);
                    if line.is_empty() {
                        if !current_data.is_empty() {
                            let event_name = if current_event.is_empty() {
                                "message".to_string()
                            } else {
                                std::mem::take(&mut current_event)
                            };
                            let data_payload = current_data.join("\n");
                            current_data.clear();
                            dispatch_sse_event(
                                &event_name,
                                &data_payload,
                                &base_url,
                                &mut endpoint_tx,
                                &pending,
                                &on_tools_changed,
                            );
                        } else {
                            current_event.clear();
                        }
                    } else if let Some(ev) = line.strip_prefix("event:") {
                        current_event = ev.trim().to_string();
                    } else if let Some(data) = line.strip_prefix("data:") {
                        current_data.push(data.trim_start().to_string());
                    }
                }
            }
            Ok(None) => break,
            Err(err) => {
                if let Some(tx) = endpoint_tx.take() {
                    let _ = tx.send(Err(format!("Legacy SSE 读取错误: {err}")));
                }
                break;
            }
        }
    }

    closed.store(true, Ordering::SeqCst);
    if let Some(tx) = endpoint_tx.take() {
        let _ = tx.send(Err("Legacy SSE 流在收到 endpoint 前结束".into()));
    }
    fail_all_pending(&pending, "Legacy SSE 长连接已断开");
}

fn dispatch_sse_event(
    event_name: &str,
    data: &str,
    base_url: &reqwest::Url,
    endpoint_tx: &mut Option<oneshot::Sender<Result<String, String>>>,
    pending: &PendingMap,
    on_tools_changed: &Arc<dyn Fn() + Send + Sync>,
) {
    let trimmed = data.trim();
    if trimmed.is_empty() {
        return;
    }
    if event_name == "endpoint" {
        if let Some(tx) = endpoint_tx.take() {
            let resolved = resolve_endpoint_url(base_url, trimmed);
            let _ = tx.send(resolved);
        }
        return;
    }
    if event_name == "message" {
        let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
            return;
        };
        if msg.get("method").and_then(Value::as_str) == Some("notifications/tools/list_changed") {
            on_tools_changed();
            return;
        }
        if let Some(id_num) = msg.get("id").and_then(Value::as_u64) {
            let tx = pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id_num);
            if let Some(tx) = tx {
                let _ = tx.send(Ok(msg));
            }
        }
    }
}

fn resolve_endpoint_url(base_url: &reqwest::Url, raw_endpoint: &str) -> Result<String, String> {
    if let Ok(abs) = reqwest::Url::parse(raw_endpoint) {
        if matches!(abs.scheme(), "http" | "https") {
            return Ok(abs.to_string());
        }
        return Err(format!(
            "Legacy SSE endpoint 使用不支持的协议: {raw_endpoint}"
        ));
    }
    base_url
        .join(raw_endpoint)
        .map(|u| u.to_string())
        .map_err(|err| format!("无法解析 Legacy SSE endpoint `{raw_endpoint}`: {err}"))
}

fn fail_all_pending(pending: &PendingMap, message: &str) {
    let drained: Vec<_> = pending
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .drain()
        .collect();
    for (_, tx) in drained {
        let _ = tx.send(Err(message.to_string()));
    }
}
