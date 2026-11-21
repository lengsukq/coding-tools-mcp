use serde_json::json;
use tempfile::tempdir;

use crate::planning::{PlanningMode, PlanningService};
use crate::tools::context::ToolContext;

use super::gates::{mutating_tool_call, planning_gate};
use super::{call_tool, requires_write_baseline};

fn context() -> (tempfile::TempDir, tempfile::TempDir, ToolContext) {
    let workspace = tempdir().expect("workspace");
    let harness = tempdir().expect("harness");
    let ctx = ToolContext::for_test(workspace.path().to_path_buf(), harness.path().to_path_buf())
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
fn compact_read_only_tool_skips_planning_state_disk_read() {
    let (workspace, _harness, ctx) = context();
    let ctx = ctx.with_tool_profile("compact");
    let state_path = workspace
        .path()
        .join(crate::planning::PLANNING_RELATIVE_PATH);
    std::fs::create_dir_all(state_path.parent().expect("dir")).expect("mkdir");
    // Corrupt state.json proves compact read-only tools do not touch planning state on disk.
    std::fs::write(&state_path, "{broken-json").expect("corrupt");
    std::fs::write(workspace.path().join("hello.txt"), "world\n").expect("file");

    let output = call_tool(&ctx, "read_file", &json!({"path": "hello.txt"}));
    assert_eq!(output["ok"], true);
    assert_eq!(output["content"], "world\n");
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
