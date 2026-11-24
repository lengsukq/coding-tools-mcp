use std::collections::HashSet;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::model::{CachedUpstreamTool, UpstreamServerConfig};

pub const MAX_ALIAS_LEN: usize = 24;
pub const MAX_EXPOSED_TOOL_NAME_LEN: usize = 64;

const RESERVED_ALIASES: &[&str] = &[
    "workspace",
    "device",
    "computer",
    "mcp_upstream",
    "local",
    "self",
    "system",
    "coding_tools",
    "coding-tools",
];

pub fn is_reserved_alias(alias: &str) -> bool {
    let lower = alias.trim().to_ascii_lowercase();
    RESERVED_ALIASES.iter().any(|reserved| *reserved == lower)
}

/// Validate and normalize a user-provided server alias to `[a-z0-9_-]{1,24}`.
pub fn normalize_alias(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        return Err("上游 MCP 别名（alias）不能为空".into());
    }
    let normalized: String = trimmed
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let normalized = normalized.trim_matches(['_', '-']).to_string();
    if normalized.is_empty() {
        return Err(format!("别名 `{raw}` 不包含有效的字母或数字"));
    }
    if normalized.len() > MAX_ALIAS_LEN {
        return Err(format!(
            "别名 `{normalized}` 超过最大长度 {MAX_ALIAS_LEN} 字符"
        ));
    }
    if !normalized
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphanumeric())
    {
        return Err(format!("别名 `{normalized}` 必须以字母或数字开头"));
    }
    if normalized.contains("__") {
        return Err(format!("别名 `{normalized}` 不能包含连续双下划线 `__`"));
    }
    if is_reserved_alias(&normalized) {
        return Err(format!(
            "别名 `{normalized}` 为系统保留前缀，请换用其他名称"
        ));
    }
    Ok(normalized)
}

/// Derive a clean, unique alias from an arbitrary display/config name (used during import).
pub fn suggest_alias(raw: &str, existing: &HashSet<String>) -> String {
    let mut base: String = raw
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    while base.contains("__") {
        base = base.replace("__", "_");
    }
    base = base.trim_matches(['_', '-']).to_string();
    if base.is_empty()
        || !base
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
    {
        base = "mcp".to_string();
    }
    if base.len() > MAX_ALIAS_LEN {
        base.truncate(MAX_ALIAS_LEN);
        base = base.trim_end_matches(['_', '-']).to_string();
    }
    if is_reserved_alias(&base) {
        base = format!("{base}_srv");
        if base.len() > MAX_ALIAS_LEN {
            base.truncate(MAX_ALIAS_LEN);
        }
    }
    if !existing.contains(&base) {
        return base;
    }
    for idx in 2..=999 {
        let suffix = format!("_{idx}");
        let max_prefix = MAX_ALIAS_LEN.saturating_sub(suffix.len());
        let prefix = base
            .chars()
            .take(max_prefix)
            .collect::<String>()
            .trim_end_matches(['_', '-'])
            .to_string();
        let candidate = format!("{prefix}{suffix}");
        if !existing.contains(&candidate) && !is_reserved_alias(&candidate) {
            return candidate;
        }
    }
    let hash = short_hash(raw);
    format!("mcp_{hash}")
}

/// Sanitize a single tool name segment into `[a-zA-Z0-9_-]`.
pub fn sanitize_tool_segment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "tool".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Build a collision-free exposed tool name matching `^[a-zA-Z0-9_-]{1,64}$`.
///
/// The returned name is inserted into `occupied` by the caller (or checked here).
pub fn compute_exposed_tool_name(
    alias: &str,
    original_tool: &str,
    occupied: &HashSet<String>,
) -> String {
    let sanitized_tool = sanitize_tool_segment(original_tool);
    let direct = format!("{alias}__{sanitized_tool}");
    if direct.len() <= MAX_EXPOSED_TOOL_NAME_LEN && !occupied.contains(&direct) {
        return direct;
    }

    let digest = short_hash(&format!("{alias}\0{original_tool}"));
    let base_candidate = truncated_with_hash(alias, &sanitized_tool, &digest);
    if !occupied.contains(&base_candidate) {
        return base_candidate;
    }

    for counter in 2..=999 {
        let salted = short_hash(&format!("{alias}\0{original_tool}\0{counter}"));
        let candidate = truncated_with_hash(alias, &sanitized_tool, &salted);
        if !occupied.contains(&candidate) {
            return candidate;
        }
    }
    format!(
        "u_{}",
        short_hash(&format!("{alias}:{original_tool}:fallback"))
    )
}

fn truncated_with_hash(alias: &str, sanitized_tool: &str, hash6: &str) -> String {
    // Format: `{alias}__{prefix}_{hash6}` with total len <= 64
    let suffix = format!("_{hash6}");
    let prefix_fixed = format!("{alias}__");
    let budget = MAX_EXPOSED_TOOL_NAME_LEN
        .saturating_sub(prefix_fixed.len())
        .saturating_sub(suffix.len());
    let kept: String = sanitized_tool
        .chars()
        .take(budget)
        .collect::<String>()
        .trim_end_matches(['_', '-'])
        .to_string();
    if kept.is_empty() {
        format!("{prefix_fixed}t{suffix}")
    } else {
        format!("{prefix_fixed}{kept}{suffix}")
    }
}

