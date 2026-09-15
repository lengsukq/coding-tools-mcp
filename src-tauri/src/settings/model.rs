use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::data::AppData;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrpProfile {
    pub id: String,
    pub name: String,
    pub server: String,
    #[serde(default = "default_frp_server_port", alias = "serverPort")]
    pub server_port: u16,
}

pub(crate) fn default_global_mcp_auth_type() -> String {
    "oauth".to_string()
}

/// Download settings for fetching frpc / cloudflared binaries.
///
/// GitHub is slow/unreliable from some networks, so downloads try a mirror
/// prefix first (ghproxy-style: `{mirror}/{full_github_url}`) and fall back to
/// the direct GitHub URL. An optional proxy can be layered on top.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadConfig {
    /// Mirror prefix applied before the full GitHub URL. Empty = direct.
    #[serde(default = "default_github_mirror")]
    pub github_mirror: String,
    /// "none" (no proxy) | "system" (env HTTP(S)_PROXY) | "manual".
    #[serde(default = "default_proxy_mode")]
    pub proxy_mode: String,
    /// Proxy URL used when `proxy_mode == "manual"` (e.g. http://127.0.0.1:7890).
    #[serde(default)]
    pub proxy_url: String,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            github_mirror: default_github_mirror(),
            proxy_mode: default_proxy_mode(),
            proxy_url: String::new(),
        }
    }
}

/// Global outbound proxy used by network-facing operations such as the
/// Cloudflare quick tunnel. Binary downloads use `download.proxy` separately.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConfig {
    /// "none" (no proxy) | "system" (env HTTP(S)_PROXY) | "manual".
    #[serde(default = "default_proxy_mode")]
    pub mode: String,
    /// Proxy URL used when `mode == "manual"` (e.g. http://127.0.0.1:7890).
    #[serde(default)]
    pub url: String,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            mode: default_proxy_mode(),
            url: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalGatewayConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_global_gateway_port")]
    pub local_port: u16,
    /// none | cloudflare | frp
    #[serde(default = "default_global_gateway_tunnel_type")]
    pub tunnel_type: String,
    #[serde(default)]
    pub public_url: String,
    #[serde(default = "default_global_gateway_cloudflare_mode")]
    pub cloudflare_mode: String,
    #[serde(default)]
    pub frp_profile_id: String,
    #[serde(default)]
    pub frp_server: String,
    #[serde(default)]
    pub frp_subdomain: String,
    #[serde(default = "default_frp_server_port")]
    pub frp_server_port: u16,
    #[serde(default = "default_global_gateway_use_proxy")]
    pub use_proxy: bool,
}

impl Default for GlobalGatewayConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            local_port: default_global_gateway_port(),
            tunnel_type: default_global_gateway_tunnel_type(),
            public_url: String::new(),
            cloudflare_mode: default_global_gateway_cloudflare_mode(),
            frp_profile_id: String::new(),
            frp_server: String::new(),
            frp_subdomain: String::new(),
            frp_server_port: default_frp_server_port(),
            use_proxy: default_global_gateway_use_proxy(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub frp_profiles: Vec<FrpProfile>,
    #[serde(default)]
    pub last_workspace_id: String,
    #[serde(default)]
    pub download: DownloadConfig,
    /// Global outbound proxy (Cloudflare tunnel, etc.).
    #[serde(default)]
    pub proxy: ProxyConfig,
    /// Global executable search paths inherited by every workspace runtime.
    #[serde(default)]
    pub global_executable_paths: String,
    /// Authentication used by the single 0.3 Global MCP endpoint.
    #[serde(default = "default_global_mcp_auth_type")]
    pub global_mcp_auth_type: String,
    /// Global execution permission mode inherited by workspaces that opt in.
    #[serde(default = "default_global_permission_mode")]
    pub global_permission_mode: String,
    /// Global command allowlist inherited by workspaces that opt in.
    #[serde(default = "default_global_allowed_commands")]
    pub global_allowed_commands: String,
    /// Global agent instructions prepended to workspace-specific instructions.
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
    /// Allow MCP and Global Gateway listeners to bind to all LAN interfaces.
    /// Defaults to false so services remain loopback-only unless explicitly enabled.
    #[serde(default)]
    pub allow_lan_access: bool,
    /// Restore the MCP services that were running in the previous app session.
    #[serde(default)]
    pub restore_runtime_state_on_launch: bool,
    /// Whether the single Global MCP runtime was running when its state was last persisted.
    #[serde(default)]
    pub global_mcp_was_running: bool,
    /// One-time notice generated when legacy per-workspace auth/tunnels could not be merged losslessly.
    #[serde(default)]
    pub global_mcp_migration_notice: String,
    #[serde(default, skip_serializing)]
    pub restore_mcp_workspace_ids: Vec<String>,
    #[serde(default)]
    pub global_gateway: GlobalGatewayConfig,
    /// Shared secrets indexed by key name (e.g. "bearer_token").
    /// Persisted alongside other app settings in app_settings.json.
    #[serde(default)]
    pub shared_secrets: HashMap<String, String>,
    /// Per-workspace secrets: workspace_id -> secret_key -> value.
    #[serde(default)]
    pub workspace_secrets: HashMap<String, HashMap<String, String>>,
    /// App-scoped secrets: scope -> item_id -> value (e.g. frp profile tokens).
    #[serde(default)]
    pub app_secrets: HashMap<String, HashMap<String, String>>,
}

