use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use vm_config::config::VmConfig;
use vm_core::error::{Result, VmError};

use super::compose_model::stable_name_component;

pub(super) fn project_artifacts_dir(config: &VmConfig, project_dir: &Path) -> Result<PathBuf> {
    let artifacts_dir = project_artifacts_location(config, project_dir)?;
    fs::create_dir_all(&artifacts_dir).map_err(|error| {
        VmError::Internal(format!(
            "Failed to create generated artifact directory '{}': {error}",
            artifacts_dir.display()
        ))
    })?;
    make_private(&artifacts_dir)?;
    Ok(artifacts_dir)
}

pub(super) fn project_artifacts_location(config: &VmConfig, project_dir: &Path) -> Result<PathBuf> {
    let state_dir = vm_core::user_paths::vm_state_dir()?;
    Ok(project_artifacts_path(&state_dir, config, project_dir))
}

fn project_artifacts_path(state_dir: &Path, config: &VmConfig, project_dir: &Path) -> PathBuf {
    let project_name = config
        .project
        .as_ref()
        .and_then(|project| project.name.as_deref())
        .unwrap_or("vm-project");
    let identity_path = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());
    let digest = Sha256::digest(identity_path.to_string_lossy().as_bytes());
    let mut identity = String::with_capacity(12);
    for byte in digest.iter().take(6) {
        write!(&mut identity, "{byte:02x}").expect("writing to a String cannot fail");
    }

    state_dir
        .join("generated")
        .join(format!(
            "{}-{identity}",
            stable_name_component(project_name)
        ))
        .join("docker")
}

pub(super) fn secure_write_if_changed(path: &Path, content: &[u8]) -> Result<()> {
    if fs::read(path).ok().as_deref() != Some(content) {
        vm_core::file_system::atomic_write(path, content)?;
    }
    make_private(path)
}

pub(super) fn compose_path(generated_dir: &Path, instance: Option<&str>) -> PathBuf {
    match instance {
        Some(instance) => generated_dir.join(format!(
            "docker-compose.{}.yml",
            stable_name_component(instance)
        )),
        None => generated_dir.join("docker-compose.yml"),
    }
}

#[cfg(unix)]
fn make_private(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = if path.is_dir() { 0o700 } else { 0o600 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(Into::into)
}

#[cfg(not(unix))]
fn make_private(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vm_config::config::ProjectConfig;

    #[test]
    fn artifact_paths_are_stable_and_project_root_scoped() {
        let state = tempfile::tempdir().unwrap();
        let first_root = tempfile::tempdir().unwrap();
        let second_root = tempfile::tempdir().unwrap();
        let config = VmConfig {
            project: Some(ProjectConfig {
                name: Some("Sketch API".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let first = project_artifacts_path(state.path(), &config, first_root.path());
        assert_eq!(
            first,
            project_artifacts_path(state.path(), &config, first_root.path())
        );
        assert_ne!(
            first,
            project_artifacts_path(state.path(), &config, second_root.path())
        );
        assert!(first.ends_with("docker"));
        assert!(first.to_string_lossy().contains("Sketch_API-"));
        assert_eq!(
            compose_path(&first, Some("feature/api")),
            first.join("docker-compose.feature_api.yml")
        );
    }

    #[cfg(unix)]
    #[test]
    fn secure_writes_use_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("docker-compose.yml");
        secure_write_if_changed(&path, b"services: {}\n").unwrap();

        let mode = fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
