use serde_json::{json, Value};

pub const TOOL_API_VERSION: &str = "2";

mod profiles;
mod schema;

pub use profiles::{
    canonical_tool_name, exposed_tool_names, is_allowed_tool, list_tools, list_tools_for_profile,
    normalize_tool_profile, ALLOWED_TOOLS, COMPACT_TOOLS, CORE_READ_ONLY_TOOLS, CORE_TOOLS,
    READ_ONLY_TOOLS,
};
pub use schema::input_schema;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolDefinition {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub read_only: bool,
    pub destructive: bool,
    pub open_world: bool,
}

impl ToolDefinition {
    pub fn is_mutating(self) -> bool {
        !self.read_only || matches!(self.name, "set_default_cwd")
    }
}

macro_rules! tool_definitions {
    ($(($name:expr, $title:expr, $description:expr, $read_only:expr, $destructive:expr, $open_world:expr,)),* $(,)?) => {
        &[
            $(ToolDefinition {
                name: $name,
                title: $title,
                description: $description,
                read_only: $read_only,
                destructive: $destructive,
                open_world: $open_world,
            }),*
        ]
    };
}

pub const P0_TOOLS: &[ToolDefinition] = tool_definitions![
    (
        "harness_status",
        "Harness status",
        "Return durable task, workspace, capability, and recovery status.",
        true,
        false,
        false,
    ),
    (
        "operation_log",
        "Operation log",
        "Return Workspace-level operation history independent of Task state.",
        true,
        false,
        false,
    ),
    (
        "server_info",
        "Server info",
        "Return server, workspace, auth, profile, and exposed-tool metadata.",
        true,
        false,
        false,
    ),
    (
        "history_manage",
        "History manager",
        "Stable Tool API v2 entry point for history bootstrap, checkpoint, validation, search, and bounded reads.",
        false,
        false,
        false,
    ),
    (
        "planning_manage",
        "Planning manager",
        "Stable Tool API v2 entry point for Goal and Plan state, lifecycle updates, and review requests.",
        false,
        false,
        false,
    ),
    (
        "task_manage",
        "Task manager",
        "Stable Tool API v2 entry point for durable task state, lifecycle, events, and change summaries.",
        false,
        false,
        false,
    ),
    (
        "history_session_bootstrap",
        "Initialize or restore development session",
        "At the start of every new ChatGPT conversation, call this exactly once before the first response and pass the user's verbatim initial_user_input. It creates or resumes a lossless archive, then returns bounded current state and search/read guidance rather than all history.",
        false,
        false,
        false,
    ),
    (
        "history_session_checkpoint",
        "Save development checkpoint",
        "Append an idempotent, redacted development checkpoint. Pass session_key and expected_path exactly as returned by history_session_bootstrap, plus the user's verbatim raw_user_input; the server cannot read ChatGPT transcripts that were not passed as arguments. Changed content for the same turn_id is preserved as a revision.",
        false,
        false,
        false,
    ),
    (
        "history_session_validate",
        "Validate session archive",
        "Validate history numbering, files, session mappings, and optionally rebuild the derived index without deleting history.",
        false,
        false,
        false,
    ),
    (
        "history_session_search",
        "Search session archive",
        "Search lossless history archives by deterministic keywords and return a bounded page of ranked locations and snippets. Use history_session_read to retrieve exact source text.",
        true,
        false,
        false,
    ),
    (
        "history_session_read",
        "Read session archive",
        "Read one lossless numeric Markdown archive by number or a path returned from history_session_search. Responses are UTF-8-safe pages: max_bytes defaults to 16 KiB and is capped at 64 KiB; follow next_cursor to recover the complete source.",
        true,
        false,
        false,
    ),
    (
        "planning_state",
        "Planning state",
        "Return project-local Goal and Plan state stored inside the configured workspace.",
        true,
        false,
        false,
    ),
    (
        "capability_health_check",
        "Capability health check",
        "Check MCP authentication, workspace access, and exposed tool capability state to distinguish capability mismatch from permission problems.",
        true,
        false,
        false,
    ),
    (
        "create_goal",
        "Create goal",
        "AI conversation workflow: create and focus a durable project Goal when the user's request benefits from an explicit objective, success criteria, and constraints. No pre-approval is required.",
        false,
        false,
        false,
    ),
    (
        "update_goal",
        "Update goal",
        "Update Goal status, focus, constraints, or completed success criteria.",
        false,
        false,
        false,
    ),
    (
        "create_plan",
        "Create plan",
        "AI conversation workflow: create, activate, and focus a durable project Plan, optionally linked to a Goal. This planning-metadata write is allowed in Plan mode; no pre-approval is required.",
        false,
        false,
        false,
    ),
    (
        "update_plan",
        "Update plan",
        "Update Plan status, focus, and individual step progress. This planning-metadata write is allowed in Plan mode.",
        false,
        false,
        false,
    ),
    (
        "request_goal_review",
        "Request goal review",
        "Submit a completed Goal to the desktop app for human acceptance. This does not archive the Goal; only the human UI can accept and archive it.",
        false,
        false,
        false,
    ),
    (
        "request_plan_review",
        "Request plan review",
        "Submit a completed Plan to the desktop app for human acceptance. This does not archive the Plan; only the human UI can accept and archive it.",
        false,
        false,
        false,
    ),
    (
        "project_state",
        "Project state",
        "Return the current project, task, change, and verification state.",
        true,
        false,
        false,
    ),
    (
        "start_task",
        "Start task",
        "Start a durable coding task and capture the workspace baseline.",
        false,
        false,
        false,
    ),
    (
        "update_task",
        "Update task",
        "Update task steps and durable progress.",
        false,
        false,
        false,
    ),
    (
        "pause_task",
        "Pause task",
        "Pause the active coding task.",
        false,
        false,
        false,
    ),
    (
        "resume_task",
        "Resume task",
        "Resume a paused or failed coding task.",
        false,
        false,
        false,
    ),
    (
        "finish_task",
        "Finish task",
        "Finish a task with verification status and change summary.",
        false,
        false,
        false,
    ),
    (
        "task_context",
        "Task context",
        "Return a bounded durable task context for a new conversation.",
        true,
        false,
        false,
    ),
    (
        "list_task_events",
        "List task events",
        "Read task event history with pagination.",
        true,
        false,
        false,
    ),
    (
        "change_summary",
        "Change summary",
        "Explain what changed, why, and what evidence exists.",
        true,
        false,
        false,
    ),
    (
        "check_exec_environment",
        "Check exec environment",
        "Return lightweight exec_command sandbox and environment status known to the server.",
        true,
        false,
        false,
    ),
    (
        "exec_health_check",
        "Exec health check",
        "Verify the exec worker, session creation, command execution, and stdout/stderr capture.",
        true,
        false,
        false,
    ),
    (
        "get_default_cwd",
        "Get default cwd",
        "Return the current default cwd inside the workspace.",
        true,
        false,
        false,
    ),
    (
        "set_default_cwd",
        "Set default cwd",
        "Set the default cwd for relative tool paths inside the workspace.",
        true,
        false,
        false,
    ),
    (
        "read_file",
        "Read file",
        "Read a UTF-8 text file slice inside the configured workspace.",
        true,
        false,
        false,
    ),
    (
        "list_dir",
        "List directory",
        "List directory entries inside the configured workspace.",
        true,
        false,
        false,
    ),
    (
        "list_files",
        "List files",
        "List workspace files using glob filters.",
        true,
        false,
        false,
    ),
    (
        "search_text",
        "Search text",
        "Search UTF-8 workspace files for text or regex matches.",
        true,
        false,
        false,
    ),
    (
        "grep_text",
        "Grep workspace text",
        "Search workspace text with grep-style regex, glob, context, and bounded results.",
        true,
        false,
        false,
    ),
    (
        "apply_patch",
        "Apply patch",
        "Apply a patch envelope transactionally inside the workspace.",
        false,
        true,
        false,
    ),
    (
        "patch_check",
        "Check patch",
        "Validate a patch without changing the workspace.",
        true,
        false,
        false,
    ),
    (
        "exec_command",
        "Execute command",
        "Run a bounded command in the workspace under runtime policy.",
        false,
        true,
        true,
    ),
    (
        "write_stdin",
        "Write stdin",
        "Write characters to a workspace-owned running command. Prefer command_id; legacy session_id remains accepted.",
        false,
        false,
        false,
    ),
    (
        "kill_session",
        "Kill session",
        "Terminate a workspace-owned running command. Prefer command_id; legacy session_id remains accepted.",
        false,
        true,
        false,
    ),
    (
        "read_output",
        "Read output",
        "Read retained stdout or stderr by output_ref with per-stream byte offset pagination.",
        true,
        false,
        false,
    ),
    (
        "list_skills",
        "List skills",
        "List skills discovered from the enabled IDE and coding-agent providers. Skill bodies are loaded separately on demand.",
        true,
        false,
        false,
    ),
    (
        "get_skill",
        "Get skill",
        "Load one discovered SKILL.md by id or unique name, including its full workflow body.",
        true,
        false,
        false,
    ),
    (
        "git_status",
        "Git status",
        "Return git working tree status for the workspace.",
        true,
        false,
        false,
    ),
    (
        "git_diff",
        "Git diff",
        "Return unified git diff for workspace changes.",
        true,
        false,
        false,
    ),
    (
        "git_log",
        "Git log",
        "Return recent git commits with bounded structured metadata.",
        true,
        false,
        false,
    ),
    (
        "git_show",
        "Git show",
        "Return bounded git show output for a revision.",
        true,
        false,
        false,
    ),
    (
        "git_blame",
        "Git blame",
        "Return bounded git blame metadata for a workspace file.",
        true,
        false,
        false,
    ),
    (
        "request_permissions",
        "Request permissions",
        "Request a scoped permission grant for dangerous runtime operations.",
        true,
        false,
        false,
    ),
    (
        "view_image",
        "View image",
        "Return a workspace image as MCP image content.",
        true,
        false,
        false,
    ),
];

