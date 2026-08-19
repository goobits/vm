use std::path::{Component, Path, PathBuf};

pub(super) fn is_environment_container(name: &str, role: &str, compose_service: &str) -> bool {
    if role == "environment" {
        return true;
    }
    if !role.is_empty() {
        return false;
    }

    // Backwards compatibility for containers created before com.vm.role was
    // introduced. The primary Compose service shares the environment's full
    // container name; database and package-edge services do not.
    name.ends_with("-dev") && (compose_service.is_empty() || compose_service == name)
}

pub(super) fn config_path_from_inspect(container: &serde_json::Value) -> Option<PathBuf> {
    let labels = &container["Config"]["Labels"];
    let name = container["Name"]
        .as_str()
        .unwrap_or_default()
        .trim_start_matches('/');
    let role = labels["com.vm.role"].as_str().unwrap_or_default();
    let compose_service = labels["com.docker.compose.service"]
        .as_str()
        .unwrap_or_default();
    if labels["com.vm.managed"].as_str() != Some("true")
        || !is_environment_container(name, role, compose_service)
    {
        return None;
    }

    if let Some(path) = labels["com.vm.config-path"].as_str() {
        let path = PathBuf::from(path);
        if path.is_absolute() && path.is_file() {
            return Some(path);
        }
    }

    let workspace = container["Config"]["WorkingDir"]
        .as_str()
        .filter(|path| path.starts_with('/'))
        .unwrap_or("/workspace");
    let source = container["Mounts"].as_array().and_then(|mounts| {
        mounts.iter().find_map(|mount| {
            if mount["Type"].as_str() == Some("bind")
                && mount["Destination"].as_str() == Some(workspace)
            {
                mount["Source"].as_str()
            } else {
                None
            }
        })
    })?;
    config_path_from_workspace_source(source, labels["com.vm.project"].as_str())
}

fn config_path_from_workspace_source(source: &str, project: Option<&str>) -> Option<PathBuf> {
    let source = Path::new(source);
    if let Some(config) = config_path_below_workspace(source, project) {
        return Some(config);
    }

    // Docker Desktop reports macOS bind sources below /host_mnt even though
    // controller commands resolve them through their native absolute paths.
    source
        .strip_prefix("/host_mnt")
        .ok()
        .map(|relative| Path::new("/").join(relative))
        .and_then(|source| config_path_below_workspace(&source, project))
}

fn config_path_below_workspace(source: &Path, project: Option<&str>) -> Option<PathBuf> {
    if !source.is_absolute() {
        return None;
    }
    let direct = source.join("vm.yaml");
    if direct.is_file() {
        return Some(direct);
    }

    let project = project.filter(|project| {
        let mut components = Path::new(project).components();
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
    })?;
    let nested = source.join(project).join("vm.yaml");
    nested.is_file().then_some(nested)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_filter_excludes_managed_compose_services() {
        assert!(is_environment_container(
            "demo-dev",
            "environment",
            "demo-dev"
        ));
        assert!(is_environment_container("demo-dev", "", "demo-dev"));
        assert!(!is_environment_container("demo-postgres", "", "postgres"));
        assert!(!is_environment_container(
            "demo-package-edge",
            "",
            "package-edge"
        ));
        assert!(!is_environment_container("demo-dev", "service", "demo-dev"));
    }

    #[test]
    fn managed_instance_recovers_its_owning_configuration() {
        let project = tempfile::tempdir().unwrap();
        let config = project.path().join("vm.yaml");
        std::fs::write(&config, "project:\n  name: demo\n").unwrap();
        let labeled = serde_json::json!({
            "Name": "/demo-dev",
            "Config": {
                "Labels": {
                    "com.vm.managed": "true",
                    "com.vm.role": "environment",
                    "com.vm.config-path": config,
                },
                "WorkingDir": "/workspace"
            },
            "Mounts": []
        });
        assert_eq!(
            config_path_from_inspect(&labeled),
            Some(project.path().join("vm.yaml"))
        );

        let legacy = serde_json::json!({
            "Name": "/demo-dev",
            "Config": {
                "Labels": {
                    "com.vm.managed": "true",
                    "com.docker.compose.service": "demo-dev"
                },
                "WorkingDir": "/workspace"
            },
            "Mounts": [{
                "Type": "bind",
                "Source": project.path(),
                "Destination": "/workspace"
            }]
        });
        assert_eq!(
            config_path_from_inspect(&legacy),
            Some(project.path().join("vm.yaml"))
        );

        let workspace_root = tempfile::tempdir().unwrap();
        let nested_project = workspace_root.path().join("vm");
        std::fs::create_dir(&nested_project).unwrap();
        std::fs::write(nested_project.join("vm.yaml"), "project:\n  name: vm\n").unwrap();
        let legacy_workspace_root = serde_json::json!({
            "Name": "/vm-dev",
            "Config": {
                "Labels": {
                    "com.vm.managed": "true",
                    "com.vm.project": "vm",
                    "com.docker.compose.service": "vm-dev"
                },
                "WorkingDir": "/workspace"
            },
            "Mounts": [{
                "Type": "bind",
                "Source": workspace_root.path(),
                "Destination": "/workspace"
            }]
        });
        assert_eq!(
            config_path_from_inspect(&legacy_workspace_root),
            Some(nested_project.join("vm.yaml"))
        );

        let docker_desktop = serde_json::json!({
            "Name": "/demo-dev",
            "Config": {
                "Labels": {
                    "com.vm.managed": "true",
                    "com.docker.compose.service": "demo-dev"
                },
                "WorkingDir": "/workspace"
            },
            "Mounts": [{
                "Type": "bind",
                "Source": format!("/host_mnt{}", project.path().display()),
                "Destination": "/workspace"
            }]
        });
        assert_eq!(
            config_path_from_inspect(&docker_desktop),
            Some(project.path().join("vm.yaml"))
        );
        assert_eq!(
            config_path_below_workspace(Path::new("relative"), None),
            None
        );
    }
}
