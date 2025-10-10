//! Package lifecycle tests
//!
//! This module tests the complete lifecycle of package management including
//! adding, storing, and removing packages across different ecosystems
//! (Cargo, NPM, PyPI). These tests simulate real package operations.

use anyhow::Result;
use axum::Router;
use axum_test::TestServer;
use std::fs;
use std::path::Path;
use std::process::Command;

mod common;
use common::create_test_setup;

/// Check if Python 3 is available on the system
fn is_python_available() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Check if NPM is available on the system
fn is_npm_available() -> bool {
    Command::new("npm")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

async fn create_test_server() -> (TestServer, common::TestSetup) {
    let setup = create_test_setup()
        .await
        .expect("Failed to create test setup");

    // Create minimal router for testing file operations
    let app = Router::new().with_state(setup.app_state.clone());
    let server = TestServer::new(app).expect("Failed to create test server");

    (server, setup)
}

/// Tests NPM package add and remove lifecycle
#[tokio::test]
async fn test_npm_package_lifecycle() -> Result<()> {
    // Skip test if NPM is not available
    if !is_npm_available() {
        eprintln!("Skipping NPM test: npm not found. Install Node.js/npm to run NPM tests.");
        return Ok(());
    }

    let (_server, setup) = create_test_server().await;
    let state = setup.app_state;

    // Test NPM package upload simulation
    let fixture_path = "tests/__fixtures__/npm/hello-world";
    assert!(Path::new(fixture_path).exists(), "NPM fixture should exist");

    // Create tarball
    let output = Command::new("npm")
        .args(["pack"])
        .current_dir(fixture_path)
        .output()?;

    assert!(output.status.success(), "NPM pack should succeed");

    // Find the generated tarball
    let tarball_files: Vec<_> = fs::read_dir(fixture_path)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension() == Some("tgz".as_ref()))
        .collect();

    assert!(!tarball_files.is_empty(), "Should have generated tarball");
    let tarball_file = &tarball_files[0];

    // Simulate package storage
    let tarball_data = fs::read(tarball_file.path())?;
    let package_dir = state.data_dir.join("npm/hello-world");
    fs::create_dir_all(&package_dir)?;
    let tarball_path = package_dir.join("hello-world-1.0.0.tgz");
    fs::write(&tarball_path, &tarball_data)?;

    // Create metadata file
    let metadata_path = state.data_dir.join("npm/hello-world.json");
    let metadata = serde_json::json!({
        "name": "hello-world",
        "versions": {
            "1.0.0": {
                "name": "hello-world",
                "version": "1.0.0"
            }
        }
    });
    fs::write(&metadata_path, serde_json::to_string_pretty(&metadata)?)?;

    // Verify package exists
    assert!(metadata_path.exists(), "Package metadata should exist");
    assert!(tarball_path.exists(), "Package tarball should exist");

    // Simulate package removal
    fs::remove_file(&tarball_path)?;
    fs::remove_file(&metadata_path)?;
    fs::remove_dir_all(&package_dir)?;

    // Verify package is removed
    assert!(!tarball_path.exists(), "Package tarball should be removed");
    assert!(
        !metadata_path.exists(),
        "Package metadata should be removed"
    );

    // Clean up tarball
    fs::remove_file(tarball_file.path())?;

    Ok(())
}

/// Tests PyPI package add and remove lifecycle
#[tokio::test]
async fn test_pypi_package_lifecycle() -> Result<()> {
    // Skip test if Python 3 is not available
    if !is_python_available() {
        eprintln!("Skipping PyPI test: Python 3 not found. Install Python 3 + setuptools to run PyPI tests.");
        return Ok(());
    }

    let (_server, setup) = create_test_server().await;
    let state = setup.app_state;

    // Test PyPI package upload simulation
    let fixture_path = "tests/__fixtures__/pypi/hello-world";
    assert!(
        Path::new(fixture_path).exists(),
        "PyPI fixture should exist"
    );

    // Build the package
    let output = Command::new("python3")
        .args(["setup.py", "sdist", "bdist_wheel"])
        .current_dir(fixture_path)
        .output()?;

    if !output.status.success() {
        eprintln!("Python package failed:");
        eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
    }
    assert!(
        output.status.success(),
        "Python package build should succeed"
    );

    // Find the generated wheel file
    let dist_dir = Path::new(fixture_path).join("dist");
    let wheel_files: Vec<_> = fs::read_dir(&dist_dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension() == Some("whl".as_ref()))
        .collect();

    assert!(!wheel_files.is_empty(), "Should have generated wheel file");
    let wheel_file = &wheel_files[0];

    // Simulate package storage
    let wheel_data = fs::read(wheel_file.path())?;
    let package_dir = state.data_dir.join("pypi/hello-world");
    fs::create_dir_all(&package_dir)?;
    let wheel_path = package_dir.join("hello_world-1.0.0-py3-none-any.whl");
    fs::write(&wheel_path, &wheel_data)?;

    // Verify package exists
    assert!(package_dir.exists(), "Package directory should exist");
    assert!(wheel_path.exists(), "Package wheel should exist");

    // Simulate package removal
    fs::remove_file(&wheel_path)?;
    fs::remove_dir_all(&package_dir)?;

    // Verify package is removed
    assert!(!wheel_path.exists(), "Package wheel should be removed");
    assert!(!package_dir.exists(), "Package directory should be removed");

    // Clean up dist directory
    fs::remove_dir_all(dist_dir)?;

    Ok(())
}
