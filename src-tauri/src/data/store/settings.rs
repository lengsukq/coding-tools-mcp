use crate::error::AppResult;
use crate::settings::{AppSettings, FrpProfile};
use crate::upstream_mcp::{
    env_secret_item_id, header_secret_item_id, UpstreamSecretPatch, UpstreamServerConfig,
    UpstreamTransportConfig, UPSTREAM_BEARER_SECRETS_SCOPE, UPSTREAM_ENV_SECRETS_SCOPE,
    UPSTREAM_HEADER_SECRETS_SCOPE,
};

use super::super::model::AppData;
use super::DataStore;

const FRP_PROFILE_TOKEN_SCOPE: &str = "frp_profile_token";

#[cfg(test)]
pub(super) fn apply_settings_update(base: &AppData, latest: &mut AppData, settings: AppSettings) {
    let gateway_changed = settings.global_gateway != base.global_gateway;
    let lan_changed = settings.lan_federation != base.lan_federation;
    let upstream_changed = settings.upstream_mcp != base.upstream_mcp;
    apply_settings_update_with_flags(
        latest,
        settings,
        gateway_changed,
        lan_changed,
        upstream_changed,
    );
}

fn apply_settings_update_with_flags(
    latest: &mut AppData,
    settings: AppSettings,
    gateway_changed_by_caller: bool,
    lan_changed_by_caller: bool,
    upstream_changed_by_caller: bool,
) {
    // Global Gateway, LAN Peer Pairing, and Upstream MCP Hub can persist state
    // outside a long-lived DataStore snapshot. Unrelated settings saves must
    // preserve those newer values instead of overwriting them.
    let latest_gateway = (!gateway_changed_by_caller).then(|| latest.global_gateway.clone());
    let latest_lan = (!lan_changed_by_caller).then(|| latest.lan_federation.clone());
    let latest_upstream = (!upstream_changed_by_caller).then(|| latest.upstream_mcp.clone());
    settings.apply_to(latest);
    if let Some(gateway) = latest_gateway {
        latest.global_gateway = gateway;
    }
    if let Some(lan) = latest_lan {
        latest.lan_federation = lan;
    }
    if let Some(upstream) = latest_upstream {
        latest.upstream_mcp = upstream;
    }
}

impl DataStore {
    pub fn settings(&self) -> AppSettings {
        AppSettings::from_data(&self.data)
    }

    pub fn update_settings(&mut self, settings: AppSettings) -> AppResult<()> {
        let gateway_changed = settings.global_gateway != self.data.global_gateway;
        let lan_changed = settings.lan_federation != self.data.lan_federation;
        let upstream_changed = settings.upstream_mcp != self.data.upstream_mcp;
        self.update_latest(move |data| {
            apply_settings_update_with_flags(
                data,
                settings,
                gateway_changed,
                lan_changed,
                upstream_changed,
            );
            Ok(())
        })
    }

    /// Persist an upstream MCP server and its secret patch in a single atomic write.
    pub fn save_upstream_server(
        &mut self,
        server: UpstreamServerConfig,
        secret_patch: UpstreamSecretPatch,
    ) -> AppResult<UpstreamServerConfig> {
        self.update_latest(move |data| {
            let mut server = server;
            let id = server.id.clone();

            apply_upstream_secret_patch(data, &id, &mut server, secret_patch);

            if let Some(existing) = data
                .upstream_mcp
                .servers
                .iter_mut()
                .find(|item| item.id == id)
            {
                *existing = server.clone();
            } else {
                data.upstream_mcp.servers.push(server.clone());
            }
            Ok(server)
        })
    }

    /// Remove an upstream MCP server and all of its stored secrets atomically.
    pub fn delete_upstream_server(&mut self, id: &str) -> AppResult<bool> {
        let target_id = id.to_string();
        self.update_latest_if_changed(move |data| {
            let before_len = data.upstream_mcp.servers.len();
            data.upstream_mcp
                .servers
                .retain(|server| server.id != target_id);
            let removed_server = data.upstream_mcp.servers.len() != before_len;
            let removed_secrets = purge_upstream_secrets_in_data(data, &target_id);
            Ok((removed_server || removed_secrets, removed_server))
        })
    }

    /// Update global MCP aggregation switches atomically without clobbering server rows.
    pub fn set_upstream_global_settings(
        &mut self,
        enabled: bool,
        always_expose_proxy_tools: bool,
    ) -> AppResult<()> {
        self.update_latest_if_changed(move |data| {
            let changed = data.upstream_mcp.enabled != enabled
                || data.upstream_mcp.always_expose_proxy_tools != always_expose_proxy_tools;
            data.upstream_mcp.enabled = enabled;
            data.upstream_mcp.always_expose_proxy_tools = always_expose_proxy_tools;
            Ok((changed, ()))
        })
    }

