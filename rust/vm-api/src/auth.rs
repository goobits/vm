use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};

#[derive(Clone, Debug)]
pub struct AuthenticatedUser {
    pub username: String,
}

/// Middleware to extract authenticated user
/// TODO: Integrate with vm-auth-proxy for real GitHub OAuth
pub async fn auth_middleware(mut req: Request, next: Next) -> Result<Response, StatusCode> {
    // Phase 1: Mock auth (just extract from header)
    // Phase 2: Integrate with vm-auth-proxy OAuth flow

    let username = req
        .headers()
        .get("x-user")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("testuser")
        .to_string();

    req.extensions_mut().insert(AuthenticatedUser { username });

    Ok(next.run(req).await)
}
