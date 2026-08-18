use std::sync::Arc;

use serde_json::{json, Value};

use crate::agent_context::{render_skill_catalog, AgentContextRuntimeConfig};
use crate::usage::ServiceUsage;

use crate::tools::{
    call_tool, list_tools_for_profile, wrap_mcp_tool_result, SharedToolContext, ToolContext,
    Workspace,
};
use crate::workspace::AuthConfig;

pub type SharedState = SharedToolContext;

pub fn handle_request(state: &SharedState, body: &Value) -> Value {
    let method = body.get("method").and_then(Value::as_str).unwrap_or("");
    let id = body.get("id").cloned().unwrap_or(Value::Null);
    let params = body.get("params").cloned().unwrap_or(Value::Null);

    if id.is_null() && method.starts_with("notifications/") {
        return Value::Null;
    }

    let result = match method {
        "initialize" => {
            Ok(initialize_result(state))
        }
        "ping" => Ok(serde_json::json!({})),
        "tools/list" => {
            let tools = list_tools_for_profile(&state.tool_profile);
            state.record_context_block("tool_definitions", &json!(tools));
            Ok(json!({ "tools": tools }))
        }
        "tools/call" => handle_tools_call(state, &params),
        _ => Err(serde_json::json!({
            "code": -32601,
            "message": format!("Method not found: {method}")
        })),
    };

    match result {
        Ok(result) => serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => serde_json::json!({ "jsonrpc": "2.0", "id": id, "error": error }),
    }
}

fn initialize_result(state: &SharedState) -> Value {
    let base_instructions = "Use these tools only for local coding operations inside the configured workspace. Planning mode is controlled exclusively by the desktop app: every tool response may contain planning_context with the authoritative current mode, revision, focused Goal, and focused Plan. Never assume or attempt to change the mode from chat. Goal and Plan records are AI-driven conversation artifacts: when a user request benefits from durable tracking, create_goal and create_plan may be called directly from the conversation without asking the user to fill desktop forms or approve a proposal first. Keep their criteria and steps updated as work progresses. When the work is ready for acceptance, call request_goal_review and/or request_plan_review with a concise verification summary. Never archive or claim final acceptance yourself; only the human desktop review action can accept and archive. If a review is rejected, continue from the reactivated Goal/Plan and incorporate the human feedback. In Plan mode, project writes and command execution are intentionally blocked by the server. In Goal mode, project mutations require an active focused Goal and must respect any focused Plan relationship/status. If the client reports missing tools while server authorization is still valid, treat it as a capability discovery mismatch rather than a permission loss: refresh the MCP session/tool list before requesting permissions. At the start of every new ChatGPT conversation, before answering the user's first request, call history_session_bootstrap exactly once and pass the user's verbatim first request as initial_user_input. Treat bootstrap as required conversation initialization: it creates or resumes a lossless Markdown archive and returns bounded current state, not all history. Use history_session_search followed by history_session_read only when exact earlier context is needed. history_session_read returns a bounded UTF-8-safe page; follow next_cursor with the returned content hash until the relevant archive is complete. Repeated successful bootstrap calls in the same conversation resume the same session and must not create duplicates. Preserve session_key and current_path returned by bootstrap, then pass them unchanged as session_key and expected_path to every history_session_checkpoint call. After completing each user-requested task in the conversation, call history_session_checkpoint before the final response and pass that user's verbatim request as raw_user_input. Only state that progress was saved after checkpoint returns ok=true with the same session_key and path. The server cannot access ChatGPT transcript text that was not provided as a tool argument; persistence is not automatic background persistence. If an operation returns DANGEROUS_OPERATION_REQUIRES_CONFIRMATION, do not request a separate permission grant. Only retry the same tool with confirm=true when the user's request already clearly authorizes that dangerous operation; otherwise ask the user for confirmation.";
    let base_instructions = if state.tool_profile == "compact" {
        "Use these tools only for local coding operations inside the configured workspace. This profile uses Stable Tool API v2: use history_manage, planning_manage, and task_manage with their action field instead of relying on lifecycle-specific tool names. The desktop app controls permissions and planning mode. History recording is controlled by the workspace setting; history bootstrap is optional and is never required before the first response. Use history_manage action=search/read only when exact older context is needed. Selected history context below is a bounded snapshot; do not repeat it in tool responses. Checkpoints may omit session_key and expected_path because the server can lazily create the current workspace session. If a dangerous operation requires confirmation, retry only the original tool with confirm=true when the user's request clearly authorizes it."
    } else {
        base_instructions
    };
    let current_ai_instructions = state.current_ai_instructions();
    let configured = if current_ai_instructions.trim().is_empty() {
        String::new()
    } else {
        format!(
            "Configured agent instructions (global, workspace, repository sources):\n{}",
            current_ai_instructions.trim()
        )
    };
    let current_skills = state.current_skills();
    let skill_catalog = if state.tool_profile == "compact" {
        String::new()
    } else {
        render_skill_catalog(&current_skills)
    };
    let history_context = crate::tools::history::context_snapshot(state)
        .ok()
        .flatten()
        .map(|value| {
            format!(
                "Selected workspace history context (revisioned snapshot; do not repeat in tool results):\n{}",
                serde_json::to_string(&value).unwrap_or_else(|_| "{}".into())
            )
        })
        .unwrap_or_default();
    state.record_context_block(
        "initialization_rules",
        &json!({
            "base": base_instructions,
            "configured": configured,
            "skills": skill_catalog
        }),
    );
    if !history_context.is_empty() {
        state.record_context_block("history_snapshot", &Value::String(history_context.clone()));
    }
    let instructions = [base_instructions, configured.as_str(), skill_catalog.as_str(), history_context.as_str()]
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    serde_json::json!({
        "protocolVersion": "2025-06-18",
        "capabilities": {
            "tools": { "listChanged": false },
            "logging": {}
        },
        "serverInfo": {
            "name": "coding-tools-mcp",
            "title": "Coding Tools MCP",
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": instructions
    })
}

fn handle_tools_call(state: &SharedState, params: &Value) -> Result<Value, Value> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| serde_json::json!({ "code": -32602, "message": "Missing tool name" }))?;
    let args = tool_arguments(name, params);

    let canonical_name = crate::tools::registry::canonical_tool_name(name);
    let known = crate::tools::registry::exposed_tool_names(&state.tool_profile);
    if !known.iter().any(|n| n == &canonical_name) {
        return Err(serde_json::json!({
            "code": -32602,
            "message": format!("Unknown tool: {name}"),
            "data": { "reason": "unknown_tool" }
        }));
    }

    let structured = call_tool(state.as_ref(), canonical_name, &args);
    let result = wrap_mcp_tool_result(canonical_name, &args, structured);
    state.record_context_block("tool_return", &result);
    Ok(result)
}

