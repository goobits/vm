use super::*;
use std::fs;
use vm_config::config::{ImageSpec, VmConfig, VmSettings};
use vm_config::detector::git::GitConfig;

#[test]
fn generated_images_are_marked_as_vm_managed() {
    assert!(include_str!("Dockerfile.j2").contains("LABEL com.vm.managed=\"true\""));
}

#[test]
fn image_pull_error_explains_unprivileged_nested_engines() {
    let stderr = "failed to register layer: unshare: operation not permitted";
    let message = BuildOperations::image_pull_error_message("ubuntu:jammy", stderr);

    assert!(message.contains("container engine cannot register image layers"));
    assert!(message.contains("unprivileged container"));
    assert!(message.contains("Run vm from the host machine"));
    assert!(message.contains(stderr));
}

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
    assert!(args.iter().any(|arg| arg == "--build-arg=NODE_VERSION=22"));
    assert!(args
        .iter()
        .any(|arg| arg == "--build-arg=NVM_VERSION=v0.40.3"));
    assert!(args
        .iter()
        .any(|arg| arg == "--build-arg=PNPM_VERSION=10.12.3"));
    assert!(!args
        .iter()
        .any(|arg| arg.starts_with("--build-arg=VIBE_RUNTIME_REQUIRED=")));
    assert!(include_str!("Dockerfile.j2").contains(
        "source=shared/scripts/install-node-toolchain.sh,target=/tmp/install-node-toolchain.sh"
    ));
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
            image: Some(ImageSpec::String("@vibe-image".to_string())),
            ..Default::default()
        }),
        ..Default::default()
    };

    let temp_path = temp_dir.path().to_path_buf();
    let build_ops = BuildOperations::new(&config, &temp_path, "docker");
    let args = build_ops.gather_build_args("vm-snapshot/global/vibe-image:latest");

    assert!(args
        .iter()
        .any(|arg| arg == "--build-arg=BASE_PREPROVISIONED=true"));
    assert!(args
        .iter()
        .any(|arg| arg == "--build-arg=VIBE_RUNTIME_REQUIRED=true"));
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

    let mut other_snapshot = config.clone();
    other_snapshot.vm.as_mut().unwrap().image = Some(ImageSpec::String("@team-image".to_string()));
    let other_args = BuildOperations::new(&other_snapshot, &temp_path, "docker")
        .gather_build_args("vm-snapshot/global/team-image:latest");
    assert!(other_args
        .iter()
        .any(|arg| arg == "--build-arg=BASE_PREPROVISIONED=true"));
    assert!(!other_args
        .iter()
        .any(|arg| arg.starts_with("--build-arg=VIBE_RUNTIME_REQUIRED=")));
}

#[test]
fn generated_vibe_build_rejects_an_incomplete_codex_runtime() {
    let template = include_str!("Dockerfile.j2");

    assert!(template.contains("ARG VIBE_RUNTIME_REQUIRED=false"));
    assert!(template.contains("codex-package/bin/codex-code-mode-host"));
    assert!(template.contains("vm system base build vibe --provider docker"));
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
        image: Some(ImageSpec::String("@vibe-image".to_string())),
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
        .derived_image_tag(
            "vm-snapshot/global/vibe-image:latest",
            "sha256:shared-base",
            build_context.path(),
        )
        .unwrap();
    let tag_b = build_ops_b
        .derived_image_tag(
            "vm-snapshot/global/vibe-image:latest",
            "sha256:shared-base",
            build_context.path(),
        )
        .unwrap();

    assert_eq!(tag_a, tag_b);
}

#[test]
fn test_derived_image_tag_changes_when_base_image_is_rebuilt() {
    let build_context = tempfile::tempdir().unwrap();
    fs::write(
        build_context.path().join("Dockerfile.generated"),
        "FROM ubuntu:24.04\n",
    )
    .unwrap();
    fs::create_dir(build_context.path().join("shared")).unwrap();

    let config = VmConfig::default();
    let generated = tempfile::tempdir().unwrap();
    let build_ops = BuildOperations::new(&config, generated.path(), "docker");
    let old_tag = build_ops
        .derived_image_tag("example/base:latest", "sha256:old", build_context.path())
        .unwrap();
    let new_tag = build_ops
        .derived_image_tag("example/base:latest", "sha256:new", build_context.path())
        .unwrap();

    assert_ne!(old_tag, new_tag);
}
