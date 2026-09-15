use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::{json, Value};

use crate::agent_context::{merge_source_lists, AgentContextRuntimeConfig};
use crate::data::DataStore;
use crate::settings::AppSettings;
use crate::tools::context::{merge_ai_instructions, merge_executable_paths};
use crate::tools::policy::PolicySettings;
use crate::tools::{SharedToolContext, ToolContext, Workspace};
use crate::usage::ServiceUsage;
use crate::workspace::{AuthConfig, WorkspaceProfile};

const SESSION_TTL_SECONDS: u64 = 24 * 60 * 60;

fn unix_timestamp_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDescriptor {
    pub id: String,
    pub name: String,
    pub path: String,
}

impl From<&WorkspaceProfile> for WorkspaceDescriptor {
    fn from(profile: &WorkspaceProfile) -> Self {
        Self {
            id: profile.id.clone(),
            name: profile.name.clone(),
            path: profile.path.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceRegistry {
    profiles_override: Option<Arc<Vec<WorkspaceProfile>>>,
}

impl WorkspaceRegistry {
    pub fn list(&self) -> Result<Vec<WorkspaceProfile>, String> {
        if let Some(profiles) = &self.profiles_override {
            return Ok(profiles.as_ref().clone());
        }
        DataStore::read_file(|data| Ok(data.profiles.clone())).map_err(|error| error.to_string())
    }

    pub fn descriptors(&self) -> Result<Vec<WorkspaceDescriptor>, String> {
        let mut items = self
            .list()?
            .iter()
            .map(WorkspaceDescriptor::from)
            .collect::<Vec<_>>();
        items.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
        Ok(items)
    }

    pub fn resolve(&self, workspace_id: &str) -> Result<WorkspaceProfile, GatewayError> {
        self.list()
            .map_err(|message| GatewayError::internal("WORKSPACE_REGISTRY_UNAVAILABLE", message))?
            .into_iter()
            .find(|profile| profile.id == workspace_id)
            .ok_or_else(|| {
                GatewayError::not_found(
                    "WORKSPACE_NOT_FOUND",
                    format!("Workspace 不存在或已被删除: {workspace_id}"),
                )
            })
    }

    pub fn revision(&self) -> Result<u64, String> {
        let mut items = self.descriptors()?;
        items.sort_by(|left, right| left.id.cmp(&right.id));
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for item in items {
            item.id.hash(&mut hasher);
            item.name.hash(&mut hasher);
            item.path.hash(&mut hasher);
        }
        Ok(hasher.finish())
    }

    #[cfg(test)]
    pub(crate) fn from_profiles(profiles: Vec<WorkspaceProfile>) -> Self {
        Self {
            profiles_override: Some(Arc::new(profiles)),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewaySessionScope {
    pub active_workspace_id: Option<String>,
    pub last_seen_at: u64,
}

#[derive(Debug, Default)]
pub struct GatewaySessionStore {
    sessions: Mutex<HashMap<String, GatewaySessionScope>>,
}

impl GatewaySessionStore {
    fn prune_expired(sessions: &mut HashMap<String, GatewaySessionScope>, now: u64) {
        sessions.retain(|_, scope| {
            scope.last_seen_at == 0 || now.saturating_sub(scope.last_seen_at) <= SESSION_TTL_SECONDS
        });
    }

    /// Register an opaque host-provided session key (for example ChatGPT's
    /// request metadata) so state can survive clients that do not preserve
    /// the MCP transport session header between tool calls.
    pub fn ensure_host_session(&self, session_id: &str) {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = unix_timestamp_seconds();
        Self::prune_expired(&mut sessions, now);
        sessions
            .entry(session_id.to_string())
            .and_modify(|scope| scope.last_seen_at = now)
            .or_insert(GatewaySessionScope {
                active_workspace_id: None,
                last_seen_at: now,
            });
    }

    /// Issue an opaque server-owned session identifier.
    pub fn issue(&self) -> String {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = unix_timestamp_seconds();
        Self::prune_expired(&mut sessions, now);
        loop {
            let session_id = uuid::Uuid::new_v4().to_string();
            if sessions.contains_key(&session_id) {
                continue;
            }
            sessions.insert(
                session_id.clone(),
                GatewaySessionScope {
                    active_workspace_id: None,
                    last_seen_at: now,
                },
            );
            return session_id;
        }
    }

    pub fn contains(&self, session_id: &str) -> bool {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self::prune_expired(&mut sessions, unix_timestamp_seconds());
        sessions.contains_key(session_id)
    }

    pub fn touch_existing(&self, session_id: &str) -> bool {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = unix_timestamp_seconds();
        Self::prune_expired(&mut sessions, now);
        let Some(scope) = sessions.get_mut(session_id) else {
            return false;
        };
        scope.last_seen_at = now;
        true
    }

    pub fn current(&self, session_id: &str) -> GatewaySessionScope {
        self.sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn select(&self, session_id: &str, workspace_id: String) -> bool {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(scope) = sessions.get_mut(session_id) else {
            return false;
        };
        scope.active_workspace_id = Some(workspace_id);
        scope.last_seen_at = unix_timestamp_seconds();
        true
    }

    pub fn clear_selection(&self, session_id: &str) {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(scope) = sessions.get_mut(session_id) {
            scope.active_workspace_id = None;
            scope.last_seen_at = unix_timestamp_seconds();
        }
    }

    pub fn clear_workspace(&self, workspace_id: &str) {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for scope in sessions.values_mut() {
            if scope.active_workspace_id.as_deref() == Some(workspace_id) {
                scope.active_workspace_id = None;
            }
        }
    }

    pub fn active_session_count(&self) -> usize {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self::prune_expired(&mut sessions, unix_timestamp_seconds());
        sessions.len()
    }

    pub fn snapshots(&self) -> Vec<(String, GatewaySessionScope)> {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self::prune_expired(&mut sessions, unix_timestamp_seconds());
        let mut rows = sessions
            .iter()
            .map(|(id, scope)| (id.clone(), scope.clone()))
            .collect::<Vec<_>>();
        rows.sort_by_key(|row| std::cmp::Reverse(row.1.last_seen_at));
        rows
    }

    #[cfg(test)]
    pub(crate) fn register_for_test(&self, session_id: &str) {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        sessions
            .entry(session_id.to_string())
            .or_insert_with(|| GatewaySessionScope {
                active_workspace_id: None,
                last_seen_at: unix_timestamp_seconds(),
            });
    }
}

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
    settings_override: Option<AppSettings>,
}

impl Default for GatewayState {
    fn default() -> Self {
        Self {
            registry: WorkspaceRegistry::default(),
            sessions: Arc::new(GatewaySessionStore::default()),
            usage: Mutex::new(HashMap::new()),
            settings_override: None,
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
            settings_override: Some(settings),
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
            .settings_override
            .clone()
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
