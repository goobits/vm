use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Test fixture for Package Registry CLI integration tests
struct RegistryTestFixture {
    _temp_dir: TempDir,
    test_dir: PathBuf,
    binary_path: PathBuf,
}

impl RegistryTestFixture {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let test_dir = temp_dir.path().join("test_project");
        fs::create_dir_all(&test_dir)?;

        let binary_path = Command::new(assert_cmd::cargo::cargo_bin!("vm"))
            .get_program()
            .into();

        Ok(Self {
            _temp_dir: temp_dir,
            test_dir,
            binary_path,
        })
    }

    fn run_registry_command(&self, args: &[&str]) -> Result<std::process::Output> {
        let mut cmd = Command::new(&self.binary_path);
        cmd.arg("registry")
            .args(args)
            .current_dir(&self.test_dir)
            .env("VM_TOOL_DIR", self.test_dir.join(".vm"))
            .env("RUST_LOG", "info");

        let output = cmd.output()?;
        Ok(output)
    }

    fn get_output(&self, output: &std::process::Output) -> String {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        format!("{}{}", stdout, stderr)
    }

    fn is_registry_running(&self) -> bool {
        Command::new("curl")
            .args([
                "-s",
                "-o",
                "/dev/null",
                "-w",
                "%{http_code}",
                "http://localhost:3080/health",
            ])
            .output()
            .map(|output| output.stdout == b"200")
            .unwrap_or(false)
    }
}

#[test]
fn test_registry_help_command() -> Result<()> {
    let fixture = RegistryTestFixture::new()?;

    let output = fixture.run_registry_command(&["--help"])?;

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("Manage package registries"));
    assert!(stdout.contains("status"));
    assert!(stdout.contains("add"));
    assert!(stdout.contains("remove"));
    assert!(stdout.contains("list"));
    assert!(stdout.contains("config"));
    assert!(stdout.contains("use"));

    Ok(())
}

#[test]
fn test_registry_status_functionality() -> Result<()> {
    use vm_package_server::show_status;

    let result = show_status("http://localhost:3080");
    assert!(result.is_ok());

    Ok(())
}

#[test]
fn test_registry_config_show_command() -> Result<()> {
    let fixture = RegistryTestFixture::new()?;

    let output = fixture.run_registry_command(&["config", "show"])?;

    assert!(output.status.success());
    let combined_output = fixture.get_output(&output);
    assert!(combined_output.contains("Package Registry Configuration"));

    Ok(())
}

#[test]
fn test_registry_use_bash_command() -> Result<()> {
    let fixture = RegistryTestFixture::new()?;

    let output = fixture.run_registry_command(&["use", "--shell", "bash"])?;

    assert!(output.status.success());
    let combined_output = fixture.get_output(&output);

    assert!(combined_output.contains("# Package registry configuration for bash"));
    assert!(combined_output.contains("export NPM_CONFIG_REGISTRY="));
    assert!(combined_output.contains("export PIP_INDEX_URL="));
    assert!(combined_output.contains("export PIP_TRUSTED_HOST="));

    Ok(())
}

#[test]
fn test_registry_use_zsh_command() -> Result<()> {
    let fixture = RegistryTestFixture::new()?;

    let output = fixture.run_registry_command(&["use", "--shell", "zsh"])?;

    assert!(output.status.success());
    let combined_output = fixture.get_output(&output);

    assert!(combined_output.contains("# Package registry configuration for zsh"));
    assert!(combined_output.contains("export NPM_CONFIG_REGISTRY="));
    assert!(combined_output.contains("export PIP_INDEX_URL="));
    assert!(combined_output.contains("export PIP_TRUSTED_HOST="));

    Ok(())
}

#[test]
fn test_registry_use_custom_port() -> Result<()> {
    let fixture = RegistryTestFixture::new()?;

    let output = fixture.run_registry_command(&["use", "--shell", "bash", "--port", "4080"])?;

    assert!(output.status.success());
    let combined_output = fixture.get_output(&output);
    assert!(combined_output.contains("http://localhost:4080"));

    Ok(())
}

#[test]
fn test_registry_auto_managed_design() -> Result<()> {
    let fixture = RegistryTestFixture::new()?;

    let output = fixture.run_registry_command(&["start", "--help"]);
    assert!(output.is_err() || !output.unwrap().status.success());

    let output = fixture.run_registry_command(&["stop", "--help"]);
    assert!(output.is_err() || !output.unwrap().status.success());

    let output = fixture.run_registry_command(&["status"]);
    match output {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                assert!(
                    stdout.contains("automatically managed")
                        || stdout.contains("service manager")
                        || stdout.contains("Package Registry Status")
                );
            }
        }
        Err(_) => {}
    }

    Ok(())
}

#[test]
fn test_registry_add_command_help() -> Result<()> {
    let fixture = RegistryTestFixture::new()?;

    let output = fixture.run_registry_command(&["add", "--help"])?;

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Publish a package"));
    assert!(stdout.contains("--type"));

    Ok(())
}

#[test]
fn test_registry_remove_command_help() -> Result<()> {
    let fixture = RegistryTestFixture::new()?;

    let output = fixture.run_registry_command(&["remove", "--help"])?;

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Remove a package"));
    assert!(stdout.contains("--force"));

    Ok(())
}

#[test]
fn test_registry_list_command() -> Result<()> {
    let fixture = RegistryTestFixture::new()?;

    let output = fixture.run_registry_command(&["list"])?;
    let combined_output = fixture.get_output(&output);

    if !output.status.success() {
        assert!(
            combined_output.contains("panicked")
                || combined_output.contains("runtime")
                || combined_output.contains("error")
                || combined_output.contains("Failed")
        );
    }

    Ok(())
}

#[test]
fn test_registry_command_error_handling() -> Result<()> {
    let fixture = RegistryTestFixture::new()?;

    let output = fixture.run_registry_command(&["invalid-command"])?;
    assert!(!output.status.success());

    let output = fixture.run_registry_command(&["status", "--invalid-flag"])?;
    assert!(!output.status.success());

    Ok(())
}

#[test]
fn test_registry_config_commands() -> Result<()> {
    let fixture = RegistryTestFixture::new()?;

    let commands = vec![vec!["config", "show"], vec!["config", "--help"]];

    for cmd_args in commands {
        let output = fixture.run_registry_command(&cmd_args)?;

        if !output.status.success() {
            let combined_output = fixture.get_output(&output);
            assert!(
                combined_output.contains("help")
                    || combined_output.contains("error")
                    || combined_output.contains("not found")
            );
        }
    }

    Ok(())
}
