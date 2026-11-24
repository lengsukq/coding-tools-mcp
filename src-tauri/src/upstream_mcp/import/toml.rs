use std::collections::{BTreeMap, HashSet};

use serde_json::Value;

use super::super::model::SelfLoopContext;
use super::json::parse_json_server_entry;
use super::{build_candidate, CandidateSource, ImportedUpstreamCandidate};

/// Lightweight parser for Codex `[mcp_servers.<name>]` and `[mcp_servers.<name>.env]` TOML tables
/// without pulling an external `toml` crate dependency.
pub(super) fn parse_codex_toml_servers(
    raw: &str,
    source_path: &str,
    existing_aliases: &HashSet<String>,
    used_aliases: &mut HashSet<String>,
    self_loop: &SelfLoopContext,
) -> Vec<ImportedUpstreamCandidate> {
    #[derive(Default)]
    struct CodexEntry {
        command: Option<String>,
        args: Vec<String>,
        cwd: Option<String>,
        url: Option<String>,
        env: serde_json::Map<String, Value>,
        headers: serde_json::Map<String, Value>,
    }

    let mut entries: BTreeMap<String, CodexEntry> = BTreeMap::new();
    let mut active_server: Option<String> = None;
    let mut active_subtable: Option<&'static str> = None;

    for raw_line in raw.lines() {
        let line = strip_toml_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let header = line[1..line.len() - 1].trim();
            if let Some(rest) = header.strip_prefix("mcp_servers.") {
                let parts: Vec<&str> = rest
                    .split('.')
                    .map(|p| p.trim().trim_matches(['"', '\'']))
                    .filter(|p| !p.is_empty())
                    .collect();
                match parts.as_slice() {
                    [name] => {
                        active_server = Some((*name).to_string());
                        active_subtable = None;
                        entries.entry((*name).to_string()).or_default();
                    }
                    [name, "env"] => {
                        active_server = Some((*name).to_string());
                        active_subtable = Some("env");
                        entries.entry((*name).to_string()).or_default();
                    }
                    [name, "http_headers"] => {
                        active_server = Some((*name).to_string());
                        active_subtable = Some("headers");
                        entries.entry((*name).to_string()).or_default();
                    }
                    _ => {
                        active_server = None;
                        active_subtable = None;
                    }
                }
            } else {
                active_server = None;
                active_subtable = None;
            }
            continue;
        }

        let Some(server_name) = active_server.as_deref() else {
            continue;
        };
        let Some((raw_k, raw_v)) = line.split_once('=') else {
            continue;
        };
        let key = raw_k.trim().trim_matches(['"', '\'']);
        let val = raw_v.trim();
        let entry = entries.entry(server_name.to_string()).or_default();

        match active_subtable {
            Some("env") => {
                if let Some(s) = parse_toml_string(val) {
                    entry.env.insert(key.to_string(), Value::String(s));
                }
            }
            Some("headers") => {
                if let Some(s) = parse_toml_string(val) {
                    entry.headers.insert(key.to_string(), Value::String(s));
                }
            }
            _ => match key {
                "command" => entry.command = parse_toml_string(val),
                "cwd" => entry.cwd = parse_toml_string(val),
                "url" => entry.url = parse_toml_string(val),
                "args" => entry.args = parse_toml_string_array(val),
                _ => {}
            },
        }
    }

    let mut out = Vec::new();
    for (name, entry) in entries {
        let mut obj = serde_json::Map::new();
        if let Some(cmd) = entry.command {
            obj.insert("command".into(), Value::String(cmd));
            obj.insert(
                "args".into(),
                Value::Array(entry.args.into_iter().map(Value::String).collect()),
            );
            if let Some(cwd) = entry.cwd {
                obj.insert("cwd".into(), Value::String(cwd));
            }
            if !entry.env.is_empty() {
                obj.insert("env".into(), Value::Object(entry.env));
            }
        } else if let Some(url) = entry.url {
            obj.insert("url".into(), Value::String(url));
            if !entry.headers.is_empty() {
                obj.insert("headers".into(), Value::Object(entry.headers));
            }
        } else {
            continue;
        }
        let Some((transport, patch)) = parse_json_server_entry(&obj) else {
            continue;
        };
        out.push(build_candidate(
            CandidateSource {
                client: "codex",
                label: "Codex",
                path: source_path,
                raw_key: &name,
            },
            transport,
            patch,
            existing_aliases,
            used_aliases,
            self_loop,
        ));
    }
    out
}

pub(crate) fn strip_toml_comment(line: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    for (idx, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_double => escaped = true,
            '"' if !in_single => in_double = !in_double,
            '\'' if !in_double => in_single = !in_single,
            '#' if !in_single && !in_double => return &line[..idx],
            _ => {}
        }
    }
    line
}

fn parse_toml_string(val: &str) -> Option<String> {
    let trimmed = val.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        return serde_json::from_str::<String>(trimmed).ok();
    }
    if trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2 {
        return Some(trimmed[1..trimmed.len() - 1].to_string());
    }
    None
}

fn parse_toml_string_array(val: &str) -> Vec<String> {
    let trimmed = val.trim();
    if let Ok(arr) = serde_json::from_str::<Vec<String>>(trimmed) {
        return arr;
    }
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return Vec::new();
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    split_toml_array_elements(inner)
        .into_iter()
        .filter_map(parse_toml_string)
        .collect()
}

/// Split comma-separated TOML array items while honoring single- and double-quoted strings.
fn split_toml_array_elements(inner: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    for (idx, ch) in inner.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_double => escaped = true,
            '"' if !in_single => in_double = !in_double,
            '\'' if !in_double => in_single = !in_single,
            ',' if !in_single && !in_double => {
                parts.push(inner[start..idx].trim());
                start = idx + 1;
            }
            _ => {}
        }
    }
    let tail = inner[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }
    parts
}
