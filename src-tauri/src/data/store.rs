use std::sync::Mutex;

use crate::error::{AppError, AppResult};
use crate::settings::AppSettings;
use crate::workspace::legacy_import::import_legacy_profiles_if_empty;
use crate::workspace::WorkspaceProfile;

use super::migrate::{data_file_path, load_or_migrate, maybe_backup_legacy_files, save};
use super::model::AppData;

static DATA_FILE_LOCK: Mutex<()> = Mutex::new(());

const SHARED_KEYS: &[&str] = &[
    "oauth_client_id",
    "bearer_token",
    "oauth_client_secret",
    "oauth_password",
    "oauth_token_secret",
];

const OBSOLETE_ACTIONS_SECRET_KEYS: &[&str] = &[
    "actions_cloudflare_token",
    "actions_api_key",
    "actions_oauth_client_secret",
    "actions_oauth_password",
    "actions_oauth_token_secret",
    "actions_oauth_dynamic_clients",
    "actions_frp_token",
];

#[derive(Debug)]
pub struct DataStore {
    data: AppData,
}

fn apply_settings_update(base: &AppData, latest: &mut AppData, settings: AppSettings) {
    // Global Gateway may persist a newly resolved public URL directly from its
    // async runtime, outside the long-lived AppState DataStore snapshot. An
    // unrelated settings save must not overwrite that newer runtime value.
    let gateway_changed_by_caller = settings.global_gateway != base.global_gateway;
    let latest_gateway = latest.global_gateway.clone();

    settings.apply_to(latest);

    if !gateway_changed_by_caller {
        latest.global_gateway = latest_gateway;
    }
}

