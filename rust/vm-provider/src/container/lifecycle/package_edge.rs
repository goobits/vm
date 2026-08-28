use super::LifecycleOperations;
use crate::container::artifacts::compose_path;
use vm_core::error::{Result, VmError};

impl LifecycleOperations<'_> {
    pub(super) fn reconcile_package_edge(&self, container_name: &str) -> Result<()> {
        let Some(edge) = self.config.package_edge.as_ref() else {
            return Ok(());
        };
        let project_name = self
            .config
            .project
            .as_ref()
            .and_then(|project| project.name.as_deref())
            .unwrap_or("vm-project");
        let instance_name = crate::container::compose_model::instance_name_from_container(
            project_name,
            container_name,
        );
        let compose_path = compose_path(self.generated_dir, instance_name.as_deref());
        if !compose_path.exists() {
            return Err(VmError::Internal(format!(
                "Generated Compose file is unavailable for package-edge reconciliation: {}",
                compose_path.display()
            )));
        }
        let edge_container = container_name.strip_suffix("-dev").map_or_else(
            || format!("{container_name}-package-edge"),
            |name| format!("{name}-package-edge"),
        );
        if package_edge_is_current(self.runtime.executable(), &edge_container, &edge.revision) {
            return Ok(());
        }

        self.runtime
            .compose_invocation(&compose_path, "up", &["--detach", "package-edge"])?
            .stream()
    }
}

fn package_edge_is_current(executable: &str, container: &str, revision: &str) -> bool {
    let Ok(output) = std::process::Command::new(executable)
        .args([
            "inspect",
            "--type",
            "container",
            "--format",
            "{{.State.Status}}\t{{index .Config.Labels \"com.vm.package-edge.revision\"}}",
            container,
        ])
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let value = String::from_utf8_lossy(&output.stdout);
    let Some((state, installed_revision)) = value.trim().split_once('\t') else {
        return false;
    };
    state == "running" && installed_revision == revision
}

#[cfg(all(test, unix))]
mod tests {
    use super::package_edge_is_current;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn probe_requires_matching_running_revision() {
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
}
