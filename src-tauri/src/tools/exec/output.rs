use serde_json::{json, Value};

const MAX_HIGHLIGHTS: usize = 10;
const HIGHLIGHT_BYTES: usize = 512;

pub(super) fn enrich_exec_result(command: &str, result: &mut Value) {
    let stdout = result.get("stdout").and_then(Value::as_str).unwrap_or("");
    let stderr = result.get("stderr").and_then(Value::as_str).unwrap_or("");
    let stdout_total = retention_total(result, "stdout_retention");
    let stderr_total = retention_total(result, "stderr_retention");
    let command_ok = result.get("command_ok").and_then(Value::as_bool);
    let kind = command_kind(command);
    let combined = [stdout, stderr].join("\n");
    let counters = output_counters(&combined);
    let highlights = important_lines(&combined);
    let returned_bytes = stdout.len().saturating_add(stderr.len());
    let original_bytes = stdout_total.saturating_add(stderr_total);

    let summary = match command_ok {
        Some(true) => format!("{kind} completed successfully"),
        Some(false) => format!("{kind} failed; inspect highlights or output_refs"),
        None => format!("{kind} is still running"),
    };
    let reduction_ratio = if original_bytes == 0 {
        1.0
    } else {
        returned_bytes as f64 / original_bytes as f64
    };
    if let Some(object) = result.as_object_mut() {
        object.insert(
            "output_summary".into(),
            json!({
                "kind": kind,
                "summary": summary,
                "errors": counters.errors,
                "warnings": counters.warnings,
                "failed": counters.failed,
                "passed": counters.passed,
                "highlights": highlights,
                "original_bytes": original_bytes,
                "returned_preview_bytes": returned_bytes,
                "reduction_ratio": reduction_ratio
            }),
        );
    }
}

#[derive(Default)]
struct Counters {
    errors: usize,
    warnings: usize,
    failed: usize,
    passed: usize,
}

fn output_counters(output: &str) -> Counters {
    let mut counters = Counters::default();
    for line in output.lines() {
        let lower = line.to_ascii_lowercase();
        if contains_signal(&lower, &["error:", "error[", " errors", "error "]) {
            counters.errors += 1;
        }
        if contains_signal(&lower, &["warning:", " warnings", "warning "]) {
            counters.warnings += 1;
        }
        if contains_signal(&lower, &[" failed", "failed:", " failure", "failures:"]) {
            counters.failed += 1;
        }
        if contains_signal(&lower, &[" passed", "passed:", " success", "succeeded"]) {
            counters.passed += 1;
        }
    }
    counters
}

fn important_lines(output: &str) -> Vec<String> {
    output
        .lines()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            contains_signal(
                &lower,
                &[
                    "error",
                    "failed",
                    "failure",
                    "warning",
                    "panic",
                    "test result:",
                    "tests ",
                ],
            )
        })
        .take(MAX_HIGHLIGHTS)
        .map(|line| preview(line.trim(), HIGHLIGHT_BYTES))
        .collect()
}

fn contains_signal(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn retention_total(result: &Value, key: &str) -> usize {
    result
        .get(key)
        .and_then(|value| value.get("total_bytes"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize
}

fn command_kind(command: &str) -> &'static str {
    let lower = command.to_ascii_lowercase();
    if lower.contains("cargo test") || contains_package_action(&lower, "test") {
        "test"
    } else if lower.contains("cargo clippy") || contains_package_action(&lower, "lint") {
        "lint"
    } else if lower.contains("cargo fmt") || contains_package_action(&lower, "format") {
        "format"
    } else if lower.contains("cargo build") || contains_package_action(&lower, "build") {
        "build"
    } else if lower.contains("cargo check")
        || contains_package_action(&lower, "check")
        || contains_package_action(&lower, "typecheck")
    {
        "check"
    } else if lower.starts_with("git ") {
        "git"
    } else if lower.starts_with("grep ") || lower.starts_with("rg ") {
        "search"
    } else {
        "command"
    }
}

fn contains_package_action(command: &str, action: &str) -> bool {
    ["npm", "pnpm", "yarn", "bun"].iter().any(|manager| {
        command.starts_with(manager)
            && (command.contains(&format!(" run {action}"))
                || command.contains(&format!(" {action}")))
    })
}

fn preview(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reducer_classifies_cargo_test_and_keeps_failure_evidence() {
        let mut result = json!({
            "command_ok": false,
            "stdout": "running 2 tests\ntest a ... ok\ntest b ... FAILED\ntest result: FAILED. 1 passed; 1 failed",
            "stderr": "error: test failed",
            "stdout_retention": {"total_bytes": 1_000_000},
            "stderr_retention": {"total_bytes": 2000}
        });
        enrich_exec_result("cargo test --all-targets", &mut result);
        assert_eq!(result["output_summary"]["kind"], "test");
        assert!(result["output_summary"]["failed"].as_u64().unwrap_or(0) > 0);
        assert!(
            result["output_summary"]["original_bytes"]
                .as_u64()
                .unwrap_or(0)
                > 1_000_000
        );
        assert!(result["output_summary"]["highlights"]
            .as_array()
            .is_some_and(|lines| !lines.is_empty()));
    }

    #[test]
    fn reducer_falls_back_to_generic_command_without_losing_byte_evidence() {
        let mut result = json!({
            "command_ok": true,
            "stdout": "done",
            "stderr": "",
            "stdout_retention": {"total_bytes": 9999},
            "stderr_retention": {"total_bytes": 0}
        });
        enrich_exec_result("custom-tool --verify", &mut result);
        assert_eq!(result["output_summary"]["kind"], "command");
        assert_eq!(result["output_summary"]["original_bytes"], 9999);
        assert_eq!(result["output_summary"]["returned_preview_bytes"], 4);
    }
}
