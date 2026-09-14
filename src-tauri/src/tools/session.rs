use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tokio::io::AsyncReadExt;
use tokio::process::{Child, ChildStdin};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use crate::tools::workspace::{tool_ok, WorkspaceError};
use serde_json::{json, Value};

const SESSION_BUFFER_BYTES: usize = 1_048_576;
const SESSION_HEAD_BYTES: usize = 128 * 1024;
const SESSION_TAIL_BYTES: usize = SESSION_BUFFER_BYTES - SESSION_HEAD_BYTES;
const MAX_EVICTED_TOMBSTONES: usize = 256;

/// One command runtime per workspace for the lifetime of the desktop process.
///
/// MCP listeners own a ToolContext, and listener instances can be
/// restarted without restarting the desktop application. Keeping the store here
/// makes running commands a workspace resource instead of a transport-session
/// resource.
static WORKSPACE_SESSION_STORES: OnceLock<Mutex<HashMap<PathBuf, Arc<SessionStore>>>> =
    OnceLock::new();

pub struct SessionStore {
    sessions: Mutex<HashMap<String, Arc<ExecSession>>>,
    evicted: Mutex<HashMap<String, EvictedCommand>>,
}

fn unix_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

impl Default for SessionStore {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            evicted: Mutex::new(HashMap::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_buffer_keeps_head_and_tail_of_large_stream() {
        let mut buffer = RetainedBuffer::default();
        buffer.append(b"UNIQUE_HEAD_ERROR\n");
        buffer.append(&vec![b'x'; 10 * 1024 * 1024]);
        buffer.append(b"\nUNIQUE_TAIL_ERROR");

        assert!(buffer.total_bytes > 10 * 1024 * 1024);
        assert_eq!(buffer.head.len(), SESSION_HEAD_BYTES);
        assert_eq!(buffer.tail.len(), SESSION_TAIL_BYTES);
        assert!(buffer.evicted_bytes() > 0);
        assert!(String::from_utf8_lossy(&buffer.head).contains("UNIQUE_HEAD_ERROR"));
        assert!(String::from_utf8_lossy(&buffer.tail).contains("UNIQUE_TAIL_ERROR"));

        let preview = buffer.preview(64 * 1024);
        assert!(preview.truncated);
        assert!(preview.content.contains("UNIQUE_HEAD_ERROR"));
        assert!(preview.content.contains("UNIQUE_TAIL_ERROR"));

        let head_page = buffer.read_page(0, SESSION_HEAD_BYTES);
        assert!(String::from_utf8_lossy(&head_page.content).contains("UNIQUE_HEAD_ERROR"));
        let tail_offset = head_page
            .next_offset
            .expect("tail offset after evicted gap");
        assert_eq!(tail_offset, buffer.tail_start());
        let tail_page = buffer.read_page(tail_offset, SESSION_TAIL_BYTES);
        assert!(String::from_utf8_lossy(&tail_page.content).contains("UNIQUE_TAIL_ERROR"));
        assert_eq!(tail_page.next_offset, None);
        assert_eq!(tail_page.evicted_bytes, buffer.evicted_bytes());
    }

    #[test]
    fn retained_buffer_skips_an_evicted_offset_to_the_tail() {
        let mut buffer = RetainedBuffer::default();
        buffer.append(&vec![b'a'; SESSION_BUFFER_BYTES * 2]);
        let gap_offset = SESSION_HEAD_BYTES + 1024;
        let page = buffer.read_page(gap_offset, 4096);
        assert!(buffer.evicted_bytes() > 0);
        assert_eq!(page.offset, buffer.tail_start());
        assert!(page.offset > gap_offset);
    }

    #[test]
    fn preview_keeps_tail_even_before_tail_retention_segment_is_used() {
        let mut buffer = RetainedBuffer::default();
        buffer.append(b"PREVIEW_HEAD\n");
        buffer.append(&vec![b'm'; 96 * 1024]);
        buffer.append(b"\nPREVIEW_TAIL");
        assert_eq!(buffer.evicted_bytes(), 0);
        assert!(buffer.tail.is_empty());

        let preview = buffer.preview(16 * 1024);
        assert!(preview.truncated);
        assert!(preview.content.contains("PREVIEW_HEAD"));
        assert!(preview.content.contains("PREVIEW_TAIL"));
    }
}

#[derive(Debug, Clone)]
struct EvictedCommand {
    command_id: String,
    evicted_at: String,
    termination_reason: String,
    exit_code: Option<i32>,
}

fn command_id_arg(args: &Value) -> Result<&str, WorkspaceError> {
    args.get("command_id")
        .and_then(Value::as_str)
        .or_else(|| args.get("session_id").and_then(Value::as_str))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            WorkspaceError::invalid_argument(
                "command_id is required (legacy session_id is also accepted)",
            )
        })
}

