use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::model::{
    detect_self_loop, is_sensitive_key, validate_http_url, SelfLoopContext, UpstreamExposeMode,
    UpstreamSecretPatch, UpstreamServerConfig, UpstreamTransportConfig, DEFAULT_CALL_TIMEOUT_SECS,
    DEFAULT_IDLE_TIMEOUT_SECS,
};
use super::naming::{normalize_alias, suggest_alias};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedUpstreamCandidate {
    /// Deterministic candidate key for UI checkbox binding.
    pub candidate_id: String,
    /// Client id: "json" | "claude_desktop" | "claude_code" | "cursor" | "antigravity" | "gemini_cli" | "codex" | "opencode"
    pub source_client: String,
    pub source_label: String,
    pub source_path: String,
    pub original_key: String,
    pub suggested_alias: String,
    pub name: String,
    pub transport_kind: String,
    pub summary_target: String,
    pub config: UpstreamServerConfig,
    /// Write-only secret keys/values extracted during scan/parse. Never logged; only used if user confirms import.
    #[serde(skip_serializing)]
    pub secret_patch: UpstreamSecretPatch,
    pub secret_env_keys: Vec<String>,
    pub secret_header_keys: Vec<String>,
    pub has_bearer_token: bool,
    pub already_exists_alias: bool,
    pub blocked_reason: Option<String>,
}

/// DTO sent back by the UI when confirming which scanned/parsed candidates to import.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSelectionItem {
    pub candidate_id: String,
    #[serde(default)]
    pub alias_override: Option<String>,
    #[serde(default)]
    pub expose_mode: Option<UpstreamExposeMode>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// Parse standard `mcpServers` JSON (and VS Code `servers` / OpenCode `mcp` / direct map)
/// into normalized candidates with automatic secret classification and self-loop checks.
pub fn parse_mcp_servers_json(
    raw: &str,
    source_client: &str,
    source_label: &str,
    source_path: &str,
    existing_aliases: &HashSet<String>,
    self_loop: &SelfLoopContext,
) -> Result<Vec<ImportedUpstreamCandidate>, String> {
    let parsed: Value =
        serde_json::from_str(raw.trim()).map_err(|err| format!("JSON 解析失败: {err}"))?;
    let Some(root_obj) = parsed.as_object() else {
        return Err("JSON 顶层必须是对象（例如 {\"mcpServers\": { ... }}）".into());
    };

    let servers_map = root_obj
        .get("mcpServers")
        .and_then(Value::as_object)
        .or_else(|| root_obj.get("servers").and_then(Value::as_object))
        .or_else(|| root_obj.get("mcp").and_then(Value::as_object))
        .unwrap_or(root_obj);

    let mut used_aliases = existing_aliases.clone();
    let mut out = Vec::new();

    for (raw_key, entry) in servers_map {
        let Some(obj) = entry.as_object() else {
            continue;
        };
        let Some((transport, secret_patch)) = parse_json_server_entry(obj) else {
            continue;
        };
        let candidate = build_candidate(
            source_client,
            source_label,
            source_path,
            raw_key,
            transport,
            secret_patch,
            existing_aliases,
            &mut used_aliases,
            self_loop,
        );
        out.push(candidate);
    }

    if out.is_empty() {
        return Err("未在 JSON 中找到有效的 MCP 服务配置（需包含 command 或 url 字段）".into());
    }
    Ok(out)
}

/// Scan well-known local AI client configuration files (read-only) for MCP servers.
pub fn scan_local_mcp_clients(
    existing_aliases: &HashSet<String>,
    self_loop: &SelfLoopContext,
) -> Vec<ImportedUpstreamCandidate> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    scan_local_mcp_clients_at(&home, existing_aliases, self_loop)
}

