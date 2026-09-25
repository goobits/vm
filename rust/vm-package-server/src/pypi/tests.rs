use super::*;
use crate::{AppState, UpstreamClient};
use axum::http::StatusCode;
use axum_test::{
    multipart::{MultipartForm, Part},
    TestServer,
};
use std::sync::Arc;
use tempfile::TempDir;

fn create_pypi_test_state() -> (Arc<AppState>, TempDir) {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let data_dir = temp_dir.path().to_path_buf();

    // Create required directories
    std::fs::create_dir_all(data_dir.join("pypi/packages"))
        .expect("should create pypi packages dir");
    std::fs::create_dir_all(data_dir.join("pypi/simple")).expect("should create pypi simple dir");

    let config = Arc::new(crate::config::Config::default());
    let state = Arc::new(AppState {
        data_dir,
        server_addr: "http://127.0.0.1:3080".to_string(),
        upstream_client: Arc::new(UpstreamClient::disabled()),
        internal_client: None,
        config,
        resolver: Arc::new(crate::resolver::ResolverService::standalone()),
    });

    (state, temp_dir)
}

#[tokio::test]
async fn test_upload_wheel_file() {
    let (state, _temp_dir) = create_pypi_test_state();
    let app = axum::Router::new()
        .route("/pypi/", axum::routing::post(upload_package))
        .with_state(state.clone());

    let server = TestServer::new(app);

    let filename = "test_package-1.0.0-py3-none-any.whl";
    let content = b"fake wheel content";

    let part = Part::bytes(content.to_vec())
        .file_name(filename)
        .mime_type("application/octet-stream");
    let form = MultipartForm::new().add_part("content", part);

    let response = server.post("/pypi/").multipart(form).await;

    assert_eq!(response.status_code(), StatusCode::OK);

    // Verify file was saved
    let saved_path = state.data_dir.join("pypi/packages").join(filename);
    assert!(saved_path.exists());
    let saved_content = std::fs::read(saved_path).expect("should read saved wheel file");
    assert_eq!(saved_content, content);
}

#[tokio::test]
async fn test_upload_tar_gz_file() {
    let (state, _temp_dir) = create_pypi_test_state();
    let app = axum::Router::new()
        .route("/pypi/", axum::routing::post(upload_package))
        .with_state(state.clone());

    let server = TestServer::new(app);

    let filename = "test-package-1.0.0.tar.gz";
    let content = b"fake tar.gz content";

    let part = Part::bytes(content.to_vec())
        .file_name(filename)
        .mime_type("application/octet-stream");
    let form = MultipartForm::new().add_part("content", part);

    let response = server.post("/pypi/").multipart(form).await;

    assert_eq!(response.status_code(), StatusCode::OK);

    // Verify file was saved
    let saved_path = state.data_dir.join("pypi/packages").join(filename);
    assert!(saved_path.exists());
}

#[tokio::test]
async fn test_reject_invalid_file_extension() {
    let (state, _temp_dir) = create_pypi_test_state();
    let app = axum::Router::new()
        .route("/pypi/", axum::routing::post(upload_package))
        .with_state(state);

    let server = TestServer::new(app);

    let filename = "test-package.txt";
    let content = b"invalid file content";

    let part = Part::bytes(content.to_vec())
        .file_name(filename)
        .mime_type("application/octet-stream");
    let form = MultipartForm::new().add_part("content", part);

    let response = server.post("/pypi/").multipart(form).await;

    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_simple_index_shows_uploaded_package() {
    let (state, _temp_dir) = create_pypi_test_state();

    // Create a test package file
    let content = b"fake content";
    let package_file = state.data_dir.join("pypi/packages/testpackage-1.0.0.whl");
    std::fs::write(&package_file, content).expect("should write test package file");

    // Create corresponding .meta file with hash
    let hash = sha256_hex(content);
    let meta_file = package_file.with_extension("whl.meta");
    std::fs::write(&meta_file, hash).expect("should write meta file");

    let app = axum::Router::new()
        .route("/pypi/simple/", axum::routing::get(simple_index))
        .with_state(state);

    let server = TestServer::new(app);
    let response = server.get("/pypi/simple/").await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let body = response.text();
    assert!(body.contains("testpackage"));
}

#[tokio::test]
async fn test_package_index_shows_file_with_hash() {
    let (state, _temp_dir) = create_pypi_test_state();

    // Create a test package file
    let content = b"fake wheel content";
    let package_file = state.data_dir.join("pypi/packages/testpackage-1.0.0.whl");
    std::fs::write(&package_file, content).expect("should write test package file");

    // Create corresponding .meta file with hash
    let hash = sha256_hex(content);
    let meta_file = package_file.with_extension("whl.meta");
    std::fs::write(&meta_file, hash).expect("should write meta file");

    let app = axum::Router::new()
        .route("/pypi/simple/{package}/", axum::routing::get(package_index))
        .with_state(state);

    let server = TestServer::new(app);
    let response = server.get("/pypi/simple/testpackage/").await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let body = response.text();
    assert!(body.contains("testpackage-1.0.0.whl"));
    assert!(body.contains("sha256="));
}

#[tokio::test]
async fn test_download_file() {
    let (state, _temp_dir) = create_pypi_test_state();

    // Create a test package file
    let content = b"test package content";
    let filename = "test-package-1.0.0.whl";
    let package_file = state.data_dir.join("pypi/packages").join(filename);
    std::fs::write(&package_file, content).expect("should write test package file");

    let app = axum::Router::new()
        .route(
            "/pypi/packages/{filename}",
            axum::routing::get(download_file),
        )
        .with_state(state);

    let server = TestServer::new(app);
    let response = server.get(&format!("/pypi/packages/{}", filename)).await;

    assert_eq!(response.status_code(), StatusCode::OK);
    assert_eq!(response.as_bytes().to_vec(), content.to_vec());
}

#[test]
fn upstream_package_links_stay_behind_the_gateway() {
    let html = r#"<a href="https://files.pythonhosted.org/packages/ab/cd/hash/pkg.whl#sha256=123">pkg</a>"#;
    let rewritten = rewrite_upstream_links(html.into(), "https://packages.internal");

    assert_eq!(
        rewritten,
        r#"<a href="https://packages.internal/pypi/upstream/ab/cd/hash/pkg.whl#sha256=123">pkg</a>"#
    );
}