impl SessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, session: ExecSession) -> Arc<ExecSession> {
        let arc = Arc::new(session);
        self.sessions
            .lock()
            .expect("sessions lock")
            .insert(arc.session_id.clone(), arc.clone());
        arc
    }

    pub fn get(&self, session_id: &str) -> Result<Arc<ExecSession>, WorkspaceError> {
        if let Some(session) = self
            .sessions
            .lock()
            .expect("sessions lock")
            .get(session_id)
            .cloned()
        {
            return Ok(session);
        }
        if let Some(evicted) = self
            .evicted
            .lock()
            .expect("evicted commands lock")
            .get(session_id)
            .cloned()
        {
            return Err(WorkspaceError::ToolDetails {
                code: "COMMAND_EVICTED",
                message: format!("Command output is no longer retained: {session_id}"),
                category: "runtime",
                retryable: false,
                details: json!({
                    "command_id": evicted.command_id,
                    "command_state": "evicted",
                    "evicted_at": evicted.evicted_at,
                    "termination_reason": evicted.termination_reason,
                    "exit_code": evicted.exit_code
                }),
            });
        }
        Err(WorkspaceError::Tool {
            code: "SESSION_NOT_FOUND",
            message: format!("Command not found: {session_id}"),
            category: "not_found",
            retryable: false,
        })
    }

    pub fn remove(&self, session_id: &str) {
        let removed = self
            .sessions
            .lock()
            .expect("sessions lock")
            .remove(session_id);
        if let Some(session) = removed {
            let mut evicted = self.evicted.lock().expect("evicted commands lock");
            if evicted.len() >= MAX_EVICTED_TOMBSTONES {
                if let Some(oldest) = evicted.keys().next().cloned() {
                    evicted.remove(&oldest);
                }
            }
            evicted.insert(
                session_id.to_string(),
                EvictedCommand {
                    command_id: session_id.to_string(),
                    evicted_at: unix_timestamp(),
                    termination_reason: session
                        .termination_reason
                        .lock()
                        .expect("termination lock")
                        .clone()
                        .unwrap_or_else(|| "evicted".into()),
                    exit_code: *session.exit_code.lock().expect("exit_code lock"),
                },
            );
        }
    }

    fn session_ids(&self) -> Vec<String> {
        self.sessions
            .lock()
            .expect("sessions lock")
            .keys()
            .cloned()
            .collect()
    }
}

pub fn workspace_session_store(workspace_root: &Path) -> Arc<SessionStore> {
    let registry = WORKSPACE_SESSION_STORES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut registry = registry.lock().expect("workspace session registry lock");
    registry
        .entry(workspace_root.to_path_buf())
        .or_insert_with(|| Arc::new(SessionStore::new()))
        .clone()
}

pub fn kill_workspace_sessions(workspace_root: &Path) -> usize {
    let store = WORKSPACE_SESSION_STORES
        .get()
        .and_then(|registry| {
            registry
                .lock()
                .expect("workspace session registry lock")
                .get(workspace_root)
                .cloned()
        })
        .unwrap_or_else(|| Arc::new(SessionStore::new()));

    store
        .session_ids()
        .into_iter()
        .filter(|command_id| {
            kill_session(
                &store,
                &json!({
                    "command_id": command_id,
                    "signal": "TERM",
                    "wait_ms": 1500,
                    "max_output_bytes": 1024
                }),
            )
            .is_ok()
        })
        .count()
}

