use std::fs;
use std::path::{Path, PathBuf};

use coding_tools_mcp_desktop_lib::planning::{PlanningMode, PlanningService, PLANNING_SCHEMA_VERSION};
use coding_tools_mcp_desktop_lib::tools::{call_tool, ToolContext};
use serde_json::{json, Value};

#[cfg(windows)]
const TEST_PYTHON: &str = "python";
#[cfg(not(windows))]
const TEST_PYTHON: &str = "python3";

struct WorkspaceFixture {
    root: PathBuf,
    harness: PathBuf,
    _temp: tempfile::TempDir,
}

#[test]
fn running_command_is_shared_across_tool_contexts_for_the_same_workspace() {
    let fixture = fixture_root("fresh");
    let first = ctx(&fixture);
    let second = ToolContext::for_test(
        fixture.root.clone(),
        fixture._temp.path().join("second-harness"),
    )
    .expect("second tool context");

    let started = call_tool(
        &first,
        "exec_command",
        &json!({
            "cmd": format!("{TEST_PYTHON} -c \"import time; print('WORKSPACE_COMMAND_READY', flush=True); time.sleep(5)\""),
            "timeout_ms": 10_000,
            "yield_time_ms": 0
        }),
    );
    assert_eq!(started["ok"], true, "{started}");
    let command_id = started["command_id"].as_str().expect("command id");
    assert_eq!(started["session_id"], command_id);
    assert_eq!(started["command_state"], "running");
    assert!(started["created_at"].as_str().is_some());
    assert!(started["started_at"].as_str().is_some());
    assert_eq!(started["finished_at"], Value::Null);
    let stdout_ref = started["output_refs"]["stdout"]
        .as_str()
        .expect("stdout ref");
    assert!(stdout_ref.starts_with("command:"));

    std::thread::sleep(std::time::Duration::from_millis(100));
    let output = call_tool(
        &second,
        "read_output",
        &json!({"output_ref": stdout_ref, "limit": 4096}),
    );
    assert_eq!(output["ok"], true, "{output}");
    assert_eq!(output["command_id"], command_id);
    assert!(output["content"]
        .as_str()
        .unwrap_or_default()
        .contains("WORKSPACE_COMMAND_READY"));

    let killed = call_tool(
        &second,
        "kill_session",
        &json!({"command_id": command_id, "wait_ms": 2_000}),
    );
    assert_eq!(killed["ok"], true, "{killed}");
    assert_eq!(killed["command_id"], command_id);
    assert_eq!(killed["killed"], true);
    assert_eq!(killed["command_state"], "killed");
    assert!(killed["finished_at"].as_str().is_some());

    let evicted = call_tool(
        &first,
        "read_output",
        &json!({"output_ref": stdout_ref, "limit": 4096}),
    );
    assert_eq!(evicted["ok"], false);
    assert_eq!(evicted["error"]["code"], "COMMAND_EVICTED");
    assert_eq!(evicted["error"]["details"]["command_state"], "evicted");
    assert_eq!(evicted["error"]["recovery"]["action"], "start_new_command");
}

#[test]
fn command_reconciles_natural_process_exit_across_contexts() {
    let fixture = fixture_root("fresh");
    let first = ctx(&fixture);
    let second = ToolContext::for_test(
        fixture.root.clone(),
        fixture._temp.path().join("reconcile-harness"),
    )
    .expect("second tool context");

    let started = call_tool(
        &first,
        "exec_command",
        &json!({
            "cmd": format!("{TEST_PYTHON} -c \"import time; print('DONE', flush=True); time.sleep(0.1)\""),
            "timeout_ms": 10_000,
            "yield_time_ms": 0
        }),
    );
    assert_eq!(started["ok"], true, "{started}");
    let command_id = started["command_id"].as_str().expect("command id");
    std::thread::sleep(std::time::Duration::from_millis(300));

    let reconciled = call_tool(
        &second,
        "write_stdin",
        &json!({"command_id": command_id, "chars": "", "yield_time_ms": 0}),
    );
    assert_eq!(reconciled["ok"], true, "{reconciled}");
    assert_eq!(reconciled["command_state"], "exited");
    assert_eq!(reconciled["status"], "exited");
    assert_eq!(reconciled["termination_reason"], "exited");
    assert!(reconciled["finished_at"].as_str().is_some());

    let cleanup = call_tool(
        &second,
        "kill_session",
        &json!({"command_id": command_id, "wait_ms": 0}),
    );
    assert_eq!(cleanup["ok"], true, "{cleanup}");
}

