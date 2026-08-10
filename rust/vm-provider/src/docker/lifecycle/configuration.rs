//! Container configuration preparation and copying.

use std::{borrow::Cow, fs, process::Command};

use super::LifecycleOperations;
use crate::docker::{build::BuildOperations, DockerOps, UserConfig};
use crate::project_plan::{ProjectPlan, PROJECT_PLAN_CONFIG_KEY};
use vm_config::config::VmConfig;
use vm_core::error::{Result, VmError};

impl<'a> LifecycleOperations<'a> {
    fn secure_container_config(&self, container_name: &str) -> Result<()> {
        let username = UserConfig::from_vm_config(self.config).username;
        for command in [
            ["chown", username.as_str(), super::TEMP_CONFIG_PATH],
            ["chmod", "600", super::TEMP_CONFIG_PATH],
        ] {
            let output = Command::new(self.executable)
                .args(["exec", "-u", "root", container_name])
                .args(command)
                .output()
                .map_err(|error| {
                    VmError::Internal(format!(
                        "Failed to secure VM configuration in container '{container_name}': {error}"
                    ))
                })?;

            if !output.status.success() {
                return Err(VmError::Internal(format!(
                    "Failed to secure VM configuration in container '{}': {}",
                    container_name,
                    String::from_utf8_lossy(&output.stderr).trim()
                )));
            }
        }

        Ok(())
    }

    fn config_matches_container(&self, container_name: &str, config_json: &str) -> bool {
        let output = Command::new(self.executable)
            .args(["exec", container_name, "cat", super::TEMP_CONFIG_PATH])
            .output();

        match output {
            Ok(output) if output.status.success() => output.stdout == config_json.as_bytes(),
            _ => false,
        }
    }

    #[must_use = "config preparation results should be checked"]
    pub(super) fn prepare_config_for_build(&self) -> Result<Cow<'_, VmConfig>> {
        let pipx_managed_packages = if let Some(pipx_json) = self.get_pipx_json()? {
            self.extract_pipx_managed_packages(&pipx_json)
        } else {
            std::collections::HashSet::new()
        };

        if pipx_managed_packages.is_empty() {
            Ok(Cow::Borrowed(self.config))
        } else {
            let mut config = self.config.clone();
            config.pip_packages = config
                .pip_packages
                .iter()
                .filter(|package| !pipx_managed_packages.contains(*package))
                .cloned()
                .collect();
            Ok(Cow::Owned(config))
        }
    }

    #[must_use = "config copy preparation results should be checked"]
    pub(super) fn prepare_config_for_copy(
        &self,
        container_pipx_packages: &[String],
        project_plan: &ProjectPlan,
    ) -> Result<VmConfig> {
        let mut config = self.config.clone();
        if !config.pip_packages.is_empty() {
            config.pip_packages.clear();
        }
        if !container_pipx_packages.is_empty() {
            config.extra_config.insert(
                "container_pipx_packages".to_string(),
                serde_json::to_value(container_pipx_packages).map_err(|error| {
                    VmError::Internal(format!(
                        "Failed to serialize pipx package list for container configuration: {error}"
                    ))
                })?,
            );
        }
        config.extra_config.insert(
            PROJECT_PLAN_CONFIG_KEY.to_string(),
            serde_json::to_value(project_plan).map_err(|error| {
                VmError::Internal(format!("Failed to serialize project install plan: {error}"))
            })?,
        );

        Ok(config)
    }

    #[must_use = "temp config preparation results should be checked"]
    pub(super) fn prepare_temp_config(&self) -> Result<Cow<'_, VmConfig>> {
        if self
            .config
            .project
            .as_ref()
            .is_some_and(|project| project.name.as_deref() == Some("vm-temp"))
        {
            return Ok(Cow::Borrowed(self.config));
        }

        let mut config = self.config.clone();
        if let Some(project) = config.project.as_mut() {
            project.name = Some("vm-temp".to_owned());
        } else {
            config.project = Some(vm_config::config::ProjectConfig {
                name: Some("vm-temp".to_owned()),
                ..Default::default()
            });
        }
        Ok(Cow::Owned(config))
    }

    #[must_use = "config preparation results should be checked"]
    pub(super) fn prepare_and_copy_config(&self, container_name: &str) -> Result<()> {
        let container_pipx_packages = if let Some(pipx_json) = self.get_pipx_json()? {
            self.categorize_pipx_packages(&pipx_json)
        } else {
            Vec::new()
        };
        let project_plan = ProjectPlan::detect(self.project_dir, self.config);
        let config_for_copy =
            self.prepare_config_for_copy(&container_pipx_packages, &project_plan)?;
        let config_json = config_for_copy.to_json()?;

        if self.config_matches_container(container_name, &config_json) {
            return self.secure_container_config(container_name);
        }

        let generated_config_path = self.generated_dir.join("vm-config.json");
        let local_config_matches = fs::read_to_string(&generated_config_path)
            .ok()
            .is_some_and(|existing| existing == config_json);

        if !local_config_matches {
            crate::docker::artifacts::secure_write_if_changed(
                &generated_config_path,
                config_json.as_bytes(),
            )
            .map_err(|error| {
                VmError::Internal(format!(
                    "Failed to write configuration to {}: {error}",
                    generated_config_path.display()
                ))
            })?;
        }

        let source = BuildOperations::path_to_string(&generated_config_path)?;
        let destination = format!("{container_name}:{}", super::TEMP_CONFIG_PATH);
        DockerOps::copy(Some(self.executable), source, &destination).map_err(|error| {
            VmError::Internal(format!(
                "Failed to copy VM configuration to container '{container_name}': {error}"
            ))
        })?;

        self.secure_container_config(container_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use vm_config::config::VmSettings;

    #[cfg(unix)]
    #[test]
    fn copied_config_is_private_and_owned_by_container_user() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("docker");
        fs::write(
            &executable,
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$0.log\"\n",
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();

        let config = VmConfig {
            vm: Some(VmSettings {
                user: Some("developer".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let generated_dir = directory.path().join("generated");
        let project_dir = directory.path().join("project");
        let executable_string = executable.to_string_lossy().into_owned();
        let lifecycle =
            LifecycleOperations::new(&config, &generated_dir, &project_dir, &executable_string);

        lifecycle.secure_container_config("test-container").unwrap();

        let commands = fs::read_to_string(format!("{}.log", executable.display())).unwrap();
        assert_eq!(
            commands.lines().collect::<Vec<_>>(),
            [
                "exec -u root test-container chown developer /tmp/vm-config.json",
                "exec -u root test-container chmod 600 /tmp/vm-config.json",
            ]
        );
    }
}
