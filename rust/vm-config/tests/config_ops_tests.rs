use serde_yaml_ng as serde_yaml;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tempfile::TempDir;
use vm_config::config::VmConfig;
use vm_config::ConfigOps;
use vm_core::error::Result;

// Global mutex to ensure tests run sequentially to avoid environment variable conflicts
static TEST_MUTEX: Mutex<()> = Mutex::new(());

/// Simple test fixture that sets up a temporary directory
struct SimpleTestFixture {
    _temp_dir: TempDir,
    test_dir: PathBuf,
    original_home: Option<String>,
    original_vm_tool_dir: Option<String>,
}

impl SimpleTestFixture {
    fn new() -> Result<Self> {
        let _ = fs::remove_file("/tmp/vm.yaml");

        let temp_dir = TempDir::new()?;
        let test_dir = temp_dir.path().to_path_buf();

        // Save original environment variables
        let original_home = std::env::var("HOME").ok();
        let original_vm_tool_dir = std::env::var("VM_TOOL_DIR").ok();

        // Set environment variables to use our temp directory
        std::env::set_var("HOME", &test_dir);
        let tool_dir = test_dir.join("vm-tool");
        fs::create_dir_all(tool_dir.join("configs").join("presets"))?;
        std::env::set_var("VM_TOOL_DIR", &tool_dir);

        Ok(Self {
            _temp_dir: temp_dir,
            test_dir,
            original_home,
            original_vm_tool_dir,
        })
    }

    fn set_working_dir(&self) -> Result<()> {
        std::env::set_current_dir(&self.test_dir)?;
        Ok(())
    }

    fn create_preset(name: &str, content: &str) -> Result<()> {
        let tool_dir = std::env::var("VM_TOOL_DIR")
            .expect("VM_TOOL_DIR environment variable not set - test fixture setup failed");
        let presets_dir = PathBuf::from(tool_dir).join("configs").join("presets");
        fs::create_dir_all(&presets_dir)?;
        let preset_path = presets_dir.join(format!("{}.yaml", name));
        fs::write(&preset_path, content)?;
        Ok(())
    }
}

impl Drop for SimpleTestFixture {
    fn drop(&mut self) {
        // Restore original environment variables
        match &self.original_home {
            Some(home) => std::env::set_var("HOME", home),
            None => std::env::remove_var("HOME"),
        }
        match &self.original_vm_tool_dir {
            Some(vm_tool_dir) => std::env::set_var("VM_TOOL_DIR", vm_tool_dir),
            None => std::env::remove_var("VM_TOOL_DIR"),
        }
    }
}

#[cfg(test)]
mod config_ops_tests {
    use super::*;

    #[test]
    fn test_local_config_set_and_get() -> Result<()> {
        let _guard = TEST_MUTEX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let fixture = SimpleTestFixture::new()?;
        fixture.set_working_dir()?;

        // Test setting a simple value
        ConfigOps::set("vm.memory", &["4096".to_string()], false, false)?;

        // Verify the file was created in current directory
        let config_path = std::env::current_dir()?.join("vm.yaml");
        assert!(config_path.exists());

        // Test getting the value back by reading the config file directly
        let config_content = fs::read_to_string(&config_path)?;
        let config: VmConfig = serde_yaml::from_str(&config_content)?;
        assert_eq!(
            config
                .vm
                .as_ref()
                .and_then(|v| v.memory.as_ref().and_then(|m| m.to_mb())),
            Some(4096)
        );

        // Test setting a nested value
        ConfigOps::set(
            "services.docker.enabled",
            &["true".to_string()],
            false,
            false,
        )?;

        // Verify the nested structure
        let config_content = fs::read_to_string(&config_path)?;
        let config: VmConfig = serde_yaml::from_str(&config_content)?;

        assert_eq!(
            config
                .vm
                .as_ref()
                .and_then(|v| v.memory.as_ref().and_then(|m| m.to_mb())),
            Some(4096)
        );
        assert_eq!(
            config
                .services
                .get("docker")
                .and_then(|s| s.enabled.then_some(true)),
            Some(true)
        );

        Ok(())
    }

