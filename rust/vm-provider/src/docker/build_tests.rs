use super::*;
use std::fs;
use vm_config::config::{BoxSpec, VmConfig, VmSettings};
use vm_config::detector::git::GitConfig;

#[test]
fn test_gather_build_args_host_integration() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config = VmConfig {
        git_config: Some(GitConfig {
            user_name: Some("Test User".to_string()),
            user_email: Some("test@example.com".to_string()),
            ..Default::default()
        }),
        vm: Some(VmSettings {
            timezone: Some("America/New_York".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    };

    let temp_path = temp_dir.path().to_path_buf();
    let build_ops = BuildOperations::new(&config, &temp_path, "docker");
    let args = build_ops.gather_build_args("ubuntu:24.04");

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

#[test]
fn test_gather_build_args_snapshot_omits_host_specific_inputs() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config = VmConfig {
        git_config: Some(GitConfig {
            user_name: Some("Test User".to_string()),
            user_email: Some("test@example.com".to_string()),
            ..Default::default()
        }),
        vm: Some(VmSettings {
            timezone: Some("America/New_York".to_string()),
            r#box: Some(BoxSpec::String("@vibe-box".to_string())),
            ..Default::default()
        }),
        ..Default::default()
    };

    let temp_path = temp_dir.path().to_path_buf();
    let build_ops = BuildOperations::new(&config, &temp_path, "docker");
    let args = build_ops.gather_build_args("vm-snapshot/global/vibe-box:latest");

    assert!(args
        .iter()
        .any(|arg| arg == "--build-arg=BASE_PREPROVISIONED=true"));
    assert!(args
        .iter()
        .any(|arg| arg == "--build-arg=TZ=America/New_York"));
    assert!(!args
        .iter()
        .any(|arg| arg.starts_with("--build-arg=PROJECT_UID=")));
    assert!(!args
        .iter()
        .any(|arg| arg.starts_with("--build-arg=PROJECT_GID=")));
    assert!(!args
        .iter()
        .any(|arg| arg.starts_with("--build-arg=PROJECT_USER=")));
    assert!(!args
        .iter()
        .any(|arg| arg.starts_with("--build-arg=GIT_USER_NAME=")));
    assert!(!args
        .iter()
        .any(|arg| arg.starts_with("--build-arg=GIT_USER_EMAIL=")));
}

#[test]
fn test_derived_image_tag_snapshot_ignores_host_identity_inputs() {
    let build_context = tempfile::tempdir().unwrap();
    fs::write(
        build_context.path().join("Dockerfile.generated"),
        "FROM ubuntu:24.04\n",
    )
    .unwrap();
    fs::create_dir(build_context.path().join("shared")).unwrap();

    let snapshot_vm = VmSettings {
        timezone: Some("America/New_York".to_string()),
        r#box: Some(BoxSpec::String("@vibe-box".to_string())),
        ..Default::default()
    };

    let config_a = VmConfig {
        git_config: Some(GitConfig {
            user_name: Some("User A".to_string()),
            user_email: Some("a@example.com".to_string()),
            ..Default::default()
        }),
        vm: Some(snapshot_vm.clone()),
        ..Default::default()
    };
    let config_b = VmConfig {
        git_config: Some(GitConfig {
            user_name: Some("User B".to_string()),
            user_email: Some("b@example.com".to_string()),
            ..Default::default()
        }),
        vm: Some(snapshot_vm),
        ..Default::default()
    };

    let temp_a = tempfile::tempdir().unwrap();
    let temp_b = tempfile::tempdir().unwrap();
    let temp_path_a = temp_a.path().to_path_buf();
    let temp_path_b = temp_b.path().to_path_buf();
    let build_ops_a = BuildOperations::new(&config_a, &temp_path_a, "docker");
    let build_ops_b = BuildOperations::new(&config_b, &temp_path_b, "docker");

    let tag_a = build_ops_a
        .derived_image_tag("vm-snapshot/global/vibe-box:latest", build_context.path())
        .unwrap();
    let tag_b = build_ops_b
        .derived_image_tag("vm-snapshot/global/vibe-box:latest", build_context.path())
        .unwrap();

    assert_eq!(tag_a, tag_b);
}
