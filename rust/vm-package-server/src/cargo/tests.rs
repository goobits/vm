//! Tests for Cargo registry functionality

#[cfg(test)]
mod cargo_tests {
    use crate::cargo::{handlers::*, index::*};
    use crate::{AppState, UpstreamClient, UpstreamConfig};
    use axum::http::StatusCode;
    use axum_test::TestServer;
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_cargo_test_state() -> (Arc<AppState>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().to_path_buf();

        // Create required directories
        std::fs::create_dir_all(data_dir.join("cargo/crates")).unwrap();
        std::fs::create_dir_all(data_dir.join("cargo/api/v1/crates")).unwrap();
        std::fs::create_dir_all(data_dir.join("cargo/index")).unwrap();

        let upstream_config = UpstreamConfig::default();
        let config = Arc::new(crate::config::Config::default());
        let state = Arc::new(AppState {
            data_dir,
            server_addr: "http://localhost:8080".to_string(),
            upstream_client: Arc::new(UpstreamClient::new(upstream_config).unwrap()),
            config,
        });

        (state, temp_dir)
    }

    fn create_cargo_publish_payload(
        crate_name: &str,
        version: &str,
        crate_content: &[u8],
    ) -> Vec<u8> {
        let metadata = json!({
            "name": crate_name,
            "vers": version,
            "deps": [],
            "features": {},
            "authors": ["test@example.com"],
            "description": "Test crate",
            "license": "MIT"
        });

        let metadata_bytes = serde_json::to_vec(&metadata).unwrap();
        let metadata_len = metadata_bytes.len() as u32;
        let crate_len = crate_content.len() as u32;

        let mut payload = Vec::new();

        // Add metadata length (little-endian)
        payload.extend_from_slice(&metadata_len.to_le_bytes());

        // Add metadata
        payload.extend_from_slice(&metadata_bytes);

        // Add crate length (little-endian)
        payload.extend_from_slice(&crate_len.to_le_bytes());

        // Add crate content
        payload.extend_from_slice(crate_content);

        payload
    }

    #[tokio::test]
    async fn test_publish_crate_with_binary_payload() {
        let (state, _temp_dir) = create_cargo_test_state();
        let app = axum::Router::new()
            .route(
                "/cargo/api/v1/crates/new",
                axum::routing::put(publish_crate),
            )
            .with_state(state.clone());

        let server = TestServer::new(app).unwrap();

        let crate_name = "test-crate";
        let version = "1.0.0";
        let crate_content = b"fake crate content";
        let payload = create_cargo_publish_payload(crate_name, version, crate_content);

        let response = server
            .put("/cargo/api/v1/crates/new")
            .bytes(payload.into())
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);

        // Verify .crate file was saved
        let crate_path = state
            .data_dir
            .join("cargo/crates")
            .join(format!("{}-{}.crate", crate_name, version));
        assert!(crate_path.exists());
        let saved_content = std::fs::read(crate_path).unwrap();
        assert_eq!(saved_content, crate_content);

        // Verify index file was created
        let index_path_str = index_path(crate_name).unwrap();
        let index_file_path = state.data_dir.join("cargo/index").join(&index_path_str);
        assert!(index_file_path.exists());

