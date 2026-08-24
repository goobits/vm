use std::path::PathBuf;
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
            "{{.Names}}\t{{.ID}}\t{{.Status}}\t{{.CreatedAt}}\t{{.RunningFor}}\t{{.Label \"com.vm.project\"}}\t{{.Label \"com.vm.role\"}}",
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
            let role = parts.get(6).copied().unwrap_or_default();
            is_environment_container(role).then(|| {
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

pub(super) fn is_environment_container(role: &str) -> bool {
    role == "environment"
}

pub(super) fn config_path_from_inspect(container: &serde_json::Value) -> Option<PathBuf> {
    let labels = &container["Config"]["Labels"];
    let role = labels["com.vm.role"].as_str().unwrap_or_default();
    if labels["com.vm.managed"].as_str() != Some("true") || !is_environment_container(role) {
        return None;
    }

    let path = PathBuf::from(labels["com.vm.config-path"].as_str()?);
    (path.is_absolute() && path.is_file()).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_filter_excludes_managed_compose_services() {
        assert!(is_environment_container("environment"));
        assert!(!is_environment_container(""));
        assert!(!is_environment_container("service"));
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

        let unlabeled = serde_json::json!({
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
        assert_eq!(config_path_from_inspect(&unlabeled), None);
    }
}
