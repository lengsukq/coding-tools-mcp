use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::http::{header::AUTHORIZATION, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::bearer::constant_time_eq_str;
use crate::secret::SecretStore;

mod handlers;
mod login;
mod registration;
mod runtime;
mod token;

pub use handlers::{
    authorize_get, authorize_post, token_exchange, verify_oauth_bearer_header, AuthorizeForm,
    AuthorizeParams, TokenForm,
};
use login::{html_error, login_page, urlencoding_encode, LoginPage};
pub use registration::register_client;
use token::{
    basic_auth_credentials, create_token, decode_token_claims, token_error, valid_code_verifier,
    verify_pkce,
};

pub const OAUTH_CODE_TTL_SECONDS: u64 = 300;
pub const OAUTH_TOKEN_TTL_SECONDS: i64 = 60 * 60 * 24 * 30;
pub const OAUTH_REFRESH_TOKEN_TTL_SECONDS: i64 = 60 * 60 * 24 * 90;

#[derive(Clone)]
pub struct OAuthRuntime {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub password: String,
    pub token_secret: String,
    pending: Arc<Mutex<HashMap<String, PendingCode>>>,
    clients: Arc<Mutex<HashMap<String, RegisteredClient>>>,
    client_registry: Option<ClientRegistryPersistence>,
}

#[derive(Clone)]
struct ClientRegistryPersistence {
    workspace_id: String,
    secret_key: String,
}

fn registration_error(error: &str, description: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        axum::Json(json!({
            "error": error,
            "error_description": description
        })),
    )
        .into_response()
}

fn valid_redirect_uri(uri: &str) -> bool {
    let uri = uri.trim();
    (uri.starts_with("https://") || uri.starts_with("http://")) && !uri.contains(['\r', '\n', '#'])
}

#[derive(Clone, Serialize, Deserialize)]
struct RegisteredClient {
    redirect_uris: Vec<String>,
    token_endpoint_auth_method: String,
    client_secret: Option<String>,
}

#[derive(Clone)]
struct PendingCode {
    code_challenge: String,
    client_id: String,
    redirect_uri: String,
    expires_at: u64,
    server_url: String,
}

#[derive(Serialize, Deserialize)]
struct TokenClaims {
    iss: String,
    aud: String,
    iat: i64,
    exp: i64,
    scope: String,
    client_id: String,
    token_use: String,
}

