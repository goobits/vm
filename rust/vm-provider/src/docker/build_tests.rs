use super::*;
use vm_config::config::{VmConfig, VmSettings};
use vm_config::detector::git::GitConfig;

#[test]
fn test_gather_build_args_host_integration() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut config = VmConfig::default();
    config.git_config = Some(GitConfig {
        user_name: Some("Test User".to_string()),
        user_email: Some("test@example.com".to_string()),
        ..Default::default()
    });
    config.vm = Some(VmSettings {
        timezone: Some("America/New_York".to_string()),
        ..Default::default()
    });

    let temp_path = temp_dir.path().to_path_buf();
    let build_ops = BuildOperations::new(&config, &temp_path);
    let args = build_ops.gather_build_args();

    assert!(args
        .iter()
        .any(|arg| arg == "--build-arg=GIT_USER_NAME=Test User"));
    assert!(args
        .iter()
        .any(|arg| arg == "--build-arg=GIT_USER_EMAIL=test@example.com"));
    assert!(args
        .iter()
        .any(|arg| arg == "--build-arg=TZ=America/New_York"));
}