#[derive(Debug, Clone, Default)]
struct RetainedBuffer {
    head: Vec<u8>,
    tail: Vec<u8>,
    total_bytes: usize,
}

#[derive(Debug, Clone)]
struct RetainedPage {
    content: Vec<u8>,
    offset: usize,
    next_offset: Option<usize>,
    total_bytes: usize,
    head_retained_bytes: usize,
    tail_retained_bytes: usize,
    evicted_bytes: usize,
}

impl RetainedBuffer {
    fn append(&mut self, mut chunk: &[u8]) {
        self.total_bytes = self.total_bytes.saturating_add(chunk.len());

        if self.head.len() < SESSION_HEAD_BYTES {
            let take = (SESSION_HEAD_BYTES - self.head.len()).min(chunk.len());
            self.head.extend_from_slice(&chunk[..take]);
            chunk = &chunk[take..];
        }

        if !chunk.is_empty() {
            self.tail.extend_from_slice(chunk);
            if self.tail.len() > SESSION_TAIL_BYTES {
                let drop = self.tail.len() - SESSION_TAIL_BYTES;
                self.tail.drain(..drop);
            }
        }
    }

    fn retained_bytes(&self) -> usize {
        self.head.len() + self.tail.len()
    }

    fn evicted_bytes(&self) -> usize {
        self.total_bytes.saturating_sub(self.retained_bytes())
    }

    fn tail_start(&self) -> usize {
        self.total_bytes.saturating_sub(self.tail.len())
    }

    fn read_page(&self, requested_offset: usize, limit: usize) -> RetainedPage {
        let head_len = self.head.len();
        let tail_start = self.tail_start();

        let (offset, content, next_offset) = if requested_offset < head_len {
            let end = head_len.min(requested_offset.saturating_add(limit));
            let next = if end < head_len {
                Some(end)
            } else if tail_start < self.total_bytes {
                Some(tail_start.max(head_len))
            } else {
                None
            };
            (
                requested_offset,
                self.head[requested_offset..end].to_vec(),
                next,
            )
        } else if requested_offset < tail_start {
            let end = self.total_bytes.min(tail_start.saturating_add(limit));
            let take = end.saturating_sub(tail_start);
            let next = (end < self.total_bytes).then_some(end);
            (tail_start, self.tail[..take].to_vec(), next)
        } else {
            let start = requested_offset
                .saturating_sub(tail_start)
                .min(self.tail.len());
            let end = self.tail.len().min(start.saturating_add(limit));
            let logical_end = tail_start + end;
            let next = (logical_end < self.total_bytes).then_some(logical_end);
            (tail_start + start, self.tail[start..end].to_vec(), next)
        };

        RetainedPage {
            content,
            offset,
            next_offset,
            total_bytes: self.total_bytes,
            head_retained_bytes: self.head.len(),
            tail_retained_bytes: self.tail.len(),
            evicted_bytes: self.evicted_bytes(),
        }
    }

    fn preview(&self, max_bytes: usize) -> Truncated {
        if self.evicted_bytes() == 0 {
            let mut data = self.head.clone();
            data.extend_from_slice(&self.tail);
            if data.len() <= max_bytes {
                return Truncated {
                    content: String::from_utf8_lossy(&data).into_owned(),
                    truncated: false,
                };
            }
            return preview_head_tail(&data, &data, self.total_bytes, max_bytes);
        }

        preview_head_tail(&self.head, &self.tail, self.total_bytes, max_bytes)
    }
}

fn preview_head_tail(head: &[u8], tail: &[u8], total_bytes: usize, max_bytes: usize) -> Truncated {
    let max_bytes = max_bytes.max(1);
    let head_budget = (max_bytes / 8).max(1).min(head.len());
    let tail_budget = max_bytes.saturating_sub(head_budget).min(tail.len());
    let mut content = String::new();
    if head_budget > 0 {
        content.push_str(&String::from_utf8_lossy(&head[..head_budget]));
    }
    let omitted = total_bytes.saturating_sub(head_budget + tail_budget);
    if omitted > 0 {
        content.push_str(&format!("\n...[{omitted} bytes omitted]...\n"));
    }
    if tail_budget > 0 {
        let start = tail.len() - tail_budget;
        content.push_str(&String::from_utf8_lossy(&tail[start..]));
    }
    Truncated {
        content,
        truncated: omitted > 0,
    }
}

