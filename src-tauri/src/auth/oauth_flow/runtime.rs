use super::*;

impl OAuthRuntime {
    pub fn new(
        _base_url: String,
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
            client_registry: None,
        }
    }

    #[cfg(test)]
    pub fn new_persistent(
        base_url: String,
        client_id: String,
        client_secret: Option<String>,
        password: String,
        token_secret: String,
        workspace_id: String,
        secret_key: String,
    ) -> Result<Self, String> {
        let clients = SecretStore::get(&workspace_id, &secret_key)
            .map_err(|error| format!("Unable to load OAuth client registry: {error}"))?
            .map(|raw| {
                serde_json::from_str::<HashMap<String, RegisteredClient>>(&raw)
                    .map_err(|error| format!("OAuth client registry is corrupt: {error}"))
            })
            .transpose()?
            .unwrap_or_default();
        let mut runtime = Self::new(base_url, client_id, client_secret, password, token_secret);
        runtime.clients = Arc::new(Mutex::new(clients));
        runtime.client_registry = Some(ClientRegistryPersistence::Workspace {
            workspace_id,
            secret_key,
        });
        Ok(runtime)
    }

    pub fn new_app_persistent(
        base_url: String,
        client_id: String,
        client_secret: Option<String>,
        password: String,
        token_secret: String,
        scope: String,
        item_id: String,
    ) -> Result<Self, String> {
        let clients = SecretStore::get_app(&scope, &item_id)
            .map_err(|error| format!("Unable to load OAuth client registry: {error}"))?
            .map(|raw| {
                serde_json::from_str::<HashMap<String, RegisteredClient>>(&raw)
                    .map_err(|error| format!("OAuth client registry is corrupt: {error}"))
            })
            .transpose()?
            .unwrap_or_default();
        let mut runtime = Self::new(base_url, client_id, client_secret, password, token_secret);
        runtime.clients = Arc::new(Mutex::new(clients));
        runtime.client_registry = Some(ClientRegistryPersistence::App { scope, item_id });
        Ok(runtime)
    }

    pub(super) fn insert_registered_client(
        &self,
        client_id: String,
        client: RegisteredClient,
    ) -> Result<(), String> {
        let mut clients = self.clients.lock().expect("oauth clients lock");
        let mut next = clients.clone();
        next.insert(client_id, client);
        if let Some(registry) = &self.client_registry {
            let raw = serde_json::to_string(&next)
                .map_err(|error| format!("Unable to serialize OAuth client registry: {error}"))?;
            match registry {
                #[cfg(test)]
                ClientRegistryPersistence::Workspace {
                    workspace_id,
                    secret_key,
                } => SecretStore::set(workspace_id, secret_key, &raw),
                ClientRegistryPersistence::App { scope, item_id } => {
                    SecretStore::set_app(scope, item_id, &raw)
                }
            }
            .map_err(|error| format!("Unable to persist OAuth client registry: {error}"))?;
        }
        *clients = next;
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
            || self
                .clients
                .lock()
                .expect("oauth clients lock")
                .contains_key(client_id)
    }

    pub(super) fn redirect_uri_allowed(&self, client_id: &str, redirect_uri: &str) -> bool {
        if constant_time_eq_str(client_id, &self.client_id) || self.client_id.is_empty() {
            return !redirect_uri.trim().is_empty();
        }
        self.clients
            .lock()
            .expect("oauth clients lock")
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
        let clients = self.clients.lock().expect("oauth clients lock");
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

    pub fn verify_access_token(&self, token: &str, server_url: &str) -> bool {
        let server_url = server_url.trim_end_matches('/');
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_audience(&[server_url]);
        validation.set_issuer(&[server_url]);
        decode::<TokenClaims>(
            token,
            &DecodingKey::from_secret(self.token_secret.as_bytes()),
            &validation,
        )
        .is_ok_and(|data| data.claims.token_use == "access")
    }
}
