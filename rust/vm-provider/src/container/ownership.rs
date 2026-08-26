use std::path::{Component, Path, PathBuf};
use std::process::Command;

use vm_core::error::{Result, VmError};

use super::{ContainerEngine, ContainerOps};
use crate::common::instance::{create_container_instance_info, InstanceInfo};

pub(super) fn list_instances(
    executable: &str,
    engine: ContainerEngine,
) -> Result<Vec<InstanceInfo>> {
    let output = Command::new(executable)
        .args([
            "ps",
            "-a",
            "--filter",
            "label=com.vm.managed=true",
            "--format",
            "{{.Names}}\t{{.ID}}\t{{.Status}}\t{{.CreatedAt}}\t{{.RunningFor}}\t{{.Label \"com.vm.project\"}}\t{{.Label \"com.vm.role\"}}\t{{.Label \"com.docker.compose.service\"}}",
        ])
        .output()
        .map_err(|error| {
            VmError::Internal(format!(
                "Failed to list containers with vm label using '{executable}': {error}"
            ))
        })?;

    if !output.status.success() {
        return Err(VmError::Internal(format!(
            "Container listing failed using '{}': {}",
            executable,
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let parts = line.split('\t').collect::<Vec<_>>();
            if parts.len() < 5 {
                return None;
            }
            let project = parts.get(5).copied().unwrap_or_default();
            let role = parts.get(6).copied().unwrap_or_default();
            let compose_service = parts.get(7).copied().unwrap_or_default();
            is_environment_container(parts[0], project, role, compose_service).then(|| {
                create_container_instance_info(
                    engine.name(),
                    parts[0],
                    parts[1],
                    parts[2],
                    Some(parts[3]),
                    Some(parts[4]),
                    parts
                        .get(5)
                        .filter(|project| !project.is_empty())
                        .map(|project| (*project).to_string()),
                )
            })
        })
        .collect())
}

pub(super) fn instance_config_path(executable: &str, instance: &str) -> Result<Option<PathBuf>> {
    let output = Command::new(executable)
        .args(["inspect", "--type", "container", instance])
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    let containers: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout)?;
    Ok(containers.first().and_then(config_path_from_inspect))
}

