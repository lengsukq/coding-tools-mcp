use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::tools::context::ToolContext;
use crate::tools::policy::validate_command_for_workspace;
use crate::tools::workspace::{tool_ok, WorkspaceError};

use super::request::{filesystem_scope, resolve_workdir, CommandRunOptions};
use super::runner::run_command;

#[derive(Debug, Clone)]
struct GateCheck {
    id: String,
    category: &'static str,
    source: &'static str,
    command: String,
}

pub(super) fn quality_gate(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    let _scope = filesystem_scope(args)?;
    let workdir = resolve_workdir(ctx, args)?;
    let mut checks = discover_checks(&workdir);
    filter_checks(&mut checks, args)?;
    let dry_run = args
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let stop_on_failure = args
        .get("stop_on_failure")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    if dry_run {
        return Ok(tool_ok(json!({
            "mode": "quality_gate",
            "status": "DRY_RUN",
            "workdir": workdir.display().to_string(),
            "stop_on_failure": stop_on_failure,
            "checks": checks.iter().map(check_descriptor).collect::<Vec<_>>(),
            "executed": 0,
            "skipped": checks.len(),
            "summary": format!("{} quality check(s) discovered", checks.len())
        })));
    }

    if checks.is_empty() {
        return Ok(tool_ok(json!({
            "mode": "quality_gate",
            "status": "WARN",
            "workdir": workdir.display().to_string(),
            "checks": [],
            "executed": 0,
            "skipped": 0,
            "summary": "No supported quality checks were discovered. Add package scripts or Cargo.toml checks, or choose explicit discovered checks."
        })));
    }

    let timeout_ms = args
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .unwrap_or(300_000)
        .clamp(1, 600_000);
    let max_output = args
        .get("max_output_bytes")
        .and_then(Value::as_u64)
        .unwrap_or(8192)
        .clamp(1024, 1_048_576) as usize;
    let options = CommandRunOptions {
        limit: Duration::from_millis(timeout_ms),
        yield_time: Duration::from_millis(timeout_ms),
        max_output,
        tty: false,
        stdin_text: String::new(),
    };
    let gate_start = Instant::now();
    let mut results = Vec::new();
    let mut failed_step = None::<String>;
    let mut skipped = 0usize;

    for (index, check) in checks.iter().enumerate() {
        let step_start = Instant::now();
        let policy_args = json!({
            "cmd": check.command,
            "filesystem_scope": "workspace",
            "timeout_ms": timeout_ms
        });
        let result = match validate_command_for_workspace(
            &policy_args,
            &ctx.policy,
            Some(&ctx.workspace),
        ) {
            Ok(()) => {
                tauri::async_runtime::block_on(run_command(ctx, &check.command, &workdir, &options))
            }
            Err(error) => Err(WorkspaceError::ToolDetails {
                code: "POLICY_REJECTED",
                message: error.0,
                category: "policy",
                retryable: false,
                details: json!({
                    "stage": "quality_gate_policy",
                    "command": check.command,
                    "recoverable": true,
                    "suggestion": "Use an allowlisted quality check or adjust workspace policy before retrying."
                }),
            }),
        };
        let step = match result {
            Ok(snapshot) => step_from_snapshot(check, snapshot, step_start.elapsed()),
            Err(error) => step_from_error(check, error, step_start.elapsed()),
        };
        let failed = step.get("status").and_then(Value::as_str) == Some("FAIL");
        results.push(step);
        if failed {
            failed_step = Some(check.id.clone());
            if stop_on_failure {
                skipped = checks.len().saturating_sub(index + 1);
                break;
            }
        }
    }

    let executed = results.len();
    let status = if failed_step.is_some() {
        "FAIL"
    } else {
        "PASS"
    };
    let summary = if let Some(failed) = failed_step.as_deref() {
        format!("Quality gate failed at {failed}; inspect that step's output_refs")
    } else {
        format!("Quality gate passed: {executed} check(s) completed")
    };
    Ok(tool_ok(json!({
        "mode": "quality_gate",
        "status": status,
        "workdir": workdir.display().to_string(),
        "stop_on_failure": stop_on_failure,
        "duration_ms": gate_start.elapsed().as_millis(),
        "executed": executed,
        "skipped": skipped,
        "failed_step": failed_step,
        "checks": results,
        "summary": summary
    })))
}