#[test]
fn large_command_output_keeps_head_and_tail_with_eviction_metadata() {
    let fixture = fixture_root("fresh");
    let first = ctx(&fixture);
    let second = ToolContext::for_test(
        fixture.root.clone(),
        fixture._temp.path().join("large-output-harness"),
    )
    .expect("second tool context");
    let payload_bytes = 10_000_000usize;

    let started = call_tool(
        &first,
        "exec_command",
        &json!({
            "cmd": format!(
                "{TEST_PYTHON} -c \"import sys,time; sys.stdout.write('UNIQUE_HEAD_ERROR\\n' + 'x'*{payload_bytes} + '\\nUNIQUE_TAIL_ERROR\\n'); sys.stdout.flush(); time.sleep(5)\""
            ),
            "timeout_ms": 15_000,
            "yield_time_ms": 0,
            "max_output_bytes": 65_536
        }),
    );
    assert_eq!(started["ok"], true, "{started}");
    let command_id = started["command_id"].as_str().expect("command id");
    let stdout_ref = started["output_refs"]["stdout"]
        .as_str()
        .expect("stdout ref")
        .to_string();

    let mut snapshot = Value::Null;
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        snapshot = call_tool(
            &second,
            "write_stdin",
            &json!({"command_id": command_id, "chars": "", "yield_time_ms": 0, "max_output_bytes": 65_536}),
        );
        if snapshot["stdout_retention"]["total_bytes"]
            .as_u64()
            .unwrap_or(0)
            >= payload_bytes as u64
        {
            break;
        }
    }
    assert_eq!(snapshot["ok"], true, "{snapshot}");
    assert!(snapshot["stdout_retention"]["total_bytes"]
        .as_u64()
        .unwrap_or(0)
        >= payload_bytes as u64);
    assert!(snapshot["stdout_retention"]["evicted_bytes"]
        .as_u64()
        .unwrap_or(0)
        > 0);
    let preview = snapshot["stdout"].as_str().unwrap_or_default();
    assert!(preview.contains("UNIQUE_HEAD_ERROR"));
    assert!(preview.contains("UNIQUE_TAIL_ERROR"));

    let head = call_tool(
        &second,
        "read_output",
        &json!({"output_ref": stdout_ref, "offset": 0, "limit": 131_072}),
    );
    assert_eq!(head["ok"], true, "{head}");
    assert!(head["content"].as_str().unwrap_or_default().contains("UNIQUE_HEAD_ERROR"));
    assert!(head["evicted_bytes"].as_u64().unwrap_or(0) > 0);
    let tail_offset = head["next_offset"].as_u64().expect("tail offset");
    assert!(tail_offset > head["head_retained_bytes"].as_u64().unwrap_or(0));

    let tail = call_tool(
        &first,
        "read_output",
        &json!({"output_ref": stdout_ref, "offset": tail_offset, "limit": 1_048_576}),
    );
    assert_eq!(tail["ok"], true, "{tail}");
    assert!(tail["content"].as_str().unwrap_or_default().contains("UNIQUE_TAIL_ERROR"));
    assert_eq!(tail["next_offset"], Value::Null);

    let killed = call_tool(
        &second,
        "kill_session",
        &json!({"command_id": command_id, "wait_ms": 2_000}),
    );
    assert_eq!(killed["ok"], true, "{killed}");
}

