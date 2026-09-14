use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("{message}")]
    Tool {
        code: &'static str,
        message: String,
        category: &'static str,
        retryable: bool,
    },
    #[error("{message}")]
    ToolDetails {
        code: &'static str,
        message: String,
        category: &'static str,
        retryable: bool,
        details: Value,
    },
}

impl WorkspaceError {
    pub fn message(&self) -> String {
        match self {
            Self::Tool { message, .. } | Self::ToolDetails { message, .. } => message.clone(),
        }
    }

    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::Tool {
            code: "INVALID_ARGUMENT",
            message: message.into(),
            category: "validation",
            retryable: false,
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::Tool {
            code: "NOT_FOUND",
            message: message.into(),
            category: "not_found",
            retryable: false,
        }
    }

    pub fn absolute_path_denied() -> Self {
        Self::Tool {
            code: "ABSOLUTE_PATH_DENIED",
            message: "Absolute paths are denied.".into(),
            category: "security",
            retryable: false,
        }
    }

    pub fn path_outside_workspace() -> Self {
        Self::Tool {
            code: "PATH_OUTSIDE_WORKSPACE",
            message: "Path escapes the configured workspace.".into(),
            category: "security",
            retryable: false,
        }
    }

    pub fn symlink_escape() -> Self {
        Self::Tool {
            code: "SYMLINK_ESCAPE",
            message: "Path escapes the configured workspace.".into(),
            category: "security",
            retryable: false,
        }
    }

    pub fn not_a_directory(message: impl Into<String>) -> Self {
        Self::Tool {
            code: "NOT_A_DIRECTORY",
            message: message.into(),
            category: "validation",
            retryable: false,
        }
    }

    pub fn to_error_value(&self) -> Value {
        match self {
            Self::Tool {
                code,
                message,
                category,
                retryable,
            } => {
                let details = json!({});
                json!({
                    "code": code,
                    "message": message,
                    "category": category,
                    "retryable": retryable,
                    "details": details,
                    "recovery": recovery_contract_value(code, category, *retryable, &json!({}))
                })
            }
            Self::ToolDetails {
                code,
                message,
                category,
                retryable,
                details,
            } => json!({
                "code": code,
                "message": message,
                "category": category,
                "retryable": retryable,
                "details": details,
                "recovery": recovery_contract_value(code, category, *retryable, details)
            }),
        }
    }
}

