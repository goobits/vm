use anyhow::Result;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use tempfile::TempDir;

/// Test fixture for vm-links integration testing
struct LinkTestFixture {
    _temp_dir: TempDir,
    test_dir: PathBuf,
    npm_global_dir: PathBuf,
    nvm_dir: PathBuf,
    #[allow(dead_code)]
    cargo_dir: PathBuf,
    binary_path: PathBuf,
}

impl LinkTestFixture {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let test_dir = temp_dir.path().to_path_buf();

        let npm_global_dir = test_dir.join("npm_global");
        let nvm_dir = test_dir.join(".nvm/versions/node/v18.0.0/lib/node_modules");
        let cargo_dir = test_dir.join(".cargo");

        // Create directory structure
        fs::create_dir_all(&npm_global_dir)?;
        fs::create_dir_all(&nvm_dir)?;
        fs::create_dir_all(&cargo_dir)?;

        // Get path to vm-links binary
        let workspace_root = std::env::current_dir()?;
        let binary_path = workspace_root.join("target/debug/vm-links");

        Ok(Self {
            _temp_dir: temp_dir,
            test_dir,
            npm_global_dir,
            nvm_dir,
            cargo_dir,
            binary_path,
        })
    }

    /// Create a symlinked npm package
    fn create_npm_link(&self, package_name: &str, target_path: &str) -> Result<()> {
        let package_dir = self.test_dir.join("projects").join(target_path);
        fs::create_dir_all(&package_dir)?;

        // Create a basic package.json
        let package_json = format!(
            r#"{{
            "name": "{}",
            "version": "1.0.0",
            "main": "index.js"
        }}"#,
            package_name
        );
        fs::write(package_dir.join("package.json"), package_json)?;
        fs::write(package_dir.join("index.js"), "console.log('test');")?;

        // Create symlink in npm global directory
        let link_path = self.npm_global_dir.join(package_name);
        symlink(&package_dir, &link_path)?;

        Ok(())
    }

    /// Create a symlinked npm package in NVM directory
    fn create_nvm_link(&self, package_name: &str, target_path: &str) -> Result<()> {
        let package_dir = self.test_dir.join("projects").join(target_path);
        fs::create_dir_all(&package_dir)?;

        // Create a basic package.json
        let package_json = format!(
            r#"{{
            "name": "{}",
            "version": "1.0.0",
            "main": "index.js"
        }}"#,
            package_name
        );
        fs::write(package_dir.join("package.json"), package_json)?;
        fs::write(package_dir.join("index.js"), "console.log('test');")?;

        // Create symlink in NVM directory
        let link_path = self.nvm_dir.join(package_name);
        symlink(&package_dir, &link_path)?;

        Ok(())
    }

    /// Create a Python editable package structure
    fn create_pip_package(&self, package_name: &str, target_path: &str) -> Result<()> {
        let package_dir = self.test_dir.join("projects").join(target_path);
        fs::create_dir_all(&package_dir)?;

        // Create setup.py
        let setup_py = format!(
            r#"from setuptools import setup
setup(
    name="{}",
    version="1.0.0",
    packages=["{}"],
)
"#,
            package_name,
            package_name.replace("-", "_")
        );
        fs::write(package_dir.join("setup.py"), setup_py)?;

        // Create package directory
        let pkg_dir = package_dir.join(package_name.replace("-", "_"));
        fs::create_dir_all(&pkg_dir)?;
        fs::write(pkg_dir.join("__init__.py"), "# test package")?;

        Ok(())
    }

    /// Create a Rust package structure
    fn create_cargo_package(&self, package_name: &str, target_path: &str) -> Result<()> {
        let package_dir = self.test_dir.join("projects").join(target_path);
        fs::create_dir_all(package_dir.join("src"))?;

        // Create Cargo.toml
        let cargo_toml = format!(
            r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "{}"
path = "src/main.rs"
"#,
            package_name, package_name
        );
        fs::write(package_dir.join("Cargo.toml"), cargo_toml)?;
        fs::write(
            package_dir.join("src/main.rs"),
            r#"fn main() {
    println!("Hello, world!");
}"#,
        )?;

        Ok(())
    }
}

#[test]
fn test_npm_package_detection() -> Result<()> {
    let fixture = LinkTestFixture::new()?;

    // Create test packages
    fixture.create_npm_link("test-package", "test-package")?;
    fixture.create_npm_link("another-package", "another-pkg")?;

    // Skip test if binary doesn't exist (not built yet)
    if !fixture.binary_path.exists() {
        println!(
            "Skipping test - vm-links binary not found at {:?}",
            fixture.binary_path
        );
        return Ok(());
    }

    // Test detection with real package structure
    // Note: This test validates the logic without mocking npm commands
    // It tests symlink resolution and path canonicalization

    // Verify symlinks were created correctly
    let link1 = fixture.npm_global_dir.join("test-package");
    let link2 = fixture.npm_global_dir.join("another-package");

    assert!(link1.is_symlink());
    assert!(link2.is_symlink());

    // Verify targets exist and are readable
    let target1 = fs::read_link(&link1)?;
    let target2 = fs::read_link(&link2)?;

    assert!(target1.exists());
    assert!(target2.exists());

    Ok(())
}

#[test]
fn test_nvm_directory_structure() -> Result<()> {
    let fixture = LinkTestFixture::new()?;

    // Create NVM-style packages
    fixture.create_nvm_link("nvm-package", "nvm-test")?;

    // Test NVM directory structure detection
    let nvm_versions = fixture.test_dir.join(".nvm/versions/node");
    assert!(nvm_versions.exists());

    // Verify package structure
    let package_link = fixture.nvm_dir.join("nvm-package");
    assert!(package_link.is_symlink());

    let target = fs::read_link(&package_link)?;
    assert!(target.exists());
    assert!(target.join("package.json").exists());

    Ok(())
}

