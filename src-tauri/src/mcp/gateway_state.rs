use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::agent_context::{merge_source_lists, AgentContextRuntimeConfig};
use crate::data::DataStore;
use crate::settings::AppSettings;
use crate::tools::context::{merge_ai_instructions, merge_executable_paths};
use crate::tools::policy::PolicySettings;
use crate::tools::{SharedToolContext, ToolContext, Workspace};
use crate::usage::ServiceUsage;
use crate::workspace::{AuthConfig, WorkspaceProfile};

const SESSION_TTL_SECONDS: u64 = 24 * 60 * 60;
// Long enough to cover the two-minute local approval window and the five-minute grant.
const COMPUTER_CAPABILITY_TTL_SECONDS: u64 = 10 * 60;

fn unix_timestamp_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod computer_auto_session_tests {
    use super::GatewaySessionStore;

    #[test]
    fn authenticated_computer_sessions_are_server_issued_and_reused_per_owner() {
        let sessions = GatewaySessionStore::default();
        let a = sessions.computer_auto_session("oauth:client-a");
        assert!(a.starts_with("computer-auto-"));
        assert!(sessions.is_server_issued(&a));
        assert!(sessions.computer_owner_matches(&a, "oauth:client-a"));
        assert_eq!(a, sessions.computer_auto_session("oauth:client-a"));

        let b = sessions.computer_auto_session("oauth:client-b");
        assert_ne!(a, b);
        assert!(sessions.computer_owner_matches(&b, "oauth:client-b"));
        assert!(!sessions.computer_owner_matches(&a, "oauth:client-b"));
        assert!(!sessions.computer_owner_matches(&b, "oauth:client-a"));
    }

    #[test]
    fn client_supplied_metadata_cannot_forge_a_computer_auto_session() {
        let sessions = GatewaySessionStore::default();
        sessions.ensure_host_session("computer-auto-forged-client-id");
        assert!(!sessions.is_server_issued("computer-auto-forged-client-id"));
        assert!(
            !sessions.computer_owner_matches("computer-auto-forged-client-id", "oauth:client-a")
        );

        let valid = sessions.computer_auto_session("oauth:client-a");
        assert!(sessions.is_server_issued(&valid));
        sessions.ensure_host_session(&valid);
        assert!(sessions.computer_owner_matches(&valid, "oauth:client-a"));
    }
}

#[cfg(test)]
mod computer_capability_tests {
    use super::*;

    #[test]
    fn capability_is_secret_owner_bound_expiring_and_revocable() {
        let sessions = GatewaySessionStore::default();
        let issued = sessions.issue_with_owner(Some("oauth:client-a".into()));
        let token = sessions
            .issue_computer_capability(&issued, "oauth:client-a")
            .unwrap();
        assert_eq!(
            sessions.resolve_computer_capability(&token, "oauth:client-a"),
            Some(issued.clone())
        );
        assert_eq!(
            sessions.resolve_computer_capability(&token, "oauth:client-b"),
            None
        );
        assert_eq!(
            sessions.resolve_computer_capability("cu1_invalid", "oauth:client-a"),
            None
        );
        assert_eq!(
            sessions.resolve_computer_capability(&format!("{token}x"), "oauth:client-a"),
            None
        );
        sessions.revoke_computer_capabilities(&issued);
        assert_eq!(
            sessions.resolve_computer_capability(&token, "oauth:client-a"),
            None
        );

        let expiring = sessions
            .issue_computer_capability(&issued, "oauth:client-a")
            .unwrap();
        let key = computer_capability_hash(&expiring);
        sessions
            .computer_capabilities
            .lock()
            .unwrap()
            .get_mut(&key)
            .unwrap()
            .created_at = unix_timestamp_seconds() - COMPUTER_CAPABILITY_TTL_SECONDS - 1;
        assert_eq!(
            sessions.resolve_computer_capability(&expiring, "oauth:client-a"),
            None
        );

        let metadata = "untrusted-client-supplied-id";
        sessions.ensure_host_session(metadata);
        assert_eq!(
            sessions.issue_computer_capability(metadata, "oauth:client-a"),
            None
        );
    }
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
    /// Host-provided metadata IDs are intentionally not valid Computer Use
    /// identities. Only sessions minted by initialize can access that surface.
    #[serde(skip)]
    pub issued_by_server: bool,
    /// Only the authenticated principal that initialized the MCP transport
    /// may reuse this session for host-level Computer Use access. Never accept
    /// client-provided request metadata as this identity.
    #[serde(skip)]
    pub computer_owner: Option<String>,
}

#[derive(Debug, Default)]
pub struct GatewaySessionStore {
    sessions: Mutex<HashMap<String, GatewaySessionScope>>,
    // Only hashes are stored. These bearer capabilities are never inferred from
    // client-provided `_meta` IDs and never grant access without local approval.
    computer_capabilities: Mutex<HashMap<String, ComputerCapability>>,
}

#[derive(Debug, Clone)]
struct ComputerCapability {
    session_id: String,
    owner: String,
    created_at: u64,
}

fn computer_capability_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

