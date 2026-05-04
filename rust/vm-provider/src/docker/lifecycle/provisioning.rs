//! Container provisioning orchestration
use super::LifecycleOperations;
use crate::context::ProviderContext;
use crate::docker::UserConfig;
use crate::progress::{AnsibleProgressParser, ProgressParser};
use vm_core::command_stream::{
    stream_command_with_progress_and_timeout, ProgressParser as CoreProgressParser,
};
use vm_core::error::{Result, VmError};
use vm_core::vm_error;

use super::{
    ANSIBLE_PLAYBOOK_PATH, CONTAINER_READINESS_MAX_ATTEMPTS, CONTAINER_READINESS_SLEEP_SECONDS,
};

// Default timeout for Ansible provisioning (5 minutes)
// Can be overridden via environment variable ANSIBLE_TIMEOUT
const DEFAULT_ANSIBLE_TIMEOUT_SECS: u64 = 300;

/// Adapter to convert AnsibleProgressParser to CoreProgressParser trait
struct AnsibleParserAdapter(AnsibleProgressParser);

impl CoreProgressParser for AnsibleParserAdapter {
    fn parse_line(&mut self, line: &str) {
        ProgressParser::parse_line(&mut self.0, line);
    }
    fn finish(&self) {
        ProgressParser::finish(&self.0);
    }
}

impl<'a> LifecycleOperations<'a> {
    fn home_ownership_fix_script(user_config: &UserConfig) -> String {
        let home_dir = format!("/home/{}", user_config.username);

        // Comprehensive permission fix for all directories that may be affected
        // by UID/GID mismatch between snapshot and current host.
        //
        // This is the single authoritative place for permission fixes.
        // When adding new cache directories, add them here.
        format!(
            r#"
            user_uid="$(id -u {username})"
            user_gid="$(id -g {username})"

            # Fix home directory and common dotfiles
            chown "$user_uid:$user_gid" {home} 2>/dev/null || true
            chown "$user_uid:$user_gid" {home}/.zshrc {home}/.bashrc {home}/.profile 2>/dev/null || true
            chmod u+rwx,go+rx {home} 2>/dev/null || true
            chmod u+rw,go+r {home}/.zshrc {home}/.bashrc {home}/.profile 2>/dev/null || true

            # Fix NVM (Node.js version manager)
            if [ -d {home}/.nvm ]; then
                chown -R "$user_uid:$user_gid" {home}/.nvm 2>/dev/null || true
                chmod -R u+rwX,go+rX {home}/.nvm 2>/dev/null || true
            fi

            # Fix Cargo/Rust
            if [ -d {home}/.cargo ]; then
                chown -R "$user_uid:$user_gid" {home}/.cargo 2>/dev/null || true
            fi
            if [ -d {home}/.rustup ]; then
                chown -R "$user_uid:$user_gid" {home}/.rustup 2>/dev/null || true
            fi

            # Fix NPM cache
            if [ -d {home}/.npm ]; then
                chown -R "$user_uid:$user_gid" {home}/.npm 2>/dev/null || true
            fi

            # Fix pip/Python cache
            if [ -d {home}/.cache ]; then
                chown -R "$user_uid:$user_gid" {home}/.cache 2>/dev/null || true
            fi

            # Fix local binaries and packages
            if [ -d {home}/.local ]; then
                chown -R "$user_uid:$user_gid" {home}/.local 2>/dev/null || true
            fi

            # Fix config directory
            if [ -d {home}/.config ]; then
                chown -R "$user_uid:$user_gid" {home}/.config 2>/dev/null || true
            fi

            # Fix shell history
            if [ -d {home}/.shell_history ]; then
                chown -R "$user_uid:$user_gid" {home}/.shell_history 2>/dev/null || true
            fi

            # Ensure first-run CLI state paths exist with normal ownership.
            # Claude Code creates a top-level ~/.claude.json file during startup;
            # if HOME itself is owned by a host UID, Claude can launch into a blank
            # screen while it waits on state initialization.
            mkdir -p {home}/.local/bin {home}/.claude/projects {home}/.claude/sessions 2>/dev/null || true
            chown -R "$user_uid:$user_gid" {home}/.local {home}/.claude 2>/dev/null || true
            chmod u+rwx,go+rx {home}/.local {home}/.local/bin {home}/.claude {home}/.claude/projects {home}/.claude/sessions 2>/dev/null || true
            if [ -f {home}/.claude.json ]; then
                chown "$user_uid:$user_gid" {home}/.claude.json 2>/dev/null || true
                chmod 600 {home}/.claude.json 2>/dev/null || true
            fi

            if [ -d {home}/.gemini ]; then
                chown -R "$user_uid:$user_gid" {home}/.gemini 2>/dev/null || true
            fi
            if [ -d {home}/.codex ]; then
                chown -R "$user_uid:$user_gid" {home}/.codex 2>/dev/null || true
            fi

            su -s /bin/sh {username} -c 'touch "$HOME/.vm-home-write-test" && rm -f "$HOME/.vm-home-write-test"' || {{
                echo "ERROR: HOME is not writable by {username}: {home}" >&2
                stat -c '%u %g %U %G %a %n' {home} >&2 || ls -ld {home} >&2
                exit 1
            }}

            echo "Permissions fixed"
        "#,
            username = user_config.username,
            home = home_dir,
        )
    }

