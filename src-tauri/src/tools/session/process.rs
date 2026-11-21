use tokio::io::BufWriter;

use super::buffer::RetainedBuffer;
use super::output_store::CommandOutputStore;
use super::*;

const OUTPUT_WRITER_BUFFER_BYTES: usize = 64 * 1024;
const STREAM_READ_BUFFER_BYTES: usize = 16 * 1024;

pub struct ExecSession {
    pub session_id: String,
    pub(crate) child: AsyncMutex<Option<Child>>,
    pid: Option<u32>,
    pub stdin: AsyncMutex<Option<ChildStdin>>,
    stdin_open: Mutex<bool>,
    interactive: bool,
    stdout: Mutex<RetainedBuffer>,
    stderr: Mutex<RetainedBuffer>,
    output_store: Option<Arc<CommandOutputStore>>,
    pub started_at: Instant,
    created_at: String,
    started_at_wall: String,
    finished_at: Mutex<Option<String>>,
    finished_instant: Mutex<Option<Instant>>,
    last_access: Mutex<Instant>,
    pub exit_code: Mutex<Option<i32>>,
    exited: AtomicBool,
    pub(crate) exit_notify: Arc<Notify>,
    kill_notify: Arc<Notify>,
    pub(super) termination_reason: Mutex<Option<String>>,
    reader_tasks: AsyncMutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl ExecSession {
    pub fn new(child: Child) -> Self {
        Self::new_with_mode(child, false)
    }

    pub fn new_with_mode(mut child: Child, interactive: bool) -> Self {
        let session_id = Uuid::new_v4().to_string();
        let pid = child.id();
        let stdin = child.stdin.take();
        let stdin_open = stdin.is_some();
        Self {
            session_id,
            child: AsyncMutex::new(Some(child)),
            pid,
            stdin: AsyncMutex::new(stdin),
            stdin_open: Mutex::new(stdin_open),
            interactive,
            stdout: Mutex::new(RetainedBuffer::default()),
            stderr: Mutex::new(RetainedBuffer::default()),
            output_store: None,
            started_at: Instant::now(),
            created_at: unix_timestamp(),
            started_at_wall: unix_timestamp(),
            finished_at: Mutex::new(None),
            finished_instant: Mutex::new(None),
            last_access: Mutex::new(Instant::now()),
            exit_code: Mutex::new(None),
            exited: AtomicBool::new(false),
            exit_notify: Arc::new(Notify::new()),
            kill_notify: Arc::new(Notify::new()),
            termination_reason: Mutex::new(None),
            reader_tasks: AsyncMutex::new(Vec::new()),
        }
    }

    pub(super) fn attach_output_store(&mut self, output_store: Arc<CommandOutputStore>) {
        if output_store.prepare_command(&self.session_id) {
            self.output_store = Some(output_store);
        }
    }

    pub async fn spawn_readers(self: &Arc<Self>) {
        let (stdout, stderr, child_opt) = {
            let mut guard = self.child.lock().await;
            let stdout = guard.as_mut().and_then(|child| child.stdout.take());
            let stderr = guard.as_mut().and_then(|child| child.stderr.take());
            let child_opt = guard.take();
            (stdout, stderr, child_opt)
        };
        if let Some(stream) = stdout {
            let session = Arc::clone(self);
            let task = tokio::spawn(async move {
                session.read_stream(stream, true).await;
            });
            self.reader_tasks.lock().await.push(task);
        }
        if let Some(stream) = stderr {
            let session = Arc::clone(self);
            let task = tokio::spawn(async move {
                session.read_stream(stream, false).await;
            });
            self.reader_tasks.lock().await.push(task);
        }
        if let Some(child) = child_opt {
            let session = Arc::clone(self);
            tokio::spawn(async move {
                session.watch_child_exit(child).await;
            });
        }
    }

    async fn watch_child_exit(&self, mut child: Child) {
        let status = tokio::select! {
            result = child.wait() => result.ok(),
            _ = self.kill_notify.notified() => {
                let _ = child.start_kill();
                child.wait().await.ok()
            }
        };
        if let Some(status) = status {
            self.record_exit_status(status);
        } else {
            self.mark_exited_without_status();
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
        let stream_name = if is_stdout { "stdout" } else { "stderr" };
        let mut full_writer = match &self.output_store {
            Some(store) => store
                .open_writer(&self.session_id, stream_name)
                .await
                .map(|file| BufWriter::with_capacity(OUTPUT_WRITER_BUFFER_BYTES, file)),
            None => None,
        };
        let mut buf = [0u8; STREAM_READ_BUFFER_BYTES];
        loop {
            match stream.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = &buf[..n];
                    if is_stdout {
                        let mut data = lock_unpoisoned(&self.stdout);
                        data.append(chunk);
                    } else {
                        let mut data = lock_unpoisoned(&self.stderr);
                        data.append(chunk);
                    }
                    if let Some(writer) = full_writer.as_mut() {
                        if writer.write_all(chunk).await.is_err() {
                            full_writer = None;
                        }
                    }
                }
                Err(_) => break,
            }
        }
        if let Some(writer) = full_writer.as_mut() {
            let _ = writer.flush().await;
        }
    }

    pub(super) fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub(super) async fn request_kill(&self) {
        let mut guard = self.child.lock().await;
        if let Some(child) = guard.as_mut() {
            let _ = child.start_kill();
        } else {
            drop(guard);
            self.kill_notify.notify_one();
        }
    }

    pub(crate) async fn wait_until_exited(&self) {
        if self.has_exited() {
            return;
        }
        let notified = self.exit_notify.notified();
        tokio::pin!(notified);
        // Register in the waiter queue before rechecking `exited` so a concurrent
        // `record_exit_status` cannot be missed.
        notified.as_mut().enable();
        if self.has_exited() {
            return;
        }
        notified.await;
    }

