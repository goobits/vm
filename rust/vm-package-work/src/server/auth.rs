use axum::{
    extract::{Request, State},
    http::header,
    middleware::Next,
    response::Response,
};
use vm_packages::authorization_token;

use super::AppState;
use crate::{Store, WorkError, WorkResult};

#[derive(Debug, Clone)]
pub(super) struct AgentAccess(pub(super) Option<String>);

pub(super) async fn read(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> WorkResult<Response> {
    let token = request_token(&request)?;
    let consumer = if state.access.tokens().contains(&token.as_str()) {
        None
    } else {
        Some(
            vm_packages::verify_agent_capability(&state.access.agent_signing_key, &token)
                .map_err(|_| WorkError::Unauthorized("invalid read credential".into()))?,
        )
    };
    request.extensions_mut().insert(AgentAccess(consumer));
    Ok(next.run(request).await)
}

pub(super) async fn agent(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> WorkResult<Response> {
    let token = request_token(&request)?;
    let consumer = if token == state.access.controller_token {
        None
    } else {
        Some(
            vm_packages::verify_agent_capability(&state.access.agent_signing_key, &token)
                .map_err(|_| WorkError::Unauthorized("invalid package agent credential".into()))?,
        )
    };
    request.extensions_mut().insert(AgentAccess(consumer));
    Ok(next.run(request).await)
}

pub(super) async fn release(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> WorkResult<Response> {
    authorize(
        &request,
        &[&state.access.release_token, &state.access.controller_token],
        "release",
    )?;
    Ok(next.run(request).await)
}

pub(super) async fn build(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> WorkResult<Response> {
    authorize(
        &request,
        &[&state.access.build_token, &state.access.controller_token],
        "build",
    )?;
    Ok(next.run(request).await)
}

pub(super) async fn rollout(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> WorkResult<Response> {
    authorize(
        &request,
        &[&state.access.rollout_token, &state.access.controller_token],
        "rollout",
    )?;
    Ok(next.run(request).await)
}

pub(super) async fn reviewer(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> WorkResult<Response> {
    authorize(
        &request,
        &[&state.access.reviewer_token, &state.access.controller_token],
        "reviewer",
    )?;
    Ok(next.run(request).await)
}

pub(super) async fn controller(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> WorkResult<Response> {
    authorize(&request, &[&state.access.controller_token], "controller")?;
    Ok(next.run(request).await)
}

fn authorize(request: &Request, allowed: &[&str], scope: &str) -> WorkResult<()> {
    let token = request_token(request)?;
    if allowed.contains(&token.as_str()) {
        Ok(())
    } else {
        Err(WorkError::Unauthorized(format!(
            "invalid {scope} credential"
        )))
    }
}

fn request_token(request: &Request) -> WorkResult<String> {
    request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(authorization_token)
        .ok_or_else(|| WorkError::Unauthorized("missing authorization credential".into()))
}

pub(super) fn ensure_requested_consumer(
    access: &AgentAccess,
    consumers: &[String],
) -> WorkResult<()> {
    if let Some(expected) = &access.0 {
        if consumers.len() != 1 || consumers.first() != Some(expected) {
            return Err(WorkError::Unauthorized(
                "package agent credential is bound to a different consumer".into(),
            ));
        }
    }
    Ok(())
}

pub(super) fn checkout_is_visible(
    access: &AgentAccess,
    checkout: &vm_packages::CheckoutRecord,
) -> bool {
    access
        .0
        .as_ref()
        .map_or(true, |consumer| checkout.consumers.contains(consumer))
}

pub(super) fn ensure_checkout_is_visible(
    access: &AgentAccess,
    checkout: &vm_packages::CheckoutRecord,
) -> WorkResult<()> {
    if checkout_is_visible(access, checkout) {
        Ok(())
    } else {
        Err(WorkError::Unauthorized(
            "checkout is not assigned to this consumer".into(),
        ))
    }
}

pub(super) async fn visible_checkout_ids(
    store: &Store,
    access: &AgentAccess,
) -> WorkResult<std::collections::HashSet<String>> {
    Ok(store
        .list_checkouts()
        .await?
        .into_iter()
        .filter(|checkout| checkout_is_visible(access, checkout))
        .map(|checkout| checkout.checkout_id)
        .collect())
}

pub(super) async fn ensure_checkout_access(
    store: &Store,
    access: &AgentAccess,
    checkout_id: &str,
) -> WorkResult<()> {
    if let Some(consumer) = &access.0 {
        let checkout = store.get_checkout(checkout_id).await?;
        ensure_requested_consumer(access, &checkout.consumers)?;
        if !checkout.consumers.contains(consumer) {
            return Err(WorkError::Unauthorized(
                "checkout is not assigned to this consumer".into(),
            ));
        }
    }
    Ok(())
}

pub(super) async fn ensure_submission_access(
    store: &Store,
    access: &AgentAccess,
    submission_id: &str,
) -> WorkResult<()> {
    if access.0.is_some() {
        let submission = store.submission(submission_id).await?;
        ensure_checkout_access(store, access, &submission.checkout_id).await?;
    }
    Ok(())
}
