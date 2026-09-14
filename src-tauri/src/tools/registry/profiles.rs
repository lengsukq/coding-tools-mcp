use serde_json::{json, Value};

use super::{input_schema, tool_definition, P0_TOOLS};

/// Legacy-compatible core surface. It keeps lifecycle-specific tool names while
/// also exposing the Stable Tool API v2 managers so MCP clients can migrate
/// without a flag day.
pub const CORE_TOOLS: &[&str] = &[
    "server_info",
    "history_manage",
    "planning_manage",
    "task_manage",
    "history_session_bootstrap",
    "history_session_checkpoint",
    "history_session_validate",
    "history_session_search",
    "history_session_read",
    "planning_state",
    "create_goal",
    "update_goal",
    "create_plan",
    "update_plan",
    "request_goal_review",
    "request_plan_review",
    "capability_health_check",
    "check_exec_environment",
    "get_default_cwd",
    "set_default_cwd",
    "list_skills",
    "get_skill",
    "read_file",
    "list_dir",
    "list_files",
    "search_text",
    "grep_text",
    "apply_patch",
    "exec_command",
    "write_stdin",
    "kill_session",
    "read_output",
    "git_status",
    "git_diff",
    "git_log",
    "git_show",
    "git_blame",
    "request_permissions",
    "view_image",
];

/// Compact default surface: keep ordinary development tools plus the Stable Tool
/// API v2 managers while moving lifecycle-specific compatibility tools, skills,
/// harness internals, and permission helpers to legacy/advanced profiles.
pub const COMPACT_TOOLS: &[&str] = &[
    "server_info",
    "history_manage",
    "planning_manage",
    "read_file",
    "list_dir",
    "list_files",
    "search_text",
    "grep_text",
    "apply_patch",
    "patch_check",
    "exec_command",
    "write_stdin",
    "kill_session",
    "read_output",
    "git_status",
    "git_diff",
    "git_log",
    "git_show",
    "git_blame",
    "view_image",
];

pub const CORE_READ_ONLY_TOOLS: &[&str] = &[
    "server_info",
    "planning_state",
    "check_exec_environment",
    "get_default_cwd",
    "list_skills",
    "get_skill",
    "set_default_cwd",
    "read_file",
    "list_dir",
    "list_files",
    "search_text",
    "grep_text",
    "read_output",
    "git_status",
    "git_diff",
    "git_log",
    "git_show",
    "git_blame",
    "request_permissions",
    "view_image",
];

pub const ALLOWED_TOOLS: &[&str] = &[
    "harness_status",
    "operation_log",
    "server_info",
    "history_manage",
    "planning_manage",
    "task_manage",
    "history_session_bootstrap",
    "history_session_checkpoint",
    "history_session_validate",
    "history_session_search",
    "history_session_read",
    "planning_state",
    "create_goal",
    "update_goal",
    "create_plan",
    "update_plan",
    "request_goal_review",
    "request_plan_review",
    "capability_health_check",
    "check_exec_environment",
    "exec_health_check",
    "get_default_cwd",
    "set_default_cwd",
    "list_skills",
    "get_skill",
    "read_file",
    "list_dir",
    "list_files",
    "search_text",
    "grep_text",
    "grep",
    "apply_patch",
    "patch_check",
    "exec_command",
    "write_stdin",
    "kill_session",
    "read_output",
    "git_status",
    "git_diff",
    "git_log",
    "git_show",
    "git_blame",
    "project_state",
    "start_task",
    "update_task",
    "pause_task",
    "resume_task",
    "finish_task",
    "task_context",
    "list_task_events",
    "change_summary",
    "request_permissions",
    "view_image",
];

pub const READ_ONLY_TOOLS: &[&str] = &[
    "harness_status",
    "operation_log",
    "server_info",
    "history_session_search",
    "history_session_read",
    "planning_state",
    "check_exec_environment",
    "exec_health_check",
    "get_default_cwd",
    "list_skills",
    "get_skill",
    "read_file",
    "list_dir",
    "list_files",
    "search_text",
    "grep_text",
    "grep",
    "read_output",
    "git_status",
    "git_diff",
    "git_log",
    "git_show",
    "git_blame",
    "request_permissions",
    "view_image",
    "patch_check",
    "project_state",
    "task_context",
    "list_task_events",
    "change_summary",
];

