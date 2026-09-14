use std::sync::Mutex;

use crate::error::{AppError, AppResult};
use crate::workspace::legacy_import::import_legacy_profiles_if_empty;

use super::migrate::{data_file_path, load_or_migrate, maybe_backup_legacy_files, save};
use super::model::AppData;

mod secrets;
mod settings;
mod workspaces;

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

    fn update_latest<R>(&mut self, f: impl FnOnce(&mut AppData) -> AppResult<R>) -> AppResult<R> {
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
}

fn lock_data_file() -> AppResult<std::sync::MutexGuard<'static, ()>> {
    DATA_FILE_LOCK
        .lock()
        .map_err(|_| AppError::Message("data file lock poisoned".into()))
}

#[cfg(test)]
mod tests {
    use super::secrets::shared_value_for_key;
    use super::settings::apply_settings_update;
    use super::*;
    use crate::settings::AppSettings;

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
        store
            .update_settings(settings)
            .expect("update unrelated settings");

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
        assert_eq!(
            data.shared_secrets.get("bearer_token").map(String::as_str),
            Some("keep-shared")
        );
        assert!(!data.shared_secrets.contains_key("actions_api_key"));
        let workspace = data
            .workspace_secrets
            .get("workspace-1")
            .expect("workspace secrets");
        assert_eq!(
            workspace.get("oauth_password").map(String::as_str),
            Some("keep-workspace")
        );
        assert!(!workspace.contains_key("actions_oauth_dynamic_clients"));
    }
}
