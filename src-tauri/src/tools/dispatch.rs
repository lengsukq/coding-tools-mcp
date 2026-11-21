//! Unified MCP tool execution pipeline (`call_tool`).

use serde_json::{json, Value};

mod cwd;
mod gates;
mod ledger;
mod routing;
#[cfg(test)]
mod tests;

pub use cwd::{check_exec_environment, get_default_cwd, server_info, set_default_cwd};

use crate::harness::OperationRecordInput;
use crate::planning::{PlanningService, PlanningState};
use crate::tools::context::ToolContext;
use crate::tools::policy::validate_tool_arguments_for_workspace;
use crate::tools::workspace::{tool_err, WorkspaceError};
use cwd::apply_default_cwd;
use gates::{
    load_planning_state, mutating_tool_call, planning_gate, planning_protected_tool,
    policy_tool_err,
};
use ledger::{
    attach_harness_status, attach_planning_context, attach_recovery_guidance,
    attach_standalone_metadata, record_execution_ledger, should_attach_planning_context,
};

/// **唯一工具执行入口**。MCP `tools/call` 必须且只能调用此函数。
/// 策略校验、分发、错误格式在此统一，传输层不得另做执行前校验。
pub fn call_tool(ctx: &ToolContext, name: &str, args: &Value) -> Value {
    let effective_args = apply_default_cwd(ctx, name, args);
    let planning_state = match prepare_planning_stage(ctx, name, &effective_args) {
        Ok(state) => state,
        Err(output) => return output,
    };
    if let Some(output) = policy_stage(ctx, name, &effective_args, planning_state.as_ref()) {
        return output;
    }
    if let Some(output) = harness_tool_stage(ctx, name, args, planning_state.as_ref()) {
        return output;
    }
    let task_id = match begin_task_tracking(ctx, name, args, &effective_args) {
        Ok(task_id) => task_id,
        Err(output) => return output,
    };
    let operation_id = begin_operation_tracking(ctx, name, args, task_id.as_deref());
    let command_snapshot =
        (name == "exec_command").then(|| crate::review::snapshot_text_files(ctx.workspace.root()));

    let result = routing::execute_tool(ctx, name, &effective_args, operation_id.as_deref());
    let mut output = match result {
        Ok(v) => v,
        Err(e) => tool_err(e),
    };
    enrich_execution_output(
        ctx,
        name,
        task_id.as_deref(),
        operation_id.as_deref(),
        &mut output,
    );
    attach_exec_snapshot_review(
        ctx,
        command_snapshot,
        operation_id.as_deref(),
        &effective_args,
        &mut output,
    );
    finish_task_tracking(ctx, name, args, task_id.as_deref(), &output);
    finish_operation_tracking(
        ctx,
        name,
        args,
        task_id.as_deref(),
        operation_id.as_deref(),
        &output,
    );
    record_execution_ledger(ctx, name, &effective_args, &output, task_id.as_deref());
    finish_planning_stage(ctx, name, &effective_args, planning_state, output)
}

fn attach_exec_snapshot_review(
    ctx: &ToolContext,
    before_snapshot: Option<std::collections::BTreeMap<String, String>>,
    operation_id: Option<&str>,
    effective_args: &Value,
    output: &mut Value,
) {
    let command_succeeded = output.get("ok").and_then(Value::as_bool) == Some(true);
    let (Some(before), Some(object)) = (before_snapshot, output.as_object_mut()) else {
        return;
    };
    if !command_succeeded {
        return;
    }
    let after = crate::review::snapshot_text_files(ctx.workspace.root());
    let change_id = uuid::Uuid::new_v4().simple().to_string();
    let session_id = effective_args
        .get("_review_session_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Ok(Some(link)) = crate::review::create_snapshot_review_with_session(
        ctx.workspace.root(),
        &change_id,
        operation_id,
        session_id,
        "exec_command workspace changes",
        &before,
        &after,
    ) {
        object.insert("change_id".into(), Value::String(change_id));
        if let Some(url) = crate::tools::patch::build_review_url(&link.change_id, &link.token) {
            object.insert("review_url".into(), Value::String(url));
        }
    }
}

