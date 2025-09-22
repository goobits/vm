use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Test fixture for creating temporary project directories
pub struct ProjectTestFixture {
    _temp_dir: TempDir,
    project_dir: std::path::PathBuf,
}

impl ProjectTestFixture {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path().to_path_buf();

        Ok(Self {
            _temp_dir: temp_dir,
            project_dir,
        })
    }

    pub fn create_file(&self, name: &str, content: &str) -> Result<(), Box<dyn std::error::Error>> {
        fs::write(self.project_dir.join(name), content)?;
        Ok(())
    }

    pub fn create_dir(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(self.project_dir.join(name))?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.project_dir
    }
}

/// Convenience macro for creating a test fixture
/// Reduces boilerplate in test functions
#[macro_export]
macro_rules! test_fixture {
    () => {
        super::fixtures::ProjectTestFixture::new()
            .expect("Failed to create test fixture - check temp directory permissions and available disk space")
    };
}
