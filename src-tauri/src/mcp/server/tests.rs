use std::fs;
use std::sync::Arc;

use serde_json::json;

use crate::tools::ToolContext;

use super::{
    audit_workspace_hint, handle_request, initialize_result, negotiate_protocol_version,
    tool_arguments, DEFAULT_PROTOCOL_VERSION, LATEST_PROTOCOL_VERSION, SUPPORTED_PROTOCOL_VERSIONS,
};

fn test_context() -> ToolContext {
    let workspace = tempfile::tempdir().expect("workspace");
    let harness = tempfile::tempdir().expect("harness");
    ToolContext::for_test(workspace.keep(), harness.keep()).expect("context")
}

fn test_state() -> Arc<ToolContext> {
    Arc::new(test_context())
}

#[test]
fn audit_workspace_attribution_follows_explicit_session_and_global_routes() {
    let mut state = crate::mcp::gateway::GatewayState::default();
    let workspace_a = crate::workspace::WorkspaceProfile::new(
        "/tmp/audit-workspace-a".into(),
        Some("Audit A".into()),
    );
    let workspace_b = crate::workspace::WorkspaceProfile::new(
        "/tmp/audit-workspace-b".into(),
        Some("Audit B".into()),
    );
    let id_a = workspace_a.id.clone();
    let id_b = workspace_b.id.clone();
    state.registry =
        crate::mcp::gateway_state::WorkspaceRegistry::from_profiles(vec![workspace_a, workspace_b]);

    let explicit = json!({
        "method":"tools/call",
        "params":{"name":"workspace_invoke","arguments":{"workspace_id":id_b,"tool":"read_file"}}
    });
    assert_eq!(
        audit_workspace_hint(&state, None, &explicit),
        Some(id_b.clone())
    );

    state.sessions.ensure_host_session("audit-session");
    assert!(state.sessions.select("audit-session", id_a.clone()));
    let selected = json!({
        "method":"tools/call",
        "params":{"name":"read_file","arguments":{"path":"/tmp/file"}}
    });
    assert_eq!(
        audit_workspace_hint(&state, Some("audit-session"), &selected),
        Some(id_a)
    );

    let global = json!({
        "method":"tools/call",
        "params":{"name":"workspace_list","arguments":{}}
    });
    assert_eq!(
        audit_workspace_hint(&state, Some("audit-session"), &global),
        None
    );
}

#[test]
fn compact_initialize_does_not_require_history_bootstrap() {
    let state = Arc::new(
        test_context()
            .with_tool_profile("compact")
            .with_history_config(true, Vec::new())
            .with_agent_runtime(Vec::new(), String::new()),
    );
    let initialized = initialize_result(&state, DEFAULT_PROTOCOL_VERSION);
    let instructions = initialized["instructions"].as_str().expect("instructions");
    assert!(instructions.contains("history bootstrap is optional"));
    assert!(instructions.contains("Workspace Root as the stable path base"));
    assert!(instructions.contains("exec_command workdir"));
    assert!(!instructions.contains("task_manage"));
    assert!(!instructions.contains("exactly once"));
    assert!(!instructions.contains("required conversation initialization"));
}

#[test]
fn legacy_initialize_keeps_the_history_persistence_workflow() {
    let state = test_state();
    let initialized = initialize_result(&state, DEFAULT_PROTOCOL_VERSION);
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
    let initialized = initialize_result(&state, DEFAULT_PROTOCOL_VERSION);

    assert_eq!(initialized["capabilities"]["tools"]["listChanged"], false);
}

#[test]
fn initialize_appends_configured_agent_instructions() {
    let state = Arc::new(
        test_context().with_agent_runtime(Vec::new(), "Global rule\n\nWorkspace rule".into()),
    );
    let initialized = initialize_result(&state, DEFAULT_PROTOCOL_VERSION);
    let instructions = initialized["instructions"].as_str().expect("instructions");

    assert!(instructions.contains("Configured agent instructions"));
    assert!(instructions.contains("Global rule"));
    assert!(instructions.contains("Workspace rule"));
}

#[test]
fn protocol_version_negotiation_matrix() {
    assert!(!SUPPORTED_PROTOCOL_VERSIONS.contains(&"2026-07-28"));
    assert_eq!(LATEST_PROTOCOL_VERSION, "2025-11-25");
    assert_eq!(negotiate_protocol_version(Some("2024-11-05")), "2024-11-05");
    assert_eq!(negotiate_protocol_version(Some("2025-06-18")), "2025-06-18");
    assert_eq!(negotiate_protocol_version(Some("2025-11-25")), "2025-11-25");
    // 未知新版本：回退到服务端最新版本，由客户端决定是否继续。
    assert_eq!(
        negotiate_protocol_version(Some("2099-01-01")),
        LATEST_PROTOCOL_VERSION
    );
    // 未知旧版本：同样回退到服务端最新版本。
    assert_eq!(
        negotiate_protocol_version(Some("2000-01-01")),
        LATEST_PROTOCOL_VERSION
    );
    // 缺省或空值：沿用历史默认版本。
    assert_eq!(negotiate_protocol_version(None), DEFAULT_PROTOCOL_VERSION);
    assert_eq!(
        negotiate_protocol_version(Some("  ")),
        DEFAULT_PROTOCOL_VERSION
    );
}

#[test]
fn initialize_echoes_the_negotiated_protocol_version() {
    let state = test_state();
    let response = handle_request(
        &state,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"protocolVersion": "2025-11-25"}
        }),
    );
    assert_eq!(response["result"]["protocolVersion"], "2025-11-25");

    let without_version = handle_request(
        &state,
        &json!({"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {}}),
    );
    assert_eq!(
        without_version["result"]["protocolVersion"],
        DEFAULT_PROTOCOL_VERSION
    );
}

#[test]
fn workspace_prompt_uses_lazy_history_workflow() {
    let component = include_str!("../../../../src/components/workspace/ChatGptSessionPrompt.vue");

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
    let compact_history = tool_arguments("history_manage", &params);
    assert_eq!(compact_history["_host_session_key"], "chatgpt-conversation");

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
    fs::write(workspace.path().join("sample.txt"), "catalog needle").expect("write sample file");
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
