use serde_json::{json, Value};

use crate::tools::error::WorkspaceError;

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
        "exec_command" | "write_stdin" | "read_output" | "kill_session" => exec_content(structured),
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
    structured
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or(false)
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
        out.push_str(&format!(
            "\n[content truncated;{hint} full metadata in structuredContent]"
        ));
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
mod tests {
    use serde_json::json;

    use super::{
        apply_content_budget, concise_content, tool_err, wrap_mcp_tool_result, CONTENT_TEXT_BUDGET,
        CONTENT_TRUNCATION_MARKER,
    };
    use crate::tools::error::WorkspaceError;

    fn content_text(result: &serde_json::Value) -> String {
        result["content"][0]["text"]
            .as_str()
            .expect("text")
            .to_string()
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
        assert_eq!(
            text,
            "search \"needle\": 1 match(es)\nsrc/lib.rs:42: let needle = 1;"
        );
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
        assert!(
            text.contains("Recovery: Switch the workspace to Goal/Direct mode before retrying.")
        );
        assert!(text
            .contains("Retry when: after the workspace planning mode changes to Goal or Direct"));
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
