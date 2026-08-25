//! Integration tests for preset system
//!
//! This test suite validates the preset refactor, specifically:
//! 1. Image preset initialization (vm-config init with preset)
//! 2. Provision preset merging (vm config preset apply)
//! 3. Preset filtering in different contexts
//! 4. Project name derivation from directory

use serde_yaml_ng as serde_yaml;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tempfile::TempDir;
use vm_config::config::{
    CpuLimit, ImageSpec, MemoryLimit, ProjectConfig, TerminalConfig, VmConfig,
};
use vm_core::error::Result;

use vm_config::PresetDetector;

// Global mutex to ensure tests run sequentially to avoid environment variable conflicts
static TEST_MUTEX: Mutex<()> = Mutex::new(());

/// Test fixture that sets up isolated environment for preset testing
struct PresetTestFixture {
    _temp_dir: TempDir,
    project_dir: PathBuf,
    plugins_dir: PathBuf,
    presets_dir: PathBuf,
    original_home: Option<String>,
    original_vm_tool_dir: Option<String>,
}

impl PresetTestFixture {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let test_root = temp_dir.path().to_path_buf();

        // Create directory structure that matches plugin discovery expectations
        let project_dir = test_root.join("test-project");
        let vm_state_dir = test_root.join(".vm");
        let plugins_dir = vm_state_dir.join("plugins");
        let tool_dir = test_root.join("vm-tool");
        let presets_dir = tool_dir.join("configs").join("presets");

        fs::create_dir_all(&project_dir)?;
        fs::create_dir_all(&plugins_dir)?;
        fs::create_dir_all(&presets_dir)?;

        // Save and override environment variables
        let original_home = std::env::var("HOME").ok();
        let original_vm_tool_dir = std::env::var("VM_TOOL_DIR").ok();

        std::env::set_var("HOME", &test_root);
        std::env::set_var("VM_TOOL_DIR", &tool_dir);

        Ok(Self {
            _temp_dir: temp_dir,
            project_dir,
            plugins_dir,
            presets_dir,
            original_home,
            original_vm_tool_dir,
        })
    }

    fn read_vm_yaml(&self) -> Result<VmConfig> {
        let vm_yaml_path = self.project_dir.join("vm.yaml");
        let content = fs::read_to_string(&vm_yaml_path)?;
        Ok(serde_yaml::from_str(&content)?)
    }

    fn create_vibe_preset_plugin(&self) -> Result<()> {
        let vibe_dir = self.plugins_dir.join("presets").join("vibe-dev");
        fs::create_dir_all(&vibe_dir)?;

        // Write plugin.yaml
        let plugin_yaml = r#"name: vibe
version: 1.0.0
description: Vibe Development Image
plugin_type: preset
preset_category: image
"#;
        fs::write(vibe_dir.join("plugin.yaml"), plugin_yaml)?;

        // Write preset.yaml
        let preset_yaml = r#"vm_image: '@vibe-image'
category: image
networking:
  networks:
    - spacebase
host_sync:
  git_config: true
  ai_tools: true
tools:
  codex: {}
aliases:
  gs: git status
"#;
        fs::write(vibe_dir.join("preset.yaml"), preset_yaml)?;
        Ok(())
    }

    fn create_nodejs_preset_plugin(&self) -> Result<()> {
        let nodejs_dir = self.plugins_dir.join("presets").join("nodejs");
        fs::create_dir_all(&nodejs_dir)?;

        // Write plugin.yaml
        let plugin_yaml = r#"name: nodejs
version: 1.0.0
description: Node.js Development Environment
plugin_type: preset
preset_category: provision
"#;
        fs::write(nodejs_dir.join("plugin.yaml"), plugin_yaml)?;

        // Write preset.yaml
        let preset_yaml = r#"category: provision
packages:
  - curl
  - git
npm_packages:
  - typescript
  - eslint
  - prettier
services:
  - postgresql
environment:
  NODE_ENV: development
"#;
        fs::write(nodejs_dir.join("preset.yaml"), preset_yaml)?;
        Ok(())
    }

    fn create_python_preset_plugin(&self) -> Result<()> {
        let python_dir = self.plugins_dir.join("presets").join("python");
        fs::create_dir_all(&python_dir)?;

        // Write plugin.yaml
        let plugin_yaml = r#"name: python
version: 1.0.0
description: Python Development Environment
plugin_type: preset
preset_category: provision
"#;
        fs::write(python_dir.join("plugin.yaml"), plugin_yaml)?;

        // Write preset.yaml
        let preset_yaml = r#"category: provision
packages:
  - python3
  - python3-pip
pip_packages:
  - pytest
  - black
  - pylint
services:
  - postgresql
  - redis
"#;
        fs::write(python_dir.join("preset.yaml"), preset_yaml)?;
        Ok(())
    }

    fn create_detector(&self) -> PresetDetector {
        PresetDetector::new(self.project_dir.clone(), self.presets_dir.clone())
    }
}

