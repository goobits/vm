use std::collections::HashSet;
use vm_core::error::{Result, VmError};

use crate::config::{
    mounts::{resolve_mount_source, validate_mount_target},
    VmConfig,
};

pub(super) fn validate_storage(config: &VmConfig) -> Result<()> {
    if config.storage.is_empty() {
        return Ok(());
    }
    if config.provider.as_deref() == Some("tart") {
        return Err(VmError::Config(
            "Named volumes and tmpfs mounts are not supported by Tart".to_string(),
        ));
    }

    let username = config
        .vm
        .as_ref()
        .and_then(|vm| vm.user.as_deref())
        .unwrap_or("developer");
    let workspace = config
        .project
        .as_ref()
        .and_then(|project| project.workspace_path.as_deref())
        .unwrap_or("/workspace");
    let mut targets = HashSet::from([format!("/home/{username}/.shell_history")]);
    targets.extend(
        config
            .mounts
            .iter()
            .map(|mount| mount.target.display().to_string()),
    );
    if config
        .services
        .get("postgresql")
        .is_some_and(|service| service.enabled)
    {
        targets.insert("/var/lib/postgresql/data".to_string());
    }
    for (name, volume) in &config.storage.volumes {
        if !valid_storage_name(name) {
            return Err(VmError::Config(format!(
                "Invalid storage volume name '{name}': use letters, numbers, dashes, or underscores"
            )));
        }
        if matches!(name.as_str(), "shell_history" | "postgres_data") {
            return Err(VmError::Config(format!(
                "Storage volume name '{name}' is reserved by the VM tool"
            )));
        }
        validate_mount_target(std::path::Path::new(&volume.target))?;
        if volume.target == workspace {
            return Err(VmError::Config(format!(
                "A named volume cannot replace the {workspace} source bind; use a nested target"
            )));
        }
        if !targets.insert(volume.target.clone()) {
            return Err(VmError::Config(format!(
                "Duplicate storage target: {}",
                volume.target
            )));
        }
    }

    for tmpfs in &config.storage.tmpfs {
        validate_mount_target(std::path::Path::new(&tmpfs.target))?;
        if !targets.insert(tmpfs.target.clone()) {
            return Err(VmError::Config(format!(
                "Duplicate storage target: {}",
                tmpfs.target
            )));
        }
        if !matches!(tmpfs.size.to_mb(), Some(size) if size > 0) {
            return Err(VmError::Config(format!(
                "tmpfs mount '{}' requires a fixed, positive size",
                tmpfs.target
            )));
        }
        if !(3..=4).contains(&tmpfs.mode.len())
            || !tmpfs
                .mode
                .chars()
                .all(|character| matches!(character, '0'..='7'))
        {
            return Err(VmError::Config(format!(
                "tmpfs mount '{}' has invalid mode '{}'; use three or four octal digits",
                tmpfs.target, tmpfs.mode
            )));
        }
    }

    Ok(())
}

pub(super) fn validate_mounts(config: &VmConfig) -> Result<()> {
    let workspace = config
        .project
        .as_ref()
        .and_then(|project| project.workspace_path.as_deref())
        .unwrap_or("/workspace");
    let mut targets = HashSet::from([workspace.to_string()]);
    let mut sources = HashSet::new();
    let project_dir = config.project_dir()?;

    for mount in &config.mounts {
        validate_mount_target(&mount.target)?;
        let source = resolve_mount_source(&mount.source, &project_dir)?;
        if !sources.insert(source.clone()) {
            return Err(VmError::Config(format!(
                "Duplicate mount source: {}",
                source.display()
            )));
        }
        let target = mount.target.display().to_string();
        if !targets.insert(target.clone()) {
            return Err(VmError::Config(format!("Duplicate mount target: {target}")));
        }
    }
    Ok(())
}

fn valid_storage_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}
