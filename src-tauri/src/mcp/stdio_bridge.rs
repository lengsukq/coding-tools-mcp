use std::fs::{self, OpenOptions};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinSet;

use super::gateway::MCP_SESSION_HEADER;

pub const GEMINI_STDIO_CLIENT_ID: &str = "gemini-local-stdio";
const AUTH_FILE_VERSION: u32 = 1;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiStdioAuthFile {
    pub version: u32,
    pub auth_type: String,
    pub endpoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

impl GeminiStdioAuthFile {
    pub fn new_oauth(
        endpoint: String,
        server_url: String,
        access_token: String,
        expires_at: i64,
    ) -> Self {
        Self {
            version: AUTH_FILE_VERSION,
            auth_type: "oauth".into(),
            endpoint,
            access_token: Some(access_token),
            server_url: Some(server_url),
            client_id: Some(GEMINI_STDIO_CLIENT_ID.into()),
            expires_at: Some(expires_at),
        }
    }
}

/// Validate that the stdio bridge only talks to a loopback HTTP `/mcp` endpoint with an explicit port.
pub fn validate_loopback_mcp_endpoint(raw: &str) -> Result<String, String> {
    let parsed =
        reqwest::Url::parse(raw.trim()).map_err(|err| format!("Invalid MCP URL '{raw}': {err}"))?;
    if parsed.scheme() != "http" {
        return Err("Local MCP stdio bridge only accepts http:// loopback endpoints".into());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("Local MCP endpoint must not include credentials".into());
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err("Local MCP endpoint must not include query parameters or fragments".into());
    }
    if parsed.path() != "/mcp" {
        return Err("Local MCP endpoint path must be /mcp".into());
    }
    let port = parsed
        .port()
        .ok_or_else(|| "Local MCP endpoint must specify an explicit port".to_string())?;
    let host = parsed.host_str().unwrap_or("");
    let normalized_host = match host {
        "127.0.0.1" | "localhost" => "127.0.0.1",
        "::1" | "[::1]" => "[::1]",
        _ => return Err("Local MCP endpoint host must be 127.0.0.1, localhost, or [::1]".into()),
    };
    Ok(format!("http://{normalized_host}:{port}/mcp"))
}

pub fn gemini_stdio_bin_path() -> Result<PathBuf, String> {
    #[cfg(unix)]
    {
        let home = dirs::home_dir()
            .ok_or_else(|| "Unable to locate home directory for local MCP binary".to_string())?;
        Ok(home.join(".local").join("bin").join("coding-tools-mcp"))
    }
    #[cfg(not(unix))]
    {
        std::env::current_exe()
            .map_err(|err| format!("Unable to resolve current executable path: {err}"))
    }
}

pub fn gemini_stdio_auth_file_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir()
        .ok_or_else(|| "Unable to locate home directory for local MCP credentials".to_string())?;
    Ok(home
        .join(".local")
        .join("share")
        .join("coding-tools-mcp")
        .join("gemini-stdio-auth.json"))
}

pub fn format_mcp_stdio_command(binary_path: &Path, endpoint: &str) -> String {
    let bin = binary_path.to_string_lossy();
    let quoted_bin = if bin.chars().any(char::is_whitespace) {
        format!("\"{bin}\"")
    } else {
        bin.into_owned()
    };
    format!("{quoted_bin} mcp-stdio {endpoint}")
}

pub fn read_auth_file(path: &Path) -> Result<Option<GeminiStdioAuthFile>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)
        .map_err(|err| format!("Unable to read local MCP credentials file: {err}"))?;
    let parsed = serde_json::from_str::<GeminiStdioAuthFile>(&raw)
        .map_err(|err| format!("Invalid local MCP credentials file: {err}"))?;
    Ok(Some(parsed))
}

pub fn write_private_auth_file(path: &Path, payload: &GeminiStdioAuthFile) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Invalid local MCP credentials file path".to_string())?;
    ensure_private_dir(parent)?;
    let encoded = serde_json::to_string_pretty(payload)
        .map_err(|err| format!("Unable to serialize local MCP credentials: {err}"))?;

    let mut options = OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|err| format!("Unable to write local MCP credentials file: {err}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|err| format!("Unable to lock local MCP credentials permissions: {err}"))?;
    }
    file.write_all(encoded.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|err| format!("Unable to flush local MCP credentials file: {err}"))
}

