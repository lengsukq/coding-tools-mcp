use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::tools::context::ToolContext;
use crate::tools::session::{ExecSession, SessionStore};
use crate::tools::workspace::WorkspaceError;

use super::output::enrich_exec_result;
use super::platform::{command_for_program, platform_command_path};
use super::request::CommandRunOptions;
use super::resolver::parse_and_resolve;

/// How long a timed-out / background session stays readable before map eviction.
const SESSION_EVICT_AFTER_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) async fn run_command(
    ctx: &ToolContext,
    cmd: &str,
    cwd: &Path,
    options: &CommandRunOptions,
) -> Result<Value, WorkspaceError> {
    let start = Instant::now();
    let session = spawn_session(ctx, cmd, cwd, options.tty).await?;
    let deadline = start + options.limit;

    if options.yield_time.is_zero() {
        return Ok(background_snapshot(
            ctx,
            session,
            start,
            deadline,
            cmd,
            cwd,
            options.max_output,
        ));
    }

    write_initial_stdin(&session, options).await?;
    wait_for_command(ctx, session, start, deadline, cmd, cwd, options).await
}

async fn spawn_session(
    ctx: &ToolContext,
    cmd: &str,
    cwd: &Path,
    tty: bool,
) -> Result<Arc<ExecSession>, WorkspaceError> {
    let search_path = ctx.executable_path_env();
    let (program, args) = parse_and_resolve(
        cmd,
        cwd,
        ctx.workspace.root(),
        &ctx.policy,
        search_path.as_deref(),
    )?;
    let mut command = command_for_program(&program, &args);
    if let Some(path) = search_path.as_ref() {
        command.env("PATH", path);
    }
    command
        .current_dir(platform_command_path(cwd))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    #[cfg(windows)]
    command
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONLEGACYWINDOWSSTDIO", "0");

    let child = command.spawn().map_err(|e| WorkspaceError::ToolDetails {
        code: "COMMAND_SPAWN_FAILED",
        message: format!("Failed to start command: {e}"),
        category: "runtime",
        retryable: true,
        details: json!({
            "termination_reason": "spawn_failed",
            "recoverable": true,
            "suggestion": "检查命令路径、权限和运行时环境后重试"
        }),
    })?;
    let session = ctx.sessions.insert(ExecSession::new_with_mode(child, tty));
    session.spawn_readers().await;
    Ok(session)
}

async fn write_initial_stdin(
    session: &Arc<ExecSession>,
    options: &CommandRunOptions,
) -> Result<(), WorkspaceError> {
    if options.tty || options.stdin_text.is_empty() {
        return Ok(());
    }
    let mut stdin_guard = session.stdin.lock().await;
    if let Some(stdin) = stdin_guard.as_mut() {
        use tokio::io::AsyncWriteExt;
        stdin
            .write_all(options.stdin_text.as_bytes())
            .await
            .map_err(|_| WorkspaceError::Tool {
                code: "SESSION_CLOSED",
                message: "Failed to write stdin.".into(),
                category: "runtime",
                retryable: false,
            })?;
        let _ = stdin.shutdown().await;
    }
    *stdin_guard = None;
    session.mark_stdin_closed();
    Ok(())
}

async fn wait_for_command(
    ctx: &ToolContext,
    session: Arc<ExecSession>,
    start: Instant,
    deadline: Instant,
    cmd: &str,
    cwd: &Path,
    options: &CommandRunOptions,
) -> Result<Value, WorkspaceError> {
    loop {
        session.refresh_status().await;
        if session.has_exited() {
            session.wait_for_readers().await;
            let snapshot = session.snapshot(options.max_output);
            ctx.sessions.remove(&session.session_id);
            return Ok(merge_exec_result(snapshot, start, cmd, cwd, false));
        }
        if !options.tty && Instant::now() >= deadline {
            return timeout_result(ctx, session, options.max_output).await;
        }
        if Instant::now() - start >= options.yield_time || options.tty {
            return Ok(background_snapshot(
                ctx,
                session,
                start,
                deadline,
                cmd,
                cwd,
                options.max_output,
            ));
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn timeout_result(
    ctx: &ToolContext,
    session: Arc<ExecSession>,
    max_output: usize,
) -> Result<Value, WorkspaceError> {
    session.mark_termination_reason("timeout");
    session.kill_and_wait().await;
    session.refresh_status().await;
    session.wait_for_readers().await;
    let snapshot = session.snapshot(max_output);
    schedule_session_eviction(ctx.sessions.clone(), session.session_id.clone());
    Err(WorkspaceError::ToolDetails {
        code: "TIMEOUT",
        message: "Command timed out.".into(),
        category: "runtime",
        retryable: true,
        details: json!({
            "termination_reason": "timeout",
            "recoverable": true,
            "suggestion": "读取 output_refs，调整 timeout_ms 后重试",
            "session": snapshot
        }),
    })
}

fn background_snapshot(
    ctx: &ToolContext,
    session: Arc<ExecSession>,
    start: Instant,
    deadline: Instant,
    cmd: &str,
    cwd: &Path,
    max_output: usize,
) -> Value {
    let snapshot = session.snapshot(max_output);
    spawn_timeout_monitor(ctx.sessions.clone(), session, deadline);
    merge_exec_result(snapshot, start, cmd, cwd, true)
}

fn spawn_timeout_monitor(
    sessions: Arc<SessionStore>,
    session: Arc<ExecSession>,
    deadline: Instant,
) {
    tauri::async_runtime::spawn(async move {
        let remaining = deadline.saturating_duration_since(Instant::now());
        tokio::time::sleep(remaining).await;
        session.refresh_status().await;
        if !session.has_exited() {
            session.mark_termination_reason("timeout");
            session.kill_and_wait().await;
            session.refresh_status().await;
            session.wait_for_readers().await;
        }
        schedule_session_eviction(sessions, session.session_id.clone());
    });
}

fn schedule_session_eviction(sessions: Arc<SessionStore>, session_id: String) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SESSION_EVICT_AFTER_TIMEOUT).await;
        sessions.remove(&session_id);
    });
}

fn merge_exec_result(
    mut snapshot: Value,
    start: Instant,
    command: &str,
    cwd: &Path,
    keep_session: bool,
) -> Value {
    if let Some(obj) = snapshot.as_object_mut() {
        let duration_ms = start.elapsed().as_millis();
        obj.insert("command".into(), json!(command));
        obj.insert("resolved_cwd".into(), json!(cwd.display().to_string()));
        obj.insert("duration_ms".into(), json!(duration_ms));
        obj.insert("elapsed_ms".into(), json!(duration_ms));
        obj.insert("transport_ok".into(), Value::Bool(true));
        let command_ok = match obj
            .get("termination_reason")
            .and_then(Value::as_str)
            .unwrap_or("running")
        {
            "exited" => obj
                .get("exit_code")
                .and_then(Value::as_i64)
                .map(|exit_code| exit_code == 0)
                .or(Some(false)),
            "running" => None,
            _ => Some(false),
        };
        obj.insert(
            "command_ok".into(),
            command_ok.map(Value::Bool).unwrap_or(Value::Null),
        );
        obj.insert("execution_mode".into(), json!("direct"));
        obj.insert(
            "warnings".into(),
            json!(if keep_session {
                vec!["session retained for read_output/write_stdin/kill_session"]
            } else {
                vec!["direct execution without shell"]
            }),
        );
    }
    enrich_exec_result(command, &mut snapshot);
    snapshot
}
