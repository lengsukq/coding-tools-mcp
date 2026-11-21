use std::fs;
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::json;

use super::sanitize::{classify_outcome, file_day, unix_time_ms};
use super::store::ToolAuditStore;
use super::{
    AuditRuntime, ToolAuditCall, ToolAuditFilter, ToolAuditRecord, DAY_MS, DEFAULT_RETENTION_DAYS,
};

#[test]
fn computer_audit_records_client_and_action_but_never_user_input_or_tokens() {
    let secret_text = "SUPER_SECRET_TYPED_PASSWORD";
    let private_token = "cu1_SUPER_SECRET_CAPABILITY";
    let body = json!({
        "params": {
            "name": "computer_action",
            "arguments": {
                "action": "type",
                "text": secret_text,
                "session_token": private_token
            }
        }
    });
    let record = ToolAuditCall::new(
        &json!(42),
        "computer_action",
        Some("opaque-server-session"),
        "oauth",
        Some("chatgpt-oauth-client"),
        1234,
        &body,
    )
    .finish(None, &json!({"result": {"isError": false}}));
    assert_eq!(record.tool_name, "computer_action");
    assert_eq!(record.action_name.as_deref(), Some("type"));
    assert_eq!(record.client_id.as_deref(), Some("chatgpt-oauth-client"));
    let serialized = serde_json::to_string(&record).unwrap();
    assert!(!serialized.contains(secret_text));
    assert!(!serialized.contains(private_token));
}

fn test_store() -> (tempfile::TempDir, ToolAuditStore) {
    let dir = tempfile::tempdir().expect("audit temp dir");
    let store = ToolAuditStore::new(dir.path().to_path_buf());
    (dir, store)
}

fn record(workspace_id: Option<&str>, timestamp_ms: u64, outcome: &str) -> ToolAuditRecord {
    ToolAuditRecord {
        timestamp_ms,
        duration_ms: 12,
        request_id: Some("42".into()),
        session_id: Some("session-a".into()),
        workspace_id: workspace_id.map(str::to_string),
        tool_name: "read_file".into(),
        action_name: None,
        wrapper_tool_name: None,
        auth_method: "shared_bearer".into(),
        client_id: None,
        outcome: outcome.into(),
        error_category: (outcome == "error").then_some("runtime".into()),
        request_bytes: 100,
        response_bytes: 80,
        change_id: None,
        changed_file_count: None,
        execution_host: None,
        source_peer: None,
    }
}

#[test]
fn writes_only_safe_metadata_and_reads_across_store_restarts() {
    let (dir, mut store) = test_store();
    let call = ToolAuditCall::new(
        &json!(7),
        "patch",
        Some("session-a"),
        "shared_bearer",
        None,
        420,
        &json!({"params":{"arguments":{"patch":"SECRET_PAYLOAD"}}}),
    );
    let response = json!({
        "result": {
            "isError": false,
            "structuredContent": {
                "change_id": "1234567890abcdef1234567890abcdef",
                "affected_files": [{"path": "/private/project/file.rs"}],
                "review_url": "https://example.test/review?t=SECRET_TOKEN"
            }
        }
    });
    store
        .append(&call.finish(Some("workspace-a".into()), &response))
        .unwrap();
    let contents = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
        .fold(String::new(), |mut combined, file| {
            combined.push_str(&file);
            combined
        });
    assert!(!contents.contains("SECRET_PAYLOAD"));
    assert!(!contents.contains("SECRET_TOKEN"));
    assert!(!contents.contains("/private/project/file.rs"));
    assert!(contents.contains("1234567890abcdef1234567890abcdef"));
    assert!(contents.contains("\"changedFileCount\":1"));

    let mut restarted = ToolAuditStore::new(dir.path().to_path_buf());
    let page = restarted
        .list(&ToolAuditFilter::default(), DEFAULT_RETENTION_DAYS)
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.records[0].tool_name, "patch");
    assert_eq!(page.records[0].workspace_id.as_deref(), Some("workspace-a"));
}

