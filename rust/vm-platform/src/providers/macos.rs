//! macOS platform provider implementation.
//!
//! macOS is Unix-like but has some specific differences in directory locations
//! and system information gathering.

use crate::providers::shells::{BashShell, FishShell, ZshShell};
use crate::traits::{PlatformProvider, ProcessProvider, ShellProvider};
use anyhow::{Context, Result};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// macOS platform provider
pub struct MacOSPlatform;

impl PlatformProvider for MacOSPlatform {
    fn name(&self) -> &'static str {
        "macos"
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
        Ok(self.home_dir()?.join(".local").join("bin"))
    }

    fn user_cache_dir(&self) -> Result<PathBuf> {
        Ok(dirs::cache_dir()
            .context("Could not determine user cache directory")?
            .join("vm"))
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
        let shell = env::var("SHELL").unwrap_or_default();

        match shell.split('/').next_back() {
            Some("bash") => Ok(Box::new(BashShell)),
            Some("zsh") => Ok(Box::new(ZshShell)), // Default on newer macOS
            Some("fish") => Ok(Box::new(FishShell)),
            _ => {
                // Default to zsh for macOS (Catalina and later)
                Ok(Box::new(ZshShell))
            }
        }
    }

    // === Binary Operations ===

    fn executable_name(&self, base: &str) -> String {
        base.to_string()
    }

    fn install_executable(&self, source: &Path, dest_dir: &Path, name: &str) -> Result<()> {
        std::fs::create_dir_all(dest_dir).context("Failed to create destination directory")?;

        let dest = dest_dir.join(name);

        // Remove existing file/symlink if it exists
        if dest.exists() || dest.is_symlink() {
            std::fs::remove_file(&dest).context("Failed to remove existing file/symlink")?;
        }

        // Create symlink
        std::os::unix::fs::symlink(source, &dest).context("Failed to create symlink")?;

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
                return Ok(Some(PathBuf::from(prefix).join("lib").join("node_modules")));
            }
        }

        // Fallback to common macOS locations
        let home = self.home_dir()?;
        let candidates = [
            home.join(".npm-global").join("lib").join("node_modules"),
            home.join(".local").join("lib").join("node_modules"),
            PathBuf::from("/usr/local/lib/node_modules"), // Homebrew location
        ];

        for candidate in &candidates {
            if candidate.exists() {
                return Ok(Some(candidate.clone()));
            }
        }

        Ok(None)
    }

    fn nvm_versions_dir(&self) -> Result<Option<PathBuf>> {
        let home = self.home_dir()?;
        let nvm_dir = home.join(".nvm").join("versions").join("node");

        if nvm_dir.exists() {
            Ok(Some(nvm_dir))
        } else {
            Ok(None)
        }
    }

    fn python_site_packages(&self) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();

        // Try to get Python site-packages directories
        let python_commands = ["python3", "python"];

        for cmd in &python_commands {
            if self.try_get_python_paths(cmd, &mut paths)? {
                break; // Use first working Python
            }
        }

        Ok(paths)
    }

    // === System Information ===

    fn cpu_core_count(&self) -> Result<u32> {
        // Use sysctl for macOS
        let output = Command::new("sysctl")
            .args(["-n", "hw.physicalcpu"])
            .output()
            .context("Failed to execute sysctl")?;

        if output.status.success() {
            let cpu_count: u32 = String::from_utf8(output.stdout)?
                .trim()
                .parse()
                .context("Failed to parse CPU count")?;
            Ok(cpu_count)
        } else {
            // Fallback to sysinfo
            let mut sys = sysinfo::System::new();
            sys.refresh_cpu();
            Ok(sys.physical_core_count().unwrap_or(1) as u32)
        }
    }

    fn total_memory_gb(&self) -> Result<u64> {
        // Use sysctl for macOS
        let output = Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .context("Failed to execute sysctl")?;

        if output.status.success() {
            let mem_bytes: u64 = String::from_utf8(output.stdout)?
                .trim()
                .parse()
                .context("Failed to parse memory size")?;
            Ok(mem_bytes / 1024 / 1024 / 1024) // Convert bytes to GB
        } else {
            // Fallback to sysinfo
            let mut sys = sysinfo::System::new();
            sys.refresh_memory();
            Ok(sys.total_memory() / 1024 / 1024 / 1024)
        }
    }

    // === Process Operations ===

    fn path_separator(&self) -> char {
        ':'
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

impl MacOSPlatform {
    fn try_get_python_paths(&self, cmd: &str, paths: &mut Vec<PathBuf>) -> Result<bool> {
        let output = Command::new(cmd)
            .args([
                "-c",
                "import site; print('\\n'.join(site.getsitepackages()))",
            ])
            .output();

        let Ok(output) = output else {
            return Ok(false);
        };

        if !output.status.success() {
            return Ok(false);
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        for path in output_str.lines() {
            let path = PathBuf::from(path.trim());
            if path.exists() && !paths.contains(&path) {
                paths.push(path);
            }
        }

        Ok(true)
    }
}

/// macOS process provider (same as Unix for most purposes)
pub struct MacOSProcessProvider;

impl ProcessProvider for MacOSProcessProvider {
    fn prepare_command(&self, _cmd: &mut Command) -> Result<()> {
        // No special preparation needed for macOS
        Ok(())
    }

    fn default_shell_command(&self) -> (&'static str, Vec<&'static str>) {
        ("sh", vec!["-c"])
    }

    fn command_exists(&self, command: &str) -> bool {
        Command::new("which")
            .arg(command)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}
