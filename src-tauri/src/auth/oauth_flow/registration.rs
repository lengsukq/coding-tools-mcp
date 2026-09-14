use super::*;

pub fn register_client(oauth: &OAuthRuntime, request: ClientRegistrationRequest) -> Response {
    if request.redirect_uris.is_empty()
        || request
            .redirect_uris
            .iter()
            .any(|uri| !valid_redirect_uri(uri))
    {
        return registration_error(
            "invalid_redirect_uri",
            "redirect_uris must contain valid http(s) URLs",
        );
    }
    if !request.grant_types.is_empty()
        && request
            .grant_types
            .iter()
            .any(|grant| grant != "authorization_code" && grant != "refresh_token")
    {
        return registration_error("invalid_client_metadata", "Unsupported grant_types");
    }
    if !request.response_types.is_empty()
        && request
            .response_types
            .iter()
            .any(|response| response != "code")
    {
        return registration_error(
            "invalid_client_metadata",
            "Only response_type code is supported",
        );
    }
    let auth_method = match request.token_endpoint_auth_method.trim() {
        "" | "none" => "none",
        "client_secret_post" => "client_secret_post",
        "client_secret_basic" => "client_secret_basic",
        _ => {
            return registration_error(
                "invalid_client_metadata",
                "Unsupported token_endpoint_auth_method",
            )
        }
    };
    let client_id = format!("dcr-{}", uuid::Uuid::new_v4().simple());
    let client_secret = (auth_method != "none").then(|| uuid::Uuid::new_v4().simple().to_string());
    if let Err(error) = oauth.insert_registered_client(
        client_id.clone(),
        RegisteredClient {
            redirect_uris: request.redirect_uris.clone(),
            token_endpoint_auth_method: auth_method.to_string(),
            client_secret: client_secret.clone(),
        },
    ) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(json!({
                "error": "server_error",
                "error_description": error
            })),
        )
            .into_response();
    }

    let mut body = json!({
        "client_id": client_id,
        "client_id_issued_at": unix_now(),
        "redirect_uris": request.redirect_uris,
        "token_endpoint_auth_method": auth_method,
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"]
    });
    if !request.client_name.trim().is_empty() {
        body["client_name"] = Value::String(request.client_name);
    }
    if let Some(secret) = client_secret {
        body["client_secret"] = Value::String(secret);
    }
    (StatusCode::CREATED, axum::Json(body)).into_response()
}
