use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::platform::platform;

pub const UPSTREAM_ENV_SECRETS_SCOPE: &str = "upstream_mcp_env";
pub const UPSTREAM_HEADER_SECRETS_SCOPE: &str = "upstream_mcp_headers";
pub const UPSTREAM_BEARER_SECRETS_SCOPE: &str = "upstream_mcp_bearer";
pub const UPSTREAM_ORIGIN_HEADER: &str = "x-coding-tools-origin";

pub const DEFAULT_CALL_TIMEOUT_SECS: u64 = 60;
pub const MIN_CALL_TIMEOUT_SECS: u64 = 5;
pub const MAX_CALL_TIMEOUT_SECS: u64 = 600;

pub const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 900;
pub const MAX_IDLE_TIMEOUT_SECS: u64 = 86_400;

pub const DIRECT_TOOLS_WARNING_THRESHOLD: usize = 25;

fn default_true() -> bool {
    true
}

fn default_expose_mode() -> UpstreamExposeMode {
    UpstreamExposeMode::Direct
}

fn default_call_timeout_secs() -> u64 {
    DEFAULT_CALL_TIMEOUT_SECS
}

fn default_idle_timeout_secs() -> u64 {
    DEFAULT_IDLE_TIMEOUT_SECS
}

fn default_source() -> String {
    "manual".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamExposeMode {
    #[default]
    Direct,
    Proxy,
}

impl UpstreamExposeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Proxy => "proxy",
        }
    }

    pub fn parse_str(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "direct" => Ok(Self::Direct),
            "proxy" => Ok(Self::Proxy),
            other => Err(format!(
                "不支持的暴露模式 `{other}`（可选 direct 或 proxy）"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UpstreamTransportConfig {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        /// Non-sensitive environment variables persisted directly.
        #[serde(default)]
        env: BTreeMap<String, String>,
        /// Keys of sensitive env vars stored in `app_secrets[UPSTREAM_ENV_SECRETS_SCOPE]`.
        #[serde(default, rename = "secretEnvKeys", alias = "secret_env_keys")]
        secret_env_keys: Vec<String>,
    },
    StreamableHttp {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
        /// Keys of sensitive HTTP headers stored in `app_secrets[UPSTREAM_HEADER_SECRETS_SCOPE]`.
        #[serde(default, rename = "secretHeaderKeys", alias = "secret_header_keys")]
        secret_header_keys: Vec<String>,
        #[serde(default, rename = "hasBearerToken", alias = "has_bearer_token")]
        has_bearer_token: bool,
        #[serde(default, rename = "useProxy", alias = "use_proxy")]
        use_proxy: bool,
    },
    Sse {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
        /// Keys of sensitive HTTP headers stored in `app_secrets[UPSTREAM_HEADER_SECRETS_SCOPE]`.
        #[serde(default, rename = "secretHeaderKeys", alias = "secret_header_keys")]
        secret_header_keys: Vec<String>,
        #[serde(default, rename = "hasBearerToken", alias = "has_bearer_token")]
        has_bearer_token: bool,
        #[serde(default, rename = "useProxy", alias = "use_proxy")]
        use_proxy: bool,
    },
}

impl UpstreamTransportConfig {
    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::Stdio { .. } => "stdio",
            Self::StreamableHttp { .. } => "streamable_http",
            Self::Sse { .. } => "sse",
        }
    }

    pub fn summary_target(&self) -> String {
        match self {
            Self::Stdio { command, args, .. } => {
                if args.is_empty() {
                    command.clone()
                } else {
                    format!("{command} {}", args.join(" "))
                }
            }
            Self::StreamableHttp { url, .. } | Self::Sse { url, .. } => url.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamServerConfig {
    pub id: String,
    /// Unique lowercase prefix for exposed tools (`{alias}__{tool}`).
    pub alias: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_expose_mode", alias = "expose_mode")]
    pub expose_mode: UpstreamExposeMode,
    pub transport: UpstreamTransportConfig,
    /// Tool original names explicitly disabled for this upstream server.
    #[serde(default, alias = "disabled_tools")]
    pub disabled_tools: Vec<String>,
    /// Per-call timeout in seconds (5..=600, default 60).
    #[serde(default = "default_call_timeout_secs", alias = "call_timeout_secs")]
    pub call_timeout_secs: u64,
    /// Stdio idle process reap threshold in seconds (0 = keep alive, default 900).
    #[serde(default = "default_idle_timeout_secs", alias = "idle_timeout_secs")]
    pub idle_timeout_secs: u64,
    #[serde(default = "default_source")]
    pub source: String,
}

impl UpstreamServerConfig {
    pub fn clamp_timeouts(&mut self) {
        self.call_timeout_secs = self
            .call_timeout_secs
            .clamp(MIN_CALL_TIMEOUT_SECS, MAX_CALL_TIMEOUT_SECS);
        if self.idle_timeout_secs > MAX_IDLE_TIMEOUT_SECS {
            self.idle_timeout_secs = MAX_IDLE_TIMEOUT_SECS;
        }
    }

    pub fn is_tool_disabled(&self, original_tool_name: &str) -> bool {
        self.disabled_tools
            .iter()
            .any(|disabled| disabled == original_tool_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamMcpConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(
        default,
        rename = "alwaysExposeProxyTools",
        alias = "always_expose_proxy_tools"
    )]
    pub always_expose_proxy_tools: bool,
    #[serde(default)]
    pub servers: Vec<UpstreamServerConfig>,
}

impl Default for UpstreamMcpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            always_expose_proxy_tools: false,
            servers: Vec::new(),
        }
    }
}

/// Write-only secret mutations paired with saving an upstream server.
/// Existing stored secrets whose keys are omitted from `set_*` and `remove_*` remain untouched.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamSecretPatch {
    #[serde(default)]
    pub set_env_secrets: BTreeMap<String, String>,
    #[serde(default)]
    pub remove_env_secrets: Vec<String>,
    #[serde(default)]
    pub set_header_secrets: BTreeMap<String, String>,
    #[serde(default)]
    pub remove_header_secrets: Vec<String>,
    /// `Some(Some(token))` sets bearer token, `Some(None)` clears it, `None` leaves unchanged.
    #[serde(default)]
    pub bearer_token: Option<Option<String>>,
}

