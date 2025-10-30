use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

/// Sets up a temporary home directory with legacy config files.
fn setup_legacy_environment(home_dir: &Path) {
    // Create old config directories
    let old_config_dir = home_dir.join(".config").join("vm");
    let vm_state_dir = home_dir.join(".vm");
    fs::create_dir_all(&old_config_dir).unwrap();
    fs::create_dir_all(&vm_state_dir).unwrap();

    // Create dummy legacy files
    fs::write(old_config_dir.join("global.yaml"), "key: value").unwrap();
    fs::write(vm_state_dir.join("port-registry.json"), "[]").unwrap();
    fs::write(vm_state_dir.join("service_state.json"), "{}").unwrap();
    fs::write(vm_state_dir.join("temp-vm.state"), "[]").unwrap();
}

#[test]
fn test_migration_command_successful() {
    let temp_home = tempdir().unwrap();
    setup_legacy_environment(temp_home.path());

    // --- Run the migration command ---
    let mut cmd = Command::from(assert_cmd::cargo::cargo_bin("vm-config"));
    cmd.env("HOME", temp_home.path())
        .arg("migrate")
        .write_stdin("y\n") // Confirm the prompt
        .assert()
        .success()
        .stdout(predicate::str::contains("Migration complete!"));

    // --- Verify new files exist ---
    let vm_state_dir = temp_home.path().join(".vm");
    assert!(vm_state_dir.join("config.yaml").exists());
    assert!(vm_state_dir.join("ports.json").exists());
    assert!(vm_state_dir.join("services.json").exists());
    assert!(vm_state_dir.join("temp-vms.json").exists());

    // --- Verify old files are gone ---
    let old_config_dir = temp_home.path().join(".config").join("vm");
    assert!(!old_config_dir.join("global.yaml").exists());
    assert!(!vm_state_dir.join("port-registry.json").exists());
    assert!(!vm_state_dir.join("service_state.json").exists());
    assert!(!vm_state_dir.join("temp-vm.state").exists());

    // --- Verify backups exist ---
    let backup_base_dir = vm_state_dir.join("backups");
    let entries = fs::read_dir(backup_base_dir).unwrap();
    let backup_dir = entries.into_iter().next().unwrap().unwrap().path(); // Get the timestamped backup dir

    assert!(backup_dir.join("global.yaml").exists());
    assert!(backup_dir.join("port-registry.json").exists());
    assert!(backup_dir.join("service_state.json").exists());
    assert!(backup_dir.join("temp-vm.state").exists());
}

#[test]
fn test_migration_command_no_files_to_migrate() {
    let temp_home = tempdir().unwrap();

    let mut cmd = Command::from(assert_cmd::cargo::cargo_bin("vm-config"));
    cmd.env("HOME", temp_home.path())
        .arg("migrate")
        .assert()
        .success()
        .stdout(predicate::str::contains("No migration needed."));
}

#[test]
fn test_migration_command_user_cancels() {
    let temp_home = tempdir().unwrap();
    setup_legacy_environment(temp_home.path());

    let mut cmd = Command::from(assert_cmd::cargo::cargo_bin("vm-config"));
    cmd.env("HOME", temp_home.path())
        .arg("migrate")
        .write_stdin("n\n") // Deny the prompt
        .assert()
        .success()
        .stdout(predicate::str::contains("Migration cancelled by user."));

    // Verify old files still exist
    let old_config_dir = temp_home.path().join(".config").join("vm");
    assert!(old_config_dir.join("global.yaml").exists());
}
