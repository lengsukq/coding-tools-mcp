use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::platform::platform;
use crate::settings::{
    default_global_allowed_commands, default_global_executable_paths, default_global_mcp_auth_type,
    default_global_permission_mode, AppSettings, GLOBAL_MCP_MIGRATION_VERSION,
    GLOBAL_RUNTIME_DEFAULTS_VERSION,
};
use crate::util::fs::atomic_write_json;
use crate::util::time::unix_now_secs;
use crate::workspace::legacy_import::import_legacy_profiles_if_empty;

use super::model::{AppData, LegacyProfilesOnlyFile};

const LEGACY_PROFILES_FILE: &str = "profiles.json";
const LEGACY_SETTINGS_FILE: &str = "app_settings.json";

pub(super) const OBSOLETE_ACTIONS_SECRET_KEYS: &[&str] = &[
    "actions_cloudflare_token",
    "actions_api_key",
    "actions_oauth_client_secret",
    "actions_oauth_password",
    "actions_oauth_token_secret",
    "actions_oauth_dynamic_clients",
    "actions_frp_token",
];

pub(super) struct LoadedAppData {
    pub data: AppData,
    pub existed_before: bool,
    pub needs_persist: bool,
}

pub fn data_file_path() -> AppResult<PathBuf> {
    Ok(platform()
        .app_config_dir()?
        .join("data")
        .join("profiles.json"))
}

pub(super) fn strip_obsolete_actions_secrets(data: &mut AppData) -> bool {
    let mut changed = false;
    for key in OBSOLETE_ACTIONS_SECRET_KEYS {
        changed |= data.shared_secrets.remove(*key).is_some();
    }
    for secrets in data.workspace_secrets.values_mut() {
        for key in OBSOLETE_ACTIONS_SECRET_KEYS {
            changed |= secrets.remove(*key).is_some();
        }
    }
    changed
}

pub(super) fn contains_obsolete_actions_json(value: &serde_json::Value) -> bool {
    let Some(root) = value.as_object() else {
        return false;
    };
    if root.contains_key("restore_actions_workspace_ids") {
        return true;
    }
    if root
        .get("profiles")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|profiles| {
            profiles.iter().any(|profile| {
                profile
                    .as_object()
                    .is_some_and(|object| object.contains_key("actions"))
            })
        })
    {
        return true;
    }
    ["shared_secrets", "workspace_secrets"]
        .into_iter()
        .filter_map(|key| root.get(key))
        .any(value_contains_obsolete_actions_secret)
}

fn value_contains_obsolete_actions_secret(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(object) => object.iter().any(|(key, value)| {
            OBSOLETE_ACTIONS_SECRET_KEYS.contains(&key.as_str())
                || value_contains_obsolete_actions_secret(value)
        }),
        serde_json::Value::Array(values) => {
            values.iter().any(value_contains_obsolete_actions_secret)
        }
        _ => false,
    }
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

fn backup_corrupt_profiles(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let ts = unix_now_secs();
    let mut candidate = parent.join(format!("profiles.json.corrupt.{ts}"));
    let mut suffix = 1u32;
    while candidate.exists() {
        candidate = parent.join(format!("profiles.json.corrupt.{ts}.{suffix}"));
        suffix = suffix.saturating_add(1);
    }
    let _ = fs::copy(path, &candidate);
    candidate
}

pub(super) fn load_for_store() -> AppResult<(PathBuf, LoadedAppData)> {
    let app_root = platform().app_config_dir()?;
    let path = app_root.join("data").join("profiles.json");
    let loaded = load_for_store_at(&app_root, &path, true)?;
    Ok((path, loaded))
}

pub(super) fn load_for_store_at(
    app_root: &Path,
    path: &Path,
    allow_legacy_home_import: bool,
) -> AppResult<LoadedAppData> {
    if path.exists() {
        let raw = fs::read_to_string(path)?;
        let mut data: AppData = match serde_json::from_str(&raw) {
            Ok(parsed) => parsed,
            Err(parse_err) => {
                let backup = backup_corrupt_profiles(path);
                return Err(AppError::Message(format!(
                    "配置文件 {} 解析失败（{parse_err}）。原始数据未覆盖并已备份至 {}，请检查或恢复该备份文件后重试。",
                    path.display(),
                    backup.display()
                )));
            }
        };
        let had_obsolete_actions_json = raw.contains("actions")
            && serde_json::from_str::<serde_json::Value>(&raw)
                .ok()
                .is_some_and(|value| contains_obsolete_actions_json(&value));

        let imported = if allow_legacy_home_import {
            import_legacy_profiles_if_empty(&mut data)?
        } else {
            0
        };
        if imported > 0 {
            data.global_runtime_defaults_version = 0;
            data.global_mcp_migration_version = 0;
        }

        let migrated =
            apply_global_runtime_defaults(&mut data) | apply_global_mcp_migration(&mut data);
        let removed_obsolete_actions = strip_obsolete_actions_secrets(&mut data);
        let needs_persist =
            migrated || imported > 0 || had_obsolete_actions_json || removed_obsolete_actions;

        return Ok(LoadedAppData {
            data,
            existed_before: true,
            needs_persist,
        });
    }

    let mut data = AppData {
        allow_local_noauth: true,
        ..AppData::default()
    };

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

    if allow_legacy_home_import {
        let _ = import_legacy_profiles_if_empty(&mut data)?;
    }

    apply_global_runtime_defaults(&mut data);
    apply_global_mcp_migration(&mut data);
    strip_obsolete_actions_secrets(&mut data);

    Ok(LoadedAppData {
        data,
        existed_before: false,
        needs_persist: true,
    })
}

