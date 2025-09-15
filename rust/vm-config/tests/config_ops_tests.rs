use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use vm_config::config_ops::ConfigOps;
use vm_config::config::VmConfig;

/// Test fixture that sets up temporary directories for testing
struct ConfigTestFixture {
    _temp_dir: TempDir,
    local_config_dir: PathBuf,
    global_config_dir: PathBuf,
    global_config_path: PathBuf,
    local_config_path: PathBuf,
}

impl ConfigTestFixture {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let local_config_dir = temp_dir.path().join("project");
        let global_config_dir = temp_dir.path().join("global_config");

        fs::create_dir_all(&local_config_dir)?;
        fs::create_dir_all(&global_config_dir)?;

        let global_config_path = global_config_dir.join("global.yaml");
        let local_config_path = local_config_dir.join("vm.yaml");

        Ok(Self {
            _temp_dir: temp_dir,
            local_config_dir,
            global_config_dir,
            global_config_path,
            local_config_path,
        })
    }

    /// Set working directory to the local config directory
    fn set_working_dir(&self) -> Result<()> {
        std::env::set_current_dir(&self.local_config_dir)?;
        Ok(())
    }

    /// Mock the global config directory by setting the HOME environment variable
    fn mock_home_dir(&self) -> Result<()> {
        let mock_home = self.global_config_dir.parent().unwrap();
        std::env::set_var("HOME", mock_home);
        Ok(())
    }

    /// Create a test preset file for testing preset functionality
    fn create_test_preset(&self, name: &str, content: &str) -> Result<PathBuf> {
        let presets_dir = self.local_config_dir.join("configs").join("presets");
        fs::create_dir_all(&presets_dir)?;
        let preset_path = presets_dir.join(format!("{}.yaml", name));
        fs::write(&preset_path, content)?;
        Ok(preset_path)
    }
}

mod config_ops_tests {
    use super::*;

    #[test]
    fn test_local_config_set_and_get() -> Result<()> {
        let fixture = ConfigTestFixture::new()?;
        fixture.set_working_dir()?;

        // Test setting a simple value
        ConfigOps::set("vm.memory", "4096", false)?;

        // Verify the file was created
        assert!(fixture.local_config_path.exists());

        // Test getting the value back
        let output = std::process::Command::new("cargo")
            .args(&["run", "--bin", "vm-config", "--", "query", "vm.yaml", "vm.memory", "--raw"])
            .current_dir(&fixture.local_config_dir.parent().unwrap())
            .output()?;

        let result = String::from_utf8(output.stdout)?.trim().to_string();
        assert_eq!(result, "4096");

        // Test setting a nested value
        ConfigOps::set("services.docker.enabled", "true", false)?;

        // Verify the nested structure
        let config_content = fs::read_to_string(&fixture.local_config_path)?;
        let config: VmConfig = serde_yaml::from_str(&config_content)?;

        assert_eq!(config.vm.as_ref().and_then(|v| v.memory), Some(4096));
        assert_eq!(
            config.services.get("docker")
                .and_then(|s| s.enabled.then_some(true)),
            Some(true)
        );

        Ok(())
    }

    #[test]
    fn test_global_config_operations() -> Result<()> {
        let fixture = ConfigTestFixture::new()?;
        fixture.set_working_dir()?;
        fixture.mock_home_dir()?;

        // Test setting global config
        ConfigOps::set("provider", "tart", true)?;
        ConfigOps::set("vm.cpus", "8", true)?;

        // Verify global config file was created at expected location
        let expected_global_path = fixture.global_config_dir.parent().unwrap()
            .join(".config").join("vm").join("global.yaml");
        assert!(expected_global_path.exists());

        // Test getting global config
        let config_content = fs::read_to_string(&expected_global_path)?;
        let config: VmConfig = serde_yaml::from_str(&config_content)?;

        assert_eq!(config.provider.as_deref(), Some("tart"));
        assert_eq!(config.vm.as_ref().and_then(|v| v.cpus), Some(8));

        // Test unsetting a global value
        ConfigOps::unset("vm.cpus", true)?;

        let updated_content = fs::read_to_string(&expected_global_path)?;
        let updated_config: VmConfig = serde_yaml::from_str(&updated_content)?;

        assert_eq!(updated_config.provider.as_deref(), Some("tart"));
        assert_eq!(updated_config.vm.as_ref().and_then(|v| v.cpus), None);

        Ok(())
    }

