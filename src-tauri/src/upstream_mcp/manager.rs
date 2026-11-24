use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex as StdMutex, RwLock as StdRwLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::sync::Mutex as AsyncMutex;

use crate::mcp::gateway::GatewayError;
use crate::platform::platform;
use crate::secret::SecretStore;
use crate::settings::AppSettings;
use crate::tools::workspace::tool_ok;
use crate::tools::wrap_mcp_tool_result;

use super::client::{ResolvedUpstreamSecrets, UpstreamClient};
use super::model::{
    detect_self_loop, env_secret_item_id, header_secret_item_id, unix_time_ms, validate_http_url,
    CachedUpstreamCatalog, SelfLoopContext, UpstreamCatalogCacheFile, UpstreamExposeMode,
    UpstreamMcpConfig, UpstreamMcpOverviewDto, UpstreamServerConfig, UpstreamServerOverviewDto,
    UpstreamServerRuntimeDto, UpstreamTestSyncResultDto, UpstreamToolItemDto,
    UpstreamTransportConfig, DIRECT_TOOLS_WARNING_THRESHOLD, UPSTREAM_BEARER_SECRETS_SCOPE,
    UPSTREAM_ENV_SECRETS_SCOPE, UPSTREAM_HEADER_SECRETS_SCOPE,
};
use super::naming::{
    build_exposed_tool_definition, compute_exposed_tool_name, normalize_alias,
    reserved_builtin_tool_names,
};
use super::proxy_tools::{
    is_proxy_meta_tool, proxy_meta_tool_definitions, TOOL_UPSTREAM_CALL, TOOL_UPSTREAM_LIST,
    TOOL_UPSTREAM_TOOLS,
};

/// Global PID tracker for managed Stdio upstream child processes so synchronous
/// process-exit hooks (`tauri::RunEvent::Exit`) can terminate every child tree
/// cleanly even if Tokio is already shutting down.
static MANAGED_STDIO_PIDS: LazyLock<StdMutex<HashSet<u32>>> =
    LazyLock::new(|| StdMutex::new(HashSet::new()));

fn track_stdio_pid(pid: Option<u32>) {
    if let Some(pid) = pid {
        MANAGED_STDIO_PIDS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pid);
    }
}

fn untrack_stdio_pid(pid: Option<u32>) {
    if let Some(pid) = pid {
        MANAGED_STDIO_PIDS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&pid);
    }
}

pub fn terminate_all_managed_children_sync() {
    let pids: Vec<u32> = MANAGED_STDIO_PIDS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .drain()
        .collect();
    for pid in pids {
        let _ = platform().terminate_process_tree(pid);
    }
}

#[derive(Debug, Clone)]
pub struct DirectToolRoute {
    pub server_id: String,
    pub alias: String,
    pub server_name: String,
    pub original_tool_name: String,
}

pub struct UpstreamCallOutcome {
    pub result: Value,
    pub execution_host: String,
}

struct ActiveUpstream {
    client: Arc<UpstreamClient>,
    config_fingerprint: String,
    last_used_ms: AtomicU64,
}

#[derive(Debug, Clone, Default)]
struct SlotRuntimeStatus {
    status: String,
    last_error: Option<String>,
    last_latency_ms: Option<u64>,
    last_used_ms: Option<u64>,
    protocol_version: Option<String>,
    server_name: Option<String>,
    server_version: Option<String>,
    stderr_tail: Vec<String>,
}

struct UpstreamServerSlot {
    connection: AsyncMutex<Option<ActiveUpstream>>,
    status: StdMutex<SlotRuntimeStatus>,
    tools_dirty: AtomicBool,
}

impl UpstreamServerSlot {
    fn new() -> Self {
        Self {
            connection: AsyncMutex::new(None),
            status: StdMutex::new(SlotRuntimeStatus {
                status: "idle".into(),
                ..SlotRuntimeStatus::default()
            }),
            tools_dirty: AtomicBool::new(false),
        }
    }

    fn set_status(&self, f: impl FnOnce(&mut SlotRuntimeStatus)) {
        let mut guard = self.status.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard);
    }

    fn snapshot_status(
        &self,
        enabled: bool,
        cached: Option<&CachedUpstreamCatalog>,
    ) -> UpstreamServerRuntimeDto {
        let guard = self.status.lock().unwrap_or_else(|e| e.into_inner());
        let status = if !enabled {
            "disabled".to_string()
        } else if guard.status.is_empty() {
            "idle".to_string()
        } else {
            guard.status.clone()
        };
        UpstreamServerRuntimeDto {
            status,
            last_error: guard.last_error.clone(),
            last_latency_ms: guard.last_latency_ms,
            last_used_ms: guard.last_used_ms,
            synced_at_ms: cached.map(|c| c.synced_at_ms).filter(|&ts| ts > 0),
            protocol_version: guard.protocol_version.clone().or_else(|| {
                cached
                    .map(|c| c.protocol_version.clone())
                    .filter(|v| !v.is_empty())
            }),
            server_info_name: guard.server_name.clone().or_else(|| {
                cached
                    .map(|c| c.server_name.clone())
                    .filter(|v| !v.is_empty())
            }),
            server_info_version: guard.server_version.clone().or_else(|| {
                cached
                    .map(|c| c.server_version.clone())
                    .filter(|v| !v.is_empty())
            }),
            stderr_tail: guard.stderr_tail.clone(),
        }
    }
}

enum CachePersistence {
    DefaultDisk,
    CustomDisk(PathBuf),
    #[cfg(test)]
    InMemoryOnly,
}

pub struct UpstreamHub {
    persistence: CachePersistence,
    catalog_loaded: AtomicBool,
    catalogs: StdRwLock<HashMap<String, CachedUpstreamCatalog>>,
    slots: StdRwLock<HashMap<String, Arc<UpstreamServerSlot>>>,
}