fn default_frp_server_port() -> u16 {
    7000
}

fn default_github_mirror() -> String {
    "https://gh-proxy.com".to_string()
}

fn default_proxy_mode() -> String {
    "system".to_string()
}

fn default_global_gateway_port() -> u16 {
    28765
}
fn default_global_gateway_tunnel_type() -> String {
    "none".to_string()
}
fn default_global_gateway_cloudflare_mode() -> String {
    "quick".to_string()
}
fn default_global_gateway_use_proxy() -> bool {
    true
}

pub(crate) const GLOBAL_RUNTIME_DEFAULTS_VERSION: u32 = 1;
pub(crate) const GLOBAL_MCP_MIGRATION_VERSION: u32 = 1;

pub(crate) fn default_global_permission_mode() -> String {
    "trusted".to_string()
}

pub(crate) fn default_global_allowed_commands() -> String {
    [
        "pytest",
        "python",
        "python3",
        "py",
        "pip",
        "pip3",
        "pipx",
        "uv",
        "poetry",
        "npm",
        "npx",
        "node",
        "pnpm",
        "yarn",
        "bun",
        "deno",
        "make",
        "cmake",
        "ninja",
        "mvn",
        "mvnw",
        "gradle",
        "gradlew",
        "cargo",
        "rustc",
        "rustup",
        "go",
        "ruff",
        "mypy",
        "eslint",
        "tsc",
        "java",
        "javac",
        "ruby",
        "gem",
        "php",
        "composer",
        "clang",
        "clang++",
        "gcc",
        "g++",
        "swift",
        "swiftc",
        "xcodebuild",
        "msbuild",
        "dotnet",
        "git",
        "gh",
        "docker",
        "docker-compose",
        "kubectl",
        "helm",
        "terraform",
        "ansible",
        "aws",
        "az",
        "gcloud",
        "curl",
        "wget",
        "brew",
        "code",
        "corepack",
        "pnpx",
        "xcrun",
        "pod",
        "fastlane",
        "winget",
        "choco",
        "scoop",
        "cmd",
        "powershell",
        "pwsh",
        "wsl",
        "bash",
        "sh",
        "zsh",
        "where",
    ]
    .join(",")
}

#[cfg(target_os = "macos")]
pub(crate) fn default_global_executable_paths() -> String {
    [
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/usr/local/sbin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
        "/Library/Apple/usr/bin",
        "/Applications/Visual Studio Code.app/Contents/Resources/app/bin",
        "~/.cargo/bin",
        "~/.local/bin",
        "~/.bun/bin",
        "~/.deno/bin",
        "~/.volta/bin",
        "~/.npm-global/bin",
        "~/.local/share/pnpm",
        "~/.pyenv/shims",
        "~/.rye/shims",
        "~/go/bin",
        "~/Library/pnpm",
    ]
    .join("\n")
}

