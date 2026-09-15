use crate::error::AppResult;

use super::{DataStore, SHARED_KEYS};

impl DataStore {
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

    pub fn set_shared_secrets(&mut self, values: &[(String, String)]) -> AppResult<()> {
        self.update_latest(|data| {
            for (key, value) in values {
                data.shared_secrets.insert(key.clone(), value.clone());
            }
            Ok(())
        })
    }

    pub fn generate_shared_secret_value(key: &str) -> String {
        shared_value_for_key(key)
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

fn random_secret() -> String {
    format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4()).replace('-', "")
}

pub(super) fn shared_value_for_key(key: &str) -> String {
    if key == "oauth_client_id" {
        format!("chatgpt-client-{}", &uuid::Uuid::new_v4().to_string()[..12])
    } else {
        random_secret()
    }
}
