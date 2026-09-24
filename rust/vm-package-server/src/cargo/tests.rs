//! Tests for Cargo registry functionality

#[cfg(test)]
mod cargo_tests {
    use crate::cargo::{handlers::*, index::*};
    use crate::{AppState, UpstreamClient};
    use axum::http::StatusCode;
    use axum_test::TestServer;
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_cargo_test_state() -> (Arc<AppState>, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let data_dir = temp_dir.path().to_path_buf();

        // Create required directories
        std::fs::create_dir_all(data_dir.join("cargo/crates"))
            .expect("Failed to create crates dir");
        std::fs::create_dir_all(data_dir.join("cargo/api/v1/crates"))
            .expect("Failed to create api dir");
        std::fs::create_dir_all(data_dir.join("cargo/index")).expect("Failed to create index dir");

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

        let metadata_bytes = serde_json::to_vec(&metadata).expect("Failed to serialize metadata");
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
    async fn local_crate_precedes_registered_package_resolution() {
        let (state, _temp_dir) = create_cargo_test_state();
        let content = b"private crate";
        std::fs::write(
            state
                .data_dir
                .join("cargo/crates/private-crate-1.0.0.crate"),
            content,
        )
        .unwrap();
        let catalog_path = state.data_dir.join("packages.json");
        let catalog =
            vm_packages::InternalPackageCatalog::new([vm_packages::PackageIdentity::new(
                vm_packages::PackageEcosystem::Cargo,
                "private-crate",
            )
            .unwrap()]);
        std::fs::write(&catalog_path, serde_json::to_vec(&catalog).unwrap()).unwrap();
        let mut state = (*state).clone();
        state.resolver = Arc::new(crate::resolver::ResolverService::new(Some(catalog_path)));
        let app = axum::Router::new()
            .route(
                "/cargo/api/v1/crates/{crate_name}/{version}/download",
                axum::routing::get(download_crate),
            )
            .with_state(Arc::new(state));

        let response = TestServer::new(app)
            .get("/cargo/api/v1/crates/private-crate/1.0.0/download")
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        assert_eq!(response.as_bytes().as_ref(), content);
    }

    #[tokio::test]
    async fn cargo_publish_dependencies_are_translated_to_installable_index_metadata() {
        let (state, _temp_dir) = create_cargo_test_state();
        let app = axum::Router::new()
            .route(
                "/cargo/api/v1/crates/new",
                axum::routing::put(publish_crate),
            )
            .with_state(state.clone());
        let metadata = json!({
            "name": "shared-auth", "vers": "1.0.0",
            "deps": [{
                "name": "serde", "version_req": "^1.0", "explicit_name_in_toml": "serialization",
                "features": ["derive"], "optional": true, "default_features": false,
                "target": "cfg(unix)", "kind": "normal", "registry": "https://github.com/rust-lang/crates.io-index"
            }, {"name": "internal-types", "version_req": "=2.0.0"}],
            "features": {"serde": ["dep:serialization"]}, "links": "shared_native", "rust_version": "1.80"
        });
        let encoded = serde_json::to_vec(&metadata).unwrap();
        let artifact = b"fixture crate";
        let mut payload = Vec::new();
        payload.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
        payload.extend_from_slice(&encoded);
        payload.extend_from_slice(&(artifact.len() as u32).to_le_bytes());
        payload.extend_from_slice(artifact);
        let server = TestServer::new(app);
        for _ in 0..2 {
            let response = server
                .put("/cargo/api/v1/crates/new")
                .bytes(payload.clone().into())
                .await;
            assert_eq!(response.status_code(), StatusCode::OK);
        }
        let index =
            std::fs::read_to_string(state.data_dir.join("cargo/index/sh/ar/shared-auth")).unwrap();
        assert_eq!(index.lines().count(), 1);
        let entry: serde_json::Value = serde_json::from_str(index.trim()).unwrap();
        assert_eq!(entry["deps"][0]["name"], "serialization");
        assert_eq!(entry["deps"][0]["package"], "serde");
        assert_eq!(entry["deps"][0]["req"], "^1.0");
        assert!(entry["deps"][0].get("version_req").is_none());
        assert_eq!(entry["deps"][0]["optional"], true);
        assert_eq!(entry["deps"][0]["default_features"], false);
        assert_eq!(entry["deps"][0]["target"], "cfg(unix)");
        assert_eq!(
            entry["deps"][0]["registry"],
            metadata["deps"][0]["registry"]
        );
        assert_eq!(entry["deps"][1]["req"], "=2.0.0");
        assert!(entry["deps"][1]["registry"].is_null());
        assert_eq!(entry["features"], metadata["features"]);
        assert_eq!(entry["links"], "shared_native");
        assert_eq!(entry["rust_version"], "1.80");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires Cargo on PATH; resolves only an isolated local HTTP registry"]
    async fn cargo_installs_a_published_crate_with_a_renamed_private_dependency() {
        use std::io::Write;
        fn archive(name: &str, manifest: &str, code: &str) -> Vec<u8> {
            let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            let mut archive = tar::Builder::new(gzip);
            for (path, contents) in [("Cargo.toml", manifest), ("src/lib.rs", code)] {
                let mut header = tar::Header::new_gnu();
                header.set_size(contents.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                archive
                    .append_data(
                        &mut header,
                        format!("{name}-1.0.0/{path}"),
                        contents.as_bytes(),
                    )
                    .unwrap();
            }
            archive.into_inner().unwrap().finish().unwrap()
        }
        let (state, _storage) = create_cargo_test_state();
        let app = axum::Router::new()
            .route(
                "/cargo/api/v1/crates/new",
                axum::routing::put(publish_crate),
            )
            .route(
                "/cargo/api/v1/crates/{crate_name}/{version}/download",
                axum::routing::get(download_crate),
            )
            .route("/cargo/index/config.json", axum::routing::get(config))
            .route("/cargo/index/{*path}", axum::routing::get(index_file))
            .with_state(state);
        let server = TestServer::builder().http_transport().build(app);
        let leaf_manifest = "[package]\nname = \"leaf\"\nversion = \"1.0.0\"\nedition = \"2021\"\n";
        let shared_manifest = "[package]\nname = \"shared\"\nversion = \"1.0.0\"\nedition = \"2021\"\n[dependencies]\nrenamed = { package = \"leaf\", version = \"1\" }\n";
        for (name, manifest, code, dependencies) in [
            (
                "leaf",
                leaf_manifest,
                "pub fn value() -> u8 { 42 }",
                json!([]),
            ),
            (
                "shared",
                shared_manifest,
                "pub fn value() -> u8 { renamed::value() }",
                json!([{"name":"leaf", "version_req":"^1", "explicit_name_in_toml":"renamed", "features":[], "optional":false, "default_features":true}]),
            ),
        ] {
            let metadata = serde_json::to_vec(
                &json!({"name":name,"vers":"1.0.0","deps":dependencies,"features":{}}),
            )
            .unwrap();
            let artifact = archive(name, manifest, code);
            let mut payload = Vec::new();
            payload
                .write_all(&(metadata.len() as u32).to_le_bytes())
                .unwrap();
            payload.extend_from_slice(&metadata);
            payload.extend_from_slice(&(artifact.len() as u32).to_le_bytes());
            payload.extend_from_slice(&artifact);
            assert_eq!(
                server
                    .put("/cargo/api/v1/crates/new")
                    .bytes(payload.into())
                    .await
                    .status_code(),
                StatusCode::OK
            );
        }
        let consumer = TempDir::new().unwrap();
        std::fs::create_dir(consumer.path().join("src")).unwrap();
        std::fs::create_dir(consumer.path().join(".cargo")).unwrap();
        std::fs::write(consumer.path().join("Cargo.toml"), "[package]\nname = \"consumer\"\nversion = \"1.0.0\"\nedition = \"2021\"\n[dependencies]\nshared = \"=1.0.0\"\n").unwrap();
        std::fs::write(
            consumer.path().join("src/lib.rs"),
            "pub fn value() -> u8 { shared::value() }\n",
        )
        .unwrap();
        std::fs::write(consumer.path().join(".cargo/config.toml"), format!("[source.crates-io]\nreplace-with = \"fixture\"\n[source.fixture]\nregistry = \"sparse+{}cargo/index/\"\n", server.server_address().unwrap())).unwrap();
        let output = std::process::Command::new("cargo")
            .arg("check")
            .current_dir(consumer.path())
            .env("CARGO_HOME", consumer.path().join("cargo-home"))
            .env("CARGO_TARGET_DIR", consumer.path().join("target"))
            .env_remove("CARGO_SOURCE_CRATES_IO_REPLACE_WITH")
            .env_remove("CARGO_SOURCE_VM_REGISTRY")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
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

        let server = TestServer::new(app);

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
        let saved_content = std::fs::read(crate_path).expect("Failed to read saved crate file");
        assert_eq!(saved_content, crate_content);

        // Verify index file was created
        let index_path_str = index_path(crate_name).expect("Failed to get index path");
        let index_file_path = state.data_dir.join("cargo/index").join(&index_path_str);
        assert!(index_file_path.exists());

        let index_content =
            std::fs::read_to_string(index_file_path).expect("Failed to read index file");
        assert!(index_content.contains(crate_name));
        assert!(index_content.contains(version));
        assert!(index_content.contains("\"cksum\":"));
    }

    #[tokio::test]
    async fn test_publish_crate_appends_to_existing_index() {
        let (state, _temp_dir) = create_cargo_test_state();

        let crate_name = "test-crate";
        let index_path_str = index_path(crate_name).expect("Failed to get index path");
        let index_file_path = state.data_dir.join("cargo/index").join(&index_path_str);

        // Create directory and initial index entry
        std::fs::create_dir_all(
            index_file_path
                .parent()
                .expect("Index path should have parent"),
        )
        .expect("Failed to create parent dir");
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
            format!(
                "{}\n",
                serde_json::to_string(&existing_entry).expect("Failed to serialize existing entry")
            ),
        )
        .expect("Failed to write existing index entry");

        let app = axum::Router::new()
            .route(
                "/cargo/api/v1/crates/new",
                axum::routing::put(publish_crate),
            )
            .with_state(state.clone());

        let server = TestServer::new(app);

        let version = "1.0.0";
        let crate_content = b"fake crate content";
        let payload = create_cargo_publish_payload(crate_name, version, crate_content);

        let response = server
            .put("/cargo/api/v1/crates/new")
            .bytes(payload.into())
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);

        // Verify index file contains both versions
        let index_content =
            std::fs::read_to_string(index_file_path).expect("Failed to read index file");
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

        let server = TestServer::new(app);

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

        let server = TestServer::new(app);
        let response = server
            .get("/cargo/config.json")
            .add_header("host", "example.com:3000")
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        let body: serde_json::Value = response.json();

        let dl_url = body["dl"].as_str().expect("dl should be a string");
        assert!(dl_url.contains("example.com:3000"));
        assert!(dl_url.contains("{crate}"));
        assert!(dl_url.contains("{version}"));
        assert_eq!(body["auth-required"], false);
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

        let index_path_str = index_path(crate_name).expect("Failed to get index path");
        let index_file_path = state.data_dir.join("cargo/index").join(&index_path_str);
        std::fs::create_dir_all(
            index_file_path
                .parent()
                .expect("index path should have parent"),
        )
        .expect("Failed to create parent dir");
        std::fs::write(
            &index_file_path,
            format!(
                "{}\n",
                serde_json::to_string(&index_entry).expect("Failed to serialize index entry")
            ),
        )
        .expect("Failed to write index file");

        let app = axum::Router::new()
            .route("/cargo/index/{crate}", axum::routing::get(index_file))
            .with_state(state);

        let server = TestServer::new(app);
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
        std::fs::write(&crate_path, content).expect("Failed to write test crate file");

        let app = axum::Router::new()
            .route(
                "/cargo/api/v1/crates/{crate}/{version}/download",
                axum::routing::get(download_crate),
            )
            .with_state(state);

        let server = TestServer::new(app);
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