    pub(super) async fn wait_for_exit_timeout(&self, timeout: Duration) {
        if self.has_exited() {
            return;
        }
        let mut guard = self.child.lock().await;
        if let Some(child) = guard.as_mut() {
            let result = tokio::time::timeout(timeout, child.wait()).await;
            drop(guard);
            if let Ok(Ok(exit_status)) = result {
                self.record_exit_status(exit_status);
            }
            return;
        }
        drop(guard);
        let _ = tokio::time::timeout(timeout, self.wait_until_exited()).await;
    }

    pub async fn kill_and_wait(&self) {
        if self.has_exited() {
            return;
        }
        let owned_child_result = {
            let mut guard = self.child.lock().await;
            if let Some(child) = guard.as_mut() {
                let _ = child.start_kill();
                Some(child.wait().await.ok())
            } else {
                None
            }
        };
        if let Some(status_opt) = owned_child_result {
            if let Some(status) = status_opt {
                self.record_exit_status(status);
            } else {
                self.mark_exited_without_status();
            }
            return;
        }
        self.kill_notify.notify_one();
        self.wait_until_exited().await;
    }

    pub async fn refresh_status(&self) {
        if self.has_exited() {
            return;
        }
        let status = {
            let mut guard = self.child.lock().await;
            guard
                .as_mut()
                .and_then(|child| child.try_wait().ok().flatten())
        };
        if let Some(status) = status {
            self.record_exit_status(status);
        }
    }

    pub(super) fn record_exit_status(&self, status: std::process::ExitStatus) {
        *lock_unpoisoned(&self.exit_code) = status.code();
        self.finalize_exit();
    }

    fn mark_exited_without_status(&self) {
        self.finalize_exit();
    }

    fn finalize_exit(&self) {
        *lock_unpoisoned(&self.stdin_open) = false;
        {
            let mut finished = lock_unpoisoned(&self.finished_at);
            if finished.is_none() {
                *finished = Some(unix_timestamp());
                *lock_unpoisoned(&self.finished_instant) = Some(Instant::now());
            }
        }
        {
            let mut reason = lock_unpoisoned(&self.termination_reason);
            if reason.is_none() {
                *reason = Some("exited".into());
            }
        }
        self.exited.store(true, Ordering::Release);
        self.exit_notify.notify_waiters();
    }

    pub(crate) fn has_exited(&self) -> bool {
        self.exited.load(Ordering::Acquire)
    }

    pub(super) fn touch_access(&self) {
        *lock_unpoisoned(&self.last_access) = Instant::now();
    }

    pub(super) fn completed_idle_for(&self, ttl: Duration) -> bool {
        self.has_exited()
            && lock_unpoisoned(&self.last_access).elapsed() >= ttl
            && lock_unpoisoned(&self.finished_instant)
                .as_ref()
                .is_some_and(|done| done.elapsed() >= ttl)
    }

    pub fn mark_termination_reason(&self, reason: &str) {
        *lock_unpoisoned(&self.termination_reason) = Some(reason.to_string());
    }

    pub(crate) fn mark_stdin_closed(&self) {
        *lock_unpoisoned(&self.stdin_open) = false;
    }

    pub async fn is_running(&self) -> bool {
        self.refresh_status().await;
        !self.has_exited()
    }

    pub(super) fn read_retained_page(
        &self,
        stream: &str,
        requested_offset: usize,
        limit: usize,
    ) -> super::buffer::RetainedPage {
        match stream {
            "stderr" => lock_unpoisoned(&self.stderr).read_page(requested_offset, limit),
            _ => lock_unpoisoned(&self.stdout).read_page(requested_offset, limit),
        }
    }

    pub fn snapshot(&self, max_output_bytes: usize) -> Value {
        let (stdout, stdout_total, stdout_head, stdout_tail, stdout_evicted) = {
            let guard = lock_unpoisoned(&self.stdout);
            (
                guard.preview(max_output_bytes),
                guard.total_bytes,
                guard.head.len(),
                guard.tail.len(),
                guard.evicted_bytes(),
            )
        };
        let (stderr, stderr_total, stderr_head, stderr_tail, stderr_evicted) = {
            let guard = lock_unpoisoned(&self.stderr);
            (
                guard.preview(max_output_bytes),
                guard.total_bytes,
                guard.head.len(),
                guard.tail.len(),
                guard.evicted_bytes(),
            )
        };
        let exit_code = *lock_unpoisoned(&self.exit_code);
        let termination_reason = lock_unpoisoned(&self.termination_reason).clone();
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
            "stdin_open": *lock_unpoisoned(&self.stdin_open),
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
            "finished_at": lock_unpoisoned(&self.finished_at).clone(),
            "stdout_retention": {
                "total_bytes": stdout_total,
                "head_retained_bytes": stdout_head,
                "tail_retained_bytes": stdout_tail,
                "evicted_bytes": stdout_evicted
            },
            "stderr_retention": {
                "total_bytes": stderr_total,
                "head_retained_bytes": stderr_head,
                "tail_retained_bytes": stderr_tail,
                "evicted_bytes": stderr_evicted
            },
            "output_refs": {
                "stdout": format!("command:{}:stdout", self.session_id),
                "stderr": format!("command:{}:stderr", self.session_id)
            },
            "raw_output_available": self.output_store.is_some(),
            "legacy_output_refs": {
                "stdout": format!("session:{}:stdout", self.session_id),
                "stderr": format!("session:{}:stderr", self.session_id)
            }
        })
    }
}
