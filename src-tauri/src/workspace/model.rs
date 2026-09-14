use serde::{Deserialize, Serialize};

use crate::settings::AppSettings;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceProfile {
    pub id: String,
    pub name: String,
    pub path: String,
    pub tunnel: TunnelConfig,
    pub auth: AuthConfig,
    pub runtime: RuntimeConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TunnelType(String);

impl TunnelType {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_frp(&self) -> bool {
        self.as_str() == "frp"
    }

    pub fn is_cloudflare(&self) -> bool {
        self.as_str() == "cloudflare"
    }
}

impl From<String> for TunnelType {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for TunnelType {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuthType(String);

impl AuthType {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_oauth(&self) -> bool {
        self.as_str() == "oauth"
    }

    pub fn is_bearer(&self) -> bool {
        self.as_str() == "bearer"
    }

    pub fn is_enabled(&self) -> bool {
        self.as_str() != "noauth"
    }
}

impl From<String> for AuthType {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for AuthType {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    #[serde(rename = "type", default = "default_tunnel_type")]
    pub tunnel_type: TunnelType,
    #[serde(default)]
    pub public_url: String,
    #[serde(default)]
    pub frp_server: String,
    #[serde(default)]
    pub frp_subdomain: String,
    #[serde(default)]
    pub frp_profile_id: String,
    #[serde(default = "default_frp_server_port")]
    pub frp_server_port: u16,
    #[serde(default = "default_cloudflare_mode")]
    pub cloudflare_mode: String,
    /// When true, apply global proxy from Settings → General when starting the tunnel.
    #[serde(default = "default_use_proxy")]
    pub use_proxy: bool,
    /// Route this service through the shared global gateway instead of a dedicated tunnel.
    #[serde(default)]
    pub use_global_gateway: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(rename = "type", default = "default_auth_type")]
    pub auth_type: AuthType,
    #[serde(default = "default_oauth_client_id")]
    pub oauth_client_id: String,
    #[serde(default)]
    pub use_shared_secrets: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    #[serde(default = "default_mcp_port")]
    pub local_port: u16,
    #[serde(default = "default_tool_profile")]
    pub tool_profile: String,
    /// Persist the current MCP conversation through explicit checkpoint calls.
    #[serde(default = "default_history_recording")]
    pub history_recording: bool,
    /// Numeric history archives selected by the desktop UI for context injection.
    #[serde(default)]
    pub history_context_sessions: Vec<u64>,
    #[serde(default = "default_permission_mode")]
    pub permission_mode: String,
    /// Workspace execution policy shared by MCP clients.
    #[serde(default = "default_allowed_commands")]
    pub allowed_commands: String,
    /// Additional executable search paths for workspace runtime commands.
    #[serde(default)]
    pub executable_paths: String,
    /// Workspace-level instructions injected into agent context.
    #[serde(default)]
    pub ai_instructions: String,
    /// IDE / coding-agent instruction providers enabled for this workspace.
    #[serde(default)]
    pub instruction_sources: Vec<String>,
    /// IDE / coding-agent skill providers enabled for this workspace.
    #[serde(default)]
    pub skill_sources: Vec<String>,
    #[serde(default)]
    pub custom_instruction_paths: String,
    #[serde(default)]
    pub custom_skill_paths: String,
    #[serde(default = "default_workspace_local_entries")]
    pub workspace_local_entries: bool,
    #[serde(default = "default_workspace_script_extensions")]
    pub workspace_script_extensions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatusDto {
    pub state: String,
    pub pid: Option<u32>,
    pub local_message: String,
    pub public_message: String,
    pub local_endpoint: String,
    pub public_endpoint: String,
}

fn default_tunnel_type() -> TunnelType {
    "frp".into()
}

fn default_cloudflare_mode() -> String {
    "quick".to_string()
}

fn default_use_proxy() -> bool {
    true
}

fn default_auth_type() -> AuthType {
    "oauth".into()
}

fn default_frp_server_port() -> u16 {
    7000
}

fn default_oauth_client_id() -> String {
    format!("chatgpt-client-{}", &uuid::Uuid::new_v4().to_string()[..12])
}

fn default_mcp_port() -> u16 {
    28766
}

fn default_tool_profile() -> String {
    "compact".to_string()
}

fn default_history_recording() -> bool {
    true
}

fn default_permission_mode() -> String {
    "trusted".to_string()
}

fn default_allowed_commands() -> String {
    "pytest,python,python3,npm,npx,node,pnpm,yarn,make,mvn,mvnw,gradle,gradlew,cargo,go,ruff,mypy,eslint,tsc,git,cmd,powershell,pwsh".to_string()
}

fn default_workspace_local_entries() -> bool {
    true
}

fn default_workspace_script_extensions() -> String {
    ".exe,.bat,.cmd,.ps1".to_string()
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            tunnel_type: default_tunnel_type(),
            public_url: String::new(),
            frp_server: String::new(),
            frp_subdomain: String::new(),
            frp_profile_id: String::new(),
            frp_server_port: default_frp_server_port(),
            cloudflare_mode: default_cloudflare_mode(),
            use_proxy: default_use_proxy(),
            use_global_gateway: false,
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            auth_type: default_auth_type(),
            oauth_client_id: default_oauth_client_id(),
            use_shared_secrets: false,
        }
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            local_port: default_mcp_port(),
            tool_profile: default_tool_profile(),
            history_recording: default_history_recording(),
            history_context_sessions: Vec::new(),
            permission_mode: default_permission_mode(),
            allowed_commands: default_allowed_commands(),
            executable_paths: String::new(),
            ai_instructions: String::new(),
            instruction_sources: Vec::new(),
            skill_sources: Vec::new(),
            custom_instruction_paths: String::new(),
            custom_skill_paths: String::new(),
            workspace_local_entries: default_workspace_local_entries(),
            workspace_script_extensions: default_workspace_script_extensions(),
        }
    }
}

impl WorkspaceProfile {
    pub fn new(path: String, name: Option<String>) -> Self {
        let cleaned = path.trim_end_matches(['\\', '/']).to_string();
        let label = name.unwrap_or_else(|| {
            cleaned
                .replace('\\', "/")
                .split('/')
                .next_back()
                .unwrap_or("工作区")
                .to_string()
        });
        Self {
            id: uuid::Uuid::new_v4().to_string().replace('-', ""),
            name: label,
            path: cleaned,
            tunnel: TunnelConfig::default(),
            auth: AuthConfig::default(),
            runtime: RuntimeConfig::default(),
        }
    }

    pub fn local_endpoint(&self) -> String {
        format!("http://127.0.0.1:{}/mcp", self.runtime.local_port)
    }

    pub fn effective_public_url(&self) -> String {
        self.effective_public_url_with(&AppSettings::load_or_default())
    }

    pub fn effective_public_url_with(&self, settings: &AppSettings) -> String {
        if self.tunnel.use_global_gateway && settings.global_gateway.enabled {
            return gateway_workspace_base(&settings.global_gateway.public_url, &self.id);
        }
        computed_public_url(
            self.tunnel.tunnel_type.as_str(),
            &self.tunnel.frp_server,
            &self.tunnel.frp_subdomain,
            &self.tunnel.public_url,
            &self.tunnel.frp_profile_id,
            settings,
        )
    }

    pub fn public_endpoint(&self) -> String {
        let base = self.effective_public_url();
        if base.is_empty() {
            return String::new();
        }
        format!("{}/mcp", base.trim_end_matches('/'))
    }
}

fn gateway_workspace_base(public_url: &str, workspace_id: &str) -> String {
    let base = public_url.trim_end_matches('/');
    if base.is_empty() {
        return String::new();
    }
    format!("{base}/w/{workspace_id}")
}

fn computed_public_url(
    tunnel_type: &str,
    frp_server: &str,
    frp_subdomain: &str,
    public_url: &str,
    frp_profile_id: &str,
    settings: &AppSettings,
) -> String {
    if tunnel_type == "frp" {
        let server = settings
            .find_frp_profile(frp_profile_id)
            .map(|profile| profile.server.as_str())
            .unwrap_or(frp_server);
        if !server.is_empty() && !frp_subdomain.is_empty() {
            return format!("https://{frp_subdomain}.{server}");
        }
    }
    public_url.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_workspace_defaults_to_compact_agent_workflow() {
        let profile = WorkspaceProfile::new("/tmp/compact-default".into(), None);
        assert_eq!(profile.runtime.tool_profile, "compact");
        assert!(profile.runtime.history_recording);
    }
}
