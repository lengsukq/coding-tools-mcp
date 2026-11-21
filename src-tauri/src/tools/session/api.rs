use super::store::command_id_arg;
use super::*;

const DEFAULT_READ_OUTPUT_LIMIT: usize = 4096;
const MAX_READ_OUTPUT_LIMIT: usize = 1_048_576;
const DEFAULT_SEARCH_MAX_MATCHES: usize = 50;
const MAX_SEARCH_MAX_MATCHES: usize = 200;
const DEFAULT_YIELD_TIME_MS: u64 = 1000;
const MAX_YIELD_TIME_MS: u64 = 30_000;
const DEFAULT_KILL_WAIT_MS: u64 = 5000;
const MAX_KILL_WAIT_MS: u64 = 30_000;

pub fn read_output(store: &SessionStore, args: &Value) -> Result<Value, WorkspaceError> {
    let output_ref = args
        .get("output_ref")
        .and_then(Value::as_str)
        .ok_or_else(|| WorkspaceError::invalid_argument("output_ref is required"))?;
    let parts: Vec<&str> = output_ref.split(':').collect();
    if parts.len() != 3 || (parts[0] != "command" && parts[0] != "session") {
        return Err(WorkspaceError::invalid_argument(
            "output_ref must look like command:<id>:stdout/stderr or legacy session:<id>:stdout/stderr/full",
        ));
    }
    let session_id = parts[1];
    let ref_stream = parts[2];
    if ref_stream != "stdout" && ref_stream != "stderr" && ref_stream != "full" {
        return Err(WorkspaceError::invalid_argument(
            "output_ref stream must be stdout, stderr, or full",
        ));
    }
    let requested_stream = args.get("stream").and_then(Value::as_str).unwrap_or("");
    let stream = if ref_stream == "stdout" || ref_stream == "stderr" {
        ref_stream
    } else if requested_stream == "stdout" || requested_stream == "stderr" {
        requested_stream
    } else {
        "stdout"
    };

    let requested_offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_READ_OUTPUT_LIMIT as u64)
        .clamp(1, MAX_READ_OUTPUT_LIMIT as u64) as usize;

    let active_session = store.output_read_session(session_id)?;

    if let Some(query) = args
        .get("query")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        let regex = args.get("regex").and_then(Value::as_bool).unwrap_or(false);
        let case_sensitive = args
            .get("case_sensitive")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let max_matches = args
            .get("max_matches")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_SEARCH_MAX_MATCHES as u64)
            .clamp(1, MAX_SEARCH_MAX_MATCHES as u64) as usize;
        match store.search_full_output(
            session_id,
            stream,
            query,
            regex,
            case_sensitive,
            max_matches,
        ) {
            Ok(Some(mut search)) => {
                if let Some(object) = search.as_object_mut() {
                    object.insert("command_id".into(), json!(session_id));
                    object.insert("session_id".into(), json!(session_id));
                    object.insert("output_ref".into(), json!(output_ref));
                    object.insert("stream".into(), json!(stream));
                    object.insert("mode".into(), json!("search"));
                    object.insert("source".into(), json!("full"));
                    object.insert("raw_available".into(), Value::Bool(true));
                }
                return Ok(tool_ok(search));
            }
            Ok(None) => {}
            Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => {
                return Err(WorkspaceError::invalid_argument(format!(
                    "invalid output search pattern: {error}"
                )));
            }
            Err(_) => {}
        }
    }

    let Some(session) = active_session else {
        if let Ok(Some(page)) = store.full_output_page(session_id, stream, requested_offset, limit)
        {
            let mut warnings = Vec::<String>::new();
            if ref_stream == "full" {
                warnings.push(
                    "legacy full output_ref defaults to stdout; use output_refs for stable stream paging"
                        .into(),
                );
            }
            return Ok(tool_ok(json!({
                "command_id": session_id,
                "session_id": session_id,
                "output_ref": output_ref,
                "stream_output_ref": format!("command:{session_id}:{stream}"),
                "legacy_stream_output_ref": format!("session:{session_id}:{stream}"),
                "stream": stream,
                "offset": page.offset,
                "requested_offset": requested_offset,
                "limit": limit,
                "content": String::from_utf8_lossy(&page.content),
                "next_offset": page.next_offset,
                "total_retained_bytes": 0,
                "total_stream_bytes": page.total_bytes,
                "head_retained_bytes": 0,
                "tail_retained_bytes": 0,
                "evicted_bytes": 0,
                "truncated": page.next_offset.is_some(),
                "source": "full",
                "raw_available": true,
                "warnings": warnings
            })));
        }
        return match store.get(session_id) {
            Ok(_) => Err(WorkspaceError::Tool {
                code: "OUTPUT_UNAVAILABLE",
                message: format!("Archived command output is unavailable: {session_id}"),
                category: "runtime",
                retryable: false,
            }),
            Err(error) => Err(error),
        };
    };

    tauri::async_runtime::block_on(session.refresh_status());
    let page = session.read_retained_page(stream, requested_offset, limit);
    let mut warnings = Vec::<String>::new();
    if ref_stream == "full" {
        warnings.push(
            "legacy full output_ref defaults to stdout; use output_refs for stable stream paging"
                .into(),
        );
    }
    if page.evicted_bytes > 0 {
        warnings.push(format!(
            "{} byte(s) from the middle of this stream were evicted; head and tail are retained",
            page.evicted_bytes
        ));
    }
    if page.offset != requested_offset {
        warnings.push(format!(
            "requested offset {requested_offset} was inside the evicted gap; resumed at retained tail offset {}",
            page.offset
        ));
    }

    Ok(tool_ok(json!({
        "command_id": session_id,
        "session_id": session_id,
        "output_ref": output_ref,
        "stream_output_ref": format!("command:{session_id}:{stream}"),
        "legacy_stream_output_ref": format!("session:{session_id}:{stream}"),
        "stream": stream,
        "offset": page.offset,
        "requested_offset": requested_offset,
        "limit": limit,
        "content": String::from_utf8_lossy(&page.content),
        "next_offset": page.next_offset,
        "total_retained_bytes": page.head_retained_bytes + page.tail_retained_bytes,
        "total_stream_bytes": page.total_bytes,
        "head_retained_bytes": page.head_retained_bytes,
        "tail_retained_bytes": page.tail_retained_bytes,
        "evicted_bytes": page.evicted_bytes,
        "truncated": page.evicted_bytes > 0 || page.next_offset.is_some(),
        "source": "retained",
        "raw_available": false,
        "warnings": warnings
    })))
}

