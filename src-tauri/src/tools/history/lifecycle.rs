use super::*;

struct BootstrapSessionOutcome {
    number: u64,
    path: String,
    created: bool,
    resumed: bool,
    initial_input_captured: bool,
}

fn rebuild_and_persist_derived_files(
    history_dir: &std::path::Path,
    report: &model::ScanReport,
    current_number: u64,
    next_state_revision: u64,
) -> WorkspaceResult<(model::MemoryManifest, model::MemoryState)> {
    let manifest = storage::build_manifest(report);
    let state = storage::build_state(
        report,
        &manifest,
        Some(current_number),
        &now_timestamp(),
        next_state_revision,
    );
    storage::write_index(history_dir, &storage::rebuild_index(report))?;
    storage::write_manifest(history_dir, &manifest)?;
    storage::write_state(history_dir, &state)?;
    Ok((manifest, state))
}

fn resume_existing_session(
    history_dir: &std::path::Path,
    report: &mut model::ScanReport,
    existing_index: usize,
    requested_initial_input: Option<String>,
    warnings: &mut Vec<String>,
) -> WorkspaceResult<BootstrapSessionOutcome> {
    let document = report.documents[existing_index].clone();
    let initial_records = markdown::parse_initial_input_records(&document.content);
    let initial_input_captured = !initial_records.is_empty();
    let initial_input_supplied = requested_initial_input.is_some();
    if let Some(mut initial_input) = requested_initial_input {
        if markdown::redact_text(&mut initial_input) {
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
            let updated_content = markdown::append_initial_input_revision(&content, &record);
            storage::write_markdown(
                &history_dir.join(format!("{}.md", document.number)),
                &updated_content,
            )?;
            storage::upsert_document(
                report,
                model::HistoryDocument {
                    number: document.number,
                    path: document.path.clone(),
                    content: updated_content,
                    session_key: document.session_key.clone(),
                    created_at: document.created_at.clone(),
                    updated_at: Some(record.captured_at),
                },
            );
        }
    } else if !initial_input_captured {
        warnings.push(
            "未提供 initial_user_input；服务端无法读取未作为工具参数传入的首次用户输入。".into(),
        );
    }
    Ok(BootstrapSessionOutcome {
        number: document.number,
        path: document.path,
        created: false,
        resumed: true,
        initial_input_captured: initial_input_captured || initial_input_supplied,
    })
}

struct NewSessionRequest<'a> {
    args: &'a Value,
    session_key: &'a str,
    source: &'a str,
    requested_initial_input: Option<String>,
}

fn create_new_session(
    ctx: &ToolContext,
    history_dir: &std::path::Path,
    report: &mut model::ScanReport,
    req: NewSessionRequest<'_>,
    warnings: &mut Vec<String>,
) -> WorkspaceResult<BootstrapSessionOutcome> {
    if !req
        .args
        .get("create_if_missing")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        return Err(history_error(
            "SESSION_NOT_BOOTSTRAPPED",
            "No history mapping exists for this session_key.",
            "not_found",
            false,
            json!({"session_key_source": req.source}),
        ));
    }
    let number = report.latest_number().unwrap_or(0) + 1;
    let doc_path = history_dir.join(format!("{number}.md"));
    let relative_path = relative_display(ctx.workspace.root(), &doc_path);
    let timestamp = now_timestamp();
    let title = req
        .args
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("开发会话");
    let initial_input = req.requested_initial_input.map(|mut value| {
        if markdown::redact_text(&mut value) {
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
            "未提供 initial_user_input；服务端无法读取未作为工具参数传入的首次用户输入。".into(),
        );
    }
    let content = markdown::render_document(
        number,
        title,
        req.session_key,
        &timestamp,
        initial_input.as_ref(),
    );
    storage::write_markdown(&doc_path, &content)?;
    storage::upsert_document(
        report,
        model::HistoryDocument {
            number,
            path: relative_path.clone(),
            content,
            session_key: Some(req.session_key.to_string()),
            created_at: Some(timestamp.clone()),
            updated_at: Some(timestamp),
        },
    );
    Ok(BootstrapSessionOutcome {
        number,
        path: relative_path,
        created: true,
        resumed: false,
        initial_input_captured: initial_input.is_some(),
    })
}