impl Default for UpstreamHub {
    fn default() -> Self {
        Self::new()
    }
}

impl UpstreamHub {
    pub fn new() -> Self {
        Self {
            persistence: CachePersistence::DefaultDisk,
            catalog_loaded: AtomicBool::new(false),
            catalogs: StdRwLock::new(HashMap::new()),
            slots: StdRwLock::new(HashMap::new()),
        }
    }

    #[cfg(test)]
    pub fn new_in_memory() -> Self {
        Self {
            persistence: CachePersistence::InMemoryOnly,
            catalog_loaded: AtomicBool::new(true),
            catalogs: StdRwLock::new(HashMap::new()),
            slots: StdRwLock::new(HashMap::new()),
        }
    }

    pub fn new_with_cache_path(path: PathBuf) -> Self {
        Self {
            persistence: CachePersistence::CustomDisk(path),
            catalog_loaded: AtomicBool::new(false),
            catalogs: StdRwLock::new(HashMap::new()),
            slots: StdRwLock::new(HashMap::new()),
        }
    }

    fn ensure_catalog_loaded(&self) {
        if self.catalog_loaded.load(Ordering::Acquire) {
            return;
        }
        let path = match &self.persistence {
            #[cfg(test)]
            CachePersistence::InMemoryOnly => {
                self.catalog_loaded.store(true, Ordering::Release);
                return;
            }
            CachePersistence::CustomDisk(path) => Some(path.clone()),
            CachePersistence::DefaultDisk => UpstreamCatalogCacheFile::default_path().ok(),
        };
        let loaded = path
            .as_deref()
            .map(UpstreamCatalogCacheFile::load_at)
            .unwrap_or_default();
        let mut guard = self.catalogs.write().unwrap_or_else(|e| e.into_inner());
        if !self.catalog_loaded.swap(true, Ordering::AcqRel) {
            for (k, v) in loaded.catalogs {
                guard.entry(k).or_insert(v);
            }
        }
    }

    fn persist_catalog_best_effort(&self) {
        let path = match &self.persistence {
            #[cfg(test)]
            CachePersistence::InMemoryOnly => return,
            CachePersistence::CustomDisk(p) => p.clone(),
            CachePersistence::DefaultDisk => match UpstreamCatalogCacheFile::default_path() {
                Ok(p) => p,
                Err(_) => return,
            },
        };
        let snapshot = UpstreamCatalogCacheFile {
            version: 1,
            catalogs: self
                .catalogs
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
        };
        if let Err(err) = snapshot.save_at(&path) {
            eprintln!("[upstream-mcp] failed to persist catalog cache: {err}");
        }
    }

    pub fn seed_catalog_cache(&self, server_id: &str, catalog: CachedUpstreamCatalog) {
        self.ensure_catalog_loaded();
        self.catalogs
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(server_id.to_string(), catalog);
        self.persist_catalog_best_effort();
    }

    fn remove_catalog_cache(&self, server_id: &str) {
        self.ensure_catalog_loaded();
        let removed = self
            .catalogs
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(server_id)
            .is_some();
        if removed {
            self.persist_catalog_best_effort();
        }
    }

    fn slot_for(&self, server_id: &str) -> Arc<UpstreamServerSlot> {
        {
            let guard = self.slots.read().unwrap_or_else(|e| e.into_inner());
            if let Some(slot) = guard.get(server_id) {
                return Arc::clone(slot);
            }
        }
        let mut guard = self.slots.write().unwrap_or_else(|e| e.into_inner());
        Arc::clone(
            guard
                .entry(server_id.to_string())
                .or_insert_with(|| Arc::new(UpstreamServerSlot::new())),
        )
    }

    /// Returns `(enabled_direct_servers, enabled_proxy_servers)` for initialize guidance.
    pub fn enabled_mode_counts(&self, settings_override: Option<&AppSettings>) -> (usize, usize) {
        let settings = effective_settings(settings_override);
        if !settings.upstream_mcp.enabled {
            return (0, 0);
        }
        let mut direct = 0usize;
        let mut proxy = 0usize;
        for srv in &settings.upstream_mcp.servers {
            if !srv.enabled {
                continue;
            }
            match srv.expose_mode {
                UpstreamExposeMode::Direct => direct += 1,
                UpstreamExposeMode::Proxy => proxy += 1,
            }
        }
        (direct, proxy)
    }

    /// Build the aggregated tools to append to Global MCP `tools/list`.
    /// Fast, non-blocking read from cached catalogs so `tools/list` never stalls on cold-starting stdio processes.
    pub fn exposed_tool_definitions(&self, settings_override: Option<&AppSettings>) -> Vec<Value> {
        let settings = effective_settings(settings_override);
        let cfg = &settings.upstream_mcp;
        if !cfg.enabled {
            return Vec::new();
        }
        self.ensure_catalog_loaded();
        let catalogs = self.catalogs.read().unwrap_or_else(|e| e.into_inner());
        let (direct_tools, _, has_proxy) = build_direct_index(cfg, &catalogs);

        let mut out = direct_tools;
        if has_proxy || cfg.always_expose_proxy_tools {
            out.extend(proxy_meta_tool_definitions());
        }
        out
    }

    /// Resolve a `direct`-mode exposed tool name back to its upstream server and exact original tool name.
    pub fn resolve_direct_tool(
        &self,
        exposed_name: &str,
        settings_override: Option<&AppSettings>,
    ) -> Option<DirectToolRoute> {
        let settings = effective_settings(settings_override);
        let cfg = &settings.upstream_mcp;
        if !cfg.enabled {
            return None;
        }
        self.ensure_catalog_loaded();
        let catalogs = self.catalogs.read().unwrap_or_else(|e| e.into_inner());
        let (_, route_map, _) = build_direct_index(cfg, &catalogs);
        route_map.get(exposed_name).cloned()
    }

