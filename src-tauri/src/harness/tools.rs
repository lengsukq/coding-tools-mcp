use serde_json::{json, Value};

use crate::tools::io_util::serialized_json_bytes;
use crate::tools::workspace::{tool_ok, WorkspaceError};
use crate::tools::ToolContext;

use super::model::{HarnessEvent, TaskSession, TaskStatus};
use super::store::HarnessError;

const DEFAULT_TASK_CONTEXT_MAX_BYTES: usize = 32_768;
const MIN_TASK_CONTEXT_MAX_BYTES: usize = 8_192;
const MAX_TASK_CONTEXT_MAX_BYTES: usize = 131_072;
const TASK_CONTEXT_EVENT_FETCH_LIMIT: usize = 100;

pub const TOOL_NAMES: &[&str] = &[
    "harness_status",
    "operation_log",
    "project_state",
    "start_task",
    "update_task",
    "pause_task",
    "resume_task",
    "finish_task",
    "task_context",
    "list_task_events",
    "change_summary",
];

pub fn call(ctx: &ToolContext, name: &str, args: &Value) -> Result<Value, WorkspaceError> {
    let value = match name {
        "harness_status" => harness_status(ctx),
        "operation_log" => operation_log(ctx, args),
        "project_state" => project_state(ctx, args),
        "start_task" => start_task(ctx, args),
        "update_task" => update_task(ctx, args),
        "pause_task" => transition(ctx, args, TaskStatus::Paused),
        "resume_task" => transition(ctx, args, TaskStatus::Active),
        "finish_task" => finish_task(ctx, args),
        "task_context" => task_context(ctx, args),
        "list_task_events" => list_task_events(ctx, args),
        "change_summary" => change_summary(ctx, args),
        _ => return Err(tool_error("INVALID_ARGUMENT", "未知 Harness 工具")),
    }?;
    Ok(tool_ok(value))
}

fn harness_status(ctx: &ToolContext) -> Result<Value, WorkspaceError> {
    serde_json::to_value(ctx.harness.status().map_err(map_error)?)
        .map_err(|e| tool_error("SERIALIZE_FAILED", e.to_string()))
}

fn operation_log(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    let offset = args.get("cursor").and_then(Value::as_u64).unwrap_or(0) as usize;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(50)
        .clamp(1, 200) as usize;
    let operations = ctx
        .harness
        .list_operations(offset, limit)
        .map_err(map_error)?;
    Ok(json!({
        "operations": operations,
        "next_cursor": offset + operations.len()
    }))
}

fn project_state(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    let max_files = args.get("max_files").and_then(Value::as_u64).unwrap_or(200) as usize;
    serde_json::to_value(ctx.harness.project_state(max_files).map_err(map_error)?)
        .map_err(|e| tool_error("SERIALIZE_FAILED", e.to_string()))
}

fn start_task(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    let objective = args
        .get("objective")
        .and_then(Value::as_str)
        .ok_or_else(|| tool_error("INVALID_ARGUMENT", "objective 是必填项"))?;
    let task = ctx.harness.start_task(objective).map_err(map_error)?;
    Ok(json!({"task": task, "next": ["project_state", "task_context"]}))
}

fn update_task(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    let task_id = task_id(args)?;
    let completed_steps = string_list(args.get("completed_steps"))?;
    let pending_steps = string_list(args.get("pending_steps"))?;
    let task = ctx
        .harness
        .update_steps(task_id, completed_steps, pending_steps)
        .map_err(map_error)?;
    Ok(json!({"task": task}))
}

fn transition(
    ctx: &ToolContext,
    args: &Value,
    status: TaskStatus,
) -> Result<Value, WorkspaceError> {
    let task = ctx
        .harness
        .transition(task_id(args)?, status)
        .map_err(map_error)?;
    Ok(json!({"task": task}))
}

fn finish_task(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    let task_id = task_id(args)?;
    let allow_unverified = args
        .get("allow_unverified")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let finish_summary = optional_non_empty_string(args, "summary", "summary 必须是字符串")?;
    let status = if allow_unverified {
        TaskStatus::CompletedUnverified
    } else {
        TaskStatus::Verifying
    };
    let task = ctx.harness.transition(task_id, status).map_err(map_error)?;
    ctx.harness
        .record_event(
            task_id,
            "task_finished",
            Some("finish_task"),
            json!({
                "summary": finish_summary,
                "allow_unverified": allow_unverified
            }),
            json!({"ok": true, "status": status}),
        )
        .map_err(map_error)?;
    let summary = change_summary(ctx, &json!({"task_id": task_id}))?;
    let mut payload = json!({"task": task, "change_summary": summary});
    if let Some(finish_summary) = finish_summary {
        payload["summary"] = Value::String(finish_summary);
    }
    Ok(payload)
}

