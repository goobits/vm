//! Windows platform provider implementation.

use crate::providers::shells::{CmdShell, PowerShell};
use crate::traits::{PlatformProvider, ProcessProvider, ShellProvider};
use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Windows platform provider
pub struct WindowsPlatform;

impl PlatformProvider for WindowsPlatform {
    fn name(&self) -> &'static str {
        "windows"
    }

    // === Path Operations ===

    fn user_config_dir(&self) -> Result<PathBuf> {
        Ok(dirs::config_dir()
            .context("Could not determine user config directory")?
            .join("vm"))
    }

    fn user_data_dir(&self) -> Result<PathBuf> {
        Ok(dirs::data_dir()
            .context("Could not determine user data directory")?
            .join("vm"))
    }

    fn user_bin_dir(&self) -> Result<PathBuf> {
        Ok(dirs::data_local_dir()
            .context("Could not determine local app data directory")?
            .join("vm")
            .join("bin"))
    }

    fn user_cache_dir(&self) -> Result<PathBuf> {
        Ok(dirs::data_local_dir()
            .context("Could not determine local app data directory")?
            .join("vm")
            .join("cache"))
    }

    fn home_dir(&self) -> Result<PathBuf> {
        dirs::home_dir().context("Could not determine home directory")
    }

    fn vm_state_dir(&self) -> Result<PathBuf> {
        Ok(self.home_dir()?.join(".vm"))
    }

    fn global_config_path(&self) -> Result<PathBuf> {
        Ok(self.user_config_dir()?.join("global.yaml"))
    }

    fn port_registry_path(&self) -> Result<PathBuf> {
        Ok(self.vm_state_dir()?.join("port-registry.json"))
    }

    // === Shell Operations ===

    fn detect_shell(&self) -> Result<Box<dyn ShellProvider>> {
        // Check for PowerShell first (most common for developers)
        if env::var("PSModulePath").is_ok() || env::var("PSVERSION").is_ok() {
            Ok(Box::new(PowerShell))
        } else {
            // Fallback to CMD
            Ok(Box::new(CmdShell))
        }
    }

    // === Binary Operations ===

    fn executable_name(&self, base: &str) -> String {
        if base.ends_with(".exe") {
            base.to_string()
        } else {
            format!("{}.exe", base)
        }
    }

    fn install_executable(&self, source: &Path, dest_dir: &Path, name: &str) -> Result<()> {
        fs::create_dir_all(dest_dir).context("Failed to create destination directory")?;

        let exe_name = self.executable_name(name);
        let dest = dest_dir.join(&exe_name);

        // Remove existing file if it exists
        if dest.exists() {
            fs::remove_file(&dest).context("Failed to remove existing executable")?;
        }

        // Copy the executable
        fs::copy(source, &dest).context("Failed to copy executable")?;

        // Create a batch file wrapper for better PATH integration
        let bat_name = format!("{}.bat", name);
        let bat_path = dest_dir.join(bat_name);
        let bat_content = format!("@echo off\n\"{}\" %*", dest.display());

        fs::write(&bat_path, bat_content).context("Failed to create batch wrapper")?;

        // Also create a PowerShell wrapper
        let ps1_name = format!("{}.ps1", name);
        let ps1_path = dest_dir.join(ps1_name);
        let ps1_content = format!("& \"{}\" @args", dest.display());

        fs::write(&ps1_path, ps1_content).context("Failed to create PowerShell wrapper")?;

        Ok(())
    }

    // === Package Manager Paths ===

    fn cargo_home(&self) -> Result<PathBuf> {
        // Check CARGO_HOME environment variable first
        if let Ok(cargo_home) = env::var("CARGO_HOME") {
            Ok(PathBuf::from(cargo_home))
        } else {
            Ok(self.home_dir()?.join(".cargo"))
        }
    }

    fn cargo_bin_dir(&self) -> Result<PathBuf> {
        Ok(self.cargo_home()?.join("bin"))
    }

    fn npm_global_dir(&self) -> Result<Option<PathBuf>> {
        // Try to get npm global directory
        if let Ok(output) = Command::new("npm")
            .args(["config", "get", "prefix"])
            .output()
        {
            if output.status.success() {
                let prefix_str = String::from_utf8_lossy(&output.stdout);
                let prefix = prefix_str.trim();
                return Ok(Some(PathBuf::from(prefix).join("node_modules")));
            }
        }

        // Fallback to common Windows npm locations
        let appdata = dirs::data_dir().context("Could not determine app data directory")?;

        let candidates = [
            appdata.join("npm").join("node_modules"),
            self.home_dir()?
                .join("AppData")
                .join("Roaming")
                .join("npm")
                .join("node_modules"),
        ];

        for candidate in &candidates {
            if candidate.exists() {
                return Ok(Some(candidate.clone()));
            }
        }

        Ok(None)
    }

    fn nvm_versions_dir(&self) -> Result<Option<PathBuf>> {
        // Windows NVM typically installs to different locations
        let candidates = [
            env::var("NVM_HOME").ok().map(PathBuf::from),
            Some(self.home_dir()?.join("AppData").join("Roaming").join("nvm")),
            Some(PathBuf::from("C:\\Program Files\\nodejs")),
        ];

        for candidate in candidates.into_iter().flatten() {
            if candidate.exists() {
                return Ok(Some(candidate));
            }
        }

        Ok(None)
    }

    fn python_site_packages(&self) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();

        // Try to get Python site-packages directories
        let python_commands = ["python", "python3", "py"];

        for cmd in &python_commands {
            if let Ok(output) = Command::new(cmd)
                .args([
                    "-c",
                    "import site; print('\\n'.join(site.getsitepackages()))",
                ])
                .output()
            {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    for path in output_str.lines() {
                        let path = PathBuf::from(path.trim());
                        if path.exists() && !paths.contains(&path) {
                            paths.push(path);
                        }
                    }
                    break; // Use first working Python
                }
            }
        }

        Ok(paths)
    }

    // === System Information ===

    fn cpu_core_count(&self) -> Result<u32> {
        let mut sys = sysinfo::System::new();
        sys.refresh_cpu();
        Ok(sys.physical_core_count().unwrap_or(1) as u32)
    }

    fn total_memory_gb(&self) -> Result<u64> {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        Ok(sys.total_memory() / 1024 / 1024 / 1024)
    }

    // === Process Operations ===

    fn path_separator(&self) -> char {
        ';'
    }

    fn split_path_env(&self, path: &str) -> Vec<PathBuf> {
        env::split_paths(path).collect()
    }

    fn join_path_env(&self, paths: &[PathBuf]) -> String {
        env::join_paths(paths)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// Windows process provider
pub struct WindowsProcessProvider;

impl ProcessProvider for WindowsProcessProvider {
    fn prepare_command(&self, cmd: &mut Command) -> Result<()> {
        // Set Windows-specific environment variables if needed
        // For now, no special preparation is required
        Ok(())
    }

    fn default_shell_command(&self) -> (&'static str, Vec<&'static str>) {
        ("cmd", vec!["/C"])
    }

    fn command_exists(&self, command: &str) -> bool {
        Command::new("where")
            .arg(command)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}
