mod common;

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Barrier};

use coding_tools_mcp_desktop_lib::tools::{list_tools_for_profile, ToolContext};
use coding_tools_mcp_desktop_lib::tools::history;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use common::{assert_err, assert_ok, invoke};

fn invoke_ok(ctx: &ToolContext, name: &str, args: Value) -> Value {
    let result = invoke(ctx, name, args);
    assert_ok(&result);
    result
}

fn test_context() -> (tempfile::TempDir, tempfile::TempDir, ToolContext) {
    let workspace = tempfile::tempdir().expect("workspace tempdir");
    let harness = tempfile::tempdir().expect("harness tempdir");
    let ctx = ToolContext::for_test(workspace.path().to_path_buf(), harness.path().to_path_buf())
        .expect("tool context");
    (workspace, harness, ctx)
}

#[test]
fn compact_bootstrap_returns_index_metadata_without_workflow_prompt() {
    let (_workspace, _harness, ctx) = test_context();
    let ctx = ctx.with_tool_profile("compact");
    let boot = invoke_ok(
        &ctx,
        "history_session_bootstrap",
        json!({"session_key": "compact-session", "initial_user_input": "建立精简历史"}),
    );

    assert_eq!(boot["context_mode"], "compact");
    assert_eq!(boot["index_only"], true);
    assert!(boot.get("state").is_none());
    assert!(boot.get("assistant_instructions").is_none());
    assert!(boot.get("required_next_actions").is_none());
}

#[test]
fn compact_checkpoint_lazily_creates_session_without_bootstrap() {
    let (workspace, _harness, ctx) = test_context();
    let ctx = ctx.with_tool_profile("compact");
    let checkpoint = invoke_ok(
        &ctx,
        "history_session_checkpoint",
        json!({
            "turn_id": "lazy-1",
            "raw_user_input": "直接保存当前任务，不先 bootstrap",
            "files_changed": ["src/history.rs"]
        }),
    );

    assert_eq!(checkpoint["recorded"], true);
    assert_eq!(checkpoint["turn_id"], "lazy-1");
    assert!(checkpoint.get("content_hash").is_none());
    assert!(workspace.path().join("docs/history-session/1.md").is_file());
}

#[test]
fn disabled_recording_does_not_write_a_checkpoint() {
    let (workspace, _harness, ctx) = test_context();
    let ctx = ctx.with_history_config(false, Vec::new());
    let checkpoint = invoke_ok(
        &ctx,
        "history_session_checkpoint",
        json!({"raw_user_input": "不要记录这次会话"}),
    );

    assert_eq!(checkpoint["recorded"], false);
    assert!(!workspace.path().join("docs/history-session").exists());
}

#[test]
fn selected_history_context_is_bounded_and_does_not_include_archive_body() {
    let (workspace, _harness, ctx) = test_context();
    let ctx = ctx.with_tool_profile("compact");
    invoke_ok(
        &ctx,
        "history_session_bootstrap",
        json!({"session_key": "selected-session", "initial_user_input": "历史选择项"}),
    );
    invoke_ok(
        &ctx,
        "history_session_checkpoint",
        json!({
            "session_key": "selected-session",
            "turn_id": "selected-1",
            "raw_user_input": "这里是精选片段",
            "files_changed": ["src/important.rs"]
        }),
    );
    let ctx = ctx.with_history_config(true, vec![1]);
    let snapshot = history::context_snapshot(&ctx)
        .expect("snapshot")
        .expect("selected snapshot");

    assert_eq!(snapshot["selected_sessions"][0], 1);
    assert_eq!(snapshot["injection_mode"], "index_and_bounded_snippets");
    assert!(snapshot["context_revision"].as_str().unwrap().starts_with("sha256:"));
    assert!(snapshot["latest_checkpoints"].is_array());
    assert!(snapshot["key_files"].to_string().contains("src/important.rs"));
    assert!(snapshot["selected_snippets"].is_array());
    assert!(snapshot.to_string().contains("这里是精选片段"));
    assert!(!snapshot.to_string().contains(&"历史选择项".repeat(20)));
    assert!(workspace.path().join("docs/history-session/1.md").is_file());
}