pub struct ExecSession {
    pub session_id: String,
    pub(crate) child: AsyncMutex<Child>,
    pub stdin: AsyncMutex<Option<ChildStdin>>,
    stdin_open: Mutex<bool>,
    interactive: bool,
    stdout: Mutex<RetainedBuffer>,
    stderr: Mutex<RetainedBuffer>,
    pub started_at: Instant,
    created_at: String,
    started_at_wall: String,
    finished_at: Mutex<Option<String>>,
    pub exit_code: Mutex<Option<i32>>,
    exited: AtomicBool,
    termination_reason: Mutex<Option<String>>,
    reader_tasks: AsyncMutex<Vec<tauri::async_runtime::JoinHandle<()>>>,
}

impl ExecSession {
    pub fn new(child: Child) -> Self {
        Self::new_with_mode(child, false)
    }

    pub fn new_with_mode(mut child: Child, interactive: bool) -> Self {
        let session_id = Uuid::new_v4().to_string();
        let stdin = child.stdin.take();
        let stdin_open = stdin.is_some();
        Self {
            session_id,
            child: AsyncMutex::new(child),
            stdin: AsyncMutex::new(stdin),
            stdin_open: Mutex::new(stdin_open),
            interactive,
            stdout: Mutex::new(RetainedBuffer::default()),
            stderr: Mutex::new(RetainedBuffer::default()),
            started_at: Instant::now(),
            created_at: unix_timestamp(),
            started_at_wall: unix_timestamp(),
            finished_at: Mutex::new(None),
            exit_code: Mutex::new(None),
            exited: AtomicBool::new(false),
            termination_reason: Mutex::new(None),
            reader_tasks: AsyncMutex::new(Vec::new()),
        }
    }

    pub async fn spawn_readers(self: &Arc<Self>) {
        let stdout = {
            let mut guard = self.child.lock().await;
            guard.stdout.take()
        };
        let stderr = {
            let mut guard = self.child.lock().await;
            guard.stderr.take()
        };
        if let Some(stream) = stdout {
            let session = Arc::clone(self);
            let task = tauri::async_runtime::spawn(async move {
                session.read_stream(stream, true).await;
            });
            self.reader_tasks.lock().await.push(task);
        }
        if let Some(stream) = stderr {
            let session = Arc::clone(self);
            let task = tauri::async_runtime::spawn(async move {
                session.read_stream(stream, false).await;
            });
            self.reader_tasks.lock().await.push(task);
        }
    }

    pub async fn wait_for_readers(&self) {
        let mut tasks = self.reader_tasks.lock().await;
        while let Some(task) = tasks.pop() {
            let _ = tokio::time::timeout(std::time::Duration::from_millis(500), task).await;
        }
    }

