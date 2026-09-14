use crate::error::AppResult;

use super::{DataStore, SHARED_KEYS};

impl DataStore {
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

    pub fn regenerate_workspace_secret(
        &mut self,
        profile_id: &str,
        key: &str,
    ) -> AppResult<String> {
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