pub fn env_secret_item_id(server_id: &str, env_key: &str) -> String {
    format!("{server_id}:{env_key}")
}

pub fn header_secret_item_id(server_id: &str, header_name: &str) -> String {
    format!("{server_id}:{header_name}")
}

/// Heuristic classifier used by JSON/local client import and UI hints to avoid
/// storing API keys or tokens in plaintext config fields.
pub fn is_sensitive_key(key: &str) -> bool {
    let upper = key.trim().to_ascii_uppercase();
    if upper.is_empty() {
        return false;
    }
    if matches!(
        upper.as_str(),
        "AUTHORIZATION" | "PROXY-AUTHORIZATION" | "COOKIE" | "X-API-KEY" | "API-KEY"
    ) {
        return true;
    }
    const SENSITIVE_MARKERS: &[&str] = &[
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "PASSWD",
        "API_KEY",
        "APIKEY",
        "API-KEY",
        "ACCESS_KEY",
        "PRIVATE_KEY",
        "AUTH",
        "CREDENTIAL",
        "COOKIE",
        "BEARER",
        "PAT",
    ];
    SENSITIVE_MARKERS
        .iter()
        .any(|marker| upper.contains(marker))
}

/// Self-loop detection context for the local Coding Tools MCP instance.
#[derive(Debug, Clone, Default)]
pub struct SelfLoopContext {
    pub local_port: u16,
    pub public_url: String,
}

impl SelfLoopContext {
    pub fn from_settings(settings: &crate::settings::AppSettings) -> Self {
        Self {
            local_port: settings.global_gateway.local_port,
            public_url: settings.global_gateway.public_url.clone(),
        }
    }
}

/// Returns `Some(reason)` when `transport` points back at this Coding Tools MCP instance.
pub fn detect_self_loop(
    transport: &UpstreamTransportConfig,
    ctx: &SelfLoopContext,
) -> Option<String> {
    match transport {
        UpstreamTransportConfig::Stdio { command, args, .. } => {
            let cmd_lower = command.trim().to_ascii_lowercase();
            let is_self_bin = cmd_lower.ends_with("coding-tools-mcp")
                || cmd_lower.ends_with("coding-tools-mcp.exe")
                || cmd_lower.ends_with("coding-tools-mcp-desktop")
                || cmd_lower.ends_with("coding-tools-mcp-desktop.exe");
            let has_stdio_bridge = args.iter().any(|arg| arg.trim() == "mcp-stdio");
            if is_self_bin && has_stdio_bridge {
                let references_own_port = args.iter().any(|arg| {
                    url_targets_loopback_port(arg, ctx.local_port)
                        || url_matches_public_base(arg, &ctx.public_url)
                });
                if references_own_port || args.len() <= 1 {
                    return Some(format!(
                        "禁止自环聚合：该 Stdio 命令指向当前 Coding Tools MCP 自身桥接入口（端口 {}）",
                        ctx.local_port
                    ));
                }
            }
            None
        }
        UpstreamTransportConfig::StreamableHttp { url, .. }
        | UpstreamTransportConfig::Sse { url, .. } => {
            if url_targets_loopback_port(url, ctx.local_port) {
                return Some(format!(
                    "禁止自环聚合：URL `{}` 指向当前本机 Global MCP 监听端口 {}",
                    url.trim(),
                    ctx.local_port
                ));
            }
            if url_matches_public_base(url, &ctx.public_url) {
                return Some(format!(
                    "禁止自环聚合：URL `{}` 指向当前 Global MCP 自身的公网入口",
                    url.trim()
                ));
            }
            None
        }
    }
}