    pub fn is_upstream_tool_name(
        &self,
        tool_name: &str,
        settings_override: Option<&AppSettings>,
    ) -> bool {
        if is_proxy_meta_tool(tool_name) {
            return true;
        }
        self.resolve_direct_tool(tool_name, settings_override)
            .is_some()
    }

    /// Ensure any enabled `direct`-mode upstream server that has never been synced yet
    /// gets a bounded parallel discovery attempt before `tools/list` returns.
    pub async fn warm_unsynced_direct_servers_arc(
        self: &Arc<Self>,
        settings_override: Option<AppSettings>,
    ) {
        let settings = effective_settings(settings_override.as_ref());
        if !settings.upstream_mcp.enabled {
            return;
        }
        self.ensure_catalog_loaded();
        let unsynced_ids: Vec<String> = {
            let catalogs = self.catalogs.read().unwrap_or_else(|e| e.into_inner());
            settings
                .upstream_mcp
                .servers
                .iter()
                .filter(|s| {
                    s.enabled
                        && s.expose_mode == UpstreamExposeMode::Direct
                        && !catalogs.contains_key(&s.id)
                })
                .map(|s| s.id.clone())
                .collect()
        };
        if unsynced_ids.is_empty() {
            return;
        }
        let mut set = tokio::task::JoinSet::new();
        for server_id in unsynced_ids {
            let hub = Arc::clone(self);
            let override_clone = settings_override.clone();
            set.spawn(async move {
                let _ = tokio::time::timeout(
                    Duration::from_secs(4),
                    hub.sync_server_catalog(&server_id, override_clone.as_ref()),
                )
                .await;
            });
        }
        while set.join_next().await.is_some() {}
    }

    /// Execute either one of the 3 `mcp_upstream_*` proxy meta-tools or a `direct`-mode `{alias}__{tool}` call.
    pub async fn handle_tool_call(
        &self,
        tool_name: &str,
        args: &Value,
        settings_override: Option<&AppSettings>,
    ) -> Result<UpstreamCallOutcome, Value> {
        let settings = effective_settings(settings_override);
        if !settings.upstream_mcp.enabled {
            return Err(GatewayError::invalid(
                "UPSTREAM_MCP_DISABLED",
                "MCP 聚合功能当前已在设置中关闭。",
            )
            .to_rpc_error());
        }

        match tool_name {
            TOOL_UPSTREAM_LIST => self.handle_meta_list(&settings, args),
            TOOL_UPSTREAM_TOOLS => self.handle_meta_tools(&settings, args).await,
            TOOL_UPSTREAM_CALL => self.handle_meta_call(&settings, args).await,
            _ => {
                let route = self
                    .resolve_direct_tool(tool_name, Some(&settings))
                    .ok_or_else(|| {
                        GatewayError::not_found(
                            "UPSTREAM_TOOL_NOT_FOUND",
                            format!("未找到聚合工具 `{tool_name}`，可能对应上游 MCP 已禁用或工具已隐藏。"),
                        )
                        .to_rpc_error()
                    })?;
                let server = settings
                    .upstream_mcp
                    .servers
                    .iter()
                    .find(|s| s.id == route.server_id && s.enabled)
                    .ok_or_else(|| {
                        GatewayError::not_found(
                            "UPSTREAM_SERVER_UNAVAILABLE",
                            format!("上游 MCP `{}` 未启用或不存在。", route.alias),
                        )
                        .to_rpc_error()
                    })?;
                self.invoke_server_tool(&settings, server, &route.original_tool_name, args.clone())
                    .await
            }
        }
    }

    fn handle_meta_list(
        &self,
        settings: &AppSettings,
        args: &Value,
    ) -> Result<UpstreamCallOutcome, Value> {
        self.ensure_catalog_loaded();
        let catalogs = self.catalogs.read().unwrap_or_else(|e| e.into_inner());
        let servers: Vec<Value> = settings
            .upstream_mcp
            .servers
            .iter()
            .filter(|s| s.enabled)
            .map(|srv| {
                let cached = catalogs.get(&srv.id);
                let slot = self.slot_for(&srv.id);
                let rt = slot.snapshot_status(srv.enabled, cached);
                let total_tools = cached.map(|c| c.tools.len()).unwrap_or(0);
                let enabled_tools = cached
                    .map(|c| {
                        c.tools
                            .iter()
                            .filter(|t| !srv.is_tool_disabled(&t.name))
                            .count()
                    })
                    .unwrap_or(0);
                json!({
                    "id": srv.id,
                    "alias": srv.alias,
                    "name": srv.name,
                    "description": srv.description,
                    "transport": srv.transport.kind_label(),
                    "expose_mode": srv.expose_mode.as_str(),
                    "status": rt.status,
                    "synced": cached.is_some(),
                    "tool_count": enabled_tools,
                    "total_tool_count": total_tools,
                    "last_error": rt.last_error,
                })
            })
            .collect();

        let structured = tool_ok(json!({
            "count": servers.len(),
            "servers": servers,
            "hint": "Use mcp_upstream_tools(server=..., query=...) to inspect tool schemas, then mcp_upstream_call(server=..., tool=..., arguments=...) to invoke."
        }));
        Ok(UpstreamCallOutcome {
            result: wrap_mcp_tool_result(TOOL_UPSTREAM_LIST, args, structured),
            execution_host: "local".into(),
        })
    }

