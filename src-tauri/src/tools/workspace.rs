use std::path::{Component, Path, PathBuf};

use serde_json::{json, Value};
use thiserror::Error;

pub const DEFAULT_EXCLUDED_NAMES: &[&str] = &[
    ".git",
    ".reference",
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "venv",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    "__pycache__",
];

#[derive(Debug, Clone)]
pub struct ResolvedPath {
    pub display: String,
    pub path: PathBuf,
    pub existed: bool,
}

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("{message}")]
    Tool {
        code: &'static str,
        message: String,
        category: &'static str,
        retryable: bool,
    },
    #[error("{message}")]
    ToolDetails {
        code: &'static str,
        message: String,
        category: &'static str,
        retryable: bool,
        details: Value,
    },
}

impl WorkspaceError {
    pub fn message(&self) -> String {
        match self {
            Self::Tool { message, .. } | Self::ToolDetails { message, .. } => message.clone(),
        }
    }

    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::Tool {
            code: "INVALID_ARGUMENT",
            message: message.into(),
            category: "validation",
            retryable: false,
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::Tool {
            code: "NOT_FOUND",
            message: message.into(),
            category: "not_found",
            retryable: false,
        }
    }

    pub fn absolute_path_denied() -> Self {
        Self::Tool {
            code: "ABSOLUTE_PATH_DENIED",
            message: "Absolute paths are denied.".into(),
            category: "security",
            retryable: false,
        }
    }

    pub fn path_outside_workspace() -> Self {
        Self::Tool {
            code: "PATH_OUTSIDE_WORKSPACE",
            message: "Path escapes the configured workspace.".into(),
            category: "security",
            retryable: false,
        }
    }

    pub fn symlink_escape() -> Self {
        Self::Tool {
            code: "SYMLINK_ESCAPE",
            message: "Path escapes the configured workspace.".into(),
            category: "security",
            retryable: false,
        }
    }

    pub fn not_a_directory(message: impl Into<String>) -> Self {
        Self::Tool {
            code: "NOT_A_DIRECTORY",
            message: message.into(),
            category: "validation",
            retryable: false,
        }
    }

    pub fn to_error_value(&self) -> Value {
        match self {
            Self::Tool {
                code,
                message,
                category,
                retryable,
            } => {
                let details = json!({});
                json!({
                    "code": code,
                    "message": message,
                    "category": category,
                    "retryable": retryable,
                    "details": details,
                    "recovery": recovery_contract_value(code, category, *retryable, &json!({}))
                })
            }
            Self::ToolDetails {
                code,
                message,
                category,
                retryable,
                details,
            } => json!({
                "code": code,
                "message": message,
                "category": category,
                "retryable": retryable,
                "details": details,
                "recovery": recovery_contract_value(code, category, *retryable, details)
            }),
        }
    }
}

