#[cfg(feature = "integration")]
use anyhow::Result;
#[cfg(feature = "integration")]
use std::fs;
#[cfg(feature = "integration")]
use std::path::PathBuf;
#[cfg(feature = "integration")]
use std::process::Command;
#[cfg(feature = "integration")]
use std::sync::Mutex;
#[cfg(feature = "integration")]
use tempfile::TempDir;

// Global mutex to ensure tests run sequentially to avoid environment variable conflicts
#[cfg(feature = "integration")]
static TEST_MUTEX: Mutex<()> = Mutex::new(());

/// Test fixture for end-to-end CLI workflow testing
#[cfg(feature = "integration")]
struct WorkflowTestFixture {
    _temp_dir: TempDir,
    test_dir: PathBuf,
    binary_path: PathBuf,
}

#[cfg(feature = "integration")]
impl WorkflowTestFixture {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let test_dir = temp_dir.path().join("test_project");
        fs::create_dir_all(&test_dir)?;

        // Get the path to the vm binary
        let workspace_root = std::env::current_dir()?;

        // Try multiple possible binary locations (prioritize .build location)
        let possible_paths = vec![
            // From workspace root (two levels up from rust/vm): /workspace/.build/target/debug/vm
            workspace_root
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join(".build")
                .join("target")
                .join("debug")
                .join("vm"),
            // From rust root (one level up from rust/vm): /workspace/rust/target/debug/vm
            workspace_root
                .parent()
                .unwrap()
                .join("target")
                .join("debug")
                .join("vm"),
            // From current dir: /workspace/rust/vm/target/debug/vm
            workspace_root.join("target").join("debug").join("vm"),
        ];

        let binary_path = possible_paths
            .into_iter()
            .find(|path| path.exists())
            .expect("Could not find vm binary in any of the expected locations");

        Ok(Self {
            _temp_dir: temp_dir,
            test_dir,
            binary_path,
        })
    }

    /// Run vm command with given arguments in the test directory
    fn run_vm_command(&self, args: &[&str]) -> Result<std::process::Output> {
        let output = Command::new(&self.binary_path)
            .args(args)
            .current_dir(&self.test_dir)
            .env("HOME", self.test_dir.parent().unwrap())
            .env("VM_TOOL_DIR", &self.test_dir)
            .env("VM_TEST_MODE", "1") // Disable structured logging for tests
            .output()?;
        Ok(output)
    }

    /// Check if a file exists in the test directory
    fn file_exists(&self, filename: &str) -> bool {
        self.test_dir.join(filename).exists()
    }

    /// Get the contents of a file as a string
    fn read_file(&self, filename: &str) -> Result<String> {
        let path = self.test_dir.join(filename);
        Ok(fs::read_to_string(path)?)
    }

    /// Create a project file to simulate different project types
    fn create_project_file(&self, filename: &str, content: &str) -> Result<()> {
        fs::write(self.test_dir.join(filename), content)?;
        Ok(())
    }

    /// Create a preset file for testing
    fn create_preset(&self, name: &str, content: &str) -> Result<()> {
        let presets_dir = self.test_dir.join("configs").join("presets");
        fs::create_dir_all(&presets_dir)?;
        let preset_path = presets_dir.join(format!("{}.yaml", name));

        // Add preset metadata header to the content
        let full_content = format!(
            "---\npreset:\n  name: {}\n  description: \"Test preset for {}\"\n\n{}",
            name, name, content
        );

        fs::write(preset_path, full_content)?;
        Ok(())
    }
}

#[test]
#[cfg(feature = "integration")]
fn test_basic_config_workflow() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = WorkflowTestFixture::new()?;

    // Skip test if binary doesn't exist
    if !fixture.binary_path.exists() {
        println!(
            "Skipping test - vm binary not found at {:?}",
            fixture.binary_path
        );
        return Ok(());
    }

    // Step 1: Set a basic configuration value
    let output = fixture.run_vm_command(&["config", "set", "vm.memory", "4096"])?;
    assert!(
        output.status.success(),
        "Failed to set config: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify file was created
    assert!(fixture.file_exists("vm.yaml"));

    // Step 2: Get the value back
    let output = fixture.run_vm_command(&["config", "get", "vm.memory"])?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert_eq!(stdout.trim(), "4096");

    // Step 3: Set another value to build up configuration
    let output = fixture.run_vm_command(&["config", "set", "provider", "docker"])?;
    assert!(output.status.success());

    // Step 4: Get all configuration
    let output = fixture.run_vm_command(&["config", "get"])?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("memory: 4096"));
    assert!(stdout.contains("provider: docker"));

    Ok(())
}

