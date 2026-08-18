use serde::Deserialize;
use serde_json::{json, Value};

use crate::planning::{
    GoalStatus, PlanStatus, PlanStepStatus, PlanningService, PLANNING_RELATIVE_PATH,
};

use super::context::ToolContext;
use super::workspace::{tool_ok, WorkspaceError, WorkspaceResult};

pub fn planning_state(ctx: &ToolContext, _args: &Value) -> WorkspaceResult<Value> {
    let state = service(ctx).state().map_err(storage_error)?;
    Ok(tool_ok(json!({
        "storage_path": PLANNING_RELATIVE_PATH,
        "state": state
    })))
}

pub fn create_goal(ctx: &ToolContext, args: &Value) -> WorkspaceResult<Value> {
    let title = required_string(args, "title")?;
    let objective = required_string(args, "objective")?;
    let planning = service(ctx);
    let goal = planning
        .create_goal(
            title,
            objective,
            string_list(args.get("success_criteria"))?,
            string_list(args.get("constraints"))?,
        )
        .map_err(storage_error)?;
    let goal = planning
        .update_goal(&goal.id, None, None, None, None, None, Some(true))
        .map_err(storage_error)?;
    Ok(tool_ok(json!({
        "goal": goal,
        "focused": true,
        "storage_path": PLANNING_RELATIVE_PATH
    })))
}

pub fn update_goal(ctx: &ToolContext, args: &Value) -> WorkspaceResult<Value> {
    let goal_id = required_string(args, "goal_id")?;
    let status = optional_enum::<GoalStatus>(args, "status")?;
    if matches!(
        status,
        Some(GoalStatus::Completed | GoalStatus::AwaitingAcceptance | GoalStatus::Archived)
    ) {
        return Err(WorkspaceError::invalid_argument(
            "AI cannot complete/archive a Goal directly. Use request_goal_review when work is ready for human acceptance.",
        ));
    }
    let goal = service(ctx)
        .update_goal(
            goal_id,
            optional_string(args, "title"),
            optional_string(args, "objective"),
            status,
            optional_string_list(args.get("constraints"))?,
            optional_string_list(args.get("completed_criteria_ids"))?,
            args.get("focus").and_then(Value::as_bool),
        )
        .map_err(storage_error)?;
    Ok(tool_ok(json!({"goal": goal, "storage_path": PLANNING_RELATIVE_PATH})))
}

pub fn create_plan(ctx: &ToolContext, args: &Value) -> WorkspaceResult<Value> {
    let title = required_string(args, "title")?;
    let objective = required_string(args, "objective")?;
    let planning = service(ctx);
    let plan = planning
        .create_plan(
            optional_string(args, "goal_id"),
            title,
            objective,
            string_list(args.get("steps"))?,
        )
        .map_err(storage_error)?;
    let plan = planning
        .update_plan(&plan.id, Some(PlanStatus::Active), Vec::new(), Some(true))
        .map_err(storage_error)?;
    Ok(tool_ok(json!({
        "plan": plan,
        "focused": true,
        "storage_path": PLANNING_RELATIVE_PATH
    })))
}

pub fn update_plan(ctx: &ToolContext, args: &Value) -> WorkspaceResult<Value> {
    let plan_id = required_string(args, "plan_id")?;
    let status = optional_enum::<PlanStatus>(args, "status")?;
    if matches!(
        status,
        Some(PlanStatus::Completed | PlanStatus::AwaitingAcceptance | PlanStatus::Archived)
    ) {
        return Err(WorkspaceError::invalid_argument(
            "AI cannot complete/archive a Plan directly. Use request_plan_review when work is ready for human acceptance.",
        ));
    }
    let step_updates = args
        .get("step_updates")
        .map(|value| serde_json::from_value::<Vec<StepUpdate>>(value.clone()))
        .transpose()
        .map_err(|error| WorkspaceError::invalid_argument(format!("invalid step_updates: {error}")))?
        .unwrap_or_default()
        .into_iter()
        .map(|update| (update.step_id, update.status, update.notes))
        .collect();
    let plan = service(ctx)
        .update_plan(
            plan_id,
            status,
            step_updates,
            args.get("focus").and_then(Value::as_bool),
        )
        .map_err(storage_error)?;
    Ok(tool_ok(json!({"plan": plan, "storage_path": PLANNING_RELATIVE_PATH})))
}

