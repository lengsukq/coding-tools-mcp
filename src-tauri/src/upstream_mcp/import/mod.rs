use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::model::{
    detect_self_loop, SelfLoopContext, UpstreamExposeMode, UpstreamSecretPatch,
    UpstreamServerConfig, UpstreamTransportConfig, DEFAULT_CALL_TIMEOUT_SECS,
    DEFAULT_IDLE_TIMEOUT_SECS,
};
use super::naming::{normalize_alias, suggest_alias};

mod json;
mod toml;

pub use json::parse_mcp_servers_json;
pub(crate) use toml::strip_toml_comment;

use toml::parse_codex_toml_servers;

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

pub(super) struct CandidateSource<'a> {
    pub(super) client: &'a str,
    pub(super) label: &'a str,
    pub(super) path: &'a str,
    pub(super) raw_key: &'a str,
}

pub(super) fn build_candidate(
    source: CandidateSource<'_>,
    transport: UpstreamTransportConfig,
    secret_patch: UpstreamSecretPatch,
    existing_aliases: &HashSet<String>,
    used_aliases: &mut HashSet<String>,
    self_loop: &SelfLoopContext,
) -> ImportedUpstreamCandidate {
    let CandidateSource {
        client: source_client,
        label: source_label,
        path: source_path,
        raw_key,
    } = source;
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

    #[test]
    fn parses_toml_string_array_with_commas_inside_quotes() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(temp.path().join(".codex")).unwrap();
        fs::write(
            temp.path().join(".codex/config.toml"),
            r#"
[mcp_servers.tagged]
command = "my-mcp"
args = ['--tags', 'alpha,beta', "gamma,\"delta\",eps"]
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
        assert_eq!(scanned.len(), 1);
        match &scanned[0].config.transport {
            UpstreamTransportConfig::Stdio { args, .. } => {
                assert_eq!(args, &vec!["--tags", "alpha,beta", "gamma,\"delta\",eps"]);
            }
            other => panic!("expected stdio transport, got {other:?}"),
        }
    }
}
