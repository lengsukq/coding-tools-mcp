use super::*;

pub fn verify_oauth_bearer_header(
    headers: &HeaderMap,
    oauth: &OAuthRuntime,
    server_url: &str,
) -> Option<Response> {
    let Some(header_value) = headers.get(AUTHORIZATION) else {
        return Some((StatusCode::UNAUTHORIZED, "Missing Authorization header").into_response());
    };
    let Ok(header_str) = header_value.to_str() else {
        return Some((StatusCode::UNAUTHORIZED, "Invalid Authorization header").into_response());
    };
    let Some(token) = header_str.strip_prefix("Bearer ").map(str::trim) else {
        return Some((StatusCode::UNAUTHORIZED, "Invalid bearer token").into_response());
    };
    if oauth.verify_access_token(token, server_url) {
        None
    } else {
        Some((StatusCode::UNAUTHORIZED, "Invalid bearer token").into_response())
    }
}

#[derive(Debug, Deserialize)]
pub struct AuthorizeParams {
    pub response_type: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub code_challenge_method: String,
    #[serde(default)]
    pub state: String,
}

#[derive(Debug, Deserialize)]
pub struct AuthorizeForm {
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub code_challenge_method: String,
    #[serde(default)]
    pub state: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct TokenForm {
    pub grant_type: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub redirect_uri: String,
    #[serde(default)]
    pub code_verifier: String,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
    #[serde(default)]
    pub refresh_token: String,
}

pub fn authorize_get(
    oauth: &OAuthRuntime,
    params: AuthorizeParams,
    workspace_path: Option<&str>,
    server_url: &str,
) -> Response {
    if params.response_type != "code" {
        return html_error("response_type must be 'code'", StatusCode::BAD_REQUEST);
    }
    if !oauth.client_id_allowed(&params.client_id) {
        return html_error("Unknown client_id", StatusCode::BAD_REQUEST);
    }
    if !oauth.redirect_uri_allowed(&params.client_id, &params.redirect_uri) {
        return html_error(
            "redirect_uri is not registered for this client",
            StatusCode::BAD_REQUEST,
        );
    }
    if params.code_challenge_method != "S256" || params.code_challenge.is_empty() {
        return html_error(
            "code_challenge_method must be S256 and code_challenge is required",
            StatusCode::BAD_REQUEST,
        );
    }
    Html(login_page(LoginPage {
        client_id: &params.client_id,
        redirect_uri: &params.redirect_uri,
        code_challenge: &params.code_challenge,
        code_challenge_method: &params.code_challenge_method,
        state: &params.state,
        error: "",
        workspace_path,
        server_url,
    }))
    .into_response()
}

pub fn authorize_post(oauth: &OAuthRuntime, form: AuthorizeForm, server_url: &str) -> Response {
    if !oauth.client_id_allowed(&form.client_id) {
        return Html(login_page(LoginPage {
            client_id: &form.client_id,
            redirect_uri: &form.redirect_uri,
            code_challenge: &form.code_challenge,
            code_challenge_method: &form.code_challenge_method,
            state: &form.state,
            error: "Invalid client",
            workspace_path: None,
            server_url,
        }))
        .into_response();
    }
    if !oauth.redirect_uri_allowed(&form.client_id, &form.redirect_uri) {
        return html_error(
            "redirect_uri is not registered for this client",
            StatusCode::BAD_REQUEST,
        );
    }
    if form.code_challenge_method != "S256" || form.code_challenge.is_empty() {
        return Html(login_page(LoginPage {
            client_id: &form.client_id,
            redirect_uri: &form.redirect_uri,
            code_challenge: &form.code_challenge,
            code_challenge_method: &form.code_challenge_method,
            state: &form.state,
            error: "Invalid PKCE parameters",
            workspace_path: None,
            server_url,
        }))
        .into_response();
    }
    if !constant_time_eq_str(&form.password, &oauth.password) {
        return (
            StatusCode::UNAUTHORIZED,
            Html(login_page(LoginPage {
                client_id: &form.client_id,
                redirect_uri: &form.redirect_uri,
                code_challenge: &form.code_challenge,
                code_challenge_method: &form.code_challenge_method,
                state: &form.state,
                error: "Invalid password",
                workspace_path: None,
                server_url,
            })),
        )
            .into_response();
    }

    let server_url = server_url.trim_end_matches('/').to_string();
    let code = uuid::Uuid::new_v4().to_string().replace('-', "");
    let now = unix_now();
    {
        let mut pending = oauth.pending.lock().expect("oauth pending lock");
        pending.retain(|_, v| v.expires_at >= now);
        pending.insert(
            code.clone(),
            PendingCode {
                code_challenge: form.code_challenge.clone(),
                client_id: form.client_id.clone(),
                redirect_uri: form.redirect_uri.clone(),
                expires_at: now + OAUTH_CODE_TTL_SECONDS,
                server_url: server_url.clone(),
            },
        );
    }

    let mut qs = format!("code={}", urlencoding_encode(&code));
    if !form.state.is_empty() {
        qs.push_str(&format!("&state={}", urlencoding_encode(&form.state)));
    }
    let sep = if form.redirect_uri.contains('?') {
        '&'
    } else {
        '?'
    };
    // 授权页面通过 POST 表单提交，但客户端回调必须使用 GET。
    // 307 会保留 POST 并把表单体转发到 ChatGPT connector，导致 Bad Request。
    Redirect::to(&format!("{}{}{}", form.redirect_uri, sep, qs)).into_response()
}

pub fn token_exchange(
    oauth: &OAuthRuntime,
    headers: &HeaderMap,
    mut form: TokenForm,
    server_url: &str,
) -> Response {
    if let Some((id, secret)) = basic_auth_credentials(headers) {
        if form.client_id.is_empty() {
            form.client_id = id;
        }
        if form.client_secret.is_empty() {
            form.client_secret = secret;
        }
    }

    match form.grant_type.as_str() {
        "authorization_code" => authorization_code_exchange(oauth, form, server_url),
        "refresh_token" => refresh_token_exchange(oauth, form, server_url),
        _ => token_error(
            "unsupported_grant_type",
            "Only authorization_code and refresh_token are supported",
        ),
    }
}

fn authorization_code_exchange(
    oauth: &OAuthRuntime,
    form: TokenForm,
    server_url: &str,
) -> Response {
    if !oauth.client_id_allowed(&form.client_id)
        || !oauth.client_credentials_allowed(&form.client_id, &form.client_secret)
    {
        return token_error("invalid_client", "Invalid client credentials");
    }
    if form.code.is_empty() {
        return token_error("invalid_grant", "code is required");
    }
    if !valid_code_verifier(&form.code_verifier) {
        return token_error("invalid_grant", "Invalid code_verifier");
    }

    let code_data = {
        let mut pending = oauth.pending.lock().expect("oauth pending lock");
        pending.remove(&form.code)
    };
    let Some(code_data) = code_data else {
        return token_error(
            "invalid_grant",
            "Unknown or already-used authorization code",
        );
    };
    if unix_now() > code_data.expires_at {
        return token_error("invalid_grant", "Authorization code expired");
    }
    if !constant_time_eq_str(&code_data.client_id, &form.client_id) {
        return token_error("invalid_grant", "client_id mismatch");
    }
    if !constant_time_eq_str(&code_data.redirect_uri, &form.redirect_uri) {
        return token_error("invalid_grant", "redirect_uri mismatch");
    }
    if !verify_pkce(&form.code_verifier, &code_data.code_challenge) {
        return token_error("invalid_grant", "PKCE verification failed");
    }

    let issuer = if code_data.server_url.trim().is_empty() {
        server_url.trim_end_matches('/').to_string()
    } else {
        code_data.server_url.trim_end_matches('/').to_string()
    };
    issue_token_pair(oauth, &issuer, &form.client_id)
}

fn refresh_token_exchange(oauth: &OAuthRuntime, mut form: TokenForm, server_url: &str) -> Response {
    if form.refresh_token.is_empty() {
        return token_error("invalid_grant", "refresh_token is required");
    }
    let issuer = server_url.trim_end_matches('/');
    let claims = match decode_token_claims(&form.refresh_token, &oauth.token_secret, issuer) {
        Ok(claims) if claims.token_use == "refresh" => claims,
        _ => return token_error("invalid_grant", "Invalid refresh_token"),
    };
    if form.client_id.is_empty() {
        form.client_id = claims.client_id.clone();
    }
    if !constant_time_eq_str(&form.client_id, &claims.client_id)
        || !oauth.client_id_allowed(&form.client_id)
        || !oauth.client_credentials_allowed(&form.client_id, &form.client_secret)
    {
        return token_error("invalid_client", "Invalid client credentials");
    }
    issue_token_pair(oauth, issuer, &form.client_id)
}

fn issue_token_pair(oauth: &OAuthRuntime, issuer: &str, client_id: &str) -> Response {
    let access = create_token(
        issuer,
        &oauth.token_secret,
        OAUTH_TOKEN_TTL_SECONDS,
        client_id,
        "access",
    );
    let refresh = create_token(
        issuer,
        &oauth.token_secret,
        OAUTH_REFRESH_TOKEN_TTL_SECONDS,
        client_id,
        "refresh",
    );
    match (access, refresh) {
        (Ok(access_token), Ok(refresh_token)) => (
            StatusCode::OK,
            axum::Json(json!({
                "access_token": access_token,
                "token_type": "Bearer",
                "expires_in": OAUTH_TOKEN_TTL_SECONDS,
                "refresh_token": refresh_token
            })),
        )
            .into_response(),
        _ => token_error("server_error", "Failed to issue token pair"),
    }
}
