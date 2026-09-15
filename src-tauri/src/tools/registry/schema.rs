use serde_json::{json, Value};

fn history_manage_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": { "type": "string", "enum": ["bootstrap", "checkpoint", "validate", "search", "read"] },
            "workspace_root": { "type": "string", "minLength": 1 },
            "session_key": { "type": "string", "minLength": 1 },
            "expected_path": { "type": "string", "minLength": 1 },
            "history_dir": { "type": "string", "default": "docs/history-session" },
            "title": { "type": "string" },
            "initial_user_input": { "type": "string" },
            "create_if_missing": { "type": "boolean", "default": true },
            "turn_id": { "type": "string", "minLength": 1 },
            "timestamp": { "type": "string" },
            "user_intent": { "type": "string" },
            "raw_user_input": { "type": "string" },
            "findings": { "type": "array", "items": { "type": "string" } },
            "decisions": { "type": "array", "items": { "type": "string" } },
            "files_changed": { "type": "array", "items": { "type": "string" } },
            "tests": { "type": "array", "items": { "type": "string" } },
            "runtime_state": { "type": "array", "items": { "type": "string" } },
            "remaining_issues": { "type": "array", "items": { "type": "string" } },
            "next_actions": { "type": "array", "items": { "type": "string" } },
            "notes": { "type": "string" },
            "repair": { "type": "boolean", "default": false },
            "query": { "type": "string", "default": "" },
            "cursor": { "type": "integer", "minimum": 0, "default": 0 },
            "limit": { "type": "integer", "minimum": 1, "maximum": 50 },
            "number": { "type": "integer", "minimum": 1 },
            "path": { "type": "string", "minLength": 1 },
            "max_bytes": { "type": "integer", "minimum": 1, "maximum": 65536 },
            "expected_hash": { "type": "string", "minLength": 64, "maxLength": 64 }
        },
        "required": ["action"],
        "additionalProperties": false
    })
}

fn planning_manage_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": { "type": "string", "enum": ["state", "create_goal", "update_goal", "create_plan", "update_plan", "request_goal_review", "request_plan_review"] },
            "goal_id": { "type": "string", "minLength": 1 },
            "plan_id": { "type": "string", "minLength": 1 },
            "title": { "type": "string", "minLength": 1 },
            "objective": { "type": "string", "minLength": 1 },
            "success_criteria": { "type": "array", "items": { "type": "string", "minLength": 1 } },
            "constraints": { "type": "array", "items": { "type": "string", "minLength": 1 } },
            "completed_criteria_ids": { "type": "array", "items": { "type": "string", "minLength": 1 } },
            "status": { "type": "string", "enum": ["draft", "active", "paused", "cancelled"] },
            "focus": { "type": "boolean" },
            "steps": { "type": "array", "items": { "type": "string", "minLength": 1 } },
            "step_updates": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "step_id": { "type": "string", "minLength": 1 },
                        "status": { "type": "string", "enum": ["pending", "in_progress", "completed", "blocked", "skipped"] },
                        "notes": { "type": "string" }
                    },
                    "required": ["step_id", "status"],
                    "additionalProperties": false
                }
            },
            "summary": { "type": "string", "minLength": 1 }
        },
        "required": ["action"],
        "additionalProperties": false
    })
}

fn task_manage_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": { "type": "string", "enum": ["status", "operation_log", "project_state", "start", "update", "pause", "resume", "finish", "context", "events", "change_summary"] },
            "task_id": { "type": "string", "minLength": 1 },
            "objective": { "type": "string", "minLength": 1 },
            "completed_steps": { "type": "array", "items": { "type": "string" } },
            "pending_steps": { "type": "array", "items": { "type": "string" } },
            "summary": { "type": "string" },
            "allow_unverified": { "type": "boolean", "default": false },
            "cursor": { "type": "integer", "minimum": 0, "default": 0 },
            "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 50 },
            "max_files": { "type": "integer", "minimum": 1, "maximum": 10000, "default": 200 },
            "max_bytes": { "type": "integer", "minimum": 8192, "maximum": 131072, "default": 32768 },
            "change_id": { "type": "string" }
        },
        "required": ["action"],
        "additionalProperties": false
    })
}

