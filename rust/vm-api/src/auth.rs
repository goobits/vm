use crate::error::ApiError;
use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use vm_orchestrator::WorkspaceOrchestrator;

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

/// Check if the authenticated user owns the workspace
///
/// Returns Ok if the user owns the workspace, otherwise returns:
/// - ApiError::NotFound if the workspace doesn't exist
/// - ApiError::Forbidden if the workspace exists but the user is not the owner
pub async fn check_workspace_owner(
    orchestrator: &WorkspaceOrchestrator,
    workspace_id: &str,
    user: &AuthenticatedUser,
) -> Result<(), ApiError> {
    let workspace = orchestrator
        .get_workspace(workspace_id)
        .await
        .map_err(|_| ApiError::NotFound(format!("Workspace not found: {}", workspace_id)))?;

    if workspace.owner != user.username {
        return Err(ApiError::Forbidden(format!(
            "Access denied: workspace {} is owned by {}",
            workspace_id, workspace.owner
        )));
    }

    Ok(())
}
