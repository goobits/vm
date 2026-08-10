use super::TartProvisioner;
use crate::shell_session::{quote_posix_argument, quote_posix_home_path};
use crate::tart::host_sync::{
    collect_host_sync_mounts, expand_tilde, file_name, resolve_guest_home_path, resolve_home_dir,
};
use std::path::Path;
use vm_config::config::VmConfig;
use vm_core::error::{Result, VmError};

impl TartProvisioner {
    pub(super) fn host_sync_mount_command(&self, config: &VmConfig) -> Option<String> {
        let mounts = collect_host_sync_mounts(config);
        if mounts.is_empty() {
            return None;
        }

        let mut commands = Vec::new();
        for mount in mounts {
            let guest_path = resolve_guest_home_path(&mount.guest_path);
            commands.push(Self::virtiofs_mount_command(&mount.tag, &guest_path));
        }

        Some(commands.join("\n"))
    }

    pub(super) fn sync_dotfiles(&self, config: &VmConfig) -> Result<()> {
        let Some(host_sync) = config.host_sync.as_ref() else {
            return Ok(());
        };
        if host_sync.dotfiles.is_empty() {
            return Ok(());
        }

        let Some(home_dir) = resolve_home_dir() else {
            return Ok(());
        };

        for dotfile in &host_sync.dotfiles {
            let Some(source) = expand_tilde(dotfile) else {
                continue;
            };
            if !source.exists() {
                continue;
            }

            let guest_target = if dotfile.starts_with("~/") || dotfile == "~" {
                resolve_guest_home_path(dotfile)
            } else if source.starts_with(&home_dir) {
                let relative = source.strip_prefix(&home_dir).unwrap_or(&source);
                resolve_guest_home_path(&format!("~/{}", relative.display()))
            } else {
                continue;
            };

            self.copy_host_path_to_guest(&source, &guest_target)?;
        }

        Ok(())
    }

    pub(super) fn sync_ssh_config(&self, config: &VmConfig) -> Result<()> {
        if !config
            .host_sync
            .as_ref()
            .map(|sync| sync.ssh_config)
            .unwrap_or(false)
        {
            return Ok(());
        }

        let Some(home_dir) = resolve_home_dir() else {
            return Ok(());
        };
        let ssh_config = home_dir.join(".ssh").join("config");
        if !ssh_config.exists() || !ssh_config.is_file() {
            return Ok(());
        }

        self.copy_host_path_to_guest(&ssh_config, "$HOME/.ssh/config")?;
        Ok(())
    }

    fn copy_host_path_to_guest(&self, source: &Path, guest_target: &str) -> Result<()> {
        let source = source
            .canonicalize()
            .unwrap_or_else(|_| source.to_path_buf());
        let guest_parent = Path::new(guest_target)
            .parent()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| ".".to_string());
        let guest_target = quote_posix_home_path(guest_target);
        let guest_parent = quote_posix_home_path(&guest_parent);
        let instance = quote_posix_argument(&self.instance_name);

        if source.is_file() {
            let source = quote_posix_argument(&source.to_string_lossy());
            let remote_command =
                quote_posix_argument(&format!("mkdir -p {guest_parent} && cat > {guest_target}"));
            let command = format!("cat {source} | tart exec {instance} bash -lc {remote_command}");
            self.host_shell(&command).run().map_err(|error| {
                VmError::Provider(format!("Failed to sync file to Tart VM: {error}"))
            })?;
            return Ok(());
        }

        if source.is_dir() {
            let Some(name) = file_name(&source) else {
                return Ok(());
            };
            let parent = source.parent().unwrap_or(source.as_path());
            let parent = quote_posix_argument(&parent.to_string_lossy());
            let name = quote_posix_argument(&name);
            let remote_command = quote_posix_argument(&format!(
                "mkdir -p {guest_parent} && tar -xf - -C {guest_parent}"
            ));
            let command = format!(
                "tar -C {parent} -cf - {name} | tart exec {instance} bash -lc {remote_command}"
            );
            self.host_shell(&command).run().map_err(|error| {
                VmError::Provider(format!("Failed to sync directory to Tart VM: {error}"))
            })?;
        }

        Ok(())
    }

    pub(super) fn copy_host_file_to_guest_home(
        &self,
        source: &Path,
        guest_relative_path: &str,
        mode: &str,
    ) -> Result<()> {
        if !source.exists() || !source.is_file() {
            return Ok(());
        }

        let source = source
            .canonicalize()
            .unwrap_or_else(|_| source.to_path_buf());
        let Some(parent) = Path::new(guest_relative_path).parent() else {
            return Ok(());
        };

        let source = quote_posix_argument(&source.to_string_lossy());
        let instance = quote_posix_argument(&self.instance_name);
        let parent = quote_posix_home_path(&format!("$HOME/{}", parent.display()));
        let target = quote_posix_home_path(&format!("$HOME/{guest_relative_path}"));
        let mode = quote_posix_argument(mode);
        let remote_script = format!(
            r#"set -e
mkdir -p {parent}
cat > {target}
home_uid="$(stat -f %u "$HOME" 2>/dev/null || stat -c %u "$HOME" 2>/dev/null || id -u)"
home_gid="$(stat -f %g "$HOME" 2>/dev/null || stat -c %g "$HOME" 2>/dev/null || id -g)"
if command -v sudo >/dev/null 2>&1; then SUDO=sudo; else SUDO=""; fi
$SUDO chown "$home_uid:$home_gid" {target} 2>/dev/null || true
chmod {mode} {target}"#,
        );
        let remote_script = quote_posix_argument(&remote_script);
        let command = format!("cat {source} | tart exec {instance} bash -lc {remote_script}");

        self.host_shell(&command).run().map_err(|error| {
            VmError::Provider(format!(
                "Failed to seed Tart guest file '{}': {}",
                guest_relative_path, error
            ))
        })?;

        Ok(())
    }

    pub(super) fn git_config_command(&self, config: &VmConfig) -> Option<String> {
        if !config
            .host_sync
            .as_ref()
            .map(|sync| sync.git_config)
            .unwrap_or(true)
        {
            return None;
        }

        let Some(git_config) = &config.git_config else {
            return None;
        };

        let mut commands = Vec::new();
        if let Some(name) = &git_config.user_name {
            commands.push(format!(
                "git config --global user.name {}",
                quote_posix_argument(name)
            ));
        }
        if let Some(email) = &git_config.user_email {
            commands.push(format!(
                "git config --global user.email {}",
                quote_posix_argument(email)
            ));
        }
        if let Some(rebase) = &git_config.pull_rebase {
            commands.push(format!(
                "git config --global pull.rebase {}",
                quote_posix_argument(rebase)
            ));
        }
        if let Some(branch) = &git_config.init_default_branch {
            commands.push(format!(
                "git config --global init.defaultBranch {}",
                quote_posix_argument(branch)
            ));
        }
        if let Some(editor) = &git_config.core_editor {
            commands.push(format!(
                "git config --global core.editor {}",
                quote_posix_argument(editor)
            ));
        }
        if let Some(content) = &git_config.core_excludesfile_content {
            commands.push(format!(
                "printf '%s' {} > \"$HOME/.gitignore_global\"",
                quote_posix_argument(content)
            ));
            commands.push(
                "git config --global core.excludesfile \"$HOME/.gitignore_global\"".to_string(),
            );
        }

        (!commands.is_empty()).then(|| commands.join("\n"))
    }
}
