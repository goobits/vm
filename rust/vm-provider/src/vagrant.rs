use crate::utils::{is_tool_installed, stream_command};
use crate::{
    error::ProviderError, progress::ProgressReporter, security::SecurityValidator, Provider,
};
use anyhow::{Context, Result};
use std::env;
use std::path::Path;
use vm_common::{vm_println, vm_success, vm_warning};
use vm_config::config::VmConfig;

/// Safely escape a string for shell execution by wrapping in single quotes
/// and escaping any existing single quotes
fn shell_escape(arg: &str) -> String {
    // If the argument contains no special characters, return as-is
    if arg
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/')
    {
        arg.to_string()
    } else {
        // Wrap in single quotes and escape any existing single quotes
        format!("'{}'", arg.replace('\'', "'\"'\"'"))
    }
}

pub struct VagrantProvider {
    config: VmConfig,
    project_dir: std::path::PathBuf,
}

impl VagrantProvider {
    pub fn new(config: VmConfig) -> Result<Self> {
        if !is_tool_installed("vagrant") {
            return Err(ProviderError::DependencyNotFound("Vagrant".into()).into());
        }
        let project_dir = env::current_dir()?;
        Ok(Self {
            config,
            project_dir,
        })
    }

    fn run_vagrant_command(&self, args: &[&str]) -> Result<()> {
        // Set VAGRANT_CWD to providers/vagrant directory
        let vagrant_cwd = self.project_dir.join("providers/vagrant");

        // Create sanitized config without sensitive environment variables
        let sanitized_config = self.create_sanitized_config();
        let config_json = serde_json::to_string_pretty(&sanitized_config)?;

        // Set environment variables
        env::set_var("VAGRANT_CWD", vagrant_cwd);
        env::set_var("VM_PROJECT_DIR", &self.project_dir);
        env::set_var("VM_CONFIG_JSON", config_json);

        stream_command("vagrant", args)
    }

    /// Create a sanitized version of the config that excludes sensitive environment variables
    fn create_sanitized_config(&self) -> VmConfig {
        let mut sanitized = self.config.clone();

        // Remove potentially sensitive environment variables to prevent exposure
        // Only expose essential non-sensitive environment for Vagrant provisioning
        if !sanitized.environment.is_empty() {
            let safe_env_prefixes = ["VM_", "VAGRANT_", "LANG", "LC_", "PATH", "HOME", "USER"];

            sanitized.environment.retain(|key, _| {
                // Keep environment variables that are clearly safe
                safe_env_prefixes.iter().any(|prefix| key.starts_with(prefix)) ||
                // Keep standard non-sensitive variables
                matches!(key.as_str(), "TERM" | "SHELL" | "EDITOR" | "PAGER")
            });
        }

        sanitized
    }
}