    /// Fix home directory ownership for snapshot-based containers (one-time, fast)
    ///
    /// Centralized permission fix-up for layered provisioning:
    /// - Snapshots may have files owned by a different UID than the container user
    /// - This fixes ownership of all key directories that tools need to write to
    /// - Called once after snapshot load, before Ansible provisioning
    ///
    /// Directories handled:
    /// - Home directory and common dotfiles (.zshrc, .bashrc, .profile)
    /// - NVM installation and cache (.nvm)
    /// - Rust/Cargo installation and cache (.cargo, .rustup)
    /// - Python/pip cache (.cache/pip, .local)
    /// - NPM cache (.npm)
    /// - General config directories (.config, .cache)
    /// - Shell history (.shell_history)
    /// - AI CLI state directories and top-level state files (.claude, .claude.json)
    fn fix_home_ownership(
        executable: &str,
        container_name: &str,
        user_config: &UserConfig,
    ) -> Result<()> {
        use std::process::Command;

        let fix_cmd = Self::home_ownership_fix_script(user_config);

        let output = Command::new(executable)
            .args(["exec", "-u", "root", container_name, "bash", "-c", &fix_cmd])
            .output()
            .map_err(|e| VmError::Internal(format!("Failed to fix home ownership: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VmError::Internal(format!(
                "Home ownership repair failed for {}: {}",
                user_config.username,
                stderr.trim()
            )));
        }

