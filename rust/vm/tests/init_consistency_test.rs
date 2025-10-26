// Test to ensure vm init produces consistent output with vm-config init
use anyhow::Result;
use std::fs;
use std::process::Command;
use tempfile::TempDir;
use vm_config::config::VmConfig;

/// Get the path to the vm binary
fn get_vm_binary() -> std::path::PathBuf {
    // Use the binary in the debug build directory
    let mut path = std::env::current_exe().expect("Failed to get current executable path");

    // Remove the test binary name and go up to deps directory
    path.pop();

    // Check if we're in deps directory, if so go up to debug
    if path.ends_with("deps") {
        path.pop();
    }

    let binary_name = if cfg!(target_os = "windows") {
        "vm.exe"
    } else {
        "vm"
    };

    path.join(binary_name)
}

#[test]
fn test_init_produces_full_config_with_all_keys() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let vm_yaml_path = temp_dir.path().join("vm.yaml");

    // Run vm init via CLI
    let output = Command::new(get_vm_binary())
        .arg("init")
        .arg("--file")
        .arg(&vm_yaml_path)
        .current_dir(temp_dir.path())
        .output()?;

    assert!(output.status.success(), "vm init failed: {:?}", output);

    // Read the generated config
    let yaml_content = fs::read_to_string(&vm_yaml_path)?;
    let config: VmConfig = serde_yaml_ng::from_str(&yaml_content)?;

    // Verify all expected top-level fields are present
    assert!(config.version.is_some(), "version field should be present");
    assert!(
        config.provider.is_some(),
        "provider field should be present"
    );
    assert!(config.os.is_some(), "os field should be present");
    assert!(config.project.is_some(), "project field should be present");
    assert!(config.vm.is_some(), "vm field should be present");
    assert!(
        config.versions.is_some(),
        "versions field should be present"
    );
    assert!(
        config.terminal.is_some(),
        "terminal field should be present"
    );

    // Verify nested vm fields are present
    if let Some(vm) = config.vm {
        assert!(vm.r#box.is_some(), "vm.box should be present");
        assert!(vm.cpus.is_some(), "vm.cpus should be present");
        assert!(vm.memory.is_some(), "vm.memory should be present");
        assert!(vm.swap.is_some(), "vm.swap should be present");
        assert!(vm.swappiness.is_some(), "vm.swappiness should be present");
        assert!(vm.user.is_some(), "vm.user should be present");
        assert!(
            vm.port_binding.is_some(),
            "vm.port_binding should be present"
        );
        assert!(vm.gui.is_some(), "vm.gui should be present");
        assert!(vm.timezone.is_some(), "vm.timezone should be present");
    }

    // Verify nested terminal fields are present
    if let Some(terminal) = config.terminal {
        assert!(terminal.emoji.is_some(), "terminal.emoji should be present");
        assert!(
            terminal.username.is_some(),
            "terminal.username should be present"
        );
        assert!(terminal.theme.is_some(), "terminal.theme should be present");
        assert!(
            terminal.show_git_branch.is_some(),
            "terminal.show_git_branch should be present"
        );
        assert!(
            terminal.show_timestamp.is_some(),
            "terminal.show_timestamp should be present"
        );
    }

    // Verify nested project fields are present
    if let Some(project) = config.project {
        assert!(
            project.hostname.is_some(),
            "project.hostname should be present"
        );
        assert!(project.name.is_some(), "project.name should be present");
        assert!(
            project.workspace_path.is_some(),
            "project.workspace_path should be present"
        );
    }

    Ok(())
}

#[test]
fn test_init_values_match_defaults() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let vm_yaml_path = temp_dir.path().join("vm.yaml");

    // Run vm init via CLI
    let output = Command::new(get_vm_binary())
        .arg("init")
        .arg("--file")
        .arg(&vm_yaml_path)
        .current_dir(temp_dir.path())
        .output()?;

    assert!(output.status.success(), "vm init failed: {:?}", output);

    // Read the generated config
    let yaml_content = fs::read_to_string(&vm_yaml_path)?;
    let config: VmConfig = serde_yaml_ng::from_str(&yaml_content)?;

    // Check that default values from defaults.yaml are preserved
    assert_eq!(config.version, Some("1.2.1".to_string()));
    assert_eq!(config.provider, Some("docker".to_string()));
    assert_eq!(config.os, Some("auto".to_string()));

    if let Some(vm) = config.vm {
        // Check swap is present and equals 2048
        if let Some(swap) = vm.swap {
            assert_eq!(swap.to_mb().unwrap_or(0), 2048);
        }
        assert_eq!(vm.swappiness, Some(60));
        assert_eq!(vm.port_binding, Some("127.0.0.1".to_string()));
        assert_eq!(vm.gui, Some(false));
        assert_eq!(vm.timezone, Some("auto".to_string()));
    }

    if let Some(versions) = config.versions {
        assert_eq!(versions.node, Some("22".to_string()));
        assert_eq!(versions.python, Some("3.11".to_string()));
    }

    if let Some(project) = config.project {
        assert_eq!(project.workspace_path, Some("/workspace".to_string()));
    }

    Ok(())
}