#[test]
#[cfg(feature = "integration")]
fn test_preset_application_workflow() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = WorkflowTestFixture::new()?;

    if !fixture.binary_path.exists() {
        println!("Skipping test - vm binary not found");
        return Ok(());
    }

    // Step 1: Create a custom preset
    fixture.create_preset(
        "workflow-test",
        r#"
vm:
  memory: 8192
  cpus: 4
services:
  redis:
    enabled: true
  postgresql:
    enabled: true
npm_packages:
  - eslint
  - prettier
"#,
    )?;

    // Step 2: List presets to verify it's available
    let output = fixture.run_vm_command(&["config", "preset", "--list"])?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("workflow-test"));

    // Step 3: Apply the preset
    let output = fixture.run_vm_command(&["config", "preset", "workflow-test"])?;
    assert!(output.status.success());

    // Step 4: Verify preset was applied
    assert!(fixture.file_exists("vm.yaml"));
    let config_content = fixture.read_file("vm.yaml")?;
    assert!(config_content.contains("memory: 8192"));
    assert!(config_content.contains("cpus: 4"));
    assert!(config_content.contains("redis:"));
    assert!(config_content.contains("postgresql:"));
    assert!(config_content.contains("eslint"));

    // Step 5: Override a value from the preset
    let output = fixture.run_vm_command(&["config", "set", "vm.memory", "16384"])?;
    assert!(output.status.success());

    // Step 6: Verify override worked
    let output = fixture.run_vm_command(&["config", "get", "vm.memory"])?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert_eq!(stdout.trim(), "16384");

    Ok(())
}

#[test]
#[cfg(feature = "integration")]
fn test_nested_configuration_workflow() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = WorkflowTestFixture::new()?;

    if !fixture.binary_path.exists() {
        println!("Skipping test - vm binary not found");
        return Ok(());
    }

    // Step 1: Set nested configuration using dot notation
    let output = fixture.run_vm_command(&["config", "set", "services.postgresql.version", "15"])?;
    assert!(output.status.success());

    let output = fixture.run_vm_command(&["config", "set", "services.postgresql.port", "5432"])?;
    assert!(output.status.success());

    let output = fixture.run_vm_command(&["config", "set", "services.redis.enabled", "true"])?;
    assert!(output.status.success());

    // Step 2: Verify nested structure was created by parsing the YAML output
    let output = fixture.run_vm_command(&["config", "get"])?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;

    // Extract YAML content (skip header lines and footer hints)
    let yaml_content: String = stdout
        .lines()
        .skip_while(|line| {
            line.trim().is_empty() || line.starts_with("📋") || line.starts_with("💡")
        })
        .take_while(|line| !line.starts_with("💡"))
        .collect::<Vec<_>>()
        .join("\n");

    // Parse the YAML output to check values programmatically (more robust than string matching)
    let config_value: serde_yaml::Value = serde_yaml::from_str(&yaml_content)?;

    // Verify postgresql service was configured correctly
    let postgresql = &config_value["services"]["postgresql"];
    assert_eq!(
        postgresql["version"],
        serde_yaml::Value::String("15".to_string())
    );
    assert_eq!(postgresql["port"], serde_yaml::Value::Number(5432.into()));

    // Verify redis service was enabled
    let redis = &config_value["services"]["redis"];
    assert_eq!(redis["enabled"], serde_yaml::Value::Bool(true));

    // Step 3: Get specific nested values
    let output = fixture.run_vm_command(&["config", "get", "services.postgresql.version"])?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert_eq!(stdout.trim(), "15");

    // Step 4: Unset a nested value
    let output = fixture.run_vm_command(&["config", "unset", "services.postgresql.port"])?;
    assert!(output.status.success());

    // Step 5: Verify unset worked by parsing YAML
    let output = fixture.run_vm_command(&["config", "get"])?;
    let stdout = String::from_utf8(output.stdout)?;

    // Extract YAML content
    let yaml_content: String = stdout
        .lines()
        .skip_while(|line| {
            line.trim().is_empty() || line.starts_with("📋") || line.starts_with("💡")
        })
        .take_while(|line| !line.starts_with("💡"))
        .collect::<Vec<_>>()
        .join("\n");

    let config_value: serde_yaml::Value = serde_yaml::from_str(&yaml_content)?;

    // Port should be removed
    let postgresql = &config_value["services"]["postgresql"];
    assert!(
        postgresql["port"].is_null()
            || !postgresql
                .as_mapping()
                .unwrap()
                .contains_key(serde_yaml::Value::String("port".to_string()))
    );
    // Version should remain
    assert_eq!(
        postgresql["version"],
        serde_yaml::Value::String("15".to_string())
    );

    Ok(())
}