    async fn read_stream<T>(&self, mut stream: T, is_stdout: bool)
    where
        T: tokio::io::AsyncRead + Unpin,
    {
        let mut buf = [0u8; 4096];
        loop {
            match stream.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = &buf[..n];
                    if is_stdout {
                        let mut data = self.stdout.lock().expect("stdout lock");
                        data.append(chunk);
                    } else {
                        let mut data = self.stderr.lock().expect("stderr lock");
                        data.append(chunk);
                    }
                }
                Err(_) => break,
            }
        }
    }

    pub async fn kill_and_wait(&self) {
        let status = {
            let mut child = self.child.lock().await;
            let _ = child.start_kill();
            child.wait().await.ok()
        };
        if let Some(status) = status {
            self.record_exit_status(status);
        }
    }

    pub async fn refresh_status(&self) {
        let mut child = self.child.lock().await;
        if let Ok(Some(status)) = child.try_wait() {
            self.record_exit_status(status);
        }
    }

    fn record_exit_status(&self, status: std::process::ExitStatus) {
        *self.exit_code.lock().expect("exit_code lock") = status.code();
        self.exited.store(true, Ordering::Release);
        *self.stdin_open.lock().expect("stdin_open lock") = false;
        let mut finished = self.finished_at.lock().expect("finished_at lock");
        if finished.is_none() {
            *finished = Some(unix_timestamp());
        }
        let mut reason = self.termination_reason.lock().expect("termination lock");
        if reason.is_none() {
            *reason = Some("exited".into());
        }
    }

    pub(crate) fn has_exited(&self) -> bool {
        self.exited.load(Ordering::Acquire)
    }

    pub fn mark_termination_reason(&self, reason: &str) {
        *self.termination_reason.lock().expect("termination lock") = Some(reason.to_string());
    }

    pub(crate) fn mark_stdin_closed(&self) {
        *self.stdin_open.lock().expect("stdin_open lock") = false;
    }

    pub async fn is_running(&self) -> bool {
        self.refresh_status().await;
        !self.has_exited()
    }

    fn retained_stream(&self, stream: &str) -> RetainedBuffer {
        match stream {
            "stderr" => self.stderr.lock().expect("stderr lock").clone(),
            _ => self.stdout.lock().expect("stdout lock").clone(),
        }
    }

    pub fn snapshot(&self, max_output_bytes: usize) -> Value {
        let stdout_buffer = self.stdout.lock().expect("stdout lock").clone();
        let stderr_buffer = self.stderr.lock().expect("stderr lock").clone();
        let stdout = stdout_buffer.preview(max_output_bytes);
        let stderr = stderr_buffer.preview(max_output_bytes);
        let exit_code = *self.exit_code.lock().expect("exit_code lock");
        let termination_reason = self
            .termination_reason
            .lock()
            .expect("termination lock")
            .clone();
        let status = if self.has_exited() {
            "exited"
        } else {
            "running"
        };
        let reason = termination_reason.as_deref().unwrap_or("running");
        let command_ok = match reason {
            "exited" => Some(exit_code.is_some_and(|code| code == 0)),
            "running" => None,
            _ => Some(false),
        };
        let command_state = if !self.has_exited() {
            "running"
        } else {
            match reason {
                "killed" => "killed",
                "timeout" | "spawn_failed" | "crashed" | "server_restart" => "failed",
                _ => "exited",
            }
        };
        json!({
            "command_id": self.session_id,
            "session_id": self.session_id,
            "command_state": command_state,
            "interactive": self.interactive,
            "stdin_open": *self.stdin_open.lock().expect("stdin_open lock"),
            "status": status,
            "termination_reason": reason,
            "recoverable": matches!(reason, "timeout" | "killed" | "spawn_failed" | "server_restart"),
            "suggestion": match reason {
                "timeout" => "读取保留输出，调整 timeout_ms 后重试",
                "killed" => "确认终止原因后重新执行命令",
                "exited" => "检查 exit_code 和 stderr",
                "crashed" => "检查 stderr 后重试或恢复工作区",
                _ => "继续读取 session 或等待进程结束",
            },
            "exit_code": exit_code,
            "transport_ok": true,
            "command_ok": command_ok,
            "stdout": stdout.content,
            "stderr": stderr.content,
            "stdout_truncated": stdout.truncated,
            "stderr_truncated": stderr.truncated,
            "elapsed_ms": self.started_at.elapsed().as_millis(),
            "created_at": self.created_at,
            "started_at": self.started_at_wall,
            "finished_at": self.finished_at.lock().expect("finished_at lock").clone(),
            "stdout_retention": {
                "total_bytes": stdout_buffer.total_bytes,
                "head_retained_bytes": stdout_buffer.head.len(),
                "tail_retained_bytes": stdout_buffer.tail.len(),
                "evicted_bytes": stdout_buffer.evicted_bytes()
            },
            "stderr_retention": {
                "total_bytes": stderr_buffer.total_bytes,
                "head_retained_bytes": stderr_buffer.head.len(),
                "tail_retained_bytes": stderr_buffer.tail.len(),
                "evicted_bytes": stderr_buffer.evicted_bytes()
            },
            "output_refs": {
                "stdout": format!("command:{}:stdout", self.session_id),
                "stderr": format!("command:{}:stderr", self.session_id)
            },
            "legacy_output_refs": {
                "stdout": format!("session:{}:stdout", self.session_id),
                "stderr": format!("session:{}:stderr", self.session_id)
            }
        })
    }
}

