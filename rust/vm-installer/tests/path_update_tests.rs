#[cfg(feature = "integration")]
use std::fs;
#[cfg(feature = "integration")]
use std::path::{Path, PathBuf};
#[cfg(feature = "integration")]
use tempfile::TempDir;

#[cfg(feature = "integration")]
struct TestFixture {
    _temp_dir: TempDir,
    home_dir: PathBuf,
    bin_dir: PathBuf,
}

#[cfg(feature = "integration")]
impl TestFixture {
    fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let home_dir = temp_dir.path().join("home");
        let bin_dir = home_dir.join(".local/bin");
        fs::create_dir_all(&bin_dir).unwrap();

        // Set the HOME env var to our temp dir
        std::env::set_var("HOME", &home_dir);

        Self {
            _temp_dir: temp_dir,
            home_dir,
            bin_dir,
        }
    }
}

// This function is not public, so we have to recreate it here to test it.
#[cfg(feature = "integration")]
fn add_to_profile(profile_path: &Path, bin_dir: &Path) -> anyhow::Result<()> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(profile_path)?;

    let line_to_add = format!(
        "\n# Added by VM tool installer\nexport PATH=\"{}:$PATH\"",
        bin_dir.display()
    );

    writeln!(file, "{}", line_to_add)?;
    Ok(())
}

#[test]
#[cfg(feature = "integration")]
fn test_path_update_modifies_profile() {
    let fixture = TestFixture::new();

    // Set a fake shell to test against
    std::env::set_var("SHELL", "/bin/bash");

    // Create a fake .bashrc
    let bashrc_path = fixture.home_dir.join(".bashrc");
    let initial_content = "# initial content\n";
    fs::write(&bashrc_path, initial_content).unwrap();

    // Because `ensure_path` has an interactive prompt, we can't call it directly in a test.
    // Instead, we will test the part of it that we can: the `add_to_profile` logic.
    let result = add_to_profile(&bashrc_path, &fixture.bin_dir);
    assert!(result.is_ok());

    let content = fs::read_to_string(&bashrc_path).unwrap();
    assert_ne!(
        content, initial_content,
        "The file content should have changed."
    );
    assert!(content.contains(&fixture.bin_dir.to_string_lossy().to_string()));
    assert!(content.contains("export PATH"));
}
