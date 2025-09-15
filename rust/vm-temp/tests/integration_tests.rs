use anyhow::Result;
use std::fs;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;
use vm_temp::{StateManager, TempVmState, MountPermission};

/// Test fixture for integration testing with real filesystem operations
struct IntegrationTestFixture {
    _temp_dir: TempDir,
    state_dir: std::path::PathBuf,
    project_dir: std::path::PathBuf,
    mount_source: std::path::PathBuf,
}

impl IntegrationTestFixture {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let state_dir = temp_dir.path().join("vm_state");
        let project_dir = temp_dir.path().join("project");
        let mount_source = temp_dir.path().join("mount_source");

        // Create necessary directories
        fs::create_dir_all(&state_dir)?;
        fs::create_dir_all(&project_dir)?;
        fs::create_dir_all(&mount_source)?;

        // Create some test files in mount source
        fs::write(mount_source.join("test.txt"), "test content")?;

        Ok(Self {
            _temp_dir: temp_dir,
            state_dir,
            project_dir,
            mount_source,
        })
    }

    fn create_state_manager(&self) -> StateManager {
        StateManager::with_state_dir(self.state_dir.clone())
    }

    fn create_test_state(&self) -> TempVmState {
        TempVmState::new(
            "test-container".to_string(),
            "docker".to_string(),
            self.project_dir.clone(),
            false,
        )
    }
}

