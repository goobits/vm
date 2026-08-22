use std::{path::PathBuf, sync::Arc};

use axum::{
    middleware,
    routing::{get, post},
    Json, Router,
};
use tokio::net::TcpListener;

use crate::{SourceManager, Store, WorkError, WorkResult};

mod agent;
pub(crate) mod auth;
mod bundles;
mod controller;
mod jobs;
mod read;

pub(crate) use auth::AgentAccess;

#[derive(Clone)]
pub struct WorkCredentials {
    read_token: String,
    controller_token: String,
    reviewer_token: String,
    build_token: String,
    release_token: String,
    rollout_token: String,
    agent_signing_key: String,
}

impl WorkCredentials {
    pub fn new(
        read: impl Into<String>,
        controller: impl Into<String>,
        reviewer: impl Into<String>,
        build: impl Into<String>,
        release: impl Into<String>,
        rollout: impl Into<String>,
        agent_key: impl Into<String>,
    ) -> Self {
        Self {
            read_token: read.into(),
            controller_token: controller.into(),
            reviewer_token: reviewer.into(),
            build_token: build.into(),
            release_token: release.into(),
            rollout_token: rollout.into(),
            agent_signing_key: agent_key.into(),
        }
    }

    fn tokens(&self) -> [&str; 6] {
        [
            &self.read_token,
            &self.controller_token,
            &self.reviewer_token,
            &self.build_token,
            &self.release_token,
            &self.rollout_token,
        ]
    }

