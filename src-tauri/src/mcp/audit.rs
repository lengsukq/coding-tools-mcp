use serde_json::Value;

use crate::mcp::server::SharedState;
use crate::tunnel::append_profile_log;

#[derive(Debug, Clone)]
pub(super) struct RpcRequestMeta {
    pub request_id: Value,
    pub method: String,
    pub tool_name: String,
    pub request_bytes: usize,
}

impl RpcRequestMeta {
    pub fn from_body(body: &Value) -> Self {
        Self {
            request_id: body.get("id").cloned().unwrap_or(Value::Null),
            method: body
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            tool_name: body
                .get("params")
                .and_then(|params| params.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            request_bytes: serde_json::to_vec(body)
                .map(|bytes| bytes.len())
                .unwrap_or_default(),
        }
    }

    pub fn is_tool_call(&self) -> bool {
        self.method == "tools/call"
    }
}

pub(super) fn log_request(profile_id: &str, session_id: Option<&str>, meta: &RpcRequestMeta) {
    append_profile_log(
        profile_id,
        "mcp-requests.log",
        &format!(
            "[rpc] request session={} id={} method={} tool={}",
            session_id.unwrap_or("none"),
            meta.request_id,
            meta.method,
            meta.tool_name
        ),
    );
}

pub(super) fn record_response(
    state: &SharedState,
    profile_id: &str,
    session_id: Option<&str>,
    meta: &RpcRequestMeta,
    response: &Value,
) {
    let response_bytes = serialized_len(response);
    let is_error = response.get("error").is_some()
        || response
            .get("result")
            .and_then(|result| result.get("isError"))
            .and_then(Value::as_bool)
            .unwrap_or(false);

    state.usage().record(
        meta.request_bytes,
        response_bytes,
        meta.is_tool_call(),
        is_error,
    );
    log_rpc_completion(state, profile_id, session_id, meta, response_bytes);
    log_exec_result(profile_id, session_id, meta, response);
}

fn serialized_len(value: &Value) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or_default()
}

fn log_rpc_completion(
    state: &SharedState,
    profile_id: &str,
    session_id: Option<&str>,
    meta: &RpcRequestMeta,
    response_bytes: usize,
) {
    let audit = state.context_audit_snapshot();
    let repeated_bytes = audit
        .get("repeated_bytes")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    append_profile_log(
        profile_id,
        "mcp-requests.log",
        &format!(
            "[rpc] completed session={} id={} method={} tool={} response_bytes={} repeated_bytes={}",
            session_id.unwrap_or("none"), meta.request_id, meta.method, meta.tool_name, response_bytes, repeated_bytes
        ),
    );

    if let Some(block) = audit
        .get("blocks")
        .and_then(Value::as_array)
        .and_then(|blocks| blocks.last())
    {
        append_profile_log(
            profile_id,
            "mcp-requests.log",
            &format!(
                "[context-audit] kind={} bytes={} hash={} repeated={} total_bytes={} repeated_bytes={}",
                block.get("kind").and_then(Value::as_str).unwrap_or("unknown"),
                block.get("bytes").and_then(Value::as_u64).unwrap_or_default(),
                block.get("hash").and_then(Value::as_str).unwrap_or("unknown"),
                block.get("repeated").and_then(Value::as_bool).unwrap_or(false),
                audit.get("total_bytes").and_then(Value::as_u64).unwrap_or_default(),
                repeated_bytes
            ),
        );
    }
}

fn log_exec_result(
    profile_id: &str,
    session_id: Option<&str>,
    meta: &RpcRequestMeta,
    response: &Value,
) {
    if !matches!(
        meta.tool_name.as_str(),
        "exec_command" | "exec_health_check"
    ) {
        return;
    }
    let structured = response
        .get("result")
        .and_then(|result| result.get("structuredContent"));
    let status = structured
        .and_then(|value| value.get("status"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let termination_reason = structured
        .and_then(|value| value.get("termination_reason"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let exit_code = structured
        .and_then(|value| value.get("exit_code"))
        .map(Value::to_string)
        .unwrap_or_default();
    let is_error = response
        .get("result")
        .and_then(|result| result.get("isError"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    append_profile_log(
        profile_id,
        "mcp-requests.log",
        &format!(
            "[exec] session={} id={} tool={} is_error={} status={} termination_reason={} exit_code={}",
            session_id.unwrap_or("none"), meta.request_id, meta.tool_name, is_error, status, termination_reason, exit_code
        ),
    );
}