fn recovery_contract_value(code: &str, category: &str, retryable: bool, details: &Value) -> Value {
    let suggestion = details
        .get("suggestion")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);

    let (action, default_instruction, retry_when): (&str, &str, Option<&str>) = match code {
        "PLAN_MODE_READ_ONLY" => (
            "switch_planning_mode",
            "Use read/planning tools, or switch the workspace to Goal/Direct mode before retrying project mutation.",
            Some("after the workspace planning mode changes to Goal or Direct"),
        ),
        "GOAL_CONTEXT_REQUIRED" | "GOAL_CONTEXT_INVALID" | "GOAL_NOT_ACTIVE"
        | "PLAN_CONTEXT_INVALID" | "PLAN_GOAL_MISMATCH" | "PLAN_NOT_EXECUTABLE" => (
            "fix_planning_context",
            "Select or reactivate a compatible Goal/Plan before retrying project mutation.",
            Some("after an active compatible Goal/Plan is focused"),
        ),
        "PLANNING_STATE_UNAVAILABLE" => (
            "repair_or_reset_planning_state",
            "Repair or reset the project planning state before retrying protected mutations.",
            Some("after .coding-tools/planning/state.json is readable again"),
        ),
        "FILE_CHANGED_EXTERNALLY" => (
            "review_external_changes",
            "Review the external workspace changes and refresh or restart the Harness task baseline before continuing.",
            Some("after the external changes are reviewed and the Harness baseline is refreshed"),
        ),
        "BASELINE_STALE" => (
            "refresh_harness_baseline",
            "Refresh or restart the Harness task after reviewing the branch/HEAD change.",
            Some("after the Harness baseline matches the current branch and HEAD"),
        ),
        "TASK_ALREADY_ACTIVE" => (
            "reuse_or_finish_active_task",
            "Reuse, pause, or finish the active Harness task instead of starting a duplicate task.",
            Some("after the active task is reused or finished"),
        ),
        "DANGEROUS_OPERATION_REQUIRES_CONFIRMATION" => (
            "confirm_and_retry",
            "Retry the original operation with confirm=true only after explicit user authorization.",
            Some("after explicit user authorization"),
        ),
        "ELICITATION_UNSUPPORTED" => (
            "continue_without_permission_request",
            "Do not retry request_permissions. Retry the original operation only when its own recovery guidance says it is allowed.",
            None,
        ),
        "TIMEOUT" => (
            "inspect_output_and_retry",
            "Read the retained output first, then retry with an adjusted timeout only if the command should continue.",
            Some("after retained output has been inspected and timeout/runtime conditions are corrected"),
        ),
        "COMMAND_SPAWN_FAILED" => (
            "fix_runtime_and_retry",
            "Check the command path, permissions, and runtime environment before retrying.",
            Some("after the runtime or command-start condition has been corrected"),
        ),
        "SESSION_NOT_FOUND" | "SESSION_CLOSED" => (
            "start_new_command",
            "The referenced command session is unavailable; start a new command instead of repeating the same session operation.",
            Some("after a new command session has been started"),
        ),
        "COMMAND_EVICTED" => (
            "start_new_command",
            "The command finished and its retained output has been evicted. Start a new command if the operation must be run again.",
            Some("after a new command has been started"),
        ),
        _ => match category {
            "validation" => (
                "fix_input",
                "Correct the invalid arguments or content before retrying.",
                Some("after the request has been corrected"),
            ),
            "not_found" => (
                "refresh_or_correct_reference",
                "Check that the referenced path, id, or resource exists and use the current value before retrying.",
                Some("after the missing or stale reference has been corrected"),
            ),
            "security" | "policy" | "permission" => (
                "change_request",
                "Do not repeat the same blocked operation unchanged; adjust the request or satisfy the required permission/context first.",
                Some("after the request or required permission/context changes"),
            ),
            "storage" | "filesystem" => {
                if retryable {
                    (
                        "retry_after_storage_recovers",
                        "Resolve the storage/filesystem condition, then retry the operation.",
                        Some("after the storage/filesystem condition has recovered"),
                    )
                } else {
                    (
                        "repair_state",
                        "Repair the stored state or filesystem input before retrying; repeating the same call unchanged will fail again.",
                        Some("after the stored state or filesystem input has been repaired"),
                    )
                }
            }
            "runtime" => {
                if retryable {
                    (
                        "retry_after_runtime_recovers",
                        "Inspect the runtime condition and retry after it has recovered or been corrected.",
                        Some("after the runtime condition has recovered or been corrected"),
                    )
                } else {
                    (
                        "fix_runtime_request",
                        "Do not repeat the same runtime operation unchanged; correct the command/session/runtime condition first.",
                        Some("after the runtime request or environment has been corrected"),
                    )
                }
            }
            "internal" => {
                if retryable {
                    (
                        "retry_or_report",
                        "Retry once after checking current state; if the failure repeats, report the internal error instead of looping.",
                        Some("after checking current state; stop retrying if the same internal error repeats"),
                    )
                } else {
                    (
                        "report_issue",
                        "Do not retry the same call unchanged; inspect or report the internal error.",
                        None,
                    )
                }
            }
            _ => {
                if retryable {
                    (
                        "retry",
                        "Retry only after checking the reported condition and current state.",
                        Some("after the reported condition has changed or been corrected"),
                    )
                } else {
                    (
                        "change_request",
                        "Do not repeat the same call unchanged; correct the reported condition first.",
                        Some("after the reported condition has changed or been corrected"),
                    )
                }
            }
        },
    };

    json!({
        "action": action,
        "instruction": suggestion.unwrap_or_else(|| default_instruction.to_string()),
        "retry_when": retry_when
    })
}

pub type WorkspaceResult<T> = Result<T, WorkspaceError>;