fn recovery_contract_value(
    code: &str,
    category: &str,
    retryable: bool,
    details: &Value,
) -> Value {
    let suggestion = details
        .get("suggestion")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);

    let (action, default_instruction, retry_when): (&str, &str, Option<&str>) = match code {
        "PLAN_MODE_READ_ONLY" => (
            "switch_planning_mode",
            "Use read/planning tools, or switch the workspace to Goal/Direct mode before retrying project mutation.",
            Some("after the workspace planning mode changes to Goal or Direct"),
        ),
        "GOAL_CONTEXT_REQUIRED" | "GOAL_CONTEXT_INVALID" | "GOAL_NOT_ACTIVE"
        | "PLAN_CONTEXT_INVALID" | "PLAN_GOAL_MISMATCH" | "PLAN_NOT_EXECUTABLE" => (
            "fix_planning_context",
            "Select or reactivate a compatible Goal/Plan before retrying project mutation.",
            Some("after an active compatible Goal/Plan is focused"),
        ),
        "PLANNING_STATE_UNAVAILABLE" => (
            "repair_or_reset_planning_state",
            "Repair or reset the project planning state before retrying protected mutations.",
            Some("after .coding-tools/planning/state.json is readable again"),
        ),
        "FILE_CHANGED_EXTERNALLY" => (
            "review_external_changes",
            "Review the external workspace changes and refresh or restart the Harness task baseline before continuing.",
            Some("after the external changes are reviewed and the Harness baseline is refreshed"),
        ),
        "BASELINE_STALE" => (
            "refresh_harness_baseline",
            "Refresh or restart the Harness task after reviewing the branch/HEAD change.",
            Some("after the Harness baseline matches the current branch and HEAD"),
        ),
        "TASK_ALREADY_ACTIVE" => (
            "reuse_or_finish_active_task",
            "Reuse, pause, or finish the active Harness task instead of starting a duplicate task.",
            Some("after the active task is reused or finished"),
        ),
        "DANGEROUS_OPERATION_REQUIRES_CONFIRMATION" => (
            "confirm_and_retry",
            "Retry the original operation with confirm=true only after explicit user authorization.",
            Some("after explicit user authorization"),
        ),
        "ELICITATION_UNSUPPORTED" => (
            "continue_without_permission_request",
            "Do not retry request_permissions. Retry the original operation only when its own recovery guidance says it is allowed.",
            None,
        ),
        "TIMEOUT" => (
            "inspect_output_and_retry",
            "Read the retained output first, then retry with an adjusted timeout only if the command should continue.",
            Some("after retained output has been inspected and timeout/runtime conditions are corrected"),
        ),
        "COMMAND_SPAWN_FAILED" => (
            "fix_runtime_and_retry",
            "Check the command path, permissions, and runtime environment before retrying.",
            Some("after the runtime or command-start condition has been corrected"),
        ),
        "SESSION_NOT_FOUND" | "SESSION_CLOSED" => (
            "start_new_command",
            "The referenced command session is unavailable; start a new command instead of repeating the same session operation.",
            Some("after a new command session has been started"),
        ),
        "COMMAND_EVICTED" => (
            "start_new_command",
            "The command finished and its retained output has been evicted. Start a new command if the operation must be run again.",
            Some("after a new command has been started"),
        ),
        _ => match category {
            "validation" => (
                "fix_input",
                "Correct the invalid arguments or content before retrying.",
                Some("after the request has been corrected"),
            ),
            "not_found" => (
                "refresh_or_correct_reference",
                "Check that the referenced path, id, or resource exists and use the current value before retrying.",
                Some("after the missing or stale reference has been corrected"),
            ),
            "security" | "policy" | "permission" => (
                "change_request",
                "Do not repeat the same blocked operation unchanged; adjust the request or satisfy the required permission/context first.",
                Some("after the request or required permission/context changes"),
            ),
            "storage" | "filesystem" => {
                if retryable {
                    (
                        "retry_after_storage_recovers",
                        "Resolve the storage/filesystem condition, then retry the operation.",
                        Some("after the storage/filesystem condition has recovered"),
                    )
                } else {
                    (
                        "repair_state",
                        "Repair the stored state or filesystem input before retrying; repeating the same call unchanged will fail again.",
                        Some("after the stored state or filesystem input has been repaired"),
                    )
                }
            }
            "runtime" => {
                if retryable {
                    (
                        "retry_after_runtime_recovers",
                        "Inspect the runtime condition and retry after it has recovered or been corrected.",
                        Some("after the runtime condition has recovered or been corrected"),
                    )
                } else {
                    (
                        "fix_runtime_request",
                        "Do not repeat the same runtime operation unchanged; correct the command/session/runtime condition first.",
                        Some("after the runtime request or environment has been corrected"),
                    )
                }
            }
            "internal" => {
                if retryable {
                    (
                        "retry_or_report",
                        "Retry once after checking current state; if the failure repeats, report the internal error instead of looping.",
                        Some("after checking current state; stop retrying if the same internal error repeats"),
                    )
                } else {
                    (
                        "report_issue",
                        "Do not retry the same call unchanged; inspect or report the internal error.",
                        None,
                    )
                }
            }
            _ => {
                if retryable {
                    (
                        "retry",
                        "Retry only after checking the reported condition and current state.",
                        Some("after the reported condition has changed or been corrected"),
                    )
                } else {
                    (
                        "change_request",
                        "Do not repeat the same call unchanged; correct the reported condition first.",
                        Some("after the reported condition has changed or been corrected"),
                    )
                }
            }
        },
    };

    json!({
        "action": action,
        "instruction": suggestion.unwrap_or_else(|| default_instruction.to_string()),
        "retry_when": retry_when
    })
}

pub type WorkspaceResult<T> = Result<T, WorkspaceError>;

