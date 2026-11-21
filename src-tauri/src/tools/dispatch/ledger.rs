use serde_json::{json, Value};

use crate::planning::{
    ExecutionLedgerUpdate, PlanStepStatus, PlanningService, PlanningState, PLANNING_RELATIVE_PATH,
};
use crate::tools::context::ToolContext;

use super::gates::mutating_tool_call;

pub(super) fn record_execution_ledger(
    ctx: &ToolContext,
    name: &str,
    args: &Value,
    output: &Value,
    tracked_task_id: Option<&str>,
) {
    if !mutating_tool_call(name, args)
        && !matches!(
            name,
            "start_task" | "update_task" | "pause_task" | "resume_task" | "finish_task"
        )
    {
        return;
    }
    let succeeded = output.get("ok").and_then(Value::as_bool) != Some(false);
    let task_id = tracked_task_id
        .map(str::to_string)
        .or_else(|| {
            args.get("task_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| {
            output
                .get("task")
                .and_then(|task| task.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    let last_error = output
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let changed_files = output
        .get("affected_files")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.as_str()
                        .map(str::to_string)
                        .or_else(|| item.get("path").and_then(Value::as_str).map(str::to_string))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let is_checkpoint = name == "history_session_checkpoint"
        || (name == "history_manage"
            && args.get("action").and_then(Value::as_str) == Some("checkpoint"));
    let history_checkpoint_ref = is_checkpoint
        .then(|| {
            output
                .get("path")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .flatten();
    let verification = args
        .get("tests")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let last_tool = args
        .get("action")
        .and_then(Value::as_str)
        .map(|action| format!("{name}:{action}"))
        .unwrap_or_else(|| name.to_string());

    let _ = PlanningService::new(ctx.workspace.root()).record_execution(ExecutionLedgerUpdate {
        task_id,
        last_tool: Some(last_tool),
        state: Some(if succeeded { "completed" } else { "failed" }.into()),
        last_error,
        changed_files,
        history_checkpoint_ref,
        verification,
    });
}

pub(super) fn should_attach_planning_context(
    ctx: &ToolContext,
    name: &str,
    output: &Value,
) -> bool {
    if !ctx.tool_profile.is_compact() {
        return true;
    }
    let planning_tool = matches!(
        name,
        "planning_manage"
            | "planning_state"
            | "create_goal"
            | "update_goal"
            | "create_plan"
            | "update_plan"
            | "request_goal_review"
            | "request_plan_review"
    );
    planning_tool || output.get("ok").and_then(Value::as_bool) == Some(false)
}

pub(super) fn attach_recovery_guidance(mut output: Value) -> Value {
    let Some(error) = output.get("error") else {
        return output;
    };
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let code = error
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or_default();

    if code == "INVALID_ARGUMENT" && message.starts_with("Unknown tool:") {
        if let Some(object) = output.as_object_mut() {
            object.insert(
                "recovery".into(),
                json!({
                    "type": "capability_discovery_mismatch",
                    "automatic_action": "refresh_tool_discovery",
                    "retry_recommended": true,
                    "user_message": "MCP capability is temporarily out of sync. Refreshing available tools is recommended before retrying."
                }),
            );
        }
    }

    output
}

pub(super) fn attach_planning_context(mut output: Value, state: &PlanningState) -> Value {
    let goal = state
        .focus_goal_id
        .as_deref()
        .and_then(|id| state.goals.iter().find(|goal| goal.id == id))
        .map(|goal| {
            let completed = goal
                .success_criteria
                .iter()
                .filter(|item| item.completed)
                .count();
            json!({
                "id": goal.id,
                "title": goal.title,
                "status": goal.status,
                "criteria_completed": completed,
                "criteria_total": goal.success_criteria.len()
            })
        });
    let plan = state
        .focus_plan_id
        .as_deref()
        .and_then(|id| state.plans.iter().find(|plan| plan.id == id))
        .map(|plan| {
            let completed = plan
                .steps
                .iter()
                .filter(|step| step.status == PlanStepStatus::Completed)
                .count();
            json!({
                "id": plan.id,
                "title": plan.title,
                "status": plan.status,
                "revision": plan.revision,
                "steps_completed": completed,
                "steps_total": plan.steps.len()
            })
        });
    if let Some(object) = output.as_object_mut() {
        object.insert(
            "planning_context".into(),
            json!({
                "mode": state.mode,
                "revision": state.revision,
                "storage_path": PLANNING_RELATIVE_PATH,
                "goal": goal,
                "plan": plan
            }),
        );
    }
    output
}

pub(super) fn attach_harness_status(
    ctx: &ToolContext,
    mut output: Value,
    standalone: bool,
) -> Value {
    if let Ok(mut status) = ctx.harness.status() {
        if standalone && status.task_id.is_none() {
            status.next_actions.clear();
        }
        status.next_actions = filter_exposed_actions(ctx, status.next_actions);
        if let Some(object) = output.as_object_mut() {
            object.insert(
                "harness".into(),
                serde_json::to_value(status).unwrap_or_else(|_| {
                    json!({
                        "status": "unavailable",
                        "reason": "无法序列化 Harness 状态"
                    })
                }),
            );
            if standalone {
                attach_standalone_metadata(
                    &mut output,
                    "命令未成功；请检查 stderr、exit_code 或调整参数后重试。",
                );
            }
        }
    }
    output
}

pub(super) fn attach_standalone_metadata(output: &mut Value, recovery_hint: &str) {
    if let Some(object) = output.as_object_mut() {
        object.insert("harness_mode".into(), Value::String("standalone".into()));
        object.insert("task_required".into(), Value::Bool(false));
        object.insert("next_actions".into(), json!([]));
        object.insert(
            "recovery_hint".into(),
            Value::String(recovery_hint.to_string()),
        );
    }
}

fn filter_exposed_actions(ctx: &ToolContext, actions: Vec<String>) -> Vec<String> {
    let exposed = crate::tools::registry::exposed_tool_names_cached(ctx.tool_profile);
    actions
        .into_iter()
        .filter(|action| exposed.contains(&action.as_str()))
        .collect()
}