    async fn handle_meta_tools(
        &self,
        settings: &AppSettings,
        args: &Value,
    ) -> Result<UpstreamCallOutcome, Value> {
        let server_filter = args
            .get("server")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty());
        let query = args
            .get("query")
            .and_then(Value::as_str)
            .map(|v| v.trim().to_ascii_lowercase())
            .filter(|v| !v.is_empty());
        let include_schema = args
            .get("include_schema")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(20)
            .clamp(1, 50) as usize;

        let target_servers: Vec<&UpstreamServerConfig> = settings
            .upstream_mcp
            .servers
            .iter()
            .filter(|srv| {
                if !srv.enabled {
                    return false;
                }
                if let Some(filter) = server_filter {
                    srv.alias.eq_ignore_ascii_case(filter) || srv.id == filter
                } else {
                    true
                }
            })
            .collect();

        if let Some(filter) = server_filter {
            if target_servers.is_empty() {
                return Err(GatewayError::not_found(
                    "UPSTREAM_SERVER_NOT_FOUND",
                    format!("未找到已启用的上游 MCP 服务 `{filter}`；请先调用 mcp_upstream_list 查看可用别名。"),
                )
                .to_rpc_error());
            }
        }

        // Lazy-sync any selected server whose catalog has not been fetched yet or was invalidated by list_changed.
        self.ensure_catalog_loaded();
        let mut sync_warnings: Vec<Value> = Vec::new();
        for srv in &target_servers {
            let needs_sync = {
                let catalogs = self.catalogs.read().unwrap_or_else(|e| e.into_inner());
                let slot = self.slot_for(&srv.id);
                !catalogs.contains_key(&srv.id) || slot.tools_dirty.load(Ordering::Relaxed)
            };
            if needs_sync {
                if let Err(err) = self.sync_server_catalog(&srv.id, Some(settings)).await {
                    sync_warnings.push(json!({
                        "server": srv.alias,
                        "error": err
                    }));
                }
            }
        }

        let catalogs = self.catalogs.read().unwrap_or_else(|e| e.into_inner());
        let mut matched: Vec<Value> = Vec::new();
        for srv in &target_servers {
            let Some(catalog) = catalogs.get(&srv.id) else {
                continue;
            };
            for tool in &catalog.tools {
                if srv.is_tool_disabled(&tool.name) {
                    continue;
                }
                if let Some(ref q) = query {
                    let in_name = tool.name.to_ascii_lowercase().contains(q);
                    let in_desc = tool.description.to_ascii_lowercase().contains(q);
                    let in_srv = srv.alias.to_ascii_lowercase().contains(q)
                        || srv.name.to_ascii_lowercase().contains(q);
                    if !in_name && !in_desc && !in_srv {
                        continue;
                    }
                }
                let mut item = json!({
                    "server": srv.alias,
                    "server_name": srv.name,
                    "expose_mode": srv.expose_mode.as_str(),
                    "tool": tool.name,
                    "description": tool.description,
                });
                if let Some(title) = &tool.title {
                    item["title"] = Value::String(title.clone());
                }
                if include_schema {
                    item["inputSchema"] = tool.input_schema.clone();
                }
                matched.push(item);
            }
        }
        drop(catalogs);