#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    pub fn new(root: PathBuf) -> WorkspaceResult<Self> {
        let root = root
            .canonicalize()
            .map_err(|_| WorkspaceError::invalid_argument("Workspace root must exist"))?;
        if !root.is_dir() {
            return Err(WorkspaceError::invalid_argument(
                "Workspace root must be a directory",
            ));
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn root_display(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }

    pub fn reject_unsafe_text(&self, raw_path: &str) -> WorkspaceResult<()> {
        if raw_path.is_empty() {
            return Err(WorkspaceError::invalid_argument(
                "Path must be a non-empty string",
            ));
        }
        if raw_path.contains('\0') {
            return Err(WorkspaceError::invalid_argument("Path contains a NUL byte"));
        }
        if raw_path.starts_with('/') || raw_path.starts_with('\\') {
            return Err(WorkspaceError::absolute_path_denied());
        }
        if raw_path.len() >= 2 {
            let bytes = raw_path.as_bytes();
            if bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
                return Err(WorkspaceError::absolute_path_denied());
            }
        }
        for part in Path::new(raw_path).components() {
            if matches!(part, Component::ParentDir) {
                return Err(WorkspaceError::path_outside_workspace());
            }
        }
        Ok(())
    }

    pub fn resolve_existing(&self, raw_path: &str) -> WorkspaceResult<ResolvedPath> {
        self.resolve_existing_at(&self.root, raw_path)
    }

    /// 解析只读路径。显式的绝对路径和 `..` 路径允许指向 Workspace 外部，
    /// 但不会被任何写入工具复用。
    pub fn resolve_read_path(&self, raw_path: &str) -> WorkspaceResult<ResolvedPath> {
        let raw = if raw_path.is_empty() { "." } else { raw_path };
        self.validate_read_text(raw)?;
        let input = Path::new(raw);
        let candidate = if input.is_absolute() {
            input.to_path_buf()
        } else {
            self.root
                .join(raw.replace('/', std::path::MAIN_SEPARATOR_STR))
        };
        let resolved = candidate
            .canonicalize()
            .map_err(|_| WorkspaceError::not_found(format!("Path not found: {raw}")))?;
        let explicit_external = input.is_absolute()
            || input
                .components()
                .any(|part| matches!(part, Component::ParentDir));
        if !explicit_external && candidate.starts_with(&self.root) {
            self.ensure_inside_workspace(&candidate, &resolved)?;
        }
        Ok(ResolvedPath {
            display: relative_display(&self.root, &resolved),
            path: resolved,
            existed: true,
        })
    }

    pub fn resolve_existing_at(
        &self,
        base: &Path,
        raw_path: &str,
    ) -> WorkspaceResult<ResolvedPath> {
        let raw = if raw_path.is_empty() { "." } else { raw_path };
        self.reject_unsafe_text(raw)?;
        let base = self.validate_base(base)?;
        let candidate = base.join(raw.replace('/', std::path::MAIN_SEPARATOR_STR));
        let resolved = candidate
            .canonicalize()
            .map_err(|_| WorkspaceError::not_found(format!("Path not found: {raw}")))?;
        self.ensure_inside_workspace(&candidate, &resolved)?;
        Ok(ResolvedPath {
            display: relative_display(&self.root, &resolved),
            path: resolved,
            existed: true,
        })
    }

    pub fn resolve_for_write(&self, raw_path: &str) -> WorkspaceResult<ResolvedPath> {
        self.reject_unsafe_text(raw_path)?;
        self.reject_protected_write_path(raw_path)?;
        let pure = Path::new(raw_path);
        if pure.file_name().is_none() || raw_path == "." || raw_path == ".." {
            return Err(WorkspaceError::invalid_argument("Invalid write target"));
        }
        let candidate = self
            .root
            .join(raw_path.replace('/', std::path::MAIN_SEPARATOR_STR));
        if candidate.exists() || candidate.is_symlink() {
            let resolved = candidate
                .canonicalize()
                .map_err(|_| WorkspaceError::not_found(format!("Path not found: {raw_path}")))?;
            self.ensure_inside_workspace(&candidate, &resolved)?;
            return Ok(ResolvedPath {
                display: relative_display(&self.root, &resolved),
                path: resolved,
                existed: true,
            });
        }
        let parent = candidate.parent().unwrap_or(&self.root);
        let resolved_parent = if parent.exists() {
            parent
                .canonicalize()
                .map_err(|_| WorkspaceError::not_found("Parent directory not found"))?
        } else {
            self.ensure_parent_chain(parent)?;
            parent.to_path_buf()
        };
        if !resolved_parent.starts_with(&self.root) {
            return Err(WorkspaceError::path_outside_workspace());
        }
        Ok(ResolvedPath {
            display: raw_path.replace('\\', "/"),
            path: candidate,
            existed: false,
        })
    }

    fn ensure_parent_chain(&self, parent: &Path) -> WorkspaceResult<()> {
        let mut cursor = parent;
        while !cursor.exists() {
            if cursor == self.root || cursor.parent() == Some(cursor) {
                break;
            }
            cursor = cursor.parent().unwrap_or(cursor);
        }
        if cursor.exists() {
            let resolved = cursor
                .canonicalize()
                .map_err(|_| WorkspaceError::not_found("Parent directory not found"))?;
            if !resolved.starts_with(&self.root) {
                return Err(WorkspaceError::path_outside_workspace());
            }
        }
        Ok(())
    }

    fn validate_base(&self, base: &Path) -> WorkspaceResult<PathBuf> {
        let resolved = base
            .canonicalize()
            .map_err(|_| WorkspaceError::not_found("Base path not found"))?;
        if !resolved.is_dir() {
            return Err(WorkspaceError::not_a_directory("Base is not a directory"));
        }
        if !resolved.starts_with(&self.root) {
            return Err(WorkspaceError::path_outside_workspace());
        }
        Ok(resolved)
    }

    fn ensure_inside_workspace(&self, candidate: &Path, resolved: &Path) -> WorkspaceResult<()> {
        if !resolved.starts_with(&self.root) {
            if candidate.is_symlink() {
                return Err(WorkspaceError::symlink_escape());
            }
            return Err(WorkspaceError::path_outside_workspace());
        }
        Ok(())
    }

    pub fn reject_write_symlink(&self, raw_path: &str) -> WorkspaceResult<()> {
        self.reject_unsafe_text(raw_path)?;
        let candidate = self
            .root
            .join(raw_path.replace('/', std::path::MAIN_SEPARATOR_STR));
        if candidate.is_symlink() {
            return Err(WorkspaceError::symlink_escape());
        }
        Ok(())
    }

    pub fn reject_protected_write_path(&self, raw_path: &str) -> WorkspaceResult<()> {
        let normalized = raw_path.replace('\\', "/");
        let first = normalized.split('/').next().unwrap_or("");
        if matches!(first, ".git" | ".github") {
            return Err(WorkspaceError::Tool {
                code: "PROTECTED_PATH",
                message: format!("禁止普通文件操作写入受保护目录: {raw_path}"),
                category: "security",
                retryable: false,
            });
        }
        Ok(())
    }

    fn validate_read_text(&self, raw_path: &str) -> WorkspaceResult<()> {
        if raw_path.contains('\0') {
            return Err(WorkspaceError::invalid_argument("Path contains a NUL byte"));
        }
        Ok(())
    }

    pub fn is_ignored_path(
        &self,
        path: &Path,
        include_hidden: bool,
        include_ignored: bool,
    ) -> bool {
        let Ok(scan_path) = path.strip_prefix(&self.root) else {
            // Workspace 外的读取路径不套用 Workspace 内部的隐藏/构建目录过滤，
            // 否则 Windows 临时目录等路径会被误判为隐藏目录而无法读取。
            return false;
        };
        let parts: Vec<String> = scan_path
            .components()
            .filter_map(|part| match part {
                Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        if !include_hidden {
            for part in &parts {
                if part.starts_with('.') && part != "." {
                    return true;
                }
            }
        }
        if !include_ignored {
            for part in &parts {
                if DEFAULT_EXCLUDED_NAMES.contains(&part.as_str()) {
                    return true;
                }
            }
        }
        false
    }

    pub fn is_safe_read_path(&self, path: &Path) -> bool {
        path.exists() || path.is_symlink()
    }
}

pub fn relative_display(root: &Path, path: &Path) -> String {
    let display = path
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().replace('\\', "/"));
    #[cfg(windows)]
    {
        if let Some(unc) = display.strip_prefix("//?/UNC/") {
            return format!("//{unc}");
        }
        if let Some(normal) = display.strip_prefix("//?/") {
            return normal.to_string();
        }
    }
    display
}

