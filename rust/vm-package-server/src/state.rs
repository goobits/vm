//! Shared HTTP handler state.

use crate::{config::Config, resolver::ResolverService, InternalRegistryClient, UpstreamClient};
use axum::http::{header, uri::Authority, HeaderMap};
use serde::Serialize;
use std::{path::PathBuf, sync::Arc};

/// Runtime resources shared by package protocol handlers.
#[derive(Clone)]
pub struct AppState {
    /// Base directory for package artifacts, indexes, and caches.
    pub data_dir: PathBuf,
    /// Fallback public origin used when a request has no valid `Host` header.
    pub server_addr: String,
    /// Shared client for approved upstream registries.
    pub upstream_client: Arc<UpstreamClient>,
    /// Optional authoritative registry used by a worker-local edge.
    pub internal_client: Option<Arc<InternalRegistryClient>>,
    /// Authentication and server policy.
    pub config: Arc<Config>,
    /// Shared internal/external source-selection policy.
    pub resolver: Arc<ResolverService>,
}

impl AppState {
    /// Resolve the externally visible request origin without trusting arbitrary text.
    pub fn public_base_url(&self, headers: &HeaderMap) -> String {
        let Some(authority) = headers
            .get(header::HOST)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<Authority>().ok())
        else {
            return self.server_addr.clone();
        };
        let scheme = headers
            .get("x-forwarded-proto")
            .and_then(|value| value.to_str().ok())
            .filter(|scheme| matches!(*scheme, "http" | "https"))
            .unwrap_or("http");
        format!("{scheme}://{authority}")
    }
}

/// Standard response for successful operations without a domain payload.
#[derive(Serialize)]
pub struct SuccessResponse {
    pub message: String,
}