        let total = matched.len();
        let page: Vec<Value> = matched.into_iter().skip(offset).take(limit).collect();
        let has_more = offset.saturating_add(page.len()) < total;
        let structured = tool_ok(json!({
            "total": total,
            "offset": offset,
            "limit": limit,
            "has_more": has_more,
            "tools": page,
            "warnings": sync_warnings,
        }));
        Ok(UpstreamCallOutcome {
            result: wrap_mcp_tool_result(TOOL_UPSTREAM_TOOLS, args, structured),
            execution_host: "local".into(),
        })
    }

    async fn handle_meta_call(
        &self,
        settings: &AppSettings,
        args: &Value,
    ) -> Result<UpstreamCallOutcome, Value> {
        let server_key = args
            .get("server")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                GatewayError::invalid(
                    "INVALID_ARGUMENT",
                    "缺少必填参数 `server`（上游 MCP 别名或 ID）",
                )
                .to_rpc_error()
            })?;
        let tool_name = args
            .get("tool")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                GatewayError::invalid("INVALID_ARGUMENT", "缺少必填参数 `tool`（上游原始工具名）")
                    .to_rpc_error()
            })?;
        let nested_args = args
            .get("arguments")
            .cloned()
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({}));

        let server = settings
            .upstream_mcp
            .servers
            .iter()
            .find(|s| s.enabled && (s.alias.eq_ignore_ascii_case(server_key) || s.id == server_key))
            .ok_or_else(|| {
                GatewayError::not_found(
                    "UPSTREAM_SERVER_NOT_FOUND",
                    format!("未找到已启用的上游 MCP 服务 `{server_key}`。请先调用 mcp_upstream_list 核对。"),
                )
                .to_rpc_error()
            })?;

        if server.is_tool_disabled(tool_name) {
            return Err(GatewayError::invalid(
                "UPSTREAM_TOOL_DISABLED",
                format!(
                    "上游 MCP `{}` 的工具 `{tool_name}` 已在管理界面中禁用。",
                    server.alias
                ),
            )
            .to_rpc_error());
        }

        self.invoke_server_tool(settings, server, tool_name, nested_args)
            .await
    }

    async fn invoke_server_tool(
        &self,
        settings: &AppSettings,
        server: &UpstreamServerConfig,
        original_tool_name: &str,
        arguments: Value,
    ) -> Result<UpstreamCallOutcome, Value> {
        let client = self
            .get_or_connect(settings, server, false)
            .await
            .map_err(|err| {
                GatewayError::internal(
                    "UPSTREAM_CONNECT_FAILED",
                    format!("连接上游 MCP `{}` 失败: {err}", server.alias),
                )
                .to_rpc_error()
            })?;

        let timeout = Duration::from_secs(server.call_timeout_secs.clamp(
            super::model::MIN_CALL_TIMEOUT_SECS,
            super::model::MAX_CALL_TIMEOUT_SECS,
        ));
        let started = Instant::now();
        let slot = self.slot_for(&server.id);
        let call_res = client
            .call_tool(original_tool_name, arguments, timeout)
            .await;
        let elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        let now_ms = unix_time_ms();

        match call_res {
            Ok(result) => {
                slot.set_status(|st| {
                    st.status = "connected".into();
                    st.last_error = None;
                    st.last_latency_ms = Some(elapsed_ms);
                    st.last_used_ms = Some(now_ms);
                    st.stderr_tail = client.stderr_tail();
                });
                Ok(UpstreamCallOutcome {
                    result,
                    execution_host: format!("upstream:{} ({})", server.alias, server.name),
                })
            }
            Err(err) => {
                let tail = client.stderr_tail();
                if !client.is_alive() {
                    let mut conn_guard = slot.connection.lock().await;
                    if let Some(active) = conn_guard.take() {
                        untrack_stdio_pid(active.client.stdio_pid());
                        active.client.shutdown().await;
                    }
                }
                slot.set_status(|st| {
                    st.status = "error".into();
                    st.last_error = Some(err.clone());
                    st.last_latency_ms = Some(elapsed_ms);
                    st.stderr_tail = tail;
                });
                Err(GatewayError::internal(
                    "UPSTREAM_TOOL_CALL_FAILED",
                    format!(
                        "调用上游 MCP `{}` 工具 `{original_tool_name}` 失败: {err}",
                        server.alias
                    ),
                )
                .to_rpc_error())
            }
        }
    }

    async fn get_or_connect(
        &self,
        settings: &AppSettings,
        server: &UpstreamServerConfig,
        force_reconnect: bool,
    ) -> Result<Arc<UpstreamClient>, String> {
        let loop_ctx = SelfLoopContext::from_settings(settings);
        if let Some(reason) = detect_self_loop(&server.transport, &loop_ctx) {
            return Err(reason);
        }

        let slot = self.slot_for(&server.id);
        let fingerprint = server_config_fingerprint(server, settings);

        let mut guard = slot.connection.lock().await;
        if !force_reconnect {
            if let Some(active) = guard.as_ref() {
                if active.config_fingerprint == fingerprint && active.client.is_alive() {
                    active.last_used_ms.store(unix_time_ms(), Ordering::Relaxed);
                    return Ok(Arc::clone(&active.client));
                }
            }
        }

        if let Some(old) = guard.take() {
            untrack_stdio_pid(old.client.stdio_pid());
            old.client.shutdown().await;
        }

        slot.set_status(|st| {
            st.status = "connecting".into();
            st.last_error = None;
        });

        let secrets = resolve_secrets_for_server(server, settings);
        let extra_paths = parse_executable_paths(&settings.global_executable_paths);
        let origin_device_id = settings.lan_federation.device_id.clone();
        let slot_for_cb = Arc::clone(&slot);
        let on_tools_changed = Arc::new(move || {
            slot_for_cb.tools_dirty.store(true, Ordering::Release);
        });

        match UpstreamClient::connect(
            &server.transport,
            secrets,
            extra_paths,
            settings.proxy.clone(),
            origin_device_id,
            on_tools_changed,
        )
        .await
        {
            Ok(client) => {
                let client = Arc::new(client);
                track_stdio_pid(client.stdio_pid());
                let now_ms = unix_time_ms();
                slot.set_status(|st| {
                    st.status = "connected".into();
                    st.last_error = None;
                    st.last_latency_ms = Some(client.handshake.latency_ms);
                    st.last_used_ms = Some(now_ms);
                    st.protocol_version = Some(client.handshake.protocol_version.clone());
                    st.server_name = Some(client.handshake.server_name.clone());
                    st.server_version = Some(client.handshake.server_version.clone());
                    st.stderr_tail = client.stderr_tail();
                });
                *guard = Some(ActiveUpstream {
                    client: Arc::clone(&client),
                    config_fingerprint: fingerprint,
                    last_used_ms: AtomicU64::new(now_ms),
                });
                Ok(client)
            }
            Err(err) => {
                slot.set_status(|st| {
                    st.status = "error".into();
                    st.last_error = Some(err.clone());
                });
                Err(err)
            }
        }
    }

    pub async fn sync_server_catalog(
        &self,
        server_id: &str,
        settings_override: Option<&AppSettings>,
    ) -> Result<CachedUpstreamCatalog, String> {
        let settings = effective_settings(settings_override);
        let server = settings
            .upstream_mcp
            .servers
            .iter()
            .find(|s| s.id == server_id || s.alias.eq_ignore_ascii_case(server_id))
            .ok_or_else(|| format!("未找到上游 MCP 服务 `{server_id}`"))?
            .clone();

        let client = self.get_or_connect(&settings, &server, false).await?;
        let tools = client.list_tools().await?;
        let catalog = CachedUpstreamCatalog {
            synced_at_ms: unix_time_ms(),
            protocol_version: client.handshake.protocol_version.clone(),
            server_name: client.handshake.server_name.clone(),
            server_version: client.handshake.server_version.clone(),
            tools,
        };
        let slot = self.slot_for(&server.id);
        slot.tools_dirty.store(false, Ordering::Release);
        self.seed_catalog_cache(&server.id, catalog.clone());
        Ok(catalog)
    }

    /// Force-reconnect, run `initialize` + `tools/list`, refresh disk cache, and return full diagnostics.
    pub async fn test_and_sync_server(
        &self,
        server_id_or_alias: &str,
        settings_override: Option<&AppSettings>,
    ) -> UpstreamTestSyncResultDto {
        let settings = effective_settings(settings_override);
        let Some(server) = settings
            .upstream_mcp
            .servers
            .iter()
            .find(|s| {
                s.id == server_id_or_alias || s.alias.eq_ignore_ascii_case(server_id_or_alias)
            })
            .cloned()
        else {
            return UpstreamTestSyncResultDto {
                server_id: server_id_or_alias.to_string(),
                alias: server_id_or_alias.to_string(),
                ok: false,
                latency_ms: 0,
                protocol_version: String::new(),
                server_name: String::new(),
                server_version: String::new(),
                tool_count: 0,
                error: Some(format!("未找到上游 MCP 服务 `{server_id_or_alias}`")),
                stderr_tail: Vec::new(),
            };
        };

        let started = Instant::now();
        let slot = self.slot_for(&server.id);
        match self.get_or_connect(&settings, &server, true).await {
            Ok(client) => match client.list_tools().await {
                Ok(tools) => {
                    let elapsed = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
                    let catalog = CachedUpstreamCatalog {
                        synced_at_ms: unix_time_ms(),
                        protocol_version: client.handshake.protocol_version.clone(),
                        server_name: client.handshake.server_name.clone(),
                        server_version: client.handshake.server_version.clone(),
                        tools: tools.clone(),
                    };
                    let stderr_tail = client.stderr_tail();
                    slot.tools_dirty.store(false, Ordering::Release);
                    slot.set_status(|st| {
                        st.status = "connected".into();
                        st.last_error = None;
                        st.last_latency_ms = Some(elapsed);
                        st.stderr_tail = stderr_tail.clone();
                    });
                    self.seed_catalog_cache(&server.id, catalog);
                    UpstreamTestSyncResultDto {
                        server_id: server.id,
                        alias: server.alias,
                        ok: true,
                        latency_ms: elapsed,
                        protocol_version: client.handshake.protocol_version.clone(),
                        server_name: client.handshake.server_name.clone(),
                        server_version: client.handshake.server_version.clone(),
                        tool_count: tools.len(),
                        error: None,
                        stderr_tail,
                    }
                }
                Err(err) => {
                    let elapsed = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
                    let stderr_tail = client.stderr_tail();
                    slot.set_status(|st| {
                        st.status = "error".into();
                        st.last_error = Some(err.clone());
                        st.stderr_tail = stderr_tail.clone();
                    });
                    UpstreamTestSyncResultDto {
                        server_id: server.id,
                        alias: server.alias,
                        ok: false,
                        latency_ms: elapsed,
                        protocol_version: client.handshake.protocol_version.clone(),
                        server_name: client.handshake.server_name.clone(),
                        server_version: client.handshake.server_version.clone(),
                        tool_count: 0,
                        error: Some(err),
                        stderr_tail,
                    }
                }
            },
            Err(err) => {
                let elapsed = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
                let rt = slot.snapshot_status(true, None);
                UpstreamTestSyncResultDto {
                    server_id: server.id,
                    alias: server.alias,
                    ok: false,
                    latency_ms: elapsed,
                    protocol_version: String::new(),
                    server_name: String::new(),
                    server_version: String::new(),
                    tool_count: 0,
                    error: Some(err),
                    stderr_tail: rt.stderr_tail,
                }
            }
        }
    }

    pub fn overview(&self, settings_override: Option<&AppSettings>) -> UpstreamMcpOverviewDto {
        let settings = effective_settings(settings_override);
        let cfg = &settings.upstream_mcp;
        self.ensure_catalog_loaded();
        let catalogs = self.catalogs.read().unwrap_or_else(|e| e.into_inner());

        let builtin_tool_count = crate::tools::list_tools_for_profile("compact").len()
            + crate::mcp::gateway::gateway_tool_definitions().len()
            + crate::computer::tool_definitions().len();

        let (direct_tools, _, has_proxy) = if cfg.enabled {
            build_direct_index(cfg, &catalogs)
        } else {
            (Vec::new(), HashMap::new(), false)
        };
        let direct_exposed_tool_count = direct_tools.len();
        let proxy_meta_tool_count = if cfg.enabled && (has_proxy || cfg.always_expose_proxy_tools) {
            proxy_meta_tool_definitions().len()
        } else {
            0
        };

        let mut occupied = reserved_builtin_tool_names();
        let mut servers = Vec::with_capacity(cfg.servers.len());
        for srv in &cfg.servers {
            let cached = catalogs.get(&srv.id);
            let slot = self.slot_for(&srv.id);
            let runtime = slot.snapshot_status(cfg.enabled && srv.enabled, cached);
            let mut tools = Vec::new();
            let mut enabled_count = 0usize;
            if let Some(cat) = cached {
                for tool in &cat.tools {
                    let disabled = srv.is_tool_disabled(&tool.name);
                    if !disabled {
                        enabled_count += 1;
                    }
                    let exposed_name = compute_exposed_tool_name(&srv.alias, &tool.name, &occupied);
                    if srv.enabled && srv.expose_mode == UpstreamExposeMode::Direct && !disabled {
                        occupied.insert(exposed_name.clone());
                    }
                    tools.push(UpstreamToolItemDto {
                        original_name: tool.name.clone(),
                        exposed_name,
                        title: tool.title.clone(),
                        description: tool.description.clone(),
                        disabled,
                        input_schema: tool.input_schema.clone(),
                    });
                }
            }
            servers.push(UpstreamServerOverviewDto {
                config: srv.clone(),
                runtime,
                total_tool_count: tools.len(),
                enabled_tool_count: enabled_count,
                tools,
            });
        }

        UpstreamMcpOverviewDto {
            enabled: cfg.enabled,
            always_expose_proxy_tools: cfg.always_expose_proxy_tools,
            builtin_tool_count,
            direct_exposed_tool_count,
            proxy_meta_tool_count,
            total_exposed_tool_count: builtin_tool_count
                + direct_exposed_tool_count
                + proxy_meta_tool_count,
            direct_tools_warning_threshold: DIRECT_TOOLS_WARNING_THRESHOLD,
            servers,
        }
    }

    /// Reconcile live connections against current persisted settings:
    /// disconnect deleted, disabled, or modified servers while preserving healthy unchanged connections.
    pub async fn reload_config(&self, settings_override: Option<&AppSettings>) {
        let settings = effective_settings(settings_override);
        let active_map: HashMap<String, (&UpstreamServerConfig, String)> =
            if settings.upstream_mcp.enabled {
                settings
                    .upstream_mcp
                    .servers
                    .iter()
                    .filter(|s| s.enabled)
                    .map(|s| (s.id.clone(), (s, server_config_fingerprint(s, &settings))))
                    .collect()
            } else {
                HashMap::new()
            };

        let existing_ids: Vec<(String, Arc<UpstreamServerSlot>)> = self
            .slots
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(k, v)| (k.clone(), Arc::clone(v)))
            .collect();

        let configured_ids: HashSet<&str> = settings
            .upstream_mcp
            .servers
            .iter()
            .map(|s| s.id.as_str())
            .collect();

        for (id, slot) in existing_ids {
            let mut conn_guard = slot.connection.lock().await;
            let should_disconnect = match active_map.get(&id) {
                None => true,
                Some((_, fp)) => conn_guard
                    .as_ref()
                    .is_some_and(|active| active.config_fingerprint != *fp),
            };
            if should_disconnect {
                if let Some(old) = conn_guard.take() {
                    untrack_stdio_pid(old.client.stdio_pid());
                    old.client.shutdown().await;
                }
                slot.set_status(|st| {
                    st.status = if configured_ids.contains(id.as_str()) {
                        "idle".into()
                    } else {
                        "disabled".into()
                    };
                });
            }
        }

        // Remove deleted servers from slot map and catalog cache.
        self.slots
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|id, _| configured_ids.contains(id.as_str()));

        self.ensure_catalog_loaded();
        let removed_any = {
            let mut cats = self.catalogs.write().unwrap_or_else(|e| e.into_inner());
            let before = cats.len();
            cats.retain(|id, _| configured_ids.contains(id.as_str()));
            cats.len() != before
        };
        if removed_any {
            self.persist_catalog_best_effort();
        }
    }

    pub async fn disconnect_server(&self, server_id: &str, remove_cache: bool) {
        let slot = {
            let guard = self.slots.read().unwrap_or_else(|e| e.into_inner());
            guard.get(server_id).cloned()
        };
        if let Some(slot) = slot {
            let mut conn = slot.connection.lock().await;
            if let Some(active) = conn.take() {
                untrack_stdio_pid(active.client.stdio_pid());
                active.client.shutdown().await;
            }
            slot.set_status(|st| {
                st.status = "idle".into();
            });
        }
        if remove_cache {
            self.remove_catalog_cache(server_id);
        }
    }

    /// Reap idle stdio child processes that have exceeded their configured `idle_timeout_secs`.
    pub async fn reap_idle_clients(&self, settings_override: Option<&AppSettings>) -> usize {
        let settings = effective_settings(settings_override);
        let idle_limits: HashMap<&str, u64> = settings
            .upstream_mcp
            .servers
            .iter()
            .map(|s| (s.id.as_str(), s.idle_timeout_secs))
            .collect();
        let now_ms = unix_time_ms();
        let entries: Vec<(String, Arc<UpstreamServerSlot>)> = self
            .slots
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(k, v)| (k.clone(), Arc::clone(v)))
            .collect();

        let mut reaped = 0usize;
        for (id, slot) in entries {
            let limit_secs = idle_limits
                .get(id.as_str())
                .copied()
                .unwrap_or(super::model::DEFAULT_IDLE_TIMEOUT_SECS);
            if limit_secs == 0 {
                continue;
            }
            let Ok(mut conn_guard) = slot.connection.try_lock() else {
                continue;
            };
            let should_reap = conn_guard.as_ref().is_some_and(|active| {
                if !active.client.is_stdio() {
                    return false;
                }
                let last_used = active.last_used_ms.load(Ordering::Relaxed);
                now_ms.saturating_sub(last_used) >= limit_secs.saturating_mul(1000)
            });
            if should_reap {
                if let Some(active) = conn_guard.take() {
                    untrack_stdio_pid(active.client.stdio_pid());
                    active.client.shutdown().await;
                    slot.set_status(|st| {
                        st.status = "idle".into();
                    });
                    reaped += 1;
                }
            }
        }
        reaped
    }

    pub async fn shutdown_all(&self) {
        let entries: Vec<Arc<UpstreamServerSlot>> = self
            .slots
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect();
        for slot in entries {
            let mut conn = slot.connection.lock().await;
            if let Some(active) = conn.take() {
                untrack_stdio_pid(active.client.stdio_pid());
                active.client.shutdown().await;
            }
            slot.set_status(|st| {
                st.status = "idle".into();
            });
        }
    }
}