pub fn scan_local_mcp_clients_at(
    home: &Path,
    existing_aliases: &HashSet<String>,
    self_loop: &SelfLoopContext,
) -> Vec<ImportedUpstreamCandidate> {
    let mut all = Vec::new();
    let mut used_aliases = existing_aliases.clone();

    let json_sources = local_json_client_sources(home);
    for (client_id, label, path) in json_sources {
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let display_path = display_home_path(home, &path);
        if let Ok(mut items) = parse_mcp_servers_json(
            &raw,
            client_id,
            label,
            &display_path,
            existing_aliases,
            self_loop,
        ) {
            for item in &mut items {
                // Re-deduplicate suggested_alias across multiple discovered clients.
                if used_aliases.contains(&item.suggested_alias) {
                    item.suggested_alias = suggest_alias(&item.original_key, &used_aliases);
                    item.config.alias = item.suggested_alias.clone();
                }
                used_aliases.insert(item.suggested_alias.clone());
            }
            all.extend(items);
        }
    }

    // Scan Codex TOML (`~/.codex/config.toml`)
    let codex_toml = home.join(".codex").join("config.toml");
    if let Ok(raw) = fs::read_to_string(&codex_toml) {
        let display_path = display_home_path(home, &codex_toml);
        let items = parse_codex_toml_servers(
            &raw,
            &display_path,
            existing_aliases,
            &mut used_aliases,
            self_loop,
        );
        all.extend(items);
    }

    all
}

pub(crate) fn local_json_client_sources(home: &Path) -> Vec<(&'static str, &'static str, PathBuf)> {
    let mut sources = vec![
        ("claude_code", "Claude Code", home.join(".claude.json")),
        ("cursor", "Cursor", home.join(".cursor/mcp.json")),
        (
            "antigravity",
            "Antigravity",
            home.join(".gemini/antigravity/mcp_config.json"),
        ),
        (
            "gemini_cli",
            "Gemini CLI",
            home.join(".gemini/settings.json"),
        ),
        (
            "opencode",
            "OpenCode",
            home.join(".config/opencode/opencode.json"),
        ),
    ];

    #[cfg(target_os = "macos")]
    sources.insert(
        0,
        (
            "claude_desktop",
            "Claude Desktop",
            home.join("Library/Application Support/Claude/claude_desktop_config.json"),
        ),
    );
    #[cfg(target_os = "windows")]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        sources.insert(
            0,
            (
                "claude_desktop",
                "Claude Desktop",
                PathBuf::from(appdata)
                    .join("Claude")
                    .join("claude_desktop_config.json"),
            ),
        );
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    sources.insert(
        0,
        (
            "claude_desktop",
            "Claude Desktop",
            home.join(".config/Claude/claude_desktop_config.json"),
        ),
    );

    sources
}