pub fn write_stdin(store: &SessionStore, args: &Value) -> Result<Value, WorkspaceError> {
    let session_id = command_id_arg(args)?;
    let session = store.get(session_id)?;
    let chars = args.get("chars").and_then(Value::as_str).unwrap_or("");
    let max_output_bytes = args
        .get("max_output_bytes")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_SESSION_MAX_OUTPUT_BYTES as u64) as usize;

    let running = tauri::async_runtime::block_on(session.is_running());
    if !running {
        if !chars.is_empty() {
            return Err(WorkspaceError::Tool {
                code: "SESSION_CLOSED",
                message: "Session is closed; stdin write blocked.".into(),
                category: "runtime",
                retryable: false,
            });
        }
        return Ok(tool_ok(session.snapshot(max_output_bytes)));
    }

    if !chars.is_empty() {
        let mut stdin_guard = tauri::async_runtime::block_on(session.stdin.lock());
        let stdin = stdin_guard.as_mut().ok_or_else(|| WorkspaceError::Tool {
            code: "SESSION_CLOSED",
            message: "Session stdin is closed.".into(),
            category: "runtime",
            retryable: false,
        })?;
        tauri::async_runtime::block_on(async {
            stdin
                .write_all(chars.as_bytes())
                .await
                .map_err(|_| WorkspaceError::Tool {
                    code: "SESSION_CLOSED",
                    message: "Session stdin is closed.".into(),
                    category: "runtime",
                    retryable: false,
                })
        })?;
        let _ = tauri::async_runtime::block_on(stdin.flush());
    }

    let yield_ms = args
        .get("yield_time_ms")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_YIELD_TIME_MS)
        .min(MAX_YIELD_TIME_MS);
    tauri::async_runtime::block_on(async {
        if yield_ms > 0 {
            session
                .wait_for_exit_timeout(Duration::from_millis(yield_ms))
                .await;
        }
        session.refresh_status().await;
        if session.has_exited() {
            session.wait_for_readers().await;
        }
    });
    Ok(tool_ok(session.snapshot(max_output_bytes)))
}

pub fn kill_session(store: &SessionStore, args: &Value) -> Result<Value, WorkspaceError> {
    let session_id = command_id_arg(args)?;
    let session = store.get(session_id)?;
    let max_output_bytes = args
        .get("max_output_bytes")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_SESSION_MAX_OUTPUT_BYTES as u64) as usize;
    let wait_ms = args
        .get("wait_ms")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_KILL_WAIT_MS)
        .min(MAX_KILL_WAIT_MS);
    let signal = args.get("signal").and_then(Value::as_str).unwrap_or("TERM");

    let running = tauri::async_runtime::block_on(session.is_running());
    let mut killed = false;
    let mut status = "exited";
    let mut evicted = true;

    if running {
        session.mark_termination_reason("killed");
        tauri::async_runtime::block_on(async {
            if let Some(pid) = session.pid() {
                send_session_signal(pid, signal);
            } else {
                session.request_kill().await;
            }
            session
                .wait_for_exit_timeout(Duration::from_millis(wait_ms))
                .await;
        });
        tauri::async_runtime::block_on(session.refresh_status());
        if tauri::async_runtime::block_on(session.is_running()) {
            status = "terminating";
            evicted = false;
        } else {
            killed = true;
            status = "killed";
        }
    }

    let mut payload = session.snapshot(max_output_bytes);
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("killed".into(), json!(killed));
        obj.insert("status".into(), json!(status));
        obj.insert("evicted".into(), json!(evicted));
        if status == "terminating" {
            obj.insert(
                "warnings".into(),
                json!(["Process did not exit after kill; session retained for retry"]),
            );
        }
    }

    if evicted {
        store.remove(session_id);
    }

    Ok(tool_ok(payload))
}
