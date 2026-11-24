use crate::error::AppResult;

use super::{DataStore, SHARED_KEYS};

impl DataStore {
    pub fn init_shared_secrets(&mut self) -> AppResult<()> {
        self.update_latest_if_changed(|data| {
            let mut changed = false;
            for key in SHARED_KEYS {
                if !data.shared_secrets.contains_key(*key) {
                    data.shared_secrets
                        .insert((*key).to_string(), shared_value_for_key(key));
                    changed = true;
                }
            }
            Ok((changed, ()))
        })
    }

    pub fn get_shared_secret(&self, key: &str) -> Option<String> {
        self.data.shared_secrets.get(key).cloned()
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
