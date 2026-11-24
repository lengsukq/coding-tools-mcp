use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::computer::ManualComputerAccess;
use crate::settings::{
    DownloadConfig, FrpProfile, GlobalGatewayConfig, LanFederationConfig, ProxyConfig,
};
use crate::upstream_mcp::UpstreamMcpConfig;
use crate::workspace::WorkspaceProfile;

/// Unified on-disk payload stored in `data/profiles.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppData {
    #[serde(default)]
    pub frp_profiles: Vec<FrpProfile>,
    #[serde(default)]
    pub last_workspace_id: String,
    #[serde(default)]
    pub download: DownloadConfig,
    #[serde(default)]
    pub proxy: ProxyConfig,
    #[serde(default)]
    pub global_executable_paths: String,
    #[serde(default)]
    pub global_mcp_auth_type: String,
    #[serde(default)]
    pub global_mcp_server_name: String,
    #[serde(default)]
    pub global_permission_mode: String,
    #[serde(default)]
    pub global_allowed_commands: String,
    #[serde(default)]
    pub global_runtime_defaults_version: u32,
    #[serde(default)]
    pub global_ai_instructions: String,
    #[serde(default)]
    pub global_instruction_sources: Vec<String>,
    #[serde(default)]
    pub global_skill_sources: Vec<String>,
    #[serde(default)]
    pub global_custom_instruction_paths: String,
    #[serde(default)]
    pub global_custom_skill_paths: String,
    #[serde(default)]
    pub global_disabled_instruction_ids: Vec<String>,
    #[serde(default)]
    pub global_disabled_skill_ids: Vec<String>,
    /// Parent directories (e.g. `~/code`) whose immediate `.git` child repositories
    /// are automatically exposed and synchronized as Workspaces.
    #[serde(default)]
    pub workspace_auto_roots: Vec<String>,
    #[serde(default)]
    pub allow_lan_access: bool,
    #[serde(default = "crate::settings::default_allow_local_noauth")]
    pub allow_local_noauth: bool,
    #[serde(default)]
    pub restore_runtime_state_on_launch: bool,
    #[serde(default)]
    pub launch_at_login: bool,
    #[serde(default)]
    pub launch_in_lightweight_mode: bool,
    #[serde(default)]
    pub global_mcp_was_running: bool,
    #[serde(default)]
    pub global_mcp_migration_version: u32,
    #[serde(default)]
    pub global_mcp_migration_notice: String,
    #[serde(default, skip_serializing)]
    pub restore_mcp_workspace_ids: Vec<String>,
    #[serde(default)]
    pub global_gateway: GlobalGatewayConfig,
    #[serde(default)]
    pub lan_federation: LanFederationConfig,
    #[serde(default)]
    pub upstream_mcp: UpstreamMcpConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_audit_retention_days: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) manual_computer_access: Option<ManualComputerAccess>,
    #[serde(default)]
    pub shared_secrets: HashMap<String, String>,
    #[serde(default)]
    pub workspace_secrets: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    pub app_secrets: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    pub profiles: Vec<WorkspaceProfile>,
}

/// Legacy `{ "profiles": [...] }` file at the app root.
#[derive(Debug, Deserialize)]
pub struct LegacyProfilesOnlyFile {
    pub profiles: Vec<WorkspaceProfile>,
}