fn url_targets_loopback_port(raw_url: &str, local_port: u16) -> bool {
    if local_port == 0 {
        return false;
    }
    let Ok(parsed) = reqwest::Url::parse(raw_url.trim()) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host_lower = host.trim_matches(['[', ']']).to_ascii_lowercase();
    let is_loopback = matches!(
        host_lower.as_str(),
        "127.0.0.1" | "localhost" | "::1" | "0.0.0.0"
    );
    if !is_loopback {
        return false;
    }
    parsed.port_or_known_default() == Some(local_port)
}

fn url_matches_public_base(raw_url: &str, public_base: &str) -> bool {
    let base = public_base.trim().trim_end_matches('/');
    if base.is_empty() {
        return false;
    }
    let Ok(target) = reqwest::Url::parse(raw_url.trim()) else {
        return false;
    };
    let Ok(own) = reqwest::Url::parse(base) else {
        return false;
    };
    target.scheme().eq_ignore_ascii_case(own.scheme())
        && target
            .host_str()
            .zip(own.host_str())
            .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
        && target.port_or_known_default() == own.port_or_known_default()
        && target.path().trim_end_matches('/') == "/mcp"
}

pub fn validate_http_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("MCP 服务 URL 不能为空".into());
    }
    let parsed =
        reqwest::Url::parse(trimmed).map_err(|err| format!("无效的 URL `{trimmed}`: {err}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(format!(
            "仅支持 http:// 或 https:// 协议，当前为 `{}`",
            parsed.scheme()
        ));
    }
    if parsed.host_str().is_none() {
        return Err(format!("URL 缺少主机名: `{trimmed}`"));
    }
    Ok(trimmed.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedUpstreamTool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_object_schema")]
    pub input_schema: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Value>,
}

fn default_object_schema() -> Value {
    json!({
        "type": "object",
        "properties": {}
    })
}

impl CachedUpstreamTool {
    pub fn from_mcp_tool(raw: &Value) -> Option<Self> {
        let name = raw
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())?
            .to_string();
        let title = raw
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string);
        let description = raw
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let input_schema = super::naming::normalize_upstream_tool_schema(
            raw.get("inputSchema").unwrap_or(&Value::Null),
        );
        let annotations = raw.get("annotations").filter(|v| v.is_object()).cloned();
        Some(Self {
            name,
            title,
            description,
            input_schema,
            annotations,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedUpstreamCatalog {
    pub synced_at_ms: u64,
    #[serde(default)]
    pub protocol_version: String,
    #[serde(default)]
    pub server_name: String,
    #[serde(default)]
    pub server_version: String,
    #[serde(default)]
    pub tools: Vec<CachedUpstreamTool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamCatalogCacheFile {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub catalogs: HashMap<String, CachedUpstreamCatalog>,
}

impl UpstreamCatalogCacheFile {
    pub fn default_path() -> AppResult<PathBuf> {
        Ok(platform()
            .app_config_dir()?
            .join("data")
            .join("upstream-mcp-cache.json"))
    }

    pub fn load_at(path: &Path) -> Self {
        let Ok(raw) = fs::read_to_string(path) else {
            return Self {
                version: 1,
                catalogs: HashMap::new(),
            };
        };
        serde_json::from_str(&raw).unwrap_or_else(|_| Self {
            version: 1,
            catalogs: HashMap::new(),
        })
    }

    pub fn save_at(&self, path: &Path) -> AppResult<()> {
        crate::util::fs::atomic_write_json(path, self)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamToolItemDto {
    pub original_name: String,
    pub exposed_name: String,
    pub title: Option<String>,
    pub description: String,
    pub disabled: bool,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamServerRuntimeDto {
    /// "idle" | "connecting" | "connected" | "error" | "disabled"
    pub status: String,
    pub last_error: Option<String>,
    pub last_latency_ms: Option<u64>,
    pub last_used_ms: Option<u64>,
    pub synced_at_ms: Option<u64>,
    pub protocol_version: Option<String>,
    pub server_info_name: Option<String>,
    pub server_info_version: Option<String>,
    pub stderr_tail: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamServerOverviewDto {
    pub config: UpstreamServerConfig,
    pub runtime: UpstreamServerRuntimeDto,
    pub total_tool_count: usize,
    pub enabled_tool_count: usize,
    pub tools: Vec<UpstreamToolItemDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamMcpOverviewDto {
    pub enabled: bool,
    pub always_expose_proxy_tools: bool,
    pub builtin_tool_count: usize,
    pub direct_exposed_tool_count: usize,
    pub proxy_meta_tool_count: usize,
    pub total_exposed_tool_count: usize,
    pub direct_tools_warning_threshold: usize,
    pub servers: Vec<UpstreamServerOverviewDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamTestSyncResultDto {
    pub server_id: String,
    pub alias: String,
    pub ok: bool,
    pub latency_ms: u64,
    pub protocol_version: String,
    pub server_name: String,
    pub server_version: String,
    pub tool_count: usize,
    pub error: Option<String>,
    pub stderr_tail: Vec<String>,
}

pub fn unix_time_ms() -> u64 {
    crate::util::time::unix_now_ms()
}