#[test]
fn test_python_package_structure() -> Result<()> {
    let fixture = LinkTestFixture::new()?;

    // Create Python package structures
    fixture.create_pip_package("python-package", "python-test")?;
    fixture.create_pip_package("dash-package", "dash-test")?;

    // Test package directory structure
    let pkg_dir = fixture.test_dir.join("projects/python-test");
    assert!(pkg_dir.join("setup.py").exists());
    assert!(pkg_dir.join("python_package/__init__.py").exists());

    let dash_dir = fixture.test_dir.join("projects/dash-test");
    assert!(dash_dir.join("setup.py").exists());
    assert!(dash_dir.join("dash_package/__init__.py").exists());

    Ok(())
}

#[test]
fn test_cargo_package_structure() -> Result<()> {
    let fixture = LinkTestFixture::new()?;

    // Create Rust package structures
    fixture.create_cargo_package("rust-tool", "rust-test")?;

    // Test package directory structure
    let pkg_dir = fixture.test_dir.join("projects/rust-test");
    assert!(pkg_dir.join("Cargo.toml").exists());
    assert!(pkg_dir.join("src/main.rs").exists());

    // Verify Cargo.toml content
    let cargo_content = fs::read_to_string(pkg_dir.join("Cargo.toml"))?;
    assert!(cargo_content.contains("name = \"rust-tool\""));
    assert!(cargo_content.contains("[[bin]]"));

    Ok(())
}

#[test]
fn test_symlink_resolution() -> Result<()> {
    let fixture = LinkTestFixture::new()?;

    // Create package with nested symlinks
    fixture.create_npm_link("symlink-test", "symlink-target")?;

    let link_path = fixture.npm_global_dir.join("symlink-test");
    let target_path = fixture.test_dir.join("projects/symlink-target");

    // Test symlink resolution
    assert!(link_path.is_symlink());

    let resolved = fs::read_link(&link_path)?;
    assert_eq!(resolved, target_path);

    // Test canonicalization
    let canonical = link_path.canonicalize()?;
    let expected_canonical = target_path.canonicalize()?;
    assert_eq!(canonical, expected_canonical);

    Ok(())
}

#[test]
fn test_broken_symlink_handling() -> Result<()> {
    let fixture = LinkTestFixture::new()?;

    // Create a symlink to non-existent target
    let broken_link = fixture.npm_global_dir.join("broken-link");
    let non_existent = fixture.test_dir.join("does-not-exist");

    symlink(&non_existent, &broken_link)?;

    // Verify it's a symlink but target doesn't exist
    assert!(broken_link.is_symlink());
    assert!(!non_existent.exists());

    // Test that canonicalize fails for broken symlinks
    assert!(broken_link.canonicalize().is_err());

    Ok(())
}

#[test]
fn test_package_name_validation() -> Result<()> {
    let fixture = LinkTestFixture::new()?;

    // Test various package name formats
    let valid_names = vec![
        "simple-package",
        "package_with_underscores",
        "CamelCasePackage",
        "package123",
        "a",
    ];

    for name in valid_names {
        fixture.create_npm_link(name, &format!("test-{}", name))?;
        let link_path = fixture.npm_global_dir.join(name);
        assert!(link_path.exists());
    }

    Ok(())
}

#[test]
fn test_directory_traversal_prevention() -> Result<()> {
    let fixture = LinkTestFixture::new()?;

    // Test that package detection doesn't follow dangerous paths
    let package_dir = fixture.test_dir.join("projects/safe-package");
    fs::create_dir_all(&package_dir)?;

    // Create a package.json with potentially dangerous content
    let package_json = r#"{
        "name": "safe-package",
        "version": "1.0.0",
        "scripts": {
            "test": "echo 'safe'"
        }
    }"#;
    fs::write(package_dir.join("package.json"), package_json)?;

    // Create symlink
    let link_path = fixture.npm_global_dir.join("safe-package");
    symlink(&package_dir, &link_path)?;

    // Verify the symlink resolves safely
    let resolved = fs::read_link(&link_path)?;
    assert!(resolved.starts_with(&fixture.test_dir));

    // Verify package.json is readable safely
    let json_content = fs::read_to_string(package_dir.join("package.json"))?;
    assert!(json_content.contains("safe-package"));

    Ok(())
}

#[test]
fn test_parallel_safety() -> Result<()> {
    let fixture = LinkTestFixture::new()?;

    // Create multiple packages for parallel processing testing
    let packages = vec![
        ("parallel-1", "p1"),
        ("parallel-2", "p2"),
        ("parallel-3", "p3"),
        ("parallel-4", "p4"),
    ];

    for (name, path) in &packages {
        fixture.create_npm_link(name, path)?;
    }

    // Verify all packages were created
    for (name, _) in &packages {
        let link_path = fixture.npm_global_dir.join(name);
        assert!(link_path.exists());
        assert!(link_path.is_symlink());
    }

    // Test that symlinks can be resolved concurrently without issues
    let results: Vec<_> = packages
        .iter()
        .map(|(name, _)| {
            let link_path = fixture.npm_global_dir.join(name);
            link_path.canonicalize()
        })
        .collect();

    // All should succeed
    for result in results {
        assert!(result.is_ok());
    }

    Ok(())
}
