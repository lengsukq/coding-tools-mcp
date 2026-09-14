use super::*;

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
            .ok_or_else(|| {
                history_error(
                    "SESSION_TARGET_UNAVAILABLE",
                    "Unable to create a lazy history target.",
                    "internal",
                    true,
                    json!({}),
                )
            })?;
        let mut retry_args = args.clone();
        if let Some(object) = retry_args.as_object_mut() {
            object.insert("session_key".into(), Value::String(session_key.clone()));
            object.insert(
                "expected_path".into(),
                Value::String(expected_path.to_string()),
            );
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
        warnings.push("宿主会话标识已变化；本次仍使用解析出的稳定目标，未切换历史文件。".into());
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
