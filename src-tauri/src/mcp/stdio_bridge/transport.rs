use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinSet;

use super::auth::load_bearer_token;
use super::compact::adapt_gemini_response;
use super::MCP_SESSION_HEADER;

const MAX_IN_FLIGHT_REQUESTS: usize = 32;

pub(super) struct BridgeState {
    pub(super) client: reqwest::Client,
    pub(super) endpoint: String,
    pub(super) auth_file: Option<PathBuf>,
    pub(super) bearer_token: RwLock<Option<String>>,
    pub(super) session_id: RwLock<Option<String>>,
}

pub(super) async fn forward_process_stdio(state: Arc<BridgeState>) -> Result<(), String> {
    let (in_tx, in_rx) = mpsc::unbounded_channel::<Result<String, String>>();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();

    let reader_handle = std::thread::Builder::new()
        .name("mcp-stdio-stdin".into())
        .spawn(move || {
            let stdin = std::io::stdin();
            let locked = stdin.lock();
            for line_result in locked.lines() {
                match line_result {
                    Ok(line) => {
                        if in_tx.send(Ok(line)).is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        let _ = in_tx.send(Err(format!("stdin read error: {err}")));
                        break;
                    }
                }
            }
        })
        .map_err(|err| format!("Unable to start stdin reader thread: {err}"))?;

    let writer_handle = std::thread::Builder::new()
        .name("mcp-stdio-stdout".into())
        .spawn(move || -> Result<(), String> {
            let stdout = std::io::stdout();
            let mut locked = stdout.lock();
            while let Some(line) = out_rx.blocking_recv() {
                locked
                    .write_all(line.as_bytes())
                    .and_then(|_| locked.write_all(b"\n"))
                    .and_then(|_| locked.flush())
                    .map_err(|err| format!("stdout write error: {err}"))?;
            }
            Ok(())
        })
        .map_err(|err| format!("Unable to start stdout writer thread: {err}"))?;

    let bridge_res = forward_line_channel(state, in_rx, out_tx).await;
    let writer_res = writer_handle
        .join()
        .map_err(|_| "stdout writer thread panicked".to_string())?;
    // Only join the blocking stdin reader when the channel drained to EOF cleanly;
    // on an early bridge error, stdin may still be blocked waiting for input.
    if bridge_res.is_ok() {
        let _ = reader_handle.join();
    }
    bridge_res?;
    writer_res
}

pub(super) async fn forward_line_channel(
    state: Arc<BridgeState>,
    mut in_rx: mpsc::UnboundedReceiver<Result<String, String>>,
    out_tx: mpsc::UnboundedSender<String>,
) -> Result<(), String> {
    let mut in_flight = JoinSet::new();

    while let Some(item) = in_rx.recv().await {
        while let Some(res) = in_flight.try_join_next() {
            if let Err(err) = res {
                eprintln!("mcp-stdio request task panicked: {err}");
            }
        }
        let raw_line = item?;
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(trimmed) {
            Ok(val) => val,
            Err(err) => {
                eprintln!("mcp-stdio invalid JSON-RPC line: {err}");
                let _ = out_tx.send(
                    json!({
                        "jsonrpc": "2.0",
                        "id": Value::Null,
                        "error": { "code": -32700, "message": format!("Parse error: {err}") }
                    })
                    .to_string(),
                );
                continue;
            }
        };

        // Run `initialize` synchronously before subsequent stdin lines so `Mcp-Session-Id`
        // is captured even when a client pipelines `notifications/initialized` right behind it.
        if is_initialize_request(&message) {
            process_message(Arc::clone(&state), message, out_tx.clone()).await;
        } else {
            while in_flight.len() >= MAX_IN_FLIGHT_REQUESTS {
                if let Some(Err(err)) = in_flight.join_next().await {
                    eprintln!("mcp-stdio request task panicked: {err}");
                }
            }
            let state_ref = Arc::clone(&state);
            let tx = out_tx.clone();
            in_flight.spawn(async move {
                process_message(state_ref, message, tx).await;
            });
        }
    }

    while let Some(res) = in_flight.join_next().await {
        if let Err(err) = res {
            eprintln!("mcp-stdio request task panicked: {err}");
        }
    }
    Ok(())
}

fn extract_request_method(message: &Value) -> Option<&str> {
    message.get("method").and_then(Value::as_str)
}

fn is_initialize_request(message: &Value) -> bool {
    extract_request_method(message) == Some("initialize")
}