pub fn input_schema(name: &str) -> Value {
    match name {
        "server_info" => json!({
            "type": "object",
            "properties": {
                "known_schema_hash": {
                    "type": "string",
                    "minLength": 1,
                    "description": "Optional schema hash remembered by the client. server_info reports schema_changed/reconnect_recommended when it differs."
                }
            },
            "additionalProperties": false
        }),
        "history_manage" => history_manage_schema(),
        "planning_manage" => planning_manage_schema(),
        "task_manage" => task_manage_schema(),
        "list_skills" => json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
        "get_skill" => json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "minLength": 1 },
                "name": { "type": "string", "minLength": 1 }
            },
            "description": "Provide either id or name to load a skill. If both are provided, id takes priority.",
            "additionalProperties": false
        }),
        "history_session_bootstrap" => json!({
            "type": "object",
            "properties": {
                "workspace_root": { "type": "string", "minLength": 1 },
                "session_key": { "type": "string", "minLength": 1 },
                "title": { "type": "string" },
                "initial_user_input": { "type": "string" },
                "history_dir": { "type": "string", "default": "docs/history-session" },
                "create_if_missing": { "type": "boolean", "default": true }
            },
            "additionalProperties": false
        }),
        "history_session_checkpoint" => json!({
            "type": "object",
            "properties": {
                "workspace_root": { "type": "string", "minLength": 1 },
                "session_key": { "type": "string", "minLength": 1 },
                "expected_path": { "type": "string", "minLength": 1 },
                "history_dir": { "type": "string", "default": "docs/history-session" },
                "turn_id": { "type": "string", "minLength": 1 },
                "timestamp": { "type": "string" },
                "user_intent": { "type": "string" },
                "raw_user_input": { "type": "string" },
                "findings": { "type": "array", "items": { "type": "string" } },
                "decisions": { "type": "array", "items": { "type": "string" } },
                "files_changed": { "type": "array", "items": { "type": "string" } },
                "tests": { "type": "array", "items": { "type": "string" } },
                "runtime_state": { "type": "array", "items": { "type": "string" } },
                "remaining_issues": { "type": "array", "items": { "type": "string" } },
                "next_actions": { "type": "array", "items": { "type": "string" } },
                "notes": { "type": "string" }
            },
            "additionalProperties": false
        }),
        "history_session_validate" => json!({
            "type": "object",
            "properties": {
                "workspace_root": { "type": "string", "minLength": 1 },
                "history_dir": { "type": "string", "default": "docs/history-session" },
                "repair": { "type": "boolean", "default": false }
            },
            "additionalProperties": false
        }),
        "history_session_search" => json!({
            "type": "object",
            "properties": {
                "workspace_root": { "type": "string", "minLength": 1 },
                "history_dir": { "type": "string", "default": "docs/history-session" },
                "query": { "type": "string", "default": "" },
                "cursor": { "type": "integer", "minimum": 0, "default": 0 },
                "limit": { "type": "integer", "minimum": 1, "maximum": 50, "default": 5 }
            },
            "additionalProperties": false
        }),
        "history_session_read" => json!({
            "type": "object",
            "properties": {
                "workspace_root": { "type": "string", "minLength": 1 },
                "history_dir": { "type": "string", "default": "docs/history-session" },
                "number": { "type": "integer", "minimum": 1 },
                "path": { "type": "string", "minLength": 1 },
                "cursor": { "type": "integer", "minimum": 0, "default": 0 },
                "max_bytes": { "type": "integer", "minimum": 1, "maximum": 65536, "default": 16384 },
                "expected_hash": { "type": "string", "minLength": 64, "maxLength": 64 }
            },
            "additionalProperties": false
        }),
        "planning_state" => json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
        "create_goal" => json!({
            "type": "object",
            "properties": {
                "title": { "type": "string", "minLength": 1 },
                "objective": { "type": "string", "minLength": 1 },
                "success_criteria": { "type": "array", "items": { "type": "string", "minLength": 1 } },
                "constraints": { "type": "array", "items": { "type": "string", "minLength": 1 } }
            },
            "required": ["title", "objective"],
            "additionalProperties": false
        }),
        "update_goal" => json!({
            "type": "object",
            "properties": {
                "goal_id": { "type": "string", "minLength": 1 },
                "title": { "type": "string", "minLength": 1 },
                "objective": { "type": "string", "minLength": 1 },
                "status": { "type": "string", "enum": ["active", "paused", "cancelled"] },
                "constraints": { "type": "array", "items": { "type": "string", "minLength": 1 } },
                "completed_criteria_ids": { "type": "array", "items": { "type": "string", "minLength": 1 } },
                "focus": { "type": "boolean" }
            },
            "required": ["goal_id"],
            "additionalProperties": false
        }),
        "create_plan" => json!({
            "type": "object",
            "properties": {
                "goal_id": { "type": "string", "minLength": 1 },
                "title": { "type": "string", "minLength": 1 },
                "objective": { "type": "string", "minLength": 1 },
                "steps": { "type": "array", "items": { "type": "string", "minLength": 1 } }
            },
            "required": ["title", "objective"],
            "additionalProperties": false
        }),
        "update_plan" => json!({
            "type": "object",
            "properties": {
                "plan_id": { "type": "string", "minLength": 1 },
                "status": { "type": "string", "enum": ["draft", "active", "paused", "cancelled"] },
                "focus": { "type": "boolean" },
                "step_updates": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "step_id": { "type": "string", "minLength": 1 },
                            "status": { "type": "string", "enum": ["pending", "in_progress", "completed", "blocked", "skipped"] },
                            "notes": { "type": "string" }
                        },
                        "required": ["step_id", "status"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["plan_id"],
            "additionalProperties": false
        }),
        "request_goal_review" => json!({
            "type": "object",
            "properties": {
                "goal_id": { "type": "string", "minLength": 1 },
                "summary": { "type": "string", "minLength": 1 }
            },
            "required": ["goal_id", "summary"],
            "additionalProperties": false
        }),
        "request_plan_review" => json!({
            "type": "object",
            "properties": {
                "plan_id": { "type": "string", "minLength": 1 },
                "summary": { "type": "string", "minLength": 1 }
            },
            "required": ["plan_id", "summary"],
            "additionalProperties": false
        }),
        "harness_status" => json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
        "exec_health_check" => json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
        "operation_log" => json!({
            "type": "object",
            "properties": {
                "cursor": { "type": "integer", "minimum": 0, "default": 0 },
                "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 50 }
            },
            "additionalProperties": false
        }),
        "project_state" => json!({
            "type": "object",
            "properties": {
                "max_files": { "type": "integer", "minimum": 1, "maximum": 10000, "default": 200 }
            },
            "additionalProperties": false
        }),
        "start_task" => json!({
            "type": "object",
            "properties": {
                "objective": { "type": "string", "minLength": 1 }
            },
            "required": ["objective"],
            "additionalProperties": false
        }),
        "update_task" => json!({
            "type": "object",
            "properties": {
                "task_id": { "type": "string", "minLength": 1 },
                "completed_steps": { "type": "array", "items": { "type": "string" } },
                "pending_steps": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["task_id"],
            "additionalProperties": false
        }),
        "pause_task" | "resume_task" => json!({
            "type": "object",
            "properties": { "task_id": { "type": "string", "minLength": 1 } },
            "required": ["task_id"],
            "additionalProperties": false
        }),
        "finish_task" => json!({
            "type": "object",
            "properties": {
                "task_id": { "type": "string", "minLength": 1 },
                "summary": { "type": "string" },
                "allow_unverified": { "type": "boolean", "default": false }
            },
            "required": ["task_id"],
            "additionalProperties": false
        }),
        "task_context" => json!({
            "type": "object",
            "properties": {
                "task_id": { "type": "string" },
                "max_bytes": { "type": "integer", "minimum": 8192, "maximum": 131072, "default": 32768 }
            },
            "additionalProperties": false
        }),
        "list_task_events" => json!({
            "type": "object",
            "properties": {
                "task_id": { "type": "string", "minLength": 1 },
                "cursor": { "type": "integer", "minimum": 0, "default": 0 },
                "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 50 }
            },
            "required": ["task_id"],
            "additionalProperties": false
        }),
        "change_summary" => json!({
            "type": "object",
            "properties": { "task_id": { "type": "string" }, "change_id": { "type": "string" } },
            "additionalProperties": false
        }),
        "read_file" => json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "minLength": 1 },
                "start_line": { "type": "integer", "minimum": 1, "default": 1 },
                "end_line": { "type": "integer", "minimum": 1 },
                "max_bytes": { "type": "integer", "minimum": 1, "maximum": 1048576, "default": 32768 }
            },
            "required": ["path"],
            "additionalProperties": false
        }),
        "list_dir" => json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "default": "." },
                "recursive": { "type": "boolean", "default": false },
                "max_depth": { "type": "integer", "minimum": 1, "maximum": 20, "default": 1 },
                "max_entries": { "type": "integer", "minimum": 1, "maximum": 10000, "default": 1000 },
                "include_hidden": { "type": "boolean", "default": false },
                "include_ignored": { "type": "boolean", "default": false }
            },
            "additionalProperties": false
        }),
        "list_files" => json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "default": "." },
                "patterns": { "type": "array", "items": { "type": "string" } },
                "glob": { "type": "string", "description": "Alias for a single patterns entry" },
                "exclude_patterns": { "type": "array", "items": { "type": "string" } },
                "include_hidden": { "type": "boolean", "default": false },
                "include_ignored": { "type": "boolean", "default": false },
                "max_results": { "type": "integer", "minimum": 1, "maximum": 50000, "default": 5000 }
            },
            "additionalProperties": false
        }),
        "search_text" | "grep_text" | "grep" => json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "minLength": 1 },
                "path": { "type": "string", "default": "." },
                "glob": { "type": "string", "description": "Alias appended to include_globs" },
                "include_globs": { "type": "array", "items": { "type": "string" } },
                "exclude_globs": { "type": "array", "items": { "type": "string" } },
                "regex": { "type": "boolean", "default": false },
                "case_sensitive": { "type": "boolean", "default": false },
                "context_lines": { "type": "integer", "minimum": 0, "maximum": 20, "default": 0 },
                "max_preview_bytes": { "type": "integer", "minimum": 64, "maximum": 4096, "default": 256 },
                "max_results": { "type": "integer", "minimum": 1, "maximum": 10000, "default": 100 },
                "max_file_bytes": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 67108864,
                    "default": 2097152,
                    "description": "Skip files larger than this many bytes (default 2MiB) to avoid memory spikes"
                }
            },
            "required": ["query"],
            "additionalProperties": false
        }),
        "apply_patch" => json!({
            "type": "object",
            "properties": {
                "patch": { "type": "string", "minLength": 1 },
                "dry_run": { "type": "boolean", "default": false },
                "confirm": { "type": "boolean", "default": false },
                "reason": { "type": "string", "default": "" }
            },
            "required": ["patch"],
            "additionalProperties": false
        }),
        "patch_check" => json!({
            "type": "object",
            "properties": {
                "patch": { "type": "string", "minLength": 1 }
            },
            "required": ["patch"],
            "additionalProperties": false
        }),
        "exec_command" => json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["run", "quality_gate"], "default": "run", "description": "run executes cmd; quality_gate discovers and runs safe project verification checks without adding another public tool." },
                "preset": { "type": "string", "enum": ["quality_gate"], "description": "Backward-compatible alias for action=quality_gate." },
                "cmd": { "type": "string", "minLength": 1 },
                "workdir": { "type": "string", "default": "." },
                "timeout_ms": { "type": "integer", "minimum": 1, "maximum": 600000, "default": 30000 },
                "max_output_bytes": { "type": "integer", "minimum": 1024, "maximum": 1048576, "default": 12288 },
                "yield_time_ms": { "type": "integer", "minimum": 0, "maximum": 30000, "default": 1000 },
                "tty": { "type": "boolean", "default": false },
                "stdin": { "type": "string", "default": "" },
                "checks": { "type": "array", "items": { "type": "string" }, "description": "For quality_gate, filter discovered checks by id (for example package:test) or category (lint/check/typecheck/test/build/format)." },
                "dry_run": { "type": "boolean", "default": false },
                "stop_on_failure": { "type": "boolean", "default": true },
                "confirm": { "type": "boolean", "default": false },
                "filesystem_scope": { "type": "string", "enum": ["workspace"], "default": "workspace" },
                "reason": { "type": "string", "default": "" }
            },
            "additionalProperties": false
        }),
        "write_stdin" => json!({
            "type": "object",
            "properties": {
                "command_id": { "type": "string", "minLength": 1, "description": "Stable workspace-owned command id." },
                "session_id": { "type": "string", "minLength": 1 },
                "chars": { "type": "string", "default": "" },
                "yield_time_ms": { "type": "integer", "minimum": 0, "maximum": 30000, "default": 1000 },
                "max_output_bytes": { "type": "integer", "minimum": 1, "maximum": 1048576, "default": 32768 }
            },
            "additionalProperties": false
        }),
        "kill_session" => json!({
            "type": "object",
            "properties": {
                "command_id": { "type": "string", "minLength": 1, "description": "Stable workspace-owned command id." },
                "session_id": { "type": "string", "minLength": 1 },
                "signal": { "type": "string", "enum": ["TERM", "KILL", "INT"], "default": "TERM" },
                "wait_ms": { "type": "integer", "minimum": 0, "maximum": 30000, "default": 5000 },
                "max_output_bytes": { "type": "integer", "minimum": 1, "maximum": 1048576, "default": 32768 }
            },
            "additionalProperties": false
        }),
        "read_output" => json!({
            "type": "object",
            "properties": {
                "output_ref": { "type": "string", "minLength": 1 },
                "stream": { "type": "string", "enum": ["stdout", "stderr"] },
                "offset": { "type": "integer", "minimum": 0, "default": 0 },
                "limit": { "type": "integer", "minimum": 1, "maximum": 1048576, "default": 4096 },
                "query": { "type": "string", "minLength": 1 },
                "regex": { "type": "boolean", "default": false },
                "case_sensitive": { "type": "boolean", "default": false },
                "max_matches": { "type": "integer", "minimum": 1, "maximum": 200, "default": 50 }
            },
            "required": ["output_ref"],
            "additionalProperties": false
        }),
        "git_status" => json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "default": "." },
                "include_untracked": { "type": "boolean", "default": true },
                "max_entries": { "type": "integer", "minimum": 1, "maximum": 10000, "default": 500 }
            },
            "additionalProperties": false
        }),
        "git_diff" => json!({
            "type": "object",
            "properties": {
                "paths": { "type": "array", "items": { "type": "string" }, "default": [] },
                "staged": { "type": "boolean", "default": false },
                "unstaged": { "type": "boolean", "default": true },
                "context_lines": { "type": "integer", "minimum": 0, "maximum": 20, "default": 3 },
                "max_bytes": { "type": "integer", "minimum": 1024, "maximum": 1048576, "default": 65536 }
            },
            "additionalProperties": false
        }),
        "git_log" => json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "default": "." },
                "ref": { "type": "string", "default": "HEAD" },
                "max_count": { "type": "integer", "minimum": 1, "maximum": 100, "default": 20 },
                "skip": { "type": "integer", "minimum": 0, "maximum": 10000, "default": 0 }
            },
            "additionalProperties": false
        }),
        "git_show" => json!({
            "type": "object",
            "properties": {
                "rev": { "type": "string", "default": "HEAD" },
                "path": { "type": "string" },
                "paths": { "type": "array", "items": { "type": "string" } },
                "include_diff": { "type": "boolean", "default": true },
                "context_lines": { "type": "integer", "minimum": 0, "maximum": 20, "default": 3 },
                "max_bytes": { "type": "integer", "minimum": 1, "maximum": 1048576, "default": 65536 }
            },
            "additionalProperties": false
        }),
        "git_blame" => json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "minLength": 1 },
                "rev": { "type": "string" },
                "start_line": { "type": "integer", "minimum": 1, "default": 1 },
                "end_line": { "type": "integer", "minimum": 1 },
                "max_lines": { "type": "integer", "minimum": 1, "maximum": 1000, "default": 200 }
            },
            "required": ["path"],
            "additionalProperties": false
        }),
        "request_permissions" => json!({
            "type": "object",
            "properties": {
                "tool_name": {
                    "type": "string",
                    "enum": ["exec_command", "apply_patch"]
                },
                "permission": {
                    "type": "string",
                    "enum": [
                        "network",
                        "destructive_command",
                        "long_timeout",
                        "sensitive_env",
                        "shell_expansion",
                        "inline_script",
                        "privileged_executable",
                        "write_generated_or_ignored"
                    ]
                },
                "reason": { "type": "string", "minLength": 1 },
                "arguments": { "type": "object", "additionalProperties": true },
                "scope": {
                    "type": "string",
                    "enum": ["once", "session"],
                    "default": "once"
                },
                "ttl_seconds": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 3600,
                    "default": 300
                }
            },
            "required": ["tool_name", "permission", "reason", "arguments"],
            "additionalProperties": false
        }),
        "set_default_cwd" => json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "default": "." }
            },
            "additionalProperties": false
        }),
        "view_image" => json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "minLength": 1 },
                "max_bytes": { "type": "integer", "minimum": 1024, "maximum": 10485760, "default": 5242880 },
                "max_width": { "type": "integer", "minimum": 1, "maximum": 10000, "default": 2000 },
                "max_height": { "type": "integer", "minimum": 1, "maximum": 10000, "default": 2000 },
                "auto_resize": { "type": "boolean", "default": true },
                "output": { "type": "string", "enum": ["mcp_image", "data_url"], "default": "mcp_image" }
            },
            "required": ["path"],
            "additionalProperties": false
        }),
        _ => json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
    }
}
