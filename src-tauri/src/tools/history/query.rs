use super::*;

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
        || path
            .file_stem()
            .and_then(|value| value.to_str())
            .and_then(|value| value.parse::<u64>().ok())
            .is_none()
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