pub fn tool_ok(mut value: Value) -> Value {
    if value.get("ok").is_none() {
        value
            .as_object_mut()
            .expect("tool result object")
            .insert("ok".into(), Value::Bool(true));
    }
    value
}

pub fn tool_err(error: WorkspaceError) -> Value {
    json!({
        "ok": false,
        "status": "error",
        "summary": error.message(),
        "error": error.to_error_value()
    })
}

/// content 文本总预算（字节）；超出按 UTF-8 安全边界截断，完整结果仍在 structuredContent。
const CONTENT_TEXT_BUDGET: usize = 32 * 1024;
/// 执行类结果中 stdout/stderr 各自的 head+tail 预览预算（字节）。
const EXEC_STREAM_PREVIEW_BYTES: usize = 8 * 1024;

const CONTENT_TRUNCATION_MARKER: &str =
    "\n...[content budget truncated; full result available in structuredContent]";

pub fn wrap_mcp_tool_result(tool_name: &str, args: &Value, structured: Value) -> Value {
    let is_error = structured.get("ok").and_then(Value::as_bool) == Some(false);
    let content = if tool_name == "view_image"
        && args
            .get("output")
            .and_then(Value::as_str)
            .unwrap_or("mcp_image")
            == "mcp_image"
        && !is_error
    {
        vec![json!({
            "type": "image",
            "data": structured.get("base64").and_then(Value::as_str).unwrap_or(""),
            "mimeType": structured
                .get("mime_type")
                .and_then(Value::as_str)
                .unwrap_or("application/octet-stream")
        })]
    } else {
        let text = concise_content(tool_name, &structured);
        vec![json!({
            "type": "text",
            "text": text
        })]
    };
    json!({
        "content": content,
        "structuredContent": structured,
        "isError": is_error
    })
}

