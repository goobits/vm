use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};

#[derive(Clone, Debug)]
pub struct AuthenticatedUser {
    pub username: String,
    pub email: Option<String>,
}

/// Auth middleware - extracts user from vm-auth-proxy headers
///
/// In production, vm-auth-proxy should be deployed in front of vm-api
/// and will set X-VM-User header after GitHub OAuth verification.
///
/// For local development without auth proxy, we fall back to x-user header.
pub async fn auth_middleware(mut req: Request, next: Next) -> Result<Response, StatusCode> {
    // Try vm-auth-proxy headers first (production)
    let username = req
        .headers()
        .get("x-vm-user")
        .or_else(|| req.headers().get("x-forwarded-user")) // oauth2-proxy format
        .or_else(|| req.headers().get("x-user")) // fallback for dev
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    let email = req
        .headers()
        .get("x-vm-email")
        .or_else(|| req.headers().get("x-forwarded-email"))
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    // If no username, return 401
    let username = username.ok_or(StatusCode::UNAUTHORIZED)?;

    req.extensions_mut()
        .insert(AuthenticatedUser { username, email });

    Ok(next.run(req).await)
}