fn tool_arguments(name: &str, params: &Value) -> Value {
    let mut args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if name.starts_with("history_session_") {
        if let Some(session_key) = params
            .get("_meta")
            .and_then(|meta| meta.get("openai/session"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if !args.is_object() {
                args = serde_json::json!({});
            }
            args["_host_session_key"] = Value::String(session_key.to_string());
        }
    }
    args
}

pub fn new_state(
    workspace: Workspace,
    auth: AuthConfig,
    policy: crate::tools::policy::PolicySettings,
    tool_profile: String,
    permission_mode: String,
    executable_paths: Vec<std::path::PathBuf>,
    ai_instructions: String,
    agent_context: AgentContextRuntimeConfig,
    history_recording: bool,
    history_context_sessions: Vec<u64>,
    usage: Arc<ServiceUsage>,
) -> SharedState {
    Arc::new(
        ToolContext::from_workspace(
            workspace,
            auth,
            policy,
            tool_profile,
            permission_mode,
        )
        .with_agent_runtime(executable_paths, ai_instructions)
        .with_agent_context(agent_context)
        .with_history_config(history_recording, history_context_sessions)
        .with_usage(usage),
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use serde_json::json;

    use crate::tools::ToolContext;

    use super::{handle_request, initialize_result, tool_arguments};

    fn test_context() -> ToolContext {
        let workspace = tempfile::tempdir().expect("workspace");
        let harness = tempfile::tempdir().expect("harness");
        ToolContext::for_test(workspace.keep(), harness.keep()).expect("context")
    }

    fn test_state() -> Arc<ToolContext> {
        Arc::new(test_context())
    }

    #[test]
    fn compact_initialize_does_not_require_history_bootstrap() {
        let state = Arc::new(
            test_context()
                .with_tool_profile("compact")
                .with_history_config(true, Vec::new())
                .with_agent_runtime(Vec::new(), String::new()),
        );
        let initialized = initialize_result(&state);
        let instructions = initialized["instructions"].as_str().expect("instructions");
        assert!(instructions.contains("history bootstrap is optional"));
        assert!(!instructions.contains("exactly once"));
        assert!(!instructions.contains("required conversation initialization"));
    }

    #[test]
    fn legacy_initialize_keeps_the_history_persistence_workflow() {
        let state = test_state();
        let initialized = initialize_result(&state);
        let instructions = initialized["instructions"].as_str().expect("instructions");
        assert!(instructions.contains("history_session_bootstrap"));
        assert!(instructions.contains("At the start of every new ChatGPT conversation"));
        assert!(instructions.contains("before answering the user's first request"));
        assert!(instructions.contains("required conversation initialization"));
        assert!(instructions.contains("initial_user_input"));
        assert!(instructions.contains("must not create duplicates"));
        assert!(instructions.contains("history_session_checkpoint"));
        assert!(instructions.contains("raw_user_input"));
        assert!(instructions.contains("history_session_search"));
        assert!(instructions.contains("history_session_read"));
        assert!(instructions.contains("follow next_cursor"));
        assert!(instructions.contains("session_key and current_path returned by bootstrap"));
        assert!(instructions.contains("session_key and expected_path"));
        assert!(instructions.contains("After completing each user-requested task"));
        assert!(instructions.contains("before the final response"));
        assert!(instructions.contains("checkpoint returns ok=true"));
        assert!(instructions.contains("not automatic background persistence"));
        assert!(instructions.contains("do not request a separate permission grant"));
        assert!(instructions.contains("confirm=true"));
        assert!(instructions.contains("planning_context"));
        assert!(instructions.contains("controlled exclusively by the desktop app"));
    }

    #[test]
    fn initialize_does_not_claim_tool_catalog_notifications_without_a_stream() {
        let state = test_state();
        let initialized = initialize_result(&state);

        assert_eq!(initialized["capabilities"]["tools"]["listChanged"], false);
    }

    #[test]
    fn initialize_appends_configured_agent_instructions() {
        let state = Arc::new(
            test_context().with_agent_runtime(
                Vec::new(),
                "Global rule\n\nWorkspace rule".into(),
            ),
        );
        let initialized = initialize_result(&state);
        let instructions = initialized["instructions"].as_str().expect("instructions");

        assert!(instructions.contains("Configured agent instructions"));
        assert!(instructions.contains("Global rule"));
        assert!(instructions.contains("Workspace rule"));
    }

    #[test]
    fn workspace_prompt_uses_lazy_history_workflow() {
        let component = include_str!("../../../src/lib/components/ChatGptSessionPrompt.svelte");

        assert!(component.contains("ChatGPT 新会话启动提示词"));
        assert!(component.contains("不需要强制调用 history_session_bootstrap"));
        assert!(component.contains("raw_user_input"));
        assert!(component.contains("history_session_search"));
        assert!(component.contains("history_session_checkpoint"));
        assert!(!component.contains("打开连接器设置"));
    }

    #[test]
    fn chatgpt_session_metadata_is_injected_only_for_history_tools() {
        let params = json!({
            "arguments": {"session_key": "explicit"},
            "_meta": {"openai/session": "chatgpt-conversation"}
        });
        let history = tool_arguments("history_session_bootstrap", &params);
        assert_eq!(history["session_key"], "explicit");
        assert_eq!(history["_host_session_key"], "chatgpt-conversation");

        let existing = tool_arguments("read_file", &params);
        assert_eq!(existing["session_key"], "explicit");
        assert!(existing.get("_host_session_key").is_none());
    }

    #[test]
    fn host_session_key_takes_precedence_over_explicit_session_key() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let harness = tempfile::tempdir().expect("harness tempdir");
        let state = Arc::new(
            ToolContext::for_test(workspace.path().to_path_buf(), harness.path().to_path_buf())
                .expect("tool context"),
        );
        let response = handle_request(
            &state,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "history_session_bootstrap",
                    "arguments": {
                        "session_key": "explicit-session",
                        "initial_user_input": "保存首轮原文"
                    },
                    "_meta": {"openai/session": "chatgpt-session"}
                }
            }),
        );
        let structured = &response["result"]["structuredContent"];
        assert_eq!(structured["ok"], true);
        assert_eq!(structured["session_key_source"], "platform_conversation_id");
        assert_eq!(structured["session_key"], "chatgpt-session");
        assert_eq!(structured["initial_input_captured"], true);
        let content = fs::read_to_string(workspace.path().join("docs/history-session/1.md"))
            .expect("read history file");
        assert!(content.contains("**Session key:** chatgpt-session"));
        assert!(!content.contains("**Session key:** explicit-session"));
    }

    #[test]
    fn legacy_grep_calls_are_mapped_to_the_public_grep_text_tool() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let harness = tempfile::tempdir().expect("harness tempdir");
        fs::write(workspace.path().join("sample.txt"), "catalog needle")
            .expect("write sample file");
        let state = Arc::new(
            ToolContext::for_test(workspace.path().to_path_buf(), harness.path().to_path_buf())
                .expect("tool context"),
        );

        let response = handle_request(
            &state,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "grep",
                    "arguments": {"query": "needle", "path": "."}
                }
            }),
        );

        assert!(response.get("error").is_none());
        assert_eq!(response["result"]["structuredContent"]["ok"], true);
    }
}