fn fixture_root(name: &str) -> WorkspaceFixture {
    let temp = tempfile::tempdir().expect("temp workspace");
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/workspaces")
        .join(name);
    assert!(source.is_dir(), "missing workspace fixture: {}", source.display());
    let root = temp.path().join("workspace");
    copy_dir_all(&source, &root).expect("copy workspace fixture");
    let harness = temp.path().join("harness");
    WorkspaceFixture {
        root,
        harness,
        _temp: temp,
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn ctx(fixture: &WorkspaceFixture) -> ToolContext {
    ToolContext::for_test(fixture.root.clone(), fixture.harness.clone()).expect("tool context")
}

#[test]
fn legacy_planning_workspace_migrates_without_losing_plan_details() {
    let fixture = fixture_root("legacy-planning");
    let service = PlanningService::new(&fixture.root);

    let state = service.state().expect("legacy planning state should load");
    assert_eq!(state.goals[0].objective, "Upgrade generation quality without breaking existing workflows.");
    assert_eq!(state.goals[0].success_criteria.len(), 1);
    assert_eq!(state.plans[0].objective, "Separate planning from final payload mapping.");
    assert_eq!(state.plans[0].steps.len(), 2);
    assert_eq!(state.plans[0].steps[0].id, "task-baseline");
    assert!(state.plans[0].extra.contains_key("phases"));
    assert!(state.plans[0].extra.contains_key("architecture"));

    service.set_mode(PlanningMode::Plan).expect("persist migrated state");
    let normalized: Value = serde_json::from_str(
        &fs::read_to_string(fixture.root.join(".coding-tools/planning/state.json"))
            .expect("normalized planning state"),
    )
    .expect("normalized planning json");
    assert_eq!(normalized["schema_version"], PLANNING_SCHEMA_VERSION);
    assert_eq!(normalized["goals"][0]["objective"], "Upgrade generation quality without breaking existing workflows.");
    assert!(normalized["goals"][0]["success_criteria"][0].is_object());
    assert_eq!(normalized["plans"][0]["steps"][0]["id"], "task-baseline");
    assert!(normalized["plans"][0]["phases"].is_array());
}

#[test]
fn corrupt_planning_fails_closed_but_reset_restores_workspace_mutation() {
    let fixture = fixture_root("corrupt-planning");
    let ctx = ctx(&fixture);

    let blocked = call_tool(
        &ctx,
        "apply_patch",
        &json!({
            "patch": "*** Begin Patch\n*** Update File: README.md\n@@\n-# Corrupt Planning Fixture\n+# Recovered Planning Fixture\n*** End Patch\n"
        }),
    );
    assert_eq!(blocked["ok"], false);
    assert_eq!(blocked["error"]["code"], "PLANNING_STATE_UNAVAILABLE");
    assert_eq!(
        blocked["error"]["recovery"]["action"],
        "repair_or_reset_planning_state"
    );

    PlanningService::new(&fixture.root)
        .reset_state()
        .expect("reset must not depend on parsing broken state");
    let recovered = call_tool(
        &ctx,
        "apply_patch",
        &json!({
            "patch": "*** Begin Patch\n*** Update File: README.md\n@@\n-# Corrupt Planning Fixture\n+# Recovered Planning Fixture\n*** End Patch\n"
        }),
    );
    assert_eq!(recovered["ok"], true, "{recovered}");
    assert!(fs::read_to_string(fixture.root.join("README.md"))
        .expect("read recovered source")
        .contains("# Recovered Planning Fixture"));
}

#[test]
fn plan_and_goal_modes_keep_planning_writable_and_gate_project_mutation() {
    let fixture = fixture_root("fresh");
    let ctx = ctx(&fixture);
    let service = PlanningService::new(&fixture.root);
    service.set_mode(PlanningMode::Plan).expect("plan mode");

    let goal = call_tool(
        &ctx,
        "create_goal",
        &json!({
            "title": "E2E Goal",
            "objective": "Verify planning and mutation gates",
            "success_criteria": ["Plan metadata remains writable"]
        }),
    );
    assert_eq!(goal["ok"], true, "{goal}");
    let goal_id = goal["goal"]["id"].as_str().expect("goal id");

    let plan = call_tool(
        &ctx,
        "create_plan",
        &json!({
            "goal_id": goal_id,
            "title": "E2E Plan",
            "objective": "Exercise real workspace planning flow",
            "steps": ["Patch source after Goal mode is enabled"]
        }),
    );
    assert_eq!(plan["ok"], true, "{plan}");

    let blocked = call_tool(
        &ctx,
        "apply_patch",
        &json!({
            "patch": "*** Begin Patch\n*** Update File: README.md\n@@\n-Stable source file used by real-workspace lifecycle tests.\n+Goal mode source mutation succeeded.\n*** End Patch\n"
        }),
    );
    assert_eq!(blocked["error"]["code"], "PLAN_MODE_READ_ONLY");
    assert_eq!(blocked["error"]["recovery"]["action"], "switch_planning_mode");

    service.set_mode(PlanningMode::Goal).expect("goal mode");
    let changed = call_tool(
        &ctx,
        "apply_patch",
        &json!({
            "patch": "*** Begin Patch\n*** Update File: README.md\n@@\n-Stable source file used by real-workspace lifecycle tests.\n+Goal mode source mutation succeeded.\n*** End Patch\n"
        }),
    );
    assert_eq!(changed["ok"], true, "{changed}");
}

#[test]
fn harness_ignores_managed_planning_and_history_but_detects_real_external_change() {
    let fixture = fixture_root("fresh");
    let ctx = ctx(&fixture);

    let started = call_tool(&ctx, "start_task", &json!({"objective": "workspace e2e"}));
    assert_eq!(started["ok"], true, "{started}");

    let bootstrap = call_tool(
        &ctx,
        "history_session_bootstrap",
        &json!({
            "session_key": "workspace-e2e-session",
            "initial_user_input": "run real workspace lifecycle",
            "create_if_missing": true
        }),
    );
    assert_eq!(bootstrap["ok"], true, "{bootstrap}");
    let expected_path = bootstrap["current_path"].as_str().expect("history path");
    let checkpoint = call_tool(
        &ctx,
        "history_session_checkpoint",
        &json!({
            "session_key": "workspace-e2e-session",
            "expected_path": expected_path,
            "raw_user_input": "checkpoint real workspace lifecycle"
        }),
    );
    assert_eq!(checkpoint["ok"], true, "{checkpoint}");

    let internal_state_only = call_tool(&ctx, "exec_command", &json!({"cmd": "pwd"}));
    assert_ne!(
        internal_state_only["error"]["code"],
        "FILE_CHANGED_EXTERNALLY",
        "managed planning/history files must not poison the Harness baseline"
    );

    fs::write(
        fixture.root.join("README.md"),
        "# Workspace E2E Fixture\n\nExternal editor change.\n",
    )
    .expect("external source edit");
    let external = call_tool(&ctx, "exec_command", &json!({"cmd": "pwd"}));
    assert_eq!(external["ok"], false);
    assert_eq!(external["error"]["code"], "FILE_CHANGED_EXTERNALLY");
    assert_eq!(
        external["error"]["recovery"]["action"],
        "review_external_changes"
    );
}
