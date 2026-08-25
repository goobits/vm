use std::path::PathBuf;
use tracing::info;
use vm_core::error::{Result, VmError};
use vm_core::msg;
use vm_messages::messages::MESSAGES;
use vm_provider::{MountPermission, Provider};

use crate::{StateManager, TempVmOps};

/// Parse `source`, `source:permissions`, or `source:target:permissions`.
fn parse_mount_string(mount_str: &str) -> Result<(PathBuf, Option<PathBuf>, MountPermission)> {
    let parts: Vec<&str> = mount_str.split(':').collect();

    match parts.len() {
        1 => Ok((
            PathBuf::from(parts[0]),
            None,
            MountPermission::default(),
        )),
        2 => {
            let permissions = parts[1].parse::<MountPermission>().map_err(|error| {
                VmError::Config(format!(
                    "Invalid permission in mount string '{mount_str}': {error}"
                ))
            })?;
            Ok((PathBuf::from(parts[0]), None, permissions))
        }
        3 => {
            let permissions = parts[2].parse::<MountPermission>().map_err(|error| {
                VmError::Config(format!(
                    "Invalid permission in mount string '{mount_str}': {error}"
                ))
            })?;
            Ok((
                PathBuf::from(parts[0]),
                Some(PathBuf::from(parts[1])),
                permissions,
            ))
        }
        _ => Err(VmError::Config(format!(
            "Invalid mount string format: {mount_str}. Expected 'source', 'source:permissions', or 'source:target:permissions'"
        ))),
    }
}

pub(crate) fn parse_mount_strings(
    mount_strings: &[String],
) -> Result<Vec<(PathBuf, Option<PathBuf>, MountPermission)>> {
    mount_strings
        .iter()
        .map(|mount| parse_mount_string(mount))
        .collect()
}

impl TempVmOps {
    /// Add a mount to the running temporary VM.
    pub fn mount(path: String, yes: bool, provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new().map_err(|error| {
            VmError::Internal(format!(
                "Failed to initialize state manager for mount operation: {error}"
            ))
        })?;
        if !state_manager.state_exists() {
            return Err(VmError::NotFound(
                "No temporary VM exists; create one before adding mounts".to_string(),
            ));
        }

        let (source, target, permissions) = parse_mount_string(&path).map_err(|error| {
            VmError::Config(format!(
                "Failed to parse mount string '{path}'. Check mount path format: {error}"
            ))
        })?;
        let mut state = state_manager.load_state()?;
        if state.has_mount(&source) {
            return Err(VmError::Internal(format!(
                "Mount already exists for source: {}",
                source.display()
            )));
        }
        if !yes {
            return Err(VmError::Conflict(format!(
                "Adding mount '{}' requires explicit confirmation",
                source.display()
            )));
        }

        let permissions_display = permissions.to_string();
        let target_display = target.clone();
        if let Some(target) = target {
            state
                .add_mount_with_target(source.clone(), target, permissions)
                .map_err(|error| {
                    VmError::Config(format!("Failed to add mount with custom target: {error}"))
                })?;
        } else {
            state
                .add_mount(source.clone(), permissions)
                .map_err(|error| VmError::Config(format!("Failed to add mount: {error}")))?;
        }
        state_manager.save_state(&state)?;
        info!(
            "🔗 Mount added: {} ({})",
            source.display(),
            permissions_display
        );

        let temp_provider = provider.as_temp_provider().ok_or_else(|| {
            VmError::Internal("Provider does not support mount updates".to_string())
        })?;
        info!("{}", MESSAGES.service.temp_vm_updating_container);
        temp_provider.update_mounts(&state).map_err(|error| {
            VmError::Provider(format!("Failed to update container mounts: {error}"))
        })?;
        info!("{}", MESSAGES.service.temp_vm_mount_applied);
        info!(
            "{}",
            msg!(
                MESSAGES.service.temp_vm_mount_source,
                source = source.display().to_string()
            )
        );
        if let Some(target) = target_display {
            info!(
                "{}",
                msg!(
                    MESSAGES.service.temp_vm_mount_target,
                    target = target.display().to_string()
                )
            );
        }
        info!(
            "{}",
            msg!(
                MESSAGES.service.temp_vm_mount_access,
                access = permissions_display
            )
        );
        info!("{}", MESSAGES.service.temp_vm_view_mounts_hint);
        Ok(())
    }