fn discover_checks(workdir: &Path) -> Vec<GateCheck> {
    let mut checks = discover_package_checks(workdir);
    if workdir.join("Cargo.toml").is_file() {
        let locked = workdir.join("Cargo.lock").is_file();
        checks.extend([
            GateCheck {
                id: "cargo:fmt".into(),
                category: "format",
                source: "cargo",
                command: "cargo fmt --all -- --check".into(),
            },
            GateCheck {
                id: "cargo:clippy".into(),
                category: "lint",
                source: "cargo",
                command: "cargo clippy --all-targets -- -D warnings".into(),
            },
            GateCheck {
                id: "cargo:check".into(),
                category: "check",
                source: "cargo",
                command: format!(
                    "cargo check --all-targets{}",
                    if locked { " --locked" } else { "" }
                ),
            },
            GateCheck {
                id: "cargo:test".into(),
                category: "test",
                source: "cargo",
                command: format!(
                    "cargo test --all-targets{}",
                    if locked { " --locked" } else { "" }
                ),
            },
            GateCheck {
                id: "cargo:build".into(),
                category: "build",
                source: "cargo",
                command: format!("cargo build{}", if locked { " --locked" } else { "" }),
            },
        ]);
    }
    checks
}

fn discover_package_checks(workdir: &Path) -> Vec<GateCheck> {
    let package_path = workdir.join("package.json");
    let Ok(text) = std::fs::read_to_string(package_path) else {
        return Vec::new();
    };
    let Ok(package) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    let Some(scripts) = package.get("scripts").and_then(Value::as_object) else {
        return Vec::new();
    };
    let manager = package_manager(workdir);
    let candidates = [
        ("lint", ["lint", "lint", "lint"]),
        ("typecheck", ["typecheck", "type-check", "check:types"]),
        ("check", ["check", "check", "check"]),
        ("test", ["test", "test", "test"]),
        ("build", ["build", "build", "build"]),
    ];
    let mut output = Vec::new();
    let mut seen = HashSet::new();
    for (category, aliases) in candidates {
        if let Some(script) = aliases.into_iter().find(|script| {
            scripts
                .get(*script)
                .and_then(Value::as_str)
                .is_some_and(safe_package_script)
        }) {
            if !seen.insert(script) {
                continue;
            }
            output.push(GateCheck {
                id: format!("package:{script}"),
                category,
                source: "package",
                command: package_command(manager, script),
            });
        }
    }
    output
}

fn safe_package_script(script: &str) -> bool {
    let lower = script.to_ascii_lowercase();
    const FORBIDDEN: &[&str] = &[
        " deploy",
        "deploy ",
        " publish",
        "publish ",
        " release",
        "release ",
        "npm install",
        "pnpm install",
        "yarn add",
        "bun add",
        "docker push",
        "kubectl apply",
        "terraform apply",
        "curl ",
        "wget ",
        "scp ",
        "ssh ",
    ];
    !FORBIDDEN.iter().any(|needle| lower.contains(needle))
}

fn package_manager(workdir: &Path) -> &'static str {
    if workdir.join("pnpm-lock.yaml").is_file() {
        "pnpm"
    } else if workdir.join("yarn.lock").is_file() {
        "yarn"
    } else if workdir.join("bun.lock").is_file() || workdir.join("bun.lockb").is_file() {
        "bun"
    } else {
        "npm"
    }
}

fn package_command(manager: &str, script: &str) -> String {
    match manager {
        "yarn" => format!("yarn {script}"),
        _ => format!("{manager} run {script}"),
    }
}

fn filter_checks(checks: &mut Vec<GateCheck>, args: &Value) -> Result<(), WorkspaceError> {
    let Some(requested) = args.get("checks").and_then(Value::as_array) else {
        return Ok(());
    };
    let filters = requested
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<HashSet<_>>();
    if filters.is_empty() {
        return Ok(());
    }
    let known = checks
        .iter()
        .flat_map(|check| [check.id.as_str(), check.category])
        .collect::<HashSet<_>>();
    if let Some(unknown) = filters
        .iter()
        .find(|filter| !known.contains(filter.as_str()))
    {
        return Err(WorkspaceError::invalid_argument(format!(
            "unknown quality gate check/category: {unknown}"
        )));
    }
    checks.retain(|check| filters.contains(&check.id) || filters.contains(check.category));
    Ok(())
}

