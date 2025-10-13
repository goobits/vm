use crate::detector::{git, os};
use std::env;
use tempfile::TempDir;

struct HomeGuard {
    _temp_dir: TempDir,
    original_home: Option<String>,
}

impl HomeGuard {
    fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let original_home = env::var("HOME").ok();
        env::set_var("HOME", temp_dir.path());
        Self {
            _temp_dir: temp_dir,
            original_home,
        }
    }
}

impl Drop for HomeGuard {
    fn drop(&mut self) {
        if let Some(home) = &self.original_home {
            env::set_var("HOME", home);
        } else {
            env::remove_var("HOME");
        }
    }
}

#[test]
fn test_detect_git_config_no_config() {
    let _guard = HomeGuard::new();
    let config = git::detect_git_config().unwrap();
    assert_eq!(config.user_name, None);
    assert_eq!(config.user_email, None);
}

#[test]
fn test_detect_timezone_fallback() {
    let timezone = os::detect_timezone();
    assert!(!timezone.is_empty());
}
