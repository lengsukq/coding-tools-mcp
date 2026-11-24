use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock as StdRwLock};

use crate::settings::AppSettings;

use super::model::{
    detect_self_loop, validate_http_url, CachedUpstreamCatalog, SelfLoopContext,
    UpstreamCatalogCacheFile, UpstreamExposeMode, UpstreamMcpOverviewDto, UpstreamServerConfig,
    UpstreamServerOverviewDto, UpstreamToolItemDto, UpstreamTransportConfig,
    DIRECT_TOOLS_WARNING_THRESHOLD,
};
use super::naming::{compute_exposed_tool_name, normalize_alias, reserved_builtin_tool_names};
use super::proxy_tools::proxy_meta_tool_definitions;

mod pool;
mod proxy;
mod routing;

pub use pool::terminate_all_managed_children_sync;
pub use routing::DirectToolRoute;

use pool::UpstreamServerSlot;
use routing::CachedDirectIndex;

pub struct UpstreamCallOutcome {
    pub result: serde_json::Value,
    pub execution_host: String,
}

enum CachePersistence {
    DefaultDisk,
    #[cfg(test)]
    InMemoryOnly,
}

pub struct UpstreamHub {
    persistence: CachePersistence,
    catalog_loaded: AtomicBool,
    catalog_revision: AtomicU64,
    catalogs: StdRwLock<HashMap<String, CachedUpstreamCatalog>>,
    direct_index_cache: StdRwLock<Option<Arc<CachedDirectIndex>>>,
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
            catalog_revision: AtomicU64::new(0),
            catalogs: StdRwLock::new(HashMap::new()),
            direct_index_cache: StdRwLock::new(None),
            slots: StdRwLock::new(HashMap::new()),
        }
    }

    #[cfg(test)]
    pub fn new_in_memory() -> Self {
        Self {
            persistence: CachePersistence::InMemoryOnly,
            catalog_loaded: AtomicBool::new(true),
            catalog_revision: AtomicU64::new(0),
            catalogs: StdRwLock::new(HashMap::new()),
            direct_index_cache: StdRwLock::new(None),
            slots: StdRwLock::new(HashMap::new()),
        }
    }

    pub(super) fn bump_catalog_revision(&self) {
        self.catalog_revision.fetch_add(1, Ordering::AcqRel);
        *self
            .direct_index_cache
            .write()
            .unwrap_or_else(|e| e.into_inner()) = None;
    }

    pub(super) fn ensure_catalog_loaded(&self) {
        if self.catalog_loaded.load(Ordering::Acquire) {
            return;
        }
        let path = match &self.persistence {
            #[cfg(test)]
            CachePersistence::InMemoryOnly => {
                self.catalog_loaded.store(true, Ordering::Release);
                return;
            }
            CachePersistence::DefaultDisk => UpstreamCatalogCacheFile::default_path().ok(),
        };
        let loaded = path
            .as_deref()
            .map(UpstreamCatalogCacheFile::load_at)
            .unwrap_or_default();
        let mut guard = self.catalogs.write().unwrap_or_else(|e| e.into_inner());
        let mut loaded_any = false;
        if !self.catalog_loaded.swap(true, Ordering::AcqRel) {
            for (k, v) in loaded.catalogs {
                guard.entry(k).or_insert(v);
                loaded_any = true;
            }
        }
        drop(guard);
        if loaded_any {
            self.bump_catalog_revision();
        }
    }

    pub(super) fn persist_catalog_best_effort(&self) {
        let path = match &self.persistence {
            #[cfg(test)]
            CachePersistence::InMemoryOnly => return,
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
        self.bump_catalog_revision();
        self.persist_catalog_best_effort();
    }

    pub(super) fn remove_catalog_cache(&self, server_id: &str) {
        self.ensure_catalog_loaded();
        let removed = self
            .catalogs
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(server_id)
            .is_some();
        if removed {
            self.bump_catalog_revision();
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

    pub fn overview(&self, settings_override: Option<&AppSettings>) -> UpstreamMcpOverviewDto {
        let settings = effective_settings(settings_override);
        let cfg = &settings.upstream_mcp;
        self.ensure_catalog_loaded();
        let catalogs = self.catalogs.read().unwrap_or_else(|e| e.into_inner());

        let builtin_tool_count = crate::tools::list_tools_for_profile("compact").len()
            + crate::mcp::gateway::gateway_tool_definitions().len()
            + crate::computer::tool_definitions().len();

        let (direct_exposed_tool_count, has_proxy) = if cfg.enabled {
            let index = self.cached_direct_index(cfg);
            (index.tools.len(), index.has_proxy)
        } else {
            (0, false)
        };
        let proxy_meta_tool_count = if cfg.enabled && (has_proxy || cfg.always_expose_proxy_tools) {
            proxy_meta_tool_definitions().len()
        } else {
            0
        };

        let mut occupied = reserved_builtin_tool_names();
        let servers = cfg
            .servers
            .iter()
            .map(|srv| {
                self.build_server_overview(srv, cfg.enabled, catalogs.get(&srv.id), &mut occupied)
            })
            .collect();

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

    fn build_server_overview(
        &self,
        srv: &UpstreamServerConfig,
        hub_enabled: bool,
        cached: Option<&CachedUpstreamCatalog>,
        occupied: &mut HashSet<String>,
    ) -> UpstreamServerOverviewDto {
        let slot = self.slot_for(&srv.id);
        let runtime = slot.snapshot_status(hub_enabled && srv.enabled, cached);
        let mut tools = Vec::new();
        let mut enabled_count = 0usize;
        if let Some(cat) = cached {
            for tool in &cat.tools {
                let disabled = srv.is_tool_disabled(&tool.name);
                if !disabled {
                    enabled_count += 1;
                }
                let exposed_name = compute_exposed_tool_name(&srv.alias, &tool.name, occupied);
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
        UpstreamServerOverviewDto {
            config: srv.clone(),
            runtime,
            total_tool_count: tools.len(),
            enabled_tool_count: enabled_count,
            tools,
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

pub(super) fn effective_settings(override_snap: Option<&AppSettings>) -> AppSettings {
    override_snap
        .cloned()
        .unwrap_or_else(AppSettings::load_or_default)
}