pub fn tool_definition(name: &str) -> Option<&'static ToolDefinition> {
    let canonical = canonical_tool_name(name);
    P0_TOOLS.iter().find(|tool| tool.name == canonical)
}

pub fn static_tool_is_mutating(name: &str) -> bool {
    tool_definition(name)
        .map(|tool| tool.is_mutating())
        .unwrap_or(false)
}

pub fn tool_api_descriptor() -> Value {
    json!({
        "version": TOOL_API_VERSION,
        "profile": "stable-aggregate",
        "aggregate_tools": ["history_manage", "planning_manage", "task_manage"],
        "legacy_compatibility": true
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{
        canonical_tool_name, input_schema, list_tools_for_profile, static_tool_is_mutating,
        tool_api_descriptor, tool_definition,
    };

    #[test]
    fn tool_definition_is_the_static_mutability_source() {
        assert!(static_tool_is_mutating("apply_patch"));
        assert!(static_tool_is_mutating("exec_command"));
        assert!(static_tool_is_mutating("set_default_cwd"));
        assert!(!static_tool_is_mutating("read_file"));
        assert!(!static_tool_is_mutating("grep"));

        assert_eq!(canonical_tool_name("grep"), "grep_text");
        assert_eq!(
            tool_definition("grep").map(|tool| tool.name),
            Some("grep_text")
        );
    }

    #[test]
    fn core_catalog_excludes_non_persistent_permission_tool() {
        let tools = list_tools_for_profile("core");
        let names: Vec<_> = tools
            .iter()
            .map(|tool| tool["name"].as_str().expect("tool name"))
            .collect();
        let unique: HashSet<_> = names.iter().copied().collect();

        assert_eq!(tools.len(), 38);
        assert_eq!(unique.len(), tools.len());
        assert!(names.contains(&"history_manage"));
        assert!(names.contains(&"planning_manage"));
        assert!(names.contains(&"task_manage"));
        assert!(names.contains(&"history_session_bootstrap"));
        assert!(names.contains(&"history_session_checkpoint"));
        assert!(names.contains(&"history_session_validate"));
        assert!(names.contains(&"history_session_search"));
        assert!(names.contains(&"history_session_read"));
        assert!(names.contains(&"planning_state"));
        assert!(names.contains(&"create_goal"));
        assert!(names.contains(&"update_goal"));
        assert!(names.contains(&"create_plan"));
        assert!(names.contains(&"update_plan"));
        assert!(names.contains(&"request_goal_review"));
        assert!(names.contains(&"request_plan_review"));
        assert!(!names.contains(&"set_planning_mode"));
        assert!(names.contains(&"grep_text"));
        assert!(!names.contains(&"grep"));
        assert!(!names.contains(&"request_permissions"));

        for name in names {
            let schema = input_schema(name);
            assert_eq!(schema["type"], "object", "{name} schema type");
            assert!(schema["properties"].is_object(), "{name} properties");
            assert!(schema.get("oneOf").is_none(), "{name} oneOf");
            assert!(schema.get("anyOf").is_none(), "{name} anyOf");
            assert!(schema.get("$ref").is_none(), "{name} ref");
        }
    }

    #[test]
    fn compact_catalog_uses_stable_v2_aggregate_managers() {
        let tools = list_tools_for_profile("compact");
        let names: Vec<_> = tools
            .iter()
            .map(|tool| tool["name"].as_str().expect("tool name"))
            .collect();

        assert_eq!(names.len(), 20);
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"apply_patch"));
        assert!(names.contains(&"exec_command"));
        assert!(names.contains(&"history_manage"));
        assert!(names.contains(&"planning_manage"));
        assert!(!names.contains(&"task_manage"));
        assert!(!names.contains(&"history_session_search"));
        assert!(!names.contains(&"history_session_read"));
        assert!(!names.contains(&"planning_state"));
        assert!(!names.contains(&"list_skills"));
        assert!(!names.contains(&"request_permissions"));
        assert!(!names.contains(&"check_exec_environment"));
        assert!(!names.contains(&"get_default_cwd"));
        assert!(!names.contains(&"set_default_cwd"));
        assert_eq!(tool_api_descriptor()["version"], "2");
    }
}