fn parse_json_server_entry(
    obj: &serde_json::Map<String, Value>,
) -> Option<(UpstreamTransportConfig, UpstreamSecretPatch)> {
    // 1. OpenCode format: { "type": "local", "command": ["npx", "-y", "..."], "environment": { ... } }
    if let Some(cmd_arr) = obj.get("command").and_then(Value::as_array) {
        let parts: Vec<String> = cmd_arr
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        let command = parts.first()?.trim().to_string();
        if command.is_empty() {
            return None;
        }
        let args = parts[1..].to_vec();
        let cwd = obj
            .get("cwd")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string);
        let raw_env = obj
            .get("environment")
            .or_else(|| obj.get("env"))
            .and_then(Value::as_object);
        let (env, secret_env_keys, set_env_secrets) = split_sensitive_kv(raw_env);
        return Some((
            UpstreamTransportConfig::Stdio {
                command,
                args,
                cwd,
                env,
                secret_env_keys,
            },
            UpstreamSecretPatch {
                set_env_secrets,
                ..UpstreamSecretPatch::default()
            },
        ));
    }

    // 2. Standard Stdio: { "command": "npx", "args": [...], "env": {...}, "cwd": "..." }
    if let Some(command) = obj
        .get("command")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        let args = obj
            .get("args")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let cwd = obj
            .get("cwd")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string);
        let raw_env = obj
            .get("env")
            .or_else(|| obj.get("environment"))
            .and_then(Value::as_object);
        let (env, secret_env_keys, set_env_secrets) = split_sensitive_kv(raw_env);
        return Some((
            UpstreamTransportConfig::Stdio {
                command: command.to_string(),
                args,
                cwd,
                env,
                secret_env_keys,
            },
            UpstreamSecretPatch {
                set_env_secrets,
                ..UpstreamSecretPatch::default()
            },
        ));
    }

    // 3. HTTP / SSE remote server
    let raw_url = obj
        .get("url")
        .or_else(|| obj.get("serverUrl"))
        .or_else(|| obj.get("httpUrl"))
        .or_else(|| obj.get("baseUrl"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())?;

    let valid_url = validate_http_url(raw_url).ok()?;
    let explicit_type = obj
        .get("type")
        .or_else(|| obj.get("transport"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let is_sse = explicit_type == "sse"
        || (explicit_type.is_empty()
            && reqwest::Url::parse(&valid_url)
                .ok()
                .is_some_and(|u| u.path().trim_end_matches('/').ends_with("/sse")));

    let raw_headers = obj.get("headers").and_then(Value::as_object);
    let SplitHeaders {
        plain: headers,
        secret_keys: secret_header_keys,
        mut secret_values,
        bearer: mut bearer_token,
    } = split_sensitive_headers(raw_headers);

    if bearer_token.is_none() {
        if let Some(tok) = obj
            .get("bearerToken")
            .or_else(|| obj.get("token"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            bearer_token = Some(tok.to_string());
        }
    }
    let has_bearer_token = bearer_token.is_some();
    if has_bearer_token {
        secret_values.remove("Authorization");
        secret_values.remove("authorization");
    }

    let transport = if is_sse {
        UpstreamTransportConfig::Sse {
            url: valid_url,
            headers,
            secret_header_keys,
            has_bearer_token,
            use_proxy: false,
        }
    } else {
        UpstreamTransportConfig::StreamableHttp {
            url: valid_url,
            headers,
            secret_header_keys,
            has_bearer_token,
            use_proxy: false,
        }
    };
    let patch = UpstreamSecretPatch {
        set_header_secrets: secret_values,
        bearer_token: bearer_token.map(Some),
        ..UpstreamSecretPatch::default()
    };
    Some((transport, patch))
}

fn split_sensitive_kv(
    raw: Option<&serde_json::Map<String, Value>>,
) -> (
    BTreeMap<String, String>,
    Vec<String>,
    BTreeMap<String, String>,
) {
    let mut plain = BTreeMap::new();
    let mut secret_keys = Vec::new();
    let mut secret_values = BTreeMap::new();
    let Some(map) = raw else {
        return (plain, secret_keys, secret_values);
    };
    for (k, v) in map {
        let key = k.trim();
        if key.is_empty() {
            continue;
        }
        let val = match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => continue,
        };
        if is_sensitive_key(key) {
            secret_keys.push(key.to_string());
            secret_values.insert(key.to_string(), val);
        } else {
            plain.insert(key.to_string(), val);
        }
    }
    secret_keys.sort();
    (plain, secret_keys, secret_values)
}

struct SplitHeaders {
    plain: BTreeMap<String, String>,
    secret_keys: Vec<String>,
    secret_values: BTreeMap<String, String>,
    bearer: Option<String>,
}

fn split_sensitive_headers(raw: Option<&serde_json::Map<String, Value>>) -> SplitHeaders {
    let mut plain = BTreeMap::new();
    let mut secret_keys = Vec::new();
    let mut secret_values = BTreeMap::new();
    let mut bearer = None;
    let Some(map) = raw else {
        return SplitHeaders {
            plain,
            secret_keys,
            secret_values,
            bearer,
        };
    };
    for (k, v) in map {
        let key = k.trim();
        let Some(val) = v.as_str().map(str::trim) else {
            continue;
        };
        if key.is_empty() || val.is_empty() {
            continue;
        }
        if key.eq_ignore_ascii_case("authorization") {
            if let Some(tok) = val
                .strip_prefix("Bearer ")
                .or_else(|| val.strip_prefix("bearer "))
                .map(str::trim)
                .filter(|t| !t.is_empty())
            {
                bearer = Some(tok.to_string());
                continue;
            }
        }
        if is_sensitive_key(key) {
            secret_keys.push(key.to_string());
            secret_values.insert(key.to_string(), val.to_string());
        } else {
            plain.insert(key.to_string(), val.to_string());
        }
    }
    secret_keys.sort();
    SplitHeaders {
        plain,
        secret_keys,
        secret_values,
        bearer,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_candidate(
    source_client: &str,
    source_label: &str,
    source_path: &str,
    raw_key: &str,
    transport: UpstreamTransportConfig,
    secret_patch: UpstreamSecretPatch,
    existing_aliases: &HashSet<String>,
    used_aliases: &mut HashSet<String>,
    self_loop: &SelfLoopContext,
) -> ImportedUpstreamCandidate {
    let blocked_reason = detect_self_loop(&transport, self_loop);
    let already_exists_alias = normalize_alias(raw_key)
        .ok()
        .is_some_and(|a| existing_aliases.contains(&a));
    let suggested_alias = suggest_alias(raw_key, used_aliases);
    used_aliases.insert(suggested_alias.clone());

    let (secret_env_keys, secret_header_keys, has_bearer_token) = match &transport {
        UpstreamTransportConfig::Stdio {
            secret_env_keys, ..
        } => (secret_env_keys.clone(), Vec::new(), false),
        UpstreamTransportConfig::StreamableHttp {
            secret_header_keys,
            has_bearer_token,
            ..
        }
        | UpstreamTransportConfig::Sse {
            secret_header_keys,
            has_bearer_token,
            ..
        } => (Vec::new(), secret_header_keys.clone(), *has_bearer_token),
    };

    let transport_kind = transport.kind_label().to_string();
    let summary_target = transport.summary_target();
    let config = UpstreamServerConfig {
        id: format!("up-{}", &uuid::Uuid::new_v4().to_string()[..12]),
        alias: suggested_alias.clone(),
        name: raw_key.trim().to_string(),
        description: String::new(),
        enabled: true,
        expose_mode: UpstreamExposeMode::Direct,
        transport,
        disabled_tools: Vec::new(),
        call_timeout_secs: DEFAULT_CALL_TIMEOUT_SECS,
        idle_timeout_secs: DEFAULT_IDLE_TIMEOUT_SECS,
        source: source_client.to_string(),
    };

    ImportedUpstreamCandidate {
        candidate_id: format!("{source_client}:{raw_key}"),
        source_client: source_client.to_string(),
        source_label: source_label.to_string(),
        source_path: source_path.to_string(),
        original_key: raw_key.to_string(),
        suggested_alias,
        name: raw_key.trim().to_string(),
        transport_kind,
        summary_target,
        config,
        secret_patch,
        secret_env_keys,
        secret_header_keys,
        has_bearer_token,
        already_exists_alias,
        blocked_reason,
    }
}

/// Lightweight parser for Codex `[mcp_servers.<name>]` and `[mcp_servers.<name>.env]` TOML tables
/// without pulling an external `toml` crate dependency.
fn parse_codex_toml_servers(
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
            "codex",
            "Codex",
            source_path,
            &name,
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
    inner
        .split(',')
        .filter_map(|part| parse_toml_string(part.trim()))
        .collect()
}

pub(crate) fn display_home_path(home: &Path, path: &Path) -> String {
    path.strip_prefix(home)
        .map(|rel| format!("~/{}", rel.to_string_lossy().replace('\\', "/")))
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_mcp_servers_json_and_separates_secrets_and_self_loop() {
        let raw = r#"{
            "mcpServers": {
                "github": {
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-github"],
                    "env": {
                        "GITHUB_PERSONAL_ACCESS_TOKEN": "ghp_secret123",
                        "LOG_LEVEL": "info"
                    }
                },
                "remote_docs": {
                    "url": "https://docs.example.com/mcp",
                    "headers": {
                        "Authorization": "Bearer tok_xyz",
                        "X-Custom-Tenant": "acme",
                        "X-Api-Key": "k_999"
                    }
                },
                "legacy_sse": {
                    "url": "https://legacy.example.com/sse",
                    "type": "sse"
                },
                "self_loop": {
                    "url": "http://127.0.0.1:28765/mcp"
                }
            }
        }"#;
        let existing = HashSet::new();
        let loop_ctx = SelfLoopContext {
            local_port: 28765,
            public_url: "https://my.trycloudflare.com".into(),
        };
        let items = parse_mcp_servers_json(raw, "json", "JSON", "", &existing, &loop_ctx)
            .expect("parse should succeed");
        assert_eq!(items.len(), 4);

        let gh = items.iter().find(|i| i.original_key == "github").unwrap();
        assert_eq!(gh.transport_kind, "stdio");
        assert_eq!(gh.secret_env_keys, vec!["GITHUB_PERSONAL_ACCESS_TOKEN"]);
        assert_eq!(
            gh.secret_patch
                .set_env_secrets
                .get("GITHUB_PERSONAL_ACCESS_TOKEN")
                .map(String::as_str),
            Some("ghp_secret123")
        );
        assert!(gh.blocked_reason.is_none());

        let docs = items
            .iter()
            .find(|i| i.original_key == "remote_docs")
            .unwrap();
        assert_eq!(docs.transport_kind, "streamable_http");
        assert!(docs.has_bearer_token);
        assert_eq!(docs.secret_header_keys, vec!["X-Api-Key"]);
        assert_eq!(
            docs.secret_patch.bearer_token,
            Some(Some("tok_xyz".to_string()))
        );

        let sse = items
            .iter()
            .find(|i| i.original_key == "legacy_sse")
            .unwrap();
        assert_eq!(sse.transport_kind, "sse");

        let self_item = items
            .iter()
            .find(|i| i.original_key == "self_loop")
            .unwrap();
        assert!(self_item.blocked_reason.is_some());
    }

    #[test]
    fn scans_codex_toml_and_cursor_json_from_temp_home() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(temp.path().join(".cursor")).unwrap();
        fs::write(
            temp.path().join(".cursor/mcp.json"),
            r#"{"mcpServers":{"context7":{"command":"npx","args":["-y","@upstash/context7-mcp"]}}}"#,
        )
        .unwrap();
        fs::create_dir_all(temp.path().join(".codex")).unwrap();
        fs::write(
            temp.path().join(".codex/config.toml"),
            r#"
[mcp_servers.playwright]
command = "npx"
args = ["@playwright/mcp@latest"]

[mcp_servers.playwright.env]
API_TOKEN = "secret-val"
HEADLESS = "true"
"#,
        )
        .unwrap();

        let scanned = scan_local_mcp_clients_at(
            temp.path(),
            &HashSet::new(),
            &SelfLoopContext {
                local_port: 28765,
                public_url: String::new(),
            },
        );
        assert_eq!(scanned.len(), 2);
        assert!(scanned
            .iter()
            .any(|c| c.source_client == "cursor" && c.original_key == "context7"));
        let pw = scanned
            .iter()
            .find(|c| c.source_client == "codex" && c.original_key == "playwright")
            .unwrap();
        assert_eq!(pw.secret_env_keys, vec!["API_TOKEN"]);
    }
}