pub fn bootstrap(ctx: &ToolContext, args: &Value) -> WorkspaceResult<Value> {
    let (session_key, source) = resolve_session_key(ctx, args)?;
    let history_dir = resolve_dir(ctx, args)?;
    storage::ensure_directory(&history_dir)?;
    let _lock = storage::lock_directory(&history_dir)?;
    let mut report = storage::scan(&ctx.workspace, &history_dir)?;
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

    let (mut warnings, previous_state_revision) = derived_file_warnings(&history_dir);
    let requested_initial_input = args
        .get("initial_user_input")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty());
    let existing_index = report
        .documents
        .iter()
        .position(|document| document.session_key.as_deref() == Some(session_key.as_str()));
    let outcome = if let Some(index) = existing_index {
        resume_existing_session(
            &history_dir,
            &mut report,
            index,
            requested_initial_input,
            &mut warnings,
        )?
    } else {
        create_new_session(
            ctx,
            &history_dir,
            &mut report,
            NewSessionRequest {
                args,
                session_key: &session_key,
                source,
                requested_initial_input,
            },
            &mut warnings,
        )?
    };

    let (manifest, state) = rebuild_and_persist_derived_files(
        &history_dir,
        &report,
        outcome.number,
        previous_state_revision + 1,
    )?;

    if ctx.tool_profile.is_compact() {
        return Ok(tool_ok(json!({
            "context_mode": "compact",
            "index_only": true,
            "is_new_session": outcome.created,
            "session_key": session_key,
            "session_key_source": source,
            "current_number": outcome.number,
            "current_path": outcome.path,
            "created": outcome.created,
            "resumed": outcome.resumed,
            "initial_input_captured": outcome.initial_input_captured,
            "sequence_valid": report.sequence_valid(),
            "history_count": report.documents.len(),
            "total_history_bytes": report.total_bytes(),
            "state_revision": state.state_revision,
            "archive_revision": manifest.archive_revision,
            "warnings": warnings
        })));
    }

    let mut result = json!({
        "is_new_session": outcome.created,
        "session_key": session_key,
        "session_key_source": source,
        "platform_conversation_id": (source == "platform_conversation_id").then_some(true),
        "current_number": outcome.number,
        "current_path": outcome.path,
        "created": outcome.created,
        "resumed": outcome.resumed,
        "initial_input_captured": outcome.initial_input_captured,
        "sequence_valid": report.sequence_valid(),
        "history_count": report.documents.len(),
        "total_history_bytes": report.total_bytes(),
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
            "expected_path": outcome.path,
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

fn resolve_checkpoint_document(
    ctx: &ToolContext,
    history_dir: &std::path::Path,
    report: &model::ScanReport,
    session_key: &str,
    timestamp: &str,
) -> WorkspaceResult<(model::HistoryDocument, bool)> {
    if let Some(index) = report
        .documents
        .iter()
        .position(|document| document.session_key.as_deref() == Some(session_key))
    {
        return Ok((report.documents[index].clone(), false));
    }
    if !report.missing_numbers.is_empty() {
        return Err(history_error(
            "HISTORY_SEQUENCE_CONFLICT",
            "History numbering contains gaps; run history_session_validate before creating a session.",
            "validation",
            true,
            json!({"missing_numbers": report.missing_numbers}),
        ));
    }
    let number = report.latest_number().unwrap_or(0) + 1;
    let doc_path = history_dir.join(format!("{number}.md"));
    let relative_path = relative_display(ctx.workspace.root(), &doc_path);
    let content = markdown::render_document(number, "开发会话", session_key, timestamp, None);
    Ok((
        model::HistoryDocument {
            number,
            path: relative_path,
            content,
            session_key: Some(session_key.to_string()),
            created_at: Some(timestamp.to_string()),
            updated_at: Some(timestamp.to_string()),
        },
        true,
    ))
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
    let _lock = storage::lock_directory(&history_dir)?;
    let mut report = storage::scan(&ctx.workspace, &history_dir)?;
    reject_ambiguous_history(&report)?;

    let timestamp = now_timestamp();
    let (document, lazily_created) =
        resolve_checkpoint_document(ctx, &history_dir, &report, &session_key, &timestamp)?;

    let expected_path = if lazily_created {
        document.path.clone()
    } else {
        args.get("expected_path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(document.path.as_str())
            .to_string()
    };
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
    if updated || lazily_created {
        storage::write_markdown(
            &history_dir.join(format!("{}.md", document.number)),
            &final_content,
        )?;
        storage::upsert_document(
            &mut report,
            model::HistoryDocument {
                number: document.number,
                path: document.path.clone(),
                content: final_content.clone(),
                session_key: document.session_key.clone(),
                created_at: document.created_at.clone(),
                updated_at: Some(record.timestamp.clone()),
            },
        );
    }

    let state_revision = storage::read_state(&history_dir)
        .ok()
        .flatten()
        .map(|state| state.state_revision + 1)
        .unwrap_or(1);
    let (manifest, state) =
        rebuild_and_persist_derived_files(&history_dir, &report, document.number, state_revision)?;

    let mut warnings: Vec<String> = Vec::new();
    if !user_input_captured {
        warnings
            .push("未提供 raw_user_input；服务端无法读取未作为工具参数传入的本轮用户输入。".into());
    }
    if redacted {
        warnings.push("检测到疑似敏感信息，归档内容已脱敏。".into());
    }
    if ctx.tool_profile.is_compact() {
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
