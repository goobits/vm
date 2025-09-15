use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use vm_config::{VmConfig, ConfigOps, preset::PresetDetector};
use vm_detector::FrameworkDetector;
use vm_ports::{PortRegistry, PortRange};
use vm_temp::{StateManager, TempVmState};

/// Cross-crate integration test fixture
struct CrossCrateTestFixture {
    _temp_dir: TempDir,
    test_dir: PathBuf,
    project_dir: PathBuf,
    config_dir: PathBuf,
    original_home: Option<String>,
    original_vm_tool_dir: Option<String>,
}

impl CrossCrateTestFixture {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let test_dir = temp_dir.path().to_path_buf();
        let project_dir = test_dir.join("project");
        let config_dir = test_dir.join("configs");

        fs::create_dir_all(&project_dir)?;
        fs::create_dir_all(&config_dir.join("presets"))?;

        // Save and set environment variables
        let original_home = std::env::var("HOME").ok();
        let original_vm_tool_dir = std::env::var("VM_TOOL_DIR").ok();

        std::env::set_var("HOME", &test_dir);
        std::env::set_var("VM_TOOL_DIR", &test_dir);

        Ok(Self {
            _temp_dir: temp_dir,
            test_dir,
            project_dir,
            config_dir,
            original_home,
            original_vm_tool_dir,
        })
    }

    fn create_project_file(&self, path: &str, content: &str) -> Result<()> {
        let full_path = self.project_dir.join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(full_path, content)?;
        Ok(())
    }

    fn create_preset(&self, name: &str, content: &str) -> Result<()> {
        let preset_path = self.config_dir.join("presets").join(format!("{}.yaml", name));
        let full_content = format!(
            "---\npreset:\n  name: {}\n  description: \"Test preset\"\n\n{}",
            name, content
        );
        fs::write(preset_path, full_content)?;
        Ok(())
    }

    fn set_working_dir(&self) -> Result<()> {
        std::env::set_current_dir(&self.project_dir)?;
        Ok(())
    }
}

impl Drop for CrossCrateTestFixture {
    fn drop(&mut self) {
        // Restore environment variables
        if let Some(home) = &self.original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }

        if let Some(tool_dir) = &self.original_vm_tool_dir {
            std::env::set_var("VM_TOOL_DIR", tool_dir);
        } else {
            std::env::remove_var("VM_TOOL_DIR");
        }
    }
}

