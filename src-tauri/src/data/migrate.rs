use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AppResult;
use crate::platform::platform;
use crate::settings::{
    default_global_allowed_commands, default_global_executable_paths,
    default_global_permission_mode, AppSettings, GLOBAL_RUNTIME_DEFAULTS_VERSION,
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

pub fn load_or_migrate() -> AppResult<AppData> {
    let path = data_file_path()?;
    if path.exists() {
        let raw = fs::read_to_string(&path)?;
        let mut data: AppData = serde_json::from_str(&raw).unwrap_or_default();
        if apply_global_runtime_defaults(&mut data) {
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
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(data)?;
    fs::write(path, format!("{text}\n"))?;
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
    data.global_permission_mode = settings.global_permission_mode;
    data.global_allowed_commands = settings.global_allowed_commands;
    data.global_ai_instructions = settings.global_ai_instructions;
    data.global_instruction_sources = settings.global_instruction_sources;
    data.global_skill_sources = settings.global_skill_sources;
    data.global_custom_instruction_paths = settings.global_custom_instruction_paths;
    data.global_custom_skill_paths = settings.global_custom_skill_paths;
    data.allow_lan_access = settings.allow_lan_access;
    data.restore_runtime_state_on_launch = settings.restore_runtime_state_on_launch;
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
}