#[test]
fn bootstrap_prefers_host_session_and_preserves_initial_input() {
    let (workspace, _harness, ctx) = test_context();
    let first_request = "修复 bootstrap，首轮输入必须完整保存";
    let boot = invoke(
        &ctx,
        "history_session_bootstrap",
        json!({
            "session_key": "explicit-but-lower-priority",
            "_host_session_key": "chatgpt-session",
            "initial_user_input": first_request
        }),
    );
    let boot = assert_ok(&boot);
    assert_eq!(boot["session_key"], "chatgpt-session");
    assert_eq!(boot["session_key_source"], "platform_conversation_id");
    assert_eq!(boot["current_path"], "docs/history-session/1.md");
    assert_eq!(boot["initial_input_captured"], true);
    assert!(boot.get("all_history_summary").is_none());
    assert!(boot.get("latest_handoff").is_none());
    assert!(boot.get("session_summaries").is_none());
    assert_eq!(
        boot["history_read_mode"],
        "bounded_state_with_on_demand_search_and_read"
    );
    assert!(boot["assistant_instructions"]
        .as_str()
        .unwrap_or("")
        .contains("raw_user_input"));

    let archive = fs::read_to_string(workspace.path().join("docs/history-session/1.md"))
        .expect("read archive");
    assert!(archive.contains(first_request));
    assert!(!archive.contains("继承的历史摘要"));
    assert!(workspace
        .path()
        .join("docs/history-session/memory/state.json")
        .is_file());
    assert!(workspace
        .path()
        .join("docs/history-session/memory/manifest.json")
        .is_file());
}

#[test]
fn bootstrap_reports_missing_initial_input_without_claiming_capture() {
    let (_workspace, _harness, ctx) = test_context();
    let boot = invoke_ok(
        &ctx,
        "history_session_bootstrap",
        json!({"session_key": "missing-input"}),
    );
    assert_eq!(boot["initial_input_captured"], false);
    assert!(boot["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .any(|warning| warning
            .as_str()
            .unwrap_or("")
            .contains("initial_user_input")));
}

#[test]
fn bootstrap_retry_keeps_old_initial_input_and_appends_a_revision() {
    let (workspace, _harness, ctx) = test_context();
    assert_ok(&invoke(
        &ctx,
        "history_session_bootstrap",
        json!({"session_key": "initial-revision", "initial_user_input": "原始首轮请求"}),
    ));
    let retry = invoke_ok(
        &ctx,
        "history_session_bootstrap",
        json!({"session_key": "initial-revision", "initial_user_input": "补充后的首轮请求"}),
    );
    assert_eq!(retry["current_number"], 1);
    let archive = fs::read_to_string(workspace.path().join("docs/history-session/1.md"))
        .expect("read archive");
    assert!(archive.contains("原始首轮请求"));
    assert!(archive.contains("补充后的首轮请求"));
    assert!(archive.contains("initial-input revision-2"));
}

#[test]
fn checkpoint_preserves_raw_input_and_superseding_revision_evidence() {
    let (workspace, _harness, ctx) = test_context();
    let boot = invoke_ok(
        &ctx,
        "history_session_bootstrap",
        json!({"session_key": "checkpoint-chat", "initial_user_input": "第一个需求"}),
    );
    let args = json!({
        "session_key": boot["session_key"],
        "expected_path": boot["current_path"],
        "turn_id": "turn-0001",
        "raw_user_input": "把历史按需读取，password=hunter2",
        "user_intent": "实现按需读取",
        "decisions": ["使用 Bearer super-secret-token"],
        "files_changed": ["src/history.rs"],
        "next_actions": ["运行测试"]
    });
    let first = invoke_ok(&ctx, "history_session_checkpoint", args.clone());
    assert_eq!(first["user_input_captured"], true);
    assert_eq!(first["revision"], 1);
    assert!(!first["warnings"].as_array().expect("warnings").is_empty());

    let duplicate = invoke_ok(&ctx, "history_session_checkpoint", args.clone());
    assert_eq!(duplicate["duplicate_ignored"], true);

    let mut changed = args;
    changed["raw_user_input"] = json!("把历史按需读取，并保留同 turn 修订证据");
    changed["next_actions"] = json!(["运行完整回归"]);
    let revision = invoke_ok(&ctx, "history_session_checkpoint", changed);
    assert_eq!(revision["duplicate_ignored"], false);
    assert_eq!(revision["revision"], 2);
    assert_eq!(revision["supersedes"], "turn-0001 revision-1");

    let archive = fs::read_to_string(workspace.path().join("docs/history-session/1.md"))
        .expect("read checkpoint archive");
    assert!(archive.contains("[REDACTED]"));
    assert!(!archive.contains("hunter2"));
    assert!(!archive.contains("super-secret-token"));
    assert!(archive.contains("turn-0001 revision-1"));
    assert!(archive.contains("turn-0001 revision-2"));
    assert!(archive.contains("运行完整回归"));
}

