use super::*;
use crate::container::compose_model::container_architecture;
use tempfile::TempDir;
use vm_config::config::{
    ContainerLoggingConfig, CpuLimit, MemoryLimit, PackageEdgeConfig, ProjectConfig, StorageConfig,
    TmpfsMountConfig, VmConfig, VmSettings, VolumeMountConfig, VolumeRetention, VolumeScope,
};

fn setup_test_env() -> (TempDir, PathBuf, PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let project_dir = temp_dir.path().to_path_buf();
    let temp_path = temp_dir.path().to_path_buf();
    (temp_dir, project_dir, temp_path)
}

fn yaml_mapping<'a>(value: &'a serde_yaml_ng::Value, key: &str) -> &'a serde_yaml_ng::Mapping {
    value
        .get(key)
        .and_then(serde_yaml_ng::Value::as_mapping)
        .unwrap_or_else(|| panic!("missing YAML mapping: {key}"))
}

fn volume_mount<'a>(
    service: &'a serde_yaml_ng::Mapping,
    source: &str,
) -> &'a serde_yaml_ng::Mapping {
    service
        .get("volumes")
        .and_then(serde_yaml_ng::Value::as_sequence)
        .and_then(|mounts| {
            mounts.iter().find_map(|mount| {
                let mapping = mount.as_mapping()?;
                (mapping.get("source").and_then(serde_yaml_ng::Value::as_str) == Some(source))
                    .then_some(mapping)
            })
        })
        .unwrap_or_else(|| panic!("missing volume mount: {source}"))
}

#[test]
fn host_bind_paths_round_trip_as_one_yaml_scalar() {
    let temp_dir = TempDir::new().unwrap();
    let project_dir = temp_dir.path().join("project #1");
    let generated_dir = temp_dir.path().join("generated");
    std::fs::create_dir_all(&project_dir).unwrap();
    let config: VmConfig = serde_yaml_ng::from_str(
        r#"
provider: docker
project:
  name: quoted-bind
host_sync:
  worktrees:
    enabled: false
"#,
    )
    .unwrap();
    let compose = ComposeOperations::new(&config, &generated_dir, &project_dir, "docker");

    let rendered = compose
        .render_docker_compose(&project_dir, &ProviderContext::default())
        .unwrap();
    let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&rendered).unwrap();
    let mounts = yaml["services"]["quoted-bind-dev"]["volumes"]
        .as_sequence()
        .unwrap();
    let expected = format!("{}:/workspace:rw", project_dir.display());

    assert!(mounts
        .iter()
        .any(|mount| mount.as_str() == Some(expected.as_str())));
}

