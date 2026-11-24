//! Privacy-preserving local JSONL audit trail for authenticated MCP tool calls.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use std::thread;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tunnel::log_dir_for_profile;
use crate::util::json::serialized_json_bytes;

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
const AUDIT_CHANNEL_CAPACITY: usize = 1024;

static RETENTION_DAYS: AtomicU16 = AtomicU16::new(DEFAULT_RETENTION_DAYS);
static RUNTIME: OnceLock<AuditRuntime> = OnceLock::new();

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

pub(crate) struct ToolAuditInput<'a> {
    pub(crate) request_id: &'a Value,
    pub(crate) tool_name: &'a str,
    pub(crate) session_id: Option<&'a str>,
    pub(crate) auth_method: &'a str,
    pub(crate) client_id: Option<&'a str>,
    pub(crate) request_bytes: usize,
    pub(crate) body: &'a Value,
}

impl ToolAuditCall {
    pub(crate) fn new(input: ToolAuditInput<'_>) -> Self {
        let ToolAuditInput {
            request_id,
            tool_name,
            session_id,
            auth_method,
            client_id,
            request_bytes,
            body,
        } = input;
        let (delegated_tool, wrapper_tool_name) = if tool_name == "workspace_invoke" {
            let inner = body
                .get("params")
                .and_then(|params| params.get("arguments"))
                .and_then(|arguments| arguments.get("tool"))
                .and_then(Value::as_str)
                .map(safe_label);
            let wrapper = inner.as_ref().map(|_| "workspace_invoke".to_string());
            (inner, wrapper)
        } else if tool_name == crate::upstream_mcp::TOOL_UPSTREAM_CALL {
            let arguments = body
                .get("params")
                .and_then(|params| params.get("arguments"));
            let srv = arguments
                .and_then(|a| a.get("server"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|v| !v.is_empty());
            let inner_tool = arguments
                .and_then(|a| a.get("tool"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|v| !v.is_empty());
            let label = match (srv, inner_tool) {
                (Some(s), Some(t)) => Some(safe_label(&format!("{s}__{t}"))),
                (None, Some(t)) => Some(safe_label(t)),
                _ => None,
            };
            let wrapper = label
                .as_ref()
                .map(|_| crate::upstream_mcp::TOOL_UPSTREAM_CALL.to_string());
            (label, wrapper)
        } else {
            (None, None)
        };

        Self {
            timestamp_ms: unix_time_ms(),
            started: Instant::now(),
            request_id: bounded_identifier(request_id, 128),
            session_id: session_id.map(|value| bounded_identifier_text(value, 256)),
            tool_name: delegated_tool.unwrap_or_else(|| safe_label(tool_name)),
            action_name: tool_name
                .starts_with("computer_")
                .then(|| {
                    body.pointer("/params/arguments/action")
                        .and_then(Value::as_str)
                        .filter(|name| is_auditable_computer_action(name))
                        .map(str::to_string)
                })
                .flatten(),
            wrapper_tool_name,
            auth_method: safe_auth_method(auth_method),
            client_id: client_id.map(|value| bounded_visible_text(value, 128)),
            request_bytes: request_bytes as u64,
            execution_host: None,
            source_peer: None,
        }
    }

    pub(crate) fn with_peer_context(
        mut self,
        execution_host: Option<String>,
        source_peer: Option<String>,
    ) -> Self {
        self.execution_host = execution_host.map(|v| bounded_visible_text(&v, 128));
        self.source_peer = source_peer.map(|v| bounded_visible_text(&v, 128));
        self
    }

    pub(crate) fn set_execution_host(&mut self, host: Option<String>) {
        self.execution_host = host.map(|v| bounded_visible_text(&v, 128));
    }

    pub(super) fn finish(self, workspace_id: Option<String>, response: &Value) -> ToolAuditRecord {
        let response_bytes = serialized_json_bytes(response)
            .map(|bytes| bytes as u64)
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

enum AuditCommand {
    Record {
        record: Box<ToolAuditRecord>,
        retention_days: u16,
    },
    Initialize {
        retention_days: u16,
        done: mpsc::SyncSender<()>,
    },
    SetRetention {
        retention_days: u16,
        done: mpsc::SyncSender<()>,
    },
    List {
        filter: ToolAuditFilter,
        retention_days: u16,
        dropped: u64,
        reply: mpsc::SyncSender<std::io::Result<ToolAuditPage>>,
    },
    Clear {
        workspace_id: Option<String>,
        reply: mpsc::SyncSender<std::io::Result<()>>,
    },
}

enum AuditBackend {
    /// Dedicated background writer thread keeps `record_tool_call` non-blocking
    /// on async request threads while FIFO command ordering gives `list`/`clear`/
    /// `set_retention_days`/`initialize` a synchronous flush barrier.
    Threaded {
        sender: Option<mpsc::SyncSender<AuditCommand>>,
        worker: Option<thread::JoinHandle<()>>,
    },
    Direct(Mutex<ToolAuditStore>),
}

pub(super) struct AuditRuntime {
    backend: AuditBackend,
    dropped_records: AtomicU64,
}

impl AuditRuntime {
    pub(super) fn new(root: PathBuf) -> Self {
        let store = ToolAuditStore::new(root.clone());
        let (tx, rx) = mpsc::sync_channel::<AuditCommand>(AUDIT_CHANNEL_CAPACITY);
        let backend = match thread::Builder::new()
            .name("tool-audit-writer".into())
            .spawn(move || run_audit_writer(store, rx))
        {
            Ok(worker) => AuditBackend::Threaded {
                sender: Some(tx),
                worker: Some(worker),
            },
            Err(_) => AuditBackend::Direct(Mutex::new(ToolAuditStore::new(root))),
        };
        Self {
            backend,
            dropped_records: AtomicU64::new(0),
        }
    }

    pub(super) fn initialize(&self, retention_days: u16) {
        match &self.backend {
            AuditBackend::Threaded {
                sender: Some(sender),
                ..
            } => {
                let (done_tx, done_rx) = mpsc::sync_channel(1);
                if sender
                    .send(AuditCommand::Initialize {
                        retention_days,
                        done: done_tx,
                    })
                    .is_ok()
                {
                    let _ = done_rx.recv();
                }
            }
            AuditBackend::Threaded { sender: None, .. } => {}
            AuditBackend::Direct(store) => {
                let mut store = store
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                apply_initialize(&mut store, retention_days);
            }
        }
    }

    pub(super) fn set_retention_days(&self, retention_days: u16) {
        match &self.backend {
            AuditBackend::Threaded {
                sender: Some(sender),
                ..
            } => {
                let (done_tx, done_rx) = mpsc::sync_channel(1);
                if sender
                    .send(AuditCommand::SetRetention {
                        retention_days,
                        done: done_tx,
                    })
                    .is_ok()
                {
                    let _ = done_rx.recv();
                }
            }
            AuditBackend::Threaded { sender: None, .. } => {}
            AuditBackend::Direct(store) => {
                let mut store = store
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                apply_set_retention(&mut store, retention_days);
            }
        }
    }

    pub(super) fn record(&self, record: ToolAuditRecord, retention_days: u16) {
        match &self.backend {
            AuditBackend::Threaded {
                sender: Some(sender),
                ..
            } => {
                if sender
                    .try_send(AuditCommand::Record {
                        record: Box::new(record),
                        retention_days,
                    })
                    .is_err()
                {
                    self.dropped_records.fetch_add(1, Ordering::Relaxed);
                }
            }
            AuditBackend::Threaded { sender: None, .. } => {
                self.dropped_records.fetch_add(1, Ordering::Relaxed);
            }
            AuditBackend::Direct(store) => {
                let mut store = store
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                apply_record(&mut store, &record, retention_days);
            }
        }
    }

    pub(super) fn list(
        &self,
        filter: &ToolAuditFilter,
        retention_days: u16,
    ) -> std::io::Result<ToolAuditPage> {
        let dropped = self.dropped_records.load(Ordering::Relaxed);
        match &self.backend {
            AuditBackend::Threaded {
                sender: Some(sender),
                ..
            } => {
                let (reply_tx, reply_rx) = mpsc::sync_channel(1);
                sender
                    .send(AuditCommand::List {
                        filter: filter.clone(),
                        retention_days,
                        dropped,
                        reply: reply_tx,
                    })
                    .map_err(|_| std::io::Error::other("tool audit writer thread stopped"))?;
                reply_rx
                    .recv()
                    .map_err(|_| std::io::Error::other("tool audit writer thread stopped"))?
            }
            AuditBackend::Threaded { sender: None, .. } => {
                Err(std::io::Error::other("tool audit writer thread stopped"))
            }
            AuditBackend::Direct(store) => {
                let mut store = store
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                apply_list(&mut store, filter, retention_days, dropped)
            }
        }
    }

    pub(super) fn clear(&self, workspace_id: Option<&str>) -> std::io::Result<()> {
        let result = match &self.backend {
            AuditBackend::Threaded {
                sender: Some(sender),
                ..
            } => {
                let (reply_tx, reply_rx) = mpsc::sync_channel(1);
                sender
                    .send(AuditCommand::Clear {
                        workspace_id: workspace_id.map(str::to_string),
                        reply: reply_tx,
                    })
                    .map_err(|_| std::io::Error::other("tool audit writer thread stopped"))?;
                reply_rx
                    .recv()
                    .map_err(|_| std::io::Error::other("tool audit writer thread stopped"))?
            }
            AuditBackend::Threaded { sender: None, .. } => {
                Err(std::io::Error::other("tool audit writer thread stopped"))
            }
            AuditBackend::Direct(store) => {
                let mut store = store
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                store.clear(workspace_id)
            }
        };
        if result.is_ok() && workspace_id.is_none() {
            self.dropped_records.store(0, Ordering::Relaxed);
        }
        result
    }
}

impl Drop for AuditRuntime {
    fn drop(&mut self) {
        if let AuditBackend::Threaded { sender, worker } = &mut self.backend {
            let _ = sender.take();
            if let Some(handle) = worker.take() {
                let _ = handle.join();
            }
        }
    }
}

fn run_audit_writer(mut store: ToolAuditStore, receiver: mpsc::Receiver<AuditCommand>) {
    while let Ok(command) = receiver.recv() {
        match command {
            AuditCommand::Record {
                record,
                retention_days,
            } => apply_record(&mut store, &record, retention_days),
            AuditCommand::Initialize {
                retention_days,
                done,
            } => {
                apply_initialize(&mut store, retention_days);
                let _ = done.send(());
            }
            AuditCommand::SetRetention {
                retention_days,
                done,
            } => {
                apply_set_retention(&mut store, retention_days);
                let _ = done.send(());
            }
            AuditCommand::List {
                filter,
                retention_days,
                dropped,
                reply,
            } => {
                let _ = reply.send(apply_list(&mut store, &filter, retention_days, dropped));
            }
            AuditCommand::Clear {
                workspace_id,
                reply,
            } => {
                let _ = reply.send(store.clear(workspace_id.as_deref()));
            }
        }
    }
}

fn apply_initialize(store: &mut ToolAuditStore, retention_days: u16) {
    if store.prune_expired(unix_time_ms(), retention_days).is_err() {
        store.last_write_error =
            Some("启动时清理过期审计记录失败；请检查应用日志目录权限和磁盘空间。".into());
    }
}

fn apply_set_retention(store: &mut ToolAuditStore, retention_days: u16) {
    if store.prune_expired(unix_time_ms(), retention_days).is_err() {
        store.last_write_error =
            Some("更新保留期限后清理旧记录失败；请检查应用日志目录权限和磁盘空间。".into());
    }
}

fn apply_record(store: &mut ToolAuditStore, record: &ToolAuditRecord, retention_days: u16) {
    if store.append(record).is_err() {
        store.last_write_error =
            Some("最近一次 MCP 调用未能写入审计记录；请检查磁盘空间和应用日志目录权限。".into());
    } else {
        store.prune_if_due(unix_time_ms(), retention_days);
    }
}

fn apply_list(
    store: &mut ToolAuditStore,
    filter: &ToolAuditFilter,
    retention_days: u16,
    dropped: u64,
) -> std::io::Result<ToolAuditPage> {
    store.list(filter, retention_days).map(|mut page| {
        if dropped > 0 && page.health_message.is_none() {
            page.health_message = Some(format!(
                "审计写入队列已满，累计丢弃 {dropped} 条记录；请检查磁盘 I/O 性能。"
            ));
        }
        page
    })
}

fn runtime() -> &'static AuditRuntime {
    RUNTIME.get_or_init(|| AuditRuntime::new(log_dir_for_profile("global-mcp").join("tool-audit")))
}

pub(crate) fn initialize(retention_days: u16) {
    let retention_days = normalize_retention_days(retention_days);
    RETENTION_DAYS.store(retention_days, Ordering::Relaxed);
    runtime().initialize(retention_days);
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
    runtime().set_retention_days(days);
}

pub(crate) fn record_tool_call(
    call: ToolAuditCall,
    workspace_id: Option<String>,
    response: &Value,
) {
    let record = call.finish(workspace_id, response);
    runtime().record(record, RETENTION_DAYS.load(Ordering::Relaxed));
}

pub(crate) fn list_tool_calls(filter: &ToolAuditFilter) -> std::io::Result<ToolAuditPage> {
    runtime().list(filter, RETENTION_DAYS.load(Ordering::Relaxed))
}

pub(crate) fn clear_tool_calls(workspace_id: Option<&str>) -> std::io::Result<()> {
    runtime().clear(workspace_id)
}
