use anyhow::Result;
use assert_cmd::prelude::*; // For CommandCargoExt
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Test fixture for CLI integration tests
struct CliTestFixture {
    _temp_dir: TempDir,
    test_dir: PathBuf,
    binary_path: PathBuf,
}

impl CliTestFixture {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let test_dir = temp_dir.path().join("test_project");
        fs::create_dir_all(&test_dir)?;

        // Clean up any existing vm.yaml files in the temp directory hierarchy
        // to prevent interference with tests
        let temp_vm_yaml = temp_dir.path().join("vm.yaml");
        if temp_vm_yaml.exists() {
            let _ = fs::remove_file(temp_vm_yaml);
        }

        // Get the path to the vm binary using assert_cmd helper
        let binary_path = Command::cargo_bin("vm")?.get_program().into();

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
            .env("HOME", self.test_dir.parent().unwrap()) // Mock HOME for global config
            .env("VM_TOOL_DIR", &self.test_dir) // Point preset system to test directory
            .env("VM_TEST_MODE", "1") // Disable request span in test mode
            .output()?;
        Ok(output)
    }

    /// Get the contents of a file as a string
    fn read_file(&self, filename: &str) -> Result<String> {
        let path = self.test_dir.join(filename);
        Ok(fs::read_to_string(path)?)
    }

    /// Check if a file exists in the test directory
    fn file_exists(&self, filename: &str) -> bool {
        self.test_dir.join(filename).exists()
    }

    /// Get global config path (new unified location)
    fn global_config_path(&self) -> PathBuf {
        self.test_dir
            .parent()
            .unwrap()
            .join(".vm")
            .join("config.yaml")
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

#[cfg(test)]
mod cli_integration_tests {
    use super::*;

    #[test]
    fn test_config_set_and_get_local() -> Result<()> {
        let fixture = CliTestFixture::new()?;

        // Test setting a local config value
        let output = fixture.run_vm_command(&["config", "set", "vm.memory", "4096"])?;
        assert!(
            output.status.success(),
            "Failed to set config: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.contains("✅ Set vm.memory = 4096"));
        assert!(stdout.contains("vm.yaml"));

        // Verify file was created
        assert!(fixture.file_exists("vm.yaml"));

        // Test getting the value back
        let output = fixture.run_vm_command(&["config", "get", "vm.memory"])?;
        assert!(output.status.success());

        let stdout = String::from_utf8(output.stdout)?;
        assert_eq!(stdout.trim(), "4096");

        // Test getting all config
        let output = fixture.run_vm_command(&["config", "get"])?;
        assert!(output.status.success());

        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.contains("vm:"));
        assert!(stdout.contains("memory: 4096"));

        Ok(())
    }

    #[test]
    fn test_config_set_and_get_global() -> Result<()> {
        let fixture = CliTestFixture::new()?;

        // Test setting a global config value
        let output = fixture.run_vm_command(&["config", "set", "--global", "provider", "tart"])?;
        assert!(
            output.status.success(),
            "Failed to set global config: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.contains("✅ Set provider = tart"));

        // Verify global config file was created
        assert!(fixture.global_config_path().exists());

        // Test getting the global value back
        let output = fixture.run_vm_command(&["config", "get", "--global", "provider"])?;
        assert!(output.status.success());

        let stdout = String::from_utf8(output.stdout)?;
        assert_eq!(stdout.trim(), "tart");

        // Test setting another global value
        let output = fixture.run_vm_command(&["config", "set", "--global", "vm.cpus", "8"])?;
        assert!(output.status.success());

        // Test getting all global config
        let output = fixture.run_vm_command(&["config", "get", "--global"])?;
        assert!(output.status.success());

        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.contains("provider: tart"));
        assert!(stdout.contains("vm:"));
        assert!(stdout.contains("cpus: 8"));

        Ok(())
    }

    #[test]
    fn test_config_unset() -> Result<()> {
        let fixture = CliTestFixture::new()?;

        // Set up some config values
        fixture.run_vm_command(&["config", "set", "vm.memory", "4096"])?;
        fixture.run_vm_command(&["config", "set", "vm.cpus", "4"])?;
        fixture.run_vm_command(&["config", "set", "provider", "docker"])?;

        // Verify values exist
        let output = fixture.run_vm_command(&["config", "get", "vm.memory"])?;
        assert_eq!(String::from_utf8(output.stdout)?.trim(), "4096");

        // Unset a value
        let output = fixture.run_vm_command(&["config", "unset", "vm.memory"])?;
        assert!(output.status.success());

        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.contains("✅ Unset vm.memory"));

        // Verify value is gone but others remain
        let output = fixture.run_vm_command(&["config", "get"])?;
        let stdout = String::from_utf8(output.stdout)?;
        assert!(!stdout.contains("memory"));
        assert!(stdout.contains("cpus: 4"));
        assert!(stdout.contains("provider: docker"));

        Ok(())
    }

    // Note: test_config_clear removed as the `config clear` command was intentionally removed from the CLI

    #[test]
    fn test_config_preset_list_and_show() -> Result<()> {
        let fixture = CliTestFixture::new()?;

        // Create a test preset
        fixture.create_preset(
            "test-preset",
            r#"
services:
  redis:
    enabled: true
vm:
  memory: 2048
npm_packages:
  - eslint
"#,
        )?;

        // Test listing presets
        let output = fixture.run_vm_command(&["config", "preset", "--list"])?;
        assert!(output.status.success());

        let stdout = String::from_utf8(output.stdout)?;
        let stderr = String::from_utf8(output.stderr)?;
        eprintln!("STDOUT: {}", stdout);
        eprintln!("STDERR: {}", stderr);
        assert!(stdout.contains("Available presets:"));
        assert!(stdout.contains("test-preset"));

        // Test showing preset details
        let output = fixture.run_vm_command(&["config", "preset", "--show", "test-preset"])?;
        assert!(output.status.success());

        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.contains("Preset 'test-preset' configuration:"));
        assert!(stdout.contains("redis:"));
        assert!(stdout.contains("enabled: true"));
        assert!(stdout.contains("memory: 2048"));

        Ok(())
    }

    #[test]
    fn test_config_preset_application() -> Result<()> {
        let fixture = CliTestFixture::new()?;

        // Create a test preset
        fixture.create_preset(
            "test-preset",
            r#"
services:
  redis:
    enabled: true
  postgresql:
    enabled: true
vm:
  memory: 2048
npm_packages:
  - eslint
  - prettier
"#,
        )?;

        // Apply the preset
        let output = fixture.run_vm_command(&["config", "preset", "test-preset"])?;
        assert!(output.status.success());

        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.contains("✅ Applied preset 'test-preset' to local"));

        // Verify the preset was applied
        assert!(fixture.file_exists("vm.yaml"));

        let config_content = fixture.read_file("vm.yaml")?;
        assert!(config_content.contains("redis:"));
        assert!(config_content.contains("enabled: true"));
        assert!(config_content.contains("memory: 2048"));
        assert!(config_content.contains("eslint"));
        assert!(config_content.contains("prettier"));

        Ok(())
    }

    #[test]
    fn test_config_preset_composition() -> Result<()> {
        let fixture = CliTestFixture::new()?;

        // Create first preset
        fixture.create_preset(
            "preset1",
            r#"
services:
  redis:
    enabled: true
vm:
  memory: 2048
npm_packages:
  - eslint
"#,
        )?;

        // Create second preset
        fixture.create_preset(
            "preset2",
            r#"
services:
  postgresql:
    enabled: true
    port: 3000
vm:
  memory: 4096  # Should override preset1
  cpus: 4
npm_packages:
  - prettier
ports:
  _range: [3000, 3010]
"#,
        )?;

        // Apply both presets with comma separation
        let output = fixture.run_vm_command(&["config", "preset", "preset1,preset2"])?;
        assert!(output.status.success());

        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.contains("✅ Applied preset 'preset1,preset2' to local"));

        // Verify both presets were merged correctly
        let config_content = fixture.read_file("vm.yaml")?;

        // Memory should be from preset2 (later preset wins)
        assert!(config_content.contains("memory: 4096"));

        // Both services should be present
        assert!(config_content.contains("redis:"));
        assert!(config_content.contains("postgresql:"));

        // CPUs should be from preset2
        assert!(config_content.contains("cpus: 4"));

        // Port range should be from preset2
        assert!(config_content.contains("_range:"));

        // NPM packages should be from preset2 (arrays replace)
        assert!(config_content.contains("prettier"));

        Ok(())
    }

    #[test]
    fn test_config_global_preset_application() -> Result<()> {
        let fixture = CliTestFixture::new()?;

        // Create a test preset
        fixture.create_preset(
            "global-preset",
            r#"
provider: tart
vm:
  memory: 8192
  cpus: 4
services:
  docker:
    enabled: true
"#,
        )?;

        // Apply preset globally
        let output = fixture.run_vm_command(&["config", "preset", "--global", "global-preset"])?;
        assert!(output.status.success());

        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.contains("✅ Applied preset 'global-preset' to global"));

        // Verify global config was created
        assert!(fixture.global_config_path().exists());

        // Test getting global config
        let output = fixture.run_vm_command(&["config", "get", "--global"])?;
        assert!(output.status.success());

        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.contains("provider: tart"));
        assert!(stdout.contains("memory: 8192"));
        assert!(stdout.contains("cpus: 4"));
        assert!(stdout.contains("docker:"));

        Ok(())
    }

    #[test]
    fn test_config_error_handling() -> Result<()> {
        let fixture = CliTestFixture::new()?;

        // Test getting from non-existent local config
        let output = fixture.run_vm_command(&["config", "get", "vm.memory"])?;
        assert!(!output.status.success());

        let stderr = String::from_utf8(output.stderr)?;
        assert!(stderr.contains("Field 'memory' not found") || stderr.contains("No vm.yaml found"));

        // Test unsetting from non-existent config
        let output = fixture.run_vm_command(&["config", "unset", "vm.memory"])?;
        assert!(!output.status.success());

        // Test applying non-existent preset
        let output = fixture.run_vm_command(&["config", "preset", "nonexistent"])?;
        assert!(!output.status.success());

        let stderr = String::from_utf8(output.stderr)?;
        assert!(stderr.contains("not found"));

        Ok(())
    }

    #[test]
    fn test_config_dot_notation() -> Result<()> {
        let fixture = CliTestFixture::new()?;

        // Test setting deeply nested values
        fixture.run_vm_command(&["config", "set", "services.postgresql.version", "15"])?;
        fixture.run_vm_command(&["config", "set", "services.postgresql.enabled", "true"])?;
        fixture.run_vm_command(&["config", "set", "services.postgresql.port", "5432"])?;
        fixture.run_vm_command(&["config", "set", "services.redis.enabled", "true"])?;

        // Verify the nested structure
        let output = fixture.run_vm_command(&["config", "get"])?;
        let stdout = String::from_utf8(output.stdout)?;

        assert!(stdout.contains("services:"));
        assert!(stdout.contains("postgresql:"));
        assert!(stdout.contains("version:") && stdout.contains("15"));
        assert!(stdout.contains("port: 5432"));
        assert!(stdout.contains("redis:"));
        assert!(stdout.contains("enabled: true"));

        // Test getting specific nested value
        let output = fixture.run_vm_command(&["config", "get", "services.postgresql.version"])?;
        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout)?;
        assert_eq!(stdout.trim(), "15");

        Ok(())
    }

    #[test]
    fn test_config_help_commands_work() -> Result<()> {
        let fixture = CliTestFixture::new()?;

        // Just test that help commands run successfully and produce output
        // Avoid fragile text matching that breaks with minor CLI changes

        // Test main config help
        let output = fixture.run_vm_command(&["config", "--help"])?;
        assert!(output.status.success());
        assert!(
            !output.stdout.is_empty(),
            "Config help should produce output"
        );

        // Test set subcommand help
        let output = fixture.run_vm_command(&["config", "set", "--help"])?;
        assert!(output.status.success());
        assert!(
            !output.stdout.is_empty(),
            "Config set help should produce output"
        );

        Ok(())
    }

    // Note: test_config_ports_basic removed - too fragile due to Docker dependencies
    // and environment setup. Port functionality is covered by unit tests in vm-config.

    // Note: test_config_ports_fix_no_conflicts removed - too fragile due to Docker dependencies
    // and external process requirements. VM operations integration tests properly cover Docker functionality.

    // Note: test_config_ports_help removed - low value help text testing.
    // Main help functionality is covered by test_config_help_messages.
}