#[test]
fn renders_stable_scoped_storage_and_runtime_policy() {
    let (_temp_dir, project_dir, temp_path) = setup_test_env();
    let mut volumes = indexmap::IndexMap::new();
    volumes.insert(
        "node_modules".to_string(),
        VolumeMountConfig {
            target: "/workspace/node_modules".to_string(),
            scope: VolumeScope::Instance,
            nocopy: true,
            retention: VolumeRetention::Keep,
        },
    );
    volumes.insert(
        "pnpm_store".to_string(),
        VolumeMountConfig {
            target: "/home/developer/.local/share/pnpm/store".to_string(),
            scope: VolumeScope::Platform,
            nocopy: true,
            retention: VolumeRetention::Keep,
        },
    );
    volumes.insert(
        "scratch".to_string(),
        VolumeMountConfig {
            target: "/var/cache/project".to_string(),
            scope: VolumeScope::Project,
            nocopy: true,
            retention: VolumeRetention::Disposable,
        },
    );
    let config = VmConfig {
        provider: Some("docker".into()),
        project: Some(ProjectConfig {
            name: Some("sketch-api".to_string()),
            ..Default::default()
        }),
        vm: Some(VmSettings {
            memory: Some(MemoryLimit::Limited(20_480)),
            cpus: Some(CpuLimit::Unlimited),
            pids_limit: Some(4096),
            stop_grace_period: Some(60),
            logging: Some(ContainerLoggingConfig::default()),
            ..Default::default()
        }),
        storage: StorageConfig {
            volumes,
            tmpfs: vec![TmpfsMountConfig {
                target: "/tmp".to_string(),
                size: MemoryLimit::Limited(4096),
                mode: "1777".to_string(),
            }],
        },
        ..Default::default()
    };
    let compose = ComposeOperations::new(&config, &temp_path, &project_dir, "docker");
    let context = ProviderContext::default();

    let rendered = compose
        .render_docker_compose(&project_dir, &context)
        .unwrap();
    assert_eq!(
        rendered,
        compose
            .render_docker_compose(&project_dir, &context)
            .unwrap(),
        "rendering the same project must be deterministic"
    );

    let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&rendered).unwrap();
    let services = yaml_mapping(&yaml, "services");
    let dev = services
        .get("sketch-api-dev")
        .and_then(serde_yaml_ng::Value::as_mapping)
        .unwrap();

    assert_eq!(
        dev.get("mem_limit").and_then(|value| value.as_str()),
        Some("20480m")
    );
    assert!(
        !dev.contains_key("cpus"),
        "unlimited CPUs must omit the limit"
    );
    assert_eq!(
        dev.get("pids_limit").and_then(|value| value.as_u64()),
        Some(4096)
    );
    assert_eq!(
        dev.get("stop_grace_period")
            .and_then(serde_yaml_ng::Value::as_str),
        Some("60s")
    );
    assert_eq!(
        dev.get("restart").and_then(|value| value.as_str()),
        Some("no")
    );
    let labels = dev
        .get("labels")
        .and_then(serde_yaml_ng::Value::as_mapping)
        .unwrap();
    assert_eq!(
        labels
            .get("com.vm.project")
            .and_then(serde_yaml_ng::Value::as_str),
        Some("sketch-api")
    );
    assert_eq!(
        labels
            .get("com.vm.instance")
            .and_then(serde_yaml_ng::Value::as_str),
        Some("sketch-api")
    );
    assert_eq!(
        labels
            .get("com.vm.role")
            .and_then(serde_yaml_ng::Value::as_str),
        Some("environment")
    );
    assert!(!labels.contains_key("com.vm.temporary"));

    let temp_state = TempVmState::new(
        "vm-temp-dev".to_string(),
        "docker".to_string(),
        project_dir.clone(),
        false,
    );
    let temp_rendered = compose
        .render_docker_compose_with_mounts(&temp_state)
        .unwrap();
    let temp_yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&temp_rendered).unwrap();
    let temp_labels = yaml_mapping(&temp_yaml, "services")["sketch-api-dev"]["labels"]
        .as_mapping()
        .unwrap();
    assert_eq!(
        temp_labels
            .get("com.vm.temporary")
            .and_then(serde_yaml_ng::Value::as_str),
        Some("true")
    );

    let logging = dev
        .get("logging")
        .and_then(serde_yaml_ng::Value::as_mapping)
        .unwrap();
    assert_eq!(
        logging.get("driver").and_then(|value| value.as_str()),
        Some("local")
    );
    let logging_options = logging
        .get("options")
        .and_then(serde_yaml_ng::Value::as_mapping)
        .unwrap();
    assert_eq!(
        logging_options
            .get("max-size")
            .and_then(serde_yaml_ng::Value::as_str),
        Some("20m")
    );
    assert_eq!(
        logging_options
            .get("max-file")
            .and_then(serde_yaml_ng::Value::as_str),
        Some("5")
    );

    let environment = dev
        .get("environment")
        .and_then(serde_yaml_ng::Value::as_sequence)
        .unwrap();
    assert!(environment
        .iter()
        .any(|value| { value.as_str() == Some("VM_IMAGE_IDENTITY=sketch-api:latest") }));
    assert!(environment.iter().any(|value| {
        value.as_str() == Some("PLAYWRIGHT_BROWSERS_PATH=/home/developer/.cache/ms-playwright")
    }));
    assert!(environment.iter().any(|value| {
        value.as_str() == Some("CARGO_TARGET_DIR=/home/developer/.cache/vm/cargo-target/sketch-api")
    }));
    assert!(environment.iter().any(|value| {
        value.as_str() == Some("npm_config_cache=/home/developer/.cache/node/npm")
    }));

    assert_eq!(
        dev.get("tmpfs")
            .and_then(serde_yaml_ng::Value::as_sequence)
            .unwrap(),
        &[serde_yaml_ng::Value::String(
            "/tmp:size=4096m,mode=1777".to_string()
        )]
    );
    assert!(
        dev.get("volumes")
            .and_then(serde_yaml_ng::Value::as_sequence)
            .unwrap()
            .iter()
            .filter_map(serde_yaml_ng::Value::as_str)
            .any(|mount| mount.ends_with(":/workspace:rw")),
        "/workspace must remain a host bind"
    );

    for (source, target) in [
        ("shell_history", "/home/developer/.shell_history"),
        ("managed_node_modules", "/workspace/node_modules"),
        (
            "managed_pnpm_store",
            "/home/developer/.local/share/pnpm/store",
        ),
    ] {
        let mount = volume_mount(dev, source);
        assert_eq!(
            mount.get("target").and_then(serde_yaml_ng::Value::as_str),
            Some(target)
        );
        assert_eq!(
            mount
                .get("volume")
                .and_then(serde_yaml_ng::Value::as_mapping)
                .and_then(|volume| volume.get("nocopy"))
                .and_then(serde_yaml_ng::Value::as_bool),
            Some(true)
        );
    }
    let tool_cache = volume_mount(dev, "tool_cache");
    assert_eq!(
        tool_cache
            .get("target")
            .and_then(serde_yaml_ng::Value::as_str),
        Some("/home/developer/.cache")
    );
    assert_eq!(
        tool_cache
            .get("volume")
            .and_then(serde_yaml_ng::Value::as_mapping)
            .and_then(|volume| volume.get("nocopy"))
            .and_then(serde_yaml_ng::Value::as_bool),
        Some(false)
    );

    let named_volumes = yaml_mapping(&yaml, "volumes");
    assert_eq!(
        named_volumes["shell_history"]
            .get("name")
            .and_then(serde_yaml_ng::Value::as_str),
        Some("vm_sketch-api_shell_history")
    );
    assert_eq!(
        named_volumes["managed_node_modules"]
            .get("name")
            .and_then(serde_yaml_ng::Value::as_str),
        Some("vm_sketch-api_node_modules")
    );
    let platform_store_name = format!(
        "vm_sketch-api_linux_{}_pnpm_store",
        container_architecture()
    );
    assert_eq!(
        named_volumes["managed_pnpm_store"]
            .get("name")
            .and_then(serde_yaml_ng::Value::as_str),
        Some(platform_store_name.as_str())
    );
    let tool_cache_name = format!(
        "vm_sketch-api_linux_{}_tool_cache",
        container_architecture()
    );
    assert_eq!(
        named_volumes["tool_cache"]
            .get("name")
            .and_then(serde_yaml_ng::Value::as_str),
        Some(tool_cache_name.as_str())
    );
    assert_eq!(
        named_volumes["managed_scratch"]["labels"]
            .get("com.vm.retention")
            .and_then(serde_yaml_ng::Value::as_str),
        Some("disposable")
    );

    let instance_rendered = compose
        .render_docker_compose_with_instance(&project_dir, "feature", &context)
        .unwrap();
    assert!(matches!(
        compose.render_docker_compose_with_instance(&project_dir, "a\"b", &context),
        Err(VmError::Validation(_))
    ));
    let instance_yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&instance_rendered).unwrap();
    let instance_volumes = yaml_mapping(&instance_yaml, "volumes");
    assert_eq!(
        instance_volumes["managed_node_modules"]
            .get("name")
            .and_then(serde_yaml_ng::Value::as_str),
        Some("vm_sketch-api-feature_node_modules")
    );
    assert_eq!(
        instance_volumes["managed_pnpm_store"]
            .get("name")
            .and_then(serde_yaml_ng::Value::as_str),
        Some(platform_store_name.as_str()),
        "platform stores remain shared across named instances"
    );
    assert_eq!(
        instance_volumes["tool_cache"]
            .get("name")
            .and_then(serde_yaml_ng::Value::as_str),
        Some(tool_cache_name.as_str()),
        "tool caches remain shared across named instances"
    );
    assert_eq!(
        instance_volumes["managed_scratch"]
            .get("name")
            .and_then(serde_yaml_ng::Value::as_str),
        Some("vm_sketch-api_scratch"),
        "project-scoped volumes remain shared across named instances"
    );
    assert_eq!(
        compose
            .instance_name_from_container("sketch-api-feature-dev")
            .as_deref(),
        Some("feature")
    );
    assert_eq!(compose.instance_name_from_container("sketch-api-dev"), None);
}