#[derive(Debug, Deserialize)]
pub struct ClientRegistrationRequest {
    pub redirect_uris: Vec<String>,
    #[serde(default)]
    pub token_endpoint_auth_method: String,
    #[serde(default)]
    pub grant_types: Vec<String>,
    #[serde(default)]
    pub response_types: Vec<String>,
    #[serde(default)]
    pub client_name: String,
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_exchange_without_client_secret() {
        use axum::http::HeaderMap;

        let oauth = OAuthRuntime::new(
            "https://lb.example.com".into(),
            "chatgpt-client-test".into(),
            None,
            "test-password".into(),
            "token-signing-secret".into(),
        );
        let verifier = "dBjftJeZ4CVP-mB92Kpru-AEJvkQlLgi3ThpmQ45N_Xyo";
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let redirect_uri = "https://chatgpt.com/connector/oauth/test";
        let redirect = authorize_post(
            &oauth,
            AuthorizeForm {
                client_id: "chatgpt-client-test".into(),
                redirect_uri: redirect_uri.into(),
                code_challenge: challenge,
                code_challenge_method: "S256".into(),
                state: "state".into(),
                password: "test-password".into(),
            },
            "https://lb.example.com",
        );
        assert_eq!(redirect.status(), StatusCode::SEE_OTHER);
        let code = {
            let pending = oauth.pending.lock().expect("lock");
            pending.keys().next().cloned().unwrap()
        };

        let response = token_exchange(
            &oauth,
            &HeaderMap::new(),
            TokenForm {
                grant_type: "authorization_code".into(),
                code,
                redirect_uri: redirect_uri.into(),
                code_verifier: verifier.into(),
                client_id: "chatgpt-client-test".into(),
                client_secret: String::new(),
                refresh_token: String::new(),
            },
            "https://lb.example.com",
        );
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn refresh_token_cannot_authenticate_as_an_access_token() {
        let oauth = OAuthRuntime::new(
            "https://lb.example.com".into(),
            "chatgpt-client-test".into(),
            None,
            "test-password".into(),
            "token-signing-secret".into(),
        );
        let refresh = create_token(
            "https://lb.example.com",
            &oauth.token_secret,
            OAUTH_REFRESH_TOKEN_TTL_SECONDS,
            "chatgpt-client-test",
            "refresh",
        )
        .expect("refresh token");

        assert!(!oauth.verify_access_token(&refresh, "https://lb.example.com"));
    }

    #[test]
    fn pkce_round_trip() {
        let verifier = "dBjftJeZ4CVP-mB92Kpru-AEJvkQlLgi3ThpmQ45N_Xyo";
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        assert!(verify_pkce(verifier, &challenge));
    }

    #[test]
    fn dynamic_registration_restricts_redirect_uri() {
        let oauth = OAuthRuntime::new(
            "https://lb.example.com".into(),
            "legacy-client".into(),
            None,
            "test-password".into(),
            "token-signing-secret".into(),
        );
        let response = register_client(
            &oauth,
            ClientRegistrationRequest {
                redirect_uris: vec!["https://chatgpt.com/connector/oauth/test".into()],
                token_endpoint_auth_method: "none".into(),
                grant_types: vec!["authorization_code".into(), "refresh_token".into()],
                response_types: vec!["code".into()],
                client_name: "ChatGPT".into(),
            },
        );
        assert_eq!(response.status(), StatusCode::CREATED);
        let clients = oauth.clients.lock().expect("clients");
        let client_id = clients.keys().next().expect("client id").clone();
        drop(clients);
        assert!(oauth.redirect_uri_allowed(&client_id, "https://chatgpt.com/connector/oauth/test"));
        assert!(!oauth.redirect_uri_allowed(&client_id, "https://attacker.example/callback"));
    }

    #[test]
    fn dynamic_registration_survives_runtime_restart() {
        let workspace_id = format!("oauth-dcr-{}", uuid::Uuid::new_v4().simple());
        let registry_key = "oauth_dynamic_clients".to_string();
        let oauth = OAuthRuntime::new_persistent(
            "https://lb.example.com".into(),
            "legacy-client".into(),
            None,
            "test-password".into(),
            "token-signing-secret".into(),
            workspace_id.clone(),
            registry_key.clone(),
        )
        .expect("persistent oauth runtime");
        let response = register_client(
            &oauth,
            ClientRegistrationRequest {
                redirect_uris: vec!["https://chatgpt.com/connector/oauth/restart".into()],
                token_endpoint_auth_method: "none".into(),
                grant_types: vec!["authorization_code".into(), "refresh_token".into()],
                response_types: vec!["code".into()],
                client_name: "ChatGPT".into(),
            },
        );
        assert_eq!(response.status(), StatusCode::CREATED);
        let client_id = oauth
            .clients
            .lock()
            .expect("clients")
            .keys()
            .next()
            .expect("client id")
            .clone();
        drop(oauth);

        let restarted = OAuthRuntime::new_persistent(
            "https://lb.example.com".into(),
            "legacy-client".into(),
            None,
            "test-password".into(),
            "token-signing-secret".into(),
            workspace_id.clone(),
            registry_key,
        )
        .expect("restarted oauth runtime");
        assert!(restarted.client_id_allowed(&client_id));
        assert!(restarted
            .redirect_uri_allowed(&client_id, "https://chatgpt.com/connector/oauth/restart"));
        let _ = SecretStore::remove_workspace_secrets(&workspace_id);
    }

    #[test]
    fn refresh_token_issues_a_new_token_pair() {
        let oauth = OAuthRuntime::new(
            "https://lb.example.com".into(),
            "chatgpt-client-test".into(),
            None,
            "test-password".into(),
            "token-signing-secret".into(),
        );
        let refresh = create_token(
            "https://lb.example.com",
            &oauth.token_secret,
            OAUTH_REFRESH_TOKEN_TTL_SECONDS,
            "chatgpt-client-test",
            "refresh",
        )
        .expect("refresh token");
        let response = token_exchange(
            &oauth,
            &HeaderMap::new(),
            TokenForm {
                grant_type: "refresh_token".into(),
                client_id: "chatgpt-client-test".into(),
                refresh_token: refresh,
                ..TokenForm::default()
            },
            "https://lb.example.com",
        );
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn login_page_posts_back_to_workspace_oauth_path() {
        let html = login_page(LoginPage {
            client_id: "client",
            redirect_uri: "https://chatgpt.com/callback",
            code_challenge: "challenge",
            code_challenge_method: "S256",
            state: "state",
            error: "",
            workspace_path: Some("/workspace"),
            server_url: "https://mcp.example.com/w/workspace-id",
        });
        assert!(html.contains("action='https://mcp.example.com/w/workspace-id/oauth/authorize'"));
    }
}