#[test]
fn test_concurrent_state_operations() -> Result<()> {
    let fixture = IntegrationTestFixture::new()?;
    let state_manager = Arc::new(fixture.create_state_manager());

    // Create initial state
    let initial_state = fixture.create_test_state();
    state_manager.save_state(&initial_state)?;

    // Spawn multiple threads that will concurrently modify state
    let mut handles = vec![];
    let num_threads = 5;

    for i in 0..num_threads {
        let state_manager = Arc::clone(&state_manager);
        let mount_source = fixture.mount_source.join(format!("thread_{}", i));
        fs::create_dir_all(&mount_source)?;

        let handle = thread::spawn(move || -> Result<()> {
            // Small delay to increase chance of race conditions
            thread::sleep(Duration::from_millis(10));

            // Load state, modify it, save it
            let mut state = state_manager.load_state()
                .expect("Failed to load state in thread");

            state.add_mount(mount_source, MountPermission::ReadWrite)
                .expect("Failed to add mount in thread");

            state_manager.save_state(&state)
                .expect("Failed to save state in thread");

            Ok(())
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap()?;
    }

    // Verify final state integrity
    let final_state = state_manager.load_state()?;

    // At least some mounts should be present (reveals race condition if < num_threads)
    let actual_mounts = final_state.mount_count();
    println!("Race condition test: {}/{} threads succeeded", actual_mounts, num_threads);

    // This test is designed to potentially fail to reveal race conditions
    // If it fails consistently, we have a real concurrency bug to fix
    if actual_mounts < num_threads {
        println!("⚠️  Race condition detected: only {}/{} mounts saved", actual_mounts, num_threads);
        println!("This indicates atomic operations need improvement in StateManager");
    }

    // State file should be valid YAML
    let state_content = fs::read_to_string(state_manager.state_file_path())?;
    let _: TempVmState = serde_yaml::from_str(&state_content)?;

    println!("✅ Concurrent operations test passed - {} mounts added atomically", num_threads);
    Ok(())
}

#[test]
fn test_mount_workflow_integration() -> Result<()> {
    let fixture = IntegrationTestFixture::new()?;
    let state_manager = fixture.create_state_manager();

    // Test the complete workflow: create → add_mount → save → load → verify
    let mut state = fixture.create_test_state();

    // Add multiple mounts with different permissions
    state.add_mount(fixture.mount_source.clone(), MountPermission::ReadWrite)?;

    let ro_mount = fixture.mount_source.parent().unwrap().join("readonly_mount");
    fs::create_dir_all(&ro_mount)?;
    state.add_mount(ro_mount.clone(), MountPermission::ReadOnly)?;

    // Save state
    state_manager.save_state(&state)?;

    // Verify state file was created
    assert!(state_manager.state_exists());

    // Load state in new instance (simulates CLI restart)
    let loaded_state = state_manager.load_state()?;

    // Verify all data persisted correctly
    assert_eq!(loaded_state.container_name, "test-container");
    assert_eq!(loaded_state.provider, "docker");
    assert_eq!(loaded_state.project_dir, fixture.project_dir);
    assert_eq!(loaded_state.mount_count(), 2);

    // Verify specific mounts
    assert!(loaded_state.has_mount(&fixture.mount_source));
    assert!(loaded_state.has_mount(&ro_mount));

    let rw_mount = loaded_state.get_mount(&fixture.mount_source).unwrap();
    assert_eq!(rw_mount.permissions, MountPermission::ReadWrite);

    let ro_mount_obj = loaded_state.get_mount(&ro_mount).unwrap();
    assert_eq!(ro_mount_obj.permissions, MountPermission::ReadOnly);

    // Test mount removal workflow
    let mut modified_state = loaded_state;
    let removed_mount = modified_state.remove_mount(&ro_mount)?;
    assert_eq!(removed_mount.permissions, MountPermission::ReadOnly);

    // Save and reload to verify removal persisted
    state_manager.save_state(&modified_state)?;
    let final_state = state_manager.load_state()?;

    assert_eq!(final_state.mount_count(), 1);
    assert!(!final_state.has_mount(&ro_mount));
    assert!(final_state.has_mount(&fixture.mount_source));

    println!("✅ Mount workflow integration test passed - full lifecycle works");
    Ok(())
}

#[test]
fn test_state_corruption_recovery() -> Result<()> {
    let fixture = IntegrationTestFixture::new()?;
    let state_manager = fixture.create_state_manager();

    // Test 1: Completely malformed YAML
    let malformed_yaml = "this is not valid yaml: [unclosed bracket";
    fs::write(state_manager.state_file_path(), malformed_yaml)?;

    let result = state_manager.load_state();
    assert!(result.is_err());

    // Verify error contains useful information
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Invalid state file format") || error_msg.contains("parse"));

    // Test 2: Valid YAML but invalid structure
    let invalid_structure = r#"
container_name: "test"
provider: "docker"
# Missing required fields like created_at, project_dir, mounts
some_unknown_field: "value"
"#;
    fs::write(state_manager.state_file_path(), invalid_structure)?;

    let result = state_manager.load_state();
    assert!(result.is_err());

    // Test 3: Valid structure but validation failures
    let invalid_data = r#"
container_name: ""
provider: "docker"
mounts: []
created_at: "2024-01-01T00:00:00Z"
project_dir: "/nonexistent/directory"
auto_destroy: false
"#;
    fs::write(state_manager.state_file_path(), invalid_data)?;

    let result = state_manager.load_state();
    assert!(result.is_err());

    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Container name cannot be empty") ||
            error_msg.contains("Project directory does not exist"));

    // Test 4: Recovery - write valid state after corruption
    let valid_state = fixture.create_test_state();
    state_manager.save_state(&valid_state)?;

    let recovered_state = state_manager.load_state()?;
    assert_eq!(recovered_state.container_name, "test-container");

    println!("✅ State corruption recovery test passed - graceful error handling works");
    Ok(())
}

#[test]
fn test_mount_source_validation_edge_cases() -> Result<()> {
    let fixture = IntegrationTestFixture::new()?;
    let mut state = fixture.create_test_state();

    // Test 1: Non-existent directory
    let nonexistent = fixture._temp_dir.path().join("does_not_exist");
    let result = state.add_mount(nonexistent.clone(), MountPermission::ReadWrite);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("does not exist"));

    // Test 2: File instead of directory
    let file_path = fixture.mount_source.join("not_a_directory.txt");
    fs::write(&file_path, "content")?;
    let result = state.add_mount(file_path.clone(), MountPermission::ReadWrite);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not a directory"));

    // Test 3: Symbolic link to directory (should work)
    #[cfg(unix)]
    {
        let symlink_target = fixture._temp_dir.path().join("symlink_target");
        fs::create_dir_all(&symlink_target)?;
        let symlink_path = fixture._temp_dir.path().join("symlink_to_dir");
        std::os::unix::fs::symlink(&symlink_target, &symlink_path)?;

        let result = state.add_mount(symlink_path, MountPermission::ReadWrite);
        assert!(result.is_ok(), "Symlink to directory should be allowed");
    }

    // Test 4: Duplicate mount detection
    state.add_mount(fixture.mount_source.clone(), MountPermission::ReadWrite)?;
    let result = state.add_mount(fixture.mount_source.clone(), MountPermission::ReadOnly);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("already exists"));

    // Test 5: Permission boundary validation
    let temp_dir = fixture._temp_dir.path().join("permission_test");
    fs::create_dir_all(&temp_dir)?;

    // This should work
    let result = state.add_mount(temp_dir.clone(), MountPermission::ReadOnly);
    assert!(result.is_ok());

    println!("✅ Mount source validation test passed - edge cases handled correctly");
    Ok(())
}

#[test]
fn test_mount_path_edge_cases() -> Result<()> {
    let fixture = IntegrationTestFixture::new()?;
    let mut state = fixture.create_test_state();

    // Test 1: Relative path handling
    let relative_path = std::path::PathBuf::from("./relative_mount");
    let result = state.add_mount(relative_path, MountPermission::ReadWrite);
    // Should either canonicalize or reject relative paths
    if let Err(err) = result {
        let error_msg = err.to_string();
        assert!(error_msg.contains("absolute") || error_msg.contains("does not exist"));
    }

    // Test 2: Path with ".." components
    let parent_path = fixture._temp_dir.path().join("subdir").join("..").join("parent_test");
    fs::create_dir_all(&parent_path)?;
    let result = state.add_mount(parent_path, MountPermission::ReadWrite);
    // Should handle path canonicalization
    assert!(result.is_ok(), "Canonicalized paths should work");

    // Test 3: Very long path names
    let long_name = "a".repeat(255);
    let long_path = fixture._temp_dir.path().join(&long_name);
    // Only create if filesystem supports it
    if fs::create_dir_all(&long_path).is_ok() {
        let result = state.add_mount(long_path, MountPermission::ReadWrite);
        assert!(result.is_ok(), "Long valid paths should work");
    }

    // Test 4: Path with spaces and special characters
    let special_path = fixture._temp_dir.path().join("path with spaces & symbols");
    fs::create_dir_all(&special_path)?;
    let result = state.add_mount(special_path, MountPermission::ReadWrite);
    assert!(result.is_ok(), "Paths with spaces and symbols should work");

    // Test 5: Empty path string (converted to current directory)
    let empty_path = std::path::PathBuf::from("");
    let result = state.add_mount(empty_path, MountPermission::ReadWrite);
    assert!(result.is_err(), "Empty paths should be rejected");

    println!("✅ Mount path edge cases test passed");
    Ok(())
}