fn check_descriptor(check: &GateCheck) -> Value {
    json!({
        "id": check.id,
        "category": check.category,
        "source": check.source,
        "command": check.command
    })
}

fn step_from_snapshot(check: &GateCheck, snapshot: Value, duration: Duration) -> Value {
    let passed = snapshot.get("command_ok").and_then(Value::as_bool) == Some(true);
    json!({
        "id": check.id,
        "category": check.category,
        "source": check.source,
        "command": check.command,
        "status": if passed { "PASS" } else { "FAIL" },
        "duration_ms": duration.as_millis(),
        "exit_code": snapshot.get("exit_code").cloned().unwrap_or(Value::Null),
        "summary": snapshot.get("output_summary").cloned().unwrap_or_else(|| json!({})),
        "output_refs": snapshot.get("output_refs").cloned().unwrap_or_else(|| json!({})),
        "termination_reason": snapshot.get("termination_reason").cloned().unwrap_or(Value::Null)
    })
}

fn step_from_error(check: &GateCheck, error: WorkspaceError, duration: Duration) -> Value {
    let error_value = error.to_error_value();
    let session = error_value
        .get("details")
        .and_then(|details| details.get("session"));
    json!({
        "id": check.id,
        "category": check.category,
        "source": check.source,
        "command": check.command,
        "status": "FAIL",
        "duration_ms": duration.as_millis(),
        "exit_code": session.and_then(|value| value.get("exit_code")).cloned().unwrap_or(Value::Null),
        "summary": session.and_then(|value| value.get("output_summary")).cloned().unwrap_or_else(|| json!({})),
        "output_refs": session.and_then(|value| value.get("output_refs")).cloned().unwrap_or_else(|| json!({})),
        "error": error_value
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::context::ToolContext;
    use crate::tools::dispatch::call_tool;
    use tempfile::tempdir;

    #[test]
    fn discovers_only_known_package_quality_scripts() {
        let workspace = tempdir().expect("workspace");
        std::fs::write(
            workspace.path().join("package.json"),
            r#"{"scripts":{"lint":"eslint .","test":"vitest","build":"vite build","deploy":"danger"}}"#,
        )
        .expect("package");
        let checks = discover_checks(workspace.path());
        let ids = checks
            .iter()
            .map(|check| check.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["package:lint", "package:test", "package:build"]);
        assert!(!checks.iter().any(|check| check.command.contains("deploy")));
    }

    #[test]
    fn unsafe_build_script_body_is_not_discovered() {
        let workspace = tempdir().expect("workspace");
        std::fs::write(
            workspace.path().join("package.json"),
            r#"{"scripts":{"build":"vite build && npm publish","check":"tsc --noEmit"}}"#,
        )
        .expect("package");
        let checks = discover_checks(workspace.path());
        assert!(!checks.iter().any(|check| check.id == "package:build"));
        assert!(checks.iter().any(|check| check.id == "package:check"));
    }

    #[test]
    fn explicit_filter_accepts_category_without_arbitrary_command() {
        let mut checks = vec![
            GateCheck {
                id: "package:test".into(),
                category: "test",
                source: "package",
                command: "npm run test".into(),
            },
            GateCheck {
                id: "package:build".into(),
                category: "build",
                source: "package",
                command: "npm run build".into(),
            },
        ];
        filter_checks(&mut checks, &json!({"checks": ["test"]})).expect("filter");
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].id, "package:test");
    }

    #[test]
    fn quality_gate_executes_discovered_cargo_check_and_returns_evidence() {
        let workspace = tempdir().expect("workspace");
        let harness = tempdir().expect("harness");
        std::fs::create_dir_all(workspace.path().join("src")).expect("src");
        std::fs::write(
            workspace.path().join("Cargo.toml"),
            "[package]\nname = \"quality-gate-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("cargo manifest");
        std::fs::write(
            workspace.path().join("src/lib.rs"),
            "pub fn answer() -> u32 { 42 }\n",
        )
        .expect("lib");
        let ctx =
            ToolContext::for_test(workspace.path().to_path_buf(), harness.path().to_path_buf())
                .expect("context");

        let pass = quality_gate(
            &ctx,
            &json!({
                "action": "quality_gate",
                "checks": ["check"],
                "timeout_ms": 60_000,
                "max_output_bytes": 4096
            }),
        )
        .expect("passing gate");
        assert_eq!(pass["status"], "PASS");
        assert_eq!(pass["checks"][0]["status"], "PASS");
        assert!(pass["checks"][0]["output_refs"]["stdout"].is_string());

        std::fs::write(workspace.path().join("src/lib.rs"), "pub fn broken( {\n")
            .expect("broken lib");
        let fail = quality_gate(
            &ctx,
            &json!({
                "action": "quality_gate",
                "checks": ["check"],
                "timeout_ms": 60_000,
                "max_output_bytes": 4096
            }),
        )
        .expect("failing gate still returns evidence");
        assert_eq!(fail["status"], "FAIL");
        assert_eq!(fail["checks"][0]["status"], "FAIL");
        assert!(fail["checks"][0]["output_refs"]["stderr"].is_string());

        let stopped = quality_gate(
            &ctx,
            &json!({
                "action": "quality_gate",
                "checks": ["check", "build"],
                "stop_on_failure": true,
                "timeout_ms": 60_000,
                "max_output_bytes": 4096
            }),
        )
        .expect("stop-on-failure gate");
        assert_eq!(stopped["status"], "FAIL");
        assert_eq!(stopped["executed"], 1);
        assert_eq!(stopped["skipped"], 1);
    }

    #[test]
    fn quality_gate_dry_run_never_includes_deploy_script() {
        let workspace = tempdir().expect("workspace");
        let harness = tempdir().expect("harness");
        std::fs::write(
            workspace.path().join("package.json"),
            r#"{"scripts":{"check":"tsc --noEmit","deploy":"publish-prod"}}"#,
        )
        .expect("package");
        let ctx =
            ToolContext::for_test(workspace.path().to_path_buf(), harness.path().to_path_buf())
                .expect("context");
        let result = quality_gate(&ctx, &json!({"action": "quality_gate", "dry_run": true}))
            .expect("dry run");
        assert_eq!(result["status"], "DRY_RUN");
        let commands = result["checks"]
            .as_array()
            .expect("checks")
            .iter()
            .filter_map(|check| check["command"].as_str())
            .collect::<Vec<_>>();
        assert!(commands.iter().all(|command| !command.contains("deploy")));
    }

    #[test]
    fn quality_gate_passes_outer_exec_policy_without_cmd() {
        let workspace = tempdir().expect("workspace");
        let harness = tempdir().expect("harness");
        std::fs::write(
            workspace.path().join("package.json"),
            r#"{"scripts":{"check":"tsc --noEmit"}}"#,
        )
        .expect("package");
        let ctx =
            ToolContext::for_test(workspace.path().to_path_buf(), harness.path().to_path_buf())
                .expect("context");
        let result = call_tool(
            &ctx,
            "exec_command",
            &json!({"action": "quality_gate", "dry_run": true}),
        );
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(result["mode"], "quality_gate");
        assert_eq!(result["status"], "DRY_RUN");
    }

    #[test]
    fn quality_gate_surfaces_policy_refusal_as_failed_evidence() {
        let workspace = tempdir().expect("workspace");
        let harness = tempdir().expect("harness");
        std::fs::create_dir_all(workspace.path().join("src")).expect("src");
        std::fs::write(
            workspace.path().join("Cargo.toml"),
            "[package]\nname = \"quality-policy-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("manifest");
        std::fs::write(workspace.path().join("src/lib.rs"), "pub fn ok() {}\n").expect("lib");
        let mut ctx =
            ToolContext::for_test(workspace.path().to_path_buf(), harness.path().to_path_buf())
                .expect("context");
        ctx.policy.allowed_commands.remove("cargo");
        ctx.policy.workspace_local_entries = false;
        let result = quality_gate(
            &ctx,
            &json!({"action": "quality_gate", "checks": ["check"], "timeout_ms": 30_000}),
        )
        .expect("policy refusal is quality evidence, not transport failure");
        assert_eq!(result["status"], "FAIL");
        assert_eq!(result["checks"][0]["status"], "FAIL");
        assert!(result["checks"][0]["error"]["code"].is_string());
    }
}
