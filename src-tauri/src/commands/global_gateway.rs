use std::fs;
use std::path::Path;

use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::app_state::AppState;
use crate::auth::{external_base_url, issue_scoped_access_token, verify_scoped_access_token};
use crate::data::DataStore;
use crate::error::{AppError, AppResult};
use crate::global_gateway::{self, GatewayHealthItem, GlobalGatewayStatusDto};
use crate::mcp::stdio_bridge::{
    format_mcp_stdio_command, gemini_stdio_auth_file_path, gemini_stdio_bin_path,
    read_auth_file, remove_auth_file_if_exists, write_private_auth_file, GeminiStdioAuthFile,
    GEMINI_STDIO_CLIENT_ID,
};
use crate::secret::SecretStore;
use crate::settings::{AppSettings, GlobalGatewayConfig};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiLocalBridgeStatusDto {
    pub command: String,
    pub binary_path: String,
    pub binary_ready: bool,
    pub auth_file_path: String,
    pub credentials_ready: bool,
    pub auth_type: String,
    pub endpoint: String,
    pub issue: String,
}

#[tauri::command]
pub fn get_global_gateway_config(_state: State<'_, AppState>) -> AppResult<GlobalGatewayConfig> {
    DataStore::read_file(|data| Ok(AppSettings::from_data(data).global_gateway))
}

#[tauri::command]
pub fn set_global_gateway_config(
    state: State<'_, AppState>,
    config: GlobalGatewayConfig,
) -> AppResult<()> {
    state.with_settings(|store| {
        let mut settings = store.settings();
        settings.global_gateway = config;
        store.update_settings(settings)
    })
}

#[tauri::command]
pub async fn start_global_gateway() -> AppResult<GlobalGatewayStatusDto> {
    global_gateway::ensure_started().await
}

#[tauri::command]
pub async fn restart_global_cloudflare_tunnel() -> AppResult<GlobalGatewayStatusDto> {
    global_gateway::force_restart_cloudflare_tunnel().await
}

#[tauri::command]
pub async fn stop_global_gateway() -> AppResult<()> {
    global_gateway::stop().await
}

#[tauri::command]
pub async fn get_global_gateway_status() -> AppResult<GlobalGatewayStatusDto> {
    Ok(global_gateway::status().await)
}

#[tauri::command]
pub async fn check_global_gateway_health() -> AppResult<Vec<GatewayHealthItem>> {
    Ok(global_gateway::health().await)
}

#[tauri::command]
pub fn get_gemini_local_bridge_status(
    state: State<'_, AppState>,
) -> AppResult<GeminiLocalBridgeStatusDto> {
    let settings = state.with_settings(|store| Ok(store.settings()))?;
    let oauth_secret = if settings.global_mcp_auth_type.trim() == "oauth" {
        SecretStore::get_shared("oauth_token_secret")?.unwrap_or_default()
    } else {
        String::new()
    };
    let bin_path = gemini_stdio_bin_path().map_err(AppError::Message)?;
    let auth_path = gemini_stdio_auth_file_path().map_err(AppError::Message)?;
    let current_exe = std::env::current_exe().ok();
    Ok(build_bridge_status_with_paths(
        &settings,
        &oauth_secret,
        &bin_path,
        current_exe.as_deref(),
        &auth_path,
    ))
}

#[tauri::command]
pub fn prepare_gemini_local_bridge(
    state: State<'_, AppState>,
) -> AppResult<GeminiLocalBridgeStatusDto> {
    let settings = state.with_settings(|store| Ok(store.settings()))?;
    let auth_type = settings.global_mcp_auth_type.trim();
    let oauth_secret = match auth_type {
        "oauth" => SecretStore::get_shared("oauth_token_secret")?.unwrap_or_default(),
        "noauth" => String::new(),
        "bearer" => {
            return Err(AppError::Message(
                "Bearer 模式不支持自动生成本地桥接凭据，请切换到 OAuth 或无认证模式。".into(),
            ))
        }
        _ => {
            return Err(AppError::Message(
                "未知 Global MCP 认证模式，无法生成 Gemini 本地桥接配置。".into(),
            ))
        }
    };

    let bin_path = gemini_stdio_bin_path().map_err(AppError::Message)?;
    let auth_path = gemini_stdio_auth_file_path().map_err(AppError::Message)?;
    let current_exe = std::env::current_exe()?;
    prepare_bridge_with_paths(&settings, &oauth_secret, &bin_path, &current_exe, &auth_path)
}