    #[test]
    fn test_global_config_operations() -> Result<()> {
        let _guard = TEST_MUTEX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let fixture = SimpleTestFixture::new()?;
        fixture.set_working_dir()?;

        // Test setting global config
        ConfigOps::set("provider", &["tart".to_string()], true, false)?;
        ConfigOps::set("vm.cpus", &["8".to_string()], true, false)?;

        // Verify global config file was created
        let global_config_path = fixture.test_dir.join(".vm").join("config.yaml");
        assert!(global_config_path.exists());

        // Test getting global config
        let config_content = fs::read_to_string(&global_config_path)?;
        let config: VmConfig = serde_yaml::from_str(&config_content)?;

        assert_eq!(config.provider.as_deref(), Some("tart"));
        assert_eq!(
            config.vm.as_ref().and_then(|v| v.cpus.as_ref()),
            Some(&vm_config::config::CpuLimit::Limited(8))
        );

        // Test unsetting a global value
        ConfigOps::unset("vm.cpus", true)?;

        let updated_content = fs::read_to_string(&global_config_path)?;
        let updated_config: VmConfig = serde_yaml::from_str(&updated_content)?;

        assert_eq!(updated_config.provider.as_deref(), Some("tart"));
        assert_eq!(
            updated_config.vm.as_ref().and_then(|v| v.cpus.as_ref()),
            None
        );

        Ok(())
    }

    // Note: test_config_clear removed as ConfigOps::clear() was removed
    // Users should manually delete config files if needed

    #[test]
    fn test_preset_application() -> Result<()> {
        let _guard = TEST_MUTEX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let fixture = SimpleTestFixture::new()?;
        fixture.set_working_dir()?;

        // Create a test preset
        let test_preset = r#"
preset:
  name: test-preset
  description: Test preset for unit tests
services:
  redis:
    enabled: true
vm:
  memory: 2048
"#;
        SimpleTestFixture::create_preset("test-preset", test_preset)?;

        // Apply the preset locally
        ConfigOps::preset("test-preset", false, false, None)?;

        // Verify preset was applied
        let config_path = std::env::current_dir()?.join("vm.yaml");
        assert!(config_path.exists());
        let config_content = fs::read_to_string(&config_path)?;
        let config: VmConfig = serde_yaml::from_str(&config_content)?;

        assert_eq!(
            config
                .vm
                .as_ref()
                .and_then(|v| v.memory.as_ref().and_then(|m| m.to_mb())),
            Some(2048)
        );
        assert_eq!(
            config
                .services
                .get("redis")
                .and_then(|s| s.enabled.then_some(true)),
            Some(true)
        );

        Ok(())
    }

    #[test]
    fn test_dot_notation_access() -> Result<()> {
        let _guard = TEST_MUTEX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let fixture = SimpleTestFixture::new()?;
        fixture.set_working_dir()?;

        // Test deep nested setting
        ConfigOps::set(
            "services.postgresql.version",
            &["15".to_string()],
            false,
            false,
        )?;
        ConfigOps::set(
            "services.postgresql.port",
            &["5432".to_string()],
            false,
            false,
        )?;
        ConfigOps::set(
            "services.redis.enabled",
            &["true".to_string()],
            false,
            false,
        )?;

        // Verify nested structure was created correctly
        let config_path = std::env::current_dir()?.join("vm.yaml");
        let config_content = fs::read_to_string(&config_path)?;
        let config: VmConfig = serde_yaml::from_str(&config_content)?;

        let postgresql = config
            .services
            .get("postgresql")
            .expect("postgresql service not found in config - preset loading may have failed");
        assert_eq!(postgresql.version.as_deref(), Some("15"));
        assert_eq!(postgresql.port, Some(5432));

        let redis = config
            .services
            .get("redis")
            .expect("redis service not found in config - preset loading may have failed");
        assert!(redis.enabled);

        // Test unsetting nested values
        ConfigOps::unset("services.postgresql.version", false)?;

        let updated_content = fs::read_to_string(&config_path)?;
        let updated_config: VmConfig = serde_yaml::from_str(&updated_content)?;

        let updated_postgresql = updated_config.services.get("postgresql").expect(
            "postgresql service not found in updated config - config modification may have failed",
        );
        assert_eq!(updated_postgresql.version, None);
        assert_eq!(updated_postgresql.port, Some(5432)); // Should still exist

        Ok(())
    }

    #[test]
    fn test_error_handling() -> Result<()> {
        let _guard = TEST_MUTEX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let fixture = SimpleTestFixture::new()?;
        fixture.set_working_dir()?;

        // Test getting from non-existent local config
        let result = ConfigOps::get(Some("vm.memory"), false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No vm.yaml configuration found"));

        // Test unsetting from non-existent config
        let result = ConfigOps::unset("vm.memory", false);
        assert!(result.is_err());

        // Test applying non-existent preset
        let result = ConfigOps::preset("nonexistent-preset", false, false, None);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Failed to load preset") || err_msg.contains("not found"),
            "Expected error message about failed preset loading, got: {}",
            err_msg
        );

        Ok(())
    }
}
