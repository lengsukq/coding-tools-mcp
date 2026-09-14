use super::buffer::RetainedBuffer;
use super::*;

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
    pub(super) termination_reason: Mutex<Option<String>>,
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

    pub(super) fn record_exit_status(&self, status: std::process::ExitStatus) {
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

    pub(super) fn retained_stream(&self, stream: &str) -> RetainedBuffer {
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