fn prepare_bridge_with_paths(
    settings: &AppSettings,
    oauth_secret: &str,
    bin_path: &Path,
    target_exe: &Path,
    auth_path: &Path,
) -> AppResult<GeminiLocalBridgeStatusDto> {
    install_bridge_entrypoint(bin_path, target_exe)?;
    let endpoint = local_mcp_endpoint(settings.global_gateway.local_port);
    match settings.global_mcp_auth_type.trim() {
        "oauth" => {
            let server_url = external_base_url(
                &HeaderMap::new(),
                settings.global_gateway.local_port,
                &settings.global_gateway.public_url,
            );
            let (token, expires_at) =
                issue_scoped_access_token(&server_url, oauth_secret, GEMINI_STDIO_CLIENT_ID)
                    .map_err(|()| {
                        AppError::Message("生成 Gemini 本地 OAuth 凭据失败。".into())
                    })?;
            let payload =
                GeminiStdioAuthFile::new_oauth(endpoint, server_url, token, expires_at);
            write_private_auth_file(auth_path, &payload).map_err(AppError::Message)?;
        }
        "noauth" => {
            remove_auth_file_if_exists(auth_path).map_err(AppError::Message)?;
        }
        _ => {}
    }

    Ok(build_bridge_status_with_paths(
        settings,
        oauth_secret,
        bin_path,
        Some(target_exe),
        auth_path,
    ))
}

fn build_bridge_status_with_paths(
    settings: &AppSettings,
    oauth_secret: &str,
    bin_path: &Path,
    expected_exe: Option<&Path>,
    auth_path: &Path,
) -> GeminiLocalBridgeStatusDto {
    let endpoint = local_mcp_endpoint(settings.global_gateway.local_port);
    let auth_type = settings.global_mcp_auth_type.trim().to_string();
    let binary_ready = check_binary_ready(bin_path, expected_exe);
    let (credentials_ready, auth_issue) =
        evaluate_credentials(settings, oauth_secret, auth_path, &endpoint);
    let issue = if !auth_issue.is_empty() {
        auth_issue
    } else if !binary_ready {
        "尚未生成本地桥接入口，请点击“生成凭据并复制命令”。".into()
    } else {
        String::new()
    };

    GeminiLocalBridgeStatusDto {
        command: format_mcp_stdio_command(bin_path, &endpoint),
        binary_path: bin_path.to_string_lossy().into_owned(),
        binary_ready,
        auth_file_path: auth_path.to_string_lossy().into_owned(),
        credentials_ready,
        auth_type,
        endpoint,
        issue,
    }
}

fn local_mcp_endpoint(port: u16) -> String {
    format!("http://127.0.0.1:{port}/mcp")
}

fn check_binary_ready(bin_path: &Path, expected_exe: Option<&Path>) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // Symlinks resolve into `/Applications/...`, which Gemini's Seatbelt sandbox denies.
        let Ok(meta) = fs::symlink_metadata(bin_path) else {
            return false;
        };
        if meta.file_type().is_symlink() || !meta.is_file() {
            return false;
        }
        let Some(exe) = expected_exe else {
            return true;
        };
        let Ok(exe_meta) = fs::metadata(exe) else {
            return true;
        };
        if meta.dev() == exe_meta.dev() {
            meta.ino() == exe_meta.ino()
        } else {
            meta.len() == exe_meta.len()
        }
    }
    #[cfg(not(unix))]
    {
        let _ = expected_exe;
        bin_path.is_file()
    }
}

fn evaluate_credentials(
    settings: &AppSettings,
    oauth_secret: &str,
    auth_path: &Path,
    endpoint: &str,
) -> (bool, String) {
    match settings.global_mcp_auth_type.trim() {
        "noauth" => (true, String::new()),
        "bearer" => (
            false,
            "当前是 Bearer 模式；为避免在命令或明文文件外暴露密钥，请切换到 OAuth 或无认证模式。".into(),
        ),
        "oauth" => {
            let server_url = external_base_url(
                &HeaderMap::new(),
                settings.global_gateway.local_port,
                &settings.global_gateway.public_url,
            );
            match read_auth_file(auth_path) {
                Ok(Some(file))
                    if file.auth_type == "oauth"
                        && file.endpoint == endpoint
                        && file
                            .access_token
                            .as_deref()
                            .is_some_and(|token| verify_scoped_access_token(token, oauth_secret, &server_url)) =>
                {
                    (true, String::new())
                }
                Ok(Some(_)) => (
                    false,
                    "本地 OAuth 凭据已过期或与当前端口/域名配置不一致，请点击重新生成。".into(),
                ),
                Ok(None) => (
                    false,
                    "尚未生成本地 OAuth 凭据，请点击“生成凭据并复制命令”。".into(),
                ),
                Err(_) => (
                    false,
                    "本地 OAuth 凭据文件损坏，请点击重新生成。".into(),
                ),
            }
        }
        _ => (
            false,
            "尚未读取到 Global MCP 认证模式，请先刷新连接状态。".into(),
        ),
    }
}

