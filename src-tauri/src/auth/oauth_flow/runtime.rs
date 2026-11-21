use super::*;

const MAX_REGISTRY_BACKUPS: usize = 3;
pub(super) const MAX_REGISTERED_CLIENTS: usize = 256;

#[derive(Debug)]
pub(super) enum RegisterClientError {
    LimitReached,
    Persistence(String),
}

fn registry_backup_item_id(item_id: &str, slot: usize) -> String {
    format!("{item_id}.backup-{slot}")
}

fn parse_client_registry(raw: &str) -> Result<HashMap<String, RegisteredClient>, String> {
    serde_json::from_str::<HashMap<String, RegisteredClient>>(raw)
        .map_err(|error| format!("OAuth client registry is corrupt: {error}"))
}

fn rotate_registry_backups(raw: &str, existing: &[Option<String>]) -> Vec<Option<String>> {
    let mut rotated = vec![Some(raw.to_string())];
    rotated.extend(
        existing
            .iter()
            .take(MAX_REGISTRY_BACKUPS.saturating_sub(1))
            .cloned(),
    );
    rotated
}

fn persist_registry_backups(
    item_id: &str,
    backups: &[Option<String>],
    mut persist: impl FnMut(&str, &str) -> Result<(), String>,
) -> Result<(), String> {
    for (index, backup) in backups.iter().enumerate() {
        if let Some(backup) = backup {
            persist(&registry_backup_item_id(item_id, index + 1), backup)?;
        }
    }
    Ok(())
}

fn recover_corrupt_client_registry(
    item_id: &str,
    raw: &str,
    existing_backups: &[Option<String>],
    mut persist: impl FnMut(&str, &str) -> Result<(), String>,
) -> Result<(HashMap<String, RegisteredClient>, String), String> {
    let parse_error = parse_client_registry(raw)
        .err()
        .ok_or_else(|| "OAuth client registry is unexpectedly valid".to_string())?;

    let rotated_backups = rotate_registry_backups(raw, existing_backups);
    persist_registry_backups(item_id, &rotated_backups, &mut persist)?;
    for (index, backup) in rotated_backups.iter().enumerate().skip(1) {
        let Some(backup) = backup else {
            continue;
        };
        if let Ok(clients) = parse_client_registry(backup) {
            persist(item_id, backup)
                .map_err(|error| format!("Unable to restore OAuth client registry: {error}"))?;
            return Ok((
                clients,
                format!(
                    "[oauth] client registry was corrupt and restored from backup slot {}",
                    index + 1
                ),
            ));
        }
    }

    persist(item_id, "{}")
        .map_err(|error| format!("Unable to reset OAuth client registry: {error}"))?;
    Ok((
        HashMap::new(),
        format!(
            "[oauth] client registry was corrupt and reset to an empty registry; original value preserved in backup slot 1 ({parse_error})"
        ),
    ))
}

fn load_registry_backups(scope: &str, item_id: &str) -> Result<Vec<Option<String>>, String> {
    (1..=MAX_REGISTRY_BACKUPS)
        .map(|slot| {
            SecretStore::get_app(scope, &registry_backup_item_id(item_id, slot))
                .map_err(|error| format!("Unable to inspect OAuth registry backup: {error}"))
        })
        .collect()
}

fn load_app_client_registry(
    scope: &str,
    item_id: &str,
) -> Result<(HashMap<String, RegisteredClient>, Option<String>), String> {
    let Some(raw) = SecretStore::get_app(scope, item_id)
        .map_err(|error| format!("Unable to load OAuth client registry: {error}"))?
    else {
        return Ok((HashMap::new(), None));
    };

    match parse_client_registry(&raw) {
        Ok(clients) => {
            let backups = load_registry_backups(scope, item_id)?;
            let has_valid_backup = backups
                .iter()
                .flatten()
                .any(|backup| parse_client_registry(backup).is_ok());
            if !has_valid_backup {
                let seeded_backups = rotate_registry_backups(&raw, &backups);
                if let Err(error) =
                    persist_registry_backups(item_id, &seeded_backups, |key, value| {
                        SecretStore::set_app(scope, key, value).map_err(|error| error.to_string())
                    })
                {
                    eprintln!("Unable to seed OAuth registry backups: {error}");
                }
            }
            Ok((clients, None))
        }
        Err(_) => {
            let backups = load_registry_backups(scope, item_id)?;
            let (clients, notice) =
                recover_corrupt_client_registry(item_id, &raw, &backups, |key, value| {
                    SecretStore::set_app(scope, key, value).map_err(|error| error.to_string())
                })?;
            Ok((clients, Some(notice)))
        }
    }
}

