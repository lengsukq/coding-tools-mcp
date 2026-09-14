use serde_json::{json, Value};

use crate::planning::{
    GoalStatus, PlanStatus, PlanningMode, PlanningService, PlanningState, PLANNING_RELATIVE_PATH,
};
use crate::tools::context::ToolContext;
use crate::tools::manage;
use crate::tools::policy::PolicyError;
use crate::tools::workspace::{tool_err, WorkspaceError};

pub(super) fn policy_tool_err(err: PolicyError) -> Value {
    let dangerous = err
        .0
        .strip_prefix("DANGEROUS_OPERATION_REQUIRES_CONFIRMATION: ");
    let protected = err.0.strip_prefix("PROTECTED_REPOSITORY_ASSET: ");
    let code = if protected.is_some() {
        "PROTECTED_REPOSITORY_ASSET"
    } else if dangerous.is_some() {
        "DANGEROUS_OPERATION_REQUIRES_CONFIRMATION"
    } else {
        "POLICY_REJECTED"
    };
    let message = protected.or(dangerous).unwrap_or(&err.0).to_string();
    let (reason, suggestion) = if dangerous.is_some() {
        (
            "confirmation_required",
            "为危险操作补充 confirm=true，确认后再重试",
        )
    } else if message.contains("allowlisted") {
        ("command_rejected", "改用允许的命令，或调整工作区命令白名单")
    } else if message.contains("Shell chaining") {
        (
            "shell_syntax_rejected",
            "移除未加引号的 shell 操作符；引号内的程序参数可以保留",
        )
    } else {
        ("policy_rejected", "根据错误信息修正参数后重试")
    };
    tool_err(WorkspaceError::ToolDetails {
        code,
        message,
        category: "policy",
        retryable: false,
        details: json!({
            "stage": "policy",
            "reason": reason,
            "recoverable": reason != "confirmation_required",
            "suggestion": suggestion
        }),
    })
}

pub(super) fn mutating_tool_call(name: &str, args: &Value) -> bool {
    if name == "history_session_validate" {
        return args.get("repair").and_then(Value::as_bool).unwrap_or(false);
    }
    manage::action_is_mutating(name, args)
        .unwrap_or_else(|| crate::tools::registry::static_tool_is_mutating(name))
}

pub(super) fn planning_protected_tool(name: &str, args: &Value) -> bool {
    const EXEMPT: &[&str] = &[
        "history_session_bootstrap",
        "history_session_checkpoint",
        "history_session_validate",
        "create_goal",
        "update_goal",
        "create_plan",
        "update_plan",
        "kill_session",
        "set_default_cwd",
    ];
    if name == "history_manage" || name == "planning_manage" {
        return false;
    }
    mutating_tool_call(name, args) && !EXEMPT.contains(&name)
}

fn plan_mode_blocks_tool(name: &str, args: &Value) -> bool {
    planning_protected_tool(name, args) || name == "exec_health_check"
}

pub(super) fn load_planning_state(ctx: &ToolContext) -> Result<PlanningState, Value> {
    PlanningService::new(ctx.workspace.root())
        .state()
        .map_err(|error| {
            tool_err(WorkspaceError::ToolDetails {
                code: "PLANNING_STATE_UNAVAILABLE",
                message: format!("Cannot read project planning state: {error}"),
                category: "storage",
                retryable: false,
                details: json!({
                    "storage_path": PLANNING_RELATIVE_PATH,
                    "fail_closed_for_mutations": true
                }),
            })
        })
}

pub(super) fn planning_gate(state: &PlanningState, name: &str, args: &Value) -> Option<Value> {
    if !planning_protected_tool(name, args) && state.mode != PlanningMode::Plan {
        return None;
    }
    match state.mode {
        PlanningMode::Direct => None,
        PlanningMode::Plan if plan_mode_blocks_tool(name, args) => {
            Some(tool_err(WorkspaceError::ToolDetails {
                code: "PLAN_MODE_READ_ONLY",
                message: format!("{name} is disabled while this workspace is in Plan mode"),
                category: "permission",
                retryable: false,
                details: json!({
                    "mode": "plan",
                    "revision": state.revision,
                    "suggestion": "Use read/planning tools, or switch the workspace to Goal/Direct mode from the desktop app."
                }),
            }))
        }
        PlanningMode::Plan => None,
        PlanningMode::Goal => goal_mode_gate(state, name),
    }
}

fn goal_mode_gate(state: &PlanningState, name: &str) -> Option<Value> {
    let Some(goal_id) = state.focus_goal_id.as_deref() else {
        return Some(planning_permission_error(
            "GOAL_CONTEXT_REQUIRED",
            format!("{name} requires an active Goal while Goal mode is enabled"),
            state,
            "Select an active Goal from the desktop app before modifying the project.",
        ));
    };
    let Some(goal) = state.goals.iter().find(|goal| goal.id == goal_id) else {
        return Some(planning_permission_error(
            "GOAL_CONTEXT_INVALID",
            format!("Focused Goal {goal_id} no longer exists"),
            state,
            "Select another Goal from the desktop app.",
        ));
    };
    if goal.status != GoalStatus::Active {
        return Some(planning_permission_error(
            "GOAL_NOT_ACTIVE",
            format!("Focused Goal '{}' is {:?}", goal.title, goal.status),
            state,
            "Resume/select an active Goal from the desktop app before modifying the project.",
        ));
    }
    if let Some(plan_id) = state.focus_plan_id.as_deref() {
        let Some(plan) = state.plans.iter().find(|plan| plan.id == plan_id) else {
            return Some(planning_permission_error(
                "PLAN_CONTEXT_INVALID",
                format!("Focused Plan {plan_id} no longer exists"),
                state,
                "Select another Plan from the desktop app.",
            ));
        };
        if plan.goal_id.as_deref() != Some(goal_id) {
            return Some(planning_permission_error(
                "PLAN_GOAL_MISMATCH",
                "Focused Plan does not belong to the focused Goal".into(),
                state,
                "Select a Plan linked to the active Goal.",
            ));
        }
        if !matches!(plan.status, PlanStatus::Active | PlanStatus::Draft) {
            return Some(planning_permission_error(
                "PLAN_NOT_EXECUTABLE",
                format!("Focused Plan '{}' is {:?}", plan.title, plan.status),
                state,
                "Activate the Plan or clear the focused Plan before modifying the project.",
            ));
        }
    }
    None
}

fn planning_permission_error(
    code: &'static str,
    message: String,
    state: &PlanningState,
    suggestion: &str,
) -> Value {
    tool_err(WorkspaceError::ToolDetails {
        code,
        message,
        category: "permission",
        retryable: false,
        details: json!({
            "mode": state.mode,
            "revision": state.revision,
            "focus_goal_id": state.focus_goal_id,
            "focus_plan_id": state.focus_plan_id,
            "suggestion": suggestion
        }),
    })
}