    fn validate(&self) -> WorkResult<()> {
        let tokens = self.tokens();
        if tokens.iter().any(|token| token.trim().is_empty()) {
            return Err(WorkError::Invalid(
                "read, controller, reviewer, build, release, and rollout tokens are required"
                    .into(),
            ));
        }
        if tokens
            .iter()
            .enumerate()
            .any(|(index, token)| tokens[..index].contains(token))
        {
            return Err(WorkError::Invalid(
                "read, controller, reviewer, build, release, and rollout tokens must be distinct"
                    .into(),
            ));
        }
        if self.agent_signing_key.len() < 32 {
            return Err(WorkError::Invalid(
                "package agent signing key must contain at least 32 characters".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) store: Arc<Store>,
    source: SourceManager,
    access: WorkCredentials,
}

pub(crate) fn router(store: Arc<Store>, credentials: WorkCredentials) -> Router {
    let state = AppState {
        source: SourceManager::new(store.root()),
        store,
        access: credentials,
    };
    let reads = Router::new()
        .route("/v1/packages", get(read::list_packages))
        .route("/v1/catalog", get(read::get_catalog))
        .route("/v1/packages/{*name}", get(read::get_package))
        .route("/v1/checkouts", get(read::list_checkouts))
        .route("/v1/checkouts/{checkout_id}", get(read::get_checkout))
        .route("/v1/receipts/{receipt_id}", get(read::get_receipt))
        .route("/v1/submissions", get(read::list_submissions))
        .route("/v1/submissions/{submission_id}", get(read::get_submission))
        .route(
            "/v1/submissions/{submission_id}/build",
            get(read::get_tool_build),
        )
        .route("/v1/releases", get(read::list_releases))
        .route("/v1/releases/{release_id}", get(read::get_release))
        .route("/v1/consumers", get(read::list_consumers))
        .route(
            "/v1/consumers/by-package/{*name}",
            get(read::package_consumers),
        )
        .route("/v1/drift", get(read::drift))
        .route("/v1/rollouts", get(read::list_rollouts))
        .route("/v1/rollouts/{rollout_id}", get(read::get_rollout))
        .route(
            "/v1/checkouts/{checkout_id}/submission",
            get(read::get_checkout_submission),
        )
        .merge(crate::tools::read_routes())
        .merge(crate::tool_activation::read_routes())
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::read));
    let writes = Router::new()
        .route("/v1/packages", post(controller::register_package))
        .route("/v1/consumers", post(controller::register_consumer))
        .route("/v1/rollouts", post(controller::create_rollout))
        .merge(crate::tools::controller_routes())
        .merge(crate::tool_activation::controller_routes())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::controller,
        ));
    let agents = Router::new()
        .route("/v1/checkouts", post(agent::create_checkout))
        .route(
            "/v1/checkouts/{checkout_id}/lease/renew",
            post(agent::renew_lease),
        )
        .route(
            "/v1/checkouts/{checkout_id}/lease/release",
            post(agent::release_lease),
        )
        .route(
            "/v1/checkouts/{checkout_id}/transition",
            post(agent::transition),
        )
        .route(
            "/v1/checkouts/{checkout_id}/cleanup",
            post(agent::cleanup_checkout),
        )
        .route(
            "/v1/submissions/{submission_id}/validate",
            post(agent::validate_submission),
        )
        .route(
            "/v1/submissions/{submission_id}/integrate",
            post(agent::prepare_integration),
        )
        .route(
            "/v1/submissions/{submission_id}/integration/complete",
            post(agent::complete_integration),
        )
        .merge(crate::tools::agent_routes())
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::agent));
    let reviews = Router::new()
        .route("/v1/jobs/review/next", get(jobs::next_review))
        .route(
            "/v1/submissions/{submission_id}/review-bundle",
            get(bundles::download_review_bundle),
        )
        .route(
            "/v1/submissions/{submission_id}/review",
            post(jobs::record_review),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::reviewer,
        ));
    let releases = Router::new()
        .route("/v1/jobs/release/next", get(jobs::next_release))
        .route(
            "/v1/submissions/{submission_id}/release",
            post(jobs::begin_release),
        )
        .route(
            "/v1/submissions/{submission_id}/release/rework",
            post(jobs::request_release_rework),
        )
        .route(
            "/v1/releases/{release_id}/publications",
            post(jobs::record_publication),
        )
        .route(
            "/v1/releases/{release_id}/complete",
            post(jobs::complete_release),
        )
        .route(
            "/v1/releases/{release_id}/cleanup",
            post(jobs::cleanup_release),
        )
        .route(
            "/v1/submissions/{submission_id}/release-bundle",
            get(bundles::download_release_bundle),
        )
        .merge(crate::tools::release_routes())
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::release));
    let builds = Router::new()
        .route("/v1/jobs/build/next", get(jobs::next_tool_build))
        .route(
            "/v1/submissions/{submission_id}/build",
            post(jobs::complete_tool_build),
        )
        .route(
            "/v1/submissions/{submission_id}/build-bundle",
            get(bundles::download_release_bundle),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::build));
    let rollouts = Router::new()
        .route(
            "/v1/jobs/rollout/reconcile",
            post(jobs::reconcile_rollout_queue),
        )
        .route(
            "/v1/rollouts/{rollout_id}/bundle",
            get(bundles::download_rollout),
        )
        .route(
            "/v1/rollouts/{rollout_id}/submission",
            post(bundles::upload_rollout),
        )
        .route(
            "/v1/rollouts/{rollout_id}/complete",
            post(jobs::complete_rollout),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::rollout));
    Router::new()
        .route("/health", get(health))
        .route(
            "/v1/checkouts/{checkout_id}/archive",
            get(bundles::download_archive),
        )
        .route(
            "/v1/checkouts/{checkout_id}/submission",
            post(bundles::upload_submission),
        )
        .route(
            "/v1/submissions/{submission_id}/integration",
            get(bundles::download_integration),
        )
        .merge(reads)
        .merge(writes)
        .merge(agents)
        .merge(reviews)
        .merge(builds)
        .merge(releases)
        .merge(rollouts)
        .with_state(state)
}

pub async fn run(
    host: String,
    port: u16,
    data: PathBuf,
    credentials: WorkCredentials,
) -> WorkResult<()> {
    credentials.validate()?;
    let listener = TcpListener::bind((host.as_str(), port)).await?;
    let store = Arc::new(Store::open(data).await?);
    tracing::info!(host, port, "package-work service listening");
    axum::serve(listener, router(store, credentials)).await?;
    Ok(())
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

#[cfg(test)]
mod tests;