#[test]
fn checkpoint_reports_missing_raw_user_input() {
    let (_workspace, _harness, ctx) = test_context();
    let boot = invoke_ok(
        &ctx,
        "history_session_bootstrap",
        json!({"session_key": "missing-turn-input", "initial_user_input": "开始"}),
    );
    let checkpoint = invoke_ok(
        &ctx,
        "history_session_checkpoint",
        json!({
            "session_key": boot["session_key"],
            "expected_path": boot["current_path"],
            "turn_id": "missing-raw"
        }),
    );
    assert_eq!(checkpoint["user_input_captured"], false);
    assert!(checkpoint["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .any(|warning| warning.as_str().unwrap_or("").contains("raw_user_input")));
}

#[test]
fn search_and_read_return_precise_lossless_archive_pages() {
    let (workspace, _harness, ctx) = test_context();
    prepare_history(workspace.path());
    let search = invoke_ok(
        &ctx,
        "history_session_search",
        json!({"query": "精确记忆", "limit": 1}),
    );
    assert_eq!(search["total_matches"], 1);
    let hit = &search["results"][0];
    assert_eq!(hit["number"], 2);
    assert_eq!(hit["path"], "docs/history-session/2.md");
    assert!(hit["snippet"].as_str().unwrap_or("").contains("精确记忆"));

    let first_page = invoke_ok(
        &ctx,
        "history_session_read",
        json!({"number": 2, "max_bytes": 17}),
    );
    let cursor = first_page["next_cursor"].as_u64().expect("next cursor");
    let second_page = invoke_ok(
        &ctx,
        "history_session_read",
        json!({
            "path": hit["path"],
            "cursor": cursor,
            "max_bytes": 65536,
            "expected_hash": first_page["content_hash"]
        }),
    );
    let reconstructed = format!(
        "{}{}",
        first_page["content"].as_str().unwrap_or(""),
        second_page["content"].as_str().unwrap_or("")
    );
    let original = fs::read_to_string(workspace.path().join("docs/history-session/2.md"))
        .expect("read original");
    assert_eq!(reconstructed, original);
    assert_eq!(second_page["next_cursor"], Value::Null);
    assert_eq!(
        assert_err(&invoke(
            &ctx,
            "history_session_read",
            json!({"path": "docs/history-session/README.md"}),
        ))["error"]["code"],
        "HISTORY_READ_NOT_FOUND"
    );
}

#[test]
fn read_without_max_bytes_uses_bounded_lossless_pages() {
    let (workspace, _harness, ctx) = test_context();
    let dir = workspace.path().join("docs/history-session");
    fs::create_dir_all(&dir).expect("create history");
    fs::write(
        dir.join("1.md"),
        history_file(
            1,
            "large-history",
            &format!("精确上下文 {}", "x".repeat(80_000)),
        ),
    )
    .expect("write archive");

    let original = fs::read_to_string(dir.join("1.md")).expect("read original archive");
    let mut reconstructed = String::new();
    let mut cursor = 0;
    let mut expected_hash = None::<String>;
    let mut page_count = 0;

    loop {
        let mut args = json!({"number": 1, "cursor": cursor});
        if let Some(hash) = &expected_hash {
            args["expected_hash"] = json!(hash);
        }
        let page = invoke_ok(&ctx, "history_session_read", args);
        let content = page["content"].as_str().expect("page content");
        assert!(content.len() <= 32 * 1024, "default page exceeded 32 KiB");
        reconstructed.push_str(content);
        page_count += 1;
        expected_hash = Some(
            page["content_hash"]
                .as_str()
                .expect("content hash")
                .to_string(),
        );
        match page["next_cursor"].as_u64() {
            Some(next) => cursor = next,
            None => break,
        }
    }

    assert!(page_count > 1);
    assert_eq!(reconstructed, original);
    assert_eq!(
        assert_err(&invoke(
            &ctx,
            "history_session_read",
            json!({"number": 1, "max_bytes": 65537}),
        ))["error"]["code"],
        "HISTORY_CURSOR_INVALID"
    );
}

#[test]
fn search_snippet_handles_unicode_case_expansion_without_invalid_utf8_slicing() {
    let (workspace, _harness, ctx) = test_context();
    let dir = workspace.path().join("docs/history-session");
    fs::create_dir_all(&dir).expect("create history");
    fs::write(
        dir.join("1.md"),
        history_file(
            1,
            "unicode-search",
            &format!("İ{} 精确记忆", "x".repeat(100)),
        ),
    )
    .expect("write unicode archive");

    let search = invoke_ok(&ctx, "history_session_search", json!({"query": "精确记忆"}));
    let snippet = search["results"][0]["snippet"].as_str().expect("snippet");
    assert!(snippet.contains("精确记忆"));
}

#[test]
fn rebuilds_derived_files_without_changing_existing_archives() {
    let (workspace, _harness, ctx) = test_context();
    prepare_history(workspace.path());
    let before = archive_hashes(workspace.path());
    let boot = invoke_ok(
        &ctx,
        "history_session_bootstrap",
        json!({"session_key": "new-chat", "initial_user_input": "创建新会话"}),
    );
    assert_eq!(boot["current_number"], 3);
    assert_existing_archives_unchanged(workspace.path(), &before);

    let memory = workspace.path().join("docs/history-session/memory");
    fs::remove_file(memory.join("state.json")).expect("remove state");
    fs::write(memory.join("manifest.json"), "{broken-json").expect("break manifest");
    let validate = invoke_ok(&ctx, "history_session_validate", json!({"repair": true}));
    assert_eq!(validate["repaired"], true);
    assert_existing_archives_unchanged(workspace.path(), &before);
    assert!(memory.join("state.json").is_file());
    assert!(memory.join("manifest.json").is_file());
}

#[test]
fn bootstrap_response_stays_bounded_for_many_large_archives() {
    let (workspace, _harness, ctx) = test_context();
    let dir = workspace.path().join("docs/history-session");
    fs::create_dir_all(&dir).expect("create history");
    let marker = "x".repeat(80_000);
    for number in 1..=40 {
        fs::write(
            dir.join(format!("{number}.md")),
            history_file(
                number,
                &format!("old-{number}"),
                &format!("精确记忆 {number} {marker}"),
            ),
        )
        .expect("write archive");
    }
    let boot = invoke_ok(
        &ctx,
        "history_session_bootstrap",
        json!({"session_key": "large-new", "initial_user_input": "新会话"}),
    );
    assert!(serde_json::to_vec(&boot).expect("serialize boot").len() < 64 * 1024);
    assert_eq!(boot["history_count"], 41);
    assert!(boot.get("latest_handoff").is_none());
    assert!(boot.get("all_history_summary").is_none());
}

#[test]
fn real_history_fixture_preserves_archives_when_opted_in() {
    let Ok(source) = env::var("HISTORY_SESSION_REAL_FIXTURE") else {
        return;
    };
    let source = Path::new(&source);
    assert!(source.is_dir(), "real fixture directory is missing");

    let (workspace, _harness, ctx) = test_context();
    let target = workspace.path().join("docs/history-session");
    copy_directory(source, &target);
    let before = archive_hashes(workspace.path());
    assert_eq!(before.len(), 40, "expected the 40-archive real fixture");

    let boot = invoke_ok(
        &ctx,
        "history_session_bootstrap",
        json!({
            "session_key": "real-fixture-new-session",
            "initial_user_input": "验证大规模历史档案不会膨胀 bootstrap 响应"
        }),
    );
    assert_eq!(boot["history_count"], 41);
    assert!(
        serde_json::to_vec(&boot)
            .expect("serialize bootstrap")
            .len()
            < 64 * 1024
    );
    assert_existing_archives_unchanged(workspace.path(), &before);

    let memory = target.join("memory");
    fs::remove_file(memory.join("state.json")).expect("remove derived state");
    fs::write(memory.join("manifest.json"), "{broken-json").expect("break manifest");
    let validate = invoke_ok(&ctx, "history_session_validate", json!({"repair": true}));
    assert_eq!(validate["repaired"], true);
    assert_existing_archives_unchanged(workspace.path(), &before);

    let search = invoke_ok(
        &ctx,
        "history_session_search",
        json!({"query": "all_history_summary", "limit": 50}),
    );
    assert!(search["results"]
        .as_array()
        .expect("search results")
        .iter()
        .any(|hit| hit["number"] == 40));

    let original = fs::read_to_string(target.join("40.md")).expect("read copied archive");
    let mut reconstructed = String::new();
    let mut cursor = 0;
    let mut expected_hash = None::<String>;
    loop {
        let mut args = json!({"number": 40, "cursor": cursor, "max_bytes": 8192});
        if let Some(hash) = &expected_hash {
            args["expected_hash"] = json!(hash);
        }
        let page = invoke_ok(&ctx, "history_session_read", args);
        expected_hash = Some(
            page["content_hash"]
                .as_str()
                .expect("content hash")
                .to_string(),
        );
        reconstructed.push_str(page["content"].as_str().expect("page content"));
        match page["next_cursor"].as_u64() {
            Some(next) => cursor = next,
            None => break,
        }
    }
    assert_eq!(reconstructed, original);
}

#[test]
fn history_tools_are_exposed_with_public_schemas() {
    let tools = list_tools_for_profile("core");
    for name in [
        "history_session_bootstrap",
        "history_session_checkpoint",
        "history_session_validate",
        "history_session_search",
        "history_session_read",
    ] {
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("missing tool: {name}"));
        assert_eq!(tool["inputSchema"]["type"], "object");
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        assert!(tool["inputSchema"]["properties"]
            .get("_host_session_key")
            .is_none());
    }
    let bootstrap = tools
        .iter()
        .find(|tool| tool["name"] == "history_session_bootstrap")
        .expect("bootstrap descriptor");
    assert!(bootstrap["description"]
        .as_str()
        .unwrap_or("")
        .contains("initial_user_input"));
    let checkpoint = tools
        .iter()
        .find(|tool| tool["name"] == "history_session_checkpoint")
        .expect("checkpoint schema");
    assert!(checkpoint["inputSchema"].get("required").is_none());
    assert!(checkpoint["inputSchema"]["properties"]
        .get("raw_user_input")
        .is_some());
    let read = tools
        .iter()
        .find(|tool| tool["name"] == "history_session_read")
        .expect("read schema");
    assert_eq!(
        read["inputSchema"]["properties"]["max_bytes"]["default"],
        16384
    );
    assert_eq!(
        read["inputSchema"]["properties"]["max_bytes"]["maximum"],
        65536
    );
}

#[test]
fn bootstrap_uses_a_workspace_fallback_without_a_host_session_id() {
    let (_workspace, _harness, ctx) = test_context();
    let result = invoke_ok(&ctx, "history_session_bootstrap", json!({}));
    assert_eq!(result["session_key_source"], "workspace_fallback");
    assert!(result["session_key"]
        .as_str()
        .unwrap_or("")
        .starts_with("workspace-session-"));
}

#[test]
fn checkpoint_rejects_a_path_from_another_session() {
    let (_workspace, _harness, ctx) = test_context();
    let first = invoke_ok(
        &ctx,
        "history_session_bootstrap",
        json!({"session_key": "session-a", "initial_user_input": "a"}),
    );
    let second = invoke_ok(
        &ctx,
        "history_session_bootstrap",
        json!({"session_key": "session-b", "initial_user_input": "b"}),
    );
    let result = invoke(
        &ctx,
        "history_session_checkpoint",
        json!({
            "session_key": first["session_key"],
            "expected_path": second["current_path"],
            "turn_id": "wrong-target"
        }),
    );
    assert_eq!(
        assert_err(&result)["error"]["code"],
        "SESSION_TARGET_MISMATCH"
    );
}

#[test]
fn workspace_root_and_history_paths_cannot_escape_workspace() {
    let (workspace, _harness, ctx) = test_context();
    let relative = invoke_ok(
        &ctx,
        "history_session_bootstrap",
        json!({"workspace_root": ".", "session_key": "relative-root", "initial_user_input": "x"}),
    );
    assert_eq!(relative["current_number"], 1);
    let outside = invoke(
        &ctx,
        "history_session_validate",
        json!({
            "workspace_root": workspace.path().parent().unwrap().to_string_lossy(),
            "repair": false
        }),
    );
    assert_eq!(
        assert_err(&outside)["error"]["code"],
        "PATH_OUTSIDE_WORKSPACE"
    );
    let traversal = invoke(
        &ctx,
        "history_session_read",
        json!({"history_dir": "../outside", "number": 1}),
    );
    assert_eq!(
        assert_err(&traversal)["error"]["code"],
        "PATH_OUTSIDE_WORKSPACE"
    );
}

#[test]
fn concurrent_bootstrap_allocates_distinct_numbers() {
    let workspace = tempfile::tempdir().expect("workspace tempdir");
    let barrier = Arc::new(Barrier::new(2));
    let root = workspace.path().to_path_buf();
    let handles = ["parallel-a", "parallel-b"].map(|session_key| {
        let root = root.clone();
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            let harness = tempfile::tempdir().expect("harness tempdir");
            let ctx = ToolContext::for_test(root, harness.path().to_path_buf())
                .expect("parallel context");
            barrier.wait();
            let result = invoke(
                &ctx,
                "history_session_bootstrap",
                json!({"session_key": session_key, "initial_user_input": session_key}),
            );
            assert_ok(&result)["current_number"]
                .as_u64()
                .expect("current number")
        })
    });
    let mut numbers = handles
        .into_iter()
        .map(|handle| handle.join().expect("bootstrap thread"))
        .collect::<Vec<_>>();
    numbers.sort_unstable();
    assert_eq!(numbers, vec![1, 2]);
}