#[test]
fn filters_and_clears_by_workspace_without_touching_global_events() {
    let (dir, mut store) = test_store();
    let timestamp = unix_time_ms();
    store
        .append(&record(Some("workspace-a"), timestamp, "success"))
        .unwrap();
    store
        .append(&record(Some("workspace-b"), timestamp + 1, "error"))
        .unwrap();
    store
        .append(&record(None, timestamp + 2, "success"))
        .unwrap();

    let filter = ToolAuditFilter {
        workspace_id: Some("workspace-b".into()),
        outcome: Some("error".into()),
        from_ms: Some(timestamp + 1),
        to_ms: Some(timestamp + 1),
        session_id: Some("SESSION-A".into()),
        tool_name: Some("READ_FILE".into()),
        ..ToolAuditFilter::default()
    };
    let page = store.list(&filter, DEFAULT_RETENTION_DAYS).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.error_count, 1);

    store.clear(Some("workspace-a")).unwrap();
    let page = store
        .list(&ToolAuditFilter::default(), DEFAULT_RETENTION_DAYS)
        .unwrap();
    assert_eq!(page.total, 2);
    assert!(page
        .records
        .iter()
        .any(|record| record.workspace_id.is_none()));

    drop(store);
    let mut store = ToolAuditStore::new(dir.path().to_path_buf());
    store.clear(None).unwrap();
    assert_eq!(
        store
            .list(&ToolAuditFilter::default(), DEFAULT_RETENTION_DAYS)
            .unwrap()
            .total,
        0
    );
}

#[test]
fn retention_removes_old_day_partitions_and_keeps_recent_records() {
    let (dir, mut store) = test_store();
    let now = unix_time_ms();
    store
        .append(&record(
            Some("workspace-a"),
            now.saturating_sub(3 * DAY_MS),
            "success",
        ))
        .unwrap();
    store
        .append(&record(Some("workspace-a"), now, "success"))
        .unwrap();

    store.prune_expired(now, 2).unwrap();
    let page = store.list(&ToolAuditFilter::default(), 2).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.records[0].timestamp_ms, now);
    assert!(!fs::read_dir(dir.path()).unwrap().any(|entry| {
        file_day(&entry.unwrap().file_name().to_string_lossy())
            .is_some_and(|day| day < now / DAY_MS - 1)
    }));
}

#[test]
fn concurrent_appends_keep_every_record() {
    let dir = tempfile::tempdir().expect("audit temp dir");
    let store = Arc::new(Mutex::new(ToolAuditStore::new(dir.path().to_path_buf())));
    let mut workers = Vec::new();
    for index in 0..8 {
        let store = store.clone();
        workers.push(thread::spawn(move || {
            for offset in 0..25 {
                let mut record = record(Some("workspace-a"), unix_time_ms(), "success");
                record.request_id = Some(format!("{}-{offset}", index));
                store.lock().unwrap().append(&record).unwrap();
            }
        }));
    }
    for worker in workers {
        worker.join().unwrap();
    }
    let page = store
        .lock()
        .unwrap()
        .list(&ToolAuditFilter::default(), DEFAULT_RETENTION_DAYS)
        .unwrap();
    assert_eq!(page.total, 200);
}

#[test]
fn classifies_policy_tool_and_worker_failures_without_storing_messages() {
    assert_eq!(
        classify_outcome(&json!({"error":{"data":{"code":"POLICY_DENIED"}}})),
        ("error".into(), Some("policy".into()))
    );
    assert_eq!(
        classify_outcome(&json!({"error":{"data":{"stage":"rpc_worker"}}})),
        ("error".into(), Some("runtime".into()))
    );
    assert_eq!(
        classify_outcome(
            &json!({"result":{"isError":true,"structuredContent":{"error":{"category":"permission","message":"private path"}}}})
        ),
        ("error".into(), Some("policy".into()))
    );
}