pub fn remove_auth_file_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("Unable to remove stale MCP credentials file: {err}")),
    }
}

fn ensure_private_dir(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir)
        .map_err(|err| format!("Unable to create local MCP credentials directory: {err}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
            .map_err(|err| format!("Unable to protect local MCP credentials directory: {err}"))?;
    }
    Ok(())
}

/// Run the headless `mcp-stdio` subcommand from `main`/`lib::run` before Tauri GUI initialization.
pub fn run_cli(args: &[String]) -> i32 {
    if args.len() != 1 || matches!(args[0].as_str(), "-h" | "--help") {
        eprintln!("Usage: coding-tools-mcp mcp-stdio <http://127.0.0.1:<port>/mcp>");
        return 2;
    }
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("mcp-stdio failed to start async runtime: {err}");
            return 1;
        }
    };
    let auth_path = gemini_stdio_auth_file_path().ok();
    match runtime.block_on(run_bridge(&args[0], auth_path.as_deref())) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("mcp-stdio error: {err}");
            1
        }
    }
}

pub async fn run_bridge(endpoint: &str, auth_file: Option<&Path>) -> Result<(), String> {
    let endpoint = validate_loopback_mcp_endpoint(endpoint)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|err| format!("Unable to initialize HTTP client: {err}"))?;

    let initial_token = load_bearer_token(auth_file);
    let state = Arc::new(BridgeState {
        client,
        endpoint,
        auth_file: auth_file.map(Path::to_path_buf),
        bearer_token: RwLock::new(initial_token),
        session_id: RwLock::new(None),
    });

    forward_process_stdio(state).await
}

struct BridgeState {
    client: reqwest::Client,
    endpoint: String,
    auth_file: Option<PathBuf>,
    bearer_token: RwLock<Option<String>>,
    session_id: RwLock<Option<String>>,
}

fn load_bearer_token(auth_file: Option<&Path>) -> Option<String> {
    let path = auth_file?;
    match read_auth_file(path) {
        Ok(Some(file)) if file.auth_type == "oauth" => file
            .access_token
            .map(|token| token.trim().to_string())
            .filter(|token| !token.is_empty()),
        Ok(_) => None,
        Err(err) => {
            eprintln!("mcp-stdio warning: {err}");
            None
        }
    }
}

async fn forward_process_stdio(state: Arc<BridgeState>) -> Result<(), String> {
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
    let _ = reader_handle.join();
    bridge_res?;
    writer_res
}

async fn forward_line_channel(
    state: Arc<BridgeState>,
    mut in_rx: mpsc::UnboundedReceiver<Result<String, String>>,
    out_tx: mpsc::UnboundedSender<String>,
) -> Result<(), String> {
    let mut in_flight = JoinSet::new();

    while let Some(item) = in_rx.recv().await {
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

fn adapt_gemini_response(method: Option<&str>, mut payload: Value) -> Value {
    match method {
        Some("tools/list") => {
            compact_tools_list_result(&mut payload);
            payload
        }
        Some("tools/call") if payload.get("error").is_some() => {
            wrap_tool_call_rpc_error_as_mcp_result(payload)
        }
        _ => payload,
    }
}

fn wrap_tool_call_rpc_error_as_mcp_result(payload: Value) -> Value {
    let Some(id) = payload.get("id").filter(|id| !id.is_null()).cloned() else {
        return payload;
    };
    let Some(error) = payload.get("error").and_then(Value::as_object) else {
        return payload;
    };

    let code = error.get("code").and_then(Value::as_i64);
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Tool call failed");
    let reason = error
        .get("data")
        .and_then(|d| d.get("reason"))
        .and_then(Value::as_str);

    let text = match (reason, code) {
        (Some("WORKSPACE_NOT_SELECTED"), _) => format!(
            "WORKSPACE_NOT_SELECTED: {message}\nNext step: call workspace_list and then workspace_select(workspace_id=...), or pass workspace_id directly in this tool's arguments."
        ),
        (Some(reason), _) => format!("{reason}: {message}"),
        (None, Some(code)) => format!("MCP error ({code}): {message}"),
        (None, None) => message.to_string(),
    };

    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "isError": true,
            "content": [{ "type": "text", "text": text }]
        }
    })
}