fn extract_request_id(message: &Value) -> Option<Value> {
    let id = message.get("id")?;
    if id.is_null() {
        None
    } else {
        Some(id.clone())
    }
}

async fn process_message(
    state: Arc<BridgeState>,
    message: Value,
    out_tx: mpsc::UnboundedSender<String>,
) {
    let request_id = extract_request_id(&message);
    let request_method = extract_request_method(&message).map(str::to_string);
    let response = post_with_auth_retry(&state, &message).await;
    match response {
        Ok(messages) => {
            for payload in messages {
                if payload.is_null() {
                    continue;
                }
                let adapted = adapt_gemini_response(request_method.as_deref(), payload);
                let _ = out_tx.send(adapted.to_string());
            }
        }
        Err(err_message) => {
            eprintln!("mcp-stdio bridge request failed: {err_message}");
            if let Some(id) = request_id {
                let _ = out_tx.send(
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32000, "message": err_message }
                    })
                    .to_string(),
                );
            }
        }
    }
}

async fn post_with_auth_retry(state: &BridgeState, message: &Value) -> Result<Vec<Value>, String> {
    let current_token = state.bearer_token.read().await.clone();
    let response = send_mcp_post(state, message, current_token.as_deref()).await?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        let refreshed_token = load_bearer_token(state.auth_file.as_deref());
        if refreshed_token.is_some() && refreshed_token != current_token {
            *state.bearer_token.write().await = refreshed_token.clone();
            let retry_response = send_mcp_post(state, message, refreshed_token.as_deref()).await?;
            return decode_mcp_http_response(state, retry_response).await;
        }
        return Err(format!(
            "Authentication failed (HTTP 401) at {}. Click \"Prepare Local Bridge\" in Coding Tools MCP and retry.",
            state.endpoint
        ));
    }

    decode_mcp_http_response(state, response).await
}

async fn send_mcp_post(
    state: &BridgeState,
    message: &Value,
    bearer_token: Option<&str>,
) -> Result<reqwest::Response, String> {
    let session = state.session_id.read().await.clone();
    let mut request = state
        .client
        .post(&state.endpoint)
        .header(ACCEPT, "application/json, text/event-stream")
        .header(CONTENT_TYPE, "application/json")
        .json(message);

    if let Some(token) = bearer_token {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    if let Some(session_id) = session {
        request = request.header(MCP_SESSION_HEADER, session_id);
    }

    request.send().await.map_err(|err| {
        format!(
            "Unable to reach Coding Tools MCP at {} (ensure Global MCP is running): {err}",
            state.endpoint
        )
    })
}

async fn decode_mcp_http_response(
    state: &BridgeState,
    response: reqwest::Response,
) -> Result<Vec<Value>, String> {
    if let Some(session_value) = response
        .headers()
        .get(MCP_SESSION_HEADER)
        .and_then(|val| val.to_str().ok())
        .map(str::trim)
        .filter(|val| !val.is_empty())
    {
        *state.session_id.write().await = Some(session_value.to_string());
    }

    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|val| val.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();

    let body = response
        .text()
        .await
        .map_err(|err| format!("Failed reading MCP HTTP response body: {err}"))?;

    if !status.is_success() {
        let preview = body.trim();
        let detail = if preview.is_empty() {
            status.to_string()
        } else {
            format!("{status}: {preview}")
        };
        return Err(format!("MCP server returned {detail}"));
    }

    extract_jsonrpc_messages(&content_type, &body)
}

pub(super) fn extract_jsonrpc_messages(
    content_type: &str,
    body: &str,
) -> Result<Vec<Value>, String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    if content_type.starts_with("text/event-stream") {
        let mut messages = Vec::new();
        for data_chunk in crate::mcp::sse::parse_sse_data_events(body) {
            if data_chunk.is_empty() || data_chunk == "[DONE]" {
                continue;
            }
            let value: Value = serde_json::from_str(&data_chunk)
                .map_err(|err| format!("Invalid SSE JSON-RPC payload: {err}"))?;
            if !value.is_null() {
                messages.push(value);
            }
        }
        return Ok(messages);
    }

    let value: Value =
        serde_json::from_str(trimmed).map_err(|err| format!("Invalid JSON-RPC response: {err}"))?;
    if value.is_null() {
        Ok(Vec::new())
    } else {
        Ok(vec![value])
    }
}
