//! Unix platform provider implementation.

use crate::providers::shared::SharedPlatformOps;
use crate::providers::shells::{BashShell, FishShell, ZshShell};
use crate::traits::{PlatformProvider, ProcessProvider, ShellProvider};
use anyhow::{Context, Result};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Unix platform provider (Linux and other Unix-like systems)
pub struct UnixPlatform;

// Use shared implementations
impl SharedPlatformOps for UnixPlatform {}

impl PlatformProvider for UnixPlatform {
    fn name(&self) -> &'static str {
        "unix"
    }

    // === Path Operations ===

    fn user_config_dir(&self) -> Result<PathBuf> {
        self.default_user_config_dir()
    }

    fn user_data_dir(&self) -> Result<PathBuf> {
        self.default_user_data_dir()
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
        self.default_home_dir()
    }

    fn vm_state_dir(&self) -> Result<PathBuf> {
        self.default_vm_state_dir()
    }

    fn global_config_path(&self) -> Result<PathBuf> {
        self.default_global_config_path()
    }

    fn port_registry_path(&self) -> Result<PathBuf> {
        self.default_port_registry_path()
    }

    // === Shell Operations ===

    fn detect_shell(&self) -> Result<Box<dyn ShellProvider>> {
        let shell = env::var("SHELL").unwrap_or_default();

        match shell.split('/').next_back() {
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
        self.default_cargo_home()
    }

    fn cargo_bin_dir(&self) -> Result<PathBuf> {
        self.default_cargo_bin_dir()
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
                .args([
                    "-c",
                    "import site; print('\\n'.join(site.getsitepackages()))",
                ])
                .output()
            {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    add_unique_site_packages(&output_str, &mut paths);
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
        if let Some(memory_gb) = parse_memory_from_proc_meminfo() {
            return Ok(memory_gb);
        }

        // Fallback to sysinfo
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        Ok(sys.total_memory() / 1024 / 1024 / 1024)
    }

    // === Process Operations ===

    fn path_separator(&self) -> char {
        self.default_path_separator()
    }

    fn split_path_env(&self, path: &str) -> Vec<PathBuf> {
        self.default_split_path_env(path)
    }

    fn join_path_env(&self, paths: &[PathBuf]) -> String {
        self.default_join_path_env(paths)
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

/// Parse memory from /proc/meminfo, returning memory in GB
fn parse_memory_from_proc_meminfo() -> Option<u64> {
    let meminfo = match std::fs::read_to_string("/proc/meminfo") {
        Ok(content) => content,
        Err(_) => return None,
    };

    for line in meminfo.lines() {
        if !line.starts_with("MemTotal:") {
            continue;
        }

        let mem_kb_str = match line.split_whitespace().nth(1) {
            Some(value) => value,
            None => continue,
        };

        let mem_kb = match mem_kb_str.parse::<u64>() {
            Ok(value) => value,
            Err(_) => continue,
        };

        return Some(mem_kb / 1024 / 1024); // Convert KB to GB
    }

    None
}

/// Helper function to add unique site packages from command output
fn add_unique_site_packages(output_str: &str, paths: &mut Vec<PathBuf>) {
    for path in output_str.lines() {
        let path = PathBuf::from(path.trim());
        if path.exists() && !paths.contains(&path) {
            paths.push(path);
        }
    }
}