fn compact_tools_list_result(payload: &mut Value) {
    let Some(tools) = payload
        .get_mut("result")
        .and_then(|r| r.get_mut("tools"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };

    for tool in tools {
        compact_tool_entry(tool);
    }
}

fn compact_tool_entry(tool: &mut Value) {
    let Some(obj) = tool.as_object_mut() else {
        return;
    };
    obj.remove("title");

    if let Some(desc) = obj.get("description").and_then(Value::as_str) {
        let compact = truncate_compact_text(desc, 110);
        if compact.is_empty() {
            obj.remove("description");
        } else {
            obj.insert("description".into(), Value::String(compact));
        }
    }

    if let Some(schema) = obj.get_mut("inputSchema") {
        compact_schema_node(schema, 0);
    }
}

fn compact_schema_node(node: &mut Value, depth: usize) {
    let Some(obj) = node.as_object_mut() else {
        return;
    };

    for noisy_key in [
        "$schema",
        "title",
        "additionalProperties",
        "default",
        "minimum",
        "maximum",
        "minLength",
        "maxLength",
        "minItems",
        "maxItems",
    ] {
        obj.remove(noisy_key);
    }

    if depth > 0 {
        if let Some(desc) = obj.get("description").and_then(Value::as_str) {
            let compact = truncate_compact_text(desc, 48);
            if compact.is_empty() {
                obj.remove("description");
            } else {
                obj.insert("description".into(), Value::String(compact));
            }
        }
    } else {
        obj.remove("description");
    }

    if let Some(props) = obj.get_mut("properties").and_then(Value::as_object_mut) {
        compact_schema_properties(props, depth + 1);
    }

    if let Some(items) = obj.get_mut("items") {
        compact_schema_node(items, depth + 1);
    }

    for combinator in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = obj.get_mut(combinator).and_then(Value::as_array_mut) {
            for branch in branches {
                compact_schema_node(branch, depth + 1);
            }
        }
    }
}

fn compact_schema_properties(props: &mut Map<String, Value>, depth: usize) {
    for (key, prop) in props.iter_mut() {
        if key == "workspace_id" {
            *prop = json!({ "type": "string" });
            continue;
        }
        compact_schema_node(prop, depth);
    }
}

fn truncate_compact_text(text: &str, max_chars: usize) -> String {
    let normalized: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }

    let cutoff = max_chars.saturating_sub(1);
    let end_byte = normalized
        .char_indices()
        .nth(cutoff)
        .map(|(idx, _)| idx)
        .unwrap_or(normalized.len());
    let prefix = normalized[..end_byte].trim_end_matches(|c: char| c.is_whitespace() || c == ',');
    format!("{prefix}…")
}

fn extract_request_id(message: &Value) -> Option<Value> {
    let id = message.get("id")?;
    if id.is_null() {
        None
    } else {
        Some(id.clone())
    }
}

