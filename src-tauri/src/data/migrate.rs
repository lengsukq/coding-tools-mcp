use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::AppResult;
use crate::platform::platform;
use crate::settings::{
    default_global_allowed_commands, default_global_executable_paths, default_global_mcp_auth_type,
    default_global_permission_mode, AppSettings, GLOBAL_MCP_MIGRATION_VERSION,
    GLOBAL_RUNTIME_DEFAULTS_VERSION,
};

use super::model::{AppData, LegacyProfilesOnlyFile};

const LEGACY_PROFILES_FILE: &str = "profiles.json";
const LEGACY_SETTINGS_FILE: &str = "app_settings.json";

pub fn data_file_path() -> AppResult<PathBuf> {
    Ok(platform()
        .app_config_dir()?
        .join("data")
        .join("profiles.json"))
}

fn migrate_global_auth_credentials(data: &mut AppData, notices: &mut Vec<String>) {
    let keys: &[&str] = match data.global_mcp_auth_type.as_str() {
        "bearer" => &["bearer_token"],
        "oauth" => &[
            "oauth_client_id",
            "oauth_client_secret",
            "oauth_password",
            "oauth_token_secret",
        ],
        _ => &[],
    };

    for key in keys {
        let mut values = Vec::new();
        for profile in &data.profiles {
            let value = if *key == "oauth_client_id" {
                if profile.auth.use_shared_secrets {
                    data.shared_secrets.get(*key).cloned()
                } else if profile.auth.oauth_client_id.trim().is_empty() {
                    None
                } else {
                    Some(profile.auth.oauth_client_id.clone())
                }
            } else if profile.auth.use_shared_secrets {
                data.shared_secrets.get(*key).cloned()
            } else {
                data.workspace_secrets
                    .get(&profile.id)
                    .and_then(|secrets| secrets.get(*key))
                    .cloned()
            };
            if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
                values.push(value);
            }
        }
        values.sort();
        values.dedup();
        match values.as_slice() {
            [only] => {
                data.shared_secrets.insert((*key).to_string(), only.clone());
            }
            [] => {}
            _ => notices.push(format!(
                "旧 Workspace 的 {key} 凭据不一致；未自动选择，已保留原数据，请在 Global MCP 密钥页面重新确认。"
            )),
        }
    }
}

fn apply_global_mcp_migration(data: &mut AppData) -> bool {
    if data.global_mcp_migration_version >= GLOBAL_MCP_MIGRATION_VERSION {
        return false;
    }

    let mut notices = Vec::new();
    if !data.restore_mcp_workspace_ids.is_empty() {
        data.global_mcp_was_running = true;
    }
    data.restore_mcp_workspace_ids.clear();

    if data.global_mcp_auth_type.trim().is_empty() {
        let mut auth_types = data
            .profiles
            .iter()
            .map(|profile| profile.auth.auth_type.as_str().trim())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        auth_types.sort_unstable();
        auth_types.dedup();
        data.global_mcp_auth_type = match auth_types.as_slice() {
            [only] => (*only).to_string(),
            [] => default_global_mcp_auth_type(),
            _ => {
                notices.push(
                    "旧 Workspace 使用了不同认证模式；0.3 已统一为全局 OAuth，请在设置中复核认证配置。"
                        .to_string(),
                );
                default_global_mcp_auth_type()
            }
        };
    }

    migrate_global_auth_credentials(data, &mut notices);

    // If the user already configured the old Global Gateway, preserve it as the
    // single public tunnel. Otherwise a single unambiguous legacy workspace
    // tunnel can be promoted automatically. Multiple different workspace
    // tunnels cannot be losslessly merged into one endpoint, so fail safe and
    // leave the public tunnel disabled with a migration notice.
    if !data.global_gateway.enabled {
        let configured = data
            .profiles
            .iter()
            .filter(|profile| {
                !profile.tunnel.use_global_gateway
                    && match profile.tunnel.tunnel_type.as_str() {
                        "cloudflare" => true,
                        "frp" => {
                            !profile.tunnel.frp_subdomain.trim().is_empty()
                                || !profile.tunnel.frp_server.trim().is_empty()
                                || !profile.tunnel.frp_profile_id.trim().is_empty()
                        }
                        _ => false,
                    }
            })
            .collect::<Vec<_>>();
        if configured.len() == 1 {
            let tunnel = &configured[0].tunnel;
            if tunnel.tunnel_type.as_str() == "cloudflare" && tunnel.cloudflare_mode == "named" {
                notices.push(
                    "旧 Workspace 使用 Named Cloudflare Tunnel；0.3 不自动迁移该固定域名，请在全局连接设置中重新配置。"
                        .to_string(),
                );
            } else {
                data.global_gateway.enabled = true;
                data.global_gateway.tunnel_type = tunnel.tunnel_type.as_str().to_string();
                data.global_gateway.public_url = tunnel.public_url.clone();
                data.global_gateway.cloudflare_mode = tunnel.cloudflare_mode.clone();
                data.global_gateway.frp_profile_id = tunnel.frp_profile_id.clone();
                data.global_gateway.frp_server = tunnel.frp_server.clone();
                data.global_gateway.frp_subdomain = tunnel.frp_subdomain.clone();
                data.global_gateway.frp_server_port = tunnel.frp_server_port;
                data.global_gateway.use_proxy = tunnel.use_proxy;
            }
        } else if configured.len() > 1 {
            notices.push(
                "检测到多个旧 Workspace 独立公网 Tunnel；0.3 只保留一个 Global MCP 公网入口，未自动选择其中任何一个。"
                    .to_string(),
            );
        }
    }

    data.global_mcp_migration_notice = notices.join("\n");
    data.global_mcp_migration_version = GLOBAL_MCP_MIGRATION_VERSION;
    true
}

