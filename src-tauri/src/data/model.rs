use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::settings::{DownloadConfig, FrpProfile, GlobalGatewayConfig, ProxyConfig};
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
    pub allow_lan_access: bool,
    #[serde(default)]
    pub restore_runtime_state_on_launch: bool,
    #[serde(default)]
    pub restore_mcp_workspace_ids: Vec<String>,
    #[serde(default)]
    pub restore_actions_workspace_ids: Vec<String>,
    #[serde(default)]
    pub global_gateway: GlobalGatewayConfig,
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
