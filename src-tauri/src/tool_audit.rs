//! Privacy-preserving local JSONL audit trail for authenticated MCP tool calls.

use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tunnel::log_dir_for_profile;

mod sanitize;
mod store;
#[cfg(test)]
mod tests;

use sanitize::{
    bounded_identifier, bounded_identifier_text, bounded_visible_text, classify_outcome,
    safe_auth_method, safe_change_id, safe_label, unix_time_ms,
};
use store::ToolAuditStore;

pub(crate) const DEFAULT_RETENTION_DAYS: u16 = 30;
const MAX_RETENTION_DAYS: u16 = 365;
const DAY_MS: u64 = 24 * 60 * 60 * 1000;

static RETENTION_DAYS: AtomicU16 = AtomicU16::new(DEFAULT_RETENTION_DAYS);
static STORE: OnceLock<Mutex<ToolAuditStore>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolAuditRecord {
    pub timestamp_ms: u64,
    pub duration_ms: u64,
    pub request_id: Option<String>,
    pub session_id: Option<String>,
    pub workspace_id: Option<String>,
    pub tool_name: String,
    /// Only a tool action enum, never text typed or captured screen content.
    #[serde(default)]
    pub action_name: Option<String>,
    pub wrapper_tool_name: Option<String>,
    pub auth_method: String,
    pub client_id: Option<String>,
    pub outcome: String,
    pub error_category: Option<String>,
    pub request_bytes: u64,
    pub response_bytes: u64,
    pub change_id: Option<String>,
    pub changed_file_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_peer: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolAuditFilter {
    pub workspace_id: Option<String>,
    pub from_ms: Option<u64>,
    pub to_ms: Option<u64>,
    pub session_id: Option<String>,
    pub tool_name: Option<String>,
    pub outcome: Option<String>,
    pub search: Option<String>,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolAuditPage {
    pub records: Vec<ToolAuditRecord>,
    pub total: usize,
    pub error_count: usize,
    pub workspace_count: usize,
    pub offset: usize,
    pub limit: usize,
    pub retention_days: u16,
    pub health_message: Option<String>,
}

pub(crate) struct ToolAuditCall {
    timestamp_ms: u64,
    started: Instant,
    request_id: Option<String>,
    session_id: Option<String>,
    tool_name: String,
    action_name: Option<String>,
    wrapper_tool_name: Option<String>,
    auth_method: String,
    client_id: Option<String>,
    request_bytes: u64,
    execution_host: Option<String>,
    source_peer: Option<String>,
}

impl ToolAuditCall {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        request_id: &Value,
        tool_name: &str,
        session_id: Option<&str>,
        auth_method: &str,
        client_id: Option<&str>,
        request_bytes: usize,
        body: &Value,
    ) -> Self {
        Self::new_with_peer_context(
            request_id,
            tool_name,
            session_id,
            auth_method,
            client_id,
            request_bytes,
            body,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_with_peer_context(
        request_id: &Value,
        tool_name: &str,
        session_id: Option<&str>,
        auth_method: &str,
        client_id: Option<&str>,
        request_bytes: usize,
        body: &Value,
        execution_host: Option<String>,
        source_peer: Option<String>,
    ) -> Self {
        let delegated_tool = if tool_name == "workspace_invoke" {
            body.get("params")
                .and_then(|params| params.get("arguments"))
                .and_then(|arguments| arguments.get("tool"))
                .and_then(Value::as_str)
                .map(safe_label)
        } else {
            None
        };

        Self {
            timestamp_ms: unix_time_ms(),
            started: Instant::now(),
            request_id: bounded_identifier(request_id, 128),
            session_id: session_id.map(|value| bounded_identifier_text(value, 256)),
            tool_name: delegated_tool
                .clone()
                .unwrap_or_else(|| safe_label(tool_name)),
            action_name: tool_name
                .starts_with("computer_")
                .then(|| {
                    body.pointer("/params/arguments/action")
                        .and_then(Value::as_str)
                        .filter(|name| is_auditable_computer_action(name))
                        .map(str::to_string)
                })
                .flatten(),
            wrapper_tool_name: delegated_tool.map(|_| "workspace_invoke".to_string()),
            auth_method: safe_auth_method(auth_method),
            client_id: client_id.map(|value| bounded_visible_text(value, 128)),
            request_bytes: request_bytes as u64,
            execution_host: execution_host.map(|v| bounded_visible_text(&v, 128)),
            source_peer: source_peer.map(|v| bounded_visible_text(&v, 128)),
        }
    }

    pub(crate) fn set_execution_host(&mut self, host: Option<String>) {
        self.execution_host = host.map(|v| bounded_visible_text(&v, 128));
    }

    pub(super) fn finish(self, workspace_id: Option<String>, response: &Value) -> ToolAuditRecord {
        let response_bytes = serde_json::to_vec(response)
            .map(|bytes| bytes.len() as u64)
            .unwrap_or_default();
        let (outcome, error_category) = classify_outcome(response);
        let structured = response
            .get("result")
            .and_then(|result| result.get("structuredContent"));
        let change_id = structured
            .and_then(|content| content.get("change_id"))
            .and_then(Value::as_str)
            .filter(|value| safe_change_id(value))
            .map(str::to_string);
        let changed_file_count = structured
            .and_then(|content| content.get("affected_files"))
            .and_then(Value::as_array)
            .map(|files| files.len() as u64);

        ToolAuditRecord {
            timestamp_ms: self.timestamp_ms,
            duration_ms: self.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            request_id: self.request_id,
            session_id: self.session_id,
            workspace_id,
            tool_name: self.tool_name,
            action_name: self.action_name,
            wrapper_tool_name: self.wrapper_tool_name,
            auth_method: self.auth_method,
            client_id: self.client_id,
            outcome,
            error_category,
            request_bytes: self.request_bytes,
            response_bytes,
            change_id,
            changed_file_count,
            execution_host: self.execution_host,
            source_peer: self.source_peer,
        }
    }
}

fn is_auditable_computer_action(name: &str) -> bool {
    matches!(
        name,
        "request"
            | "status"
            | "stop"
            | "screenshot"
            | "windows"
            | "ui_tree"
            | "click"
            | "double_click"
            | "right_click"
            | "move"
            | "drag"
            | "scroll"
            | "type"
            | "key"
            | "press_element"
            | "focus_app"
            | "open_url"
    )
}

fn store() -> &'static Mutex<ToolAuditStore> {
    STORE.get_or_init(|| {
        Mutex::new(ToolAuditStore::new(
            log_dir_for_profile("global-mcp").join("tool-audit"),
        ))
    })
}

fn with_store<R>(f: impl FnOnce(&mut ToolAuditStore) -> R) -> R {
    let mut store = store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut store)
}

pub(crate) fn initialize(retention_days: u16) {
    let retention_days = normalize_retention_days(retention_days);
    RETENTION_DAYS.store(retention_days, Ordering::Relaxed);
    with_store(|store| {
        if store.prune_expired(unix_time_ms(), retention_days).is_err() {
            store.last_write_error =
                Some("启动时清理过期审计记录失败；请检查应用日志目录权限和磁盘空间。".into());
        }
    });
}

pub(crate) fn normalize_retention_days(days: u16) -> u16 {
    days.clamp(1, MAX_RETENTION_DAYS)
}

pub(crate) fn is_valid_retention_days(days: u16) -> bool {
    (1..=MAX_RETENTION_DAYS).contains(&days)
}

pub(crate) fn set_retention_days(days: u16) {
    let days = normalize_retention_days(days);
    RETENTION_DAYS.store(days, Ordering::Relaxed);
    with_store(|store| {
        if store.prune_expired(unix_time_ms(), days).is_err() {
            store.last_write_error =
                Some("更新保留期限后清理旧记录失败；请检查应用日志目录权限和磁盘空间。".into());
        }
    });
}

pub(crate) fn record_tool_call(
    call: ToolAuditCall,
    workspace_id: Option<String>,
    response: &Value,
) {
    let record = call.finish(workspace_id, response);
    with_store(|store| {
        if store.append(&record).is_err() {
            store.last_write_error = Some(
                "最近一次 MCP 调用未能写入审计记录；请检查磁盘空间和应用日志目录权限。".into(),
            );
        } else {
            store.prune_if_due(unix_time_ms(), RETENTION_DAYS.load(Ordering::Relaxed));
        }
    });
}

pub(crate) fn list_tool_calls(filter: &ToolAuditFilter) -> std::io::Result<ToolAuditPage> {
    with_store(|store| store.list(filter, RETENTION_DAYS.load(Ordering::Relaxed)))
}

pub(crate) fn clear_tool_calls(workspace_id: Option<&str>) -> std::io::Result<()> {
    with_store(|store| store.clear(workspace_id))
}