pub fn load_or_migrate() -> AppResult<AppData> {
    let path = data_file_path()?;
    if path.exists() {
        let raw = fs::read_to_string(&path)?;
        let mut data: AppData = serde_json::from_str(&raw).unwrap_or_default();
        if apply_global_runtime_defaults(&mut data) | apply_global_mcp_migration(&mut data) {
            write_data(&path, &data)?;
        }
        return Ok(data);
    }

    let app_root = platform().app_config_dir()?;
    let mut data = AppData::default();

    let legacy_profiles = app_root.join(LEGACY_PROFILES_FILE);
    if legacy_profiles.exists() {
        let raw = fs::read_to_string(&legacy_profiles)?;
        if let Ok(file) = serde_json::from_str::<LegacyProfilesOnlyFile>(&raw) {
            data.profiles = file.profiles;
        }
    }

    let legacy_settings = app_root.join(LEGACY_SETTINGS_FILE);
    if legacy_settings.exists() {
        let raw = fs::read_to_string(&legacy_settings)?;
        if let Ok(settings) = serde_json::from_str::<AppSettings>(&raw) {
            merge_settings(&mut data, settings);
        }
    }

    apply_global_runtime_defaults(&mut data);
    apply_global_mcp_migration(&mut data);

    Ok(data)
}

fn apply_global_runtime_defaults(data: &mut AppData) -> bool {
    if data.global_runtime_defaults_version >= GLOBAL_RUNTIME_DEFAULTS_VERSION {
        return false;
    }

    if data.global_executable_paths.trim().is_empty() {
        data.global_executable_paths = default_global_executable_paths();
    }
    if data.global_permission_mode.trim().is_empty() {
        data.global_permission_mode = default_global_permission_mode();
    }
    if data.global_allowed_commands.trim().is_empty() {
        data.global_allowed_commands = default_global_allowed_commands();
    }
    for profile in &mut data.profiles {
        if !profile.runtime.inherit_global_execution_policy
            && profile.runtime.uses_legacy_default_execution_policy()
        {
            profile.runtime.inherit_global_execution_policy = true;
        }
    }
    data.global_runtime_defaults_version = GLOBAL_RUNTIME_DEFAULTS_VERSION;
    true
}

pub fn save(data: &AppData) -> AppResult<()> {
    let path = data_file_path()?;
    write_data(&path, data)
}