/// Validate and normalize an `UpstreamServerConfig` prior to persistence.
pub fn validate_and_normalize_server(
    server: &mut UpstreamServerConfig,
    existing_servers: &[UpstreamServerConfig],
    self_loop: &SelfLoopContext,
) -> Result<(), String> {
    if server.id.trim().is_empty() {
        server.id = format!("up-{}", &uuid::Uuid::new_v4().to_string()[..12]);
    }
    server.alias = normalize_alias(&server.alias)?;
    let dup_alias = existing_servers
        .iter()
        .any(|other| other.id != server.id && other.alias.eq_ignore_ascii_case(&server.alias));
    if dup_alias {
        return Err(format!("别名 `{}` 已被其他上游 MCP 服务占用", server.alias));
    }
    server.name = server.name.trim().to_string();
    if server.name.is_empty() {
        server.name = server.alias.clone();
    }
    server.description = server.description.trim().to_string();
    server.clamp_timeouts();

    match &mut server.transport {
        UpstreamTransportConfig::Stdio {
            command, args, cwd, ..
        } => {
            *command = command.trim().to_string();
            if command.is_empty() {
                return Err("Stdio MCP 启动命令不能为空".into());
            }
            *args = args
                .iter()
                .map(|a| a.to_string())
                .filter(|a| !a.is_empty())
                .collect();
            if let Some(dir) = cwd.as_mut() {
                let trimmed = dir.trim().to_string();
                if trimmed.is_empty() {
                    *cwd = None;
                } else {
                    *dir = trimmed;
                }
            }
        }
        UpstreamTransportConfig::StreamableHttp { url, .. }
        | UpstreamTransportConfig::Sse { url, .. } => {
            *url = validate_http_url(url)?;
        }
    }

    if let Some(reason) = detect_self_loop(&server.transport, self_loop) {
        return Err(reason);
    }

    Ok(())
}

