use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};

use crate::tools::context::ToolContext;
use crate::tools::workspace::WorkspaceError;

#[derive(Debug, Clone)]
pub(super) struct ExecRequest {
    pub cmd: String,
    pub workdir: PathBuf,
    pub filesystem_scope: String,
    pub options: CommandRunOptions,
}

#[derive(Debug, Clone)]
pub(super) struct CommandRunOptions {
    pub limit: Duration,
    pub yield_time: Duration,
    pub max_output: usize,
    pub tty: bool,
    pub stdin_text: String,
}

impl ExecRequest {
    pub fn parse(ctx: &ToolContext, args: &Value) -> Result<Self, WorkspaceError> {
        let cmd = required_command(args)?;
        let workdir = resolve_workdir(ctx, args)?;
        let filesystem_scope = filesystem_scope(args)?;
        Ok(Self {
            cmd,
            workdir,
            filesystem_scope,
            options: CommandRunOptions::from_args(args),
        })
    }
}

impl CommandRunOptions {
    fn from_args(args: &Value) -> Self {
        let timeout_ms = args
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .unwrap_or(30_000);
        let yield_ms = args
            .get("yield_time_ms")
            .and_then(Value::as_u64)
            .unwrap_or(1000)
            .min(30_000);
        Self {
            limit: Duration::from_millis(timeout_ms),
            yield_time: Duration::from_millis(yield_ms),
            max_output: args
                .get("max_output_bytes")
                .and_then(Value::as_u64)
                .unwrap_or(32_768) as usize,
            tty: args.get("tty").and_then(Value::as_bool).unwrap_or(false),
            stdin_text: args
                .get("stdin")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }
    }
}

fn required_command(args: &Value) -> Result<String, WorkspaceError> {
    args.get("cmd")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| WorkspaceError::invalid_argument("cmd is required"))
}

fn resolve_workdir(ctx: &ToolContext, args: &Value) -> Result<PathBuf, WorkspaceError> {
    let raw = args
        .get("workdir")
        .or_else(|| args.get("cwd"))
        .and_then(Value::as_str)
        .unwrap_or(".");
    let resolved = ctx.workspace.resolve_existing(raw)?;
    if !resolved.path.is_dir() {
        return Err(WorkspaceError::not_a_directory(
            "workdir is not a directory",
        ));
    }
    Ok(resolved.path)
}

fn filesystem_scope(args: &Value) -> Result<String, WorkspaceError> {
    let scope = args
        .get("filesystem_scope")
        .and_then(Value::as_str)
        .unwrap_or("workspace");
    match scope {
        "workspace" => Ok(scope.to_string()),
        "host" => Err(WorkspaceError::ToolDetails {
            code: "EXTERNAL_EXECUTION_NOT_ALLOWED",
            message: "exec_command 只允许在 Workspace 内执行，Workspace 外执行已禁用。".into(),
            category: "permission",
            retryable: false,
            details: json!({
                "stage": "policy",
                "filesystem_scope": "host",
                "sandbox_enforced": false,
                "recoverable": false,
                "suggestion": "将 filesystem_scope 设置为 workspace，并在当前 Workspace 内执行"
            }),
        }),
        _ => Err(WorkspaceError::invalid_argument(
            "filesystem_scope must be workspace",
        )),
    }
}
