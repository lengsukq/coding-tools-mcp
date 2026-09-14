use super::*;

pub(super) fn proxy_already_exists(error: &AppError) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("proxy") && message.contains("already exists")
}

pub(super) fn tunnel_type_for(profile: &WorkspaceProfile) -> &str {
    if profile.tunnel.use_global_gateway {
        "none"
    } else {
        profile.tunnel.tunnel_type.as_str()
    }
}

pub(super) fn public_url_for_profile(profile: &WorkspaceProfile, settings: &AppSettings) -> String {
    profile.effective_public_url_with(settings)
}

pub(super) fn validate_tunnel_requirements(
    profile: &WorkspaceProfile,
    settings: &AppSettings,
) -> AppResult<()> {
    let tunnel_type = tunnel_type_for(profile);
    if tunnel_type == "frp" {
        let profile_id = profile.tunnel.frp_profile_id.as_str();
        let server = profile.tunnel.frp_server.as_str();
        let subdomain = profile.tunnel.frp_subdomain.as_str();
        let port = profile.tunnel.frp_server_port;
        let server = resolve_frp_server(profile_id, server, settings);
        if server.trim().is_empty() {
            return Err(AppError::Message(
                "FRP 模式需要选择全局配置或填写服务器域名。".into(),
            ));
        }
        if subdomain.trim().is_empty() {
            return Err(AppError::Message("FRP 模式需要填写子域名。".into()));
        }
        if port == 0 && settings.find_frp_profile(profile_id).is_none() {
            return Err(AppError::Message("FRP 服务器端口无效。".into()));
        }
        return Ok(());
    }
    if tunnel_type != "cloudflare" {
        return Err(AppError::Message("当前仅支持 FRP 和 Cloudflare。".into()));
    }

    cloudflare::resolve_cloudflared()?;

    let mode = profile.tunnel.cloudflare_mode.as_str();
    let token = SecretStore::get(&profile.id, "cloudflare_token")?.unwrap_or_default();
    let named_url = profile.tunnel.public_url.clone();

    if mode == "named" {
        if token.trim().is_empty() {
            return Err(AppError::Message(
                "Cloudflare 命名隧道模式需要填写 Tunnel Token。".into(),
            ));
        }
        if named_url.trim().is_empty() {
            return Err(AppError::Message(
                "Cloudflare 命名隧道模式需要填写固定公网地址。".into(),
            ));
        }
    }

    Ok(())
}

fn resolve_frp_server(profile_id: &str, inline_server: &str, settings: &AppSettings) -> String {
    if let Some(profile) = settings.find_frp_profile(profile_id) {
        return profile.server.clone();
    }
    inline_server.to_string()
}

pub(super) fn cloudflare_config(
    profile: &WorkspaceProfile,
) -> AppResult<(u16, &str, String, String, &'static str)> {
    let token = SecretStore::get(&profile.id, "cloudflare_token")?.unwrap_or_default();
    Ok((
        profile.runtime.local_port,
        profile.tunnel.cloudflare_mode.as_str(),
        token,
        profile.tunnel.public_url.clone(),
        "cloudflared.log",
    ))
}

pub fn log_dir_for_profile(profile_id: &str) -> PathBuf {
    platform()
        .app_config_dir()
        .map(|home| home.join("logs").join(profile_id))
        .unwrap_or_else(|_| PathBuf::from("logs").join(profile_id))
}

pub fn append_profile_log(profile_id: &str, file_name: &str, line: &str) {
    use std::io::Write;

    let log_dir = log_dir_for_profile(profile_id);
    if std::fs::create_dir_all(&log_dir).is_err() {
        return;
    }
    let path = log_dir.join(file_name);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{line}");
    }
}
