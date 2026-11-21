use std::sync::Arc;

use axum::extract::{Form, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use super::{resolve_mcp_resource_base_with_policy, resolve_oauth_base_with_policy, ListenerState};
use crate::auth::{
    authorization_server_metadata, authorize_get, authorize_post, protected_resource_metadata,
    register_client, token_exchange, AuthorizeForm, AuthorizeParams, ClientRegistrationRequest,
    TokenForm,
};

pub(super) async fn oauth_register_post(
    State(state): State<ListenerState>,
    Json(request): Json<ClientRegistrationRequest>,
) -> Response {
    let policy = state.policy_snapshot();
    let Some(oauth) = policy.oauth.as_ref().map(Arc::clone) else {
        return oauth_not_configured();
    };
    tokio::task::spawn_blocking(move || register_client(&oauth, request))
        .await
        .unwrap_or_else(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "server_error",
                    "error_description": format!("Client registration worker failed: {err}")
                })),
            )
                .into_response()
        })
}

pub(super) async fn oauth_authorization_server_metadata(
    State(state): State<ListenerState>,
    headers: HeaderMap,
) -> Response {
    let policy = state.policy_snapshot();
    if !policy.auth.oauth_enabled() {
        return oauth_not_configured();
    }
    let base = resolve_oauth_base_with_policy(&policy, state.bind_port, &headers);
    Json(authorization_server_metadata(&base)).into_response()
}

pub(super) async fn oauth_protected_resource_metadata(
    State(state): State<ListenerState>,
    headers: HeaderMap,
) -> Response {
    let policy = state.policy_snapshot();
    if !policy.auth.oauth_enabled() {
        return oauth_not_configured();
    }
    let resource_base = resolve_mcp_resource_base_with_policy(&policy, state.bind_port, &headers);
    let issuer_base = resolve_oauth_base_with_policy(&policy, state.bind_port, &headers);
    let mut metadata = protected_resource_metadata(&resource_base);
    // Keep the configured authorization server (issuer) unchanged even when
    // tools/call is routed over localhost. Public clients are unaffected.
    metadata["authorization_servers"] = json!([issuer_base]);
    Json(metadata).into_response()
}

pub(super) async fn oauth_authorize_get(
    State(state): State<ListenerState>,
    headers: HeaderMap,
    Query(params): Query<AuthorizeParams>,
) -> Response {
    let policy = state.policy_snapshot();
    let Some(oauth) = policy.oauth.as_ref() else {
        return oauth_not_configured();
    };
    authorize_get(
        oauth,
        params,
        Some(state.scope_label.as_str()),
        &resolve_oauth_base_with_policy(&policy, state.bind_port, &headers),
    )
}

pub(super) async fn oauth_authorize_post(
    State(state): State<ListenerState>,
    headers: HeaderMap,
    Form(form): Form<AuthorizeForm>,
) -> Response {
    let policy = state.policy_snapshot();
    let Some(oauth) = policy.oauth.as_ref() else {
        return oauth_not_configured();
    };
    authorize_post(
        oauth,
        form,
        &resolve_oauth_base_with_policy(&policy, state.bind_port, &headers),
    )
}

pub(super) async fn oauth_token_post(
    State(state): State<ListenerState>,
    headers: HeaderMap,
    Form(form): Form<TokenForm>,
) -> Response {
    let policy = state.policy_snapshot();
    let Some(oauth) = policy.oauth.as_ref() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "unsupported_grant_type" })),
        )
            .into_response();
    };
    token_exchange(
        oauth,
        &headers,
        form,
        &resolve_oauth_base_with_policy(&policy, state.bind_port, &headers),
    )
}

fn oauth_not_configured() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "OAuth not configured" })),
    )
        .into_response()
}
