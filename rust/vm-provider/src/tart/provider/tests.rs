use super::TartProvider;
use crate::{tart_base, ProvisioningProvider};
use vm_config::config::{ImageSpec, ProjectConfig, TartConfig, VmConfig, VmSettings};

fn provider(mut config: VmConfig) -> TartProvider {
    config
        .tart
        .get_or_insert_with(TartConfig::default)
        .storage_path
        .get_or_insert_with(|| "/tmp/vm-provider-tests-tart".to_string());
    TartProvider::from_config(config).unwrap()
}

#[test]
fn managed_linux_alias_resolves_to_the_versioned_cache() {
    let provider = provider(VmConfig::default());
    let config = VmConfig {
        vm: Some(VmSettings {
            image: Some(ImageSpec::String(tart_base::LINUX_NAME.to_string())),
            ..Default::default()
        }),
        ..Default::default()
    };

    assert_eq!(
        provider.get_tart_image(&config).unwrap(),
        tart_base::versioned_cache_name()
    );
}

#[test]
fn tart_stop_only_runs_for_running_state() {
    assert!(TartProvider::tart_state_requires_stop(Some("running")));
    assert!(!TartProvider::tart_state_requires_stop(Some("stopped")));
    assert!(!TartProvider::tart_state_requires_stop(Some("suspended")));
    assert!(!TartProvider::tart_state_requires_stop(None));
}

#[test]
fn high_tart_allocations_are_detected_for_host_headroom_warning() {
    assert!(TartProvider::uses_most_of_host(Some(6), None, 8, 16 * 1024));
    assert!(TartProvider::uses_most_of_host(
        None,
        Some(12 * 1024),
        8,
        16 * 1024
    ));
    assert!(!TartProvider::uses_most_of_host(
        Some(4),
        Some(8 * 1024),
        8,
        16 * 1024
    ));
}

#[test]
fn host_workspace_path_uses_loaded_config_parent() {
    let outer = tempfile::tempdir().unwrap();
    let project_dir = outer.path().join("workspace");
    std::fs::create_dir_all(&project_dir).unwrap();
    let config_path = project_dir.join("vm.yaml");
    std::fs::write(&config_path, "provider: tart\n").unwrap();

    let provider = provider(VmConfig {
        source_path: Some(config_path),
        ..Default::default()
    });

    let resolved = provider.host_workspace_path().unwrap();
    assert_eq!(resolved, project_dir.canonicalize().unwrap());
}

#[test]
fn host_workspace_path_skips_outer_workspace_wrapper() {
    let temp_dir = tempfile::tempdir().unwrap();
    let outer_workspace = temp_dir.path().join("workspace");
    let inner_workspace = outer_workspace.join("workspace");
    std::fs::create_dir_all(&inner_workspace).unwrap();
    std::fs::write(inner_workspace.join("vm.yaml"), "provider: tart\n").unwrap();

    let resolved = TartProvider::normalize_host_workspace_path(&outer_workspace).unwrap();
    assert_eq!(resolved, inner_workspace.canonicalize().unwrap());
}

#[test]
fn host_workspace_path_keeps_real_project_named_workspace() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().join("workspace");
    std::fs::create_dir_all(workspace.join("workspace")).unwrap();
    std::fs::write(workspace.join("vm.yaml"), "provider: tart\n").unwrap();

    let resolved = TartProvider::normalize_host_workspace_path(&workspace).unwrap();
    assert_eq!(resolved, workspace.canonicalize().unwrap());
}

#[test]
fn macos_guest_uses_writable_default_workspace() {
    let provider = provider(VmConfig {
        project: Some(ProjectConfig {
            workspace_path: Some("/workspace".to_string()),
            ..Default::default()
        }),
        tart: Some(TartConfig {
            guest_os: Some("macos".to_string()),
            ssh_user: Some("admin".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    });

    assert_eq!(provider.get_sync_directory(), "/Users/admin/workspace");
}

#[test]
fn linux_guest_keeps_default_workspace() {
    let provider = provider(VmConfig {
        project: Some(ProjectConfig {
            workspace_path: Some("/workspace".to_string()),
            ..Default::default()
        }),
        tart: Some(TartConfig {
            guest_os: Some("linux".to_string()),
            ssh_user: Some("admin".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    });

    assert_eq!(provider.get_sync_directory(), "/workspace");
}

#[test]
fn macos_guest_respects_custom_workspace() {
    let provider = provider(VmConfig {
        project: Some(ProjectConfig {
            workspace_path: Some("/Volumes/work/project".to_string()),
            ..Default::default()
        }),
        tart: Some(TartConfig {
            guest_os: Some("macos".to_string()),
            ssh_user: Some("admin".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    });

    assert_eq!(provider.get_sync_directory(), "/Volumes/work/project");
}

#[test]
fn tart_run_includes_nested_flag_when_configured() {
    let provider = provider(VmConfig {
        tart: Some(TartConfig {
            nested: Some(true),
            guest_os: Some("linux".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    });

    assert!(provider
        .build_run_args("vm-mac", &[])
        .iter()
        .any(|argument| argument == "--nested"));
}

#[test]
fn tart_run_omits_nested_flag_for_macos_guests() {
    let provider = provider(VmConfig {
        tart: Some(TartConfig {
            nested: Some(true),
            guest_os: Some("macos".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    });

    assert!(!provider
        .build_run_args("vm-mac", &[])
        .iter()
        .any(|argument| argument == "--nested"));
}

#[test]
fn tart_run_arguments_preserve_directory_paths_without_shell_parsing() {
    let provider = provider(VmConfig::default());
    let args = provider.build_run_args(
        "vm-mac",
        &["/Users/me/project with spaces:tag=workspace".to_string()],
    );

    assert_eq!(args[0..3], ["tart", "run", "--no-graphics"]);
    assert!(args
        .windows(2)
        .any(|pair| pair == ["--dir", "/Users/me/project with spaces:tag=workspace"]));
    assert_eq!(args.last().map(String::as_str), Some("vm-mac"));
}
