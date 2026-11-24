use std::collections::{BTreeMap, HashSet};

use serde_json::Value;

use super::super::model::{
    is_sensitive_key, validate_http_url, SelfLoopContext, UpstreamSecretPatch,
    UpstreamTransportConfig,
};
use super::{build_candidate, CandidateSource, ImportedUpstreamCandidate};

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
            CandidateSource {
                client: source_client,
                label: source_label,
                path: source_path,
                raw_key,
            },
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

pub(super) fn parse_json_server_entry(
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
        return Some(build_stdio_transport(command, parts[1..].to_vec(), obj));
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
        return Some(build_stdio_transport(command.to_string(), args, obj));
    }

    // 3. HTTP / SSE remote server
    parse_remote_server_entry(obj)
}

fn build_stdio_transport(
    command: String,
    args: Vec<String>,
    obj: &serde_json::Map<String, Value>,
) -> (UpstreamTransportConfig, UpstreamSecretPatch) {
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
    (
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
    )
}

fn parse_remote_server_entry(
    obj: &serde_json::Map<String, Value>,
) -> Option<(UpstreamTransportConfig, UpstreamSecretPatch)> {
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
