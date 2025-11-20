//! Helper utilities for lifecycle operations
use super::LifecycleOperations;
use crate::docker::command::DockerCommand;
use vm_core::error::{Result, VmError};
use vm_core::vm_error_with_details;

// Constants (moved from top of lifecycle.rs)
pub(super) const DEFAULT_PROJECT_NAME: &str = "vm-project";
pub(super) const CONTAINER_SUFFIX: &str = "-dev";
pub(super) const HIGH_MEMORY_THRESHOLD: u32 = 8192;
pub(super) const DEFAULT_WORKSPACE_PATH: &str = "/workspace";

impl<'a> LifecycleOperations<'a> {
    /// Extract project name from config or default
    pub fn project_name(&self) -> &str {
        self.config
            .project
            .as_ref()
            .and_then(|p| p.name.as_deref())
            .unwrap_or(DEFAULT_PROJECT_NAME)
    }

    /// Generate default container name
    pub fn container_name(&self) -> String {
        format!("{}{}", self.project_name(), CONTAINER_SUFFIX)
    }

    /// Generate container name with instance suffix
    pub fn container_name_with_instance(&self, instance_name: &str) -> String {
        format!("{}-{}", self.project_name(), instance_name)
    }

    /// Resolve target container (from Option or default)
    pub fn resolve_target_container(&self, container: Option<&str>) -> Result<String> {
        match container {
            None => Ok(self.container_name()),
            Some(name) => self.resolve_container_name(name),
        }
    }

    /// Get sync directory path
    pub fn get_sync_directory(&self) -> String {
        self.config
            .project
            .as_ref()
            .and_then(|p| p.workspace_path.as_deref())
            .unwrap_or("/workspace")
            .to_string()
    }

    // Private validation helpers (pub(super) for cross-module use)
    pub(super) fn check_memory_allocation(&self, vm_config: &vm_config::config::VmSettings) {
        if let Some(memory) = &vm_config.memory {
            match memory.to_mb() {
                Some(mb) if mb > HIGH_MEMORY_THRESHOLD => {
                    vm_core::vm_error_hint!("High memory allocation detected ({}MB). Ensure your system has sufficient RAM.", mb);
                }
                None => {
                    vm_core::vm_error_hint!(
                        "Unlimited memory detected. Monitor system resources during development."
                    );
                }
                _ => {} // Normal memory allocation, no warning needed
            }
        }
    }

    #[must_use = "Docker daemon status should be checked"]
    pub(super) fn check_daemon_is_running(&self) -> Result<()> {
        crate::docker::DockerOps::check_daemon_running(Some(self.executable))
            .map_err(|_| VmError::Internal("Docker daemon is not running".to_string()))
    }

    /// Check Docker build requirements (disk space, resources)
    pub(super) fn check_docker_build_requirements(&self) {
        self.check_disk_space_unix();
        self.check_disk_space_windows();
    }

    /// Check disk space on Unix-like systems (Linux and macOS)
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn check_disk_space_unix(&self) {
        let available_gb = match self.get_available_disk_space_unix() {
            Some(gb) => gb,
            None => return, // Couldn't determine disk space, continue silently
        };

        if available_gb < 2 {
            vm_core::vm_warning!(
                "Low disk space: {}GB available. Docker builds may fail with insufficient storage.",
                available_gb
            );
        }
    }

    /// Get available disk space on Unix systems, returning GB as u32
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn get_available_disk_space_unix(&self) -> Option<u32> {
        let output = std::process::Command::new("df")
            .args(["-BG", "."])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let df_output = String::from_utf8(output.stdout).ok()?;
        let line = df_output.lines().nth(1)?;
        let available = line.split_whitespace().nth(3)?;
        available.trim_end_matches('G').parse::<u32>().ok()
    }