fn install_bridge_entrypoint(bin_path: &Path, target_exe: &Path) -> AppResult<()> {
    #[cfg(unix)]
    {
        const EXDEV: i32 = 18;
        let parent = bin_path
            .parent()
            .ok_or_else(|| AppError::Message("无效的本地桥接入口路径。".into()))?;
        fs::create_dir_all(parent)?;
        if let Ok(meta) = fs::symlink_metadata(bin_path) {
            if meta.is_dir() {
                return Err(AppError::Message(format!(
                    "{} 是目录，无法覆盖为桥接入口。",
                    bin_path.display()
                )));
            }
            fs::remove_file(bin_path)?;
        }
        // Hard-link inside ~/.local/bin shares the signed Mach-O inode and avoids
        // macOS Seatbelt rejecting symlink targets located inside /Applications.
        if let Err(err) = fs::hard_link(target_exe, bin_path) {
            if err.raw_os_error() == Some(EXDEV) {
                fs::copy(target_exe, bin_path)?;
            } else {
                return Err(err.into());
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (bin_path, target_exe);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_settings(auth_type: &str, public_url: &str) -> AppSettings {
        let mut settings = AppSettings::default();
        settings.global_mcp_auth_type = auth_type.into();
        settings.global_gateway.local_port = 28765;
        settings.global_gateway.public_url = public_url.into();
        settings
    }

    #[test]
    fn prepare_and_verify_oauth_bridge_files_without_touching_home() {
        let root = std::env::temp_dir().join(format!("ctm-cmd-bridge-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let fake_exe = root.join("Coding Tools MCP");
        fs::write(&fake_exe, b"binary").unwrap();
        let bin_path = root.join("bin").join("coding-tools-mcp");
        let auth_path = root.join("share").join("gemini-stdio-auth.json");
        let settings = test_settings("oauth", "https://mcp.example.com");

        let before =
            build_bridge_status_with_paths(&settings, "secret-123", &bin_path, Some(&fake_exe), &auth_path);
        assert!(!before.binary_ready);
        assert!(!before.credentials_ready);

        // Existing symlinks must be rejected so Seatbelt sandbox execution cannot fail.
        #[cfg(unix)]
        {
            fs::create_dir_all(bin_path.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(&fake_exe, &bin_path).unwrap();
            let symlink_status = build_bridge_status_with_paths(
                &settings,
                "secret-123",
                &bin_path,
                Some(&fake_exe),
                &auth_path,
            );
            assert!(!symlink_status.binary_ready);
        }

        let after = prepare_bridge_with_paths(
            &settings,
            "secret-123",
            &bin_path,
            &fake_exe,
            &auth_path,
        )
        .expect("prepare oauth bridge");
        assert!(after.binary_ready);
        assert!(!fs::symlink_metadata(&bin_path).unwrap().file_type().is_symlink());
        assert!(after.credentials_ready);
        assert!(after.issue.is_empty());
        assert_eq!(
            after.command,
            format_mcp_stdio_command(&bin_path, "http://127.0.0.1:28765/mcp")
        );

        // Changing public_url invalidates previously minted audience/issuer cleanly.
        let changed = test_settings("oauth", "https://other.example.com");
        let invalidated =
            build_bridge_status_with_paths(&changed, "secret-123", &bin_path, Some(&fake_exe), &auth_path);
        assert!(!invalidated.credentials_ready);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn noauth_removes_stale_credentials_and_marks_ready() {
        let root = std::env::temp_dir().join(format!("ctm-cmd-noauth-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let fake_exe = root.join("coding-tools-mcp-desktop");
        fs::write(&fake_exe, b"binary").unwrap();
        let bin_path = root.join("bin").join("coding-tools-mcp");
        let auth_path = root.join("share").join("gemini-stdio-auth.json");
        write_private_auth_file(
            &auth_path,
            &GeminiStdioAuthFile::new_oauth(
                "http://127.0.0.1:28765/mcp".into(),
                "http://127.0.0.1:28765".into(),
                "old-token".into(),
                1_900_000_000,
            ),
        )
        .unwrap();

        let status = prepare_bridge_with_paths(
            &test_settings("noauth", ""),
            "",
            &bin_path,
            &fake_exe,
            &auth_path,
        )
        .expect("prepare noauth bridge");
        assert!(status.binary_ready);
        assert!(status.credentials_ready);
        assert!(!auth_path.exists());

        let _ = fs::remove_dir_all(&root);
    }
}