impl Provider for VagrantProvider {
    fn name(&self) -> &'static str {
        "vagrant"
    }

    fn create(&self) -> Result<()> {
        let progress = ProgressReporter::new();
        let main_phase = progress.start_phase("Creating Vagrant Environment");

        // Check if VM already exists
        ProgressReporter::task(&main_phase, "Checking existing VM status...");
        let status_output = std::process::Command::new("vagrant")
            .env("VAGRANT_CWD", self.project_dir.join("providers/vagrant"))
            .args(["status", "default"])
            .output();

        if let Ok(output) = status_output {
            let status_str = String::from_utf8_lossy(&output.stdout);
            if status_str.contains("running")
                || status_str.contains("poweroff")
                || status_str.contains("saved")
            {
                ProgressReporter::task(&main_phase, "VM already exists.");
                vm_warning!("Vagrant VM already exists.");
                vm_println!("To recreate, first run: vm destroy");
                ProgressReporter::finish_phase(&main_phase, "Skipped creation.");
                return Ok(());
            }
        }
        ProgressReporter::task(&main_phase, "No existing VM found.");

        // Start VM with full provisioning
        ProgressReporter::task(&main_phase, "Starting Vagrant VM with provisioning...");
        let vagrant_cwd = self.project_dir.join("providers/vagrant");

        // Create sanitized config without sensitive environment variables
        let sanitized_config = self.create_sanitized_config();
        let config_json = serde_json::to_string_pretty(&sanitized_config)?;

        env::set_var("VAGRANT_CWD", &vagrant_cwd);
        env::set_var("VM_PROJECT_DIR", &self.project_dir);
        env::set_var("VM_CONFIG_JSON", config_json);

        let result = stream_command("vagrant", &["up"]);

        if result.is_err() {
            ProgressReporter::task(&main_phase, "VM creation failed.");
            ProgressReporter::finish_phase(&main_phase, "Creation failed.");
            return result;
        }

        ProgressReporter::task(&main_phase, "VM created successfully.");
        ProgressReporter::finish_phase(&main_phase, "Environment ready.");

        vm_success!("Vagrant environment created successfully!");
        vm_println!("Use 'vm ssh' to connect to the VM");
        Ok(())
    }

    fn start(&self) -> Result<()> {
        self.run_vagrant_command(&["resume"])
            .or_else(|_| self.run_vagrant_command(&["up"]))
    }

    fn stop(&self) -> Result<()> {
        self.run_vagrant_command(&["halt"])
    }

    fn destroy(&self) -> Result<()> {
        self.run_vagrant_command(&["destroy", "-f"])
    }

    fn ssh(&self, relative_path: &Path) -> Result<()> {
        let vagrant_cwd = self.project_dir.join("providers/vagrant");
        env::set_var("VAGRANT_CWD", &vagrant_cwd);

        if relative_path.as_os_str().is_empty() || relative_path == Path::new(".") {
            // Simple SSH without path change
            duct::cmd("vagrant", &["ssh"]).run()?;
        } else {
            // SSH with directory change
            let workspace_path = self
                .config
                .project
                .as_ref()
                .and_then(|p| p.workspace_path.as_deref())
                .unwrap_or("/workspace");

            // Validate and calculate target directory (prevent path traversal)
            let target_path =
                SecurityValidator::validate_relative_path(relative_path, workspace_path)
                    .context("Invalid path for SSH operation")?;
            let target_dir = target_path.to_string_lossy();

            // Get shell from config (terminal.shell or default to bash)
            let shell = self
                .config
                .terminal
                .as_ref()
                .and_then(|t| t.shell.as_deref())
                .unwrap_or("bash");

            // Use safe argument passing - avoid shell interpolation by using printf and exec
            // This prevents injection even if target_dir or shell contain special characters
            let safe_cmd = "cd \"$1\" && exec \"$2\"".to_string();

            duct::cmd(
                "vagrant",
                &["ssh", "-c", &safe_cmd, "--", &target_dir, shell],
            )
            .run()?;
        }
        Ok(())
    }

    fn exec(&self, cmd: &[String]) -> Result<()> {
        // Safely escape each argument for shell execution
        let escaped_args: Vec<String> = cmd.iter().map(|arg| shell_escape(arg)).collect();
        let safe_cmd = escaped_args.join(" ");
        self.run_vagrant_command(&["ssh", "-c", &safe_cmd])
    }

    fn logs(&self) -> Result<()> {
        vm_println!("Showing service logs - Press Ctrl+C to stop...");
        self.run_vagrant_command(&[
            "ssh",
            "-c",
            "sudo journalctl -u postgresql -u redis-server -u mongod -f",
        ])
    }

    fn status(&self) -> Result<()> {
        self.run_vagrant_command(&["status"])
    }

    fn restart(&self) -> Result<()> {
        // Use vagrant reload to restart
        self.run_vagrant_command(&["reload"])
    }

    fn provision(&self) -> Result<()> {
        // Use vagrant provision to re-run provisioning
        self.run_vagrant_command(&["provision"])
    }

    fn list(&self) -> Result<()> {
        // Use vagrant global-status to list all VMs
        stream_command("vagrant", &["global-status"])
    }

    fn kill(&self, _container: Option<&str>) -> Result<()> {
        // Force destroy the VM
        self.run_vagrant_command(&["destroy", "-f"])
    }

    fn get_sync_directory(&self) -> String {
        // Return workspace_path from config
        self
            .config
            .project
            .as_ref()
            .and_then(|p| p.workspace_path.as_deref())
            .unwrap_or("/workspace")
            .to_string()
    }
}
