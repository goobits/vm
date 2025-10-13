#[cfg(feature = "integration")]
use std::fs;
#[cfg(feature = "integration")]
use std::path::PathBuf;
#[cfg(feature = "integration")]
use tempfile::TempDir;
#[cfg(feature = "integration")]
use vm_core::user_paths;
#[cfg(feature = "integration")]
use vm_platform::platform::executable_name;

#[cfg(feature = "integration")]
struct TestFixture {
    _temp_dir: TempDir,
    project_root: PathBuf,
}

#[cfg(feature = "integration")]
impl TestFixture {
    fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path().join("test-project");

        // Set up a fake project root that the installer can find
        let rust_dir = project_root.join("rust");
        fs::create_dir_all(&rust_dir).unwrap();
        fs::write(rust_dir.join("Cargo.toml"), "[package]\nname = \"vm\"").unwrap();

        // Also need to trick the current_exe() call
        let fake_exe_dir = project_root.join("rust/target/debug");
        fs::create_dir_all(&fake_exe_dir).unwrap();
        let fake_exe_path = fake_exe_dir.join(executable_name("vm-installer"));
        fs::write(&fake_exe_path, "fake installer binary").unwrap();
        std::env::set_current_dir(&fake_exe_dir).unwrap();

        // Set user bin dir to our temp bin dir
        std::env::set_var("HOME", temp_dir.path());

        Self {
            _temp_dir: temp_dir,
            project_root,
        }
    }
}

#[test]
#[cfg(feature = "integration")]
fn test_install_symlink_creation() {
    let fixture = TestFixture::new();

    // Create a fake binary that the installer would "build"
    let release_dir = fixture
        .project_root
        .join("rust/target-test-os-arch/release");
    fs::create_dir_all(&release_dir).unwrap();
    let source_binary_path = release_dir.join(executable_name("vm"));
    fs::write(&source_binary_path, "fake vm binary").unwrap();

    // To avoid running a real `cargo build`, we can't call `installer::install` directly.
    // However, we can test the `create_symlink` logic it calls.
    // This is a compromise because `install` is not easily testable.
    let user_bin = user_paths::user_bin_dir().unwrap();
    let result = vm_platform::current().install_executable(&source_binary_path, &user_bin, "vm");

    assert!(result.is_ok(), "install_executable should succeed");

    let expected_symlink = user_bin.join(executable_name("vm"));
    assert!(
        expected_symlink.exists(),
        "The symlink should be created in the user's bin directory"
    );
}