#[cfg(target_os = "windows")]
pub(crate) fn default_global_executable_paths() -> String {
    [
        r"C:\Windows\System32",
        r"C:\Windows",
        r"C:\Windows\System32\WindowsPowerShell\v1.0",
        r"C:\Program Files\PowerShell\7",
        r"C:\Program Files\Git\cmd",
        r"C:\Program Files\nodejs",
        r"C:\ProgramData\chocolatey\bin",
        r"~\.cargo\bin",
        r"~\.volta\bin",
        r"~\scoop\shims",
        r"~\AppData\Roaming\npm",
        r"~\AppData\Local\pnpm",
        r"~\go\bin",
        r"~\AppData\Local\Programs\Microsoft VS Code\bin",
        r"~\AppData\Local\Microsoft\WindowsApps",
    ]
    .join("\n")
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn default_global_executable_paths() -> String {
    [
        "/usr/local/bin",
        "/usr/local/sbin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
        "~/.cargo/bin",
        "~/.local/bin",
        "~/.bun/bin",
        "~/.deno/bin",
    ]
    .join("\n")
}

impl AppSettings {
    pub fn from_data(data: &AppData) -> Self {
        Self {
            frp_profiles: data.frp_profiles.clone(),
            last_workspace_id: data.last_workspace_id.clone(),
            download: data.download.clone(),
            proxy: data.proxy.clone(),
            global_executable_paths: data.global_executable_paths.clone(),
            global_mcp_auth_type: if data.global_mcp_auth_type.trim().is_empty() {
                default_global_mcp_auth_type()
            } else {
                data.global_mcp_auth_type.clone()
            },
            global_permission_mode: data.global_permission_mode.clone(),
            global_allowed_commands: data.global_allowed_commands.clone(),
            global_ai_instructions: data.global_ai_instructions.clone(),
            global_instruction_sources: data.global_instruction_sources.clone(),
            global_skill_sources: data.global_skill_sources.clone(),
            global_custom_instruction_paths: data.global_custom_instruction_paths.clone(),
            global_custom_skill_paths: data.global_custom_skill_paths.clone(),
            allow_lan_access: data.allow_lan_access,
            restore_runtime_state_on_launch: data.restore_runtime_state_on_launch,
            global_mcp_was_running: data.global_mcp_was_running,
            global_mcp_migration_notice: data.global_mcp_migration_notice.clone(),
            restore_mcp_workspace_ids: data.restore_mcp_workspace_ids.clone(),
            global_gateway: data.global_gateway.clone(),
            shared_secrets: data.shared_secrets.clone(),
            workspace_secrets: data.workspace_secrets.clone(),
            app_secrets: data.app_secrets.clone(),
        }
    }

    pub fn apply_to(&self, data: &mut AppData) {
        data.frp_profiles = self.frp_profiles.clone();
        data.last_workspace_id = self.last_workspace_id.clone();
        data.download = self.download.clone();
        data.proxy = self.proxy.clone();
        data.global_executable_paths = self.global_executable_paths.clone();
        data.global_mcp_auth_type = self.global_mcp_auth_type.clone();
        data.global_permission_mode = self.global_permission_mode.clone();
        data.global_allowed_commands = self.global_allowed_commands.clone();
        data.global_ai_instructions = self.global_ai_instructions.clone();
        data.global_instruction_sources = self.global_instruction_sources.clone();
        data.global_skill_sources = self.global_skill_sources.clone();
        data.global_custom_instruction_paths = self.global_custom_instruction_paths.clone();
        data.global_custom_skill_paths = self.global_custom_skill_paths.clone();
        data.allow_lan_access = self.allow_lan_access;
        data.restore_runtime_state_on_launch = self.restore_runtime_state_on_launch;
        data.global_mcp_was_running = self.global_mcp_was_running;
        data.global_mcp_migration_notice = self.global_mcp_migration_notice.clone();
        data.restore_mcp_workspace_ids = self.restore_mcp_workspace_ids.clone();
        data.global_gateway = self.global_gateway.clone();
    }

    pub fn load_or_default() -> Self {
        crate::data::DataStore::read_file(|data| Ok(Self::from_data(data))).unwrap_or_else(|_| {
            Self {
                global_executable_paths: default_global_executable_paths(),
                global_permission_mode: default_global_permission_mode(),
                global_allowed_commands: default_global_allowed_commands(),
                ..Self::default()
            }
        })
    }

    pub fn find_frp_profile(&self, id: &str) -> Option<&FrpProfile> {
        if id.trim().is_empty() {
            return None;
        }
        self.frp_profiles.iter().find(|profile| profile.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::FrpProfile;

    #[test]
    fn accepts_frontend_camel_case_server_port() {
        let profile: FrpProfile = serde_json::from_value(serde_json::json!({
            "id": "p1",
            "name": "公司 FRP",
            "server": "frp.example.com",
            "serverPort": 7004
        }))
        .expect("FRP profile should deserialize");

        assert_eq!(profile.server_port, 7004);
    }

    #[test]
    fn keeps_legacy_snake_case_server_port_compatible() {
        let profile: FrpProfile = serde_json::from_value(serde_json::json!({
            "id": "p1",
            "name": "公司 FRP",
            "server": "frp.example.com",
            "server_port": 7005
        }))
        .expect("legacy FRP profile should deserialize");

        assert_eq!(profile.server_port, 7005);
    }
}
