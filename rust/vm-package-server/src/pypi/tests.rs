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

#[test]
fn artifact_html_escapes_markup_and_encodes_url_segments() {
    let link = html::artifact_link(
        "pkg-1.0.0\"><img src=x onerror=alert(1)>&'.whl",
        "\"><script>alert(2)</script>&",
    );
    assert!(!link.contains("<img"));
    assert!(!link.contains("<script"));
    assert!(link.contains("&quot;&gt;&lt;img src=x onerror=alert(1)&gt;&amp;&#39;"));
    assert!(link.contains("pkg-1.0.0%22%3E%3Cimg%20src%3Dx%20onerror%3Dalert%281%29%3E%26%27.whl"));
    assert!(link.contains("#sha256=%22%3E%3Cscript%3Ealert%282%29%3C%2Fscript%3E%26"));
    assert_eq!(link.matches("href=\"").count(), 1);
}

#[tokio::test]
async fn uploaded_special_filename_is_inert_and_download_link_roundtrips() {
    let (state, _directory) = create_pypi_test_state();
    let server = TestServer::new(
        axum::Router::new()
            .route("/pypi/", axum::routing::post(upload_package))
            .route("/pypi/simple/", axum::routing::get(simple_index))
            .route("/pypi/simple/{package}/", axum::routing::get(package_index))
            .route(
                "/pypi/packages/{filename}",
                axum::routing::get(download_file),
            )
            .with_state(state),
    );
    // All characters are legal on Windows too. Literal angle brackets/quotes are
    // tested by the renderer above without relying on filesystem support.
    let filename = "testpackage-1.0.0' onmouseover='alert(1)&lt;img&gt;#% café.whl";
    let content = b"artifact bytes";
    let part = Part::bytes(content.to_vec()).file_name(filename);
    server
        .post("/pypi/")
        .multipart(MultipartForm::new().add_part("content", part))
        .await
        .assert_status_ok();

    let index = server.get("/pypi/simple/TestPackage/").await;
    index.assert_status_ok();
    assert_eq!(
        index.headers()["content-security-policy"],
        "default-src 'none'; base-uri 'none'; form-action 'none'; sandbox"
    );
    let body = index.text();
    assert!(body.contains("<h1>Links for testpackage</h1>"));
    assert!(body.contains("&#39; onmouseover=&#39;alert(1)&amp;lt;img&amp;gt;#% café.whl"));
    assert!(!body.contains("<img"));
    let href = body
        .split("href=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    assert!(href.ends_with(&format!("#sha256={}", sha256_hex(content))));
    assert!(href.contains("%23%25%20caf%C3%A9.whl"));
    let index_url = url::Url::parse("http://registry.test/pypi/simple/testpackage/").unwrap();
    let artifact_url = index_url.join(href).unwrap();
    let download = server.get(artifact_url.path()).await;
    download.assert_status_ok();
    assert_eq!(download.as_bytes().as_ref(), content);

    let root_index = server.get("/pypi/simple/").await;
    assert_eq!(
        root_index.headers()["content-security-policy"],
        index.headers()["content-security-policy"]
    );
}

#[test]
fn remote_index_response_has_no_script_or_same_origin_authority() {
    let response = html::response("<script>alert(1)</script>".into());
    assert_eq!(
        response.headers()["content-security-policy"],
        "default-src 'none'; base-uri 'none'; form-action 'none'; sandbox"
    );
    assert_eq!(
        response.headers()["content-type"],
        "text/html; charset=utf-8"
    );
}