fn short_hash(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    format!("{:02x}{:02x}{:02x}", digest[0], digest[1], digest[2])
}

/// Ensure an upstream tool's `inputSchema` is a valid JSON Schema object accepted
/// by strict clients (OpenAI, Claude, Gemini) without dropping valid upstream fields.
pub fn normalize_upstream_tool_schema(raw_schema: &Value) -> Value {
    let mut obj = match raw_schema.as_object() {
        Some(map) => map.clone(),
        None => serde_json::Map::new(),
    };
    if obj.get("type").and_then(Value::as_str).is_none() {
        obj.insert("type".into(), Value::String("object".into()));
    }
    if obj.get("type").and_then(Value::as_str) == Some("object")
        && !obj.get("properties").is_some_and(Value::is_object)
    {
        obj.insert("properties".into(), json!({}));
    }
    Value::Object(obj)
}

/// Construct the MCP `tools/list` entry for a `direct`-mode upstream tool.
pub fn build_exposed_tool_definition(
    exposed_name: &str,
    server: &UpstreamServerConfig,
    tool: &CachedUpstreamTool,
) -> Value {
    let desc = if tool.description.trim().is_empty() {
        format!("[MCP: {}] {}", server.name, tool.name)
    } else {
        format!("[MCP: {}] {}", server.name, tool.description.trim())
    };
    let mut map = serde_json::Map::new();
    map.insert("name".into(), Value::String(exposed_name.to_string()));
    if let Some(title) = tool.title.as_deref().filter(|t| !t.trim().is_empty()) {
        map.insert(
            "title".into(),
            Value::String(format!("{} · {}", server.name, title.trim())),
        );
    }
    map.insert("description".into(), Value::String(desc));
    map.insert("inputSchema".into(), tool.input_schema.clone());
    if let Some(annotations) = &tool.annotations {
        map.insert("annotations".into(), annotations.clone());
    }
    Value::Object(map)
}

/// Collect the reserved set of all built-in Global MCP tool names so an upstream
/// direct tool can never shadow any built-in tool.
pub fn reserved_builtin_tool_names() -> HashSet<String> {
    let mut set: HashSet<String> = crate::tools::registry::P0_TOOLS
        .iter()
        .map(|t| t.name.to_string())
        .collect();
    for name in [
        "device_list",
        "workspace_list",
        "workspace_current",
        "workspace_select",
        "workspace_invoke",
        "computer_session",
        "computer_observe",
        "computer_action",
        "mcp_upstream_list",
        "mcp_upstream_tools",
        "mcp_upstream_call",
    ] {
        set.insert(name.to_string());
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_normalization_enforces_rules_and_reserved_names() {
        assert_eq!(normalize_alias("GitHub").unwrap(), "github");
        assert_eq!(normalize_alias("my-db_01").unwrap(), "my-db_01");
        assert!(normalize_alias("").is_err());
        assert!(normalize_alias("workspace").is_err());
        assert!(normalize_alias("mcp_upstream").is_err());
        assert!(normalize_alias("a__b").is_err());
        assert!(normalize_alias("a".repeat(25).as_str()).is_err());
    }

    #[test]
    fn exposed_tool_name_respects_64_char_limit_and_avoids_collisions() {
        let mut occupied = reserved_builtin_tool_names();
        let short = compute_exposed_tool_name("github", "create_issue", &occupied);
        assert_eq!(short, "github__create_issue");
        occupied.insert(short);

        let dotted = compute_exposed_tool_name("github", "pull.requests/create", &occupied);
        assert_eq!(dotted, "github__pull_requests_create");
        assert!(dotted
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));

        let very_long =
            "a_very_long_upstream_tool_name_that_definitely_exceeds_sixty_four_characters_total";
        let truncated = compute_exposed_tool_name("my_upstream_server", very_long, &occupied);
        assert!(truncated.len() <= MAX_EXPOSED_TOOL_NAME_LEN);
        assert!(truncated.starts_with("my_upstream_server__"));
        occupied.insert(truncated.clone());

        // Same sanitized collision with different original name gets a distinct hash
        let distinct =
            compute_exposed_tool_name("my_upstream_server", &format!("{very_long}_v2"), &occupied);
        assert!(distinct.len() <= MAX_EXPOSED_TOOL_NAME_LEN);
        assert_ne!(truncated, distinct);
    }

    #[test]
    fn normalize_schema_fills_missing_type_and_properties() {
        let normalized = normalize_upstream_tool_schema(&Value::Null);
        assert_eq!(normalized["type"], "object");
        assert!(normalized["properties"].is_object());

        let partial = json!({ "required": ["id"] });
        let normalized2 = normalize_upstream_tool_schema(&partial);
        assert_eq!(normalized2["type"], "object");
        assert!(normalized2["properties"].is_object());
        assert_eq!(normalized2["required"][0], "id");
    }

    #[test]
    fn suggest_alias_deduplicates_and_avoids_reserved() {
        let mut existing = HashSet::new();
        assert_eq!(suggest_alias("Workspace", &existing), "workspace_srv");
        existing.insert("github".into());
        assert_eq!(suggest_alias("GitHub", &existing), "github_2");
    }
}
