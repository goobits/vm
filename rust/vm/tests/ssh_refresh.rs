
use assert_cmd::prelude::*;
use std::process::Command;
use tempfile::tempdir;
use std::fs;

#[test]
fn test_ssh_refresh_mounts() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let repo_path = temp_dir.path();

    // 1. Create Git repo
    Command::new("git")
        .arg("init")
        .current_dir(&repo_path)
        .assert()
        .success();

    // Create a dummy file and commit it
    fs::write(repo_path.join("README.md"), "Initial commit")?;
    Command::new("git")
        .args(&["add", "."])
        .current_dir(&repo_path)
        .assert()
        .success();
    Command::new("git")
        .args(&["commit", "-m", "Initial commit"])
        .current_dir(&repo_path)
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
    cmd.arg("create").current_dir(&repo_path).assert().success();

    // 4. Add NEW worktree
    Command::new("git")
        .args(&["worktree", "add", "../feature-branch"])
        .current_dir(&repo_path)
        .assert()
        .success();

    // 5. SSH with refresh
    let mut cmd = Command::cargo_bin("vm")?;
    let output = cmd.arg("ssh").arg("--force-refresh").current_dir(&repo_path).output()?;
    assert!(output.status.success());

    // 6. Verify new worktree is mounted
    let mut cmd = Command::cargo_bin("vm")?;
    let output = cmd.arg("exec").arg("--").arg("ls").arg("/workspace").current_dir(&repo_path).output()?;
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("feature-branch"));

    // Cleanup
    let mut cmd = Command::cargo_bin("vm")?;
    cmd.arg("destroy").arg("--force").current_dir(&repo_path).assert().success();

    Ok(())
}