fn effective_settings(override_snap: Option<&AppSettings>) -> AppSettings {
    override_snap
        .cloned()
        .unwrap_or_else(AppSettings::load_or_default)
}

fn build_direct_index(
    cfg: &UpstreamMcpConfig,
    catalogs: &HashMap<String, CachedUpstreamCatalog>,
) -> (Vec<Value>, HashMap<String, DirectToolRoute>, bool) {
    let mut occupied = reserved_builtin_tool_names();
    let mut tools = Vec::new();
    let mut route_map = HashMap::new();
    let mut has_proxy = false;

    for srv in &cfg.servers {
        if !srv.enabled {
            continue;
        }
        if srv.expose_mode == UpstreamExposeMode::Proxy {
            has_proxy = true;
            continue;
        }
        let Some(catalog) = catalogs.get(&srv.id) else {
            continue;
        };
        for tool in &catalog.tools {
            if srv.is_tool_disabled(&tool.name) {
                continue;
            }
            let exposed = compute_exposed_tool_name(&srv.alias, &tool.name, &occupied);
            occupied.insert(exposed.clone());
            tools.push(build_exposed_tool_definition(&exposed, srv, tool));
            route_map.insert(
                exposed,
                DirectToolRoute {
                    server_id: srv.id.clone(),
                    alias: srv.alias.clone(),
                    server_name: srv.name.clone(),
                    original_tool_name: tool.name.clone(),
                },
            );
        }
    }

    (tools, route_map, has_proxy)
}

