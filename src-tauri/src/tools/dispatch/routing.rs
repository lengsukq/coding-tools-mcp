use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde_json::{json, Value};

use crate::tools::context::ToolContext;
use crate::tools::workspace::{tool_err, tool_ok, WorkspaceError, WorkspaceResult};
use crate::tools::{exec, file, git, history, image_tool, manage, patch, planning, session, skill};

pub(super) fn execute_tool(ctx: &ToolContext, name: &str, args: &Value) -> WorkspaceResult<Value> {
    let ws = &ctx.workspace;
    match name {
        "history_manage" => manage::history_manage(ctx, args),
        "planning_manage" => manage::planning_manage(ctx, args),
        "task_manage" => manage::task_manage(ctx, args),
        "history_session_bootstrap" => history::bootstrap(ctx, args),
        "history_session_checkpoint" => history::checkpoint(ctx, args),
        "history_session_validate" => history::validate(ctx, args),
        "history_session_search" => history::search(ctx, args),
        "history_session_read" => history::read(ctx, args),
        "capability_health_check" => Ok(capability_health_check(ctx)),
        "planning_state" => planning::planning_state(ctx, args),
        "create_goal" => planning::create_goal(ctx, args),
        "update_goal" => planning::update_goal(ctx, args),
        "create_plan" => planning::create_plan(ctx, args),
        "update_plan" => planning::update_plan(ctx, args),
        "request_goal_review" => planning::request_goal_review(ctx, args),
        "request_plan_review" => planning::request_plan_review(ctx, args),
        "server_info" => super::server_info(ctx),
        "check_exec_environment" => super::check_exec_environment(ctx),
        "exec_health_check" => exec::exec_health_check(ctx),
        "get_default_cwd" => super::get_default_cwd(ctx),
        "set_default_cwd" => super::set_default_cwd(ctx, args),
        "list_skills" => skill::list_skills(ctx, args),
        "get_skill" => skill::get_skill(ctx, args),
        "read_file" => file::read_file(ws, args),
        "list_dir" => file::list_dir(ws, args),
        "list_files" => file::list_files(ws, args),
        "search_text" | "grep_text" | "grep" => file::search_text(ws, args),
        "patch_check" => patch::patch_check(ctx, args),
        "apply_patch" => patch::apply_patch(ctx, args),
        "exec_command" => exec::exec_command(ctx, args),
        "read_output" => session::read_output(&ctx.sessions, args),
        "write_stdin" => session::write_stdin(&ctx.sessions, args),
        "kill_session" => session::kill_session(&ctx.sessions, args),
        "git_status" => git::git_status(ws, args),
        "git_diff" => git::git_diff(ws, args),
        "git_log" => git::git_log(ws, args),
        "git_show" => git::git_show(ws, args),
        "git_blame" => git::git_blame(ws, args),
        "view_image" => image_tool::view_image(ws, args),
        "request_permissions" => request_permissions(ctx, args),
        _ => Err(WorkspaceError::ToolDetails {
            code: "INVALID_ARGUMENT",
            message: format!("Unknown tool: {name}"),
            category: "validation",
            retryable: false,
            details: json!({
                "reason": "unknown_tool",
                "suggestion": "Call capability_health_check and compare the available server tool list. If the tool exists on the server but is missing in the client session, refresh MCP tool discovery before retrying."
            }),
        }),
    }
}

fn capability_health_check(ctx: &ToolContext) -> Value {
    let tools = crate::tools::registry::exposed_tool_names(ctx.tool_profile.as_str());
    let mut hasher = DefaultHasher::new();
    tools.hash(&mut hasher);
    json!({
        "authentication": { "status": "available" },
        "authorization": { "mode": ctx.policy.permission_mode.as_str() },
        "workspace": {
            "path": ctx.workspace.root().display().to_string(),
            "status": "available"
        },
        "capability": {
            "server_tool_count": tools.len(),
            "tool_profile": ctx.tool_profile.as_str(),
            "tool_fingerprint": format!("{:x}", hasher.finish()),
            "server_version": env!("CARGO_PKG_VERSION"),
            "tool_api": crate::tools::registry::tool_api_descriptor()
        },
        "recommendation": "If client tools are missing while server capability is healthy, refresh MCP tool discovery instead of requesting permissions."
    })
}

fn request_permissions(ctx: &ToolContext, args: &Value) -> WorkspaceResult<Value> {
    if ctx.policy.skip_permission_gates() {
        return Ok(tool_ok(json!({
            "ok": true,
            "status": "granted",
            "grant_id": "dangerously-skip-all-permissions",
            "expires_at": null,
            "constraints": {
                "mode": "dangerous",
                "workspace": ctx.workspace.root_display(),
                "requested": args
            },
            "warnings": [
                "dangerous permission mode is enabled; permission-gated operations are auto-granted"
            ]
        })));
    }

    let mut output = tool_err(WorkspaceError::ToolDetails {
        code: "ELICITATION_UNSUPPORTED",
        message: "Permission elicitation is not available for this client. Do not retry request_permissions; it cannot create a persistent grant.".into(),
        category: "permission",
        retryable: false,
        details: json!({ "requested": args }),
    });
    if let Some(object) = output.as_object_mut() {
        object.insert("status".into(), json!("unsupported"));
        object.insert("grant_id".into(), Value::Null);
        object.insert("expires_at".into(), Value::Null);
        object.insert(
            "next_actions".into(),
            json!([
                "Do not retry request_permissions.",
                "If the original operation returned DANGEROUS_OPERATION_REQUIRES_CONFIRMATION and the user already explicitly authorized it, retry the original tool with confirm=true."
            ]),
        );
    }
    Ok(output)
}
