use super::TartProvisioner;
use crate::tart::host_sync::{
    collect_host_sync_mounts, expand_tilde, file_name, resolve_guest_home_path, resolve_home_dir,
};
use std::path::Path;
use vm_config::config::VmConfig;
use vm_core::error::{Result, VmError};

impl TartProvisioner {
    pub(super) fn ensure_host_sync_mounts(&self, config: &VmConfig) -> Result<()> {
        let mounts = collect_host_sync_mounts(config);
        if mounts.is_empty() {
            return Ok(());
        }

        let mut commands = Vec::new();
        for mount in mounts {
            let guest_path = resolve_guest_home_path(&mount.guest_path);
            commands.push(Self::virtiofs_mount_command(&mount.tag, &guest_path));
        }

        self.ssh_exec(&commands.join("\n"))?;
        Ok(())
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
        let guest_target_escaped = Self::shell_escape_single_quotes(guest_target);
        let guest_parent = Path::new(guest_target)
            .parent()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| ".".to_string());
        let guest_parent_escaped = Self::shell_escape_single_quotes(&guest_parent);

        if source.is_file() {
            let source_escaped = Self::shell_escape_single_quotes(&source.to_string_lossy());
            let command = format!(
                "cat '{}' | tart exec {} bash -lc \"mkdir -p '{}' && cat > '{}'\"",
                source_escaped, self.instance_name, guest_parent_escaped, guest_target_escaped
            );
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
            let parent_escaped = Self::shell_escape_single_quotes(&parent.to_string_lossy());
            let name_escaped = Self::shell_escape_single_quotes(&name);
            let command = format!(
                "tar -C '{}' -cf - '{}' | tart exec {} bash -lc \"mkdir -p '{}' && tar -xf - -C '{}'\"",
                parent_escaped,
                name_escaped,
                self.instance_name,
                guest_parent_escaped,
                guest_parent_escaped
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

        let source_escaped = Self::shell_escape_single_quotes(&source.to_string_lossy());
        let instance_escaped = Self::shell_escape_single_quotes(&self.instance_name);
        let parent_escaped = Self::shell_escape_single_quotes(&parent.to_string_lossy());
        let target_escaped = Self::shell_escape_single_quotes(guest_relative_path);
        let mode_escaped = Self::shell_escape_single_quotes(mode);
        let remote_script = format!(
            r#"set -e
mkdir -p "$HOME/{parent}"
cat > "$HOME/{target}"
home_uid="$(stat -f %u "$HOME" 2>/dev/null || stat -c %u "$HOME" 2>/dev/null || id -u)"
home_gid="$(stat -f %g "$HOME" 2>/dev/null || stat -c %g "$HOME" 2>/dev/null || id -g)"
if command -v sudo >/dev/null 2>&1; then SUDO=sudo; else SUDO=""; fi
$SUDO chown "$home_uid:$home_gid" "$HOME/{target}" 2>/dev/null || true
chmod {mode} "$HOME/{target}""#,
            parent = parent_escaped,
            target = target_escaped,
            mode = mode_escaped
        );
        let remote_script_escaped = Self::shell_escape_single_quotes(&remote_script);
        let command = format!(
            "cat '{source}' | tart exec '{instance}' bash -lc '{script}'",
            source = source_escaped,
            instance = instance_escaped,
            script = remote_script_escaped
        );

        self.host_shell(&command).run().map_err(|error| {
            VmError::Provider(format!(
                "Failed to seed Tart guest file '{}': {}",
                guest_relative_path, error
            ))
        })?;

        Ok(())
    }

    pub(super) fn apply_git_config(&self, config: &VmConfig) -> Result<()> {
        if !config
            .host_sync
            .as_ref()
            .map(|sync| sync.git_config)
            .unwrap_or(true)
        {
            return Ok(());
        }

        let Some(git_config) = &config.git_config else {
            return Ok(());
        };

        let mut commands = Vec::new();
        if let Some(name) = &git_config.user_name {
            commands.push(format!(
                "git config --global user.name '{}'",
                Self::shell_escape_single_quotes(name)
            ));
        }
        if let Some(email) = &git_config.user_email {
            commands.push(format!(
                "git config --global user.email '{}'",
                Self::shell_escape_single_quotes(email)
            ));
        }
        if let Some(rebase) = &git_config.pull_rebase {
            commands.push(format!(
                "git config --global pull.rebase '{}'",
                Self::shell_escape_single_quotes(rebase)
            ));
        }
        if let Some(branch) = &git_config.init_default_branch {
            commands.push(format!(
                "git config --global init.defaultBranch '{}'",
                Self::shell_escape_single_quotes(branch)
            ));
        }
        if let Some(editor) = &git_config.core_editor {
            commands.push(format!(
                "git config --global core.editor '{}'",
                Self::shell_escape_single_quotes(editor)
            ));
        }
        if let Some(content) = &git_config.core_excludesfile_content {
            commands.push(format!(
                "cat > \"$HOME/.gitignore_global\" <<'EOF'\n{}\nEOF",
                content
            ));
            commands.push(
                "git config --global core.excludesfile \"$HOME/.gitignore_global\"".to_string(),
            );
        }

        if commands.is_empty() {
            return Ok(());
        }

        self.ssh_exec(&commands.join("\n"))?;
        Ok(())
    }
}
