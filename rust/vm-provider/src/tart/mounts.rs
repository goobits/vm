use super::{provisioner::TartProvisioner, TartProvider};
use crate::{Mount, MountPermission, VmError};
use std::path::PathBuf;
use vm_core::error::Result;

#[derive(Clone, Debug)]
pub(super) struct TartDirShare {
    pub(super) tag: String,
    pub(super) host_path: PathBuf,
    pub(super) guest_path: Option<PathBuf>,
    pub(super) access: MountPermission,
}

impl TartDirShare {
    pub(super) fn from_mount(tag: String, mount: Mount) -> Self {
        Self {
            tag,
            host_path: mount.source,
            guest_path: Some(mount.target),
            access: mount.permissions,
        }
    }

    pub(super) fn tart_argument(&self) -> String {
        let access = if self.access == MountPermission::ReadOnly {
            ":ro"
        } else {
            ""
        };
        format!("{}:tag={}{}", self.host_path.display(), self.tag, access)
    }
}

impl TartProvider {
    pub(super) fn configured_dir_shares(&self) -> Result<Vec<TartDirShare>> {
        let project_dir = self.host_workspace_path()?;
        self.config
            .mounts
            .iter()
            .enumerate()
            .map(|(index, config)| {
                Mount::from_config(config, &project_dir)
                    .map(|mount| TartDirShare::from_mount(format!("vmmount{index}"), mount))
            })
            .collect()
    }

    pub(super) fn persist_tart_dir_shares(
        &self,
        vm_name: &str,
        shares: &[TartDirShare],
    ) -> Result<()> {
        for share in shares {
            let dir_arg = share.tart_argument();
            tracing::info!("Adding Tart directory share: {}", dir_arg);
            self.tart_expr(&["set", vm_name, "--dir", &dir_arg])
                .run()
                .map_err(|error| {
                    VmError::Provider(format!("Failed to add Tart directory share: {error}"))
                })?;
        }
        Ok(())
    }

    pub(super) fn mount_tart_dir_shares_in_guest(
        &self,
        vm_name: &str,
        shares: &[TartDirShare],
    ) -> Result<()> {
        let commands = shares
            .iter()
            .filter_map(|share| {
                share.guest_path.as_ref().map(|guest_path| {
                    TartProvisioner::virtiofs_mount_command(
                        &share.tag,
                        &guest_path.display().to_string(),
                    )
                })
            })
            .collect::<Vec<_>>();
        if commands.is_empty() {
            return Ok(());
        }

        self.tart_expr(&["exec", vm_name, "sh", "-c", &commands.join("\n")])
            .run()
            .map(|_| ())
            .map_err(|error| {
                VmError::Provider(format!("Failed to mount shared directories: {error}"))
            })
    }

    pub(super) fn ensure_configured_mounts_ready(&self, vm_name: &str) -> Result<()> {
        self.mount_tart_dir_shares_in_guest(vm_name, &self.configured_dir_shares()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn tart_arguments_include_access_without_shell_syntax() {
        let share = TartDirShare {
            tag: "auth".to_string(),
            host_path: Path::new("/tmp/shared auth").to_path_buf(),
            guest_path: Some(Path::new("/packages/auth").to_path_buf()),
            access: MountPermission::ReadOnly,
        };

        assert_eq!(share.tart_argument(), "/tmp/shared auth:tag=auth:ro");
    }
}
