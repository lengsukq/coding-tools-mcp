use std::path::Path;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde_json::{json, Value};

use crate::planning::{GoalStatus, PlanStatus, PlanningMode, PlanningService, PlanningState, PLANNING_RELATIVE_PATH};
use crate::tools::context::ToolContext;
use crate::tools::policy::{validate_tool_arguments_for_workspace, PolicyError};
use crate::tools::workspace::{tool_err, tool_err_code, tool_ok, WorkspaceError};
use crate::tools::{exec, file, git, history, image_tool, patch, planning, session, skill};

fn policy_tool_err(err: PolicyError) -> Value {
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

fn capability_health_check(ctx: &ToolContext) -> Value {
    let tools = crate::tools::registry::exposed_tool_names(&ctx.tool_profile);
    let mut hasher = DefaultHasher::new();
    tools.hash(&mut hasher);
    json!({
        "authentication": {
            "status": "available"
        },
        "authorization": {
            "mode": ctx.permission_mode
        },
        "workspace": {
            "path": ctx.workspace.root().display().to_string(),
            "status": "available"
        },
        "capability": {
            "server_tool_count": tools.len(),
            "tool_profile": ctx.tool_profile,
            "tool_fingerprint": format!("{:x}", hasher.finish()),
            "server_version": env!("CARGO_PKG_VERSION")
        },
        "recommendation": "If client tools are missing while server capability is healthy, refresh MCP tool discovery instead of requesting permissions."
    })
}

fn planning_protected_tool(name: &str) -> bool {
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
    crate::tools::registry::MUTATING_TOOLS.contains(&name) && !EXEMPT.contains(&name)
}

fn plan_mode_blocks_tool(name: &str) -> bool {
    planning_protected_tool(name) || name == "exec_health_check"
}

fn load_planning_state(ctx: &ToolContext) -> Result<PlanningState, Value> {
    PlanningService::new(ctx.workspace.root()).state().map_err(|error| {
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

fn planning_gate(state: &PlanningState, name: &str) -> Option<Value> {
    if !planning_protected_tool(name) && state.mode != PlanningMode::Plan {
        return None;
    }
    match state.mode {
        PlanningMode::Direct => None,
        PlanningMode::Plan if plan_mode_blocks_tool(name) => Some(tool_err(WorkspaceError::ToolDetails {
            code: "PLAN_MODE_READ_ONLY",
            message: format!("{name} is disabled while this workspace is in Plan mode"),
            category: "permission",
            retryable: false,
            details: json!({
                "mode": "plan",
                "revision": state.revision,
                "suggestion": "Use read/planning tools, or switch the workspace to Goal/Direct mode from the desktop app."
            }),
        })),
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

/// **唯一工具执行入口**。MCP `tools/call` 与 Actions `POST /actions/{tool}` 必须且只能调用此函数。
/// 策略校验、分发、错误格式在此统一，两路传输层不得另做执行前校验（Actions 仅允许额外的暴露层 `validate_actions_exposure`）。
pub fn call_tool(ctx: &ToolContext, name: &str, args: &Value) -> Value {
    let effective_args = apply_default_cwd(ctx, name, args);
    let planning_state = match load_planning_state(ctx) {
        Ok(state) => Some(state),
        Err(error) if planning_protected_tool(name) || name == "exec_health_check" => return error,
        Err(_) => None,
    };
    if let Some(state) = planning_state.as_ref() {
        if let Some(error) = planning_gate(state, name) {
            return attach_planning_context(error, state);
        }
    }
    if let Err(e) = validate_tool_arguments_for_workspace(
        name,
        &effective_args,
        &ctx.policy,
        Some(&ctx.workspace),
    ) {
        let output = policy_tool_err(e);
        return planning_state
            .as_ref()
            .map(|state| attach_planning_context(output.clone(), state))
            .unwrap_or(output);
    }

    if crate::harness::tools::TOOL_NAMES.contains(&name) {
        let output = match crate::harness::tools::call(ctx, name, args) {
            Ok(value) => value,
            Err(error) => attach_harness_status(ctx, tool_err(error), false),
        };
        return planning_state
            .as_ref()
            .map(|state| attach_planning_context(output.clone(), state))
            .unwrap_or(output);
    }

    let task_id = if requires_write_baseline(name, &effective_args) {
        let task = ctx.harness.current_task().ok().flatten();
        if let Some(task) = task {
            if let Err(error) = ctx.harness.check_baseline(&task.id) {
                return attach_harness_status(
                    ctx,
                    tool_err_code(error.code(), error.to_string(), "permission"),
                    false,
                );
            }
            let _ = ctx.harness.record_event(
                &task.id,
                "operation_started",
                Some(name),
                operation_input(args),
                json!({"ok": true, "tracking": "task"}),
            );
            Some(task.id)
        } else {
            None
        }
    } else {
        None
    };

    let operation = if should_log_operation(name) {
        ctx.harness
            .record_operation(
                None,
                task_id.as_deref(),
                name,
                "started",
                json!({"arguments_present": !args.is_null()}),
                json!({"ok": true}),
            )
            .ok()
    } else {
        None
    };

    let ws = &ctx.workspace;
    let result = match name {
        "history_session_bootstrap" => history::bootstrap(ctx, &effective_args),
        "history_session_checkpoint" => history::checkpoint(ctx, &effective_args),
        "history_session_validate" => history::validate(ctx, &effective_args),
        "history_session_search" => history::search(ctx, &effective_args),
        "history_session_read" => history::read(ctx, &effective_args),
        "capability_health_check" => Ok(capability_health_check(ctx)),
        "planning_state" => planning::planning_state(ctx, &effective_args),
        "create_goal" => planning::create_goal(ctx, &effective_args),
        "update_goal" => planning::update_goal(ctx, &effective_args),
        "create_plan" => planning::create_plan(ctx, &effective_args),
        "update_plan" => planning::update_plan(ctx, &effective_args),
        "request_goal_review" => planning::request_goal_review(ctx, &effective_args),
        "request_plan_review" => planning::request_plan_review(ctx, &effective_args),
        "server_info" => server_info(ctx),
        "check_exec_environment" => check_exec_environment(ctx),
        "exec_health_check" => exec::exec_health_check(ctx),
        "get_default_cwd" => get_default_cwd(ctx),
        "set_default_cwd" => set_default_cwd(ctx, &effective_args),
        "list_skills" => skill::list_skills(ctx, &effective_args),
        "get_skill" => skill::get_skill(ctx, &effective_args),
        "read_file" => file::read_file(ws, &effective_args),
        "list_dir" => file::list_dir(ws, &effective_args),
        "list_files" => file::list_files(ws, &effective_args),
        "search_text" | "grep_text" | "grep" => file::search_text(ws, &effective_args),
        "patch_check" => patch::patch_check(ctx, &effective_args),
        "apply_patch" => patch::apply_patch(ctx, &effective_args),
        "exec_command" => exec::exec_command(ctx, &effective_args),
        "read_output" => session::read_output(&ctx.sessions, &effective_args),
        "write_stdin" => session::write_stdin(&ctx.sessions, &effective_args),
        "kill_session" => session::kill_session(&ctx.sessions, &effective_args),
        "git_status" => git::git_status(ws, &effective_args),
        "git_diff" => git::git_diff(ws, &effective_args),
        "git_log" => git::git_log(ws, &effective_args),
        "git_show" => git::git_show(ws, &effective_args),
        "git_blame" => git::git_blame(ws, &effective_args),
        "view_image" => image_tool::view_image(ws, &effective_args),
        "request_permissions" => {
            if ctx.policy.skip_permission_gates() {
                Ok(tool_ok(json!({
                    "ok": true,
                    "status": "granted",
                    "grant_id": "dangerously-skip-all-permissions",
                    "expires_at": null,
                    "constraints": {
                        "mode": "dangerous",
                        "workspace": ctx.workspace.root_display(),
                        "requested": effective_args
                    },
                    "warnings": [
                        "dangerous permission mode is enabled; permission-gated operations are auto-granted"
                    ]
                })))
            } else {
                Ok(tool_ok(json!({
                    "ok": false,
                    "status": "unsupported",
                    "grant_id": null,
                    "expires_at": null,
                    "next_actions": [
                        "Do not retry request_permissions.",
                        "If the original operation returned DANGEROUS_OPERATION_REQUIRES_CONFIRMATION and the user already explicitly authorized it, retry the original tool with confirm=true."
                    ],
                    "error": {
                        "code": "ELICITATION_UNSUPPORTED",
                        "message": "Permission elicitation is not available for this client. Do not retry request_permissions; it cannot create a persistent grant.",
                        "category": "permission",
                        "retryable": false,
                        "details": { "requested": effective_args }
                    }
                })))
            }
        }
        _ => {
            let mut output = tool_err_code(
                "INVALID_ARGUMENT",
                format!("Unknown tool: {name}"),
                "validation",
            );
            if let Some(object) = output.as_object_mut() {
                object.insert(
                    "recovery".into(),
                    json!({
                        "type": "capability_discovery_check",
                        "message": "If this tool exists on the server but is missing in the client session, refresh MCP tool discovery instead of requesting permissions.",
                        "next_action": "Call capability_health_check and compare the available tool list before retrying."
                    }),
                );
            }
            return planning_state
                .as_ref()
                .map(|state| attach_planning_context(output.clone(), state))
                .unwrap_or(output);
        }
    };
    let mut output = match result {
        Ok(v) => v,
        Err(e) => tool_err(e),
    };
    if task_id.is_none()
        && standalone_operation(name)
        && output.get("ok") == Some(&Value::Bool(true))
    {
        attach_standalone_metadata(
            &mut output,
            "当前操作已在 standalone 模式完成；如需继续，直接调用下一个开发工具。",
        );
    }
    if let Some(operation) = operation.as_ref() {
        if let Some(object) = output.as_object_mut() {
            object.insert("operation_id".into(), Value::String(operation.id.clone()));
        }
    }
    if output.get("ok").and_then(Value::as_bool) == Some(false) {
        output = attach_harness_status(ctx, output, task_id.is_none());
        output = attach_recovery_guidance(output);
    }
    if let Some(task_id) = task_id.as_deref() {
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
    if let Some(operation) = operation {
        let succeeded = output.get("ok").and_then(Value::as_bool) == Some(true);
        let _ = ctx.harness.record_operation(
            Some(&operation.id),
            task_id.as_deref(),
            name,
            if succeeded { "completed" } else { "failed" },
            operation_input(args),
            json!({
                "ok": succeeded,
                "tool": name,
                "affected_files": output.get("affected_files")
            }),
        );
    }
    if should_attach_planning_context(ctx, name, &output) {
        if let Ok(latest) = PlanningService::new(ctx.workspace.root()).state() {
            output = attach_planning_context(output, &latest);
            if let Some(planning) = output.get("planning_context") {
                ctx.record_context_block("planning_status", planning);
            }
        }
    }
    output
}

fn should_attach_planning_context(ctx: &ToolContext, name: &str, output: &Value) -> bool {
    if ctx.tool_profile != "compact" {
        return true;
    }
    let planning_tool = matches!(
        name,
        "planning_state"
            | "create_goal"
            | "update_goal"
            | "create_plan"
            | "update_plan"
            | "request_goal_review"
            | "request_plan_review"
    );
    planning_tool || output.get("ok").and_then(Value::as_bool) == Some(false)
}

fn attach_recovery_guidance(mut output: Value) -> Value {
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

    let needs_capability_recovery =
        code == "INVALID_ARGUMENT" && message.starts_with("Unknown tool:");

    if needs_capability_recovery {
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

fn attach_planning_context(mut output: Value, state: &PlanningState) -> Value {
    let goal = state
        .focus_goal_id
        .as_deref()
        .and_then(|id| state.goals.iter().find(|goal| goal.id == id))
        .map(|goal| {
            let completed = goal.success_criteria.iter().filter(|item| item.completed).count();
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
                .filter(|step| step.status == crate::planning::PlanStepStatus::Completed)
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

fn apply_default_cwd(ctx: &ToolContext, name: &str, args: &Value) -> Value {
    let base = if ctx.default_cwd_path() == ctx.workspace.root() {
        ".".to_string()
    } else {
        ctx.default_cwd_display()
    };
    if base == "." {
        return args.clone();
    }

    let mut effective = args.clone();
    match name {
        "exec_command" if effective.get("workdir").is_none() && effective.get("cwd").is_none() => {
            effective["workdir"] = Value::String(base.clone());
        }
        "list_dir" | "list_files" | "git_status" | "git_log" => {
            let path = effective.get("path").and_then(Value::as_str).unwrap_or(".");
            effective["path"] = Value::String(prefix_relative_path(&base, path));
        }
        "read_file" | "search_text" | "grep_text" | "grep" | "git_blame" | "view_image" => {
            if let Some(path) = effective.get("path").and_then(Value::as_str) {
                effective["path"] = Value::String(prefix_relative_path(&base, path));
            }
        }
        "git_diff" => {
            if let Some(path) = effective.get("path").and_then(Value::as_str) {
                effective["path"] = Value::String(prefix_relative_path(&base, path));
            }
            if let Some(paths) = effective.get("paths").and_then(Value::as_array).cloned() {
                effective["paths"] = Value::Array(
                    paths
                        .iter()
                        .map(|path| {
                            path.as_str()
                                .map(|value| Value::String(prefix_relative_path(&base, value)))
                                .unwrap_or_else(|| path.clone())
                        })
                        .collect(),
                );
            }
        }
        "apply_patch" | "patch_check" => {
            if let Some(patch) = effective.get("patch").and_then(Value::as_str) {
                effective["patch"] = Value::String(prefix_patch_paths(&base, patch));
            }
        }
        _ => {}
    }
    effective
}

fn prefix_relative_path(base: &str, path: &str) -> String {
    if path == "." || path.is_empty() {
        return base.to_string();
    }
    if Path::new(path).is_absolute() || path.starts_with("..") {
        return path.to_string();
    }
    format!("{base}/{}", path.trim_start_matches("./"))
}

fn prefix_patch_paths(base: &str, patch: &str) -> String {
    patch
        .lines()
        .map(|line| {
            for marker in ["--- a/", "+++ b/"] {
                if let Some(path) = line.strip_prefix(marker) {
                    return format!("{marker}{base}/{path}");
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn requires_write_baseline(name: &str, args: &Value) -> bool {
    match name {
        "exec_command" => true,
        "apply_patch" => !args
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        _ => false,
    }
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

fn attach_harness_status(ctx: &ToolContext, mut output: Value, standalone: bool) -> Value {
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

fn attach_standalone_metadata(output: &mut Value, recovery_hint: &str) {
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
    let exposed = crate::tools::registry::exposed_tool_names(&ctx.tool_profile);
    actions
        .into_iter()
        .filter(|action| exposed.contains(&action.as_str()))
        .collect()
}

pub fn server_info(ctx: &ToolContext) -> Result<Value, WorkspaceError> {
    let tools = crate::tools::registry::exposed_tool_names(&ctx.tool_profile);
    let history_context = crate::tools::history::context_snapshot(ctx)
        .ok()
        .flatten();
    Ok(tool_ok(json!({
        "server": "coding-tools-mcp",
        "title": "Coding Tools MCP",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol_version": "2025-06-18",
        "workspace": ctx.workspace.root_display(),
        "permission_mode": ctx.permission_mode,
        "default_cwd": ctx.default_cwd_display(),
        "network_allowed": ctx.policy.network_allowed(),
        "tool_profile": ctx.tool_profile,
        "history_recording": ctx.history_recording,
        "history_context_sessions": ctx.history_context_sessions,
        "history_context_revision": history_context
            .as_ref()
            .and_then(|value| value.get("context_revision")),
        "context_audit": ctx.context_audit_snapshot(),
        "auth_enabled": ctx.auth.auth_enabled(),
        "auth_type": ctx.auth.auth_type,
        "endpoint_path": "/mcp",
        "tools": tools,
        "tool_count": tools.len()
    })))
}

#[cfg(test)]
mod planning_tests {
    use tempfile::tempdir;

    use super::*;

    fn context() -> (tempfile::TempDir, tempfile::TempDir, ToolContext) {
        let workspace = tempdir().expect("workspace");
        let harness = tempdir().expect("harness");
        let ctx = ToolContext::for_test(
            workspace.path().to_path_buf(),
            harness.path().to_path_buf(),
        )
        .expect("context");
        (workspace, harness, ctx)
    }

    #[test]
    fn plan_mode_blocks_project_mutation_but_keeps_kill_session_available() {
        let (_workspace, _harness, ctx) = context();
        PlanningService::new(ctx.workspace.root())
            .set_mode(PlanningMode::Plan)
            .expect("plan mode");
        let state = PlanningService::new(ctx.workspace.root()).state().expect("state");

        let blocked = planning_gate(&state, "apply_patch").expect("blocked");
        assert_eq!(blocked["error"]["code"], "PLAN_MODE_READ_ONLY");
        assert!(planning_gate(&state, "exec_command").is_some());
        assert!(planning_gate(&state, "kill_session").is_none());
    }

    #[test]
    fn goal_mode_requires_an_active_focused_goal() {
        let (_workspace, _harness, ctx) = context();
        let service = PlanningService::new(ctx.workspace.root());
        service.set_mode(PlanningMode::Goal).expect("goal mode");
        let state = service.state().expect("state");
        let blocked = planning_gate(&state, "apply_patch").expect("blocked");
        assert_eq!(blocked["error"]["code"], "GOAL_CONTEXT_REQUIRED");

        service
            .create_goal("Goal", "Objective", Vec::new(), Vec::new())
            .expect("goal");
        let state = service.state().expect("state");
        assert!(planning_gate(&state, "apply_patch").is_none());
    }

    #[test]
    fn goal_mode_blocks_mutation_while_goal_waits_for_human_acceptance() {
        let (_workspace, _harness, ctx) = context();
        let service = PlanningService::new(ctx.workspace.root());
        service.set_mode(PlanningMode::Goal).expect("goal mode");
        let goal = service
            .create_goal("Goal", "Objective", Vec::new(), Vec::new())
            .expect("goal");
        service
            .request_goal_review(&goal.id, "Ready for human acceptance")
            .expect("request review");

        let state = service.state().expect("state");
        let blocked = planning_gate(&state, "apply_patch").expect("blocked");
        assert_eq!(blocked["error"]["code"], "GOAL_NOT_ACTIVE");
    }

    #[test]
    fn every_normal_tool_response_contains_current_planning_context() {
        let (_workspace, _harness, ctx) = context();
        let service = PlanningService::new(ctx.workspace.root());
        service.set_mode(PlanningMode::Plan).expect("plan mode");

        let output = call_tool(&ctx, "server_info", &json!({}));
        assert_eq!(output["planning_context"]["mode"], "plan");
        assert!(output["planning_context"]["revision"].as_u64().is_some());
    }

    #[test]
    fn compact_normal_tool_response_omits_repeated_planning_context() {
        let (_workspace, _harness, ctx) = context();
        let ctx = ctx.with_tool_profile("compact");
        let output = call_tool(&ctx, "server_info", &json!({}));
        assert!(output.get("planning_context").is_none());
        assert!(output["context_audit"]["blocks"].is_array());
    }
}

pub fn check_exec_environment(ctx: &ToolContext) -> Result<Value, WorkspaceError> {
    Ok(tool_ok(json!({
        "workspace": ctx.workspace.root_display(),
        "permission_mode": ctx.permission_mode,
        "network_allowed": ctx.policy.network_allowed(),
        "landlock_enabled": false,
        "filesystem_sandbox": {
            "available": false,
            "enforced": false,
            "default_scope": "workspace",
            "host_scope_available": false
        },
        "global_tmp_write": if ctx.permission_mode == "dangerous" { "allowed" } else { "tmp-prefix" },
        "workspace_exec_available": true,
        "workspace_exec_sandbox_enforced": false,
        "workspace_exec_boundary": "policy_only",
        "system_command_allowlist": ctx.policy.allowed_commands.iter().cloned().collect::<Vec<_>>(),
        "configured_executable_paths": ctx.executable_paths.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
        "workspace_local_entries": {
            "enabled": ctx.policy.workspace_local_entries,
            "script_extensions": ctx.policy.workspace_script_extensions.iter().cloned().collect::<Vec<_>>(),
            "resolution": "workdir_first"
        },
        // Backward-compatible alias for older MCP clients.
        "allowed_commands": ctx.policy.allowed_commands.iter().cloned().collect::<Vec<_>>(),
        "warnings": ["Workspace 子进程当前允许执行，但尚未启用操作系统级文件系统沙箱"]
    })))
}

pub fn get_default_cwd(ctx: &ToolContext) -> Result<Value, WorkspaceError> {
    Ok(tool_ok(json!({
        "workspace": ctx.workspace.root_display(),
        "default_cwd": ctx.default_cwd_display(),
        "resolved_cwd": ctx.default_cwd_path().display().to_string()
    })))
}

pub fn set_default_cwd(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    let path = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let resolved = ctx.workspace.resolve_existing(path)?;
    if !resolved.path.is_dir() {
        return Err(WorkspaceError::not_a_directory(
            "Default cwd must be a directory",
        ));
    }
    ctx.set_default_cwd(resolved.path.clone());
    Ok(tool_ok(json!({
        "workspace": ctx.workspace.root_display(),
        "default_cwd": resolved.display,
        "resolved_cwd": resolved.path.display().to_string()
    })))
}
