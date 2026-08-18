mod markdown;
mod model;
mod storage;

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::tools::context::ToolContext;
use crate::tools::workspace::{relative_display, tool_ok, Workspace, WorkspaceError, WorkspaceResult};

use self::model::{InitialInputRecord, SearchHit};

const BOOTSTRAP_RESPONSE_BUDGET: usize = 64 * 1024;
const DEFAULT_SEARCH_LIMIT: usize = 5;
const MAX_SEARCH_LIMIT: usize = 50;
const DEFAULT_READ_MAX_BYTES: usize = 16 * 1024;
const MAX_READ_MAX_BYTES: usize = 64 * 1024;
const CONTEXT_SESSION_SNIPPET_LIMIT: usize = 3;
const CONTEXT_SNIPPET_MAX_CHARS: usize = 512;

/// Return the small, UI-safe session catalog used by the desktop history picker.
/// The catalog contains metadata and bounded focus text, never the archive body.
pub fn list_sessions_for_workspace(workspace: &Workspace) -> WorkspaceResult<Value> {
    let history_dir = storage::resolve_history_dir(workspace, None, None)?;
    let report = storage::scan(workspace, &history_dir)?;
    let manifest = storage::build_manifest(&report);
    let sessions = report
        .documents
        .iter()
        .map(|document| session_summary(document, true))
        .collect::<Vec<_>>();
    Ok(json!({
        "context_revision": manifest.archive_revision,
        "history_count": sessions.len(),
        "sessions": sessions
    }))
}

