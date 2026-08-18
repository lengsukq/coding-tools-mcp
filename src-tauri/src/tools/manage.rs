use serde_json::Value;

use crate::tools::workspace::WorkspaceError;
use crate::tools::ToolContext;

use super::{history, planning};

fn action<'a>(args: &'a Value, label: &str) -> Result<&'a str, WorkspaceError> {
    args.get("action")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| WorkspaceError::invalid_argument(format!("{label} action is required")))
}

pub fn history_manage(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    match action(args, "history")? {
        "bootstrap" => history::bootstrap(ctx, args),
        "checkpoint" => history::checkpoint(ctx, args),
        "validate" => history::validate(ctx, args),
        "search" => history::search(ctx, args),
        "read" => history::read(ctx, args),
        other => Err(WorkspaceError::invalid_argument(format!(
            "Unknown history action: {other}"
        ))),
    }
}

pub fn planning_manage(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    match action(args, "planning")? {
        "state" => planning::planning_state(ctx, args),
        "create_goal" => planning::create_goal(ctx, args),
        "update_goal" => planning::update_goal(ctx, args),
        "create_plan" => planning::create_plan(ctx, args),
        "update_plan" => planning::update_plan(ctx, args),
        "request_goal_review" => planning::request_goal_review(ctx, args),
        "request_plan_review" => planning::request_plan_review(ctx, args),
        other => Err(WorkspaceError::invalid_argument(format!(
            "Unknown planning action: {other}"
        ))),
    }
}

pub fn task_manage(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    let tool_name = match action(args, "task")? {
        "status" => "harness_status",
        "operation_log" => "operation_log",
        "project_state" => "project_state",
        "start" => "start_task",
        "update" => "update_task",
        "pause" => "pause_task",
        "resume" => "resume_task",
        "finish" => "finish_task",
        "context" => "task_context",
        "events" => "list_task_events",
        "change_summary" => "change_summary",
        other => {
            return Err(WorkspaceError::invalid_argument(format!(
                "Unknown task action: {other}"
            )))
        }
    };
    crate::harness::tools::call(ctx, tool_name, args)
}

pub fn action_is_mutating(name: &str, args: &Value) -> Option<bool> {
    let action = args.get("action").and_then(Value::as_str)?;
    match name {
        "history_manage" => Some(matches!(action, "bootstrap" | "checkpoint" | "validate")),
        "planning_manage" => Some(!matches!(action, "state")),
        "task_manage" => Some(matches!(
            action,
            "start" | "update" | "pause" | "resume" | "finish"
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn context() -> (tempfile::TempDir, tempfile::TempDir, ToolContext) {
        let workspace = tempfile::tempdir().expect("workspace");
        let harness = tempfile::tempdir().expect("harness");
        let ctx = ToolContext::for_test(workspace.path().to_path_buf(), harness.path().to_path_buf())
            .expect("context");
        (workspace, harness, ctx)
    }

    #[test]
    fn planning_manager_routes_state_and_create_goal_actions() {
        let (_workspace, _harness, ctx) = context();

        let state = planning_manage(&ctx, &json!({"action":"state"})).expect("state");
        assert_eq!(state["ok"], true);
        assert_eq!(state["state"]["mode"], "direct");

        let created = planning_manage(
            &ctx,
            &json!({
                "action": "create_goal",
                "title": "Stable API",
                "objective": "Route through the v2 manager"
            }),
        )
        .expect("goal");
        assert_eq!(created["ok"], true);
        assert_eq!(created["goal"]["title"], "Stable API");
    }

    #[test]
    fn task_manager_routes_read_only_status_action() {
        let (_workspace, _harness, ctx) = context();
        let status = task_manage(&ctx, &json!({"action":"status"})).expect("status");
        assert_eq!(status["ok"], true);
    }

    #[test]
    fn managers_reject_unknown_actions() {
        let (_workspace, _harness, ctx) = context();
        let error = planning_manage(&ctx, &json!({"action":"explode"})).expect_err("invalid");
        assert_eq!(error.to_error_value()["code"], "INVALID_ARGUMENT");
    }
}
