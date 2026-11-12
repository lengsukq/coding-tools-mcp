use axum::extract::{Form, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use super::{resolve_oauth_base, ListenerState};
use crate::auth::{
    authorization_server_metadata, authorize_get, authorize_post, protected_resource_metadata,
    register_client, token_exchange, AuthorizeForm, AuthorizeParams, ClientRegistrationRequest,
    TokenForm,
};

pub(super) async fn oauth_register_post(
    State(state): State<ListenerState>,
    Json(request): Json<ClientRegistrationRequest>,
) -> Response {
    let Some(oauth) = state.oauth.as_ref() else {
        return oauth_not_configured();
    };
    register_client(oauth, request)
}

pub(super) async fn oauth_authorization_server_metadata(
    State(state): State<ListenerState>,
    headers: HeaderMap,
) -> Response {
    if !state.auth.oauth_enabled() {
        return oauth_not_configured();
    }
    let base = resolve_oauth_base(&state, &headers);
    Json(authorization_server_metadata(
        &base,
        state.oauth_client_secret.as_deref(),
    ))
    .into_response()
}

pub(super) async fn oauth_protected_resource_metadata(
    State(state): State<ListenerState>,
    headers: HeaderMap,
) -> Response {
    if !state.auth.oauth_enabled() {
        return oauth_not_configured();
    }
    Json(protected_resource_metadata(&resolve_oauth_base(
        &state, &headers,
    )))
    .into_response()
}

pub(super) async fn oauth_authorize_get(
    State(state): State<ListenerState>,
    headers: HeaderMap,
    Query(params): Query<AuthorizeParams>,
) -> Response {
    let Some(oauth) = state.oauth.as_ref() else {
        return oauth_not_configured();
    };
    authorize_get(
        oauth,
        params,
        Some(state.scope_label.as_str()),
        &resolve_oauth_base(&state, &headers),
    )
}

pub(super) async fn oauth_authorize_post(
    State(state): State<ListenerState>,
    headers: HeaderMap,
    Form(form): Form<AuthorizeForm>,
) -> Response {
    let Some(oauth) = state.oauth.as_ref() else {
        return oauth_not_configured();
    };
    authorize_post(oauth, form, &resolve_oauth_base(&state, &headers))
}

pub(super) async fn oauth_token_post(
    State(state): State<ListenerState>,
    headers: HeaderMap,
    Form(form): Form<TokenForm>,
) -> Response {
    let Some(oauth) = state.oauth.as_ref() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "unsupported_grant_type" })),
        )
            .into_response();
    };
    token_exchange(oauth, &headers, form, &resolve_oauth_base(&state, &headers))
}

fn oauth_not_configured() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "OAuth not configured" })),
    )
        .into_response()
}