fn prepare_planning_stage(
    ctx: &ToolContext,
    name: &str,
    effective_args: &Value,
) -> Result<Option<PlanningState>, Value> {
    if ctx.tool_profile.is_compact()
        && !planning_protected_tool(name, effective_args)
        && name != "exec_health_check"
    {
        return Ok(None);
    }
    let state = match load_planning_state(ctx) {
        Ok(state) => Some(state),
        Err(error)
            if planning_protected_tool(name, effective_args) || name == "exec_health_check" =>
        {
            return Err(error)
        }
        Err(_) => None,
    };
    if let Some(planning) = state.as_ref() {
        if let Some(error) = planning_gate(planning, name, effective_args) {
            return Err(attach_planning_context(error, planning));
        }
    }
    Ok(state)
}

fn policy_stage(
    ctx: &ToolContext,
    name: &str,
    effective_args: &Value,
    planning_state: Option<&PlanningState>,
) -> Option<Value> {
    let error = validate_tool_arguments_for_workspace(
        name,
        effective_args,
        &ctx.policy,
        Some(&ctx.workspace),
    )
    .err()?;
    let output = policy_tool_err(error);
    if let Some(state) = planning_state {
        return Some(attach_planning_context(output, state));
    }
    if let Ok(state) = load_planning_state(ctx) {
        return Some(attach_planning_context(output, &state));
    }
    Some(output)
}

fn harness_tool_stage(
    ctx: &ToolContext,
    name: &str,
    args: &Value,
    planning_state: Option<&PlanningState>,
) -> Option<Value> {
    if !crate::harness::tools::TOOL_NAMES.contains(&name) {
        return None;
    }
    let output = match crate::harness::tools::call(ctx, name, args) {
        Ok(value) => value,
        Err(error) => attach_harness_status(ctx, tool_err(error), false),
    };
    record_execution_ledger(ctx, name, args, &output, None);
    Some(
        planning_state
            .map(|state| attach_planning_context(output.clone(), state))
            .unwrap_or(output),
    )
}

fn begin_task_tracking(
    ctx: &ToolContext,
    name: &str,
    args: &Value,
    effective_args: &Value,
) -> Result<Option<String>, Value> {
    if !requires_write_baseline(name, effective_args) {
        return Ok(None);
    }
    let Some(task) = ctx.harness.current_task().ok().flatten() else {
        return Ok(None);
    };
    if let Err(error) = ctx.harness.check_baseline(&task.id) {
        return Err(attach_harness_status(
            ctx,
            tool_err(WorkspaceError::Tool {
                code: error.code(),
                message: error.to_string(),
                category: "permission",
                retryable: matches!(
                    error.code(),
                    "TASK_ALREADY_ACTIVE" | "FILE_CHANGED_EXTERNALLY" | "BASELINE_STALE"
                ),
            }),
            false,
        ));
    }
    let _ = ctx.harness.record_event(
        &task.id,
        "operation_started",
        Some(name),
        operation_input(args),
        json!({"ok": true, "tracking": "task"}),
    );
    Ok(Some(task.id))
}

fn begin_operation_tracking(
    ctx: &ToolContext,
    name: &str,
    args: &Value,
    task_id: Option<&str>,
) -> Option<String> {
    should_log_operation(name)
        .then(|| {
            ctx.harness.record_operation(OperationRecordInput {
                operation_id: None,
                task_id: task_id.map(str::to_string),
                tool: name.to_string(),
                kind: "started".into(),
                input_summary: json!({"arguments_present": !args.is_null()}),
                result_summary: json!({"ok": true}),
            })
        })
        .and_then(Result::ok)
        .map(|operation| operation.id)
}

fn enrich_execution_output(
    ctx: &ToolContext,
    name: &str,
    task_id: Option<&str>,
    operation_id: Option<&str>,
    output: &mut Value,
) {
    if task_id.is_none()
        && standalone_operation(name)
        && output.get("ok") == Some(&Value::Bool(true))
    {
        attach_standalone_metadata(
            output,
            "当前操作已在 standalone 模式完成；如需继续，直接调用下一个开发工具。",
        );
    }
    if let (Some(id), Some(object)) = (operation_id, output.as_object_mut()) {
        object.insert("operation_id".into(), Value::String(id.to_string()));
        if name != "apply_patch" {
            if let Some(change_id) = object
                .get("change_id")
                .and_then(Value::as_str)
                .map(str::to_string)
            {
                let _ = crate::review::attach_operation(ctx.workspace.root(), &change_id, id);
            }
        }
    }
    if output.get("ok").and_then(Value::as_bool) == Some(false) {
        *output = attach_harness_status(ctx, std::mem::take(output), task_id.is_none());
        *output = attach_recovery_guidance(std::mem::take(output));
    }
}

