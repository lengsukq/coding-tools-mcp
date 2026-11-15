use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::DAY_MS;

pub(super) fn classify_outcome(response: &Value) -> (String, Option<String>) {
    if let Some(error) = response.get("error") {
        let data = error.get("data");
        let category = if data
            .and_then(|value| value.get("stage"))
            .and_then(Value::as_str)
            == Some("rpc_worker")
        {
            "runtime"
        } else if data
            .and_then(|value| value.get("reason").or_else(|| value.get("code")))
            .and_then(Value::as_str)
            .is_some_and(|code| {
                let code = code.to_ascii_uppercase();
                code.contains("POLICY") || code.contains("PERMISSION") || code.contains("DENIED")
            })
        {
            "policy"
        } else {
            "protocol"
        };
        return ("error".into(), Some(category.into()));
    }

    let result = response.get("result");
    if result
        .and_then(|value| value.get("isError"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let raw = result
            .and_then(|value| value.get("structuredContent"))
            .and_then(|value| value.get("error"))
            .and_then(|value| value.get("category"))
            .and_then(Value::as_str)
            .unwrap_or("tool");
        return ("error".into(), Some(safe_error_category(raw)));
    }

    ("success".into(), None)
}

pub(super) fn safe_error_category(value: &str) -> String {
    match value {
        "policy" | "permission" | "security" => "policy",
        "validation" | "not_found" | "runtime" | "storage" | "filesystem" | "internal" => value,
        _ => "tool",
    }
    .to_string()
}

pub(super) fn safe_auth_method(value: &str) -> String {
    match value {
        "oauth" | "shared_bearer" => value,
        _ => "none",
    }
    .to_string()
}

pub(super) fn safe_label(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return "unknown".into();
    }
    let value: String = value
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '/')
        })
        .take(128)
        .collect();
    if value.is_empty() {
        "unknown".into()
    } else {
        value
    }
}

pub(super) fn bounded_identifier(value: &Value, max_chars: usize) -> Option<String> {
    if value.is_null() {
        return None;
    }
    Some(
        value
            .to_string()
            .chars()
            .filter(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | ':')
            })
            .take(max_chars)
            .collect(),
    )
}

pub(super) fn bounded_identifier_text(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | ':')
        })
        .take(max_chars)
        .collect()
}

pub(super) fn bounded_visible_text(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(max_chars)
        .collect()
}

pub(super) fn safe_change_id(value: &str) -> bool {
    (8..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

pub(super) fn workspace_prefix(workspace_id: &str) -> String {
    format!("w-{:x}-", Sha256::digest(workspace_id.as_bytes()))
}

pub(super) fn file_day_in_range(name: &str, from_ms: u64, to_ms: u64) -> bool {
    file_day(name).is_some_and(|day| {
        let from_day = from_ms / DAY_MS;
        let to_day = to_ms / DAY_MS;
        day >= from_day && day <= to_day
    })
}

pub(super) fn file_day(name: &str) -> Option<u64> {
    let stem = name.strip_suffix(".jsonl")?;
    stem.rsplit('-').next()?.parse().ok()
}

pub(super) fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or_default()
}
