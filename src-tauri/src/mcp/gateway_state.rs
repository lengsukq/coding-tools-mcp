use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::agent_context::{merge_id_lists, merge_source_lists, AgentContextRuntimeConfig};
#[cfg(test)]
use crate::lan_peer::LAN_PEER_SECRETS_SCOPE;
use crate::settings::AppSettings;
#[cfg(test)]
use crate::settings::PairedPeerConfig;
use crate::tools::context::{merge_ai_instructions, merge_executable_paths};
use crate::tools::policy::PolicySettings;
use crate::tools::{SharedToolContext, ToolContext, Workspace};
use crate::usage::ServiceUsage;
use crate::workspace::{AuthConfig, WorkspaceProfile};

mod registry;
mod session_store;

pub use registry::{WorkspaceDescriptor, WorkspaceRegistry};
pub use session_store::GatewaySessionStore;

#[derive(Debug, Clone)]
pub struct GatewayError {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
}

impl GatewayError {
    pub fn invalid(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: false,
        }
    }

    pub fn not_found(code: &'static str, message: impl Into<String>) -> Self {
        Self::invalid(code, message)
    }

    pub fn internal(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: true,
        }
    }

    pub fn to_rpc_error(&self) -> Value {
        json!({
            "code": -32602,
            "message": self.message,
            "data": {
                "reason": self.code,
                "retryable": self.retryable
            }
        })
    }
}

#[derive(Clone)]
pub struct WorkspaceRequestContext {
    pub session_id: String,
    pub workspace_id: String,
    pub profile: WorkspaceProfile,
    pub tools: SharedToolContext,
}

/// Runtime state shared by the single MCP endpoint.
pub struct GatewayState {
    pub registry: WorkspaceRegistry,
    pub sessions: Arc<GatewaySessionStore>,
    usage: Mutex<HashMap<String, Arc<ServiceUsage>>>,
    settings_override: Arc<Mutex<Option<AppSettings>>>,
}

impl Default for GatewayState {
    fn default() -> Self {
        Self {
            registry: WorkspaceRegistry::default(),
            sessions: Arc::new(GatewaySessionStore::default()),
            usage: Mutex::new(HashMap::new()),
            settings_override: Arc::new(Mutex::new(None)),
        }
    }
}

impl GatewayState {
    #[cfg(test)]
    pub(crate) fn for_test_profiles(
        profiles: Vec<WorkspaceProfile>,
        settings: AppSettings,
    ) -> Self {
        Self {
            registry: WorkspaceRegistry::from_profiles(profiles),
            sessions: Arc::new(GatewaySessionStore::default()),
            usage: Mutex::new(HashMap::new()),
            settings_override: Arc::new(Mutex::new(Some(settings))),
        }
    }

    #[cfg(test)]
    pub(crate) fn clone_for_test(&self) -> Self {
        let usage_snapshot = self.usage.lock().unwrap_or_else(|e| e.into_inner()).clone();
        Self {
            registry: self.registry.clone(),
            sessions: self.sessions.clone(),
            usage: Mutex::new(usage_snapshot),
            settings_override: self.settings_override.clone(),
        }
    }