fn history_file(number: u64, session_key: &str, marker: &str) -> String {
    format!(
        "# 会话 {number}：{marker}\n\n\
**Session key:** {session_key}\n\
**Created:** 2026-07-17T08:00:00+08:00\n\
**Updated:** 2026-07-17T09:00:00+08:00\n\
**Status:** completed\n\n\
## 用户核心目标\n\n{marker}\n\n\
## 已确认事实\n\n事实-{marker}\n\n\
## 本轮检查点\n"
    )
}

fn prepare_history(root: &std::path::Path) {
    let dir = root.join("docs/history-session");
    fs::create_dir_all(&dir).expect("create history dir");
    fs::write(dir.join("README.md"), "# 历史归档说明\n").expect("write readme");
    fs::write(
        dir.join("1.md"),
        history_file(1, "old-session-1", "第一阶段"),
    )
    .expect("write 1.md");
    fs::write(
        dir.join("2.md"),
        history_file(2, "old-session-2", "第二阶段 精确记忆"),
    )
    .expect("write 2.md");
}

fn archive_hashes(root: &std::path::Path) -> BTreeMap<String, String> {
    let dir = root.join("docs/history-session");
    fs::read_dir(dir)
        .expect("list archives")
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let numeric_markdown = name
                .strip_suffix(".md")
                .and_then(|stem| stem.parse::<u64>().ok())
                .is_some();
            numeric_markdown.then(|| {
                let content = fs::read(entry.path()).expect("read archive");
                (name, format!("{:x}", Sha256::digest(content)))
            })
        })
        .collect()
}

fn assert_existing_archives_unchanged(root: &std::path::Path, before: &BTreeMap<String, String>) {
    let after = archive_hashes(root);
    for (path, hash) in before {
        assert_eq!(after.get(path), Some(hash), "archive changed: {path}");
    }
}

fn copy_directory(source: &Path, target: &Path) {
    fs::create_dir_all(target).expect("create copied history directory");
    for entry in fs::read_dir(source).expect("read fixture directory") {
        let entry = entry.expect("fixture entry");
        let destination = target.join(entry.file_name());
        if entry.file_type().expect("fixture entry type").is_dir() {
            copy_directory(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).expect("copy fixture file");
        }
    }
}