pub(super) fn load_or_migrate_at(
    app_root: &Path,
    path: &Path,
    allow_legacy_home_import: bool,
) -> AppResult<AppData> {
    let loaded = load_for_store_at(app_root, path, allow_legacy_home_import)?;
    if loaded.existed_before && loaded.needs_persist {
        write_data(path, &loaded.data)?;
    }
    Ok(loaded.data)
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

pub(super) fn write_data(path: &Path, data: &AppData) -> AppResult<()> {
    atomic_write_json(path, data)
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
    data.global_mcp_server_name = settings.global_mcp_server_name;
    data.global_permission_mode = settings.global_permission_mode;
    data.global_allowed_commands = settings.global_allowed_commands;
    data.global_ai_instructions = settings.global_ai_instructions;
    data.global_instruction_sources = settings.global_instruction_sources;
    data.global_skill_sources = settings.global_skill_sources;
    data.global_custom_instruction_paths = settings.global_custom_instruction_paths;
    data.global_custom_skill_paths = settings.global_custom_skill_paths;
    data.global_disabled_instruction_ids = settings.global_disabled_instruction_ids;
    data.global_disabled_skill_ids = settings.global_disabled_skill_ids;
    data.allow_lan_access = settings.allow_lan_access;
    data.allow_local_noauth = settings.allow_local_noauth;
    data.restore_runtime_state_on_launch = settings.restore_runtime_state_on_launch;
    data.launch_at_login = settings.launch_at_login;
    data.launch_in_lightweight_mode = settings.launch_in_lightweight_mode;
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
    fn corrupt_profiles_json_is_backed_up_and_returns_error_without_overwriting() {
        let dir = tempfile::tempdir().expect("temp app root");
        let data_dir = dir.path().join("data");
        fs::create_dir_all(&data_dir).expect("create data dir");
        let path = data_dir.join("profiles.json");
        let broken = "{ \"profiles\": [ truncated";
        fs::write(&path, broken).expect("write corrupt json");

        let err = load_or_migrate_at(dir.path(), &path, false)
            .expect_err("corrupt profiles.json must return an explicit error");
        assert!(err.to_string().contains("profiles.json.corrupt."));
        assert_eq!(
            fs::read_to_string(&path).expect("original corrupt file must remain intact"),
            broken
        );

        let backups = fs::read_dir(&data_dir)
            .expect("list data dir")
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("profiles.json.corrupt.")
            })
            .collect::<Vec<_>>();
        assert_eq!(
            backups.len(),
            1,
            "exactly one corrupt backup should be created"
        );
        assert_eq!(
            fs::read_to_string(backups[0].path()).expect("read backup"),
            broken
        );
    }

    #[test]
    fn legacy_profiles_at_app_root_receive_0_3_migrations_before_completion() {
        let dir = tempfile::tempdir().expect("temp app root");
        let legacy_profiles = dir.path().join(LEGACY_PROFILES_FILE);
        let mut legacy_ws =
            crate::workspace::WorkspaceProfile::new("/tmp/legacy-ws".into(), Some("legacy".into()));
        legacy_ws.auth.auth_type = "bearer".into();
        legacy_ws.runtime.inherit_global_execution_policy = false;
        let payload = serde_json::json!({
            "profiles": [
                {
                    "id": legacy_ws.id,
                    "name": legacy_ws.name,
                    "path": legacy_ws.path,
                    "auth": { "type": "bearer", "use_shared_secrets": false },
                    "tunnel": { "type": "none" },
                    "runtime": legacy_ws.runtime
                }
            ]
        });
        fs::write(&legacy_profiles, payload.to_string()).expect("write legacy profiles");

        let target = dir.path().join("data").join("profiles.json");
        let loaded = load_or_migrate_at(dir.path(), &target, false)
            .expect("legacy load_or_migrate should succeed");
        assert_eq!(loaded.profiles.len(), 1);
        assert_eq!(
            loaded.global_runtime_defaults_version,
            GLOBAL_RUNTIME_DEFAULTS_VERSION
        );
        assert_eq!(
            loaded.global_mcp_migration_version,
            GLOBAL_MCP_MIGRATION_VERSION
        );
        assert_eq!(loaded.global_mcp_auth_type, "bearer");
        assert!(loaded.profiles[0].runtime.inherit_global_execution_policy);
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
