use super::*;
use crate::{AppState, UpstreamClient};
use axum::http::StatusCode;
use axum_test::TestServer;
use base64::engine::general_purpose;
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

#[test]
fn npm_shasum_uses_sha1_hex() {
    assert_eq!(
        sha1_hex(b"hello world"),
        "2aae6c35c94fcfb415dbe95f408b9ce91ee846ed"
    );
}

fn create_npm_test_state() -> (Arc<AppState>, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir for test");
    let data_dir = temp_dir.path().to_path_buf();

    // Create required directories
    std::fs::create_dir_all(data_dir.join("npm/tarballs"))
        .expect("Failed to create npm tarballs dir");
    std::fs::create_dir_all(data_dir.join("npm/metadata"))
        .expect("Failed to create npm metadata dir");

    let config = Arc::new(crate::config::Config::default());
    let state = Arc::new(AppState {
        data_dir,
        server_addr: "http://localhost:8080".to_string(),
        upstream_client: Arc::new(UpstreamClient::disabled()),
        internal_client: None,
        config,
        resolver: Arc::new(crate::resolver::ResolverService::standalone()),
    });

    (state, temp_dir)
}

fn create_npm_publish_payload(package_name: &str, version: &str, tarball_content: &[u8]) -> Value {
    let encoded_tarball = general_purpose::STANDARD.encode(tarball_content);
    let filename = format!("{}-{}.tgz", package_name, version);

    json!({
        "_id": package_name,
        "name": package_name,
        "description": "Test package",
        "dist-tags": {
            "latest": version
        },
        "versions": {
            version: {
                "name": package_name,
                "version": version,
                "description": "Test package",
                "dist": {
                    "tarball": format!("http://localhost:8080/npm/{}/-/{}", package_name, filename)
                }
            }
        },
        "_attachments": {
            filename: {
                "content_type": "application/octet-stream",
                "data": encoded_tarball,
                "length": tarball_content.len()
            }
        }
    })
}

#[test]
fn metadata_paths_encode_scopes_and_reject_traversal() {
    let root = std::path::Path::new("/registry");
    assert_eq!(
        metadata_path(root, "@scope/package").unwrap(),
        root.join("npm/metadata/@scope%2Fpackage.json")
    );
    assert_eq!(
        package_from_metadata_file_name("@scope%2Fpackage.json").as_deref(),
        Some("@scope/package")
    );
    for package in ["..", "../outside", "@scope/../outside", "/tmp/outside"] {
        assert!(metadata_path(root, package).is_err());
    }
    assert!(metadata_path(root, "Express").is_err());
}

#[tokio::test]
async fn test_publish_package_with_tarball() {
    let (state, _temp_dir) = create_npm_test_state();
    let app = axum::Router::new()
        .route("/npm/{package}", axum::routing::put(publish_package))
        .with_state(state.clone());

    let server = TestServer::new(app);

    let package_name = "test-package";
    let version = "1.0.0";
    let tarball_content = b"fake tarball content";
    let payload = create_npm_publish_payload(package_name, version, tarball_content);

    let response = server
        .put(&format!("/npm/{}", package_name))
        .json(&payload)
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);

    // Verify tarball was saved
    let tarball_path = state
        .data_dir
        .join("npm/tarballs")
        .join(format!("{}-{}.tgz", package_name, version));
    assert!(tarball_path.exists());
    let saved_content = std::fs::read(tarball_path).expect("should read saved tarball");
    assert_eq!(saved_content, tarball_content);

    // Verify metadata was saved
    let metadata_path = state
        .data_dir
        .join("npm/metadata")
        .join(format!("{}.json", package_name));
    assert!(metadata_path.exists());
    let metadata_content =
        std::fs::read_to_string(metadata_path).expect("should read saved metadata");
    let metadata: Value = serde_json::from_str(&metadata_content).expect("should parse metadata");

    // Verify _attachments was removed from saved metadata
    assert!(metadata.get("_attachments").is_none());

    // Verify shasum was calculated and added
    assert!(metadata["versions"][version]["dist"]["shasum"].is_string());
}

