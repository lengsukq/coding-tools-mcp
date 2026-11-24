//! Upstream MCP Hub: aggregates external Stdio, Streamable HTTP, and Legacy SSE
//! MCP servers behind Coding Tools MCP's single Global MCP endpoint.

mod client;
pub mod import;
mod manager;
mod model;
pub mod naming;
pub mod proxy_tools;
mod transport_http;
mod transport_sse;
mod transport_stdio;

pub use import::{
    parse_mcp_servers_json, scan_local_mcp_clients, ImportSelectionItem, ImportedUpstreamCandidate,
};
pub use manager::{
    terminate_all_managed_children_sync, validate_and_normalize_server, DirectToolRoute,
    UpstreamCallOutcome, UpstreamHub,
};
pub use model::{
    detect_self_loop, env_secret_item_id, header_secret_item_id, is_sensitive_key,
    validate_http_url, CachedUpstreamCatalog, CachedUpstreamTool, SelfLoopContext,
    UpstreamExposeMode, UpstreamMcpConfig, UpstreamMcpOverviewDto, UpstreamSecretPatch,
    UpstreamServerConfig, UpstreamServerOverviewDto, UpstreamServerRuntimeDto,
    UpstreamTestSyncResultDto, UpstreamToolItemDto, UpstreamTransportConfig,
    DEFAULT_CALL_TIMEOUT_SECS, DEFAULT_IDLE_TIMEOUT_SECS, DIRECT_TOOLS_WARNING_THRESHOLD,
    UPSTREAM_BEARER_SECRETS_SCOPE, UPSTREAM_ENV_SECRETS_SCOPE, UPSTREAM_HEADER_SECRETS_SCOPE,
    UPSTREAM_ORIGIN_HEADER,
};
pub use proxy_tools::{
    is_proxy_meta_tool, proxy_meta_tool_definitions, TOOL_UPSTREAM_CALL, TOOL_UPSTREAM_LIST,
    TOOL_UPSTREAM_TOOLS,
};

#[cfg(test)]
mod tests;
