use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, Mutex as AsyncMutex};

use crate::platform::platform;

const STDERR_RING_CAPACITY: usize = 64;
const MAX_STDERR_LINE_BYTES: usize = 1024;

type PendingMap = Arc<StdMutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

pub struct StdioTransportConfig {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: BTreeMap<String, String>,
    pub extra_paths: Vec<PathBuf>,
}

pub struct StdioTransport {
    pid: Option<u32>,
    child: Arc<AsyncMutex<Option<Child>>>,
    write_tx: mpsc::UnboundedSender<String>,
    pending: PendingMap,
    next_id: AtomicU64,
    closed: Arc<AtomicBool>,
    stderr_ring: Arc<StdMutex<VecDeque<String>>>,
    reader_task: tokio::task::JoinHandle<()>,
    writer_task: tokio::task::JoinHandle<()>,
    stderr_task: tokio::task::JoinHandle<()>,
}

impl StdioTransport {
    pub async fn spawn(
        config: StdioTransportConfig,
        on_tools_changed: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, String> {
        let cmd_str = config.command.trim();
        if cmd_str.is_empty() {
            return Err("Stdio MCP 启动命令不能为空".into());
        }

        let mut command = Command::new(cmd_str);
        command
            .args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        if let Some(cwd) = config
            .cwd
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            let expanded = expand_tilde(cwd);
            if !expanded.is_dir() {
                return Err(format!("Stdio 工作目录不存在: {}", expanded.display()));
            }
            command.current_dir(expanded);
        }

        if !config.extra_paths.is_empty() {
            let merged_path = merge_path_env(&config.extra_paths);
            if !merged_path.is_empty() {
                command.env("PATH", merged_path);
            }
        }

        for (key, value) in &config.env {
            let k = key.trim();
            if !k.is_empty() {
                command.env(k, value);
            }
        }

        let mut child = command.spawn().map_err(|err| {
            format!(
                "启动 Stdio MCP 进程 `{cmd_str}` 失败: {err}（请确认命令已安装或在「全局 PATH」中可见）"
            )
        })?;
        let pid = child.id();

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "无法接管 Stdio MCP 子进程 stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "无法接管 Stdio MCP 子进程 stdout".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "无法接管 Stdio MCP 子进程 stderr".to_string())?;

        let (write_tx, mut write_rx) = mpsc::unbounded_channel::<String>();
        let pending: PendingMap = Arc::new(StdMutex::new(HashMap::new()));
        let closed = Arc::new(AtomicBool::new(false));
        let stderr_ring = Arc::new(StdMutex::new(VecDeque::with_capacity(STDERR_RING_CAPACITY)));

        let closed_writer = Arc::clone(&closed);
        let pending_writer = Arc::clone(&pending);
        let writer_task = tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(line) = write_rx.recv().await {
                if stdin.write_all(line.as_bytes()).await.is_err()
                    || stdin.write_all(b"\n").await.is_err()
                    || stdin.flush().await.is_err()
                {
                    closed_writer.store(true, Ordering::SeqCst);
                    fail_all_pending(
                        &pending_writer,
                        "写入 Stdio MCP 子进程管道失败（进程可能已退出）",
                    );
                    break;
                }
            }
        });

        let stderr_ring_task = Arc::clone(&stderr_ring);
        let stderr_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let clean =
                    crate::util::text::truncate_utf8(line.trim_end(), MAX_STDERR_LINE_BYTES)
                        .to_string();
                if clean.is_empty() {
                    continue;
                }
                let mut ring = stderr_ring_task.lock().unwrap_or_else(|e| e.into_inner());
                if ring.len() >= STDERR_RING_CAPACITY {
                    ring.pop_front();
                }
                ring.push_back(clean);
            }
        });

        let pending_reader = Arc::clone(&pending);
        let closed_reader = Arc::clone(&closed);
        let stderr_ring_reader = Arc::clone(&stderr_ring);
        let reader_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
                    // Some stdio MCP servers emit startup banners on stdout; keep in stderr ring for diagnosis.
                    let mut ring = stderr_ring_reader.lock().unwrap_or_else(|e| e.into_inner());
                    if ring.len() >= STDERR_RING_CAPACITY {
                        ring.pop_front();
                    }
                    ring.push_back(format!(
                        "[stdout-non-json] {}",
                        crate::util::text::truncate_utf8(trimmed, 256)
                    ));
                    continue;
                };
                if msg.get("method").and_then(Value::as_str)
                    == Some("notifications/tools/list_changed")
                {
                    on_tools_changed();
                    continue;
                }
                if let Some(id_num) = msg.get("id").and_then(Value::as_u64) {
                    let tx = pending_reader
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(&id_num);
                    if let Some(tx) = tx {
                        let _ = tx.send(Ok(msg));
                    }
                }
            }
            closed_reader.store(true, Ordering::SeqCst);
            let tail = stderr_tail_snapshot(&stderr_ring_reader);
            let reason = if tail.is_empty() {
                "Stdio MCP 子进程意外退出（stdout EOF）".to_string()
            } else {
                format!(
                    "Stdio MCP 子进程意外退出（stdout EOF）。最近 stderr: {}",
                    tail.last().cloned().unwrap_or_default()
                )
            };
            fail_all_pending(&pending_reader, &reason);
        });

        Ok(Self {
            pid,
            child: Arc::new(AsyncMutex::new(Some(child))),
            write_tx,
            pending,
            next_id: AtomicU64::new(1),
            closed,
            stderr_ring,
            reader_task,
            writer_task,
            stderr_task,
        })
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub fn is_alive(&self) -> bool {
        !self.closed.load(Ordering::SeqCst)
    }

    pub fn stderr_tail(&self) -> Vec<String> {
        stderr_tail_snapshot(&self.stderr_ring)
    }

    pub async fn send_request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        if !self.is_alive() {
            return Err("Stdio MCP 子进程已断开".into());
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        })
        .to_string();

        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, tx);

        if self.write_tx.send(payload).is_err() {
            self.pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id);
            self.closed.store(true, Ordering::SeqCst);
            return Err("无法向 Stdio MCP 子进程发送请求（通道已关闭）".into());
        }

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("Stdio MCP 子进程在返回结果前中断连接".into()),
            Err(_) => {
                self.pending
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&id);
                Err(format!(
                    "Stdio MCP 调用 `{method}` 超时（>{} 秒）",
                    timeout.as_secs()
                ))
            }
        }
    }

    pub async fn send_notification(&self, method: &str, params: Value) -> Result<(), String> {
        if !self.is_alive() {
            return Err("Stdio MCP 子进程已断开".into());
        }
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        })
        .to_string();
        self.write_tx
            .send(payload)
            .map_err(|_| "无法发送 Stdio MCP 通知（通道已关闭）".to_string())
    }

    pub async fn shutdown(&self) {
        self.closed.store(true, Ordering::SeqCst);
        fail_all_pending(&self.pending, "Stdio MCP 连接已主动关闭");
        if let Some(pid) = self.pid {
            let _ = platform().terminate_process_tree(pid);
        }
        if let Some(mut child) = self.child.lock().await.take() {
            let _ = child.kill().await;
            let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
        }
        self.reader_task.abort();
        self.writer_task.abort();
        self.stderr_task.abort();
    }

    pub fn terminate_sync(&self) {
        self.closed.store(true, Ordering::SeqCst);
        if let Some(pid) = self.pid {
            let _ = platform().terminate_process_tree(pid);
        }
        if let Ok(mut guard) = self.child.try_lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.start_kill();
            }
        }
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        self.terminate_sync();
        self.reader_task.abort();
        self.writer_task.abort();
        self.stderr_task.abort();
    }
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

fn stderr_tail_snapshot(ring: &Arc<StdMutex<VecDeque<String>>>) -> Vec<String> {
    ring.lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .cloned()
        .collect()
}

fn expand_tilde(raw: &str) -> PathBuf {
    if raw == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(raw));
    }
    if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(raw)
}

fn merge_path_env(extra: &[PathBuf]) -> std::ffi::OsString {
    let mut paths: Vec<PathBuf> = extra.to_vec();
    if let Some(existing) = std::env::var_os("PATH") {
        for item in std::env::split_paths(&existing) {
            if !paths.contains(&item) {
                paths.push(item);
            }
        }
    }
    std::env::join_paths(paths).unwrap_or_default()
}
