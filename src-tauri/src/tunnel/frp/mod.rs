mod client;

use crate::settings::{AppSettings, GlobalGatewayConfig};

pub(crate) use client::{
    acquire_frpc_operation_lock, clear_managed_frpc_pid, stop_recorded_frpc_instance,
};
pub(crate) use client::{cached_frpc_path, download_frpc_to_cache};
pub use client::{resolve_frpc, spawn_global_frpc};

const FRP_VERSION: &str = "0.61.2";
pub(crate) const VERSION: &str = FRP_VERSION;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct FrpProxyConfig {
    pub proxy_name: String,
    pub local_port: u16,
    pub subdomain: String,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct FrpServerConfig {
    pub server_addr: String,
    pub server_port: u16,
    pub token: Option<String>,
    pub proxy: FrpProxyConfig,
}

pub fn frp_public_url(config: &GlobalGatewayConfig, settings: &AppSettings) -> String {
    let config = global_frp_server_config(config, settings, None);
    if config.server_addr.trim().is_empty() || config.proxy.subdomain.trim().is_empty() {
        return String::new();
    }
    format!(
        "https://{}.{}",
        config.proxy.subdomain.trim(),
        config.server_addr.trim().trim_end_matches('/')
    )
}

pub(crate) fn global_frp_server_config(
    gateway: &GlobalGatewayConfig,
    settings: &AppSettings,
    token_override: Option<String>,
) -> FrpServerConfig {
    let proxy = FrpProxyConfig {
        proxy_name: "global-mcp".into(),
        local_port: gateway.local_port,
        subdomain: gateway.frp_subdomain.clone(),
    };
    let profile_id = gateway.frp_profile_id.as_str();
    let inline_server = gateway.frp_server.clone();
    let inline_port = gateway.frp_server_port;
    let (server_addr, server_port) =
        if let Some(frp_profile) = settings.find_frp_profile(profile_id) {
            (frp_profile.server.clone(), frp_profile.server_port)
        } else {
            (inline_server, inline_port)
        };
    let token =
        token_override.or_else(|| resolve_global_frp_token(profile_id, &server_addr, settings));

    FrpServerConfig {
        server_addr,
        server_port,
        token,
        proxy,
    }
}

fn resolve_global_frp_token(
    profile_id: &str,
    server_addr: &str,
    settings: &AppSettings,
) -> Option<String> {
    if !profile_id.trim().is_empty() {
        if let Ok(Some(token)) =
            crate::secret::SecretStore::get_app("frp_profile_token", profile_id)
        {
            if !token.trim().is_empty() {
                return Some(token);
            }
        }
    }

    let inline_server = server_addr.trim();
    if !inline_server.is_empty() {
        for profile in &settings.frp_profiles {
            if profile.server.trim().eq_ignore_ascii_case(inline_server) {
                if let Ok(Some(token)) =
                    crate::secret::SecretStore::get_app("frp_profile_token", &profile.id)
                {
                    if !token.trim().is_empty() {
                        return Some(token);
                    }
                }
            }
        }
    }

    None
}

pub(crate) fn build_frpc_toml(config: &FrpServerConfig) -> String {
    let mut lines = vec![
        format!("serverAddr = \"{}\"", config.server_addr.trim()),
        format!("serverPort = {}", config.server_port),
        "loginFailExit = false".to_string(),
        String::new(),
    ];
    if let Some(token) = config
        .token
        .as_ref()
        .filter(|token| !token.trim().is_empty())
    {
        lines.push("auth.method = \"token\"".to_string());
        lines.push(format!("auth.token = \"{}\"", token.trim()));
        lines.push(String::new());
    }
    lines.push(build_proxy_snippet(&config.proxy));
    lines.join("\n")
}

fn build_proxy_snippet(proxy: &FrpProxyConfig) -> String {
    [
        "[[proxies]]".to_string(),
        format!("name = \"{}\"", proxy.proxy_name),
        "type = \"http\"".to_string(),
        "localIP = \"127.0.0.1\"".to_string(),
        format!("localPort = {}", proxy.local_port),
        format!("subdomain = \"{}\"", proxy.subdomain.trim()),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::FrpProfile;

    fn global_config() -> GlobalGatewayConfig {
        GlobalGatewayConfig {
            local_port: 28765,
            frp_profile_id: "p1".into(),
            frp_subdomain: "coding-tools".into(),
            ..GlobalGatewayConfig::default()
        }
    }

    fn settings() -> AppSettings {
        AppSettings {
            frp_profiles: vec![FrpProfile {
                id: "p1".into(),
                name: "Main".into(),
                server: "frp.example.com".into(),
                server_port: 7000,
            }],
            ..AppSettings::default()
        }
    }

    #[test]
    fn global_frp_config_uses_global_endpoint_without_workspace_profile() {
        let config = global_frp_server_config(&global_config(), &settings(), Some("secret".into()));

        assert_eq!(config.proxy.proxy_name, "global-mcp");
        assert_eq!(config.proxy.local_port, 28765);
        assert_eq!(config.proxy.subdomain, "coding-tools");
        assert_eq!(config.server_addr, "frp.example.com");
        assert_eq!(config.server_port, 7000);
        assert_eq!(config.token.as_deref(), Some("secret"));
    }

    #[test]
    fn global_frp_url_points_to_the_single_mcp_proxy() {
        assert_eq!(
            frp_public_url(&global_config(), &settings()),
            "https://coding-tools.frp.example.com"
        );
    }

    #[test]
    fn global_frpc_toml_contains_one_proxy_for_the_global_endpoint() {
        let config = global_frp_server_config(&global_config(), &settings(), Some("secret".into()));
        let toml = build_frpc_toml(&config);

        assert_eq!(toml.matches("[[proxies]]").count(), 1);
        assert!(toml.contains("name = \"global-mcp\""));
        assert!(toml.contains("localPort = 28765"));
        assert!(toml.contains("auth.token = \"secret\""));
    }
}
