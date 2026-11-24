use std::borrow::Cow;
use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

use crate::util::hash::sha256_hex;

use super::model::{CheckpointRecord, InitialInputRecord};

const CHECKPOINT_HEADING: &str = "## 本轮检查点";
const INITIAL_INPUT_HEADING: &str = "## 首次用户输入";

pub fn metadata(content: &str, label: &str) -> Option<String> {
    let prefix = format!("**{label}:**");
    content.lines().find_map(|line| {
        line.trim()
            .strip_prefix(&prefix)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

pub fn document_title(content: &str, number: u64) -> String {
    content
        .lines()
        .find_map(|line| line.trim().strip_prefix("# "))
        .and_then(|line| line.split_once('：').map(|(_, title)| title.trim()))
        .filter(|title| !title.is_empty())
        .unwrap_or("开发会话")
        .to_string()
        .replace(&format!("会话 {number}"), "")
        .trim_matches(['：', ':', ' '])
        .to_string()
}

pub fn render_document(
    number: u64,
    title: &str,
    session_key: &str,
    created_at: &str,
    initial_input: Option<&InitialInputRecord>,
) -> String {
    let title = if title.trim().is_empty() {
        "开发会话"
    } else {
        title.trim()
    };
    let mut output = format!(
        "# 会话 {number}：{title}\n\n\
**Session key:** {session_key}\n\
**Created:** {created_at}\n\
**Updated:** {created_at}\n\
**Status:** active\n\n\
{INITIAL_INPUT_HEADING}\n\n"
    );
    if let Some(input) = initial_input {
        push_json_block(&mut output, "initial-input revision-1", input);
    } else {
        output.push_str("未提供首次用户输入；服务端无法读取未作为工具参数传入的聊天内容。\n\n");
    }
    output.push_str(CHECKPOINT_HEADING);
    output.push_str("\n\n");
    output
}

pub fn parse_initial_input_records(content: &str) -> Vec<InitialInputRecord> {
    section_body(content, INITIAL_INPUT_HEADING)
        .map(parse_json_blocks)
        .unwrap_or_default()
}

pub fn append_initial_input_revision(content: &str, record: &InitialInputRecord) -> String {
    let mut updated = content.to_string();
    let block = json_block(
        &format!("initial-input revision-{}", record.revision),
        record,
    );
    if let Some(start) = updated.find(INITIAL_INPUT_HEADING) {
        let tail = &updated[start + INITIAL_INPUT_HEADING.len()..];
        let end = tail
            .find("\n## ")
            .map(|offset| start + INITIAL_INPUT_HEADING.len() + offset);
        let insert_at = end.unwrap_or(updated.len());
        updated.insert_str(insert_at, &format!("\n\n{block}"));
    } else if let Some(checkpoint_start) = updated.find(CHECKPOINT_HEADING) {
        updated.insert_str(
            checkpoint_start,
            &format!("{INITIAL_INPUT_HEADING}\n\n{block}\n\n"),
        );
    } else {
        updated.push_str(&format!("\n{INITIAL_INPUT_HEADING}\n\n{block}\n"));
    }
    updated
}

pub fn parse_checkpoint_records(content: &str) -> Vec<CheckpointRecord> {
    section_body(content, CHECKPOINT_HEADING)
        .map(parse_json_blocks)
        .unwrap_or_default()
}

pub fn append_checkpoint_record(content: &str, record: &CheckpointRecord) -> String {
    let mut updated = content.to_string();
    if !updated.contains(CHECKPOINT_HEADING) {
        if !updated.ends_with('\n') {
            updated.push('\n');
        }
        updated.push_str(&format!("\n{CHECKPOINT_HEADING}\n"));
    }
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    push_json_block(
        &mut updated,
        &format!("{} revision-{}", record.turn_id, record.revision),
        record,
    );
    updated
}

pub fn with_updated_at(content: &str, timestamp: &str) -> String {
    let prefix = "**Updated:**";
    let Some(start) = content.find(prefix) else {
        return content.to_string();
    };
    let tail = &content[start..];
    let Some(end) = tail.find('\n') else {
        return content.to_string();
    };
    let mut updated = content.to_string();
    updated.replace_range(start..start + end, &format!("{prefix} {timestamp}"));
    updated
}

fn section_body<'a>(content: &'a str, heading: &str) -> Option<&'a str> {
    let start = content.find(heading)? + heading.len();
    let tail = &content[start..];
    let end = tail.find("\n## ").unwrap_or(tail.len());
    Some(tail[..end].trim())
}

fn parse_json_blocks<T>(content: &str) -> Vec<T>
where
    T: serde::de::DeserializeOwned,
{
    let mut records = Vec::new();
    let mut remaining = content;
    while let Some(fence_start) = remaining.find("```json\n") {
        let json_start = fence_start + "```json\n".len();
        let Some(fence_end) = remaining[json_start..].find("\n```") else {
            break;
        };
        let json_text = &remaining[json_start..json_start + fence_end];
        if let Ok(record) = serde_json::from_str::<T>(json_text) {
            records.push(record);
        }
        remaining = &remaining[json_start + fence_end + "\n```".len()..];
    }
    records
}

fn json_block<T: serde::Serialize>(heading: &str, value: &T) -> String {
    format!(
        "### {heading}\n\n```json\n{}\n```\n",
        serde_json::to_string_pretty(value).expect("history record is serializable")
    )
}

fn push_json_block<T: serde::Serialize>(output: &mut String, heading: &str, value: &T) {
    output.push_str(&json_block(heading, value));
    output.push('\n');
}

pub fn checkpoint_from_args(
    args: &Value,
    default_timestamp: &str,
) -> Result<CheckpointRecord, String> {
    let explicit_turn_id = args
        .get("turn_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let explicit_timestamp = string_field(args, "timestamp");
    let mut record = CheckpointRecord {
        turn_id: explicit_turn_id.unwrap_or_default().to_string(),
        timestamp: explicit_timestamp.clone().unwrap_or_default(),
        user_intent: string_field(args, "user_intent").unwrap_or_default(),
        raw_user_input: string_field(args, "raw_user_input").unwrap_or_default(),
        findings: string_array(args, "findings")?,
        decisions: string_array(args, "decisions")?,
        files_changed: string_array(args, "files_changed")?,
        tests: string_array(args, "tests")?,
        runtime_state: string_array(args, "runtime_state")?,
        remaining_issues: string_array(args, "remaining_issues")?,
        next_actions: string_array(args, "next_actions")?,
        notes: string_field(args, "notes").unwrap_or_default(),
        ..CheckpointRecord::default()
    };
    if record.turn_id.is_empty() {
        record.turn_id = automatic_turn_id(&record);
    }
    record.timestamp = explicit_timestamp.unwrap_or_else(|| default_timestamp.to_string());
    Ok(record)
}

pub fn checkpoint_fingerprint(record: &CheckpointRecord) -> String {
    let mut canonical = record.clone();
    canonical.timestamp.clear();
    canonical.revision = 0;
    canonical.supersedes = None;
    canonical.content_hash.clear();
    let encoded = serde_json::to_vec(&canonical).expect("checkpoint record is serializable");
    sha256_hex(&encoded)
}

pub fn initial_input_fingerprint(value: &str) -> String {
    sha256_hex(value.as_bytes())
}

fn automatic_turn_id(record: &CheckpointRecord) -> String {
    let hash = checkpoint_fingerprint(record);
    format!("auto-{}", &hash[..16])
}

fn string_field(args: &Value, name: &str) -> Option<String> {
    args.get(name).and_then(Value::as_str).map(str::to_string)
}

fn string_array(args: &Value, name: &str) -> Result<Vec<String>, String> {
    let Some(value) = args.get(name) else {
        return Ok(Vec::new());
    };
    let array = value
        .as_array()
        .ok_or_else(|| format!("{name} must be an array of strings"))?;
    array
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("{name} must contain only strings"))
        })
        .collect()
}