fn finish_task_tracking(
    ctx: &ToolContext,
    name: &str,
    args: &Value,
    task_id: Option<&str>,
    output: &Value,
) {
    let Some(task_id) = task_id else { return };
    let succeeded = output.get("ok").and_then(Value::as_bool) == Some(true);
    let _ = ctx.harness.record_event(
        task_id,
        "operation_finished",
        Some(name),
        operation_input(args),
        json!({"ok": succeeded, "tool": name}),
    );
    if succeeded {
        let _ = ctx.harness.refresh_expected_state(task_id);
    }
}

fn finish_operation_tracking(
    ctx: &ToolContext,
    name: &str,
    args: &Value,
    task_id: Option<&str>,
    operation_id: Option<&str>,
    output: &Value,
) {
    let Some(operation_id) = operation_id else {
        return;
    };
    let succeeded = output.get("ok").and_then(Value::as_bool) == Some(true);
    let _ = ctx.harness.record_operation(OperationRecordInput {
        operation_id: Some(operation_id.to_string()),
        task_id: task_id.map(str::to_string),
        tool: name.to_string(),
        kind: if succeeded { "completed" } else { "failed" }.into(),
        input_summary: operation_input(args),
        result_summary: json!({
            "ok": succeeded,
            "tool": name,
            "affected_files": output.get("affected_files")
        }),
    });
}

fn finish_planning_stage(
    ctx: &ToolContext,
    name: &str,
    effective_args: &Value,
    planning_state: Option<PlanningState>,
    mut output: Value,
) -> Value {
    if should_attach_planning_context(ctx, name, &output) {
        let latest = if !mutating_tool_call(name, effective_args) {
            planning_state.or_else(|| PlanningService::new(ctx.workspace.root()).state().ok())
        } else {
            PlanningService::new(ctx.workspace.root()).state().ok()
        };
        if let Some(latest) = latest {
            output = attach_planning_context(output, &latest);
            if let Some(planning) = output.get("planning_context") {
                ctx.record_context_block("planning_status", planning);
            }
        }
    }
    output
}

pub(super) fn requires_write_baseline(name: &str, args: &Value) -> bool {
    match name {
        "exec_command" => true,
        "apply_patch" => !args
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "history_session_bootstrap" | "history_session_checkpoint" => {
            history_write_affects_baseline(args)
        }
        "history_session_validate" => {
            args.get("repair").and_then(Value::as_bool).unwrap_or(false)
                && history_write_affects_baseline(args)
        }
        "history_manage" => match args.get("action").and_then(Value::as_str) {
            Some("bootstrap" | "checkpoint") => history_write_affects_baseline(args),
            Some("validate") => {
                args.get("repair").and_then(Value::as_bool).unwrap_or(false)
                    && history_write_affects_baseline(args)
            }
            _ => false,
        },
        _ => false,
    }
}

fn history_write_affects_baseline(args: &Value) -> bool {
    let history_dir = args
        .get("history_dir")
        .and_then(Value::as_str)
        .unwrap_or("docs/history-session")
        .replace('\\', "/");
    let history_dir = history_dir.trim_start_matches("./").trim_end_matches('/');
    let managed_default =
        history_dir == "docs/history-session" || history_dir.starts_with("docs/history-session/");
    let managed_runtime = history_dir
        .split('/')
        .any(|component| component == ".coding-tools");
    !managed_default && !managed_runtime
}

fn standalone_operation(name: &str) -> bool {
    matches!(name, "patch_check" | "apply_patch" | "exec_command")
}

fn should_log_operation(name: &str) -> bool {
    standalone_operation(name)
        || matches!(
            name,
            "git_status" | "git_diff" | "git_log" | "git_show" | "git_blame"
        )
}

fn operation_input(args: &Value) -> Value {
    json!({
        "arguments_present": !args.is_null(),
        "reason": args.get("reason")
    })
}