/// Build the selected-history block for MCP initialization. This is deliberately
/// separate from tool results so it is emitted only when the workspace selection
/// changes, not after every file or command operation.
pub fn context_snapshot(ctx: &ToolContext) -> WorkspaceResult<Option<Value>> {
    if ctx.history_context_sessions.is_empty() {
        return Ok(None);
    }
    let history_dir = resolve_dir(ctx, &json!({}))?;
    let report = storage::scan(&ctx.workspace, &history_dir)?;
    let selected = ctx
        .history_context_sessions
        .iter()
        .filter_map(|number| {
            report
                .documents
                .iter()
                .find(|document| document.number == *number)
        })
        .map(|document| session_summary(document, false))
        .collect::<Vec<_>>();
    let selected_snippets = ctx
        .history_context_sessions
        .iter()
        .filter_map(|number| {
            report
                .documents
                .iter()
                .find(|document| document.number == *number)
        })
        .flat_map(|document| {
            session_summary(document, true)
                .get("snippets")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    let latest_checkpoints = selected
        .iter()
        .map(|session| {
            json!({
                "number": session["number"],
                "updated_at": session["updated_at"],
                "focus": session["latest_focus"]
            })
        })
        .collect::<Vec<_>>();
    let mut key_files = Vec::new();
    for session in &selected {
        if let Some(files) = session.get("key_files").and_then(Value::as_array) {
            for file in files.iter().filter_map(Value::as_str) {
                if !key_files.iter().any(|item: &String| item == file) {
                    key_files.push(file.to_string());
                }
                if key_files.len() >= 24 {
                    break;
                }
            }
        }
        if key_files.len() >= 24 {
            break;
        }
    }
    let revision = storage::sha256(
        &serde_json::to_vec(&selected).map_err(|error| {
            WorkspaceError::invalid_argument(format!("history context is not serializable: {error}"))
        })?,
    );
    Ok(Some(json!({
        "context_revision": format!("sha256:{revision}"),
        "selected_sessions": ctx.history_context_sessions,
        "session_metadata": selected,
        "latest_checkpoints": latest_checkpoints,
        "key_files": key_files,
        "selected_snippets": selected_snippets,
        "injection_mode": "index_and_bounded_snippets",
        "full_history": "available_on_demand_via_history_session_search_and_read"
    })))
}

fn session_summary(document: &self::model::HistoryDocument, include_snippets: bool) -> Value {
    let checkpoints = markdown::parse_checkpoint_records(&document.content);
    let latest = checkpoints.iter().max_by_key(|record| record.revision);
    let focus = latest
        .and_then(|record| {
            let value = if record.raw_user_input.trim().is_empty() {
                &record.user_intent
            } else {
                &record.raw_user_input
            };
            (!value.trim().is_empty()).then_some(value.clone())
        })
        .or_else(|| {
            markdown::parse_initial_input_records(&document.content)
                .into_iter()
                .max_by_key(|record| record.revision)
                .map(|record| record.raw_user_input)
        })
        .unwrap_or_else(|| "尚未记录任务焦点".to_string());
    let mut key_files = Vec::new();
    for record in checkpoints.iter().rev() {
        for file in &record.files_changed {
            if !file.trim().is_empty() && !key_files.contains(file) {
                key_files.push(file.clone());
            }
            if key_files.len() >= 12 {
                break;
            }
        }
        if key_files.len() >= 12 {
            break;
        }
    }
    let snippets = if include_snippets {
        checkpoints
            .iter()
            .rev()
            .take(CONTEXT_SESSION_SNIPPET_LIMIT)
            .map(|record| {
                let mut parts = Vec::new();
                let text = if record.raw_user_input.trim().is_empty() {
                    record.user_intent.trim()
                } else {
                    record.raw_user_input.trim()
                };
                if !text.is_empty() {
                    parts.push(text.to_string());
                }
                if !record.decisions.is_empty() {
                    parts.push(format!("决策：{}", record.decisions.join("；")));
                }
                if !record.files_changed.is_empty() {
                    parts.push(format!("文件：{}", record.files_changed.join("、")));
                }
                json!({
                    "turn_id": record.turn_id,
                    "timestamp": record.timestamp,
                    "text": storage::truncate_text(&parts.join(" "), CONTEXT_SNIPPET_MAX_CHARS)
                })
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    json!({
        "number": document.number,
        "path": document.path,
        "title": markdown::document_title(&document.content, document.number),
        "session_key": document.session_key,
        "created_at": document.created_at,
        "updated_at": document.updated_at,
        "bytes": document.content.len(),
        "entry_count": checkpoints.len(),
        "latest_focus": storage::truncate_text(&focus, CONTEXT_SNIPPET_MAX_CHARS),
        "key_files": key_files,
        "snippets": snippets
    })
}

pub fn bootstrap(ctx: &ToolContext, args: &Value) -> WorkspaceResult<Value> {
    let (session_key, source) = resolve_session_key(ctx, args)?;
    let history_dir = resolve_dir(ctx, args)?;
    storage::ensure_directory(&history_dir)?;
    let _lock = storage::lock_directory(&history_dir)?;
    let report = storage::scan(&ctx.workspace, &history_dir)?;
    reject_ambiguous_history(&report)?;
    if !report.missing_numbers.is_empty() {
        return Err(history_error(
            "HISTORY_SEQUENCE_CONFLICT",
            "History numbering contains gaps; run history_session_validate before creating a session.",
            "validation",
            true,
            json!({"missing_numbers": report.missing_numbers}),
        ));
    }

    let mut warnings = derived_file_warnings(&history_dir);
    let requested_initial_input = args
        .get("initial_user_input")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty());
    let existing = report
        .documents
        .iter()
        .find(|document| document.session_key.as_deref() == Some(session_key.as_str()));
    let (current_number, current_path, created, resumed, initial_input_captured) =
        if let Some(document) = existing {
            let initial_records = markdown::parse_initial_input_records(&document.content);
            let initial_input_captured = !initial_records.is_empty();
            let initial_input_supplied = requested_initial_input.is_some();
            if let Some(mut initial_input) = requested_initial_input {
                let redacted = markdown::redact_text(&mut initial_input);
                if redacted {
                    warnings.push("首次用户输入含疑似敏感信息，归档内容已脱敏。".into());
                }
                let content_hash = markdown::initial_input_fingerprint(&initial_input);
                let duplicate = initial_records
                    .iter()
                    .any(|record| record.content_hash == content_hash);
                if !duplicate {
                    let latest = initial_records.iter().max_by_key(|record| record.revision);
                    let revision = latest.map(|record| record.revision + 1).unwrap_or(1);
                    let record = InitialInputRecord {
                        raw_user_input: initial_input,
                        captured_at: now_timestamp(),
                        revision,
                        supersedes: latest
                            .map(|record| format!("initial-input revision-{}", record.revision)),
                        content_hash,
                    };
                    let content = markdown::with_updated_at(&document.content, &record.captured_at);
                    storage::write_markdown(
                        &history_dir.join(format!("{}.md", document.number)),
                        &markdown::append_initial_input_revision(&content, &record),
                    )?;
                }
            } else if !initial_input_captured {
                warnings.push(
                    "未提供 initial_user_input；服务端无法读取未作为工具参数传入的首次用户输入。"
                        .into(),
                );
            }
            (
                document.number,
                document.path.clone(),
                false,
                true,
                initial_input_captured || initial_input_supplied,
            )
        } else {
            if !args
                .get("create_if_missing")
                .and_then(Value::as_bool)
                .unwrap_or(true)
            {
                return Err(history_error(
                    "SESSION_NOT_BOOTSTRAPPED",
                    "No history mapping exists for this session_key.",
                    "not_found",
                    false,
                    json!({"session_key_source": source}),
                ));
            }
            let number = report.latest_number().unwrap_or(0) + 1;
            let relative_path = format!("{}/{number}.md", history_dir_display(ctx, &history_dir));
            let timestamp = now_timestamp();
            let title = args
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("开发会话");
            let initial_input = requested_initial_input.map(|mut value| {
                let redacted = markdown::redact_text(&mut value);
                if redacted {
                    warnings.push("首次用户输入含疑似敏感信息，归档内容已脱敏。".into());
                }
                InitialInputRecord {
                    content_hash: markdown::initial_input_fingerprint(&value),
                    raw_user_input: value,
                    captured_at: timestamp.clone(),
                    revision: 1,
                    supersedes: None,
                }
            });
            if initial_input.is_none() {
                warnings.push(
                    "未提供 initial_user_input；服务端无法读取未作为工具参数传入的首次用户输入。"
                        .into(),
                );
            }
            let content = markdown::render_document(
                number,
                title,
                &session_key,
                &timestamp,
                initial_input.as_ref(),
            );
            storage::write_markdown(&history_dir.join(format!("{number}.md")), &content)?;
            (number, relative_path, true, false, initial_input.is_some())
        };

    let refreshed = storage::scan(&ctx.workspace, &history_dir)?;
    reject_ambiguous_history(&refreshed)?;
    let manifest = storage::build_manifest(&refreshed);
    let previous_state_revision = storage::read_state(&history_dir)
        .ok()
        .flatten()
        .map(|state| state.state_revision)
        .unwrap_or(0);
    let state = storage::build_state(
        &refreshed,
        &manifest,
        Some(current_number),
        &now_timestamp(),
        previous_state_revision + 1,
    );
    storage::write_index(&history_dir, &storage::rebuild_index(&refreshed))?;
    storage::write_manifest(&history_dir, &manifest)?;
    storage::write_state(&history_dir, &state)?;

    if ctx.tool_profile == "compact" {
        return Ok(tool_ok(json!({
            "context_mode": "compact",
            "index_only": true,
            "is_new_session": created,
            "session_key": session_key,
            "session_key_source": source,
            "current_number": current_number,
            "current_path": current_path,
            "created": created,
            "resumed": resumed,
            "initial_input_captured": initial_input_captured,
            "sequence_valid": refreshed.sequence_valid(),
            "history_count": refreshed.documents.len(),
            "total_history_bytes": refreshed.total_bytes(),
            "state_revision": state.state_revision,
            "archive_revision": manifest.archive_revision,
            "warnings": warnings
        })));
    }

    let mut result = json!({
        "is_new_session": created,
        "session_key": session_key,
        "session_key_source": source,
        "platform_conversation_id": (source == "platform_conversation_id").then_some(true),
        "current_number": current_number,
        "current_path": current_path,
        "created": created,
        "resumed": resumed,
        "initial_input_captured": initial_input_captured,
        "sequence_valid": refreshed.sequence_valid(),
        "history_count": refreshed.documents.len(),
        "total_history_bytes": refreshed.total_bytes(),
        "state_revision": state.state_revision,
        "archive_revision": manifest.archive_revision,
        "state": state,
        "history_read_mode": "bounded_state_with_on_demand_search_and_read",
        "persistence_mode": "model_mediated_tool_calls",
        "assistant_instructions": "Use the bounded state to begin work. To recover exact earlier context, call history_session_search and then history_session_read for only the relevant archive. Preserve session_key and current_path. Before the final response for each user task, call history_session_checkpoint with the user's verbatim raw_user_input. The server can only save text passed as tool arguments and reports missing input explicitly.",
        "required_next_actions": [
            "review_bounded_state",
            "search_or_read_relevant_archives_when_precision_is_needed",
            "verify_workspace_state",
            "execute_user_task",
            "checkpoint_with_raw_user_input_before_final_response"
        ],
        "checkpoint_policy": {
            "tool": "history_session_checkpoint",
            "session_key": session_key,
            "expected_path": current_path,
            "raw_user_input_required_for_full_fidelity": true,
            "required_before_final_response": true,
            "automatic_background_persistence": false
        },
        "search_guide": {
            "tool": "history_session_search",
            "then_read_with": "history_session_read",
            "archive_is_lossless": true
        },
        "warnings": warnings
    });
    bound_bootstrap_result(&mut result);
    Ok(tool_ok(result))
}

pub fn checkpoint(ctx: &ToolContext, args: &Value) -> WorkspaceResult<Value> {
    if !ctx.history_recording {
        return Ok(tool_ok(json!({
            "recorded": false,
            "reason": "session_recording_disabled"
        })));
    }
    let session_key = resolve_session_key(ctx, args)?.0;
    let history_dir = resolve_dir(ctx, args)?;
    storage::ensure_directory(&history_dir)?;
    let report = storage::scan(&ctx.workspace, &history_dir)?;
    reject_ambiguous_history(&report)?;
    let document = report
        .documents
        .iter()
        .find(|document| document.session_key.as_deref() == Some(session_key.as_str()));
    if document.is_none() {
        let bootstrap_result = bootstrap(
            ctx,
            &json!({
                "session_key": session_key,
                "history_dir": args.get("history_dir"),
                "workspace_root": args.get("workspace_root"),
                "title": "开发会话",
                "create_if_missing": true
            }),
        )?;
        let expected_path = bootstrap_result
            .get("current_path")
            .and_then(Value::as_str)
            .ok_or_else(|| history_error(
                "SESSION_TARGET_UNAVAILABLE",
                "Unable to create a lazy history target.",
                "internal",
                true,
                json!({}),
            ))?;
        let mut retry_args = args.clone();
        if let Some(object) = retry_args.as_object_mut() {
            object.insert("session_key".into(), Value::String(session_key.clone()));
            object.insert("expected_path".into(), Value::String(expected_path.to_string()));
        }
        return checkpoint(ctx, &retry_args);
    }
    let expected_path = args
        .get("expected_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| document.expect("checked above").path.as_str())
        .to_string();
    let host_session_key_mismatch = host_session_key(args)
        .map(|host| host != session_key.as_str())
        .unwrap_or(false);
    let _lock = storage::lock_directory(&history_dir)?;
    let report = storage::scan(&ctx.workspace, &history_dir)?;
    reject_ambiguous_history(&report)?;
    let document = report
        .documents
        .iter()
        .find(|document| document.session_key.as_deref() == Some(session_key.as_str()))
        .ok_or_else(session_not_bootstrapped)?;
    if document.path != expected_path {
        return Err(history_error(
            "SESSION_TARGET_MISMATCH",
            "The checkpoint target does not match the session initialized by bootstrap.",
            "validation",
            false,
            json!({
                "expected_path": expected_path,
                "resolved_path": document.path,
                "session_key": session_key
            }),
        ));
    }

    let timestamp = now_timestamp();
    let mut record = markdown::checkpoint_from_args(args, &timestamp)
        .map_err(WorkspaceError::invalid_argument)?;
    let user_input_captured = !record.raw_user_input.trim().is_empty();
    let redacted = markdown::redact_record(&mut record);
    record.content_hash = markdown::checkpoint_fingerprint(&record);
    let existing = markdown::parse_checkpoint_records(&document.content);
    let same_turn = existing
        .iter()
        .filter(|existing| existing.turn_id == record.turn_id)
        .collect::<Vec<_>>();
    let duplicate_ignored = same_turn.iter().any(|existing| {
        let fingerprint = if existing.content_hash.is_empty() {
            markdown::checkpoint_fingerprint(existing)
        } else {
            existing.content_hash.clone()
        };
        fingerprint == record.content_hash
    });
    let (updated, final_content) = if duplicate_ignored {
        (false, document.content.clone())
    } else {
        let latest = same_turn.iter().max_by_key(|existing| existing.revision);
        record.revision = latest.map(|existing| existing.revision + 1).unwrap_or(1);
        record.supersedes =
            latest.map(|existing| format!("{} revision-{}", existing.turn_id, existing.revision));
        let content = markdown::with_updated_at(&document.content, &record.timestamp);
        (true, markdown::append_checkpoint_record(&content, &record))
    };
    if updated {
        storage::write_markdown(
            &history_dir.join(format!("{}.md", document.number)),
            &final_content,
        )?;
    }

    let refreshed = storage::scan(&ctx.workspace, &history_dir)?;
    let manifest = storage::build_manifest(&refreshed);
    let state_revision = storage::read_state(&history_dir)
        .ok()
        .flatten()
        .map(|state| state.state_revision + 1)
        .unwrap_or(1);
    let state = storage::build_state(
        &refreshed,
        &manifest,
        Some(document.number),
        &now_timestamp(),
        state_revision,
    );
    storage::write_index(&history_dir, &storage::rebuild_index(&refreshed))?;
    storage::write_manifest(&history_dir, &manifest)?;
    storage::write_state(&history_dir, &state)?;

    let mut warnings: Vec<String> = Vec::new();
    if !user_input_captured {
        warnings
            .push("未提供 raw_user_input；服务端无法读取未作为工具参数传入的本轮用户输入。".into());
    }
    if redacted {
        warnings.push("检测到疑似敏感信息，归档内容已脱敏。".into());
    }
    if host_session_key_mismatch {
        warnings.push(
            "宿主会话标识已变化；本次仍使用解析出的稳定目标，未切换历史文件。".into(),
        );
    }
    if ctx.tool_profile == "compact" {
        return Ok(tool_ok(json!({
            "recorded": true,
            "session_number": document.number,
            "path": document.path,
            "session_key": session_key,
            "turn_id": record.turn_id,
            "updated": updated,
            "duplicate_ignored": duplicate_ignored,
            "user_input_captured": user_input_captured,
            "archive_revision": manifest.archive_revision,
            "state_revision": state.state_revision,
            "warnings": warnings
        })));
    }
    Ok(tool_ok(json!({
        "session_number": document.number,
        "path": document.path,
        "session_key": session_key,
        "expected_path": expected_path,
        "host_session_key_mismatch": host_session_key_mismatch,
        "turn_id": record.turn_id,
        "revision": record.revision,
        "supersedes": record.supersedes,
        "created": false,
        "updated": updated,
        "duplicate_ignored": duplicate_ignored,
        "user_input_captured": user_input_captured,
        "content_hash": storage::sha256(final_content.as_bytes()),
        "archive_revision": manifest.archive_revision,
        "state_revision": state.state_revision,
        "warnings": warnings
    })))
}

pub fn search(ctx: &ToolContext, args: &Value) -> WorkspaceResult<Value> {
    let history_dir = resolve_dir(ctx, args)?;
    let report = storage::scan(&ctx.workspace, &history_dir)?;
    let manifest = storage::read_manifest(&history_dir)
        .ok()
        .flatten()
        .filter(|manifest| {
            manifest.archive_revision == storage::build_manifest(&report).archive_revision
        })
        .unwrap_or_else(|| storage::build_manifest(&report));
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let tokens = storage::tokenize(query);
    let limit = bounded_usize(args, "limit", DEFAULT_SEARCH_LIMIT, MAX_SEARCH_LIMIT)?;
    let cursor = bounded_usize(args, "cursor", 0, usize::MAX)?;
    let mut hits = manifest
        .entries
        .iter()
        .filter_map(|entry| {
            let document = report
                .documents
                .iter()
                .find(|document| document.number == entry.number)?;
            let score = search_score(entry, &document.content, &tokens);
            if !tokens.is_empty() && score == 0 {
                return None;
            }
            Some(SearchHit {
                number: entry.number,
                path: entry.path.clone(),
                title: entry.title.clone(),
                updated_at: entry.updated_at.clone(),
                sha256: entry.sha256.clone(),
                score,
                snippet: search_snippet(&document.content, &tokens),
            })
        })
        .collect::<Vec<_>>();
    hits.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| right.number.cmp(&left.number))
    });
    let total_matches = hits.len();
    let end = cursor.saturating_add(limit).min(total_matches);
    let page = if cursor >= total_matches {
        Vec::new()
    } else {
        hits.drain(cursor..end).collect()
    };
    Ok(tool_ok(json!({
        "query": query,
        "history_count": report.documents.len(),
        "total_matches": total_matches,
        "cursor": cursor,
        "limit": limit,
        "next_cursor": (end < total_matches).then_some(end),
        "results": page
    })))
}

pub fn read(ctx: &ToolContext, args: &Value) -> WorkspaceResult<Value> {
    let history_dir = resolve_dir(ctx, args)?;
    let path = if let Some(number) = args.get("number").and_then(Value::as_u64) {
        history_dir.join(format!("{number}.md"))
    } else if let Some(path) = args.get("path").and_then(Value::as_str) {
        ctx.workspace.resolve_read_path(path)?.path
    } else {
        return Err(history_error(
            "HISTORY_READ_NOT_FOUND",
            "Pass an existing archive number or a manifest-returned relative path.",
            "not_found",
            false,
            json!({}),
        ));
    };
    if path.parent() != Some(history_dir.as_path())
        || path.extension().and_then(|value| value.to_str()) != Some("md")
        || path.file_stem().and_then(|value| value.to_str()).and_then(|value| value.parse::<u64>().ok()).is_none()
    {
        return Err(history_error(
            "HISTORY_READ_NOT_FOUND",
            "Pass an existing archive number or a manifest-returned relative path.",
            "not_found",
            false,
            json!({}),
        ));
    }
    let content = fs::read_to_string(&path).map_err(|_| {
        history_error(
            "HISTORY_READ_NOT_FOUND",
            "The selected history archive does not exist.",
            "not_found",
            false,
            json!({}),
        )
    })?;
    let number = path
        .file_stem()
        .and_then(|value| value.to_str())
        .and_then(|value| value.parse::<u64>().ok())
        .expect("validated numeric history path");
    let display_path = relative_display(ctx.workspace.root(), &path);
    let bytes = content.as_bytes();
    let content_hash = storage::sha256(bytes);
    if let Some(expected_hash) = args.get("expected_hash").and_then(Value::as_str) {
        if expected_hash != content_hash {
            return Err(history_error(
                "HISTORY_ARCHIVE_CHANGED",
                "The archive changed since the previous page; restart the read with the new hash.",
                "conflict",
                true,
                json!({"expected_hash": expected_hash, "content_hash": content_hash}),
            ));
        }
    }
    let cursor = bounded_usize(args, "cursor", 0, bytes.len())?;
    if cursor > bytes.len() || !content.is_char_boundary(cursor) {
        return Err(history_error(
            "HISTORY_CURSOR_INVALID",
            "cursor must be a UTF-8 character boundary inside the archive.",
            "validation",
            false,
            json!({"cursor": cursor, "total_bytes": bytes.len()}),
        ));
    }
    let requested = bounded_usize(
        args,
        "max_bytes",
        DEFAULT_READ_MAX_BYTES,
        MAX_READ_MAX_BYTES,
    )?;
    let mut end = cursor.saturating_add(requested).min(bytes.len());
    while end > cursor && !content.is_char_boundary(end) {
        end -= 1;
    }
    if end == cursor && end < bytes.len() {
        let next = content[cursor..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(0);
        end = cursor + next;
    }
    Ok(tool_ok(json!({
        "number": number,
        "path": display_path,
        "content": &content[cursor..end],
        "cursor": cursor,
        "next_cursor": (end < bytes.len()).then_some(end),
        "total_bytes": bytes.len(),
        "content_hash": content_hash
    })))
}

pub fn validate(ctx: &ToolContext, args: &Value) -> WorkspaceResult<Value> {
    let history_dir = resolve_dir(ctx, args)?;
    let repair = args.get("repair").and_then(Value::as_bool).unwrap_or(false);
    if repair {
        storage::ensure_directory(&history_dir)?;
    }
    let index_status = derived_status(storage::read_index(&history_dir));
    let manifest_status = derived_status(storage::read_manifest(&history_dir));
    let state_status = derived_status(storage::read_state(&history_dir));
    let report = storage::scan(&ctx.workspace, &history_dir)?;
    let mut warnings = Vec::<String>::new();
    if !report.duplicate_session_keys.is_empty() {
        warnings.push("存在重复 session_key，相关映射未写入索引。".into());
    }
    let repaired = if repair {
        let _lock = storage::lock_directory(&history_dir)?;
        let locked_report = storage::scan(&ctx.workspace, &history_dir)?;
        let manifest = storage::build_manifest(&locked_report);
        let state_revision = storage::read_state(&history_dir)
            .ok()
            .flatten()
            .map(|state| state.state_revision + 1)
            .unwrap_or(1);
        let state = storage::build_state(
            &locked_report,
            &manifest,
            locked_report.latest_number(),
            &now_timestamp(),
            state_revision,
        );
        storage::write_index(&history_dir, &storage::rebuild_index(&locked_report))?;
        storage::write_manifest(&history_dir, &manifest)?;
        storage::write_state(&history_dir, &state)?;
        true
    } else {
        false
    };
    let latest_number = report.latest_number();
    let latest_path = latest_number.and_then(|number| {
        report
            .documents
            .iter()
            .find(|document| document.number == number)
            .map(|document| document.path.clone())
    });
    Ok(tool_ok(json!({
        "sequence_valid": report.sequence_valid(),
        "numbers": report.numbers,
        "missing_numbers": report.missing_numbers,
        "duplicate_session_keys": report.duplicate_session_keys,
        "invalid_files": report.invalid_files,
        "empty_files": report.empty_files,
        "latest_number": latest_number,
        "latest_path": latest_path,
        "archive_count": report.documents.len(),
        "total_archive_bytes": report.total_bytes(),
        "index_status": index_status,
        "manifest_status": manifest_status,
        "state_status": state_status,
        "repaired": repaired,
        "warnings": warnings
    })))
}

fn search_score(entry: &model::ManifestEntry, content: &str, tokens: &[String]) -> u64 {
    if tokens.is_empty() {
        return 1;
    }
    let title = entry.title.to_lowercase();
    let keywords = entry.keywords.join(" ").to_lowercase();
    let content = content.to_lowercase();
    tokens.iter().fold(0, |score, token| {
        let mut token_score = 0;
        if title.contains(token) {
            token_score += 16;
        }
        if keywords.contains(token) {
            token_score += 10;
        }
        if content.contains(token) {
            token_score += 4;
        }
        token_score + score
    })
}

fn search_snippet(content: &str, tokens: &[String]) -> String {
    if tokens.is_empty() {
        return storage::truncate_text(content.trim(), 280);
    }
    let (normalized, source_offsets) = normalized_search_text(content);
    let start = tokens
        .iter()
        .filter_map(|token| normalized.find(token))
        .filter_map(|offset| source_offsets.get(offset).copied())
        .min()
        .unwrap_or(0);
    let prefix = &content[..start];
    let start = prefix
        .char_indices()
        .rev()
        .nth(80)
        .map(|(index, _)| index)
        .unwrap_or(0);
    storage::truncate_text(content[start..].trim(), 280)
}

fn normalized_search_text(content: &str) -> (String, Vec<usize>) {
    let mut normalized = String::with_capacity(content.len());
    let mut source_offsets = Vec::with_capacity(content.len());
    for (source_offset, character) in content.char_indices() {
        for lower in character.to_lowercase() {
            let byte_count = lower.len_utf8();
            normalized.push(lower);
            source_offsets.extend(std::iter::repeat_n(source_offset, byte_count));
        }
    }
    (normalized, source_offsets)
}

fn bounded_usize(
    args: &Value,
    name: &str,
    default: usize,
    maximum: usize,
) -> WorkspaceResult<usize> {
    let Some(value) = args.get(name) else {
        return Ok(default);
    };
    let value = value.as_u64().ok_or_else(|| {
        history_error(
            "HISTORY_CURSOR_INVALID",
            &format!("{name} must be a non-negative integer."),
            "validation",
            false,
            json!({"argument": name}),
        )
    })?;
    let value = usize::try_from(value).map_err(|_| {
        history_error(
            "HISTORY_CURSOR_INVALID",
            &format!("{name} is too large."),
            "validation",
            false,
            json!({"argument": name}),
        )
    })?;
    if value > maximum || matches!(name, "limit" | "max_bytes") && value == 0 {
        return Err(history_error(
            "HISTORY_CURSOR_INVALID",
            &format!("{name} is outside the allowed range."),
            "validation",
            false,
            json!({"argument": name, "maximum": maximum}),
        ));
    }
    Ok(value)
}

fn bound_bootstrap_result(result: &mut Value) {
    let mut encoded = serde_json::to_vec(result).unwrap_or_default();
    if encoded.len() <= BOOTSTRAP_RESPONSE_BUDGET {
        return;
    }
    if let Some(state) = result.get_mut("state").and_then(Value::as_object_mut) {
        state.insert("recent_changes".into(), json!([]));
        state.insert("open_items".into(), json!([]));
        state.insert("references".into(), json!([]));
        state.insert(
            "current_focus".into(),
            Value::String("当前状态已收紧；请用 history_session_search 定位档案。".into()),
        );
    }
    if let Some(object) = result.as_object_mut() {
        object.insert("state_truncated".into(), Value::Bool(true));
    }
    encoded = serde_json::to_vec(result).unwrap_or_default();
    if encoded.len() > BOOTSTRAP_RESPONSE_BUDGET {
        if let Some(object) = result.as_object_mut() {
            object.insert("assistant_instructions".into(), Value::String("Use history_session_search and history_session_read for exact archive context; checkpoint raw_user_input before final response.".into()));
            object.insert(
                "warnings".into(),
                json!(["Bootstrap state was reduced to preserve a bounded remote response."]),
            );
        }
    }
}

fn derived_file_warnings(history_dir: &std::path::Path) -> Vec<String> {
    let mut warnings = Vec::new();
    for (label, result) in [
        (
            "历史索引",
            storage::read_index(history_dir).map(|value| value.is_some()),
        ),
        (
            "memory/manifest.json",
            storage::read_manifest(history_dir).map(|value| value.is_some()),
        ),
        (
            "memory/state.json",
            storage::read_state(history_dir).map(|value| value.is_some()),
        ),
    ] {
        match result {
            Ok(true) => {}
            Ok(false) => warnings.push(format!("{label} 缺失，已根据 Markdown 档案重建。")),
            Err(_) => warnings.push(format!("{label} 损坏，已根据 Markdown 档案重建。")),
        }
    }
    let readme = history_dir.join("README.md");
    if readme.exists() {
        let _ = fs::read_to_string(readme);
    }
    warnings
}

fn derived_status<T>(result: WorkspaceResult<Option<T>>) -> &'static str {
    match result {
        Ok(Some(_)) => "valid",
        Ok(None) => "missing",
        Err(_) => "invalid",
    }
}

fn host_session_key(args: &Value) -> Option<&str> {
    args.get("_host_session_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn resolve_dir(ctx: &ToolContext, args: &Value) -> WorkspaceResult<std::path::PathBuf> {
    storage::resolve_history_dir(
        &ctx.workspace,
        args.get("workspace_root").and_then(Value::as_str),
        args.get("history_dir").and_then(Value::as_str),
    )
}

fn resolve_session_key(ctx: &ToolContext, args: &Value) -> WorkspaceResult<(String, &'static str)> {
    if let Some(value) = host_session_key(args) {
        return Ok((value.to_string(), "platform_conversation_id"));
    }
    if let Some(value) = args
        .get("session_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok((value.to_string(), "explicit_session_key"));
    }
    let fingerprint = storage::sha256(ctx.workspace.root_display().as_bytes());
    Ok((
        format!("workspace-session-{}", &fingerprint[..16]),
        "workspace_fallback",
    ))
}

fn reject_ambiguous_history(report: &model::ScanReport) -> WorkspaceResult<()> {
    if report.duplicate_session_keys.is_empty() {
        return Ok(());
    }
    Err(history_error(
        "HISTORY_INDEX_CONFLICT",
        "Multiple history files declare the same session_key.",
        "validation",
        false,
        json!({"duplicate_session_keys": report.duplicate_session_keys}),
    ))
}

fn session_not_bootstrapped() -> WorkspaceError {
    history_error(
        "SESSION_NOT_BOOTSTRAPPED",
        "The session_key has not been bootstrapped.",
        "not_found",
        false,
        json!({}),
    )
}

fn history_error(
    code: &'static str,
    message: &str,
    category: &'static str,
    retryable: bool,
    details: Value,
) -> WorkspaceError {
    WorkspaceError::ToolDetails {
        code,
        message: message.into(),
        category,
        retryable,
        details,
    }
}

fn history_dir_display(ctx: &ToolContext, path: &std::path::Path) -> String {
    crate::tools::workspace::relative_display(ctx.workspace.root(), path)
}

fn now_timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("unix:{seconds}")
}
