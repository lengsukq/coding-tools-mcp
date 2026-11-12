use axum::extract::{Path, Query, State};
use axum::http::{
    header::{CACHE_CONTROL, CONTENT_SECURITY_POLICY, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS},
    StatusCode,
};
use axum::response::{IntoResponse, Response};
use axum::Json;

use super::ListenerState;

#[derive(serde::Deserialize)]
pub(super) struct ReviewQuery {
    t: String,
}

fn review_from_request(
    state: &ListenerState,
    id: &str,
    token: &str,
) -> Result<crate::review::ChangeSet, StatusCode> {
    let roots = state
        .gateway
        .registry
        .list()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .map(|profile| std::path::PathBuf::from(profile.path));
    crate::review::find_review(roots, id, token).map_err(|message| match message.as_str() {
        "invalid review token" => StatusCode::UNAUTHORIZED,
        "review expired" => StatusCode::GONE,
        _ => StatusCode::NOT_FOUND,
    })
}

pub(super) async fn review_api(
    State(state): State<ListenerState>,
    Path(id): Path<String>,
    Query(query): Query<ReviewQuery>,
) -> Response {
    match review_from_request(&state, &id, &query.t) {
        Ok(review) => (
            [
                (CACHE_CONTROL, "no-store"),
                (X_CONTENT_TYPE_OPTIONS, "nosniff"),
                (REFERRER_POLICY, "no-referrer"),
            ],
            Json(review),
        )
            .into_response(),
        Err(status) => status.into_response(),
    }
}

pub(super) async fn review_page(
    State(state): State<ListenerState>,
    Path(id): Path<String>,
    Query(query): Query<ReviewQuery>,
) -> Response {
    let review = match review_from_request(&state, &id, &query.t) {
        Ok(review) => review,
        Err(status) => return status.into_response(),
    };
    let data = serde_json::to_string(&review)
        .unwrap_or_else(|_| "{}".into())
        .replace('<', "\\u003c");
    let html = include_str!("review_page.html").replace("__REVIEW_DATA_JSON__", &data);
    let headers = [
        (CACHE_CONTROL, "no-store"),
        (X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (REFERRER_POLICY, "no-referrer"),
        (
            CONTENT_SECURITY_POLICY,
            "default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data:; base-uri 'none'; frame-ancestors 'none'",
        ),
    ];
    (headers, axum::response::Html(html)).into_response()
}