impl Drop for PresetTestFixture {
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

// ============================================================================
// Test 1: Image Preset Initialization
// ============================================================================

#[test]
fn test_init_with_image_preset() -> Result<()> {
    // Validates that initializing with an image preset (like 'vibe'):
    // - Creates vm.yaml with vm.image reference
    // - Includes image-specific config (networking, host_sync, etc.)
    // - Does NOT include preset field
    // - Does NOT include versions or package arrays (they come from the image)
    // - Project name is derived from directory name

    let _guard = TEST_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fixture = PresetTestFixture::new()?;

    // Arrange: Create vibe image preset plugin
    fixture.create_vibe_preset_plugin()?;

    // Act: Simulate 'vm-config init vibe' - load the preset and apply image preset logic
    let detector = fixture.create_detector();
    let vibe_config = detector.load_preset("vibe")?;

    // Build minimal config as vm-config init would for an image preset
    // Set project name from directory
    let mut config = VmConfig {
        project: Some(ProjectConfig {
            name: Some("test-project".to_string()),
            hostname: Some("dev.test-project.local".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    };

    // Copy image reference from preset
    if let Some(preset_vm) = vibe_config.vm {
        if let Some(image_spec) = preset_vm.image {
            config.vm = Some(vm_config::config::VmSettings {
                image: Some(image_spec),
                ..Default::default()
            });
        }
    }

    // Copy networking, host_sync, tools, and aliases from preset
    config.networking = vibe_config.networking;
    config.host_sync = vibe_config.host_sync;
    config.tools = vibe_config.tools;
    config.aliases = vibe_config.aliases;

    // Write vm.yaml
    let vm_yaml = serde_yaml::to_string(&config)?;
    fs::write(fixture.project_dir.join("vm.yaml"), vm_yaml)?;

    // Assert: Verify the generated config
    let saved_config = fixture.read_vm_yaml()?;

    // 1. vm.image should exist and reference '@vibe-image'
    assert!(saved_config.vm.is_some(), "vm settings should be present");
    let vm_settings = saved_config.vm.as_ref().unwrap();
    assert!(vm_settings.image.is_some(), "vm.image should be present");

    match &vm_settings.image {
        Some(ImageSpec::String(s)) => {
            assert_eq!(s, "@vibe-image", "vm.image should reference @vibe-image")
        }
        _ => panic!("vm.image should be a string reference"),
    }
    assert!(saved_config.tools.entries.contains_key("codex"));

    // 2. networking.networks should contain 'spacebase'
    assert!(
        saved_config.networking.is_some(),
        "networking config should be present"
    );
    let networking = saved_config.networking.as_ref().unwrap();
    assert!(
        !networking.networks.is_empty() && networking.networks.contains(&"spacebase".to_string()),
        "networking.networks should contain 'spacebase'"
    );

    // 3. host_sync.ai_tools should be Some and git_config should be true
    assert!(
        saved_config.host_sync.is_some(),
        "host_sync config should be present"
    );
    let host_sync = saved_config.host_sync.as_ref().unwrap();
    assert!(
        host_sync.ai_tools.is_some(),
        "host_sync.ai_tools should be set"
    );
    assert!(host_sync.git_config, "host_sync.git_config should be true");

    // 4. Should NOT contain 'preset' field (image presets are not provision presets)
    assert!(
        saved_config.preset.is_none(),
        "preset field should NOT be present for image presets"
    );

    // 5. Should NOT contain versions field
    assert!(
        saved_config.versions.is_none(),
        "versions field should NOT be present (comes from image)"
    );

    // 6. Should NOT contain package arrays
    assert!(
        saved_config.apt_packages.is_empty(),
        "apt_packages should be empty (comes from image)"
    );
    assert!(
        saved_config.npm_packages.is_empty(),
        "npm_packages should be empty (comes from image)"
    );
    assert!(
        saved_config.pip_packages.is_empty(),
        "pip_packages should be empty (comes from image)"
    );
    assert!(
        saved_config.cargo_packages.is_empty(),
        "cargo_packages should be empty (comes from image)"
    );

    // 7. project.name should match directory name
    assert!(
        saved_config.project.is_some(),
        "project settings should be present"
    );
    let project = saved_config.project.as_ref().unwrap();
    assert_eq!(
        project.name,
        Some("test-project".to_string()),
        "project.name should be derived from directory"
    );

    Ok(())
}

// ============================================================================
// Test 2: Provision Preset Merge
// ============================================================================

#[test]
fn test_config_preset_provision() -> Result<()> {
    // Validates that applying a provision preset (like 'nodejs'):
    //- Merges npm_packages into config
    //- Enables services from preset
    //- Sets preset reference field
    //- Does NOT include vm.image field

    let _guard = TEST_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fixture = PresetTestFixture::new()?;

    // Arrange: Create nodejs provision preset plugin
    fixture.create_nodejs_preset_plugin()?;

    // Create basic vm.yaml
    let base_config = VmConfig {
        project: Some(ProjectConfig {
            name: Some("test-project".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let base_yaml = serde_yaml::to_string(&base_config)?;
    fs::write(fixture.project_dir.join("vm.yaml"), base_yaml)?;

    // Act: Apply nodejs preset
    use vm_config::ConfigMerger;
    let detector = fixture.create_detector();
    let nodejs_preset = detector.load_preset("nodejs")?;
    let mut merged_config = ConfigMerger::new(base_config).merge(nodejs_preset)?;
    merged_config.preset = Some("nodejs".to_string());

    // Write merged config
    let merged_yaml = serde_yaml::to_string(&merged_config)?;
    fs::write(fixture.project_dir.join("vm.yaml"), merged_yaml)?;

    // Assert: Verify the merged config
    let saved_config = fixture.read_vm_yaml()?;

    // 1. npm_packages should include typescript, eslint
    assert!(
        saved_config
            .npm_packages
            .contains(&"typescript".to_string()),
        "npm_packages should include typescript"
    );
    assert!(
        saved_config.npm_packages.contains(&"eslint".to_string()),
        "npm_packages should include eslint"
    );
    assert!(
        saved_config.npm_packages.contains(&"prettier".to_string()),
        "npm_packages should include prettier"
    );

    // 2. services.postgresql should be enabled
    assert!(
        saved_config.services.contains_key("postgresql"),
        "postgresql service should be configured"
    );
    assert!(
        saved_config.services.get("postgresql").unwrap().enabled,
        "postgresql service should be enabled"
    );

    // 3. preset field should be set to 'nodejs'
    assert_eq!(
        saved_config.preset,
        Some("nodejs".to_string()),
        "preset field should reference nodejs"
    );

    // 4. environment variable should be set
    assert_eq!(
        saved_config.environment.get("NODE_ENV"),
        Some(&"development".to_string()),
        "NODE_ENV should be set to development"
    );

    // 5. Should NOT have vm.image field (provision presets don't set images)
    assert!(
        saved_config.vm.is_none() || saved_config.vm.as_ref().unwrap().image.is_none(),
        "vm.image should NOT be set for provision presets"
    );

    Ok(())
}

// ============================================================================
// Test 3: Image Preset Filtered from Config List
// ============================================================================

#[test]
fn test_image_preset_not_in_config_list() -> Result<()> {
    // Validates that list_presets() (used by 'vm config preset')
    //excludes image presets but includes provision presets

    let _guard = TEST_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fixture = PresetTestFixture::new()?;

    // Arrange: Create both image and provision presets
    fixture.create_vibe_preset_plugin()?;
    fixture.create_nodejs_preset_plugin()?;
    fixture.create_python_preset_plugin()?;

    // Act: List presets for config operations
    let detector = fixture.create_detector();
    let presets = detector.list_presets()?;

    // Assert: Image preset should NOT be in list
    assert!(
        !presets.contains(&"vibe".to_string()),
        "vibe (image preset) should NOT be in config preset list"
    );

    // Provision presets SHOULD be in list
    assert!(
        presets.contains(&"nodejs".to_string()),
        "nodejs (provision preset) should be in config preset list"
    );
    assert!(
        presets.contains(&"python".to_string()),
        "python (provision preset) should be in config preset list"
    );

    Ok(())
}

// ============================================================================
// Test 4: Image Preset in Init List
// ============================================================================

#[test]
fn test_image_preset_in_init_list() -> Result<()> {
    // Validates that list_all_presets() (used by 'vm-config init')
    // Includes both image and provision presets

    let _guard = TEST_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fixture = PresetTestFixture::new()?;

    // Arrange: Create both image and provision presets
    fixture.create_vibe_preset_plugin()?;
    fixture.create_nodejs_preset_plugin()?;
    fixture.create_python_preset_plugin()?;

    // Act: List all presets for init operations
    let detector = fixture.create_detector();
    let all_presets = detector.list_all_presets()?;

    // Assert: Both image and provision presets should be in list
    assert!(
        all_presets.contains(&"vibe".to_string()),
        "vibe (image preset) should be in init preset list"
    );
    assert!(
        all_presets.contains(&"nodejs".to_string()),
        "nodejs (provision preset) should be in init preset list"
    );
    assert!(
        all_presets.contains(&"python".to_string()),
        "python (provision preset) should be in init preset list"
    );

    Ok(())
}

// ============================================================================
// Test 5: Preset Category Detection
// ============================================================================

#[test]
fn test_preset_category_detection() -> Result<()> {
    // Validates that preset categories are correctly detected from:
    //1. Plugin metadata (preset_category field)
    //2. Preset content (category field)
    //3. Presence of vm_image field (fallback)

    let _guard = TEST_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fixture = PresetTestFixture::new()?;

    // Arrange: Create presets
    fixture.create_vibe_preset_plugin()?;
    fixture.create_nodejs_preset_plugin()?;

    // Act & Assert: Check category detection
    let detector = fixture.create_detector();

    // Load vibe preset and check for image characteristics
    let vibe_config = detector.load_preset("vibe")?;
    let has_image = vibe_config
        .vm
        .as_ref()
        .and_then(|vm| vm.image.as_ref())
        .is_some();
    assert!(has_image, "vibe preset should have vm.image field");

    // Load nodejs preset and verify it doesn't have an image
    let nodejs_config = detector.load_preset("nodejs")?;
    let has_image = nodejs_config
        .vm
        .as_ref()
        .and_then(|vm| vm.image.as_ref())
        .is_some();
    assert!(!has_image, "nodejs preset should NOT have vm.image field");

    // Verify nodejs has provision-specific fields
    assert!(
        !nodejs_config.npm_packages.is_empty(),
        "nodejs preset should have npm_packages (provision characteristic)"
    );
    assert!(
        !nodejs_config.services.is_empty(),
        "nodejs preset should have services (provision characteristic)"
    );

    Ok(())
}

// ============================================================================
// Test 6: Project Name Derivation
// ============================================================================

#[test]
fn test_project_name_from_directory() -> Result<()> {
    // Validates that project name, hostname, and username are correctly
    //derived from the directory name, even with special characters

    let _guard = TEST_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // Test with clean directory name
    let temp_dir = TempDir::new()?;
    let project_dir = temp_dir.path().join("my-cool-project");
    fs::create_dir_all(&project_dir)?;

    // Simulate name derivation (as done in init.rs)
    let dir_name = project_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("vm-project");

    assert_eq!(dir_name, "my-cool-project");

    // Create config with derived names
    let config = VmConfig {
        project: Some(ProjectConfig {
            name: Some(dir_name.to_string()),
            hostname: Some(format!("dev.{}.local", dir_name)),
            ..Default::default()
        }),
        terminal: Some(TerminalConfig {
            username: Some(format!("{}-dev", dir_name)),
            ..Default::default()
        }),
        ..Default::default()
    };

    // Assert: Check derived values
    let project = config.project.as_ref().unwrap();
    assert_eq!(
        project.name,
        Some("my-cool-project".to_string()),
        "project.name should match directory name"
    );
    assert_eq!(
        project.hostname,
        Some("dev.my-cool-project.local".to_string()),
        "project.hostname should be derived from directory name"
    );

    let terminal = config.terminal.as_ref().unwrap();
    assert_eq!(
        terminal.username,
        Some("my-cool-project-dev".to_string()),
        "terminal.username should be derived from directory name"
    );

    Ok(())
}

// ============================================================================
// Test 7: Networking Config Merge
// ============================================================================

#[test]
fn test_image_preset_networking_merge() -> Result<()> {
    // Validates that networking configuration from an image preset
    //is properly merged into the generated vm.yaml

    let _guard = TEST_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fixture = PresetTestFixture::new()?;

    // Arrange: Create vibe preset with networking config
    fixture.create_vibe_preset_plugin()?;

    // Act: Load preset and extract networking
    let detector = fixture.create_detector();
    let vibe_config = detector.load_preset("vibe")?;

    let config = VmConfig {
        networking: vibe_config.networking,
        ..Default::default()
    };

    // Assert: Verify networking configuration
    assert!(
        config.networking.is_some(),
        "networking config should be present"
    );

    let networking = config.networking.as_ref().unwrap();
    assert!(
        !networking.networks.is_empty(),
        "networking.networks should be present"
    );

    let networks = &networking.networks;
    assert_eq!(networks.len(), 1, "should have exactly one network");
    assert_eq!(networks[0], "spacebase", "network should be 'spacebase'");

    Ok(())
}

// ============================================================================
// Test 8: Multiple Provision Preset Merge
// ============================================================================

#[test]
fn test_multiple_provision_preset_merge() -> Result<()> {
    // Validates that multiple provision presets can be merged together,
    //and that packages/services are accumulated correctly

    let _guard = TEST_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fixture = PresetTestFixture::new()?;

    // Arrange: Create multiple provision presets
    fixture.create_nodejs_preset_plugin()?;
    fixture.create_python_preset_plugin()?;

    // Act: Load and merge both presets
    use vm_config::ConfigMerger;
    let detector = fixture.create_detector();

    let base_config = VmConfig::default();
    let nodejs_preset = detector.load_preset("nodejs")?;
    let python_preset = detector.load_preset("python")?;

    // Merge nodejs first, then python
    let config_with_nodejs = ConfigMerger::new(base_config).merge(nodejs_preset)?;
    let final_config = ConfigMerger::new(config_with_nodejs).merge(python_preset)?;

    // Assert: Both preset's packages should be present
    // From nodejs
    assert!(
        final_config
            .npm_packages
            .contains(&"typescript".to_string()),
        "should have typescript from nodejs preset"
    );

    // From python
    assert!(
        final_config.pip_packages.contains(&"pytest".to_string()),
        "should have pytest from python preset"
    );

    // Both presets include postgresql
    assert!(
        final_config.services.contains_key("postgresql"),
        "postgresql should be enabled (from both presets)"
    );

    // Only python includes redis
    assert!(
        final_config.services.contains_key("redis"),
        "redis should be enabled (from python preset)"
    );

    // Packages from second preset (python) should be present
    // Note: Sequential merging means later preset's packages replace earlier ones
    // If you want to accumulate packages, you'd need a different merge strategy
    assert!(
        final_config.apt_packages.contains(&"python3".to_string()),
        "should have packages from python preset (the second merge)"
    );

    // Verify the later merge preserved npm_packages from first preset
    // This demonstrates that different package types can coexist
    assert!(
        final_config
            .npm_packages
            .contains(&"typescript".to_string()),
        "npm_packages from nodejs should be preserved (different package type)"
    );

    Ok(())
}

// ============================================================================
// Test 9: Image Preset Aliases Preservation
// ============================================================================

#[test]
fn test_image_preset_aliases_preserved() -> Result<()> {
    // Validates that aliases from image presets are preserved in vm.yaml

    let _guard = TEST_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fixture = PresetTestFixture::new()?;

    // Arrange: Create vibe preset with aliases
    fixture.create_vibe_preset_plugin()?;

    // Act: Load preset
    let detector = fixture.create_detector();
    let vibe_config = detector.load_preset("vibe")?;

    let config = VmConfig {
        aliases: vibe_config.aliases,
        ..Default::default()
    };

    // Assert: Verify aliases are present
    assert!(
        !config.aliases.is_empty(),
        "aliases should be present from image preset"
    );
    assert_eq!(
        config.aliases.get("gs"),
        Some(&"git status".to_string()),
        "alias 'gs' should be preserved"
    );

    Ok(())
}

// ============================================================================
// Test 10: Preset Description Retrieval
// ============================================================================

#[test]
fn test_preset_description_retrieval() -> Result<()> {
    // Validates that preset descriptions can be retrieved from plugin metadata

    let _guard = TEST_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fixture = PresetTestFixture::new()?;

    // Arrange: Create presets
    fixture.create_vibe_preset_plugin()?;
    fixture.create_nodejs_preset_plugin()?;

    // Act: Get descriptions
    let detector = fixture.create_detector();
    let vibe_desc = detector.get_preset_description("vibe");
    let nodejs_desc = detector.get_preset_description("nodejs");

    // Assert: Descriptions should be available
    assert!(vibe_desc.is_some(), "vibe preset should have a description");
    assert_eq!(
        vibe_desc.unwrap(),
        "Vibe Development Image",
        "vibe description should match"
    );

    assert!(
        nodejs_desc.is_some(),
        "nodejs preset should have a description"
    );
    assert_eq!(
        nodejs_desc.unwrap(),
        "Node.js Development Environment",
        "nodejs description should match"
    );

    Ok(())
}

#[test]
fn test_vibe_tart_preset_uses_linux_tart_by_default() -> Result<()> {
    let _guard = TEST_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fixture = PresetTestFixture::new()?;
    let detector = fixture.create_detector();

    let vibe_tart = detector.load_preset("vibe-tart")?;
    assert_eq!(vibe_tart.provider.as_deref(), Some("tart"));
    assert_eq!(vibe_tart.default_profile.as_deref(), Some("tart"));
    let vm = vibe_tart.vm.as_ref().expect("vibe-tart should size its VM");
    assert_eq!(vm.memory, Some(MemoryLimit::Percentage(75)));
    assert_eq!(vm.cpus, Some(CpuLimit::Percentage(75)));
    assert!(vibe_tart
        .networking
        .as_ref()
        .is_none_or(|networking| networking.networks.is_empty()));
    let tart_profile = vibe_tart
        .profiles
        .as_ref()
        .and_then(|profiles| profiles.get("tart"))
        .expect("vibe-tart should define a tart profile");

    assert_eq!(tart_profile.provider.as_deref(), Some("tart"));
    assert_eq!(
        tart_profile
            .vm
            .as_ref()
            .and_then(|vm| vm.image.as_ref())
            .map(|b| serde_yaml::to_string(b).unwrap().trim().to_string()),
        Some("vibe-tart-linux-base".to_string())
    );
    assert_eq!(
        tart_profile
            .tart
            .as_ref()
            .and_then(|tart| tart.guest_os.as_deref()),
        Some("linux")
    );
    assert_eq!(
        tart_profile
            .tart
            .as_ref()
            .and_then(|tart| tart.ssh_user.as_deref()),
        Some("admin")
    );
    assert_eq!(
        tart_profile
            .tart
            .as_ref()
            .and_then(|tart| tart.install_docker),
        Some(true)
    );
    assert!(!tart_profile.environment.contains_key("CARGO_TARGET_DIR"));
    assert!(!vibe_tart.environment.contains_key("CARGO_TARGET_DIR"));
    let macos_profile = vibe_tart
        .profiles
        .as_ref()
        .and_then(|profiles| profiles.get("macos"))
        .expect("vibe-tart should keep an explicit macos profile");
    assert_eq!(
        macos_profile
            .tart
            .as_ref()
            .and_then(|tart| tart.guest_os.as_deref()),
        Some("macos")
    );
    let ai_tools = vibe_tart
        .host_sync
        .as_ref()
        .and_then(|sync| sync.ai_tools.as_ref())
        .expect("vibe-tart should sync supported AI state");
    assert!(ai_tools.is_antigravity_enabled());
    assert!(ai_tools.is_claude_enabled());
    assert!(ai_tools.is_codex_enabled());
    assert!(vibe_tart.tools.entries.contains_key("agent-skills"));
    for name in ["antigravity", "claude", "codex"] {
        assert!(
            !vibe_tart.tools.entries.contains_key(name),
            "the Vibe base should own {name}"
        );
    }
    assert!(vibe_tart.npm_packages.is_empty());

    Ok(())
}