fn persist_client_registry(
    registry: &ClientRegistryPersistence,
    clients: &HashMap<String, RegisteredClient>,
) -> Result<(), String> {
    let raw = serde_json::to_string(clients)
        .map_err(|error| format!("Unable to serialize OAuth client registry: {error}"))?;
    match registry {
        #[cfg(test)]
        ClientRegistryPersistence::Workspace {
            workspace_id,
            secret_key,
        } => SecretStore::set(workspace_id, secret_key, &raw)
            .map_err(|error| format!("Unable to persist OAuth client registry: {error}")),
        ClientRegistryPersistence::App { scope, item_id } => {
            SecretStore::set_app(scope, item_id, &raw)
                .map_err(|error| format!("Unable to persist OAuth client registry: {error}"))?;
            match load_registry_backups(scope, item_id) {
                Ok(existing_backups) => {
                    let rotated_backups = rotate_registry_backups(&raw, &existing_backups);
                    if let Err(error) =
                        persist_registry_backups(item_id, &rotated_backups, |key, value| {
                            SecretStore::set_app(scope, key, value)
                                .map_err(|error| error.to_string())
                        })
                    {
                        eprintln!("Unable to persist OAuth registry backups: {error}");
                    }
                }
                Err(error) => {
                    eprintln!("Unable to inspect OAuth registry backups: {error}");
                }
            }
            Ok(())
        }
    }
}

impl OAuthRuntime {
    pub fn new(
        client_id: String,
        client_secret: Option<String>,
        password: String,
        token_secret: String,
    ) -> Self {
        Self {
            client_id,
            client_secret,
            password,
            token_secret,
            pending: Arc::new(Mutex::new(HashMap::new())),
            clients: Arc::new(Mutex::new(HashMap::new())),
            persist_lock: Arc::new(Mutex::new(())),
            client_registry: None,
            recovery_notice: None,
        }
    }

    #[cfg(test)]
    pub fn new_persistent(
        client_id: String,
        client_secret: Option<String>,
        password: String,
        token_secret: String,
        workspace_id: String,
        secret_key: String,
    ) -> Result<Self, String> {
        let clients = SecretStore::get(&workspace_id, &secret_key)
            .map_err(|error| format!("Unable to load OAuth client registry: {error}"))?
            .map(|raw| parse_client_registry(&raw))
            .transpose()?
            .unwrap_or_default();
        let mut runtime = Self::new(client_id, client_secret, password, token_secret);
        runtime.clients = Arc::new(Mutex::new(clients));
        runtime.client_registry = Some(ClientRegistryPersistence::Workspace {
            workspace_id,
            secret_key,
        });
        Ok(runtime)
    }

    pub fn new_app_persistent(
        client_id: String,
        client_secret: Option<String>,
        password: String,
        token_secret: String,
        scope: String,
        item_id: String,
    ) -> Result<Self, String> {
        let (clients, recovery_notice) = load_app_client_registry(&scope, &item_id)?;
        let mut runtime = Self::new(client_id, client_secret, password, token_secret);
        runtime.clients = Arc::new(Mutex::new(clients));
        runtime.client_registry = Some(ClientRegistryPersistence::App { scope, item_id });
        runtime.recovery_notice = recovery_notice;
        Ok(runtime)
    }

    pub fn recovery_notice(&self) -> Option<&str> {
        self.recovery_notice.as_deref()
    }

