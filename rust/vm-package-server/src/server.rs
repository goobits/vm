//! HTTP server setup and route handlers for package registry operations
//!
//! This module contains the complete Axum-based HTTP server implementation
//! supporting npm, PyPI, and Cargo package registry operations.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Result;
use tokio::net::TcpListener;
use tracing::{error, info};

mod routes;
mod setup;

use routes::app_router;
#[cfg(test)]
use setup::configure_security;

pub async fn run_server_background(host: String, port: u16, data_dir: PathBuf) -> Result<()> {
    run_server_with_shutdown(host, port, data_dir, None).await
}

pub async fn run_server_with_shutdown(
    host: String,
    port: u16,
    data_dir: PathBuf,
    shutdown_receiver: Option<tokio::sync::oneshot::Receiver<()>>,
) -> Result<()> {
    run_server_internal(host, port, data_dir, shutdown_receiver).await
}

pub async fn run_server(host: String, port: u16, data_dir: PathBuf) -> Result<()> {
    run_server_internal(host, port, data_dir, None).await
}

async fn run_server_internal(
    host: String,
    port: u16,
    data_dir: PathBuf,
    shutdown_receiver: Option<tokio::sync::oneshot::Receiver<()>>,
) -> Result<()> {
    info!("🚀 Starting Goobits Package Server...");

    let state = setup::app_state(&host, port, &data_dir)?;
    info!("📂 Using data directory: {}", state.data_dir.display());
    info!("Starting server on {host}:{port}");
    let app = app_router(state);

    let addr: SocketAddr = format!("{host}:{port}").parse().map_err(|e| {
        error!(host = %host, port = %port, error = %e, "Invalid socket address");
        anyhow::anyhow!("Invalid socket address {host}:{port}: {e}")
    })?;

    let listener = TcpListener::bind(&addr).await.map_err(|e| {
        error!(addr = %addr, error = %e, "Failed to bind to address");
        anyhow::anyhow!("Failed to bind to {host}:{port}: {e}")
    })?;

    info!("✅ Server is running on http://{}:{}", host, port);
    info!("Server listening on {}", addr);

    match shutdown_receiver {
        Some(shutdown_rx) => {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    shutdown_rx.await.ok();
                    info!("Received shutdown signal, stopping server gracefully");
                })
                .await
                .map_err(|e| anyhow::anyhow!("Server error: {e}"))?;
        }
        None => {
            axum::serve(listener, app)
                .await
                .map_err(|e| anyhow::anyhow!("Server error: {e}"))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::{
        resolver::ResolverService, state::AppState, upstream::UpstreamClient,
        InternalRegistryClient,
    };
    use axum::{
        http::{header, StatusCode},
        routing::get,
        Json, Router,
    };
    use axum_test::TestServer;
    use vm_packages::{InternalPackageCatalog, PackageEcosystem, PackageIdentity};

    async fn spawn_http(app: Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{address}"), task)
    }

    fn edge_state(data_dir: &std::path::Path, gateway: &str) -> AppState {
        let internal = Arc::new(InternalRegistryClient::new(gateway, "read").unwrap());
        AppState {
            data_dir: data_dir.to_path_buf(),
            server_addr: "http://127.0.0.1:3080".into(),
            upstream_client: Arc::new(UpstreamClient::disabled()),
            internal_client: Some(Arc::clone(&internal)),
            config: Arc::new(configure_security("0.0.0.0", Some("read"), None, true).unwrap()),
            resolver: Arc::new(ResolverService::worker_edge(data_dir, internal)),
        }
    }

    #[test]
    fn loopback_bind_does_not_require_authentication() {
        let config = configure_security("127.0.0.1", None, None, false).unwrap();
        assert!(!config.security.require_authentication);
    }

    #[test]
    fn remote_bind_requires_and_enables_authentication() {
        assert!(configure_security("0.0.0.0", None, None, false).is_err());
        assert!(configure_security("0.0.0.0", Some("read"), None, false).is_err());

        let config = configure_security("0.0.0.0", Some("read"), Some("publish"), false).unwrap();
        assert!(config.security.require_authentication);
        assert_eq!(config.security.read_keys, ["read"]);
        assert_eq!(config.security.publish_keys, ["publish"]);
    }

    #[test]
    fn remote_read_only_edge_can_serve_without_exposing_a_credential() {
        let config = configure_security("0.0.0.0", None, None, true).unwrap();
        assert!(!config.security.require_authentication);
        assert!(config.security.read_keys.is_empty());
        assert!(config.security.publish_keys.is_empty());
    }

    #[tokio::test]
    async fn router_separates_health_read_and_publish_access() {
        let directory = tempfile::tempdir().unwrap();
        let config =
            Arc::new(configure_security("0.0.0.0", Some("read"), Some("publish"), false).unwrap());
        let state = AppState {
            data_dir: directory.path().to_path_buf(),
            server_addr: "http://127.0.0.1:3080".into(),
            upstream_client: Arc::new(UpstreamClient::disabled()),
            internal_client: None,
            config,
            resolver: Arc::new(ResolverService::standalone()),
        };
        let server = TestServer::new(app_router(state));

        assert_eq!(server.get("/health").await.status_code(), StatusCode::OK);
        assert_eq!(
            server.get("/api/status").await.status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            server.get("/vm-client").await.status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            server
                .get("/vm-client")
                .add_header(header::AUTHORIZATION, "Bearer read")
                .await
                .status_code(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            server
                .get("/api/status")
                .add_header(header::AUTHORIZATION, "Bearer read")
                .await
                .status_code(),
            StatusCode::OK
        );
        assert_eq!(
            server
                .put("/npm/example")
                .add_header(header::AUTHORIZATION, "Bearer read")
                .json(&serde_json::json!({"name": "example"}))
                .await
                .status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            server
                .put("/npm/example")
                .add_header(header::AUTHORIZATION, "Bearer publish")
                .json(&serde_json::json!({"name": "example"}))
                .await
                .status_code(),
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn worker_edge_exposes_no_publish_routes() {
        let directory = tempfile::tempdir().unwrap();
        let config = Arc::new(configure_security("0.0.0.0", None, None, true).unwrap());
        let state = AppState {
            data_dir: directory.path().to_path_buf(),
            server_addr: "http://127.0.0.1:3080".into(),
            upstream_client: Arc::new(UpstreamClient::disabled()),
            internal_client: Some(Arc::new(
                InternalRegistryClient::new("http://127.0.0.1:9", "read").unwrap(),
            )),
            config,
            resolver: Arc::new(ResolverService::standalone()),
        };
        let server = TestServer::new(app_router(state));

        assert_eq!(
            server
                .put("/npm/example")
                .add_header(header::AUTHORIZATION, "Bearer read")
                .json(&serde_json::json!({"name": "example"}))
                .await
                .status_code(),
            StatusCode::METHOD_NOT_ALLOWED
        );
        assert_eq!(
            server
                .put("/tools/artifacts/tool/1.0.0/any/deadbeef.tar.gz")
                .await
                .status_code(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn worker_edge_caches_internal_npm_across_infra_restart() {
        let directory = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let gateway = format!("http://{address}");
        let package = PackageIdentity::new(PackageEcosystem::Npm, "@goobits/auth").unwrap();
        let catalog = InternalPackageCatalog::new([package]);
        let tarball_url = format!("{gateway}/npm/%40goobits%2Fauth/-/auth-1.0.0.tgz");
        let infra = Router::new()
            .route(
                "/work/v1/catalog",
                get(move || {
                    let catalog = catalog.clone();
                    async move { Json(catalog) }
                }),
            )
            .route(
                "/npm/{package}",
                get(move || {
                    let tarball_url = tarball_url.clone();
                    async move {
                        Json(serde_json::json!({
                            "name": "@goobits/auth",
                            "versions": {
                                "1.0.0": {
                                    "name": "@goobits/auth",
                                    "version": "1.0.0",
                                    "dist": { "tarball": tarball_url }
                                }
                            }
                        }))
                    }
                }),
            )
            .route(
                "/npm/{package}/-/{filename}",
                get(|| async { b"internal npm archive".to_vec() }),
            );
        let infra_task = tokio::spawn(async move {
            axum::serve(listener, infra).await.unwrap();
        });

        let (edge_gateway, edge_task) =
            spawn_http(app_router(edge_state(directory.path(), &gateway))).await;
        let http = reqwest::Client::new();
        let metadata_url = format!("{edge_gateway}/npm/%40goobits%2Fauth");
        let metadata: serde_json::Value = http
            .get(&metadata_url)
            .bearer_auth("read")
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        let edge_tarball = metadata["versions"]["1.0.0"]["dist"]["tarball"]
            .as_str()
            .unwrap();
        assert!(edge_tarball.starts_with(&edge_gateway));
        let archive = http
            .get(edge_tarball)
            .bearer_auth("read")
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .bytes()
            .await
            .unwrap();
        assert_eq!(&archive[..], b"internal npm archive");

        infra_task.abort();
        edge_task.abort();
        let _ = infra_task.await;
        let _ = edge_task.await;
        let (restarted_gateway, restarted_edge) =
            spawn_http(app_router(edge_state(directory.path(), &gateway))).await;
        let metadata = http
            .get(format!("{restarted_gateway}/npm/%40goobits%2Fauth"))
            .bearer_auth("read")
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        let tarball = metadata["versions"]["1.0.0"]["dist"]["tarball"]
            .as_str()
            .unwrap();
        assert!(tarball.starts_with(&restarted_gateway));
        let archive = http
            .get(tarball)
            .bearer_auth("read")
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .bytes()
            .await
            .unwrap();
        assert_eq!(&archive[..], b"internal npm archive");
        restarted_edge.abort();
    }
}