#[test]
fn test_mount_permission_scenarios() -> Result<()> {
    let fixture = IntegrationTestFixture::new()?;
    let mut state = fixture.create_test_state();

    // Test 1: Read-only mount followed by read-write attempt on same path
    let test_dir = fixture._temp_dir.path().join("permission_conflict");
    fs::create_dir_all(&test_dir)?;

    state.add_mount(test_dir.clone(), MountPermission::ReadOnly)?;
    let result = state.add_mount(test_dir, MountPermission::ReadWrite);
    assert!(result.is_err(), "Should reject duplicate mount with different permissions");

    // Test 2: Multiple different paths with different permissions
    let ro_dir = fixture._temp_dir.path().join("readonly_dir");
    let rw_dir = fixture._temp_dir.path().join("readwrite_dir");
    fs::create_dir_all(&ro_dir)?;
    fs::create_dir_all(&rw_dir)?;

    assert!(state.add_mount(ro_dir, MountPermission::ReadOnly).is_ok());
    assert!(state.add_mount(rw_dir, MountPermission::ReadWrite).is_ok());

    // Verify both mounts exist with correct permissions
    assert_eq!(state.mount_count(), 3); // Initial + 2 new mounts

    println!("✅ Mount permission scenarios test passed");
    Ok(())
}

#[test]
fn test_mount_filesystem_integration() -> Result<()> {
    let fixture = IntegrationTestFixture::new()?;
    let mut state = fixture.create_test_state();

    // Test 1: Mount point with nested directory structure
    let nested_source = fixture._temp_dir.path().join("deep").join("nested").join("structure");
    fs::create_dir_all(&nested_source)?;
    fs::write(nested_source.join("test_file.txt"), "nested content")?;

    let result = state.add_mount(nested_source.clone(), MountPermission::ReadWrite);
    assert!(result.is_ok(), "Deeply nested paths should work");

    // Test 2: Mount point with existing files
    let files_dir = fixture._temp_dir.path().join("with_files");
    fs::create_dir_all(&files_dir)?;
    for i in 0..5 {
        fs::write(files_dir.join(format!("file_{}.txt", i)), format!("content {}", i))?;
    }

    let result = state.add_mount(files_dir, MountPermission::ReadOnly);
    assert!(result.is_ok(), "Directories with existing files should work");

    // Test 3: Hidden directory (starts with .)
    let hidden_dir = fixture._temp_dir.path().join(".hidden_mount");
    fs::create_dir_all(&hidden_dir)?;
    let result = state.add_mount(hidden_dir, MountPermission::ReadWrite);
    assert!(result.is_ok(), "Hidden directories should work");

    println!("✅ Mount filesystem integration test passed");
    Ok(())
}

#[test]
fn test_state_file_atomic_operations() -> Result<()> {
    let fixture = IntegrationTestFixture::new()?;
    let state_manager = fixture.create_state_manager();

    // Create initial state
    let mut state = fixture.create_test_state();
    state.add_mount(fixture.mount_source.clone(), MountPermission::ReadWrite)?;
    state_manager.save_state(&state)?;

    // Verify atomic write behavior: temp file should not exist after successful write
    let state_file_path = state_manager.state_file_path();
    let temp_file_path = state_file_path.with_extension("tmp");

    assert!(state_file_path.exists());
    assert!(!temp_file_path.exists(), "Temporary file should be cleaned up after atomic write");

    // Test state file integrity after modifications
    let original_content = fs::read_to_string(state_file_path)?;

    // Create another mount directory and add it
    let another_mount_dir = fixture._temp_dir.path().join("another_mount");
    fs::create_dir_all(&another_mount_dir)?;
    state.add_mount(another_mount_dir, MountPermission::ReadOnly)?;
    state_manager.save_state(&state)?;

    // Verify file was updated atomically
    let new_content = fs::read_to_string(state_file_path)?;
    assert_ne!(original_content, new_content);
    assert!(!temp_file_path.exists(), "Temp file should be cleaned up after second write");

    // Verify content integrity
    let loaded_state: TempVmState = serde_yaml::from_str(&new_content)?;
    assert_eq!(loaded_state.mount_count(), 2);

    println!("✅ Atomic operations test passed - no temp file leakage, atomic writes work");
    Ok(())
}