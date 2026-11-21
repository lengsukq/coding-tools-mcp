use super::*;

pub fn list_sessions_for_workspace(workspace: &Workspace) -> WorkspaceResult<Value> {
    let history_dir = storage::resolve_history_dir(workspace, None, None)?;
    let report = storage::scan(workspace, &history_dir)?;
    let context_revision = storage::archive_revision(&report);
    let sessions = report
        .documents
        .iter()
        .map(|document| session_summary(document, true).0)
        .collect::<Vec<_>>();
    Ok(json!({
        "context_revision": context_revision,
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
    let mut selected = Vec::new();
    let mut selected_snippets = Vec::new();
    for number in &ctx.history_context_sessions {
        let Ok(index) = report
            .documents
            .binary_search_by_key(number, |document| document.number)
        else {
            continue;
        };
        let (summary, snippets) = session_summary(&report.documents[index], false);
        selected.push(summary);
        selected_snippets.extend(snippets);
    }
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
    let revision = storage::sha256(&serde_json::to_vec(&selected).map_err(|error| {
        WorkspaceError::invalid_argument(format!("history context is not serializable: {error}"))
    })?);
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

fn session_summary(
    document: &self::model::HistoryDocument,
    embed_snippets: bool,
) -> (Value, Vec<Value>) {
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
    let snippets = checkpoints
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
        .collect::<Vec<_>>();
    let (embedded_snippets, detached_snippets) = if embed_snippets {
        (snippets, Vec::new())
    } else {
        (Vec::new(), snippets)
    };
    (
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
            "snippets": embedded_snippets
        }),
        detached_snippets,
    )
}