        Ok(())
    }

    /// Run Ansible provisioning on a container
    fn run_ansible_provisioning(
        executable: &str,
        container_name: &str,
        context: &ProviderContext,
    ) -> Result<()> {
        let parser = if context.is_verbose() {
            None
        } else {
            Some(
                Box::new(AnsibleParserAdapter(AnsibleProgressParser::new(false)))
                    as Box<dyn CoreProgressParser>,
            )
        };

        // Allow timeout override via environment variable for debugging
        let timeout_secs = std::env::var("ANSIBLE_TIMEOUT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_ANSIBLE_TIMEOUT_SECS);

        // Enable timing instrumentation if ANSIBLE_PROFILE is set
        let enable_profiling = std::env::var("ANSIBLE_PROFILE").is_ok();

        // Pass snapshot status and refresh flag to Ansible
        // - base_preprovisioned: skip redundant base system tasks for snapshots
        // - refresh_packages: force apt-get update and bypass installed package checks
        let extra_vars = format!(
            "--extra-vars 'base_preprovisioned={} refresh_packages={}'",
            context.is_snapshot, context.refresh_packages
        );

        let ansible_cmd = if enable_profiling {
            format!(
                "ANSIBLE_CALLBACKS_ENABLED=profile_tasks ansible-playbook -i localhost, -c local {} {} {}",
                ANSIBLE_PLAYBOOK_PATH,
                context.ansible_verbosity(),
                extra_vars
            )
        } else {
            format!(
                "ansible-playbook -i localhost, -c local {} {} {}",
                ANSIBLE_PLAYBOOK_PATH,
                context.ansible_verbosity(),
                extra_vars
            )
        };

        // Use a throwaway HOME under /tmp to avoid touching /home/developer (which may have wrong UID from snapshots)
        // This is much faster than chown -R on the entire home directory
        let ansible_cmd_with_fix = format!(
            "mkdir -p /tmp/ansible-home /tmp/ansible-local /tmp/ansible-remote && \
             HOME=/tmp/ansible-home ANSIBLE_LOCAL_TEMP=/tmp/ansible-local ANSIBLE_REMOTE_TEMP=/tmp/ansible-remote {}",
            ansible_cmd
        );

        stream_command_with_progress_and_timeout(
            executable,
            &["exec", container_name, "bash", "-c", &ansible_cmd_with_fix],
            parser,
            Some(timeout_secs),
        )
        .map_err(|e| {
            VmError::Internal(format!(
                "Ansible provisioning failed. The playbook exited with an error. Re-run `vm run linux` with debug logging for full output. Error: {e}"
            ))
        })?;

        Ok(())
    }

    /// Wait for container to become ready with exponential backoff
    async fn wait_for_container_ready_async(&self, container_name: &str) -> Result<()> {
        use tokio::time::{sleep, Duration, Instant};

        let start = Instant::now();
        let max_duration = Duration::from_secs(
            u64::from(CONTAINER_READINESS_MAX_ATTEMPTS) * CONTAINER_READINESS_SLEEP_SECONDS,
        );

        // Exponential backoff: 100ms, 200ms, 400ms, 800ms, 1s, 1s, ...
        let mut backoff = Duration::from_millis(100);
        let max_backoff = Duration::from_secs(1);

        loop {
            if crate::docker::DockerOps::test_container_readiness(
                Some(self.executable),
                container_name,
            ) {
                return Ok(());
            }

            if start.elapsed() >= max_duration {
                vm_error!("Container failed to become ready");
                return Err(VmError::Internal(format!(
                    "Container '{}' failed to become ready after {} seconds. Container may be unhealthy or not starting properly",
                    container_name,
                    u64::from(CONTAINER_READINESS_MAX_ATTEMPTS) * CONTAINER_READINESS_SLEEP_SECONDS
                )));
            }

            sleep(backoff).await;
            backoff = (backoff * 2).min(max_backoff);
        }
    }

    /// Wait for container to become ready (synchronous wrapper)
    fn wait_for_container_ready(&self, container_name: &str) -> Result<()> {
        // Use block_in_place to call async code from sync context within an existing runtime
        // This avoids creating a nested runtime which would panic
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(self.wait_for_container_ready_async(container_name))
        })
    }

    /// Re-provision existing container (public API)
    pub fn provision_existing(&self, container: Option<&str>) -> Result<()> {
        let context = ProviderContext::default();
        let target_container = self.resolve_target_container(container)?;
        let status_output = std::process::Command::new(self.executable)
            .args([
                "inspect",
                "--format",
                "{{.State.Status}}",
                &target_container,
            ])
            .output()?;
        let status = String::from_utf8_lossy(&status_output.stdout)
            .trim()
            .to_owned();
        if status != "running" {
            return Err(VmError::Internal(format!(
                "Container {target_container} is not running. Start it first with 'vm run linux'"
            )));
        }

        self.provision_container_with_context(&context)
    }

    /// Internal provisioning with context
    pub(super) fn provision_container_with_context(&self, context: &ProviderContext) -> Result<()> {
        // Skip provisioning if requested (e.g., for snapshot builds from Dockerfiles)
        if context.skip_provisioning {
            return Ok(());
        }

        let container_name = self.container_name();

        // Step 6: Wait for readiness and run ansible (configuration only)
        self.wait_for_container_ready(&container_name)?;

        // Fix home directory ownership for snapshot-based containers (one-time, fast)
        // This ensures tools like nvm, cargo work regardless of host UID
        if context.is_snapshot {
            let user_config = UserConfig::from_vm_config(self.config);
            Self::fix_home_ownership(self.executable, &container_name, &user_config)?;
        }

        self.prepare_and_copy_config(&container_name)?;

        Self::run_ansible_provisioning(self.executable, &container_name, context)
    }

    /// Provision container with custom instance name and context
    pub(super) fn provision_container_with_instance_and_context(
        &self,
        instance_name: &str,
        context: &ProviderContext,
    ) -> Result<()> {
        // Skip provisioning if requested (e.g., for snapshot builds from Dockerfiles)
        if context.skip_provisioning {
            return Ok(());
        }

        let container_name = self.container_name_with_instance(instance_name);

        // Step 6: Wait for readiness and run ansible (configuration only)
        self.wait_for_container_ready(&container_name)?;

        // Fix home directory ownership for snapshot-based containers (one-time, fast)
        // This ensures tools like nvm, cargo work regardless of host UID
        if context.is_snapshot {
            let user_config = UserConfig::from_vm_config(self.config);
            Self::fix_home_ownership(self.executable, &container_name, &user_config)?;
        }

        self.prepare_and_copy_config(&container_name)?;

        Self::run_ansible_provisioning(self.executable, &container_name, context)
    }
}

#[cfg(test)]
mod tests {
    use super::LifecycleOperations;
    use crate::docker::UserConfig;

    #[test]
    fn home_ownership_fix_script_repairs_claude_first_run_state_paths() {
        let user_config = UserConfig {
            uid: 1000,
            gid: 1000,
            username: "developer".to_string(),
        };

        let script = LifecycleOperations::home_ownership_fix_script(&user_config);

        assert!(script.contains("user_uid=\"$(id -u developer)\""));
        assert!(script.contains("user_gid=\"$(id -g developer)\""));
        assert!(script.contains("chown \"$user_uid:$user_gid\" /home/developer"));
        assert!(script.contains("/home/developer/.claude/projects"));
        assert!(script.contains("/home/developer/.claude/sessions"));
        assert!(script.contains("/home/developer/.claude.json"));
        assert!(script.contains(".vm-home-write-test"));
        assert!(script.contains("ERROR: HOME is not writable by developer"));
    }
}