fn task_context(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    let max_bytes = args
        .get("max_bytes")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_TASK_CONTEXT_MAX_BYTES as u64)
        .clamp(
            MIN_TASK_CONTEXT_MAX_BYTES as u64,
            MAX_TASK_CONTEXT_MAX_BYTES as u64,
        ) as usize;
    let task = if let Some(task_id) = args.get("task_id").and_then(Value::as_str) {
        Some(ctx.harness.task(task_id).map_err(map_error)?)
    } else {
        ctx.harness.current_task().map_err(map_error)?
    };
    let Some(task) = task else {
        return Ok(json!({"task": null, "message": "当前没有活动任务"}));
    };
    let mut events = ctx
        .harness
        .list_events(&task.id, 0, TASK_CONTEXT_EVENT_FETCH_LIMIT + 1)
        .map_err(map_error)?;
    let mut truncated = false;
    if events.len() > TASK_CONTEXT_EVENT_FETCH_LIMIT {
        events.truncate(TASK_CONTEXT_EVENT_FETCH_LIMIT);
        truncated = true;
    }
    let (events, budget_truncated) = trim_events_to_budget(&task, events, max_bytes);
    truncated |= budget_truncated;
    Ok(json!({"task": task, "events": events, "truncated": truncated}))
}

fn trim_events_to_budget(
    task: &TaskSession,
    events: Vec<HarnessEvent>,
    max_bytes: usize,
) -> (Vec<HarnessEvent>, bool) {
    let base_bytes = serialized_json_bytes(&json!({
        "ok": true,
        "task": task,
        "events": [],
        "truncated": false
    }))
    .unwrap_or(0);
    let remaining_bytes = max_bytes.saturating_sub(base_bytes);

    // Keep the most recent events that fit within `max_bytes`, preserving chronological order.
    let mut kept = Vec::new();
    let mut used_bytes = 0usize;
    let mut truncated = false;
    for event in events.into_iter().rev() {
        let separator = usize::from(!kept.is_empty());
        let event_bytes = serialized_json_bytes(&event)
            .unwrap_or(0)
            .saturating_add(separator);
        if used_bytes.saturating_add(event_bytes) <= remaining_bytes {
            used_bytes += event_bytes;
            kept.push(event);
        } else {
            truncated = true;
            break;
        }
    }
    kept.reverse();
    (kept, truncated)
}

fn list_task_events(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    let task_id = task_id(args)?;
    let offset = args.get("cursor").and_then(Value::as_u64).unwrap_or(0) as usize;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(50)
        .clamp(1, 200) as usize;
    let events = ctx
        .harness
        .list_events(task_id, offset, limit)
        .map_err(map_error)?;
    Ok(json!({"events": events, "next_cursor": offset + events.len()}))
}

fn change_summary(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    let requested_change_id =
        optional_non_empty_string(args, "change_id", "change_id 必须是字符串")?;
    if let Some(change_id) = requested_change_id.as_deref() {
        if !is_valid_change_id(change_id) {
            return Err(tool_error("INVALID_ARGUMENT", "change_id 格式无效"));
        }
    }
    let task = if let Some(task_id) = args.get("task_id").and_then(Value::as_str) {
        ctx.harness.task(task_id).map_err(map_error)?
    } else {
        ctx.harness
            .current_task()
            .map_err(map_error)?
            .ok_or_else(|| tool_error("TASK_STATE_REQUIRED", "没有可总结的活动任务"))?
    };
    let state = ctx.harness.project_state(200).map_err(map_error)?;
    let files = state
        .files
        .iter()
        .filter(|file| file.status != "unchanged")
        .cloned()
        .collect::<Vec<_>>();
    let events = ctx
        .harness
        .list_events(&task.id, 0, 100)
        .map_err(map_error)?;
    let effective_change_id = requested_change_id.or_else(|| task.latest_change_id.clone());
    let review = effective_change_id
        .as_deref()
        .and_then(|id| crate::review::review_summary(ctx.workspace.root(), id));

    let mut payload = json!({
        "task_id": task.id,
        "change_id": effective_change_id,
        "objective": task.objective,
        "why": {"text": task.objective, "source": "task_objective"},
        "files": files,
        "evidence": events,
        "verification": [],
        "risks": [],
        "rollback_capability": "not_available_in_foundation"
    });
    if let Some(review) = review {
        payload["review"] = json!(review);
    }
    Ok(payload)
}

fn is_valid_change_id(change_id: &str) -> bool {
    !change_id.is_empty()
        && change_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
}

fn optional_non_empty_string(
    args: &Value,
    key: &str,
    type_error_message: &str,
) -> Result<Option<String>, WorkspaceError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => {
            let trimmed = value.trim();
            Ok((!trimmed.is_empty()).then(|| trimmed.to_string()))
        }
        Some(_) => Err(tool_error("INVALID_ARGUMENT", type_error_message)),
    }
}