/// 渲染面向 Agent 的简洁 content：保留可直接推理的主体字段，去掉冗余元数据；
/// structuredContent 保留完整机器结果。部分 Connector 只消费 content，因此 content 必须自足。
fn concise_content(tool_name: &str, structured: &Value) -> String {
    if structured.get("ok").and_then(Value::as_bool) == Some(false) {
        return apply_content_budget(error_content(tool_name, structured));
    }
    let body = match tool_name {
        "read_file" => read_file_content(structured),
        "search_text" | "grep_text" => search_text_content(structured),
        "list_dir" | "list_files" => listing_content(structured),
        "exec_command" | "write_stdin" | "read_output" | "kill_session" => {
            exec_content(structured)
        }
        "git_status" => git_status_content(structured),
        "git_diff" => git_diff_content(structured),
        "git_log" => git_log_content(structured),
        "git_show" => git_show_content(structured),
        "git_blame" => git_blame_content(structured),
        "apply_patch" => patch_content(structured),
        _ => structured.to_string(),
    };
    apply_content_budget(body)
}

fn apply_content_budget(body: String) -> String {
    let limit = CONTENT_TEXT_BUDGET.saturating_sub(CONTENT_TRUNCATION_MARKER.len());
    if body.len() <= limit {
        return body;
    }
    let mut end = limit;
    while end > 0 && !body.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{}", &body[..end], CONTENT_TRUNCATION_MARKER)
}

fn str_field<'a>(structured: &'a Value, key: &str) -> &'a str {
    structured.get(key).and_then(Value::as_str).unwrap_or("")
}