#[test]
fn renders_read_only_package_edge_without_blocking_the_worker() {
    let (_temp_dir, project_dir, generated_dir) = setup_test_env();
    let config = VmConfig {
        provider: Some("docker".into()),
        project: Some(ProjectConfig {
            name: Some("edge-test".into()),
            ..Default::default()
        }),
        package_edge: Some(PackageEdgeConfig {
            image: "registry.example/packages:1".into(),
            internal_gateway: "http://host.docker.internal:3080".into(),
            client_gateway: "http://package-edge:3080".into(),
            read_token: "read-token".into(),
            revision: "revision-1".into(),
        }),
        ..Default::default()
    };
    let rendered = ComposeOperations::new(&config, &generated_dir, &project_dir, "docker")
        .render_docker_compose(&project_dir, &ProviderContext::default())
        .unwrap();
    let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&rendered).unwrap();
    let services = yaml_mapping(&yaml, "services");
    let dev = services["edge-test-dev"].as_mapping().unwrap();
    let edge = services["package-edge"].as_mapping().unwrap();

    assert!(!dev.contains_key("depends_on"));
    assert!(dev["environment"]
        .as_sequence()
        .unwrap()
        .iter()
        .any(|value| value.as_str() == Some("VM_MANAGED_GUEST=1")));
    assert_eq!(edge["read_only"].as_bool(), Some(true));
    assert_eq!(edge["restart"].as_str(), Some("unless-stopped"));
    assert!(edge["environment"]
        .as_mapping()
        .unwrap()
        .contains_key("PKG_SERVER_INTERNAL_GATEWAY"));
    assert!(!edge["environment"]
        .as_mapping()
        .unwrap()
        .contains_key("PKG_SERVER_PUBLISH_TOKEN"));
    assert!(yaml_mapping(&yaml, "volumes").contains_key("package_edge_cache"));

    let preview = ComposeOperations::new(&config, &generated_dir, &project_dir, "docker")
        .render_docker_compose_preview(&project_dir, None, &ProviderContext::default())
        .unwrap();
    assert!(!preview.contains("read-token"));
}

