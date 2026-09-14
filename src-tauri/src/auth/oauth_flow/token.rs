use super::*;

pub(super) fn create_token(
    server_url: &str,
    token_secret: &str,
    ttl: i64,
    client_id: &str,
    token_use: &str,
) -> Result<String, ()> {
    let now = unix_now() as i64;
    let claims = TokenClaims {
        iss: server_url.to_string(),
        aud: server_url.to_string(),
        iat: now,
        exp: now + ttl,
        scope: "mcp".into(),
        client_id: client_id.to_string(),
        token_use: token_use.to_string(),
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(token_secret.as_bytes()),
    )
    .map_err(|_| ())
}

pub(super) fn decode_token_claims(
    token: &str,
    token_secret: &str,
    server_url: &str,
) -> Result<TokenClaims, ()> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&[server_url]);
    validation.set_issuer(&[server_url]);
    decode::<TokenClaims>(
        token,
        &DecodingKey::from_secret(token_secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|_| ())
}

pub(super) fn verify_pkce(code_verifier: &str, code_challenge: &str) -> bool {
    let digest = Sha256::digest(code_verifier.as_bytes());
    let expected = URL_SAFE_NO_PAD.encode(digest);
    constant_time_eq_str(&expected, code_challenge)
}

pub(super) fn valid_code_verifier(verifier: &str) -> bool {
    (43..=128).contains(&verifier.len())
        && verifier
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | '_' | '~'))
}

pub(super) fn basic_auth_credentials(headers: &HeaderMap) -> Option<(String, String)> {
    let header = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let encoded = header.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let text = String::from_utf8(decoded).ok()?;
    let (id, secret) = text.split_once(':')?;
    Some((id.to_string(), secret.to_string()))
}

pub(super) fn token_error(error: &str, description: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        axum::Json(json!({
            "error": error,
            "error_description": description
        })),
    )
        .into_response()
}