fn task_id(args: &Value) -> Result<&str, WorkspaceError> {
    args.get("task_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| tool_error("INVALID_ARGUMENT", "task_id 是必填项"))
}

fn string_list(value: Option<&Value>) -> Result<Option<Vec<String>>, WorkspaceError> {
    let Some(value) = value else { return Ok(None) };
    let list = value
        .as_array()
        .ok_or_else(|| tool_error("INVALID_ARGUMENT", "步骤必须是字符串数组"))?
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_string)
                .ok_or_else(|| tool_error("INVALID_ARGUMENT", "步骤必须是字符串数组"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(list))
}

fn map_error(error: HarnessError) -> WorkspaceError {
    tool_error(error.code(), error.to_string())
}

fn tool_error(code: &'static str, message: impl Into<String>) -> WorkspaceError {
    let category = match code {
        "INVALID_ARGUMENT" | "INVALID_TASK_TRANSITION" => "validation",
        "STORE_UNAVAILABLE" | "STORE_IO_FAILED" | "STORE_SERIALIZE_FAILED" | "STORE_CORRUPT" => {
            "storage"
        }
        "WORKSPACE_UNAVAILABLE" => "runtime",
        _ => "permission",
    };
    WorkspaceError::Tool {
        code,
        message: message.into(),
        category,
        retryable: matches!(
            code,
            "TASK_ALREADY_ACTIVE"
                | "FILE_CHANGED_EXTERNALLY"
                | "BASELINE_STALE"
                | "STORE_UNAVAILABLE"
                | "STORE_IO_FAILED"
                | "WORKSPACE_UNAVAILABLE"
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn task_context_trims_events_to_max_bytes_and_marks_truncated() {
        let workspace = tempdir().expect("workspace");
        let harness_root = tempdir().expect("harness");
        fs::write(workspace.path().join("main.rs"), "fn main() {}\n").expect("file");
        let ctx = ToolContext::for_test(
            workspace.path().to_path_buf(),
            harness_root.path().to_path_buf(),
        )
        .expect("context");

        let started =
            call(&ctx, "start_task", &json!({"objective": "测试预算裁剪"})).expect("start");
        let task_id = started["task"]["id"].as_str().expect("task_id");
        let padding = "x".repeat(1200);
        for index in 0..20 {
            ctx.harness
                .record_event(
                    task_id,
                    "progress",
                    Some("test_tool"),
                    json!({"index": index, "padding": padding}),
                    json!({"ok": true}),
                )
                .expect("record event");
        }

        let context = call(
            &ctx,
            "task_context",
            &json!({"task_id": task_id, "max_bytes": MIN_TASK_CONTEXT_MAX_BYTES}),
        )
        .expect("task_context");
        assert_eq!(context["truncated"], json!(true));
        let events = context["events"].as_array().expect("events array");
        assert!(!events.is_empty());
        assert!(events.len() < 21);
        let serialized_len = serialized_json_bytes(&json!({
            "task": &context["task"],
            "events": &context["events"],
            "truncated": &context["truncated"]
        }))
        .expect("serialized len");
        assert!(serialized_len <= MIN_TASK_CONTEXT_MAX_BYTES);
    }

    #[test]
    fn finish_task_records_summary_and_change_summary_handles_change_id() {
        let workspace = tempdir().expect("workspace");
        let harness_root = tempdir().expect("harness");
        fs::write(workspace.path().join("main.rs"), "fn main() {}\n").expect("file");
        let ctx = ToolContext::for_test(
            workspace.path().to_path_buf(),
            harness_root.path().to_path_buf(),
        )
        .expect("context");

        let started =
            call(&ctx, "start_task", &json!({"objective": "测试完成摘要"})).expect("start");
        let task_id = started["task"]["id"].as_str().expect("task_id");

        let review = crate::review::create_patch_review(
            workspace.path(),
            "change-1234",
            None,
            "updated main.rs",
            &[(
                "main.rs".into(),
                "update".into(),
                "fn main() {}\n".into(),
                "fn main() { println!(\"ok\"); }\n".into(),
            )],
        )
        .expect("patch review");

        let summary_with_change = call(
            &ctx,
            "change_summary",
            &json!({"task_id": task_id, "change_id": review.change_id}),
        )
        .expect("change_summary");
        assert_eq!(summary_with_change["change_id"], json!("change-1234"));
        assert_eq!(summary_with_change["review"]["id"], json!("change-1234"));

        let invalid = call(
            &ctx,
            "change_summary",
            &json!({"task_id": task_id, "change_id": "../escape"}),
        );
        assert!(invalid.is_err());

        let finished = call(
            &ctx,
            "finish_task",
            &json!({
                "task_id": task_id,
                "summary": "所有单测与检查均已通过",
                "allow_unverified": true
            }),
        )
        .expect("finish_task");
        assert_eq!(finished["summary"], json!("所有单测与检查均已通过"));
        let evidence = finished["change_summary"]["evidence"]
            .as_array()
            .expect("evidence");
        assert!(evidence.iter().any(|event| {
            event["kind"] == json!("task_finished")
                && event["input_summary"]["payload"]["summary"] == json!("所有单测与检查均已通过")
        }));
    }
}
