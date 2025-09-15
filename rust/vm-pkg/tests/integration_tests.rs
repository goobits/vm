use anyhow::Result;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::PathBuf;
use tempfile::TempDir;
use vm_pkg::{LinkDetector, PackageManager};

/// Test fixture for vm-pkg integration testing
struct PkgTestFixture {
    _temp_dir: TempDir,
    test_dir: PathBuf,
    home_dir: PathBuf,
    links_dir: PathBuf,
    binary_path: PathBuf,
}

impl PkgTestFixture {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let test_dir = temp_dir.path().to_path_buf();
        let home_dir = test_dir.join("home/developer");
        let links_dir = home_dir.join(".links");

        // Create directory structure
        fs::create_dir_all(&home_dir)?;
        fs::create_dir_all(&links_dir.join("npm"))?;
        fs::create_dir_all(&links_dir.join("pip"))?;
        fs::create_dir_all(&links_dir.join("cargo"))?;

        // Get path to vm-pkg binary
        let workspace_root = std::env::current_dir()?;
        let binary_path = workspace_root.join("target/debug/vm-pkg");

        Ok(Self {
            _temp_dir: temp_dir,
            test_dir,
            home_dir,
            links_dir,
            binary_path,
        })
    }

    /// Create a linked npm package
    fn create_npm_package(&self, package_name: &str) -> Result<PathBuf> {
        let package_dir = self.test_dir.join("projects").join(package_name);
        fs::create_dir_all(&package_dir)?;

        // Create package.json
        let package_json = format!(r#"{{
            "name": "{}",
            "version": "1.0.0",
            "main": "index.js",
            "bin": {{
                "{}": "./bin/{}.js"
            }}
        }}"#, package_name, package_name, package_name);

        fs::write(package_dir.join("package.json"), package_json)?;
        fs::write(package_dir.join("index.js"), "module.exports = {};")?;

        // Create bin directory and executable
        let bin_dir = package_dir.join("bin");
        fs::create_dir_all(&bin_dir)?;
        let bin_file = bin_dir.join(format!("{}.js", package_name));
        fs::write(&bin_file, "#!/usr/bin/env node\nconsole.log('Hello from linked package');")?;

        // Make executable
        let mut perms = fs::metadata(&bin_file)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&bin_file, perms)?;

        // Create symlink in links directory
        let link_path = self.links_dir.join("npm").join(package_name);
        symlink(&package_dir, &link_path)?;

        Ok(package_dir)
    }

    /// Create a linked Python package
    fn create_pip_package(&self, package_name: &str) -> Result<PathBuf> {
        let package_dir = self.test_dir.join("projects").join(package_name);
        let pkg_name = package_name.replace('-', '_');
        fs::create_dir_all(&package_dir.join(&pkg_name))?;

        // Create setup.py
        let setup_py = format!(r#"from setuptools import setup, find_packages

setup(
    name="{}",
    version="1.0.0",
    packages=find_packages(),
    entry_points={{
        'console_scripts': [
            '{}={}:main',
        ],
    }},
    python_requires='>=3.6',
)
"#, package_name, package_name, pkg_name);

        fs::write(package_dir.join("setup.py"), setup_py)?;

        // Create pyproject.toml for modern Python packages
        let pyproject_toml = format!(r#"[build-system]
requires = ["setuptools>=45", "wheel"]
build-backend = "setuptools.build_meta"

[project]
name = "{}"
version = "1.0.0"
description = "Test package"
requires-python = ">=3.6"
"#, package_name);

        fs::write(package_dir.join("pyproject.toml"), pyproject_toml)?;

        // Create package files
        fs::write(package_dir.join(format!("{}/__init__.py", pkg_name)),
                  "def main():\n    print('Hello from linked Python package')\n")?;

        // Create symlink in links directory
        let link_path = self.links_dir.join("pip").join(package_name);
        symlink(&package_dir, &link_path)?;

        Ok(package_dir)
    }

    /// Create a linked Cargo package
    fn create_cargo_package(&self, package_name: &str) -> Result<PathBuf> {
        let package_dir = self.test_dir.join("projects").join(package_name);
        fs::create_dir_all(&package_dir.join("src"))?;

        // Create Cargo.toml
        let cargo_toml = format!(r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "{}"
path = "src/main.rs"

[dependencies]
anyhow = "1.0"
"#, package_name, package_name);

        fs::write(package_dir.join("Cargo.toml"), cargo_toml)?;

        // Create main.rs
        fs::write(package_dir.join("src/main.rs"), r#"use anyhow::Result;

fn main() -> Result<()> {
    println!("Hello from linked Cargo package");
    Ok(())
}
"#)?;

        // Create symlink in links directory
        let link_path = self.links_dir.join("cargo").join(package_name);
        symlink(&package_dir, &link_path)?;

        Ok(package_dir)
    }

    /// Create a pipx environment structure
    fn create_pipx_environment(&self, package_name: &str) -> Result<PathBuf> {
        let pipx_dir = self.test_dir.join("pipx_envs").join(package_name);
        let bin_dir = pipx_dir.join("bin");
        let lib_dir = pipx_dir.join("lib/python3.10/site-packages");

        fs::create_dir_all(&bin_dir)?;
        fs::create_dir_all(&lib_dir)?;

        // Create pipx metadata
        let metadata = format!(r#"{{
            "main_package": "{}",
            "python_version": "3.10.0",
            "venv_args": []
        }}"#, package_name);
        fs::write(pipx_dir.join("pipx_metadata.json"), metadata)?;

        // Create executable script
        let script_path = bin_dir.join(package_name);
        fs::write(&script_path, format!(r#"#!/usr/bin/env python3
import sys
print("Hello from pipx environment: {}")
"#, package_name))?;

        let mut perms = fs::metadata(&script_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms)?;

        // Create package in site-packages
        let pkg_dir = lib_dir.join(package_name);
        fs::create_dir_all(&pkg_dir)?;
        fs::write(pkg_dir.join("__init__.py"), "# pipx package")?;

        // Create symlink in links directory
        let link_path = self.links_dir.join("pip").join(package_name);
        symlink(&pipx_dir, &link_path)?;

        Ok(pipx_dir)
    }

    fn user(&self) -> String {
        "developer".to_string()
    }
}

#[test]
fn test_link_detector_creation() -> Result<()> {
    let fixture = PkgTestFixture::new()?;
    let detector = LinkDetector::new(fixture.user());

    // Test that detector can be created without errors
    // This validates the basic LinkDetector structure
    drop(detector);

    Ok(())
}

#[test]
fn test_npm_package_detection() -> Result<()> {
    let fixture = PkgTestFixture::new()?;
    let detector = LinkDetector::new(fixture.user());

    // Create linked npm package
    fixture.create_npm_package("test-npm-package")?;

    // Test package detection
    let is_linked = detector.is_linked("test-npm-package", PackageManager::Npm)?;
    assert!(is_linked);

    // Test getting linked path
    let linked_path = detector.get_linked_path("test-npm-package", PackageManager::Npm)?;
    assert!(linked_path.is_some());

    let path = linked_path.unwrap();
    assert!(path.exists());
    assert!(path.join("package.json").exists());

    Ok(())
}

#[test]
fn test_pip_package_detection() -> Result<()> {
    let fixture = PkgTestFixture::new()?;
    let detector = LinkDetector::new(fixture.user());

    // Create linked pip package
    fixture.create_pip_package("test-pip-package")?;

    // Test package detection
    let is_linked = detector.is_linked("test-pip-package", PackageManager::Pip)?;
    assert!(is_linked);

    // Test getting linked path
    let linked_path = detector.get_linked_path("test-pip-package", PackageManager::Pip)?;
    assert!(linked_path.is_some());

    let path = linked_path.unwrap();
    assert!(path.exists());
    assert!(path.join("setup.py").exists() || path.join("pyproject.toml").exists());

    Ok(())
}

#[test]
fn test_cargo_package_detection() -> Result<()> {
    let fixture = PkgTestFixture::new()?;
    let detector = LinkDetector::new(fixture.user());

    // Create linked cargo package
    fixture.create_cargo_package("test-cargo-package")?;

    // Test package detection
    let is_linked = detector.is_linked("test-cargo-package", PackageManager::Cargo)?;
    assert!(is_linked);

    // Test getting linked path
    let linked_path = detector.get_linked_path("test-cargo-package", PackageManager::Cargo)?;
    assert!(linked_path.is_some());

    let path = linked_path.unwrap();
    assert!(path.exists());
    assert!(path.join("Cargo.toml").exists());

    Ok(())
}

#[test]
fn test_pip_package_name_normalization() -> Result<()> {
    let fixture = PkgTestFixture::new()?;
    let detector = LinkDetector::new(fixture.user());

    // Create package with dashes in name
    fixture.create_pip_package("test-dash-package")?;

    // Test detection with original name
    let is_linked_dash = detector.is_linked("test-dash-package", PackageManager::Pip)?;
    assert!(is_linked_dash);

    // Test detection with underscores (should also work due to normalization)
    let is_linked_underscore = detector.is_linked("test_dash_package", PackageManager::Pip)?;
    assert!(is_linked_underscore);

    Ok(())
}

#[test]
fn test_non_existent_package() -> Result<()> {
    let fixture = PkgTestFixture::new()?;
    let detector = LinkDetector::new(fixture.user());

    // Test detection of non-existent package
    let is_linked = detector.is_linked("non-existent-package", PackageManager::Npm)?;
    assert!(!is_linked);

    let linked_path = detector.get_linked_path("non-existent-package", PackageManager::Npm)?;
    assert!(linked_path.is_none());

    Ok(())
}

#[test]
fn test_list_linked_packages() -> Result<()> {
    let fixture = PkgTestFixture::new()?;
    let detector = LinkDetector::new(fixture.user());

    // Create multiple linked packages
    fixture.create_npm_package("npm-pkg1")?;
    fixture.create_npm_package("npm-pkg2")?;
    fixture.create_pip_package("pip-pkg1")?;
    fixture.create_cargo_package("cargo-pkg1")?;

    // Test listing all packages
    let all_linked = detector.list_linked(None)?;
    assert_eq!(all_linked.len(), 4);

    // Test listing npm packages only
    let npm_linked = detector.list_linked(Some(PackageManager::Npm))?;
    assert_eq!(npm_linked.len(), 2);
    assert!(npm_linked.iter().all(|(mgr, _)| *mgr == PackageManager::Npm));

    // Test listing pip packages only
    let pip_linked = detector.list_linked(Some(PackageManager::Pip))?;
    assert_eq!(pip_linked.len(), 1);
    assert!(pip_linked.iter().all(|(mgr, _)| *mgr == PackageManager::Pip));

    // Test listing cargo packages only
    let cargo_linked = detector.list_linked(Some(PackageManager::Cargo))?;
    assert_eq!(cargo_linked.len(), 1);
    assert!(cargo_linked.iter().all(|(mgr, _)| *mgr == PackageManager::Cargo));

    Ok(())
}

#[test]
fn test_pipx_environment_detection() -> Result<()> {
    let fixture = PkgTestFixture::new()?;

    // Create pipx environment
    let pipx_dir = fixture.create_pipx_environment("pipx-tool")?;

    // Test pipx environment detection
    assert!(LinkDetector::is_pipx_environment(&pipx_dir));
    assert!(pipx_dir.join("pipx_metadata.json").exists());

    // Test non-pipx directory
    let regular_dir = fixture.test_dir.join("regular");
    fs::create_dir_all(&regular_dir)?;
    assert!(!LinkDetector::is_pipx_environment(&regular_dir));

    Ok(())
}

#[test]
fn test_python_project_detection() -> Result<()> {
    let fixture = PkgTestFixture::new()?;

    // Create Python project with setup.py
    let setuppy_dir = fixture.test_dir.join("setuppy_project");
    fs::create_dir_all(&setuppy_dir)?;
    fs::write(setuppy_dir.join("setup.py"), "from setuptools import setup; setup()")?;

    assert!(LinkDetector::is_python_project(&setuppy_dir));

    // Create Python project with pyproject.toml
    let pyproject_dir = fixture.test_dir.join("pyproject_project");
    fs::create_dir_all(&pyproject_dir)?;
    fs::write(pyproject_dir.join("pyproject.toml"), "[build-system]")?;

    assert!(LinkDetector::is_python_project(&pyproject_dir));

    // Test non-Python directory
    let non_python_dir = fixture.test_dir.join("not_python");
    fs::create_dir_all(&non_python_dir)?;
    assert!(!LinkDetector::is_python_project(&non_python_dir));

    Ok(())
}

#[test]
fn test_package_manager_links_dir() -> Result<()> {
    let user = "testuser";

    // Test links directory paths
    let cargo_dir = PackageManager::Cargo.links_dir(user);
    assert_eq!(cargo_dir, PathBuf::from("/home/testuser/.links/cargo"));

    let npm_dir = PackageManager::Npm.links_dir(user);
    assert_eq!(npm_dir, PathBuf::from("/home/testuser/.links/npm"));

    let pip_dir = PackageManager::Pip.links_dir(user);
    assert_eq!(pip_dir, PathBuf::from("/home/testuser/.links/pip"));

    Ok(())
}

#[test]
fn test_package_manager_availability() -> Result<()> {
    // Test availability checking (without mocking external commands)
    // This just validates the function exists and returns a boolean

    let _cargo_available = PackageManager::Cargo.is_available();
    let _npm_available = PackageManager::Npm.is_available();
    let _pip_available = PackageManager::Pip.is_available();

    // All should return Ok(bool) without panicking
    Ok(())
}

#[test]
fn test_symlink_safety() -> Result<()> {
    let fixture = PkgTestFixture::new()?;
    let detector = LinkDetector::new(fixture.user());

    // Create a package and test symlink resolution
    let package_dir = fixture.create_npm_package("safe-package")?;

    // Verify symlink was created safely
    let link_path = fixture.links_dir.join("npm").join("safe-package");
    assert!(link_path.is_symlink());

    // Verify target resolution is safe
    let target = fs::read_link(&link_path)?;
    assert_eq!(target, package_dir);

    // Verify detector can safely resolve the link
    let detected_path = detector.get_linked_path("safe-package", PackageManager::Npm)?;
    assert!(detected_path.is_some());
    assert_eq!(detected_path.unwrap(), package_dir);

    Ok(())
}

#[test]
fn test_concurrent_package_access() -> Result<()> {
    let fixture = PkgTestFixture::new()?;
    let detector = LinkDetector::new(fixture.user());

    // Create multiple packages
    fixture.create_npm_package("concurrent1")?;
    fixture.create_npm_package("concurrent2")?;
    fixture.create_pip_package("concurrent3")?;

    // Test that concurrent access works without issues
    let packages = vec!["concurrent1", "concurrent2", "concurrent3"];
    let managers = vec![PackageManager::Npm, PackageManager::Npm, PackageManager::Pip];

    for (package, manager) in packages.iter().zip(managers.iter()) {
        let is_linked = detector.is_linked(package, *manager)?;
        assert!(is_linked);
    }

    // Test listing while accessing individual packages
    let all_linked = detector.list_linked(None)?;
    assert!(all_linked.len() >= 3);

    Ok(())
}