pub(super) fn reusable_host_ports(executable: &str, environment: &str) -> Result<Vec<u16>> {
    let mut ports = Vec::new();
    for service in ContainerOps::list_managed_service_containers(Some(executable), environment)? {
        let output = Command::new(executable)
            .args([
                "inspect",
                "--type",
                "container",
                "--format",
                "{{json .HostConfig.PortBindings}}",
                &service,
            ])
            .output()
            .map_err(|error| {
                VmError::Internal(format!(
                    "Failed to inspect managed service '{service}': {error}"
                ))
            })?;
        if !output.status.success() {
            return Err(VmError::Internal(format!(
                "Failed to inspect managed service '{}': {}",
                service,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        ports.extend(host_ports_from_bindings(
            String::from_utf8_lossy(&output.stdout).trim(),
        )?);
    }
    ports.sort_unstable();
    ports.dedup();
    Ok(ports)
}

fn host_ports_from_bindings(bindings: &str) -> Result<Vec<u16>> {
    let value: serde_json::Value = serde_json::from_str(bindings).map_err(|error| {
        VmError::Internal(format!(
            "Failed to parse managed service port bindings: {error}"
        ))
    })?;
    let mut ports = value
        .as_object()
        .into_iter()
        .flat_map(|bindings| bindings.values())
        .filter_map(serde_json::Value::as_array)
        .flatten()
        .filter_map(|binding| binding.get("HostPort"))
        .filter_map(serde_json::Value::as_str)
        .filter_map(|port| port.parse::<u16>().ok())
        .collect::<Vec<_>>();
    ports.sort_unstable();
    ports.dedup();
    Ok(ports)
}

pub(super) fn is_environment_container(
    name: &str,
    project: &str,
    role: &str,
    compose_service: &str,
) -> bool {
    if role == "environment" {
        return true;
    }

    let compatible = is_pre_role_environment(name, project, role, compose_service);
    if compatible {
        tracing::warn!(
            container = name,
            compatibility = "pre_role_environment",
            "managed environment uses retired pre-role labels; recreate it before v6"
        );
    }
    compatible
}

fn is_pre_role_environment(name: &str, project: &str, role: &str, compose_service: &str) -> bool {
    !project.is_empty()
        && role.is_empty()
        && name == format!("{project}-dev")
        && (compose_service.is_empty() || compose_service == name)
}

pub(super) fn config_path_from_inspect(container: &serde_json::Value) -> Option<PathBuf> {
    let labels = &container["Config"]["Labels"];
    let name = container["Name"]
        .as_str()
        .unwrap_or_default()
        .trim_start_matches('/');
    let project = labels["com.vm.project"].as_str().unwrap_or_default();
    let role = labels["com.vm.role"].as_str().unwrap_or_default();
    let compose_service = labels["com.docker.compose.service"]
        .as_str()
        .unwrap_or_default();
    if labels["com.vm.managed"].as_str() != Some("true")
        || !is_environment_container(name, project, role, compose_service)
    {
        return None;
    }

    if let Some(path) = labels["com.vm.config-path"].as_str() {
        let path = PathBuf::from(path);
        if path.is_absolute() && path.is_file() {
            return Some(path);
        }
    }

    if project.is_empty()
        || (role != "environment" && !is_pre_role_environment(name, project, role, compose_service))
    {
        return None;
    }

    let source = container["Mounts"].as_array().and_then(|mounts| {
        mounts.iter().find_map(|mount| {
            if mount["Type"].as_str() == Some("bind")
                && mount["Destination"].as_str() == Some("/workspace")
            {
                mount["Source"].as_str()
            } else {
                None
            }
        })
    })?;
    config_path_from_workspace_source(source, project)
}

fn config_path_from_workspace_source(source: &str, project: &str) -> Option<PathBuf> {
    let source = Path::new(source);
    if let Some(config) = config_path_below_workspace(source, project) {
        return Some(config);
    }

    source
        .strip_prefix("/host_mnt")
        .ok()
        .map(|relative| Path::new("/").join(relative))
        .and_then(|source| config_path_below_workspace(&source, project))
}

fn config_path_below_workspace(source: &Path, project: &str) -> Option<PathBuf> {
    if !source.is_absolute() {
        return None;
    }
    let direct = source.join("vm.yaml");
    if direct.is_file() {
        return Some(direct);
    }

    let mut components = Path::new(project).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return None;
    }
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
            "demo",
            "environment",
            "demo-dev"
        ));
        assert!(is_environment_container("demo-dev", "demo", "", "demo-dev"));
        assert!(!is_environment_container(
            "demo-postgres",
            "demo",
            "",
            "postgres"
        ));
        assert!(!is_environment_container("demo-dev", "", "", "demo-dev"));
        assert!(!is_environment_container(
            "other-dev",
            "demo",
            "",
            "other-dev"
        ));
        assert!(!is_environment_container(
            "demo-dev", "demo", "service", "demo-dev"
        ));
    }

    #[test]
    fn parses_managed_service_host_port_bindings() {
        assert_eq!(
            host_ports_from_bindings(
                r#"{"5432/tcp":[{"HostIp":"127.0.0.1","HostPort":"3129"}],"6379/tcp":[{"HostIp":"::1","HostPort":"3130"}]}"#
            )
            .unwrap(),
            vec![3129, 3130]
        );
        assert!(host_ports_from_bindings("not-json").is_err());
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

        let partially_labeled = serde_json::json!({
            "Name": "/demo-dev",
            "Config": {
                "Labels": {
                    "com.vm.managed": "true",
                    "com.vm.project": "demo",
                    "com.vm.role": "environment",
                    "com.docker.compose.service": "demo-dev"
                }
            },
            "Mounts": [{
                "Type": "bind",
                "Source": project.path(),
                "Destination": "/workspace"
            }]
        });
        assert_eq!(
            config_path_from_inspect(&partially_labeled),
            Some(config.clone())
        );

        let pre_role = serde_json::json!({
            "Name": "/demo-dev",
            "Config": {
                "Labels": {
                    "com.vm.managed": "true",
                    "com.vm.project": "demo",
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
        assert_eq!(config_path_from_inspect(&pre_role), Some(config));

        let service = serde_json::json!({
            "Name": "/demo-postgres",
            "Config": {
                "Labels": {
                    "com.vm.managed": "true",
                    "com.vm.project": "demo",
                    "com.docker.compose.service": "postgres"
                }
            },
            "Mounts": [{
                "Type": "bind",
                "Source": project.path(),
                "Destination": "/workspace"
            }]
        });
        assert_eq!(config_path_from_inspect(&service), None);
    }
}
