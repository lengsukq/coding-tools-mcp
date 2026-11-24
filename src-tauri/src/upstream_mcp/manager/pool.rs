use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex as StdMutex};
use std::time::{Duration, Instant};

use tokio::sync::Mutex as AsyncMutex;

use crate::platform::platform;
use crate::settings::AppSettings;

use super::super::client::UpstreamClient;
use super::super::model::{
    detect_self_loop, unix_time_ms, CachedUpstreamCatalog, SelfLoopContext, UpstreamExposeMode,
    UpstreamServerConfig, UpstreamServerRuntimeDto, UpstreamTestSyncResultDto,
    DEFAULT_IDLE_TIMEOUT_SECS,
};
use super::routing::{resolve_secrets_for_server, server_config_fingerprint};
use super::{effective_settings, UpstreamHub};

/// Global PID tracker for managed Stdio upstream child processes so synchronous
/// process-exit hooks (`tauri::RunEvent::Exit`) can terminate every child tree
/// cleanly even if Tokio is already shutting down.
static MANAGED_STDIO_PIDS: LazyLock<StdMutex<HashSet<u32>>> =
    LazyLock::new(|| StdMutex::new(HashSet::new()));

pub(super) fn track_stdio_pid(pid: Option<u32>) {
    if let Some(pid) = pid {
        MANAGED_STDIO_PIDS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pid);
    }
}

pub(super) fn untrack_stdio_pid(pid: Option<u32>) {
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

pub(super) struct ActiveUpstream {
    pub(super) client: Arc<UpstreamClient>,
    pub(super) config_fingerprint: String,
    pub(super) last_used_ms: AtomicU64,
}

#[derive(Debug, Clone, Default)]
pub(super) struct SlotRuntimeStatus {
    pub(super) status: String,
    pub(super) last_error: Option<String>,
    pub(super) last_latency_ms: Option<u64>,
    pub(super) last_used_ms: Option<u64>,
    pub(super) protocol_version: Option<String>,
    pub(super) server_name: Option<String>,
    pub(super) server_version: Option<String>,
    pub(super) stderr_tail: Vec<String>,
}

pub(super) struct UpstreamServerSlot {
    pub(super) connection: AsyncMutex<Option<ActiveUpstream>>,
    status: StdMutex<SlotRuntimeStatus>,
    pub(super) tools_dirty: AtomicBool,
}

impl UpstreamServerSlot {
    pub(super) fn new() -> Self {
        Self {
            connection: AsyncMutex::new(None),
            status: StdMutex::new(SlotRuntimeStatus {
                status: "idle".into(),
                ..SlotRuntimeStatus::default()
            }),
            tools_dirty: AtomicBool::new(false),
        }
    }

    pub(super) fn set_status(&self, f: impl FnOnce(&mut SlotRuntimeStatus)) {
        let mut guard = self.status.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard);
    }

    pub(super) fn snapshot_status(
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

impl UpstreamHub {
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

    pub(super) async fn get_or_connect(
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
        let extra_paths =
            crate::tools::context::merge_executable_paths("", &settings.global_executable_paths);
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
            self.bump_catalog_revision();
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
                .unwrap_or(DEFAULT_IDLE_TIMEOUT_SECS);
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
