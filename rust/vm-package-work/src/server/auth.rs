use axum::{
    extract::{Request, State},
    http::{header, HeaderMap},
    middleware::Next,
    response::Response,
};
use vm_packages::{authorization_token, AgentCapabilityClaims};

use super::AppState;
use crate::{Store, WorkError, WorkResult};

#[derive(Debug, Clone)]
pub(super) struct AgentAccess(Option<AgentCapabilityClaims>);

impl AgentAccess {
    pub(super) fn consumer(&self) -> Option<&str> {
        self.0.as_ref().map(|claims| claims.consumer.as_str())
    }

    pub(super) fn canonical_repository(&self) -> Option<&str> {
        self.0
            .as_ref()
            .and_then(|claims| claims.canonical_repository.as_deref())
    }

    pub(super) fn is_agent(&self) -> bool {
        self.0.is_some()
    }
}

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

pub(super) fn agent_capability_access(
    state: &AppState,
    headers: &HeaderMap,
) -> WorkResult<AgentAccess> {
    let token = headers
        .get(vm_packages::AGENT_CAPABILITY_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(authorization_token)
        .ok_or_else(|| {
            WorkError::Unauthorized("missing package agent capability credential".into())
        })?;
    let claims = vm_packages::verify_agent_capability(&state.access.agent_signing_key, &token)
        .map_err(|_| {
            WorkError::Unauthorized("invalid package agent capability credential".into())
        })?;
    Ok(AgentAccess(Some(claims)))
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
    if let Some(expected) = access.consumer() {
        if consumers.len() != 1 || consumers.first().map(String::as_str) != Some(expected) {
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
    access.consumer().map_or(true, |consumer| {
        checkout
            .consumers
            .iter()
            .any(|candidate| candidate == consumer)
    })
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
) -> WorkResult<vm_packages::CheckoutRecord> {
    let checkout = store.get_checkout(checkout_id).await?;
    ensure_checkout_record_access(store, access, &checkout).await?;
    Ok(checkout)
}

pub(super) async fn ensure_checkout_record_access(
    store: &Store,
    access: &AgentAccess,
    checkout: &vm_packages::CheckoutRecord,
) -> WorkResult<()> {
    if let Some(consumer) = access.consumer() {
        ensure_requested_consumer(access, &checkout.consumers)?;
        if !checkout
            .consumers
            .iter()
            .any(|candidate| candidate == consumer)
        {
            return Err(WorkError::Unauthorized(
                "checkout is not assigned to this consumer".into(),
            ));
        }
        if checkout.workspace_release {
            let source = store.source(&checkout.package).await?;
            ensure_workspace_source_access(access, true, &source.repository)?;
        }
    }
    Ok(())
}

pub(super) fn ensure_workspace_source_access(
    access: &AgentAccess,
    workspace_release: bool,
    source_repository: &str,
) -> WorkResult<()> {
    if !workspace_release || !access.is_agent() {
        return Ok(());
    }
    let repository = access.canonical_repository().ok_or_else(|| {
        WorkError::Unauthorized(
            "canonical workspace release requires a repository-bound v2 credential".into(),
        )
    })?;
    if !vm_packages::repository_urls_equivalent(repository, source_repository) {
        return Err(WorkError::Unauthorized(
            "package agent credential is bound to a different canonical repository".into(),
        ));
    }
    Ok(())
}

pub(super) async fn ensure_submission_access(
    store: &Store,
    access: &AgentAccess,
    submission_id: &str,
) -> WorkResult<vm_packages::CheckoutRecord> {
    let submission = store.submission(submission_id).await?;
    ensure_checkout_access(store, access, &submission.checkout_id).await
}