    /// Check disk space on Windows systems
    #[cfg(target_os = "windows")]
    fn check_disk_space_windows(&self) {
        let available_gb = match self.get_available_disk_space_windows() {
            Some(gb) => gb,
            None => return, // Couldn't determine disk space, continue silently
        };

        if available_gb < 2.0 {
            vm_core::vm_warning!("Low disk space: {:.1}GB available. Docker builds may fail with insufficient storage.", available_gb);
        }
    }

    /// Get available disk space on Windows systems, returning GB as f32
    #[cfg(target_os = "windows")]
    fn get_available_disk_space_windows(&self) -> Option<f32> {
        let output = std::process::Command::new("powershell")
            .args(["-Command", "(Get-PSDrive C).Free / 1GB"])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let space_str = String::from_utf8(output.stdout).ok()?;
        space_str.trim().parse::<f32>().ok()
    }

    /// No-op implementation for non-Unix, non-Windows systems
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    fn check_disk_space_unix(&self) {}

    /// No-op implementation for non-Windows systems
    #[cfg(not(target_os = "windows"))]
    fn check_disk_space_windows(&self) {}

    /// Handle potential Docker issues proactively
    pub(super) fn handle_potential_issues(&self) {
        // Check for port conflicts and provide helpful guidance
        if let Some(vm_config) = &self.config.vm {
            self.check_memory_allocation(vm_config);
        }

        // Check Docker daemon status more thoroughly
        if DockerCommand::new(Some(self.executable))
            .subcommand("ps")
            .execute()
            .is_err()
        {
            vm_error_with_details!(
                "Docker daemon may not be responding properly",
                &["Try: docker system prune -f", "Or: restart Docker Desktop"]
            );
        }
    }

    /// Resolve a partial container name to a full container name
    /// Supports matching by:
    /// - Exact container name
    /// - Project name (resolves to project-dev)
    /// - Partial container ID
    #[must_use = "container resolution results should be checked"]
    pub(super) fn resolve_container_name(&self, partial_name: &str) -> Result<String> {
        // Get list of all containers
        let output = std::process::Command::new(self.executable)
            .args(["ps", "-a", "--format", "{{.Names}}\t{{.ID}}"])
            .output()
            .map_err(|e| {
                VmError::Internal(format!(
                    "Failed to list containers for name resolution. Docker may not be running or accessible: {e}"
                ))
            })?;

        if !output.status.success() {
            return Err(VmError::Internal(
                "Docker container listing failed during name resolution. Check Docker daemon status".to_string()
            ));
        }

        let containers_output = String::from_utf8_lossy(&output.stdout);

        // First, try exact name match
        for line in containers_output.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                let name = parts[0];
                let id = parts[1];

                // Exact name match
                if name == partial_name {
                    return Ok(name.to_string());
                }

                // Exact ID match (full or partial)
                if id.starts_with(partial_name) {
                    return Ok(name.to_string());
                }
            }
        }

        // Second, try project name resolution (partial_name -> partial_name-dev)
        let candidate_name = format!("{partial_name}-dev");
        for line in containers_output.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if !parts.is_empty() {
                let name = parts[0];
                if name == candidate_name {
                    return Ok(name.to_string());
                }
            }
        }

        // Third, try fuzzy matching on container names
        let mut matches = Vec::new();
        for line in containers_output.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if !parts.is_empty() {
                let name = parts[0];
                if name.contains(partial_name) {
                    matches.push(name.to_string());
                }
            }
        }

        match matches.len() {
            0 => Err(VmError::Internal(format!(
                "No container found matching '{partial_name}'. Use 'vm list' to see available containers"
            ))),
            1 => Ok(matches[0].clone()),
            _ => {
                // Multiple matches - prefer exact project name match
                for name in &matches {
                    if name == &format!("{partial_name}-dev") {
                        return Ok(name.clone());
                    }
                }
                // Otherwise return first match but warn about ambiguity
                eprintln!(
                    "Warning: Multiple containers match '{}': {}",
                    partial_name,
                    matches.join(", ")
                );
                eprintln!("Using: {}", matches[0]);
                Ok(matches[0].clone())
            }
        }
    }
}