pub fn is_allowed_tool(name: &str) -> bool {
    ALLOWED_TOOLS.contains(&name)
}

pub fn canonical_tool_name(name: &str) -> &str {
    match name {
        "grep" => "grep_text",
        _ => name,
    }
}

pub fn normalize_tool_profile(profile: &str) -> &'static str {
    match profile {
        "compact" => "compact",
        "advanced" => "advanced",
        "read-only" => "read-only",
        "compat-readonly-all" => "compat-readonly-all",
        _ => "core",
    }
}

pub fn exposed_tool_names(tool_profile: &str) -> Vec<&'static str> {
    let names = match normalize_tool_profile(tool_profile) {
        "compact" => COMPACT_TOOLS.to_vec(),
        "read-only" => CORE_READ_ONLY_TOOLS.to_vec(),
        "advanced" | "compat-readonly-all" => P0_TOOLS.iter().map(|tool| tool.name).collect(),
        _ => CORE_TOOLS.to_vec(),
    };

    names
        .into_iter()
        .filter(|name| !CLIENT_HIDDEN_TOOLS.contains(name))
        .collect()
}

/// Legacy compatibility tools that remain callable by name but must not be advertised to
/// MCP clients. `request_permissions` cannot persist a grant for ChatGPT in trusted/safe
/// mode, so advertising it encourages retry loops after policy errors.
const CLIENT_HIDDEN_TOOLS: &[&str] = &["request_permissions"];

pub fn list_tools() -> Vec<Value> {
    list_tools_for_profile("full")
}

fn compact_description<'a>(name: &str, fallback: &'a str) -> &'a str {
    match name {
        "server_info" => "Return compact server and workspace metadata.",
        "history_manage" => "Manage project history through one stable action-based API.",
        "planning_manage" => "Manage Goal and Plan state through one stable action-based API. Goal/Plan writes remain allowed in Plan mode.",
        "task_manage" => "Manage durable task state through one stable action-based API.",
        "history_session_bootstrap" => "Create or resume a history archive when explicitly requested; returns bounded metadata only.",
        "history_session_checkpoint" => "Append a redacted checkpoint when session recording is enabled; session target may be omitted for lazy initialization.",
        "history_session_validate" => "Validate or rebuild history indexes without deleting archives.",
        "history_session_search" => "Search indexed session archives and return bounded matches.",
        "history_session_read" => "Read a bounded UTF-8 page from one selected session archive.",
        "read_file" => "Read a bounded UTF-8 text range from a workspace file.",
        "search_text" | "grep_text" => "Search workspace text with bounded previews.",
        "apply_patch" => "Apply a workspace patch and return a change summary.",
        "exec_command" => "Run an allowed workspace command with bounded output.",
        "write_stdin" => "Write to a running command session with bounded output.",
        "read_output" => "Read a bounded page from a command output session.",
        "git_diff" => "Return bounded Git diff output.",
        _ => fallback,
    }
}

pub fn list_tools_for_profile(tool_profile: &str) -> Vec<Value> {
    let compat = tool_profile == "compat-readonly-all";
    exposed_tool_names(tool_profile)
        .into_iter()
        .filter_map(|name| {
            tool_definition(name).map(|tool| {
                let name = tool.name;
                let title = tool.title;
                let description = tool.description;
                let (read_only, destructive, open_world) =
                    (tool.read_only, tool.destructive, tool.open_world);
                let (read_only, destructive, open_world) = if compat {
                    (true, false, false)
                } else {
                    (read_only, destructive, open_world)
                };
                json!({
                    "name": name,
                    "title": title,
                    "description": if tool_profile == "compact" {
                        compact_description(name, description)
                    } else {
                        description
                    },
                    "inputSchema": input_schema(name),
                    "annotations": {
                        "title": title,
                        "readOnlyHint": read_only,
                        "destructiveHint": destructive,
                        "idempotentHint": read_only,
                        "openWorldHint": open_world
                    }
                })
            })
        })
        .collect()
}
