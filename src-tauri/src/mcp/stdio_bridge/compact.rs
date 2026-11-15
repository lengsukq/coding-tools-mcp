use serde_json::{json, Map, Value};

pub(super) fn adapt_gemini_response(method: Option<&str>, mut payload: Value) -> Value {
    match method {
        Some("tools/list") => {
            compact_tools_list_result(&mut payload);
            payload
        }
        Some("tools/call") if payload.get("error").is_some() => {
            wrap_tool_call_rpc_error_as_mcp_result(payload)
        }
        _ => payload,
    }
}

fn wrap_tool_call_rpc_error_as_mcp_result(payload: Value) -> Value {
    let Some(id) = payload.get("id").filter(|id| !id.is_null()).cloned() else {
        return payload;
    };
    let Some(error) = payload.get("error").and_then(Value::as_object) else {
        return payload;
    };

    let code = error.get("code").and_then(Value::as_i64);
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Tool call failed");
    let reason = error
        .get("data")
        .and_then(|d| d.get("reason"))
        .and_then(Value::as_str);

    let text = match (reason, code) {
        (Some("WORKSPACE_NOT_SELECTED"), _) => format!(
            "WORKSPACE_NOT_SELECTED: {message}\nNext step: call workspace_list and then workspace_select(workspace_id=...), or pass workspace_id directly in this tool's arguments."
        ),
        (Some(reason), _) => format!("{reason}: {message}"),
        (None, Some(code)) => format!("MCP error ({code}): {message}"),
        (None, None) => message.to_string(),
    };

    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "isError": true,
            "content": [{ "type": "text", "text": text }]
        }
    })
}

fn compact_tools_list_result(payload: &mut Value) {
    let Some(tools) = payload
        .get_mut("result")
        .and_then(|r| r.get_mut("tools"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };

    for tool in tools {
        compact_tool_entry(tool);
    }
}

fn compact_tool_entry(tool: &mut Value) {
    let Some(obj) = tool.as_object_mut() else {
        return;
    };
    obj.remove("title");

    if let Some(desc) = obj.get("description").and_then(Value::as_str) {
        let compact = truncate_compact_text(desc, 110);
        if compact.is_empty() {
            obj.remove("description");
        } else {
            obj.insert("description".into(), Value::String(compact));
        }
    }

    if let Some(schema) = obj.get_mut("inputSchema") {
        compact_schema_node(schema, 0);
    }
}

fn compact_schema_node(node: &mut Value, depth: usize) {
    let Some(obj) = node.as_object_mut() else {
        return;
    };

    for noisy_key in [
        "$schema",
        "title",
        "additionalProperties",
        "default",
        "minimum",
        "maximum",
        "minLength",
        "maxLength",
        "minItems",
        "maxItems",
    ] {
        obj.remove(noisy_key);
    }

    if depth > 0 {
        if let Some(desc) = obj.get("description").and_then(Value::as_str) {
            let compact = truncate_compact_text(desc, 48);
            if compact.is_empty() {
                obj.remove("description");
            } else {
                obj.insert("description".into(), Value::String(compact));
            }
        }
    } else {
        obj.remove("description");
    }

    if let Some(props) = obj.get_mut("properties").and_then(Value::as_object_mut) {
        compact_schema_properties(props, depth + 1);
    }

    if let Some(items) = obj.get_mut("items") {
        compact_schema_node(items, depth + 1);
    }

    for combinator in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = obj.get_mut(combinator).and_then(Value::as_array_mut) {
            for branch in branches {
                compact_schema_node(branch, depth + 1);
            }
        }
    }
}

fn compact_schema_properties(props: &mut Map<String, Value>, depth: usize) {
    for (key, prop) in props.iter_mut() {
        if key == "workspace_id" {
            *prop = json!({ "type": "string" });
            continue;
        }
        compact_schema_node(prop, depth);
    }
}

fn truncate_compact_text(text: &str, max_chars: usize) -> String {
    let normalized: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }

    let cutoff = max_chars.saturating_sub(1);
    let end_byte = normalized
        .char_indices()
        .nth(cutoff)
        .map(|(idx, _)| idx)
        .unwrap_or(normalized.len());
    let prefix = normalized[..end_byte].trim_end_matches(|c: char| c.is_whitespace() || c == ',');
    format!("{prefix}…")
}
