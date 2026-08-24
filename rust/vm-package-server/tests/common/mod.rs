//! Common test utilities and helpers
//!
//! This module provides shared functionality for all test files to reduce code duplication
//! and improve maintainability of the test suite.

use anyhow::Result;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;
use vm_package_server::{AppState, Config, ResolverService, UpstreamClient, UpstreamConfig};

/// Test server setup result
pub struct TestSetup {
    _temp_dir: TempDir,
    pub app_state: Arc<AppState>,
}

/// Creates a test server setup with temporary directories and app state
///
/// This function handles the common setup required for most integration tests:
/// - Creates temporary directories for each package ecosystem
/// - Sets up upstream client configuration
/// - Returns app state that can be used for testing
pub async fn create_test_setup() -> Result<TestSetup> {
    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().to_path_buf();

    // Create required directories for all package ecosystems
    create_package_directories(&data_dir)?;

    let upstream_config = UpstreamConfig {
        enabled: false,
        ..Default::default()
    };
    let upstream_client = Arc::new(UpstreamClient::new(upstream_config).unwrap());
    let config = Arc::new(Config::default());

    let app_state = Arc::new(AppState {
        data_dir: data_dir.clone(),
        server_addr: "http://localhost:8080".to_string(),
        upstream_client,
        internal_client: None,
        config,
        resolver: Arc::new(ResolverService::standalone()),
    });

    Ok(TestSetup {
        _temp_dir: temp_dir,
        app_state,
    })
}

/// Creates all required package ecosystem directories
pub fn create_package_directories(data_dir: &Path) -> Result<()> {
    // PyPI directories
    fs::create_dir_all(data_dir.join("pypi/packages"))?;

    // NPM directories
    fs::create_dir_all(data_dir.join("npm/tarballs"))?;
    fs::create_dir_all(data_dir.join("npm/metadata"))?;

    // Cargo directories
    fs::create_dir_all(data_dir.join("cargo/crates"))?;
    fs::create_dir_all(data_dir.join("cargo/index"))?;

    Ok(())
}