    pub(super) fn lock_pending(&self) -> std::sync::MutexGuard<'_, HashMap<String, PendingCode>> {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lock_clients(&self) -> std::sync::MutexGuard<'_, HashMap<String, RegisteredClient>> {
        self.clients
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lock_persist(&self) -> std::sync::MutexGuard<'_, ()> {
        self.persist_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(super) fn insert_registered_client(
        &self,
        client_id: String,
        client: RegisteredClient,
    ) -> Result<(), RegisterClientError> {
        let _persist_guard = self.lock_persist();
        let snapshot = {
            let clients = self.lock_clients();
            if clients.len() >= MAX_REGISTERED_CLIENTS && !clients.contains_key(&client_id) {
                return Err(RegisterClientError::LimitReached);
            }
            let mut next = clients.clone();
            next.insert(client_id.clone(), client.clone());
            next
        };
        if let Some(registry) = &self.client_registry {
            persist_client_registry(registry, &snapshot)
                .map_err(RegisterClientError::Persistence)?;
        }
        self.lock_clients().insert(client_id, client);
        Ok(())
    }

    pub fn client_id_allowed(&self, client_id: &str) -> bool {
        if client_id.is_empty() {
            return false;
        }
        if self.client_id.is_empty() {
            return true;
        }
        constant_time_eq_str(client_id, &self.client_id)
            || self.lock_clients().contains_key(client_id)
    }

    pub(super) fn redirect_uri_allowed(&self, client_id: &str, redirect_uri: &str) -> bool {
        if constant_time_eq_str(client_id, &self.client_id) || self.client_id.is_empty() {
            return !redirect_uri.trim().is_empty();
        }
        self.lock_clients()
            .get(client_id)
            .is_some_and(|client| client.redirect_uris.iter().any(|uri| uri == redirect_uri))
    }

    pub(super) fn client_credentials_allowed(&self, client_id: &str, client_secret: &str) -> bool {
        if constant_time_eq_str(client_id, &self.client_id) || self.client_id.is_empty() {
            return self
                .client_secret
                .as_deref()
                .is_none_or(|expected| constant_time_eq_str(client_secret, expected));
        }
        let clients = self.lock_clients();
        let Some(client) = clients.get(client_id) else {
            return false;
        };
        if client.token_endpoint_auth_method == "none" {
            return true;
        }
        client
            .client_secret
            .as_deref()
            .is_some_and(|expected| constant_time_eq_str(client_secret, expected))
    }

    #[cfg(test)]
    pub fn verify_access_token(&self, token: &str, server_url: &str) -> bool {
        self.access_token_client_id(token, server_url).is_some()
    }

    pub fn access_token_client_id(&self, token: &str, server_url: &str) -> Option<String> {
        let server_url = server_url.trim_end_matches('/');
        decode_token_claims(token, &self.token_secret, server_url)
            .ok()
            .filter(|claims| claims.token_use == "access")
            .map(|claims| claims.client_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrupt_registry_rotates_backups_and_restores_the_oldest_valid_slot() {
        let mut persisted = HashMap::new();
        let backup = "{}";
        let (clients, notice) = recover_corrupt_client_registry(
            "oauth_dynamic_clients",
            "not-json",
            &[
                Some("also-not-json".to_string()),
                Some(backup.to_string()),
                Some("older-valid-value".to_string()),
            ],
            |key, value| {
                persisted.insert(key.to_string(), value.to_string());
                Ok(())
            },
        )
        .expect("restore corrupt registry");

        assert!(clients.is_empty());
        assert!(notice.contains("backup slot 3"));
        assert_eq!(
            persisted.get("oauth_dynamic_clients"),
            Some(&backup.to_string())
        );
        assert_eq!(
            persisted.get("oauth_dynamic_clients.backup-1"),
            Some(&"not-json".to_string())
        );
        assert_eq!(
            persisted.get("oauth_dynamic_clients.backup-2"),
            Some(&"also-not-json".to_string())
        );
        assert_eq!(
            persisted.get("oauth_dynamic_clients.backup-3"),
            Some(&backup.to_string())
        );
        assert_eq!(persisted.len(), 4);
    }

    #[test]
    fn corrupt_registry_is_backed_up_and_reset_without_valid_backup() {
        let mut persisted = HashMap::new();
        let raw = "not-json";
        let (clients, notice) = recover_corrupt_client_registry(
            "oauth_dynamic_clients",
            raw,
            &[None, None, None],
            |key, value| {
                persisted.insert(key.to_string(), value.to_string());
                Ok(())
            },
        )
        .expect("reset corrupt registry");

        assert!(clients.is_empty());
        assert!(notice.contains("reset to an empty registry"));
        assert_eq!(
            persisted.get("oauth_dynamic_clients"),
            Some(&"{}".to_string())
        );
        assert_eq!(
            persisted.get("oauth_dynamic_clients.backup-1"),
            Some(&raw.to_string())
        );
        assert_eq!(persisted.len(), 2);
    }

    #[test]
    fn registry_backup_rotation_keeps_at_most_three_slots() {
        let rotated = rotate_registry_backups(
            "new",
            &[
                Some("one".to_string()),
                Some("two".to_string()),
                Some("three".to_string()),
                Some("four".to_string()),
            ],
        );

        assert_eq!(
            rotated,
            vec![Some("new".into()), Some("one".into()), Some("two".into())]
        );
    }
}