#[test]
#[cfg(feature = "integration")]
fn test_global_vs_local_config_workflow() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = WorkflowTestFixture::new()?;

    if !fixture.binary_path.exists() {
        println!("Skipping test - vm binary not found");
        return Ok(());
    }

    // Step 1: Set global configuration
    let output = fixture.run_vm_command(&["config", "set", "--global", "provider", "tart"])?;
    assert!(output.status.success());

    let output = fixture.run_vm_command(&["config", "set", "--global", "vm.cpus", "8"])?;
    assert!(output.status.success());

    // Step 2: Set local configuration
    let output = fixture.run_vm_command(&["config", "set", "vm.memory", "4096"])?;
    assert!(output.status.success());

    let output = fixture.run_vm_command(&["config", "set", "provider", "docker"])?;
    assert!(output.status.success());

    // Step 3: Verify global config
    let output = fixture.run_vm_command(&["config", "get", "--global"])?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("provider: tart"));
    assert!(stdout.contains("cpus: 8"));

    // Step 4: Verify local config
    let output = fixture.run_vm_command(&["config", "get"])?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("provider: docker")); // Local overrides global
    assert!(stdout.contains("memory: 4096"));

    // Step 5: Verify local provider overrides global
    let output = fixture.run_vm_command(&["config", "get", "provider"])?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert_eq!(stdout.trim(), "docker");

    Ok(())
}

#[test]
#[cfg(feature = "integration")]
fn test_preset_composition_workflow() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = WorkflowTestFixture::new()?;

    if !fixture.binary_path.exists() {
        println!("Skipping test - vm binary not found");
        return Ok(());
    }

    // Step 1: Create base preset
    fixture.create_preset(
        "base-preset",
        r#"
vm:
  memory: 2048
  cpus: 2
services:
  redis:
    enabled: true
npm_packages:
  - eslint
"#,
    )?;

    // Step 2: Create override preset
    fixture.create_preset(
        "override-preset",
        r#"
vm:
  memory: 4096  # Override memory
  swap: 1024    # Add new field
services:
  postgresql:
    enabled: true  # Add new service
    port: 3000
npm_packages:
  - prettier      # Replace packages
ports:
  _range: [3000, 3010]  # Add port range
"#,
    )?;

    // Step 3: Apply both presets in sequence
    let output = fixture.run_vm_command(&["config", "preset", "base-preset,override-preset"])?;
    assert!(output.status.success());

    // Step 4: Verify composition results
    let config_content = fixture.read_file("vm.yaml")?;

    // Memory should be from override preset
    assert!(config_content.contains("memory: 4096"));

    // CPUs should be from base preset (not overridden)
    assert!(config_content.contains("cpus: 2"));

    // Swap should be added from override
    assert!(config_content.contains("swap: 1024"));

    // Both services should be present
    assert!(config_content.contains("redis:"));
    assert!(config_content.contains("postgresql:"));

    // npm_packages should be from override (arrays replace)
    assert!(config_content.contains("prettier"));

    // Ports range should be added
    assert!(config_content.contains("_range:"));

    Ok(())
}