    pub fn settings_snapshot(&self) -> Option<AppSettings> {
        self.settings_override
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    #[cfg(test)]
    pub fn is_test_override(&self) -> bool {
        self.settings_override
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    #[cfg(test)]
    pub fn register_test_paired_peer(&self, peer: PairedPeerConfig, shared_secret: String) {
        let mut guard = self
            .settings_override
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(settings) = guard.as_mut() {
            settings.lan_federation.enabled = true;
            if let Some(existing) = settings
                .lan_federation
                .paired_peers
                .iter_mut()
                .find(|p| p.device_id == peer.device_id)
            {
                *existing = peer.clone();
            } else {
                settings.lan_federation.paired_peers.push(peer.clone());
            }
            settings
                .app_secrets
                .entry(LAN_PEER_SECRETS_SCOPE.to_string())
                .or_default()
                .insert(peer.device_id, shared_secret);
        }
    }

    pub fn usage_for(&self, workspace_id: &str) -> Arc<ServiceUsage> {
        self.usage
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entry(workspace_id.to_string())
            .or_insert_with(|| Arc::new(ServiceUsage::default()))
            .clone()
    }

    pub fn drop_workspace(&self, workspace_id: &str) {
        self.sessions.clear_workspace(workspace_id);
        let mut usage = self
            .usage
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        usage.remove(workspace_id);
        if usage.capacity() > 32 && usage.len() * 4 < usage.capacity() {
            usage.shrink_to_fit();
        }
    }

    pub fn select_workspace(
        &self,
        session_id: &str,
        workspace_id: &str,
    ) -> Result<WorkspaceRequestContext, GatewayError> {
        if !self.sessions.contains(session_id) {
            return Err(GatewayError::invalid(
                "MCP_SESSION_INVALID",
                "MCP session 不存在或已过期。请重新建立 MCP 连接并使用服务端返回的 session 标识。",
            ));
        }
        let profile = self.registry.resolve(workspace_id)?;
        let context = self.build_request_context(session_id, profile)?;
        if !self
            .sessions
            .select(session_id, context.workspace_id.clone())
        {
            return Err(GatewayError::invalid(
                "MCP_SESSION_INVALID",
                "MCP session 不存在或已过期。请重新建立 MCP 连接并使用服务端返回的 session 标识。",
            ));
        }
        Ok(context)
    }

    pub fn active_workspace(
        &self,
        session_id: &str,
    ) -> Result<WorkspaceRequestContext, GatewayError> {
        let scope = self.sessions.current(session_id);
        let workspace_id = scope.active_workspace_id.ok_or_else(|| {
            GatewayError::invalid(
                "WORKSPACE_NOT_SELECTED",
                "当前 MCP 会话尚未选择 Workspace。请先调用 workspace_list，再调用 workspace_select。",
            )
        })?;
        match self.registry.resolve(&workspace_id) {
            Ok(profile) => self.build_request_context(session_id, profile),
            Err(error) => {
                self.sessions.clear_selection(session_id);
                Err(error)
            }
        }
    }

    pub fn workspace_by_id(
        &self,
        session_id: &str,
        workspace_id: &str,
    ) -> Result<WorkspaceRequestContext, GatewayError> {
        let profile = self.registry.resolve(workspace_id)?;
        self.build_request_context(session_id, profile)
    }

    fn build_request_context(
        &self,
        session_id: &str,
        profile: WorkspaceProfile,
    ) -> Result<WorkspaceRequestContext, GatewayError> {
        let global = self
            .settings_snapshot()
            .unwrap_or_else(AppSettings::load_or_default);
        let workspace = Workspace::new(profile.path.clone().into()).map_err(|error| {
            GatewayError::invalid("WORKSPACE_UNAVAILABLE", error.message().to_string())
        })?;
        let policy = PolicySettings::from_runtime_and_global(&profile.runtime, &global);
        let executable_paths = merge_executable_paths(
            &profile.runtime.executable_paths,
            &global.global_executable_paths,
        );
        let instruction_sources = merge_source_lists(
            &global.global_instruction_sources,
            &profile.runtime.instruction_sources,
        );
        let skill_sources =
            merge_source_lists(&global.global_skill_sources, &profile.runtime.skill_sources);
        let agent_context = AgentContextRuntimeConfig {
            instruction_sources,
            skill_sources,
            custom_instruction_paths: merge_config_text(
                &global.global_custom_instruction_paths,
                &profile.runtime.custom_instruction_paths,
            ),
            custom_skill_paths: merge_config_text(
                &global.global_custom_skill_paths,
                &profile.runtime.custom_skill_paths,
            ),
            disabled_instruction_ids: merge_id_lists(
                &global.global_disabled_instruction_ids,
                &profile.runtime.disabled_instruction_ids,
            ),
            disabled_skill_ids: merge_id_lists(
                &global.global_disabled_skill_ids,
                &profile.runtime.disabled_skill_ids,
            ),
        };
        let ai_instructions = merge_ai_instructions(
            &global.global_ai_instructions,
            &profile.runtime.ai_instructions,
        );
        let usage = self.usage_for(&profile.id);
        let tools = Arc::new(
            ToolContext::from_workspace(
                workspace,
                AuthConfig {
                    auth_type: global.global_mcp_auth_type.as_str().into(),
                    oauth_client_id: String::new(),
                    use_shared_secrets: true,
                },
                policy,
                profile.runtime.tool_profile.clone(),
            )
            .with_agent_runtime(executable_paths, ai_instructions)
            .with_agent_context(agent_context)
            .with_history_config(
                profile.runtime.history_recording,
                profile.runtime.history_context_sessions.clone(),
            )
            .with_usage(usage),
        );
        Ok(WorkspaceRequestContext {
            session_id: session_id.to_string(),
            workspace_id: profile.id.clone(),
            profile,
            tools,
        })
    }
}

fn merge_config_text(global: &str, workspace: &str) -> String {
    [global.trim(), workspace.trim()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