fn resolve_secrets_for_server(
    server: &UpstreamServerConfig,
    settings: &AppSettings,
) -> ResolvedUpstreamSecrets {
    let mut env = BTreeMap::new();
    let mut headers = BTreeMap::new();
    let mut bearer_token = None;

    let read_secret = |scope: &str, item_id: &str| -> Option<String> {
        settings
            .app_secrets
            .get(scope)
            .and_then(|map| map.get(item_id))
            .filter(|v| !v.is_empty())
            .cloned()
            .or_else(|| SecretStore::get_app(scope, item_id).ok().flatten())
    };

    match &server.transport {
        UpstreamTransportConfig::Stdio {
            secret_env_keys, ..
        } => {
            for key in secret_env_keys {
                if let Some(val) = read_secret(
                    UPSTREAM_ENV_SECRETS_SCOPE,
                    &env_secret_item_id(&server.id, key),
                ) {
                    env.insert(key.clone(), val);
                }
            }
        }
        UpstreamTransportConfig::StreamableHttp {
            secret_header_keys,
            has_bearer_token,
            ..
        }
        | UpstreamTransportConfig::Sse {
            secret_header_keys,
            has_bearer_token,
            ..
        } => {
            for key in secret_header_keys {
                if let Some(val) = read_secret(
                    UPSTREAM_HEADER_SECRETS_SCOPE,
                    &header_secret_item_id(&server.id, key),
                ) {
                    headers.insert(key.clone(), val);
                }
            }
            if *has_bearer_token {
                bearer_token = read_secret(UPSTREAM_BEARER_SECRETS_SCOPE, &server.id);
            }
        }
    }

    ResolvedUpstreamSecrets {
        env,
        headers,
        bearer_token,
    }
}

fn server_config_fingerprint(server: &UpstreamServerConfig, settings: &AppSettings) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_vec(&server.transport).unwrap_or_default());
    let secrets = resolve_secrets_for_server(server, settings);
    for (k, v) in secrets.env {
        hasher.update(k.as_bytes());
        hasher.update(b"=");
        hasher.update(v.as_bytes());
        hasher.update(b"\0");
    }
    for (k, v) in secrets.headers {
        hasher.update(k.as_bytes());
        hasher.update(b":");
        hasher.update(v.as_bytes());
        hasher.update(b"\0");
    }
    if let Some(tok) = secrets.bearer_token {
        hasher.update(tok.as_bytes());
    }
    hasher.update(settings.proxy.mode.as_bytes());
    hasher.update(settings.proxy.url.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn parse_executable_paths(raw: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for line in raw.split(['\n', '\r', ',']) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let path = if trimmed == "~" {
            dirs::home_dir().unwrap_or_else(|| PathBuf::from(trimmed))
        } else if let Some(rest) = trimmed
            .strip_prefix("~/")
            .or_else(|| trimmed.strip_prefix("~\\"))
        {
            dirs::home_dir()
                .map(|h| h.join(rest))
                .unwrap_or_else(|| PathBuf::from(trimmed))
        } else {
            PathBuf::from(trimmed)
        };
        if !out.contains(&path) {
            out.push(path);
        }
    }
    out
}
