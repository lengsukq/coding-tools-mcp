use std::path::Path;

use serde_json::{json, Value};

mod gates;
mod routing;

use crate::planning::{
    ExecutionLedgerUpdate, PlanningService, PlanningState, PLANNING_RELATIVE_PATH,
};
use crate::tools::context::ToolContext;
use crate::tools::policy::validate_tool_arguments_for_workspace;
use crate::tools::workspace::{tool_err, tool_ok, WorkspaceError};
use gates::{
    load_planning_state, mutating_tool_call, planning_gate, planning_protected_tool,
    policy_tool_err,
};

fn record_execution_ledger(
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

/// **唯一工具执行入口**。MCP `tools/call` 必须且只能调用此函数。
/// 策略校验、分发、错误格式在此统一，传输层不得另做执行前校验。
pub fn call_tool(ctx: &ToolContext, name: &str, args: &Value) -> Value {
    let effective_args = apply_default_cwd(ctx, name, args);
    let planning_state = match load_planning_state(ctx) {
        Ok(state) => Some(state),
        Err(error)
            if planning_protected_tool(name, &effective_args) || name == "exec_health_check" =>
        {
            return error
        }
        Err(_) => None,
    };
    if let Some(state) = planning_state.as_ref() {
        if let Some(error) = planning_gate(state, name, &effective_args) {
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
        record_execution_ledger(ctx, name, args, &output, None);
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

    let result = routing::execute_tool(ctx, name, &effective_args);
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
    record_execution_ledger(ctx, name, &effective_args, &output, task_id.as_deref());
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
    let history_context = crate::tools::history::context_snapshot(ctx).ok().flatten();
    Ok(tool_ok(json!({
        "server": "coding-tools-mcp",
        "title": "Coding Tools MCP",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol_version": crate::mcp::LATEST_PROTOCOL_VERSION,
        "supported_protocol_versions": crate::mcp::SUPPORTED_PROTOCOL_VERSIONS,
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
        "tool_api": crate::tools::registry::tool_api_descriptor(),
        "tools": tools,
        "tool_count": tools.len()
    })))
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

#[cfg(test)]
mod planning_tests {
    use tempfile::tempdir;

    use crate::planning::PlanningMode;

    use super::*;

    fn context() -> (tempfile::TempDir, tempfile::TempDir, ToolContext) {
        let workspace = tempdir().expect("workspace");
        let harness = tempdir().expect("harness");
        let ctx =
            ToolContext::for_test(workspace.path().to_path_buf(), harness.path().to_path_buf())
                .expect("context");
        (workspace, harness, ctx)
    }

    #[test]
    fn plan_mode_blocks_project_mutation_but_keeps_kill_session_available() {
        let (_workspace, _harness, ctx) = context();
        PlanningService::new(ctx.workspace.root())
            .set_mode(PlanningMode::Plan)
            .expect("plan mode");
        let state = PlanningService::new(ctx.workspace.root())
            .state()
            .expect("state");

        let blocked = planning_gate(&state, "apply_patch", &json!({})).expect("blocked");
        assert_eq!(blocked["error"]["code"], "PLAN_MODE_READ_ONLY");
        assert_eq!(
            blocked["error"]["recovery"]["action"],
            "switch_planning_mode"
        );
        assert_eq!(blocked["error"]["retryable"], false);
        assert!(planning_gate(&state, "exec_command", &json!({})).is_some());
        assert!(planning_gate(&state, "kill_session", &json!({})).is_none());
    }

    #[test]
    fn goal_mode_requires_an_active_focused_goal() {
        let (_workspace, _harness, ctx) = context();
        let service = PlanningService::new(ctx.workspace.root());
        service.set_mode(PlanningMode::Goal).expect("goal mode");
        let state = service.state().expect("state");
        let blocked = planning_gate(&state, "apply_patch", &json!({})).expect("blocked");
        assert_eq!(blocked["error"]["code"], "GOAL_CONTEXT_REQUIRED");
        assert_eq!(
            blocked["error"]["recovery"]["action"],
            "fix_planning_context"
        );

        service
            .create_goal("Goal", "Objective", Vec::new(), Vec::new())
            .expect("goal");
        let state = service.state().expect("state");
        assert!(planning_gate(&state, "apply_patch", &json!({})).is_none());
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
        let blocked = planning_gate(&state, "apply_patch", &json!({})).expect("blocked");
        assert_eq!(blocked["error"]["code"], "GOAL_NOT_ACTIVE");
    }

    #[test]
    fn plan_mode_allows_read_only_v2_actions_and_blocks_task_mutation() {
        let (_workspace, _harness, ctx) = context();
        let service = PlanningService::new(ctx.workspace.root());
        service.set_mode(PlanningMode::Plan).expect("plan mode");
        let state = service.state().expect("state");

        assert!(planning_gate(&state, "planning_manage", &json!({"action":"state"})).is_none());
        assert!(planning_gate(&state, "task_manage", &json!({"action":"context"})).is_none());
        assert!(planning_gate(&state, "task_manage", &json!({"action":"start"})).is_some());
    }

    #[test]
    fn plan_mode_allows_direct_plan_create_and_update() {
        let (_workspace, _harness, ctx) = context();
        PlanningService::new(ctx.workspace.root())
            .set_mode(PlanningMode::Plan)
            .expect("plan mode");

        let created = call_tool(
            &ctx,
            "create_plan",
            &json!({
                "title": "Plan-mode writable metadata",
                "objective": "Prove Plan mode can persist Plan records",
                "steps": ["First step"]
            }),
        );
        assert_eq!(created["ok"], true);
        let plan_id = created["plan"]["id"].as_str().expect("plan id");
        let step_id = created["plan"]["steps"][0]["id"].as_str().expect("step id");

        let updated = call_tool(
            &ctx,
            "update_plan",
            &json!({
                "plan_id": plan_id,
                "step_updates": [{"step_id": step_id, "status": "completed"}]
            }),
        );
        assert_eq!(updated["ok"], true);
        assert_eq!(updated["plan"]["steps"][0]["status"], "completed");

        let blocked = call_tool(
            &ctx,
            "apply_patch",
            &json!({
                "patch": "*** Begin Patch\n*** Add File: blocked.txt\n+blocked\n*** End Patch\n"
            }),
        );
        assert_eq!(blocked["error"]["code"], "PLAN_MODE_READ_ONLY");
    }

    #[test]
    fn plan_mode_allows_compact_planning_manager_writes() {
        let (_workspace, _harness, ctx) = context();
        let ctx = ctx.with_tool_profile("compact");
        PlanningService::new(ctx.workspace.root())
            .set_mode(PlanningMode::Plan)
            .expect("plan mode");

        let created = call_tool(
            &ctx,
            "planning_manage",
            &json!({
                "action": "create_plan",
                "title": "Compact Plan-mode metadata",
                "objective": "Prove aggregate planning API remains writable in Plan mode",
                "steps": ["Persist plan"]
            }),
        );
        assert_eq!(created["ok"], true);
        assert_eq!(created["plan"]["status"], "active");
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

    #[test]
    fn history_validation_is_mutating_only_when_repairing() {
        assert!(!mutating_tool_call(
            "history_session_validate",
            &json!({"repair": false})
        ));
        assert!(mutating_tool_call(
            "history_session_validate",
            &json!({"repair": true})
        ));
        assert!(!mutating_tool_call(
            "history_manage",
            &json!({"action": "validate", "repair": false})
        ));
        assert!(mutating_tool_call(
            "history_manage",
            &json!({"action": "validate", "repair": true})
        ));
    }

    #[test]
    fn default_managed_history_does_not_require_harness_baseline_tracking() {
        assert!(!requires_write_baseline(
            "history_session_bootstrap",
            &json!({})
        ));
        assert!(!requires_write_baseline(
            "history_session_validate",
            &json!({"repair": true})
        ));
        assert!(!requires_write_baseline(
            "history_session_checkpoint",
            &json!({"history_dir": "docs/history-session/nested"})
        ));
        assert!(!requires_write_baseline(
            "history_session_checkpoint",
            &json!({"history_dir": "runtime/.coding-tools/history"})
        ));
        assert!(requires_write_baseline(
            "history_session_checkpoint",
            &json!({"history_dir": "docs/custom-history"})
        ));
        assert!(requires_write_baseline(
            "history_manage",
            &json!({
                "action": "validate",
                "repair": true,
                "history_dir": "docs/custom-history"
            })
        ));
    }
}