#[tokio::test]
async fn test_publish_package_rejects_no_attachments() {
    let (state, _temp_dir) = create_npm_test_state();
    let app = axum::Router::new()
        .route("/npm/{package}", axum::routing::put(publish_package))
        .with_state(state);

    let server = TestServer::new(app);

    let payload = json!({
        "name": "test-package",
        "version": "1.0.0"
    });

    let response = server.put("/npm/test-package").json(&payload).await;

    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_publish_package_rejects_path_attachment_name() {
    let (state, _temp_dir) = create_npm_test_state();
    let app = axum::Router::new()
        .route("/npm/{package}", axum::routing::put(publish_package))
        .with_state(state);

    let server = TestServer::new(app);

    let tarball_content = b"fake tarball content";
    let encoded_tarball = general_purpose::STANDARD.encode(tarball_content);
    let payload = json!({
        "name": "test-package",
        "versions": {
            "1.0.0": {
                "name": "test-package",
                "version": "1.0.0",
                "dist": {
                    "tarball": "http://localhost:8080/npm/test-package/-/test-package-1.0.0.tgz"
                }
            }
        },
        "_attachments": {
            "../test-package-1.0.0.tgz": {
                "content_type": "application/octet-stream",
                "data": encoded_tarball,
                "length": tarball_content.len()
            }
        }
    });

    let response = server.put("/npm/test-package").json(&payload).await;

    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_package_metadata_after_publish() {
    let (state, _temp_dir) = create_npm_test_state();

    // Create test metadata file
    let package_name = "test-package";
    let metadata = json!({
        "name": package_name,
        "dist-tags": { "latest": "1.0.0" },
        "versions": {
            "1.0.0": {
                "name": package_name,
                "version": "1.0.0",
                "dist": {
                    "tarball": "http://localhost:8080/npm/test-package/-/test-package-1.0.0.tgz",
                    "shasum": "abc123"
                }
            }
        }
    });

    let metadata_path = state
        .data_dir
        .join("npm/metadata")
        .join(format!("{}.json", package_name));
    std::fs::write(
        metadata_path,
        serde_json::to_string_pretty(&metadata).expect("should serialize metadata"),
    )
    .expect("should write metadata file");

    let app = axum::Router::new()
        .route("/npm/{package}", axum::routing::get(package_metadata))
        .with_state(state);

    let server = TestServer::new(app);
    let response = server.get(&format!("/npm/{}", package_name)).await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["name"], package_name);
    assert_eq!(body["dist-tags"]["latest"], "1.0.0");
    assert!(body["versions"]["1.0.0"]["dist"]["tarball"]
        .as_str()
        .expect("tarball URL should be a string")
        .contains("test-package-1.0.0.tgz"));
}

#[tokio::test]
async fn test_package_metadata_updates_host_header() {
    let (state, _temp_dir) = create_npm_test_state();

    // Create test metadata file with localhost URL
    let package_name = "test-package";
    let metadata = json!({
        "name": package_name,
        "versions": {
            "1.0.0": {
                "dist": {
                    "tarball": "http://localhost:8080/npm/test-package/-/test-package-1.0.0.tgz"
                }
            }
        }
    });

    let metadata_path = state
        .data_dir
        .join("npm/metadata")
        .join(format!("{}.json", package_name));
    std::fs::write(
        metadata_path,
        serde_json::to_string_pretty(&metadata).expect("should serialize metadata"),
    )
    .expect("should write metadata file");

    let app = axum::Router::new()
        .route("/npm/{package}", axum::routing::get(package_metadata))
        .with_state(state);

    let server = TestServer::new(app);
    let response = server
        .get(&format!("/npm/{}", package_name))
        .add_header("host", "example.com:3000")
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let body: Value = response.json();

    // Reverse-proxied package URLs use the validated public request authority.
    let tarball_url = body["versions"]["1.0.0"]["dist"]["tarball"]
        .as_str()
        .expect("tarball URL should be a string");
    assert!(tarball_url.contains("example.com:3000"));
}

#[tokio::test]
async fn test_download_tarball() {
    let (state, _temp_dir) = create_npm_test_state();

    // Create test tarball file
    let content = b"test tarball content";
    let filename = "test-package-1.0.0.tgz";
    let tarball_path = state.data_dir.join("npm/tarballs").join(filename);
    std::fs::write(&tarball_path, content).expect("should write test tarball");

    let app = axum::Router::new()
        .route(
            "/npm/{package}/-/{filename}",
            axum::routing::get(download_tarball),
        )
        .with_state(state);

    let server = TestServer::new(app);
    let response = server
        .get("/npm/test-package/-/test-package-1.0.0.tgz")
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);
    assert_eq!(response.as_bytes().to_vec(), content.to_vec());
}

#[tokio::test]
async fn local_tarball_precedes_registered_package_resolution() {
    let (state, _temp_dir) = create_npm_test_state();
    let content = b"private tarball";
    let filename = "private-package-1.0.0.tgz";
    std::fs::write(state.data_dir.join("npm/tarballs").join(filename), content).unwrap();
    let catalog_path = state.data_dir.join("packages.json");
    let catalog = vm_packages::InternalPackageCatalog::new([vm_packages::PackageIdentity::new(
        vm_packages::PackageEcosystem::Npm,
        "private-package",
    )
    .unwrap()]);
    std::fs::write(&catalog_path, serde_json::to_vec(&catalog).unwrap()).unwrap();
    let mut state = (*state).clone();
    state.resolver = Arc::new(crate::resolver::ResolverService::new(Some(catalog_path)));

    let app = axum::Router::new()
        .route(
            "/npm/{package}/-/{filename}",
            axum::routing::get(download_tarball),
        )
        .with_state(Arc::new(state));
    let response = TestServer::new(app)
        .get("/npm/private-package/-/private-package-1.0.0.tgz")
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);
    assert_eq!(response.as_bytes().as_ref(), content);
}

#[tokio::test]
async fn test_download_scoped_tarball_with_decoded_separator() {
    let (state, _temp_dir) = create_npm_test_state();
    let content = b"scoped tarball content";
    let filename = "fs-minipass-4.0.1.tgz";
    let tarball_path = state.data_dir.join("npm/tarballs").join(filename);
    std::fs::write(&tarball_path, content).expect("should write test tarball");

    let app = axum::Router::new()
        .route(
            "/npm/{scope}/{package}/-/{filename}",
            axum::routing::get(download_scoped_tarball),
        )
        .with_state(state);

    let response = TestServer::new(app)
        .get("/npm/@isaacs/fs-minipass/-/fs-minipass-4.0.1.tgz")
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);
    assert_eq!(response.as_bytes().to_vec(), content.to_vec());
}