#[test]
#[cfg(unix)]
fn package_edge_probe_requires_matching_running_revision() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let runtime = directory.path().join("runtime");
    std::fs::write(&runtime, "#!/bin/sh\nprintf 'running\\trevision-1\\n'\n").unwrap();
    let mut permissions = std::fs::metadata(&runtime).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&runtime, permissions).unwrap();

    assert!(package_edge_is_current(
        runtime.to_str().unwrap(),
        "demo-package-edge",
        "revision-1"
    ));
    assert!(!package_edge_is_current(
        runtime.to_str().unwrap(),
        "demo-package-edge",
        "revision-2"
    ));
}

#[test]
fn preview_redacts_environment_and_database_credentials() {
    let (_temp_dir, project_dir, generated_dir) = setup_test_env();
    let config: VmConfig = serde_yaml_ng::from_str(
        r#"
provider: docker
project:
  name: secret-project
environment:
  API_TOKEN: top-secret
host_sync:
  worktrees:
    enabled: false
services:
  postgresql:
    enabled: true
    port: 5432
"#,
    )
    .unwrap();
    let compose = ComposeOperations::new(&config, &generated_dir, &project_dir, "docker");

    let preview = compose
        .render_docker_compose_preview(&project_dir, None, &ProviderContext::default())
        .unwrap();

    assert!(!preview.contains("top-secret"));
    assert!(!preview.contains(project_dir.to_string_lossy().as_ref()));
    assert!(preview.contains("API_TOKEN=<redacted>"));
    assert!(preview.contains("DATABASE_URL=<redacted>"));
    assert!(preview.contains("<host-path>:/workspace:rw"));
}