fn strip_obsolete_actions_secrets(data: &mut AppData) -> bool {
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

fn contains_obsolete_actions_json(value: &serde_json::Value) -> bool {
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

impl DataStore {
    pub fn load() -> AppResult<Self> {
        let _guard = lock_data_file()?;
        let path = data_file_path()?;
        let existed_before = path.exists();
        let had_obsolete_actions_json = if existed_before {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                .is_some_and(|value| contains_obsolete_actions_json(&value))
        } else {
            false
        };
        let mut data = load_or_migrate()?;
        let imported = import_legacy_profiles_if_empty(&mut data)?;
        let removed_obsolete_actions_secrets = strip_obsolete_actions_secrets(&mut data);
        let store = Self { data };
        if !existed_before
            || imported > 0
            || had_obsolete_actions_json
            || removed_obsolete_actions_secrets
        {
            store.persist_unlocked()?;
        }
        if !existed_before {
            maybe_backup_legacy_files(&path)?;
        }
        Ok(store)
    }

    pub fn read_file<R>(f: impl FnOnce(&AppData) -> AppResult<R>) -> AppResult<R> {
        let _guard = lock_data_file()?;
        let data = load_or_migrate()?;
        f(&data)
    }

    pub fn update_file<R>(f: impl FnOnce(&mut AppData) -> AppResult<R>) -> AppResult<R> {
        let _guard = lock_data_file()?;
        let mut data = load_or_migrate()?;
        let result = f(&mut data)?;
        save(&data)?;
        Ok(result)
    }

    pub fn data(&self) -> &AppData {
        &self.data
    }

    fn update_latest<R>(
        &mut self,
        f: impl FnOnce(&mut AppData) -> AppResult<R>,
    ) -> AppResult<R> {
        let _guard = lock_data_file()?;
        let mut data = load_or_migrate()?;
        let result = f(&mut data)?;
        save(&data)?;
        self.data = data;
        Ok(result)
    }

    fn persist_unlocked(&self) -> AppResult<()> {
        save(&self.data)
    }

    pub fn settings(&self) -> AppSettings {
        AppSettings::from_data(&self.data)
    }

    pub fn update_settings(&mut self, settings: AppSettings) -> AppResult<()> {
        let base = self.data.clone();
        self.update_latest(move |data| {
            apply_settings_update(&base, data, settings);
            Ok(())
        })
    }

    pub fn list(&self) -> &[WorkspaceProfile] {
        &self.data.profiles
    }

    pub fn get(&self, id: &str) -> Option<&WorkspaceProfile> {
        self.data.profiles.iter().find(|profile| profile.id == id)
    }

    pub fn add(&mut self, profile: WorkspaceProfile) -> AppResult<()> {
        self.update_latest(move |data| {
            data.profiles.push(profile);
            Ok(())
        })
    }

    pub fn update(&mut self, profile: WorkspaceProfile) -> AppResult<()> {
        self.update_latest(move |data| {
            let Some(index) = data
                .profiles
                .iter()
                .position(|item| item.id == profile.id)
            else {
                return Err(AppError::Message(format!(
                    "workspace not found: {}",
                    profile.id
                )));
            };
            data.profiles[index] = profile;
            Ok(())
        })
    }

    pub fn remove(&mut self, id: &str) -> AppResult<Option<WorkspaceProfile>> {
        self.update_latest(|data| {
            let Some(index) = data.profiles.iter().position(|item| item.id == id) else {
                return Ok(None);
            };
            let removed = data.profiles.remove(index);
            data.workspace_secrets.remove(id);
            Ok(Some(removed))
        })
    }

    pub fn init_workspace_secrets(&mut self, profile_id: &str) -> AppResult<()> {
        self.update_latest(|data| {
            // oauth_client_secret is optional for MCP OAuth (ChatGPT PKCE); not auto-generated.
            let secrets = data
                .workspace_secrets
                .entry(profile_id.to_string())
                .or_default();
            secrets.insert("oauth_password".into(), random_secret());
            secrets.insert("oauth_token_secret".into(), random_secret());
            secrets.insert("bearer_token".into(), random_secret());
            Ok(())
        })
    }

    pub fn init_shared_secrets(&mut self) -> AppResult<()> {
        self.update_latest(|data| {
            for key in SHARED_KEYS {
                data.shared_secrets
                    .entry((*key).to_string())
                    .or_insert_with(|| shared_value_for_key(key));
            }
            Ok(())
        })
    }

    pub fn get_workspace_secret(&self, profile_id: &str, key: &str) -> AppResult<Option<String>> {
        Self::read_file(|data| {
            Ok(data
                .workspace_secrets
                .get(profile_id)
                .and_then(|secrets| secrets.get(key))
                .filter(|value| !value.is_empty())
                .cloned())
        })
    }

    pub fn set_workspace_secret(
        &mut self,
        profile_id: &str,
        key: &str,
        value: &str,
    ) -> AppResult<()> {
        self.update_latest(|data| {
            data.workspace_secrets
                .entry(profile_id.to_string())
                .or_default()
                .insert(key.to_string(), value.to_string());
            Ok(())
        })
    }

    pub fn regenerate_workspace_secret(&mut self, profile_id: &str, key: &str) -> AppResult<String> {
        let value = shared_value_for_key(key);
        self.set_workspace_secret(profile_id, key, &value)?;
        Ok(value)
    }

    pub fn remove_workspace_secrets(&mut self, profile_id: &str) -> AppResult<()> {
        self.update_latest(|data| {
            data.workspace_secrets.remove(profile_id);
            Ok(())
        })
    }

    pub fn get_shared_secret(&self, key: &str) -> Option<String> {
        self.data.shared_secrets.get(key).cloned()
    }

    pub fn set_shared_secret(&mut self, key: &str, value: &str) -> AppResult<()> {
        self.update_latest(|data| {
            data.shared_secrets
                .insert(key.to_string(), value.to_string());
            Ok(())
        })
    }

    pub fn regenerate_shared_secret(&mut self, key: &str) -> AppResult<String> {
        let value = random_secret();
        self.set_shared_secret(key, &value)?;
        Ok(value)
    }

    pub fn get_app_secret(&self, scope: &str, item_id: &str) -> Option<String> {
        self.data
            .app_secrets
            .get(scope)
            .and_then(|items| items.get(item_id))
            .filter(|value| !value.is_empty())
            .cloned()
    }

    pub fn set_app_secret(&mut self, scope: &str, item_id: &str, value: &str) -> AppResult<()> {
        self.update_latest(|data| {
            data.app_secrets
                .entry(scope.to_string())
                .or_default()
                .insert(item_id.to_string(), value.to_string());
            Ok(())
        })
    }

    pub fn delete_app_secret(&mut self, scope: &str, item_id: &str) -> AppResult<()> {
        self.update_latest(|data| {
            if let Some(items) = data.app_secrets.get_mut(scope) {
                items.remove(item_id);
                if items.is_empty() {
                    data.app_secrets.remove(scope);
                }
            }
            Ok(())
        })
    }

}

fn lock_data_file() -> AppResult<std::sync::MutexGuard<'static, ()>> {
    DATA_FILE_LOCK
        .lock()
        .map_err(|_| AppError::Message("data file lock poisoned".into()))
}

fn random_secret() -> String {
    format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4()).replace('-', "")
}

