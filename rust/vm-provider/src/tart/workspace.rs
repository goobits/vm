use std::path::{Path, PathBuf};

use vm_config::config::{ImageSpec, VmConfig};
use vm_core::error::Result;

use super::provider::TartProvider;
use crate::{shell_session, tart_base, ExecOptions, VmError};

impl TartProvider {
    pub(super) fn host_workspace_path(&self) -> Result<PathBuf> {
        Self::normalize_host_workspace_path(&self.config.project_dir()?)
    }

    pub(super) fn normalize_host_workspace_path(path: &Path) -> Result<PathBuf> {
        let canonical = path.canonicalize().map_err(|error| {
            VmError::Internal(format!(
                "Failed to resolve host workspace path {}: {error}",
                path.display()
            ))
        })?;
        if Self::looks_like_project_root(&canonical) {
            return Ok(canonical);
        }
        let nested = canonical.join("workspace");
        if canonical.file_name().and_then(|name| name.to_str()) == Some("workspace")
            && nested.is_dir()
            && Self::looks_like_project_root(&nested)
        {
            return nested.canonicalize().map_err(|error| {
                VmError::Internal(format!(
                    "Failed to resolve nested host workspace path {}: {error}",
                    nested.display()
                ))
            });
        }
        Ok(canonical)
    }

    fn looks_like_project_root(path: &Path) -> bool {
        [
            "vm.yaml",
            ".git",
            "Cargo.toml",
            "package.json",
            "pyproject.toml",
        ]
        .iter()
        .any(|marker| path.join(marker).exists())
    }

    pub(super) fn effective_sync_directory(&self) -> String {
        effective_sync_directory_for_config(&self.config)
    }

    pub(super) fn is_macos_guest_config(config: &VmConfig) -> bool {
        if config.os.as_deref() == Some("macos") {
            return true;
        }
        if config.os.as_deref() == Some("linux") {
            return false;
        }
        let guest_os = config
            .tart
            .as_ref()
            .and_then(|tart| tart.guest_os.as_deref());
        if guest_os == Some("macos") {
            return true;
        }
        if guest_os == Some("linux") {
            return false;
        }
        if let Some(ImageSpec::String(name)) = config.vm.as_ref().and_then(|vm| vm.image.clone()) {
            if let Some(os) = tart_base::guest_os(&name) {
                return os == "macos";
            }
            if name.contains("ubuntu") || name.contains("debian") || name.contains("linux") {
                return false;
            }
        }
        true
    }

    pub(super) fn guest_exec_args(
        &self,
        container: Option<&str>,
        command: &[String],
    ) -> Result<Vec<String>> {
        self.guest_exec_args_with_options(container, command, &ExecOptions::default())
    }

    pub(super) fn guest_exec_args_with_options(
        &self,
        container: Option<&str>,
        command: &[String],
        options: &ExecOptions,
    ) -> Result<Vec<String>> {
        options.validate_user()?;
        let vm_name = self.vm_name_with_instance(container)?;
        let shell = self
            .config
            .terminal
            .as_ref()
            .and_then(|terminal| terminal.shell.as_deref())
            .unwrap_or("zsh");
        let sync_dir = self.effective_sync_directory();
        let working_dir = options.guest_cwd(&sync_dir);
        self.ensure_workspace_mount_ready(&vm_name, &sync_dir)?;
        self.ensure_configured_mounts_ready(&vm_name)?;
        self.ensure_shell_config_ready(&vm_name, &sync_dir)?;
        let script = shell_session::exec_script(&sync_dir, &working_dir.to_string_lossy());
        let mut args = vec!["exec".to_string(), vm_name];
        if let Some(user) = options.user.as_deref() {
            args.extend(["sudo".to_string(), "-nHu".to_string(), user.to_string()]);
        }
        args.extend([
            shell.to_string(),
            "-lc".to_string(),
            script,
            "vm-exec".to_string(),
        ]);
        args.extend(command.iter().cloned());
        Ok(args)
    }
}

pub(super) fn effective_sync_directory_for_config(config: &VmConfig) -> String {
    let configured = config
        .project
        .as_ref()
        .and_then(|project| project.workspace_path.as_deref())
        .unwrap_or("/workspace");
    if configured == "/workspace" && TartProvider::is_macos_guest_config(config) {
        let user = config
            .tart
            .as_ref()
            .and_then(|tart| tart.ssh_user.as_deref())
            .unwrap_or("admin");
        return format!("/Users/{user}/workspace");
    }
    configured.to_string()
}