#[test]
#[cfg(feature = "integration")]
fn test_configuration_error_recovery() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = WorkflowTestFixture::new()?;

    if !fixture.binary_path.exists() {
        println!("Skipping test - vm binary not found");
        return Ok(());
    }

    // Step 1: Try to get config from non-existent file
    let output = fixture.run_vm_command(&["config", "get", "vm.memory"])?;
    assert!(!output.status.success());

    // Step 2: Set a valid configuration
    let output = fixture.run_vm_command(&["config", "set", "vm.memory", "4096"])?;
    assert!(output.status.success());

    // Step 3: Try to apply non-existent preset
    let output = fixture.run_vm_command(&["config", "preset", "nonexistent"])?;
    assert!(!output.status.success());

    // Step 4: Verify original config is still intact
    let output = fixture.run_vm_command(&["config", "get", "vm.memory"])?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert_eq!(stdout.trim(), "4096");

    // Step 5: Try to unset from non-existent nested path
    let _output = fixture.run_vm_command(&["config", "unset", "nonexistent.path"])?;
    // This might succeed or fail depending on implementation, but shouldn't crash

    // Step 6: Verify main config is still intact
    let output = fixture.run_vm_command(&["config", "get", "vm.memory"])?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert_eq!(stdout.trim(), "4096");

    Ok(())
}

#[test]
#[cfg(feature = "integration")]
fn test_project_type_detection_workflow() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = WorkflowTestFixture::new()?;

    if !fixture.binary_path.exists() {
        println!("Skipping test - vm binary not found");
        return Ok(());
    }

    // Step 1: Create the nodejs preset for testing
    fixture.create_preset(
        "nodejs",
        r#"
provider: docker
vm:
  memory: 2048
  cpus: 2
npm_packages:
  - nodemon
  - eslint
environment:
  NODE_ENV: development
"#,
    )?;

    // Step 2: Create a Node.js project indicator
    fixture.create_project_file(
        "package.json",
        r#"{
        "name": "test-project",
        "version": "1.0.0",
        "dependencies": {
            "express": "^4.21.1"
        }
    }"#,
    )?;

    // Step 3: List available presets (should include nodejs)
    let output = fixture.run_vm_command(&["config", "preset", "--list"])?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("nodejs"));

    // Step 4: Apply nodejs preset
    let output = fixture.run_vm_command(&["config", "preset", "nodejs"])?;
    assert!(output.status.success());

    // Step 5: Verify nodejs-specific configuration was applied
    let config_content = fixture.read_file("vm.yaml")?;
    assert!(config_content.contains("npm_packages"));

    Ok(())
}

#[test]
#[cfg(feature = "integration")]
fn test_configuration_clear_workflow() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = WorkflowTestFixture::new()?;

    if !fixture.binary_path.exists() {
        println!("Skipping test - vm binary not found");
        return Ok(());
    }

    // Step 1: Set up some configuration
    let output = fixture.run_vm_command(&["config", "set", "vm.memory", "4096"])?;
    assert!(output.status.success());

    let output = fixture.run_vm_command(&["config", "set", "provider", "docker"])?;
    assert!(output.status.success());

    // Step 2: Verify configuration exists
    assert!(fixture.file_exists("vm.yaml"));

    // Step 3: Clear the configuration
    let output = fixture.run_vm_command(&["config", "clear"])?;
    if !output.status.success() {
        println!(
            "Command failed with stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        println!(
            "Command failed with stdout: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
    assert!(output.status.success());

    // Step 4: Verify file is gone
    assert!(!fixture.file_exists("vm.yaml"));

    // Step 5: Try to get configuration (should fail)
    let output = fixture.run_vm_command(&["config", "get", "vm.memory"])?;
    assert!(!output.status.success());

    Ok(())
}

#[test]
#[cfg(feature = "integration")]
fn test_help_system_workflow() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = WorkflowTestFixture::new()?;

    if !fixture.binary_path.exists() {
        println!("Skipping test - vm binary not found");
        return Ok(());
    }

    // Step 1: Test main config help
    let output = fixture.run_vm_command(&["config", "--help"])?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("config"));

    // Step 2: Test subcommand help
    let output = fixture.run_vm_command(&["config", "set", "--help"])?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("Change"));

    // Step 3: Test preset help
    let output = fixture.run_vm_command(&["config", "preset", "--help"])?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("preset"));

    Ok(())
}