    #[test]
    fn test_config_clear() -> Result<()> {
        let fixture = ConfigTestFixture::new()?;
        fixture.set_working_dir()?;

        // Create a local config file
        ConfigOps::set("vm.memory", "2048", false)?;
        ConfigOps::set("provider", "docker", false)?;

        assert!(fixture.local_config_path.exists());

        // Clear the config
        ConfigOps::clear(false)?;

        // Verify file was removed
        assert!(!fixture.local_config_path.exists());

        Ok(())
    }

    #[test]
    fn test_preset_application() -> Result<()> {
        let fixture = ConfigTestFixture::new()?;
        fixture.set_working_dir()?;

        // Create a test preset
        let test_preset = r#"
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
"#;
        fixture.create_test_preset("test-preset", test_preset)?;

        // Apply the preset locally
        ConfigOps::preset("test-preset", false, false, None)?;

        // Verify preset was applied
        assert!(fixture.local_config_path.exists());
        let config_content = fs::read_to_string(&fixture.local_config_path)?;
        let config: VmConfig = serde_yaml::from_str(&config_content)?;

        assert_eq!(config.vm.as_ref().and_then(|v| v.memory), Some(2048));
        assert_eq!(
            config.services.get("redis")
                .and_then(|s| s.enabled.then_some(true)),
            Some(true)
        );
        assert!(config.npm_packages.contains(&"eslint".to_string()));

        Ok(())
    }

    #[test]
    fn test_multiple_preset_composition() -> Result<()> {
        let fixture = ConfigTestFixture::new()?;
        fixture.set_working_dir()?;

        // Create first preset
        let preset1 = r#"
services:
  redis:
    enabled: true
vm:
  memory: 2048
npm_packages:
  - eslint
"#;
        fixture.create_test_preset("preset1", preset1)?;

        // Create second preset
        let preset2 = r#"
services:
  postgresql:
    enabled: true
vm:
  memory: 4096  # This should override the first preset
npm_packages:
  - prettier
ports:
  web: 3000
"#;
        fixture.create_test_preset("preset2", preset2)?;

        // Apply both presets with comma separation
        ConfigOps::preset("preset1,preset2", false, false, None)?;

        // Verify both presets were merged correctly
        let config_content = fs::read_to_string(&fixture.local_config_path)?;
        let config: VmConfig = serde_yaml::from_str(&config_content)?;

        // Memory should be from preset2 (last wins)
        assert_eq!(config.vm.as_ref().and_then(|v| v.memory), Some(4096));

        // Services should be merged
        assert_eq!(
            config.services.get("redis")
                .and_then(|s| s.enabled.then_some(true)),
            Some(true)
        );
        assert_eq!(
            config.services.get("postgresql")
                .and_then(|s| s.enabled.then_some(true)),
            Some(true)
        );

        // NPM packages should be merged (arrays replace, not merge)
        assert!(config.npm_packages.contains(&"prettier".to_string()));

        // Ports should be from preset2
        assert_eq!(config.ports.get("web"), Some(&3000));

        Ok(())
    }

    #[test]
    fn test_config_merge_chain() -> Result<()> {
        let fixture = ConfigTestFixture::new()?;
        fixture.set_working_dir()?;
        fixture.mock_home_dir()?;

        // Set up global config
        ConfigOps::set("provider", "vagrant", true)?;
        ConfigOps::set("vm.memory", "8192", true)?;
        ConfigOps::set("vm.cpus", "4", true)?;

        // Create a preset that overrides some values
        let preset_content = r#"
provider: docker  # This should override global
vm:
  memory: 4096    # This should override global
  swap: 2048      # This is new
services:
  redis:
    enabled: true
"#;
        fixture.create_test_preset("override-preset", preset_content)?;

        // Apply preset locally (this merges global -> preset)
        ConfigOps::preset("override-preset", false, false, None)?;

        // Set local override
        ConfigOps::set("vm.memory", "16384", false)?;  // Local override
        ConfigOps::set("vm.user", "developer", false)?;  // Local addition

        // Load and verify the final merged configuration
        let config_content = fs::read_to_string(&fixture.local_config_path)?;
        let config: VmConfig = serde_yaml::from_str(&config_content)?;

        // Verify merge precedence: local > preset > global > defaults
        assert_eq!(config.provider.as_deref(), Some("docker"));  // From preset (overrides global)
        assert_eq!(config.vm.as_ref().and_then(|v| v.memory), Some(16384));  // From local (final override)
        assert_eq!(config.vm.as_ref().and_then(|v| v.cpus), Some(4));  // From global (not overridden)
        assert_eq!(config.vm.as_ref().and_then(|v| v.swap), Some(2048));  // From preset
        assert_eq!(config.vm.as_ref().and_then(|v| v.user.as_deref()), Some("developer"));  // From local
        assert_eq!(
            config.services.get("redis")
                .and_then(|s| s.enabled.then_some(true)),
            Some(true)
        );  // From preset

        Ok(())
    }