struct Truncated {
    content: String,
    truncated: bool,
}

pub fn read_output(store: &SessionStore, args: &Value) -> Result<Value, WorkspaceError> {
    let output_ref = args
        .get("output_ref")
        .and_then(Value::as_str)
        .ok_or_else(|| WorkspaceError::invalid_argument("output_ref is required"))?;
    let parts: Vec<&str> = output_ref.split(':').collect();
    if parts.len() != 3 || (parts[0] != "command" && parts[0] != "session") {
        return Err(WorkspaceError::invalid_argument(
            "output_ref must look like command:<id>:stdout/stderr or legacy session:<id>:stdout/stderr/full",
        ));
    }
    let session_id = parts[1];
    let ref_stream = parts[2];
    if ref_stream != "stdout" && ref_stream != "stderr" && ref_stream != "full" {
        return Err(WorkspaceError::invalid_argument(
            "output_ref stream must be stdout, stderr, or full",
        ));
    }
    let session = store.get(session_id)?;
    tauri::async_runtime::block_on(session.refresh_status());

    let requested_stream = args.get("stream").and_then(Value::as_str).unwrap_or("");
    let stream = if ref_stream == "stdout" || ref_stream == "stderr" {
        ref_stream
    } else if requested_stream == "stdout" || requested_stream == "stderr" {
        requested_stream
    } else {
        "stdout"
    };

    let retained = session.retained_stream(stream);
    let requested_offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(4096)
        .clamp(1, 1_048_576) as usize;
    let page = retained.read_page(requested_offset, limit);
    let mut warnings = Vec::<String>::new();
    if ref_stream == "full" {
        warnings.push(
            "legacy full output_ref defaults to stdout; use output_refs for stable stream paging"
                .into(),
        );
    }
    if page.evicted_bytes > 0 {
        warnings.push(format!(
            "{} byte(s) from the middle of this stream were evicted; head and tail are retained",
            page.evicted_bytes
        ));
    }
    if page.offset != requested_offset {
        warnings.push(format!(
            "requested offset {requested_offset} was inside the evicted gap; resumed at retained tail offset {}",
            page.offset
        ));
    }

    Ok(tool_ok(json!({
        "command_id": session_id,
        "session_id": session_id,
        "output_ref": output_ref,
        "stream_output_ref": format!("command:{session_id}:{stream}"),
        "legacy_stream_output_ref": format!("session:{session_id}:{stream}"),
        "stream": stream,
        "offset": page.offset,
        "requested_offset": requested_offset,
        "limit": limit,
        "content": String::from_utf8_lossy(&page.content),
        "next_offset": page.next_offset,
        "total_retained_bytes": page.head_retained_bytes + page.tail_retained_bytes,
        "total_stream_bytes": page.total_bytes,
        "head_retained_bytes": page.head_retained_bytes,
        "tail_retained_bytes": page.tail_retained_bytes,
        "evicted_bytes": page.evicted_bytes,
        "truncated": page.evicted_bytes > 0 || page.next_offset.is_some(),
        "warnings": warnings
    })))
}