        let index_content = std::fs::read_to_string(index_file_path).unwrap();
        assert!(index_content.contains(crate_name));
        assert!(index_content.contains(version));
        assert!(index_content.contains("\"cksum\":"));
    }

    #[tokio::test]
    async fn test_publish_crate_appends_to_existing_index() {
        let (state, _temp_dir) = create_cargo_test_state();

        let crate_name = "test-crate";
        let index_path_str = index_path(crate_name).unwrap();
        let index_file_path = state.data_dir.join("cargo/index").join(&index_path_str);

        // Create directory and initial index entry
        std::fs::create_dir_all(index_file_path.parent().unwrap()).unwrap();
        let existing_entry = json!({
            "name": crate_name,
            "vers": "0.9.0",
            "deps": [],
            "cksum": "abc123",
            "features": {},
            "yanked": false
        });
        std::fs::write(
            &index_file_path,
            format!("{}\n", serde_json::to_string(&existing_entry).unwrap()),
        )
        .unwrap();

        let app = axum::Router::new()
            .route(
                "/cargo/api/v1/crates/new",
                axum::routing::put(publish_crate),
            )
            .with_state(state.clone());

        let server = TestServer::new(app).unwrap();

        let version = "1.0.0";
        let crate_content = b"fake crate content";
        let payload = create_cargo_publish_payload(crate_name, version, crate_content);

        let response = server
            .put("/cargo/api/v1/crates/new")
            .bytes(payload.into())
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);

        // Verify index file contains both versions
        let index_content = std::fs::read_to_string(index_file_path).unwrap();
        let lines: Vec<&str> = index_content.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("0.9.0"));
        assert!(lines[1].contains("1.0.0"));
    }

    #[tokio::test]
    async fn test_publish_crate_rejects_malformed_payload() {
        let (state, _temp_dir) = create_cargo_test_state();
        let app = axum::Router::new()
            .route(
                "/cargo/api/v1/crates/new",
                axum::routing::put(publish_crate),
            )
            .with_state(state);

        let server = TestServer::new(app).unwrap();

        // Send malformed payload (too short)
        let payload = vec![1, 2, 3];

        let response = server
            .put("/cargo/api/v1/crates/new")
            .bytes(payload.into())
            .await;

        assert_eq!(response.status_code(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn test_cargo_config() {
        let (state, _temp_dir) = create_cargo_test_state();
        let app = axum::Router::new()
            .route("/cargo/config.json", axum::routing::get(config))
            .with_state(state);

        let server = TestServer::new(app).unwrap();
        let response = server
            .get("/cargo/config.json")
            .add_header("host", "example.com:3000")
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        let body: serde_json::Value = response.json();

        let dl_url = body["dl"].as_str().unwrap();
        assert!(dl_url.contains("localhost:8080")); // Uses static server_addr now
        assert!(dl_url.contains("{crate}"));
        assert!(dl_url.contains("{version}"));
    }

    #[tokio::test]
    async fn test_index_file_after_publish() {
        let (state, _temp_dir) = create_cargo_test_state();

        let crate_name = "test-crate";
        let index_entry = json!({
            "name": crate_name,
            "vers": "1.0.0",
            "deps": [],
            "cksum": "abc123def456",
            "features": {},
            "yanked": false
        });

        let index_path_str = index_path(crate_name).unwrap();
        let index_file_path = state.data_dir.join("cargo/index").join(&index_path_str);
        std::fs::create_dir_all(index_file_path.parent().unwrap()).unwrap();
        std::fs::write(
            &index_file_path,
            format!("{}\n", serde_json::to_string(&index_entry).unwrap()),
        )
        .unwrap();

        let app = axum::Router::new()
            .route("/cargo/index/{crate}", axum::routing::get(index_file))
            .with_state(state);

        let server = TestServer::new(app).unwrap();
        let response = server.get(&format!("/cargo/index/{}", crate_name)).await;

        assert_eq!(response.status_code(), StatusCode::OK);
        let body = response.text();
        assert!(body.contains(crate_name));
        assert!(body.contains("1.0.0"));
        assert!(body.contains("abc123def456"));
    }

    #[tokio::test]
    async fn test_download_crate() {
        let (state, _temp_dir) = create_cargo_test_state();

        // Create test crate file
        let content = b"test crate content";
        let crate_name = "test-crate";
        let version = "1.0.0";
        let filename = format!("{}-{}.crate", crate_name, version);
        let crate_path = state.data_dir.join("cargo/crates").join(&filename);
        std::fs::write(&crate_path, content).unwrap();

        let app = axum::Router::new()
            .route(
                "/cargo/api/v1/crates/{crate}/{version}/download",
                axum::routing::get(download_crate),
            )
            .with_state(state);

        let server = TestServer::new(app).unwrap();
        let response = server
            .get(&format!(
                "/cargo/api/v1/crates/{}/{}/download",
                crate_name, version
            ))
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        assert_eq!(response.as_bytes().to_vec(), content.to_vec());
    }
}