#[test]
fn test_detector_config_integration() -> Result<()> {
    let fixture = CrossCrateTestFixture::new()?;

    // Step 1: Create a Django project
    fixture.create_project_file("manage.py", "#!/usr/bin/env python\nimport os\nimport sys")?;
    fixture.create_project_file("requirements.txt", "Django>=4.0\npsycopg2>=2.8")?;

    // Step 2: Use detector to identify project type
    let detector = FrameworkDetector::new(&fixture.project_dir);
    let detected_frameworks = detector.detect_all_frameworks()?;

    assert!(!detected_frameworks.is_empty());

    // Step 3: Create corresponding preset
    fixture.create_preset("django", r#"
services:
  postgresql:
    enabled: true
    version: 15
  redis:
    enabled: true
vm:
  memory: 4096
pip_packages:
  - django
  - psycopg2
"#)?;

    // Step 4: Use preset detector to find and load preset
    let preset_detector = PresetDetector::new(fixture.project_dir.clone(), fixture.config_dir.join("presets"));
    let detected_preset = preset_detector.detect()?;

    assert_eq!(detected_preset, Some("django".to_string()));

    // Step 5: Load the preset configuration
    let preset_config = preset_detector.load_preset("django")?;

    // Step 6: Verify preset contains expected configuration
    assert!(preset_config.services.is_some());
    if let Some(services) = &preset_config.services {
        assert!(services.contains_key("postgresql"));
        assert!(services.contains_key("redis"));
    }

    Ok(())
}

#[test]
fn test_config_ports_integration() -> Result<()> {
    let fixture = CrossCrateTestFixture::new()?;
    fixture.set_working_dir()?;

    // Step 1: Set up configuration with port requirements
    let output = ConfigOps::set("ports.web", "3000", false)?;
    assert!(output.contains("✅"));

    let output = ConfigOps::set("ports.api", "8080", false)?;
    assert!(output.contains("✅"));

    // Step 2: Load the configuration
    let config = VmConfig::load(None, false)?;

    // Step 3: Extract port information and test with port registry
    let mut registry = PortRegistry::new();

    if let Some(ports) = &config.ports {
        for (name, port_config) in ports {
            let port = match port_config {
                serde_yaml::Value::Number(n) => n.as_u64().unwrap() as u16,
                serde_yaml::Value::String(s) => s.parse().unwrap(),
                _ => continue,
            };

            // Step 4: Register port ranges around configured ports
            let range = PortRange::new(port, port)?;
            registry.register_range(name.clone(), range)?;
        }
    }

    // Step 5: Test conflict detection
    let conflicting_range = PortRange::new(3000, 3000)?;
    let conflicts = registry.check_conflicts(&conflicting_range);
    assert!(!conflicts.is_empty());

    // Step 6: Test port suggestion
    let suggested = registry.suggest_next_range(80, 90)?;
    assert!(suggested.start() >= 80);
    assert!(suggested.end() <= 90);

    Ok(())
}

#[test]
fn test_config_temp_integration() -> Result<()> {
    let fixture = CrossCrateTestFixture::new()?;
    fixture.set_working_dir()?;

    // Step 1: Set up configuration
    let output = ConfigOps::set("vm.memory", "4096", false)?;
    assert!(output.contains("✅"));

    let output = ConfigOps::set("provider", "docker", false)?;
    assert!(output.contains("✅"));

    // Step 2: Load configuration
    let config = VmConfig::load(None, false)?;

    // Step 3: Create temp state based on configuration
    let state_manager = StateManager::with_state_dir(fixture.test_dir.join("vm_state"));
    let temp_state = TempVmState::new(
        "test-container".to_string(),
        config.provider.clone(),
        fixture.project_dir.clone(),
        false
    );

    // Step 4: Save and load state
    state_manager.save_state(&temp_state)?;
    let loaded_state = state_manager.load_state("test-container")?;

    // Step 5: Verify state consistency with configuration
    assert_eq!(loaded_state.provider(), &config.provider);
    assert_eq!(loaded_state.project_path(), &fixture.project_dir);

    // Step 6: Test state cleanup
    state_manager.cleanup_state("test-container")?;
    let cleanup_result = state_manager.load_state("test-container");
    assert!(cleanup_result.is_err());

    Ok(())
}

#[test]
fn test_preset_detector_integration() -> Result<()> {
    let fixture = CrossCrateTestFixture::new()?;

    // Step 1: Create a React project
    fixture.create_project_file("package.json", r#"{
        "name": "test-react-app",
        "version": "1.0.0",
        "dependencies": {
            "react": "^18.0.0",
            "react-dom": "^18.0.0"
        },
        "scripts": {
            "start": "react-scripts start"
        }
    }"#)?;

    // Step 2: Use framework detector
    let framework_detector = FrameworkDetector::new(&fixture.project_dir);
    let frameworks = framework_detector.detect_all_frameworks()?;

    // Should detect React
    let react_detected = frameworks.iter().any(|f| f.framework_type.contains("React"));
    assert!(react_detected, "React framework should be detected");

    // Step 3: Create React preset
    fixture.create_preset("react", r#"
npm_packages:
  - react-scripts
  - eslint
  - prettier
services:
  redis:
    enabled: false
vm:
  memory: 2048
  cpus: 2
ports:
  dev: 3000
"#)?;

    // Step 4: Use preset detector for auto-detection
    let preset_detector = PresetDetector::new(fixture.project_dir.clone(), fixture.config_dir.join("presets"));
    let detected_preset = preset_detector.detect()?;

    assert_eq!(detected_preset, Some("react".to_string()));

    // Step 5: Apply preset through ConfigOps
    fixture.set_working_dir()?;
    ConfigOps::preset("react", false, false, None)?;

    // Step 6: Verify configuration was applied
    let config = VmConfig::load(None, false)?;
    assert!(config.npm_packages.is_some());
    assert!(config.ports.is_some());

    if let Some(vm) = config.vm {
        assert_eq!(vm.memory, Some(2048));
        assert_eq!(vm.cpus, Some(2));
    }

    Ok(())
}

#[test]
fn test_multi_framework_detection_and_config() -> Result<()> {
    let fixture = CrossCrateTestFixture::new()?;

    // Step 1: Create a project with multiple frameworks
    fixture.create_project_file("package.json", r#"{
        "name": "fullstack-app",
        "dependencies": {
            "express": "^4.18.0",
            "react": "^18.0.0"
        }
    }"#)?;

    fixture.create_project_file("requirements.txt", "fastapi>=0.68.0\nuvicorn>=0.15.0")?;

    fixture.create_project_file("docker-compose.yml", r#"version: '3.8'
services:
  web:
    build: .
    ports:
      - "3000:3000"
  api:
    build: ./api
    ports:
      - "8000:8000"
"#)?;

    // Step 2: Detect all frameworks
    let detector = FrameworkDetector::new(&fixture.project_dir);
    let frameworks = detector.detect_all_frameworks()?;

    // Should detect multiple frameworks
    assert!(frameworks.len() > 1, "Should detect multiple frameworks");

    // Step 3: Create presets for different aspects
    fixture.create_preset("nodejs", r#"
npm_packages:
  - express
  - cors
ports:
  web: 3000
"#)?;

    fixture.create_preset("python", r#"
pip_packages:
  - fastapi
  - uvicorn
ports:
  api: 8000
"#)?;

    fixture.create_preset("docker", r#"
services:
  postgresql:
    enabled: true
  redis:
    enabled: true
vm:
  memory: 8192
"#)?;

    // Step 4: Apply multiple presets in composition
    fixture.set_working_dir()?;
    ConfigOps::preset("nodejs,python,docker", false, false, None)?;

    // Step 5: Verify merged configuration
    let config = VmConfig::load(None, false)?;

    // Should have npm packages from nodejs preset
    assert!(config.npm_packages.is_some());
    if let Some(npm_packages) = &config.npm_packages {
        assert!(npm_packages.contains(&"express".to_string()));
    }

    // Should have pip packages from python preset
    assert!(config.pip_packages.is_some());
    if let Some(pip_packages) = &config.pip_packages {
        assert!(pip_packages.contains(&"fastapi".to_string()));
    }

    // Should have services from docker preset
    assert!(config.services.is_some());

    // Should have ports from both presets
    assert!(config.ports.is_some());

    Ok(())
}

#[test]
fn test_configuration_inheritance_chain() -> Result<()> {
    let fixture = CrossCrateTestFixture::new()?;
    fixture.set_working_dir()?;

    // Step 1: Set global configuration
    let output = ConfigOps::set("vm.cpus", "8", true)?;
    assert!(output.contains("✅"));

    let output = ConfigOps::set("provider", "tart", true)?;
    assert!(output.contains("✅"));

    // Step 2: Create and apply preset
    fixture.create_preset("test-preset", r#"
vm:
  memory: 4096
  cpus: 4  # Should override global
services:
  redis:
    enabled: true
npm_packages:
  - lodash
"#)?;

    ConfigOps::preset("test-preset", false, false, None)?;

    // Step 3: Set local overrides
    let output = ConfigOps::set("vm.memory", "8192", false)?;
    assert!(output.contains("✅"));

    let output = ConfigOps::set("provider", "docker", false)?;
    assert!(output.contains("✅"));

    // Step 4: Test final merged configuration
    let config = VmConfig::load(None, false)?;

    // Local should override everything
    assert_eq!(config.provider, "docker");

    if let Some(vm) = config.vm {
        // Memory: local override wins
        assert_eq!(vm.memory, Some(8192));
        // CPUs: preset override wins over global
        assert_eq!(vm.cpus, Some(4));
    }

    // Services from preset should be present
    assert!(config.services.is_some());

    // NPM packages from preset should be present
    assert!(config.npm_packages.is_some());

    Ok(())
}

#[test]
fn test_state_persistence_across_operations() -> Result<()> {
    let fixture = CrossCrateTestFixture::new()?;
    fixture.set_working_dir()?;

    // Step 1: Set up initial configuration
    ConfigOps::set("vm.memory", "4096", false)?;
    ConfigOps::set("provider", "docker", false)?;

    // Step 2: Create temp state
    let state_manager = StateManager::with_state_dir(fixture.test_dir.join("vm_state"));
    let initial_state = TempVmState::new(
        "persistent-test".to_string(),
        "docker".to_string(),
        fixture.project_dir.clone(),
        false
    );

    state_manager.save_state(&initial_state)?;

    // Step 3: Modify configuration
    ConfigOps::set("vm.memory", "8192", false)?;

    // Step 4: Create new state with updated config
    let config = VmConfig::load(None, false)?;
    let updated_state = TempVmState::new(
        "persistent-test-2".to_string(),
        config.provider.clone(),
        fixture.project_dir.clone(),
        false
    );

    state_manager.save_state(&updated_state)?;

    // Step 5: Verify both states can coexist
    let loaded_initial = state_manager.load_state("persistent-test")?;
    let loaded_updated = state_manager.load_state("persistent-test-2")?;

    assert_eq!(loaded_initial.container_name(), "persistent-test");
    assert_eq!(loaded_updated.container_name(), "persistent-test-2");
    assert_eq!(loaded_initial.provider(), "docker");
    assert_eq!(loaded_updated.provider(), "docker");

    // Step 6: Test cleanup doesn't affect other states
    state_manager.cleanup_state("persistent-test")?;

    let cleanup_result = state_manager.load_state("persistent-test");
    assert!(cleanup_result.is_err());

    let still_exists = state_manager.load_state("persistent-test-2");
    assert!(still_exists.is_ok());

    Ok(())
}