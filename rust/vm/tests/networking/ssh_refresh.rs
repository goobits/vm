use assert_cmd::prelude::*;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// Check if Docker daemon is available
fn is_docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[test]
fn test_ssh_refresh_mounts() -> Result<(), Box<dyn std::error::Error>> {
    if !is_docker_available() {
        eprintln!("⚠️  Skipping test: Docker daemon not available");
        eprintln!("   To run this test, ensure Docker is running");
        return Ok(());
    }

    let temp_dir = tempdir()?;
    let repo_path = temp_dir.path();

    // 1. Create Git repo with configured user
    Command::new("git")
        .arg("init")
        .current_dir(repo_path)
        .assert()
        .success();

    // Configure git user for this repo
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .assert()
        .success();
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .assert()
        .success();

    // Create a dummy file and commit it
    fs::write(repo_path.join("README.md"), "Initial commit")?;
    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .assert()
        .success();
    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .assert()
        .success();

    // 2. Create vm.yaml
    let vm_yaml = r#"
project:
  name: ssh-refresh-test
provider: docker
"#;
    fs::write(repo_path.join("vm.yaml"), vm_yaml)?;

    // 3. Create VM
    let mut cmd = Command::cargo_bin("vm")?;
    cmd.arg("create").current_dir(repo_path).assert().success();

    // 4. Add NEW worktree
    Command::new("git")
        .args(["worktree", "add", "../feature-branch"])
        .current_dir(repo_path)
        .assert()
        .success();

    // 5. SSH with refresh
    let mut cmd = Command::cargo_bin("vm")?;
    let output = cmd
        .arg("ssh")
        .arg("--force-refresh")
        .current_dir(repo_path)
        .output()?;
    assert!(output.status.success());

    // 6. Verify new worktree is mounted
    let mut cmd = Command::cargo_bin("vm")?;
    let output = cmd
        .arg("exec")
        .arg("--")
        .arg("ls")
        .arg("/workspace")
        .current_dir(repo_path)
        .output()?;
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("feature-branch"));

    // Cleanup
    let mut cmd = Command::cargo_bin("vm")?;
    cmd.arg("destroy")
        .arg("--force")
        .current_dir(repo_path)
        .assert()
        .success();

    Ok(())
}