pub fn write_stdin(store: &SessionStore, args: &Value) -> Result<Value, WorkspaceError> {
    let session_id = command_id_arg(args)?;
    let session = store.get(session_id)?;
    let chars = args.get("chars").and_then(Value::as_str).unwrap_or("");
    let max_output_bytes = args
        .get("max_output_bytes")
        .and_then(Value::as_u64)
        .unwrap_or(65_536) as usize;

    let running = tauri::async_runtime::block_on(session.is_running());
    if !running {
        if !chars.is_empty() {
            return Err(WorkspaceError::Tool {
                code: "SESSION_CLOSED",
                message: "Session is closed; stdin write blocked.".into(),
                category: "runtime",
                retryable: false,
            });
        }
        return Ok(tool_ok(session.snapshot(max_output_bytes)));
    }

    if !chars.is_empty() {
        let mut stdin_guard = tauri::async_runtime::block_on(session.stdin.lock());
        let stdin = stdin_guard.as_mut().ok_or_else(|| WorkspaceError::Tool {
            code: "SESSION_CLOSED",
            message: "Session stdin is closed.".into(),
            category: "runtime",
            retryable: false,
        })?;
        use tokio::io::AsyncWriteExt;
        tauri::async_runtime::block_on(async {
            stdin
                .write_all(chars.as_bytes())
                .await
                .map_err(|_| WorkspaceError::Tool {
                    code: "SESSION_CLOSED",
                    message: "Session stdin is closed.".into(),
                    category: "runtime",
                    retryable: false,
                })
        })?;
        let _ = tauri::async_runtime::block_on(stdin.flush());
    }

    let yield_ms = args
        .get("yield_time_ms")
        .and_then(Value::as_u64)
        .unwrap_or(1000)
        .min(30_000);
    std::thread::sleep(std::time::Duration::from_millis(yield_ms));
    tauri::async_runtime::block_on(session.refresh_status());
    Ok(tool_ok(session.snapshot(max_output_bytes)))
}

pub fn kill_session(store: &SessionStore, args: &Value) -> Result<Value, WorkspaceError> {
    let session_id = command_id_arg(args)?;
    let session = store.get(session_id)?;
    let max_output_bytes = args
        .get("max_output_bytes")
        .and_then(Value::as_u64)
        .unwrap_or(65_536) as usize;
    let wait_ms = args
        .get("wait_ms")
        .and_then(Value::as_u64)
        .unwrap_or(5000)
        .min(30_000);
    let signal = args.get("signal").and_then(Value::as_str).unwrap_or("TERM");

    let running = tauri::async_runtime::block_on(session.is_running());
    let mut killed = false;
    let mut status = "exited";
    let mut evicted = true;

    if running {
        session.mark_termination_reason("killed");
        tauri::async_runtime::block_on(async {
            let pid = {
                let child = session.child.lock().await;
                child.id()
            };
            if let Some(pid) = pid {
                send_session_signal(pid, signal);
            } else {
                let mut child = session.child.lock().await;
                let _ = child.start_kill();
            }
            if let Ok(Ok(exit_status)) =
                tokio::time::timeout(std::time::Duration::from_millis(wait_ms), async {
                    let mut child = session.child.lock().await;
                    child.wait().await
                })
                .await
            {
                session.record_exit_status(exit_status);
            }
        });
        tauri::async_runtime::block_on(session.refresh_status());
        if tauri::async_runtime::block_on(session.is_running()) {
            status = "terminating";
            evicted = false;
        } else {
            killed = true;
            status = "killed";
        }
    }

    let mut payload = session.snapshot(max_output_bytes);
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("killed".into(), json!(killed));
        obj.insert("status".into(), json!(status));
        obj.insert("evicted".into(), json!(evicted));
        if status == "terminating" {
            obj.insert(
                "warnings".into(),
                json!(["Process did not exit after kill; session retained for retry"]),
            );
        }
    }

    if evicted {
        store.remove(session_id);
    }

    Ok(tool_ok(payload))
}

#[cfg(unix)]
fn send_session_signal(pid: u32, signal: &str) {
    let sig = match signal {
        "KILL" => libc::SIGKILL,
        "INT" => libc::SIGINT,
        _ => libc::SIGTERM,
    };
    unsafe {
        libc::kill(pid as i32, sig);
    }
}

#[cfg(windows)]
fn send_session_signal(pid: u32, _signal: &str) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    unsafe {
        if let Ok(handle) = OpenProcess(PROCESS_TERMINATE, false, pid) {
            let _ = TerminateProcess(handle, 1);
            let _ = CloseHandle(handle);
        }
    }
}