    /// Remove one or all mounts from the temporary VM.
    pub fn unmount(
        path: Option<String>,
        all: bool,
        yes: bool,
        provider: Box<dyn Provider>,
    ) -> Result<()> {
        let state_manager = StateManager::new().map_err(|error| {
            VmError::Internal(format!(
                "Failed to initialize state manager for SSH connection: {error}"
            ))
        })?;
        if !state_manager.state_exists() {
            info!("No temporary VM found.");
            info!("💡 Create one with: vm temp create <directory>");
            info!("   Or use 'vm temp ssh' to create and connect automatically");
            return Err(VmError::NotFound("No temporary VM exists".to_string()));
        }

        let mut state = state_manager.load_state()?;
        if all {
            let mount_count = state.mount_count();
            if !yes {
                return Err(VmError::Conflict(format!(
                    "Removing all {mount_count} mounts requires explicit confirmation"
                )));
            }
            state.clear_mounts();
            state_manager.save_state(&state).map_err(|error| {
                VmError::Internal(format!("Failed to save updated temp VM state: {error}"))
            })?;
            info!(
                "{}",
                msg!(
                    MESSAGES.service.temp_vm_mounts_removed,
                    count = mount_count.to_string()
                )
            );
            if let Some(temp_provider) = provider.as_temp_provider() {
                info!("{}", MESSAGES.service.temp_vm_updating_container);
                temp_provider.update_mounts(&state).map_err(|error| {
                    VmError::Provider(format!("Failed to update container mounts: {error}"))
                })?;
                info!(
                    "{}",
                    msg!(
                        MESSAGES.service.temp_vm_all_mounts_removed,
                        count = mount_count.to_string()
                    )
                );
                info!("{}", MESSAGES.service.temp_vm_add_mounts_hint);
            }
            return Ok(());
        }

        let Some(path) = path else {
            info!("{}", MESSAGES.service.temp_vm_unmount_required);
            info!("{}", MESSAGES.service.temp_vm_unmount_options);
            info!("{}", MESSAGES.service.temp_vm_unmount_specific);
            info!("{}", MESSAGES.service.temp_vm_unmount_all);
            return Err(VmError::Internal(
                "Must specify --path or --all".to_string(),
            ));
        };
        let source = PathBuf::from(path);
        if !state.has_mount(&source) {
            return Err(VmError::Internal(format!(
                "Mount not found for source: {}",
                source.display()
            )));
        }
        if !yes {
            return Err(VmError::Conflict(format!(
                "Removing mount '{}' requires explicit confirmation",
                source.display()
            )));
        }

        let removed = state
            .remove_mount(&source)
            .map_err(|error| VmError::Config(format!("Failed to remove mount: {error}")))?;
        state_manager.save_state(&state).map_err(|error| {
            VmError::Internal(format!("Failed to save updated temp VM state: {error}"))
        })?;
        info!(
            "{}",
            msg!(
                MESSAGES.service.temp_vm_mount_removed_detail,
                source = removed.source.display().to_string(),
                permissions = removed.permissions.to_string()
            )
        );
        if let Some(temp_provider) = provider.as_temp_provider() {
            info!("{}", MESSAGES.service.temp_vm_updating_container);
            temp_provider.update_mounts(&state).map_err(|error| {
                VmError::Provider(format!("Failed to update container mounts: {error}"))
            })?;
            info!("{}", MESSAGES.service.temp_vm_mount_removed);
            info!("  Path: {}", source.display());
            info!("{}", MESSAGES.service.temp_vm_view_remaining_hint);
        }
        Ok(())
    }

    /// List current mounts.
    pub fn mounts() -> Result<()> {
        let state_manager = StateManager::new().map_err(|error| {
            VmError::Internal(format!(
                "Failed to initialize state manager for SSH connection: {error}"
            ))
        })?;
        if !state_manager.state_exists() {
            info!("{}", MESSAGES.service.temp_vm_no_vm_found);
            info!("{}", MESSAGES.service.temp_vm_create_hint);
            return Ok(());
        }

        let state = state_manager.load_state()?;
        if state.mount_count() == 0 {
            info!("{}", MESSAGES.service.temp_vm_no_mounts);
            info!("{}", MESSAGES.service.temp_vm_add_mount_hint);
            return Ok(());
        }
        info!(
            "{}",
            msg!(
                MESSAGES.service.temp_vm_current_mounts,
                count = state.mount_count().to_string()
            )
        );
        for mount in state.get_mounts() {
            info!(
                "{}",
                msg!(
                    MESSAGES.service.temp_vm_mount_display_item,
                    source = mount.source.display().to_string(),
                    target = mount.target.display().to_string(),
                    permissions = mount.permissions.to_string()
                )
            );
        }
        info!(
            "{}",
            msg!(
                MESSAGES.service.temp_vm_mount_summary,
                ro_count = state
                    .mount_count_by_permission(MountPermission::ReadOnly)
                    .to_string(),
                rw_count = state
                    .mount_count_by_permission(MountPermission::ReadWrite)
                    .to_string()
            )
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mount_parser() {
        // Test simple source
        let (source, target, perm) =
            parse_mount_string("/home/user").expect("Should parse simple mount string");
        assert_eq!(source, PathBuf::from("/home/user"));
        assert_eq!(target, None);
        assert_eq!(perm, MountPermission::ReadWrite);

        // Test source with permissions
        let (source, target, perm) = parse_mount_string("/home/user:ro")
            .expect("Should parse mount string with permissions");
        assert_eq!(source, PathBuf::from("/home/user"));
        assert_eq!(target, None);
        assert_eq!(perm, MountPermission::ReadOnly);

        // Test source with target and permissions
        let (source, target, perm) = parse_mount_string("/home/user:/workspace/user:rw")
            .expect("Should parse mount string with target and permissions");
        assert_eq!(source, PathBuf::from("/home/user"));
        assert_eq!(target, Some(PathBuf::from("/workspace/user")));
        assert_eq!(perm, MountPermission::ReadWrite);
    }
}