fn u64_field(structured: &Value, key: &str) -> u64 {
    structured.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn bool_field(structured: &Value, key: &str) -> bool {
    structured.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn error_content(tool_name: &str, structured: &Value) -> String {
    let summary = str_field(structured, "summary");
    let error = structured.get("error");
    let code = error
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let category = error
        .and_then(|error| error.get("category"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let retryable = error
        .and_then(|error| error.get("retryable"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let recovery = error.and_then(|error| error.get("recovery"));
    let instruction = recovery
        .and_then(|recovery| recovery.get("instruction"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let retry_when = recovery
        .and_then(|recovery| recovery.get("retry_when"))
        .and_then(Value::as_str)
        .unwrap_or("");

    let mut lines = Vec::new();
    if summary.is_empty() && code.is_empty() {
        lines.push(format!("{tool_name} failed"));
    } else if code.is_empty() {
        lines.push(format!("{tool_name} failed: {summary}"));
    } else {
        lines.push(format!("{tool_name} failed [{code}]: {summary}"));
    }
    lines.push(format!("Category: {category}"));
    lines.push(format!("Retryable: {retryable}"));
    if !instruction.is_empty() {
        lines.push(format!("Recovery: {instruction}"));
    }
    if !retry_when.is_empty() {
        lines.push(format!("Retry when: {retry_when}"));
    } else if !retryable {
        lines.push("Do not repeat this call unchanged.".to_string());
    }
    lines.join("\n")
}

fn read_file_content(structured: &Value) -> String {
    let path = str_field(structured, "path");
    let start = u64_field(structured, "start_line");
    let end = u64_field(structured, "end_line");
    let total = u64_field(structured, "total_lines");
    let mut out = format!("{path} (lines {start}-{end} of {total}):\n");
    out.push_str(str_field(structured, "content"));
    if bool_field(structured, "truncated") {
        let next = u64_field(structured, "next_start_line");
        let hint = if next > 0 {
            format!(" continue with start_line={next}")
        } else {
            String::new()
        };
        out.push_str(&format!("\n[content truncated;{hint} full metadata in structuredContent]"));
    }
    out
}

fn search_text_content(structured: &Value) -> String {
    let query = str_field(structured, "query");
    let total = u64_field(structured, "total_matches");
    let mut out = format!("search \"{query}\": {total} match(es)");
    if let Some(matches) = structured.get("matches").and_then(Value::as_array) {
        for matched in matches {
            let path = str_field(matched, "path");
            let line = u64_field(matched, "line");
            let preview = str_field(matched, "preview");
            out.push_str(&format!("\n{path}:{line}: {preview}"));
        }
    }
    if bool_field(structured, "truncated") {
        out.push_str("\n[result limit reached; scan stopped early]");
    }
    out
}

fn listing_content(structured: &Value) -> String {
    let path = str_field(structured, "path");
    let items = structured
        .get("entries")
        .or_else(|| structured.get("files"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut out = format!("{path}: {} item(s)", items.len());
    for item in &items {
        let item_path = str_field(item, "path");
        let item_type = str_field(item, "type");
        let size = u64_field(item, "size_bytes");
        out.push_str(&format!("\n{item_type} {item_path} ({size}B)"));
    }
    if bool_field(structured, "truncated") {
        out.push_str("\n[listing truncated by limit]");
    }
    out
}

fn exec_content(structured: &Value) -> String {
    let status = str_field(structured, "status");
    let exit_code = structured
        .get("exit_code")
        .map(|value| match value {
            Value::Null => "-".to_string(),
            other => other.to_string(),
        })
        .unwrap_or_else(|| "-".to_string());
    let reason = str_field(structured, "termination_reason");
    let command_id = {
        let command_id = str_field(structured, "command_id");
        if command_id.is_empty() {
            str_field(structured, "session_id")
        } else {
            command_id
        }
    };
    let mut out =
        format!("exit_code={exit_code} status={status} reason={reason} command={command_id}");
    append_stream_section(&mut out, "stdout", structured);
    append_stream_section(&mut out, "stderr", structured);
    out
}

fn append_stream_section(out: &mut String, stream: &str, structured: &Value) {
    let text = str_field(structured, stream);
    if text.is_empty() {
        return;
    }
    let truncated_flag = bool_field(structured, &format!("{stream}_truncated"));
    out.push_str(&format!("\n--- {stream} ({}B) ---\n", text.len()));
    if text.len() > EXEC_STREAM_PREVIEW_BYTES {
        let head_budget = EXEC_STREAM_PREVIEW_BYTES / 4;
        let tail_budget = EXEC_STREAM_PREVIEW_BYTES - head_budget;
        let mut head_end = head_budget.min(text.len());
        while head_end > 0 && !text.is_char_boundary(head_end) {
            head_end -= 1;
        }
        let mut tail_start = text.len().saturating_sub(tail_budget);
        while tail_start < text.len() && !text.is_char_boundary(tail_start) {
            tail_start += 1;
        }
        let omitted = tail_start.saturating_sub(head_end);
        out.push_str(&format!(
            "{}\n...[{omitted}B omitted from middle; showing head+tail of {stream}]...\n{}",
            &text[..head_end],
            &text[tail_start..]
        ));
    } else {
        out.push_str(text);
    }
    if truncated_flag {
        out.push_str(&format!("\n[{stream} truncated; command runtime retains head+tail. Use read_output and next_offset to page retained segments]"));
    }
}

fn git_status_content(structured: &Value) -> String {
    let branch = str_field(structured, "branch");
    let head = str_field(structured, "head");
    let clean = bool_field(structured, "clean");
    let mut out = format!("branch={branch} head={head} clean={clean}");
    if let Some(entries) = structured.get("entries").and_then(Value::as_array) {
        for entry in entries {
            let index_status = str_field(entry, "index_status");
            let worktree_status = str_field(entry, "worktree_status");
            let path = str_field(entry, "path");
            out.push_str(&format!("\n{index_status}{worktree_status} {path}"));
        }
    }
    out
}

fn git_diff_content(structured: &Value) -> String {
    let files = structured
        .get("files")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let mut out = format!("diff over {files} file(s):");
    let diff = str_field(structured, "diff");
    if diff.is_empty() {
        out.push_str("\n(no changes)");
    } else {
        out.push('\n');
        out.push_str(diff);
    }
    out
}

fn git_log_content(structured: &Value) -> String {
    let ref_name = str_field(structured, "ref");
    let mut out = format!("git log {ref_name}:");
    if let Some(commits) = structured.get("commits").and_then(Value::as_array) {
        for commit in commits {
            let short_hash = str_field(commit, "short_hash");
            let date = str_field(commit, "author_date");
            let author = str_field(commit, "author_name");
            let subject = str_field(commit, "subject");
            out.push_str(&format!("\n{short_hash} {date} {author}: {subject}"));
        }
    }
    out
}

fn git_show_content(structured: &Value) -> String {
    let rev = str_field(structured, "rev");
    let mut out = format!("git show {rev}:");
    let content = str_field(structured, "content");
    if content.is_empty() {
        out.push_str("\n(no output)");
    } else {
        out.push('\n');
        out.push_str(content);
    }
    out
}

fn git_blame_content(structured: &Value) -> String {
    let path = str_field(structured, "path");
    let mut out = format!("git blame {path}:");
    if let Some(lines) = structured.get("lines").and_then(Value::as_array) {
        for row in lines {
            let line = u64_field(row, "line");
            let text = str_field(row, "content");
            let author = str_field(row, "author");
            let commit = str_field(row, "commit");
            let short = &commit[..commit.len().min(8)];
            out.push_str(&format!("\n{line}: {text} ({short} {author})"));
        }
    }
    out
}

fn patch_content(structured: &Value) -> String {
    let dry_run = bool_field(structured, "dry_run");
    let mode = if dry_run { "preflight" } else { "applied" };
    let change_id = str_field(structured, "change_id");
    let mut out = if change_id.is_empty() {
        format!("apply_patch {mode}")
    } else {
        format!("apply_patch {mode} change_id={change_id}")
    };
    let summary = str_field(structured, "summary");
    if !summary.is_empty() {
        out.push_str(&format!("\n{summary}"));
    }
    if let Some(files) = structured.get("affected_files").and_then(Value::as_array) {
        for file in files {
            if let Some(path) = file.as_str() {
                out.push_str(&format!("\n- {path}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod content_budget_tests {
    use serde_json::json;

    use super::{
        apply_content_budget, concise_content, tool_err, wrap_mcp_tool_result, WorkspaceError,
        CONTENT_TEXT_BUDGET, CONTENT_TRUNCATION_MARKER,
    };

    fn content_text(result: &serde_json::Value) -> String {
        result["content"][0]["text"].as_str().expect("text").to_string()
    }

    #[test]
    fn large_read_file_content_stays_within_budget() {
        let huge = "中文内容".repeat(64 * 1024);
        let structured = json!({
            "ok": true,
            "path": "src/big.rs",
            "content": huge,
            "start_line": 1,
            "end_line": 100,
            "total_lines": 100,
            "truncated": false
        });
        let result = wrap_mcp_tool_result("read_file", &json!({}), structured.clone());
        let text = content_text(&result);
        assert!(text.len() <= CONTENT_TEXT_BUDGET);
        assert!(text.ends_with(CONTENT_TRUNCATION_MARKER));
        assert!(text.starts_with("src/big.rs (lines 1-100 of 100):"));
        // structuredContent 保持全量不变。
        assert_eq!(result["structuredContent"], structured);
    }

    #[test]
    fn exec_content_keeps_head_and_tail_preview_and_summary() {
        let stdout = format!("HEAD_MARKER{}TAIL_MARKER", "A".repeat(64 * 1024));
        let structured = json!({
            "ok": true,
            "status": "exited",
            "termination_reason": "exited",
            "exit_code": 0,
            "command_id": "c-1",
            "session_id": "c-1",
            "stdout": stdout,
            "stderr": "boom",
            "stdout_truncated": true,
            "stderr_truncated": false
        });
        let text = concise_content("exec_command", &structured);
        assert!(text.starts_with("exit_code=0 status=exited reason=exited command=c-1"));
        assert!(text.contains("HEAD_MARKER"));
        assert!(text.contains("TAIL_MARKER"));
        assert!(text.contains("showing head+tail"));
        assert!(text.contains("boom"));
        assert!(text.contains("Use read_output and next_offset"));
        assert!(text.len() <= CONTENT_TEXT_BUDGET);
    }

    #[test]
    fn search_content_renders_hits_without_metadata() {
        let structured = json!({
            "ok": true,
            "query": "needle",
            "total_matches": 1,
            "matches": [{
                "path": "src/lib.rs",
                "line": 42,
                "column": 1,
                "preview": "let needle = 1;"
            }],
            "truncated": false,
            "max_file_bytes": 123456
        });
        let text = concise_content("search_text", &structured);
        assert_eq!(text, "search \"needle\": 1 match(es)\nsrc/lib.rs:42: let needle = 1;");
    }

    #[test]
    fn error_content_includes_recovery_guidance_for_agent() {
        let structured = tool_err(WorkspaceError::ToolDetails {
            code: "PLAN_MODE_READ_ONLY",
            message: "apply_patch is disabled while this workspace is in Plan mode".into(),
            category: "permission",
            retryable: false,
            details: json!({
                "suggestion": "Switch the workspace to Goal/Direct mode before retrying."
            }),
        });
        let text = concise_content("read_file", &structured);
        assert!(text.contains("read_file failed [PLAN_MODE_READ_ONLY]"));
        assert!(text.contains("Category: permission"));
        assert!(text.contains("Retryable: false"));
        assert!(text.contains("Recovery: Switch the workspace to Goal/Direct mode before retrying."));
        assert!(text.contains("Retry when: after the workspace planning mode changes to Goal or Direct"));
    }

    #[test]
    fn workspace_error_always_contains_recovery_contract() {
        let error = WorkspaceError::invalid_argument("bad input").to_error_value();
        assert_eq!(error["code"], "INVALID_ARGUMENT");
        assert_eq!(error["recovery"]["action"], "fix_input");
        assert!(error["recovery"]["instruction"].as_str().is_some());
        assert!(error["recovery"].get("retry_when").is_some());
    }

    #[test]
    fn harness_conflict_recovery_tells_agent_when_retry_is_safe() {
        let error = WorkspaceError::Tool {
            code: "FILE_CHANGED_EXTERNALLY",
            message: "workspace changed".into(),
            category: "permission",
            retryable: true,
        }
        .to_error_value();
        assert_eq!(error["recovery"]["action"], "review_external_changes");
        assert_eq!(error["retryable"], true);
        assert!(error["recovery"]["retry_when"]
            .as_str()
            .unwrap_or_default()
            .contains("Harness baseline"));
    }

    #[test]
    fn view_image_branch_is_unchanged() {
        let structured = json!({
            "ok": true,
            "base64": "aGVsbG8=",
            "mime_type": "image/png"
        });
        let result = wrap_mcp_tool_result("view_image", &json!({}), structured);
        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["data"], "aGVsbG8=");
    }

    #[test]
    fn budget_truncation_is_utf8_safe() {
        let text = "测".repeat(CONTENT_TEXT_BUDGET);
        let capped = apply_content_budget(text);
        assert!(capped.len() <= CONTENT_TEXT_BUDGET);
        assert!(capped.ends_with(CONTENT_TRUNCATION_MARKER));
    }
}
