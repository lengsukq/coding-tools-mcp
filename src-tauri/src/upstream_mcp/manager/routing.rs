use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::Value;

use crate::secret::SecretStore;
use crate::settings::AppSettings;

use super::super::client::ResolvedUpstreamSecrets;
use super::super::model::{
    env_secret_item_id, header_secret_item_id, CachedUpstreamCatalog, UpstreamExposeMode,
    UpstreamMcpConfig, UpstreamServerConfig, UpstreamTransportConfig,
    UPSTREAM_BEARER_SECRETS_SCOPE, UPSTREAM_ENV_SECRETS_SCOPE, UPSTREAM_HEADER_SECRETS_SCOPE,
};
use super::super::naming::{
    build_exposed_tool_definition, compute_exposed_tool_name, reserved_builtin_tool_names,
};
use super::super::proxy_tools::{is_proxy_meta_tool, proxy_meta_tool_definitions};
use super::{effective_settings, UpstreamHub};

#[derive(Debug, Clone)]
pub struct DirectToolRoute {
    pub server_id: String,
    pub alias: String,
    pub server_name: String,
    pub original_tool_name: String,
}

pub(super) struct CachedDirectIndex {
    config_hash: u64,
    catalog_revision: u64,
    pub(super) tools: Arc<Vec<Value>>,
    pub(super) route_map: Arc<HashMap<String, DirectToolRoute>>,
    pub(super) has_proxy: bool,
}

impl UpstreamHub {
    pub(super) fn cached_direct_index(&self, cfg: &UpstreamMcpConfig) -> Arc<CachedDirectIndex> {
        self.ensure_catalog_loaded();
        let config_hash = direct_index_config_hash(cfg);
        let catalog_revision = self.catalog_revision.load(Ordering::Acquire);
        if let Some(cached) = self
            .direct_index_cache
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            if cached.config_hash == config_hash && cached.catalog_revision == catalog_revision {
                return Arc::clone(cached);
            }
        }
        let catalogs = self.catalogs.read().unwrap_or_else(|e| e.into_inner());
        let (tools, route_map, has_proxy) = build_direct_index(cfg, &catalogs);
        let built = Arc::new(CachedDirectIndex {
            config_hash,
            catalog_revision,
            tools: Arc::new(tools),
            route_map: Arc::new(route_map),
            has_proxy,
        });
        *self
            .direct_index_cache
            .write()
            .unwrap_or_else(|e| e.into_inner()) = Some(Arc::clone(&built));
        built
    }

    /// Build the aggregated tools to append to Global MCP `tools/list`.
    /// Fast, non-blocking read from cached catalogs so `tools/list` never stalls on cold-starting stdio processes.
    pub fn exposed_tool_definitions(&self, settings_override: Option<&AppSettings>) -> Vec<Value> {
        let settings = effective_settings(settings_override);
        let cfg = &settings.upstream_mcp;
        if !cfg.enabled {
            return Vec::new();
        }
        let index = self.cached_direct_index(cfg);
        let mut out = (*index.tools).clone();
        if index.has_proxy || cfg.always_expose_proxy_tools {
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
        let index = self.cached_direct_index(cfg);
        index.route_map.get(exposed_name).cloned()
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
}

fn direct_index_config_hash(cfg: &UpstreamMcpConfig) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    cfg.enabled.hash(&mut hasher);
    cfg.servers.len().hash(&mut hasher);
    for srv in &cfg.servers {
        srv.id.hash(&mut hasher);
        srv.alias.hash(&mut hasher);
        srv.name.hash(&mut hasher);
        srv.enabled.hash(&mut hasher);
        matches!(srv.expose_mode, UpstreamExposeMode::Direct).hash(&mut hasher);
        srv.disabled_tools.hash(&mut hasher);
    }
    hasher.finish()
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

pub(super) fn resolve_secrets_for_server(
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

pub(super) fn server_config_fingerprint(
    server: &UpstreamServerConfig,
    settings: &AppSettings,
) -> String {
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
    crate::util::hash::encode_hex_lower(&hasher.finalize())
}