impl GatewaySessionStore {
    /// Return a server-minted, principal-bound session for locally enabled
    /// Computer Use. Never accept an identifier supplied by the caller.
    pub fn computer_auto_session(&self, owner: &str) -> String {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = unix_timestamp_seconds();
        Self::prune_expired(&mut sessions, now);
        if let Some((id, scope)) = sessions.iter_mut().find(|(id, scope)| {
            id.starts_with("computer-auto-")
                && scope.issued_by_server
                && scope.computer_owner.as_deref() == Some(owner)
        }) {
            scope.last_seen_at = now;
            return id.clone();
        }
        let id = format!("computer-auto-{}", uuid::Uuid::new_v4());
        sessions.insert(
            id.clone(),
            GatewaySessionScope {
                active_workspace_id: None,
                last_seen_at: now,
                issued_by_server: true,
                computer_owner: Some(owner.to_owned()),
            },
        );
        id
    }
    /// Create a short-lived, high-entropy capability for clients which drop
    /// the MCP transport session header between calls. The caller must first
    /// have a verified OAuth / bearer / local auth principal.
    pub fn issue_computer_capability(&self, session_id: &str, owner: &str) -> Option<String> {
        if !self.computer_owner_matches(session_id, owner) {
            return None;
        }
        let token = format!(
            "cu1_{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let mut capabilities = self
            .computer_capabilities
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = unix_timestamp_seconds();
        capabilities.retain(|_, entry| {
            now.saturating_sub(entry.created_at) <= COMPUTER_CAPABILITY_TTL_SECONDS
        });
        capabilities.insert(
            computer_capability_hash(&token),
            ComputerCapability {
                session_id: session_id.to_string(),
                owner: owner.to_string(),
                created_at: now,
            },
        );
        Some(token)
    }

    pub fn resolve_computer_capability(&self, token: &str, owner: &str) -> Option<String> {
        if token.len() != 68
            || !token.starts_with("cu1_")
            || !token[4..].bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return None;
        }
        let now = unix_timestamp_seconds();
        let capability = {
            let mut capabilities = self
                .computer_capabilities
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            capabilities.retain(|_, entry| {
                now.saturating_sub(entry.created_at) <= COMPUTER_CAPABILITY_TTL_SECONDS
            });
            capabilities.get(&computer_capability_hash(token)).cloned()
        }?;
        if capability.owner != owner || !self.computer_owner_matches(&capability.session_id, owner)
        {
            return None;
        }
        Some(capability.session_id)
    }

    pub fn revoke_computer_capabilities(&self, session_id: &str) {
        let mut capabilities = self
            .computer_capabilities
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        capabilities.retain(|_, entry| entry.session_id != session_id);
    }

    /// Compare a previously server-issued Computer Use session to the verified
    /// HTTP auth identity; client-supplied metadata never confers host rights.
    pub fn computer_owner_matches(&self, session_id: &str, owner: &str) -> bool {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self::prune_expired(&mut sessions, unix_timestamp_seconds());
        sessions.get(session_id).is_some_and(|scope| {
            scope.issued_by_server && scope.computer_owner.as_deref() == Some(owner)
        })
    }
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
                issued_by_server: false,
                computer_owner: None,
            });
    }

    /// Issue an opaque server-owned session identifier.
    #[cfg(test)]
    pub fn issue(&self) -> String {
        self.issue_with_owner(None)
    }

    pub fn issue_with_owner(&self, computer_owner: Option<String>) -> String {
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
                    issued_by_server: true,
                    computer_owner: computer_owner.clone(),
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

    pub fn is_server_issued(&self, session_id: &str) -> bool {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self::prune_expired(&mut sessions, unix_timestamp_seconds());
        sessions
            .get(session_id)
            .is_some_and(|scope| scope.issued_by_server)
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
                issued_by_server: true,
                computer_owner: None,
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

#[cfg(test)]
mod computer_owner_tests {
    use super::*;

    #[test]
    fn authenticated_client_owns_only_its_issued_session() {
        let store = GatewaySessionStore::default();
        let first = store.issue_with_owner(Some("oauth:client-a".into()));
        let second = store.issue_with_owner(Some("oauth:client-b".into()));
        assert!(store.computer_owner_matches(&first, "oauth:client-a"));
        assert!(store.computer_owner_matches(&second, "oauth:client-b"));
        assert!(!store.computer_owner_matches(&first, "oauth:client-b"));
        assert!(!store.computer_owner_matches(&second, "oauth:client-a"));
        assert!(!store.computer_owner_matches(&first, "shared_bearer"));
    }

    #[test]
    fn metadata_or_legacy_sessions_are_not_computer_credentials() {
        let store = GatewaySessionStore::default();
        store.ensure_host_session("client-chosen");
        assert!(!store.computer_owner_matches("client-chosen", "oauth:client-a"));
        let legacy = store.issue();
        assert!(store.is_server_issued(&legacy));
        assert!(!store.computer_owner_matches(&legacy, "oauth:client-a"));
        assert!(!store.computer_owner_matches("unknown", "oauth:client-a"));
    }
}