pub fn redact_record(record: &mut CheckpointRecord) -> bool {
    let mut changed = redact_text(&mut record.timestamp);
    changed |= redact_text(&mut record.user_intent);
    changed |= redact_text(&mut record.raw_user_input);
    changed |= redact_text(&mut record.notes);
    for values in [
        &mut record.findings,
        &mut record.decisions,
        &mut record.files_changed,
        &mut record.tests,
        &mut record.runtime_state,
        &mut record.remaining_issues,
        &mut record.next_actions,
    ] {
        for value in values {
            changed |= redact_text(value);
        }
    }
    changed
}

pub fn redact_text(value: &mut String) -> bool {
    static PATTERNS: OnceLock<[Regex; 3]> = OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        [
            Regex::new(r"(?i)\b(bearer\s+)[A-Za-z0-9._~+/=-]{6,}").expect("bearer regex"),
            Regex::new(r"(?i)\b(api[_ -]?key|token|cookie|password|passwd|pwd)\s*[:=]\s*[^\s,;]+")
                .expect("secret assignment regex"),
            Regex::new(r"(?is)-----BEGIN[^\n]*PRIVATE KEY-----.*?-----END[^\n]*PRIVATE KEY-----")
                .expect("private key regex"),
        ]
    });

    // Keep borrowing `value` while patterns do not match so clean inputs incur zero allocations.
    let mut current = Cow::Borrowed(value.as_str());
    if let Cow::Owned(updated) = patterns[0].replace_all(&current, "${1}[REDACTED]") {
        current = Cow::Owned(updated);
    }
    if let Cow::Owned(updated) = patterns[1]
        .replace_all(&current, |captures: &regex::Captures<'_>| {
            format!("{}=[REDACTED]", &captures[1])
        })
    {
        current = Cow::Owned(updated);
    }
    if let Cow::Owned(updated) = patterns[2].replace_all(&current, "[REDACTED]") {
        current = Cow::Owned(updated);
    }
    if let Cow::Owned(updated) = current {
        if updated != *value {
            *value = updated;
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_text_preserves_clean_text_and_redacts_secrets() {
        let mut clean = String::from("refactor history query pagination and manifest lookup");
        assert!(!redact_text(&mut clean));
        assert_eq!(
            clean,
            "refactor history query pagination and manifest lookup"
        );

        let mut with_secret =
            String::from("Authorization: Bearer secret-token-12345; password=hunter2");
        assert!(redact_text(&mut with_secret));
        assert_eq!(
            with_secret,
            "Authorization: Bearer [REDACTED]; password=[REDACTED]"
        );
        // Second pass over already-redacted content reports no further change.
        assert!(!redact_text(&mut with_secret));
    }
}