    /// Persist an FRP profile and its optional token in a single atomic write.
    /// Returns whether a non-empty token is stored for this profile after saving.
    pub fn save_frp_profile(
        &mut self,
        profile: FrpProfile,
        token: Option<&str>,
    ) -> AppResult<bool> {
        let trimmed_token = token
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        self.update_latest(move |data| {
            let profile_id = profile.id.clone();
            if let Some(existing) = data
                .frp_profiles
                .iter_mut()
                .find(|item| item.id == profile_id)
            {
                *existing = profile;
            } else {
                data.frp_profiles.push(profile);
            }
            if let Some(secret) = trimmed_token {
                data.app_secrets
                    .entry(FRP_PROFILE_TOKEN_SCOPE.to_string())
                    .or_default()
                    .insert(profile_id.clone(), secret);
            }
            let has_token = data
                .app_secrets
                .get(FRP_PROFILE_TOKEN_SCOPE)
                .and_then(|items| items.get(&profile_id))
                .is_some_and(|value| !value.trim().is_empty());
            Ok(has_token)
        })
    }

    /// Remove an FRP profile and its associated token in a single atomic write.
    pub fn delete_frp_profile(&mut self, id: &str) -> AppResult<()> {
        self.update_latest_if_changed(|data| {
            let before_len = data.frp_profiles.len();
            data.frp_profiles.retain(|profile| profile.id != id);
            let profile_removed = data.frp_profiles.len() != before_len;

            let mut secret_removed = false;
            if let Some(items) = data.app_secrets.get_mut(FRP_PROFILE_TOKEN_SCOPE) {
                secret_removed = items.remove(id).is_some();
                if items.is_empty() {
                    data.app_secrets.remove(FRP_PROFILE_TOKEN_SCOPE);
                }
            }
            Ok((profile_removed || secret_removed, ()))
        })
    }

    pub fn save_upstream_server_file(
        server: UpstreamServerConfig,
        secret_patch: UpstreamSecretPatch,
    ) -> AppResult<UpstreamServerConfig> {
        Self::update_file(move |data| {
            let mut server = server;
            let id = server.id.clone();
            apply_upstream_secret_patch(data, &id, &mut server, secret_patch);
            if let Some(existing) = data
                .upstream_mcp
                .servers
                .iter_mut()
                .find(|item| item.id == id)
            {
                *existing = server.clone();
            } else {
                data.upstream_mcp.servers.push(server.clone());
            }
            Ok(server)
        })
    }

    pub fn delete_upstream_server_file(id: &str) -> AppResult<bool> {
        let target_id = id.to_string();
        Self::update_file_if_changed(move |data| {
            let before_len = data.upstream_mcp.servers.len();
            data.upstream_mcp
                .servers
                .retain(|server| server.id != target_id);
            let removed_server = data.upstream_mcp.servers.len() != before_len;
            let removed_secrets = purge_upstream_secrets_in_data(data, &target_id);
            Ok((removed_server || removed_secrets, removed_server))
        })
    }
}