fn write_data(path: &Path, data: &AppData) -> AppResult<()> {
    let parent = path.parent().ok_or_else(|| {
        crate::error::AppError::Message("data file path has no parent directory".into())
    })?;
    fs::create_dir_all(parent)?;
    let text = serde_json::to_string_pretty(data)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(format!("{text}\n").as_bytes())?;
    temp.as_file_mut().sync_all()?;
    temp.persist(path).map_err(|error| error.error)?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

pub fn maybe_backup_legacy_files(path: &Path) -> AppResult<()> {
    if !path.exists() {
        return Ok(());
    }
    let app_root = platform().app_config_dir()?;
    for name in [LEGACY_PROFILES_FILE, LEGACY_SETTINGS_FILE] {
        let legacy = app_root.join(name);
        if legacy.exists() {
            let backup = app_root.join(format!("{name}.bak"));
            if !backup.exists() {
                let _ = fs::rename(&legacy, &backup);
            }
        }
    }
    Ok(())
}

fn merge_settings(data: &mut AppData, settings: AppSettings) {
    data.frp_profiles = settings.frp_profiles;
    data.last_workspace_id = settings.last_workspace_id;
    data.download = settings.download;
    data.proxy = settings.proxy;
    data.global_executable_paths = settings.global_executable_paths;
    data.global_mcp_auth_type = settings.global_mcp_auth_type;
    data.global_permission_mode = settings.global_permission_mode;
    data.global_allowed_commands = settings.global_allowed_commands;
    data.global_ai_instructions = settings.global_ai_instructions;
    data.global_instruction_sources = settings.global_instruction_sources;
    data.global_skill_sources = settings.global_skill_sources;
    data.global_custom_instruction_paths = settings.global_custom_instruction_paths;
    data.global_custom_skill_paths = settings.global_custom_skill_paths;
    data.allow_lan_access = settings.allow_lan_access;
    data.restore_runtime_state_on_launch = settings.restore_runtime_state_on_launch;
    data.global_mcp_was_running = settings.global_mcp_was_running;
    data.global_mcp_migration_notice = settings.global_mcp_migration_notice;
    data.restore_mcp_workspace_ids = settings.restore_mcp_workspace_ids;
    data.global_gateway = settings.global_gateway;
    data.shared_secrets = settings.shared_secrets;
    data.workspace_secrets = settings.workspace_secrets;
    data.app_secrets = settings.app_secrets;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_data_replaces_existing_file_without_leaving_temp_files() {
        let dir = tempfile::tempdir().expect("temp data dir");
        let path = dir.path().join("profiles.json");
        fs::write(&path, "{ broken old content\n").expect("legacy partial file");

        let data = AppData {
            global_mcp_auth_type: "oauth".into(),
            ..AppData::default()
        };
        write_data(&path, &data).expect("atomic write");

        let persisted: AppData = serde_json::from_str(
            &fs::read_to_string(&path).expect("persisted data should be readable"),
        )
        .expect("persisted data should be valid json");
        assert_eq!(persisted.global_mcp_auth_type, "oauth");
        assert_eq!(
            fs::read_dir(dir.path()).expect("list temp dir").count(),
            1,
            "temporary file should be removed after persist"
        );
    }

    #[test]
    fn global_runtime_defaults_are_applied_once() {
        let mut data = AppData::default();
        assert!(apply_global_runtime_defaults(&mut data));
        assert_eq!(
            data.global_runtime_defaults_version,
            GLOBAL_RUNTIME_DEFAULTS_VERSION
        );
        assert_eq!(data.global_permission_mode, "trusted");
        assert!(data.global_allowed_commands.contains("git"));
        assert!(!data.global_executable_paths.trim().is_empty());

        data.global_executable_paths.clear();
        assert!(!apply_global_runtime_defaults(&mut data));
        assert!(data.global_executable_paths.is_empty());
    }

    #[test]
    fn migration_preserves_existing_custom_runtime_values() {
        let mut data = AppData {
            global_executable_paths: "/custom/bin".into(),
            global_permission_mode: "safe".into(),
            global_allowed_commands: "mytool".into(),
            ..AppData::default()
        };

        assert!(apply_global_runtime_defaults(&mut data));
        assert_eq!(data.global_executable_paths, "/custom/bin");
        assert_eq!(data.global_permission_mode, "safe");
        assert_eq!(data.global_allowed_commands, "mytool");
    }

    #[test]
    fn migration_enables_global_policy_for_legacy_default_workspaces_only() {
        let default_profile = crate::workspace::WorkspaceProfile::new(
            "/tmp/default-workspace".into(),
            Some("default".into()),
        );
        let mut custom_profile = crate::workspace::WorkspaceProfile::new(
            "/tmp/custom-workspace".into(),
            Some("custom".into()),
        );
        custom_profile.runtime.permission_mode = "safe".into();

        let mut data = AppData {
            profiles: vec![default_profile, custom_profile],
            ..AppData::default()
        };
        data.profiles[0].runtime.inherit_global_execution_policy = false;
        data.profiles[1].runtime.inherit_global_execution_policy = false;

        assert!(apply_global_runtime_defaults(&mut data));
        assert!(data.profiles[0].runtime.inherit_global_execution_policy);
        assert!(!data.profiles[1].runtime.inherit_global_execution_policy);
    }

    #[test]
    fn global_mcp_migration_collapses_restore_state_and_unanimous_auth() {
        let mut first = crate::workspace::WorkspaceProfile::new("/tmp/a".into(), Some("a".into()));
        let mut second = crate::workspace::WorkspaceProfile::new("/tmp/b".into(), Some("b".into()));
        first.auth.auth_type = "bearer".into();
        second.auth.auth_type = "bearer".into();
        let mut data = AppData {
            profiles: vec![first, second],
            restore_mcp_workspace_ids: vec!["a".into()],
            ..AppData::default()
        };

        assert!(apply_global_mcp_migration(&mut data));
        assert!(data.global_mcp_was_running);
        assert!(data.restore_mcp_workspace_ids.is_empty());
        assert_eq!(data.global_mcp_auth_type, "bearer");
        assert!(data.global_mcp_migration_notice.is_empty());
    }

    #[test]
    fn legacy_restore_workspace_ids_are_not_written_after_migration() {
        let mut data = AppData {
            restore_mcp_workspace_ids: vec!["legacy-workspace".into()],
            ..AppData::default()
        };

        assert!(apply_global_mcp_migration(&mut data));
        let serialized = serde_json::to_value(&data).expect("serialize migrated data");
        assert!(serialized.get("restore_mcp_workspace_ids").is_none());
        assert!(data.restore_mcp_workspace_ids.is_empty());
    }

    #[test]
    fn global_mcp_migration_fails_safe_on_conflicting_auth_and_tunnels() {
        let mut first = crate::workspace::WorkspaceProfile::new("/tmp/a".into(), Some("a".into()));
        let mut second = crate::workspace::WorkspaceProfile::new("/tmp/b".into(), Some("b".into()));
        first.auth.auth_type = "bearer".into();
        second.auth.auth_type = "oauth".into();
        first.tunnel.tunnel_type = "frp".into();
        first.tunnel.frp_subdomain = "a".into();
        second.tunnel.tunnel_type = "frp".into();
        second.tunnel.frp_subdomain = "b".into();
        let mut data = AppData {
            profiles: vec![first, second],
            ..AppData::default()
        };

        assert!(apply_global_mcp_migration(&mut data));
        assert_eq!(data.global_mcp_auth_type, "oauth");
        assert!(!data.global_gateway.enabled);
        assert!(data.global_mcp_migration_notice.contains("不同认证模式"));
        assert!(data
            .global_mcp_migration_notice
            .contains("多个旧 Workspace"));
    }

    #[test]
    fn global_mcp_migration_promotes_matching_workspace_bearer_secret() {
        let mut first = crate::workspace::WorkspaceProfile::new("/tmp/a".into(), Some("a".into()));
        let mut second = crate::workspace::WorkspaceProfile::new("/tmp/b".into(), Some("b".into()));
        first.auth.auth_type = "bearer".into();
        second.auth.auth_type = "bearer".into();
        first.auth.use_shared_secrets = false;
        second.auth.use_shared_secrets = false;
        let mut data = AppData {
            profiles: vec![first, second],
            ..AppData::default()
        };
        data.workspace_secrets
            .entry(data.profiles[0].id.clone())
            .or_default()
            .insert("bearer_token".into(), "same-token".into());
        data.workspace_secrets
            .entry(data.profiles[1].id.clone())
            .or_default()
            .insert("bearer_token".into(), "same-token".into());

        assert!(apply_global_mcp_migration(&mut data));
        assert_eq!(
            data.shared_secrets.get("bearer_token").map(String::as_str),
            Some("same-token")
        );
    }

    #[test]
    fn global_mcp_migration_reports_conflicting_workspace_bearer_secrets() {
        let mut first = crate::workspace::WorkspaceProfile::new("/tmp/a".into(), Some("a".into()));
        let mut second = crate::workspace::WorkspaceProfile::new("/tmp/b".into(), Some("b".into()));
        first.auth.auth_type = "bearer".into();
        second.auth.auth_type = "bearer".into();
        first.auth.use_shared_secrets = false;
        second.auth.use_shared_secrets = false;
        let mut data = AppData {
            profiles: vec![first, second],
            ..AppData::default()
        };
        data.workspace_secrets
            .entry(data.profiles[0].id.clone())
            .or_default()
            .insert("bearer_token".into(), "token-a".into());
        data.workspace_secrets
            .entry(data.profiles[1].id.clone())
            .or_default()
            .insert("bearer_token".into(), "token-b".into());

        assert!(apply_global_mcp_migration(&mut data));
        assert!(data.global_mcp_migration_notice.contains("bearer_token"));
    }
}
