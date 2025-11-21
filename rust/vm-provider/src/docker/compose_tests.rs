#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use vm_config::{GlobalConfig, config::VmConfig};

    #[test]
    fn test_package_registry_env_vars_injection() {
        // Create a temporary directory
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().to_path_buf();
        let build_dir = temp_dir.path().join("build");
        std::fs::create_dir_all(&build_dir).unwrap();

        // Create a minimal VmConfig
        let vm_config = VmConfig {
            project: Some(vm_config::config::ProjectConfig {
                name: Some("test-project".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Create GlobalConfig with package registry enabled
        let global_config = GlobalConfig {
            services: vm_config::global_config::GlobalServices {
                package_registry: vm_config::global_config::PackageRegistrySettings {
                    enabled: true,
                    port: 3080,
                    max_storage_gb: 10,
                },
                ..Default::default()
            },
            ..Default::default()
        };

        // Create ProviderContext with global config
        let context = ProviderContext::default().with_config(global_config);

        // Create ComposeOperations
        let compose_ops = ComposeOperations::new(
            &vm_config,
            &temp_dir.path().to_path_buf(),
            &project_dir,
            "docker",
        );

        // Render docker-compose
        let result = compose_ops.render_docker_compose(&build_dir, &context);
        assert!(result.is_ok(), "render_docker_compose should succeed");

        let content = result.unwrap();

        // Verify that environment variables are in the rendered output
        let host = vm_platform::platform::get_host_gateway();

        assert!(
            content.contains(&format!("NPM_CONFIG_REGISTRY=http://{}:3080/npm/", host)),
            "Should contain NPM_CONFIG_REGISTRY"
        );
        assert!(
            content.contains(&format!("PIP_INDEX_URL=http://{}:3080/pypi/simple/", host)),
            "Should contain PIP_INDEX_URL"
        );
        assert!(
            content.contains("PIP_EXTRA_INDEX_URL=https://pypi.org/simple/"),
            "Should contain PIP_EXTRA_INDEX_URL for fallback"
        );
        assert!(
            content.contains(&format!("PIP_TRUSTED_HOST={}", host)),
            "Should contain PIP_TRUSTED_HOST"
        );
        assert!(
            content.contains(&format!("VM_CARGO_REGISTRY_HOST={}", host)),
            "Should contain VM_CARGO_REGISTRY_HOST"
        );
        assert!(
            content.contains("VM_CARGO_REGISTRY_PORT=3080"),
            "Should contain VM_CARGO_REGISTRY_PORT"
        );
    }

    #[test]
    fn test_package_registry_disabled_no_env_vars() {
        // Create a temporary directory
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().to_path_buf();
        let build_dir = temp_dir.path().join("build");
        std::fs::create_dir_all(&build_dir).unwrap();

        // Create a minimal VmConfig
        let vm_config = VmConfig {
            project: Some(vm_config::config::ProjectConfig {
                name: Some("test-project".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Create GlobalConfig with package registry DISABLED
        let global_config = GlobalConfig {
            services: vm_config::global_config::GlobalServices {
                package_registry: vm_config::global_config::PackageRegistrySettings {
                    enabled: false,
                    port: 3080,
                    max_storage_gb: 10,
                },
                ..Default::default()
            },
            ..Default::default()
        };

        // Create ProviderContext with global config
        let context = ProviderContext::default().with_config(global_config);

        // Create ComposeOperations
        let compose_ops = ComposeOperations::new(
            &vm_config,
            &temp_dir.path().to_path_buf(),
            &project_dir,
            "docker",
        );

        // Render docker-compose
        let result = compose_ops.render_docker_compose(&build_dir, &context);
        assert!(result.is_ok(), "render_docker_compose should succeed");

        let content = result.unwrap();

        // Verify that registry environment variables are NOT in the rendered output
        assert!(
            !content.contains("NPM_CONFIG_REGISTRY="),
            "Should NOT contain NPM_CONFIG_REGISTRY when disabled"
        );
        assert!(
            !content.contains("VM_CARGO_REGISTRY_HOST="),
            "Should NOT contain VM_CARGO_REGISTRY_HOST when disabled"
        );
    }

    #[test]
    fn test_no_global_config_no_env_vars() {
        // Create a temporary directory
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().to_path_buf();
        let build_dir = temp_dir.path().join("build");
        std::fs::create_dir_all(&build_dir).unwrap();

        // Create a minimal VmConfig
        let vm_config = VmConfig {
            project: Some(vm_config::config::ProjectConfig {
                name: Some("test-project".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Create ProviderContext WITHOUT global config
        let context = ProviderContext::default();

        // Create ComposeOperations
        let compose_ops = ComposeOperations::new(
            &vm_config,
            &temp_dir.path().to_path_buf(),
            &project_dir,
            "docker",
        );

        // Render docker-compose
        let result = compose_ops.render_docker_compose(&build_dir, &context);
        assert!(result.is_ok(), "render_docker_compose should succeed");

        let content = result.unwrap();

        // Verify that registry environment variables are NOT in the rendered output
        assert!(
            !content.contains("NPM_CONFIG_REGISTRY="),
            "Should NOT contain NPM_CONFIG_REGISTRY when no global config"
        );
        assert!(
            !content.contains("VM_CARGO_REGISTRY_HOST="),
            "Should NOT contain VM_CARGO_REGISTRY_HOST when no global config"
        );
    }

    #[test]
    fn test_host_gateway_detection() {
        let host = vm_platform::platform::get_host_gateway();

        #[cfg(target_os = "linux")]
        assert_eq!(host, "172.17.0.1", "Linux should use Docker bridge IP");

        #[cfg(not(target_os = "linux"))]
        assert_eq!(host, "host.docker.internal", "macOS/Windows should use host.docker.internal");
    }
}