fn apply_upstream_secret_patch(
    data: &mut AppData,
    server_id: &str,
    server: &mut UpstreamServerConfig,
    patch: UpstreamSecretPatch,
) {
    // 1. Env secrets
    if !patch.remove_env_secrets.is_empty() {
        if let Some(map) = data.app_secrets.get_mut(UPSTREAM_ENV_SECRETS_SCOPE) {
            for key in &patch.remove_env_secrets {
                map.remove(&env_secret_item_id(server_id, key.trim()));
            }
            if map.is_empty() {
                data.app_secrets.remove(UPSTREAM_ENV_SECRETS_SCOPE);
            }
        }
    }
    for (key, value) in &patch.set_env_secrets {
        let clean_key = key.trim();
        if clean_key.is_empty() {
            continue;
        }
        if value.is_empty() {
            if let Some(map) = data.app_secrets.get_mut(UPSTREAM_ENV_SECRETS_SCOPE) {
                map.remove(&env_secret_item_id(server_id, clean_key));
                if map.is_empty() {
                    data.app_secrets.remove(UPSTREAM_ENV_SECRETS_SCOPE);
                }
            }
        } else {
            data.app_secrets
                .entry(UPSTREAM_ENV_SECRETS_SCOPE.to_string())
                .or_default()
                .insert(env_secret_item_id(server_id, clean_key), value.clone());
        }
    }

    // 2. Header secrets
    if !patch.remove_header_secrets.is_empty() {
        if let Some(map) = data.app_secrets.get_mut(UPSTREAM_HEADER_SECRETS_SCOPE) {
            for key in &patch.remove_header_secrets {
                map.remove(&header_secret_item_id(server_id, key.trim()));
            }
            if map.is_empty() {
                data.app_secrets.remove(UPSTREAM_HEADER_SECRETS_SCOPE);
            }
        }
    }
    for (key, value) in &patch.set_header_secrets {
        let clean_key = key.trim();
        if clean_key.is_empty() {
            continue;
        }
        if value.is_empty() {
            if let Some(map) = data.app_secrets.get_mut(UPSTREAM_HEADER_SECRETS_SCOPE) {
                map.remove(&header_secret_item_id(server_id, clean_key));
                if map.is_empty() {
                    data.app_secrets.remove(UPSTREAM_HEADER_SECRETS_SCOPE);
                }
            }
        } else {
            data.app_secrets
                .entry(UPSTREAM_HEADER_SECRETS_SCOPE.to_string())
                .or_default()
                .insert(header_secret_item_id(server_id, clean_key), value.clone());
        }
    }

    // 3. Bearer token
    if let Some(maybe_token) = patch.bearer_token {
        match maybe_token
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        {
            Some(token) => {
                data.app_secrets
                    .entry(UPSTREAM_BEARER_SECRETS_SCOPE.to_string())
                    .or_default()
                    .insert(server_id.to_string(), token);
            }
            None => {
                if let Some(map) = data.app_secrets.get_mut(UPSTREAM_BEARER_SECRETS_SCOPE) {
                    map.remove(server_id);
                    if map.is_empty() {
                        data.app_secrets.remove(UPSTREAM_BEARER_SECRETS_SCOPE);
                    }
                }
            }
        }
    }

    // Recompute authoritative secret key lists / flags from stored secrets so config and secret store never drift.
    let prefix = format!("{server_id}:");
    let stored_env_keys: Vec<String> = data
        .app_secrets
        .get(UPSTREAM_ENV_SECRETS_SCOPE)
        .map(|map| {
            let mut keys: Vec<String> = map
                .iter()
                .filter_map(|(k, v)| {
                    if v.is_empty() {
                        None
                    } else {
                        k.strip_prefix(&prefix).map(str::to_string)
                    }
                })
                .collect();
            keys.sort();
            keys
        })
        .unwrap_or_default();
    let stored_header_keys: Vec<String> = data
        .app_secrets
        .get(UPSTREAM_HEADER_SECRETS_SCOPE)
        .map(|map| {
            let mut keys: Vec<String> = map
                .iter()
                .filter_map(|(k, v)| {
                    if v.is_empty() {
                        None
                    } else {
                        k.strip_prefix(&prefix).map(str::to_string)
                    }
                })
                .collect();
            keys.sort();
            keys
        })
        .unwrap_or_default();
    let has_bearer = data
        .app_secrets
        .get(UPSTREAM_BEARER_SECRETS_SCOPE)
        .and_then(|map| map.get(server_id))
        .is_some_and(|v| !v.trim().is_empty());

    match &mut server.transport {
        UpstreamTransportConfig::Stdio {
            secret_env_keys,
            env,
            ..
        } => {
            *secret_env_keys = stored_env_keys;
            // Ensure secret keys are never duplicated in plaintext env.
            for secret_key in secret_env_keys.iter() {
                env.remove(secret_key);
            }
        }
        UpstreamTransportConfig::StreamableHttp {
            secret_header_keys,
            headers,
            has_bearer_token,
            ..
        }
        | UpstreamTransportConfig::Sse {
            secret_header_keys,
            headers,
            has_bearer_token,
            ..
        } => {
            *secret_header_keys = stored_header_keys;
            *has_bearer_token = has_bearer;
            for secret_key in secret_header_keys.iter() {
                headers.remove(secret_key);
            }
        }
    }
}

fn purge_upstream_secrets_in_data(data: &mut AppData, server_id: &str) -> bool {
    let prefix = format!("{server_id}:");
    let mut removed = false;
    for scope in [UPSTREAM_ENV_SECRETS_SCOPE, UPSTREAM_HEADER_SECRETS_SCOPE] {
        if let Some(map) = data.app_secrets.get_mut(scope) {
            let before = map.len();
            map.retain(|k, _| !k.starts_with(&prefix));
            if map.len() != before {
                removed = true;
            }
            if map.is_empty() {
                data.app_secrets.remove(scope);
            }
        }
    }
    if let Some(map) = data.app_secrets.get_mut(UPSTREAM_BEARER_SECRETS_SCOPE) {
        if map.remove(server_id).is_some() {
            removed = true;
        }
        if map.is_empty() {
            data.app_secrets.remove(UPSTREAM_BEARER_SECRETS_SCOPE);
        }
    }
    removed
}