async fn post_with_auth_retry(state: &BridgeState, message: &Value) -> Result<Vec<Value>, String> {
    let current_token = state.bearer_token.read().await.clone();
    let response = send_mcp_post(state, message, current_token.as_deref()).await?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        let refreshed_token = load_bearer_token(state.auth_file.as_deref());
        if refreshed_token.is_some() && refreshed_token != current_token {
            *state.bearer_token.write().await = refreshed_token.clone();
            let retry_response =
                send_mcp_post(state, message, refreshed_token.as_deref()).await?;
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

fn extract_jsonrpc_messages(content_type: &str, body: &str) -> Result<Vec<Value>, String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    if content_type.starts_with("text/event-stream") {
        let mut messages = Vec::new();
        for data_chunk in parse_sse_data_events(body) {
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

fn parse_sse_data_events(body: &str) -> Vec<String> {
    let mut events = Vec::new();
    let mut current_lines = Vec::new();

    for line in body.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            if !current_lines.is_empty() {
                events.push(current_lines.join("\n"));
                current_lines.clear();
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            current_lines.push(rest.trim_start().to_string());
        }
    }
    if !current_lines.is_empty() {
        events.push(current_lines.join("\n"));
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::post;
    use axum::{Json, Router};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn validate_loopback_endpoint_accepts_and_normalizes_loopback_urls() {
        assert_eq!(
            validate_loopback_mcp_endpoint(" http://localhost:28765/mcp ").unwrap(),
            "http://127.0.0.1:28765/mcp"
        );
        assert_eq!(
            validate_loopback_mcp_endpoint("http://127.0.0.1:29000/mcp").unwrap(),
            "http://127.0.0.1:29000/mcp"
        );
        assert_eq!(
            validate_loopback_mcp_endpoint("http://[::1]:28765/mcp").unwrap(),
            "http://[::1]:28765/mcp"
        );
    }

    #[test]
    fn validate_loopback_endpoint_rejects_remote_or_malformed_urls() {
        for bad in [
            "https://127.0.0.1:28765/mcp",
            "http://example.com:28765/mcp",
            "http://127.0.0.1/mcp",
            "http://127.0.0.1:28765/other",
            "http://127.0.0.1:28765/mcp?foo=bar",
            "http://user:pass@127.0.0.1:28765/mcp",
        ] {
            assert!(
                validate_loopback_mcp_endpoint(bad).is_err(),
                "expected error for {bad}"
            );
        }
    }

    #[test]
    fn format_command_quotes_only_when_binary_has_whitespace() {
        assert_eq!(
            format_mcp_stdio_command(
                Path::new("/Users/leo/.local/bin/coding-tools-mcp"),
                "http://127.0.0.1:28765/mcp"
            ),
            "/Users/leo/.local/bin/coding-tools-mcp mcp-stdio http://127.0.0.1:28765/mcp"
        );
        assert_eq!(
            format_mcp_stdio_command(
                Path::new("C:\\Program Files\\Coding Tools\\coding-tools-mcp.exe"),
                "http://127.0.0.1:28765/mcp"
            ),
            "\"C:\\Program Files\\Coding Tools\\coding-tools-mcp.exe\" mcp-stdio http://127.0.0.1:28765/mcp"
        );
    }

    #[test]
    fn auth_file_roundtrips_with_private_permissions() {
        let dir = std::env::temp_dir().join(format!("ctm-stdio-auth-{}", uuid::Uuid::new_v4()));
        let auth_path = dir.join("gemini-stdio-auth.json");
        let payload = GeminiStdioAuthFile::new_oauth(
            "http://127.0.0.1:28765/mcp".into(),
            "https://mcp.example.com".into(),
            "secret.jwt.token".into(),
            1_800_000_000,
        );
        write_private_auth_file(&auth_path, &payload).expect("write auth file");
        let loaded = read_auth_file(&auth_path)
            .expect("read auth file")
            .expect("auth file present");
        assert_eq!(loaded, payload);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&auth_path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        remove_auth_file_if_exists(&auth_path).expect("remove auth file");
        assert!(!auth_path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_jsonrpc_handles_null_empty_json_and_sse() {
        assert!(extract_jsonrpc_messages("application/json", "null")
            .unwrap()
            .is_empty());
        assert!(extract_jsonrpc_messages("application/json", "")
            .unwrap()
            .is_empty());
        let json_msgs =
            extract_jsonrpc_messages("application/json", r#"{"jsonrpc":"2.0","id":1,"result":{}}"#)
                .unwrap();
        assert_eq!(json_msgs.len(), 1);

        let sse = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"tools\":[]}}\n\ndata: [DONE]\n\n";
        let sse_msgs = extract_jsonrpc_messages("text/event-stream; charset=utf-8", sse).unwrap();
        assert_eq!(sse_msgs.len(), 1);
        assert_eq!(sse_msgs[0]["id"], json!(2));
    }

    #[tokio::test]
    async fn bridge_forwards_initialize_session_notifications_and_auth() {
        #[derive(Clone)]
        struct MockState {
            expected_token: String,
            session_id: String,
            seen_session_on_list: Arc<AtomicUsize>,
        }

        async fn mock_mcp(
            State(state): State<MockState>,
            headers: HeaderMap,
            Json(body): Json<Value>,
        ) -> impl IntoResponse {
            let auth = headers
                .get(AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if auth != format!("Bearer {}", state.expected_token) {
                return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
            }

            let method = body.get("method").and_then(Value::as_str).unwrap_or("");
            match method {
                "initialize" => {
                    let mut resp = Json(json!({
                        "jsonrpc": "2.0",
                        "id": body["id"],
                        "result": { "protocolVersion": "2025-03-26" }
                    }))
                    .into_response();
                    resp.headers_mut()
                        .insert(MCP_SESSION_HEADER, state.session_id.parse().unwrap());
                    resp
                }
                "notifications/initialized" => Json(Value::Null).into_response(),
                "tools/list" => {
                    if headers
                        .get(MCP_SESSION_HEADER)
                        .and_then(|v| v.to_str().ok())
                        == Some(state.session_id.as_str())
                    {
                        state.seen_session_on_list.fetch_add(1, Ordering::SeqCst);
                    }
                    Json(json!({
                        "jsonrpc": "2.0",
                        "id": body["id"],
                        "result": { "tools": [{"name": "workspace_list"}] }
                    }))
                    .into_response()
                }
                _ => Json(json!({"jsonrpc": "2.0", "id": body["id"], "result": {}})).into_response(),
            }
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen_session_on_list = Arc::new(AtomicUsize::new(0));
        let mock_state = MockState {
            expected_token: "test-jwt-token".into(),
            session_id: "sess-123".into(),
            seen_session_on_list: Arc::clone(&seen_session_on_list),
        };
        let app = Router::new()
            .route("/mcp", post(mock_mcp))
            .with_state(mock_state);
        let server_task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let dir = std::env::temp_dir().join(format!("ctm-bridge-test-{}", uuid::Uuid::new_v4()));
        let auth_path = dir.join("gemini-stdio-auth.json");
        write_private_auth_file(
            &auth_path,
            &GeminiStdioAuthFile::new_oauth(
                format!("http://127.0.0.1:{port}/mcp"),
                format!("http://127.0.0.1:{port}"),
                "test-jwt-token".into(),
                1_900_000_000,
            ),
        )
        .unwrap();

        let (in_tx, in_rx) = mpsc::unbounded_channel();
        let (out_tx, mut out_rx) = mpsc::unbounded_channel();

        let bridge_state = Arc::new(BridgeState {
            client: reqwest::Client::builder().no_proxy().build().unwrap(),
            endpoint: format!("http://127.0.0.1:{port}/mcp"),
            auth_file: Some(auth_path.clone()),
            bearer_token: RwLock::new(load_bearer_token(Some(&auth_path))),
            session_id: RwLock::new(None),
        });

        for line in [
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
        ] {
            in_tx.send(Ok(line.to_string())).unwrap();
        }
        drop(in_tx);

        forward_line_channel(bridge_state, in_rx, out_tx)
            .await
            .unwrap();
        let mut output_lines = Vec::new();
        while let Some(line) = out_rx.recv().await {
            output_lines.push(serde_json::from_str::<Value>(&line).expect("valid json line"));
        }

        // Notification `Value::Null` response must NOT be printed to stdout.
        assert_eq!(output_lines.len(), 2);
        assert_eq!(output_lines[0]["id"], json!(1));
        assert_eq!(output_lines[1]["id"], json!(2));
        assert_eq!(output_lines[1]["result"]["tools"][0]["name"], json!("workspace_list"));
        assert_eq!(seen_session_on_list.load(Ordering::SeqCst), 1);

        server_task.abort();
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn bridge_returns_jsonrpc_error_on_unauthorized_without_hanging() {
        async fn always_unauthorized() -> impl IntoResponse {
            (StatusCode::UNAUTHORIZED, "Unauthorized")
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let app = Router::new().route("/mcp", post(always_unauthorized));
        let server_task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let (in_tx, in_rx) = mpsc::unbounded_channel();
        let (out_tx, mut out_rx) = mpsc::unbounded_channel();

        let bridge_state = Arc::new(BridgeState {
            client: reqwest::Client::builder().no_proxy().build().unwrap(),
            endpoint: format!("http://127.0.0.1:{port}/mcp"),
            auth_file: None,
            bearer_token: RwLock::new(None),
            session_id: RwLock::new(None),
        });

        in_tx
            .send(Ok(
                r#"{"jsonrpc":"2.0","id":7,"method":"initialize"}"#.to_string(),
            ))
            .unwrap();
        drop(in_tx);

        forward_line_channel(bridge_state, in_rx, out_tx)
            .await
            .unwrap();
        let raw_out = out_rx.recv().await.expect("error line");
        let parsed: Value = serde_json::from_str(raw_out.trim()).unwrap();
        assert_eq!(parsed["id"], json!(7));
        assert_eq!(parsed["error"]["code"], json!(-32000));
        assert!(parsed["error"]["message"]
            .as_str()
            .unwrap()
            .contains("HTTP 401"));

        server_task.abort();
    }

    #[test]
    fn adapt_gemini_response_compacts_tools_list_without_dropping_schema() {
        let raw = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "tools": [
                    {
                        "name": "exec_command",
                        "title": "Exec Command",
                        "description": "Execute a single whitelisted CLI command inside the active workspace without shell operators like && or | and inspect its stdout/stderr carefully before acting.",
                        "annotations": { "destructiveHint": true },
                        "inputSchema": {
                            "$schema": "http://json-schema.org/draft-07/schema#",
                            "title": "ExecParams",
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["cmd"],
                            "properties": {
                                "workspace_id": {
                                    "type": "string",
                                    "description": "Optional workspace ID returned by workspace_list or workspace_select when no session workspace has been selected yet or when targeting another workspace explicitly."
                                },
                                "cmd": {
                                    "type": "string",
                                    "minLength": 1,
                                    "description": "Single command to run inside the workspace root directory without chaining operators"
                                },
                                "mode": {
                                    "type": "string",
                                    "enum": ["fast", "full"],
                                    "default": "fast"
                                }
                            }
                        }
                    }
                ]
            }
        });

        let raw_len = raw.to_string().len();
        let adapted = adapt_gemini_response(Some("tools/list"), raw);
        let adapted_len = adapted.to_string().len();
        assert!(adapted_len < raw_len);

        let tool = &adapted["result"]["tools"][0];
        assert!(tool.get("title").is_none());
        assert_eq!(tool["annotations"]["destructiveHint"], json!(true));
        assert!(tool["description"].as_str().unwrap().chars().count() <= 110);
        assert_eq!(tool["inputSchema"]["required"], json!(["cmd"]));
        assert_eq!(
            tool["inputSchema"]["properties"]["workspace_id"],
            json!({ "type": "string" })
        );
        assert_eq!(
            tool["inputSchema"]["properties"]["mode"],
            json!({ "type": "string", "enum": ["fast", "full"] })
        );
        assert!(tool["inputSchema"]["properties"]["cmd"]["description"]
            .as_str()
            .unwrap()
            .chars()
            .count()
            <= 48);
    }

    #[test]
    fn adapt_gemini_response_converts_tool_call_rpc_error_into_mcp_is_error_result() {
        let rpc_err = json!({
            "jsonrpc": "2.0",
            "id": 9,
            "error": {
                "code": -32602,
                "message": "No active workspace selected for this session.",
                "data": { "reason": "WORKSPACE_NOT_SELECTED", "retryable": false }
            }
        });

        let adapted = adapt_gemini_response(Some("tools/call"), rpc_err.clone());
        assert!(adapted.get("error").is_none());
        assert_eq!(adapted["id"], json!(9));
        assert_eq!(adapted["result"]["isError"], json!(true));
        let text = adapted["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("WORKSPACE_NOT_SELECTED"));
        assert!(text.contains("workspace_list"));
        assert!(text.contains("workspace_select"));

        // Non-tools/call RPC errors stay standard JSON-RPC errors.
        let untouched = adapt_gemini_response(Some("resources/read"), rpc_err);
        assert_eq!(untouched["error"]["code"], json!(-32602));
    }
}