#[test]
fn records_success_tool_policy_runtime_and_global_outcomes() {
    let (dir, mut store) = test_store();
    let outcomes = [
        (
            json!({"result":{"isError":false,"structuredContent":{"message":"private result"}}}),
            "success",
            None,
            Some("workspace-a"),
        ),
        (
            json!({"result":{"isError":true,"structuredContent":{"error":{"category":"validation","message":"private tool error"}}}}),
            "error",
            Some("validation"),
            Some("workspace-a"),
        ),
        (
            json!({"error":{"code":-32602,"message":"private policy detail","data":{"reason":"POLICY_DENIED"}}}),
            "error",
            Some("policy"),
            None,
        ),
        (
            json!({"error":{"code":-32603,"message":"private runtime detail","data":{"stage":"rpc_worker","reason":"worker_failed"}}}),
            "error",
            Some("runtime"),
            None,
        ),
    ];

    for (index, (response, expected_outcome, expected_category, workspace_id)) in
        outcomes.into_iter().enumerate()
    {
        let call = ToolAuditCall::new(
            &json!(index + 1),
            "read_file",
            Some("session-a"),
            "shared_bearer",
            None,
            200,
            &json!({"params":{"arguments":{"path":"private request"}}}),
        );
        let record = call.finish(workspace_id.map(str::to_string), &response);
        assert_eq!(record.outcome, expected_outcome);
        assert_eq!(record.error_category.as_deref(), expected_category);
        store.append(&record).unwrap();
    }

    let contents = fs::read_to_string(
        fs::read_dir(dir.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path(),
    )
    .unwrap();
    assert!(!contents.contains("private request"));
    assert!(!contents.contains("private result"));
    assert!(!contents.contains("private policy detail"));
    assert!(!contents.contains("private runtime detail"));

    let page = store
        .list(&ToolAuditFilter::default(), DEFAULT_RETENTION_DAYS)
        .unwrap();
    assert_eq!(page.total, 4);
    assert_eq!(page.error_count, 3);
    assert!(page
        .records
        .iter()
        .any(|record| record.workspace_id.is_none()));
}

#[test]
fn finish_counts_response_bytes_identically_to_compact_json_serialization() {
    let response = json!({
        "result": {
            "isError": false,
            "structuredContent": {
                "message": "中文与ASCII混合",
                "items": [1, 2, 3, {"nested": true}]
            }
        }
    });
    let expected = serde_json::to_vec(&response).unwrap().len() as u64;
    let call = ToolAuditCall::new(
        &json!(99),
        "read_file",
        Some("sess-bytes"),
        "oauth",
        None,
        64,
        &json!({}),
    );
    let finished = call.finish(Some("ws-1".into()), &response);
    assert_eq!(finished.response_bytes, expected);
}

#[test]
fn background_runtime_flushes_queued_records_before_list_clear_and_retention() {
    let dir = tempfile::tempdir().expect("audit runtime temp dir");
    let runtime = Arc::new(AuditRuntime::new(dir.path().to_path_buf()));
    let now = unix_time_ms();

    let mut workers = Vec::new();
    for worker_index in 0..4 {
        let runtime = runtime.clone();
        workers.push(thread::spawn(move || {
            for offset in 0..25 {
                let mut item = record(Some("workspace-bg"), now + offset, "success");
                item.request_id = Some(format!("bg-{worker_index}-{offset}"));
                runtime.record(item, DEFAULT_RETENTION_DAYS);
            }
        }));
    }
    for worker in workers {
        worker.join().unwrap();
    }

    // Immediate list must act as a synchronous flush barrier and observe all 100 records.
    let page = runtime
        .list(&ToolAuditFilter::default(), DEFAULT_RETENTION_DAYS)
        .unwrap();
    assert_eq!(page.total, 100);

    // Enqueue an expired record and immediately narrow retention; flush barrier ensures
    // the expired record is written and then pruned deterministically.
    runtime.record(
        record(
            Some("workspace-bg"),
            now.saturating_sub(5 * DAY_MS),
            "success",
        ),
        DEFAULT_RETENTION_DAYS,
    );
    runtime.set_retention_days(2);
    let after_prune = runtime.list(&ToolAuditFilter::default(), 2).unwrap();
    assert_eq!(after_prune.total, 100);

    // Enqueue fresh records and immediately clear; flush barrier ensures no queued writes
    // leak onto disk after clear returns.
    for offset in 0..20 {
        runtime.record(
            record(Some("workspace-bg"), now + 1000 + offset, "success"),
            DEFAULT_RETENTION_DAYS,
        );
    }
    runtime.clear(None).unwrap();
    let after_clear = runtime
        .list(&ToolAuditFilter::default(), DEFAULT_RETENTION_DAYS)
        .unwrap();
    assert_eq!(after_clear.total, 0);
}