#[test]
fn renders_configured_mounts_and_read_only_workspace_at_the_real_target() {
    let (_temp_dir, project_dir, generated_dir) = setup_test_env();
    std::fs::create_dir(project_dir.join("shared")).unwrap();
    let config: VmConfig = serde_yaml_ng::from_str(
        r#"
provider: docker
project:
  name: mounted-project
  workspace_path: /source
  workspace_access: read_only
mounts:
  - source: shared
    target: /packages/shared
    access: read_only
host_sync:
  worktrees:
    enabled: false
"#,
    )
    .unwrap();
    let compose = ComposeOperations::new(&config, &generated_dir, &project_dir, "docker");

    let rendered = compose
        .render_docker_compose(&project_dir, &ProviderContext::default())
        .unwrap();
    let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&rendered).unwrap();
    let dev = &yaml["services"]["mounted-project-dev"];
    assert_eq!(dev["working_dir"].as_str(), Some("/source"));
    let mounts = dev["volumes"].as_sequence().unwrap();
    assert!(mounts
        .iter()
        .filter_map(|mount| mount.as_str())
        .any(|mount| { mount == format!("{}:/source:ro", project_dir.display()) }));
    assert!(mounts
        .iter()
        .filter_map(|mount| mount.as_str())
        .any(|mount| {
            mount
                == format!(
                    "{}:/packages/shared:ro",
                    project_dir.join("shared").canonicalize().unwrap().display()
                )
        }));
    let dependency_mount = mounts
        .iter()
        .filter_map(serde_yaml_ng::Value::as_mapping)
        .find(|mount| mount["source"] == "workspace_node_modules")
        .unwrap();
    assert_eq!(dependency_mount["target"], "/source/node_modules");
}

#[test]
fn binds_all_published_ports_to_configured_address() {
    let (_temp_dir, project_dir, generated_dir) = setup_test_env();
    let config: VmConfig = serde_yaml_ng::from_str(
        r#"
provider: docker
project:
  name: bound-project
vm:
  port_binding: 127.0.0.1
ports:
  _range: [3360, 3361]
  mappings:
    - host: 4000
      guest: 80
services:
  postgresql:
    enabled: true
    port: 55432
host_sync:
  worktrees:
    enabled: false
"#,
    )
    .unwrap();
    let compose = ComposeOperations::new(&config, &generated_dir, &project_dir, "docker");

    let rendered = compose
        .render_docker_compose(&project_dir, &ProviderContext::default())
        .unwrap();
    let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&rendered).unwrap();
    let services = yaml_mapping(&yaml, "services");
    let dev_ports = services["bound-project-dev"]["ports"]
        .as_sequence()
        .unwrap();
    let postgres_ports = services["postgres"]["ports"].as_sequence().unwrap();

    assert!(dev_ports
        .iter()
        .any(|port| { port.as_str() == Some("127.0.0.1:4000:80") }));
    assert!(dev_ports
        .iter()
        .any(|port| { port.as_str() == Some("127.0.0.1:3360:3360") }));
    assert!(dev_ports
        .iter()
        .any(|port| { port.as_str() == Some("127.0.0.1:3361:3361") }));
    assert_eq!(
        postgres_ports,
        &[serde_yaml_ng::Value::String(
            "127.0.0.1:55432:5432".to_string()
        )]
    );
}