pub fn request_goal_review(ctx: &ToolContext, args: &Value) -> WorkspaceResult<Value> {
    let goal = service(ctx)
        .request_goal_review(
            required_string(args, "goal_id")?,
            required_string(args, "summary")?,
        )
        .map_err(storage_error)?;
    Ok(tool_ok(json!({
        "goal": goal,
        "awaiting_human_acceptance": true,
        "storage_path": PLANNING_RELATIVE_PATH
    })))
}

pub fn request_plan_review(ctx: &ToolContext, args: &Value) -> WorkspaceResult<Value> {
    let plan = service(ctx)
        .request_plan_review(
            required_string(args, "plan_id")?,
            required_string(args, "summary")?,
        )
        .map_err(storage_error)?;
    Ok(tool_ok(json!({
        "plan": plan,
        "awaiting_human_acceptance": true,
        "storage_path": PLANNING_RELATIVE_PATH
    })))
}

#[derive(Debug, Deserialize)]
struct StepUpdate {
    step_id: String,
    status: PlanStepStatus,
    #[serde(default)]
    notes: Option<String>,
}

fn service(ctx: &ToolContext) -> PlanningService {
    PlanningService::new(ctx.workspace.root())
}

fn required_string<'a>(args: &'a Value, key: &str) -> WorkspaceResult<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| WorkspaceError::invalid_argument(format!("{key} is required")))
}

fn optional_string(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn string_list(value: Option<&Value>) -> WorkspaceResult<Vec<String>> {
    optional_string_list(value).map(Option::unwrap_or_default)
}

fn optional_string_list(value: Option<&Value>) -> WorkspaceResult<Option<Vec<String>>> {
    value
        .map(|value| {
            serde_json::from_value::<Vec<String>>(value.clone())
                .map_err(|error| WorkspaceError::invalid_argument(error.to_string()))
        })
        .transpose()
}

fn optional_enum<T>(args: &Value, key: &str) -> WorkspaceResult<Option<T>>
where
    T: serde::de::DeserializeOwned,
{
    args.get(key)
        .map(|value| {
            serde_json::from_value(value.clone()).map_err(|error| {
                WorkspaceError::invalid_argument(format!("invalid {key}: {error}"))
            })
        })
        .transpose()
}

fn storage_error(error: crate::error::AppError) -> WorkspaceError {
    WorkspaceError::Tool {
        code: "PLANNING_STORE_ERROR",
        message: error.to_string(),
        category: "storage",
        retryable: false,
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn mcp_goal_and_plan_tools_share_project_local_state() {
        let workspace = tempdir().expect("workspace");
        let harness = tempdir().expect("harness");
        let ctx = ToolContext::for_test(
            workspace.path().to_path_buf(),
            harness.path().to_path_buf(),
        )
        .expect("context");

        let goal = create_goal(
            &ctx,
            &json!({
                "title": "Goal mode",
                "objective": "Persist goal state",
                "success_criteria": ["Stored in workspace"]
            }),
        )
        .expect("goal");
        let goal_id = goal["goal"]["id"].as_str().expect("goal id");
        create_plan(
            &ctx,
            &json!({
                "goal_id": goal_id,
                "title": "Plan mode",
                "objective": "Implement Plan",
                "steps": ["Model", "UI"]
            }),
        )
        .expect("plan");

        let state = planning_state(&ctx, &json!({})).expect("state");
        assert_eq!(state["state"]["goals"].as_array().unwrap().len(), 1);
        assert_eq!(state["state"]["plans"].as_array().unwrap().len(), 1);
        assert!(workspace.path().join(PLANNING_RELATIVE_PATH).exists());
    }
}