fn shared_value_for_key(key: &str) -> String {
    if key == "oauth_client_id" {
        format!("chatgpt-client-{}", &uuid::Uuid::new_v4().to_string()[..12])
    } else {
        random_secret()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_secret_roundtrip() {
        let id = uuid::Uuid::new_v4().to_string().replace('-', "");
        let mut store = DataStore::load().expect("load");
        store
            .set_workspace_secret(&id, "oauth_client_secret", "roundtrip-secret")
            .expect("set");
        let loaded = store
            .get_workspace_secret(&id, "oauth_client_secret")
            .expect("get");
        assert_eq!(loaded.as_deref(), Some("roundtrip-secret"));
        store.remove_workspace_secrets(&id).expect("remove");
    }

    #[test]
    fn stale_store_update_does_not_overwrite_externally_persisted_workspace_secret() {
        let id = uuid::Uuid::new_v4().to_string().replace('-', "");
        let marker = format!("stale-store-regression-{id}");
        let mut store = DataStore::load().expect("load");

        crate::secret::SecretStore::set(&id, "oauth_dynamic_clients", &marker)
            .expect("persist external secret");

        let mut settings = store.settings();
        settings.last_workspace_id = format!("regression-{id}");
        store.update_settings(settings).expect("update unrelated settings");

        let persisted = crate::secret::SecretStore::get(&id, "oauth_dynamic_clients")
            .expect("read external secret");
        assert_eq!(persisted.as_deref(), Some(marker.as_str()));

        let _ = crate::secret::SecretStore::remove_workspace_secrets(&id);
    }

    #[test]
    fn unrelated_settings_update_preserves_newer_global_gateway_state() {
        let base = AppData::default();
        let mut latest = base.clone();
        latest.global_gateway.public_url = "https://runtime.example.test".into();

        let mut settings = AppSettings::from_data(&base);
        settings.last_workspace_id = "workspace-2".into();
        apply_settings_update(&base, &mut latest, settings);

        assert_eq!(latest.last_workspace_id, "workspace-2");
        assert_eq!(
            latest.global_gateway.public_url,
            "https://runtime.example.test"
        );
    }

    #[test]
    fn explicit_global_gateway_update_still_wins_over_runtime_snapshot() {
        let base = AppData::default();
        let mut latest = base.clone();
        latest.global_gateway.public_url = "https://runtime.example.test".into();

        let mut settings = AppSettings::from_data(&base);
        settings.global_gateway.enabled = true;
        settings.global_gateway.public_url = "https://configured.example.test".into();
        apply_settings_update(&base, &mut latest, settings);

        assert!(latest.global_gateway.enabled);
        assert_eq!(
            latest.global_gateway.public_url,
            "https://configured.example.test"
        );
    }

    #[test]
    fn shared_oauth_client_id_uses_client_id_format() {
        let value = shared_value_for_key("oauth_client_id");
        assert!(value.starts_with("chatgpt-client-"));
        assert_eq!(value.len(), "chatgpt-client-".len() + 12);
    }

    #[test]
    fn obsolete_actions_data_is_detected_and_removed_without_touching_mcp_secrets() {
        let legacy = serde_json::json!({
            "restore_actions_workspace_ids": ["workspace-1"],
            "profiles": [{"id": "workspace-1", "actions": {"local_port": 8787}}],
            "shared_secrets": {
                "bearer_token": "keep-shared",
                "actions_api_key": "remove-shared"
            },
            "workspace_secrets": {
                "workspace-1": {
                    "oauth_password": "keep-workspace",
                    "actions_oauth_dynamic_clients": "remove-workspace"
                }
            }
        });
        assert!(contains_obsolete_actions_json(&legacy));

        let mut data = AppData::default();
        data.shared_secrets
            .insert("bearer_token".into(), "keep-shared".into());
        data.shared_secrets
            .insert("actions_api_key".into(), "remove-shared".into());
        data.workspace_secrets.insert(
            "workspace-1".into(),
            std::collections::HashMap::from([
                ("oauth_password".into(), "keep-workspace".into()),
                (
                    "actions_oauth_dynamic_clients".into(),
                    "remove-workspace".into(),
                ),
            ]),
        );

        assert!(strip_obsolete_actions_secrets(&mut data));
        assert_eq!(data.shared_secrets.get("bearer_token").map(String::as_str), Some("keep-shared"));
        assert!(!data.shared_secrets.contains_key("actions_api_key"));
        let workspace = data.workspace_secrets.get("workspace-1").expect("workspace secrets");
        assert_eq!(workspace.get("oauth_password").map(String::as_str), Some("keep-workspace"));
        assert!(!workspace.contains_key("actions_oauth_dynamic_clients"));
    }
}