    #[test]
    fn test_dot_notation_access() -> Result<()> {
        let fixture = ConfigTestFixture::new()?;
        fixture.set_working_dir()?;

        // Test deep nested setting
        ConfigOps::set("services.postgresql.version", "15", false)?;
        ConfigOps::set("services.postgresql.port", "5432", false)?;
        ConfigOps::set("services.redis.enabled", "true", false)?;

        // Verify nested structure was created correctly
        let config_content = fs::read_to_string(&fixture.local_config_path)?;
        let config: VmConfig = serde_yaml::from_str(&config_content)?;

        let postgresql = config.services.get("postgresql").unwrap();
        assert_eq!(postgresql.version.as_deref(), Some("15"));
        assert_eq!(postgresql.port, Some(5432));

        let redis = config.services.get("redis").unwrap();
        assert_eq!(redis.enabled, true);

        // Test unsetting nested values
        ConfigOps::unset("services.postgresql.version", false)?;

        let updated_content = fs::read_to_string(&fixture.local_config_path)?;
        let updated_config: VmConfig = serde_yaml::from_str(&updated_content)?;

        let updated_postgresql = updated_config.services.get("postgresql").unwrap();
        assert_eq!(updated_postgresql.version, None);
        assert_eq!(updated_postgresql.port, Some(5432));  // Should still exist

        Ok(())
    }

    #[test]
    fn test_error_handling() -> Result<()> {
        let fixture = ConfigTestFixture::new()?;
        fixture.set_working_dir()?;

        // Test getting from non-existent local config
        let result = ConfigOps::get(Some("vm.memory"), false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No vm.yaml found"));

        // Test unsetting from non-existent config
        let result = ConfigOps::unset("vm.memory", false);
        assert!(result.is_err());

        // Test applying non-existent preset
        let result = ConfigOps::preset("nonexistent-preset", false, false, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));

        Ok(())
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use vm_config::merge;

    #[test]
    fn test_full_config_loading_with_global() -> Result<()> {
        let fixture = ConfigTestFixture::new()?;
        fixture.set_working_dir()?;
        fixture.mock_home_dir()?;

        // Setup: Create a minimal defaults config
        let defaults = VmConfig {
            version: Some("1.0".to_string()),
            provider: Some("docker".to_string()),
            vm: Some(vm_config::config::VmSettings {
                memory: Some(2048),
                cpus: Some(2),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Setup: Create global config
        ConfigOps::set("vm.memory", "8192", true)?;
        ConfigOps::set("provider", "vagrant", true)?;
        let global_config = vm_config::config_ops::load_global_config();

        // Setup: Create preset
        let preset_content = r#"
vm:
  cpus: 4
  swap: 2048
services:
  redis:
    enabled: true
"#;
        fixture.create_test_preset("test-preset", preset_content)?;
        let preset_config = Some(VmConfig::from_file(&fixture.create_test_preset("test-preset", preset_content)?)?);

        // Setup: Create local config
        ConfigOps::set("vm.memory", "16384", false)?;  // Override everything
        ConfigOps::set("project.name", "test-project", false)?;
        let user_config = Some(VmConfig::from_file(&fixture.local_config_path)?);

        // Test the full merge chain
        let merged = merge::merge_configs(
            Some(defaults),
            global_config,
            preset_config,
            user_config,
        )?;

        // Verify final result follows precedence: defaults < global < preset < local
        assert_eq!(merged.version.as_deref(), Some("1.0"));  // From defaults
        assert_eq!(merged.provider.as_deref(), Some("vagrant"));  // From global (overrides defaults)
        assert_eq!(merged.vm.as_ref().and_then(|v| v.memory), Some(16384));  // From local (final override)
        assert_eq!(merged.vm.as_ref().and_then(|v| v.cpus), Some(4));  // From preset (overrides global)
        assert_eq!(merged.vm.as_ref().and_then(|v| v.swap), Some(2048));  // From preset (new)
        assert_eq!(merged.project.as_ref().and_then(|p| p.name.as_deref()), Some("test-project"));  // From local (new)
        assert_eq!(
            merged.services.get("redis")
                .and_then(|s| s.enabled.then_some(true)),
            Some(true)
        );  // From preset

        Ok(())
    }
}