//! Unix platform provider implementation.

use crate::traits::{PlatformProvider, ShellProvider, ProcessProvider};
use crate::providers::shells::{BashShell, ZshShell, FishShell};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::env;

/// Unix platform provider (Linux and other Unix-like systems)
pub struct UnixPlatform;

impl PlatformProvider for UnixPlatform {
    fn name(&self) -> &'static str {
        "unix"
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

        match shell.split('/').last() {
            Some("bash") => Ok(Box::new(BashShell)),
            Some("zsh") => Ok(Box::new(ZshShell)),
            Some("fish") => Ok(Box::new(FishShell)),
            _ => {
                // Default to bash if we can't detect
                Ok(Box::new(BashShell))
            }
        }
    }

    // === Binary Operations ===

    fn executable_name(&self, base: &str) -> String {
        base.to_string()
    }

    fn install_executable(&self, source: &Path, dest_dir: &Path, name: &str) -> Result<()> {
        std::fs::create_dir_all(dest_dir)
            .context("Failed to create destination directory")?;

        let dest = dest_dir.join(name);

        // Remove existing file/symlink if it exists
        if dest.exists() || dest.is_symlink() {
            std::fs::remove_file(&dest)
                .context("Failed to remove existing file/symlink")?;
        }

        // Create symlink
        std::os::unix::fs::symlink(source, &dest)
            .context("Failed to create symlink")?;

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
        if let Ok(output) = Command::new("npm").args(["config", "get", "prefix"]).output() {
            if output.status.success() {
                let prefix_str = String::from_utf8_lossy(&output.stdout);
                let prefix = prefix_str.trim();
                return Ok(Some(PathBuf::from(prefix).join("lib").join("node_modules")));
            }
        }

        // Fallback to common locations
        let home = self.home_dir()?;
        let candidates = [
            home.join(".npm-global").join("lib").join("node_modules"),
            home.join(".local").join("lib").join("node_modules"),
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
            if let Ok(output) = Command::new(cmd)
                .args(["-c", "import site; print('\\n'.join(site.getsitepackages()))"])
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
        // Try reading from /proc/cpuinfo first (Linux)
        if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
            let core_count = cpuinfo
                .lines()
                .filter(|line| line.starts_with("processor"))
                .count() as u32;
            return Ok(core_count);
        }

        // Fallback to sysinfo
        let mut sys = sysinfo::System::new();
        sys.refresh_cpu();
        Ok(sys.physical_core_count().unwrap_or(1) as u32)
    }

    fn total_memory_gb(&self) -> Result<u64> {
        // Try reading from /proc/meminfo first (Linux)
        if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            for line in meminfo.lines() {
                if line.starts_with("MemTotal:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(mem_kb) = parts[1].parse::<u64>() {
                            return Ok(mem_kb / 1024 / 1024); // Convert KB to GB
                        }
                    }
                }
            }
        }

        // Fallback to sysinfo
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        Ok(sys.total_memory() / 1024 / 1024 / 1024)
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

/// Unix process provider
pub struct UnixProcessProvider;

impl ProcessProvider for UnixProcessProvider {
    fn prepare_command(&self, _cmd: &mut Command) -> Result<()> {
        // No special preparation needed for Unix
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