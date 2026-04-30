use super::{
    host_sync::collect_host_sync_mounts, instance::TartInstanceManager,
    provisioner::TartProvisioner, temp::TartDirShare,
};
use crate::{
    common::instance::{extract_project_name, InstanceInfo, InstanceResolver},
    context::ProviderContext,
    progress::ProgressReporter,
    security::SecurityValidator,
    BoxConfig, Provider, ResourceUsage, ServiceStatus, TempProvider, VmError, VmStatusReport,
};
use duct::cmd;
use serde::Deserialize;
use std::ffi::OsStr;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};
use vm_cli::msg;
use vm_config::config::{BoxSpec, VmConfig};
use vm_core::command_stream::{
    is_tool_installed, stream_command, stream_command_visible_with_env, stream_command_with_env,
};
use vm_core::error::Result;
use vm_core::vm_println;
use vm_core::{get_cpu_core_count, get_total_memory_gb};
use vm_messages::messages::MESSAGES;

// Constants for Tart provider
const DEFAULT_TART_IMAGE: &str = "ghcr.io/cirruslabs/macos-sequoia-base:latest";
const DEFAULT_TART_VIBE_BASE: &str = "vibe-tart-sequoia-base";
const DEFAULT_TART_LINUX_VIBE_BASE: &str = "vibe-tart-linux-base";

struct CollectedMetrics {
    resources: ResourceUsage,
    services: Vec<ServiceStatus>,
    uptime: Option<String>,
}

pub(crate) fn sanitize_log_name(input: &str) -> String {
    input
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

pub(crate) fn tart_run_log_path(vm_name: &str) -> String {
    format!("/tmp/vm-tart-{}.log", sanitize_log_name(vm_name))
}

#[derive(Clone)]
pub struct TartProvider {
    pub(super) config: VmConfig,
}

impl TartProvider {
    pub(super) fn shell_escape_single_quotes(input: &str) -> String {
        input.replace('\'', "'\"'\"'")
    }

    pub fn new(config: VmConfig) -> Result<Self> {
        if !is_tool_installed("tart") {
            return Err(VmError::Dependency("Tart".into()));
        }
        Ok(Self { config })
    }

    fn tart_home(&self) -> Option<String> {
        self.config
            .tart
            .as_ref()
            .and_then(|tart| tart.storage_path.as_deref())
            .filter(|path| !path.trim().is_empty())
            .map(Self::expand_tart_home)
    }

    fn expand_tart_home(path: &str) -> String {
        if path == "~" {
            return std::env::var("HOME").unwrap_or_else(|_| path.to_string());
        }
        if let Some(rest) = path.strip_prefix("~/") {
            if let Ok(home) = std::env::var("HOME") {
                return format!("{home}/{rest}");
            }
        }
        path.to_string()
    }

    pub(super) fn tart_expr<A: AsRef<OsStr>>(&self, args: &[A]) -> duct::Expression {
        let mut expr = cmd("tart", args);
        if let Some(tart_home) = self.tart_home() {
            expr = expr.env("TART_HOME", tart_home);
        }
        expr
    }

    fn tart_command(&self) -> Command {
        let mut command = Command::new("tart");
        if let Some(tart_home) = self.tart_home() {
            command.env("TART_HOME", tart_home);
        }
        command
    }

    fn stream_tart_command<A: AsRef<OsStr>>(&self, args: &[A]) -> Result<()> {
        if let Some(tart_home) = self.tart_home() {
            stream_command_with_env("tart", args, &[("TART_HOME", tart_home.as_str())])
        } else {
            stream_command("tart", args)
        }
    }

    fn stream_tart_command_visible<A: AsRef<OsStr>>(&self, args: &[A]) -> Result<()> {
        if let Some(tart_home) = self.tart_home() {
            stream_command_visible_with_env("tart", args, &[("TART_HOME", tart_home.as_str())])
        } else {
            vm_core::command_stream::stream_command_visible("tart", args)
        }
    }

    pub(super) fn get_instance_state(&self, instance_name: &str) -> Result<Option<String>> {
        let output = self.tart_expr(&["list", "--format", "json"]).read()?;
        let vms: Vec<serde_json::Value> = serde_json::from_str(&output)?;
        for vm in vms {
            if vm["Name"] == instance_name {
                return Ok(vm["State"].as_str().map(|state| state.to_string()));
            }
        }
        Ok(None)
    }

    fn tart_image_exists(&self, image_name: &str) -> Result<bool> {
        let output = self.tart_expr(&["list", "--format", "json"]).read()?;
        let vms: Vec<serde_json::Value> = serde_json::from_str(&output)?;
        Ok(vms.iter().any(|vm| vm["Name"].as_str() == Some(image_name)))
    }

    pub(super) fn is_instance_running(&self, instance_name: &str) -> Result<bool> {
        Ok(matches!(
            self.get_instance_state(instance_name)?.as_deref(),
            Some("running")
        ))
    }

    fn is_guest_agent_ready(&self, instance_name: &str) -> bool {
        self.run_guest_agent_probe(instance_name, Duration::from_secs(3))
    }

    fn run_guest_agent_probe(&self, instance_name: &str, timeout: Duration) -> bool {
        let Ok(mut child) = self
            .tart_command()
            .args(["exec", instance_name, "echo", "ready"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return false;
        };

        let deadline = Instant::now() + timeout;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return status.success(),
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(100));
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return false;
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return false;
                }
            }
        }
    }

    pub(super) fn wait_for_guest_agent_ready(
        &self,
        instance_name: &str,
        timeout: Duration,
    ) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if self.run_guest_agent_probe(instance_name, Duration::from_secs(3)) {
                return true;
            }

            if Instant::now() >= deadline {
                return false;
            }

            thread::sleep(Duration::from_secs(1));
        }
    }

    fn ensure_workspace_mount_ready(&self, instance_name: &str, sync_dir: &str) -> Result<()> {
        TartProvisioner::new(
            instance_name.to_string(),
            sync_dir.to_string(),
            self.tart_home(),
        )
            .ensure_workspace_mount()
            .map_err(|e| {
                VmError::Provider(format!(
                    "Tart workspace mount is not ready at '{sync_dir}'. This VM may be partially provisioned or was started without the workspace share. Recreate it with `vm rm <name> --force && vm run mac as <name>`. Mount error: {e}"
                ))
            })
    }

    fn ensure_shell_config_ready(&self, instance_name: &str, sync_dir: &str) -> Result<()> {
        let provisioner = TartProvisioner::new(
            instance_name.to_string(),
            sync_dir.to_string(),
            self.tart_home(),
        );
        if !self.is_shell_config_ready(instance_name) {
            provisioner.apply_canonical_shell_config(&self.config)?;
            provisioner.apply_shell_overrides(&self.config)?;
        }

        provisioner.ensure_codex_runtime_config(&self.config)
    }

    fn is_shell_config_ready(&self, instance_name: &str) -> bool {
        self.tart_expr(&[
            "exec",
            instance_name,
            "sh",
            "-lc",
            "test -f \"$HOME/.zshrc\" && grep -Fq 'VM_PROJECT_PATH=' \"$HOME/.zshrc\" && grep -Fq 'VM_AI_ALIAS_REPAIR_VERSION=2' \"$HOME/.zshrc\"",
        ])
        .stderr_null()
        .stdout_null()
        .run()
        .is_ok()
    }

    fn collect_metrics(&self, instance: &str) -> Result<CollectedMetrics> {
        let metrics_script = include_str!("scripts/collect_metrics.sh");
        let output = self
            .tart_expr(&["exec", instance, "sh", "-c", metrics_script])
            .stderr_capture()
            .read()
            .map_err(|e| VmError::Provider(format!("SSH command failed: {}", e)))?;

        self.parse_metrics_json(&output)
    }

    fn host_workspace_path(&self) -> Result<PathBuf> {
        if let Some(source) = &self.config.source_path {
            let resolved = if source.is_absolute() {
                source.clone()
            } else {
                std::env::current_dir()
                    .map_err(|e| {
                        VmError::Internal(format!("Failed to determine host workspace path: {e}"))
                    })?
                    .join(source)
            };

            if resolved.is_dir() {
                return Self::normalize_host_workspace_path(&resolved);
            }

            if let Some(parent) = resolved.parent() {
                return Self::normalize_host_workspace_path(parent);
            }
        }

        let current_dir = std::env::current_dir().map_err(|e| {
            VmError::Internal(format!("Failed to determine host workspace path: {e}"))
        })?;
        Self::normalize_host_workspace_path(&current_dir)
    }

    fn normalize_host_workspace_path(path: &Path) -> Result<PathBuf> {
        let canonical_path = path.canonicalize().map_err(|e| {
            VmError::Internal(format!(
                "Failed to resolve host workspace path {}: {e}",
                path.display()
            ))
        })?;

        if Self::looks_like_project_root(&canonical_path) {
            return Ok(canonical_path);
        }

        let nested_workspace = canonical_path.join("workspace");
        if canonical_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "workspace")
            && nested_workspace.is_dir()
            && Self::looks_like_project_root(&nested_workspace)
        {
            return nested_workspace.canonicalize().map_err(|e| {
                VmError::Internal(format!(
                    "Failed to resolve nested host workspace path {}: {e}",
                    nested_workspace.display()
                ))
            });
        }

        Ok(canonical_path)
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

    fn effective_sync_directory(&self) -> String {
        let configured = self
            .config
            .project
            .as_ref()
            .and_then(|p| p.workspace_path.as_deref())
            .unwrap_or("/workspace");

        if configured == "/workspace" && Self::is_macos_guest_config(&self.config) {
            let user = self
                .config
                .tart
                .as_ref()
                .and_then(|tart| tart.ssh_user.as_deref())
                .unwrap_or("admin");
            return format!("/Users/{user}/workspace");
        }

        configured.to_string()
    }

    fn is_macos_guest_config(config: &VmConfig) -> bool {
        if matches!(config.os.as_deref(), Some("macos")) {
            return true;
        }

        if matches!(config.os.as_deref(), Some("linux")) {
            return false;
        }

        if matches!(
            config.tart.as_ref().and_then(|t| t.guest_os.as_deref()),
            Some("macos")
        ) {
            return true;
        }

        if matches!(
            config.tart.as_ref().and_then(|t| t.guest_os.as_deref()),
            Some("linux")
        ) {
            return false;
        }

        if let Some(BoxSpec::String(name)) = config.vm.as_ref().and_then(|vm| vm.get_box_spec()) {
            if name == DEFAULT_TART_VIBE_BASE || name.contains("macos") {
                return true;
            }
            if name == DEFAULT_TART_LINUX_VIBE_BASE
                || name.contains("ubuntu")
                || name.contains("debian")
                || name.contains("linux")
            {
                return false;
            }
        }

        match config.tart.as_ref().and_then(|tart| tart.image.as_deref()) {
            Some(image) => !image.contains("ubuntu") && !image.contains("linux"),
            None => true,
        }
    }

    fn start_vm_background(&self, vm_name: &str) -> Result<()> {
        self.start_vm_background_with_dir_shares(vm_name, &[])
    }

    pub(super) fn start_vm_background_with_dir_shares(
        &self,
        vm_name: &str,
        extra_dir_shares: &[TartDirShare],
    ) -> Result<()> {
        let host_path = self.host_workspace_path()?;
        let raw_log_path = tart_run_log_path(vm_name);
        info!("Tart run log for '{}': {}", vm_name, raw_log_path);
        let vm_name = Self::shell_escape_single_quotes(vm_name);
        let log_path = Self::shell_escape_single_quotes(&raw_log_path);
        let mut dir_args = vec![format!("{}:tag=workspace", host_path.display())];
        for mount in collect_host_sync_mounts(&self.config) {
            dir_args.push(format!("{}:tag={}", mount.host_path.display(), mount.tag));
        }
        for share in extra_dir_shares {
            dir_args.push(format!("{}:tag={}", share.host_path.display(), share.tag));
        }

        let escaped_dir_args: Vec<String> = dir_args
            .iter()
            .map(|arg| format!("--dir '{}'", Self::shell_escape_single_quotes(arg)))
            .collect();
        let cmd = format!(
            "nohup tart run --no-graphics {} '{}' >'{}' 2>&1 &",
            escaped_dir_args.join(" "),
            vm_name,
            log_path
        );

        let mut command = std::process::Command::new("sh");
        command.args(["-c", &cmd]);
        if let Some(tart_home) = self.tart_home() {
            command.env("TART_HOME", tart_home);
        }

        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                VmError::Provider(format!(
                    "Failed to start Tart VM: {}. See {}",
                    e, raw_log_path
                ))
            })?;

        Ok(())
    }

    fn parse_metrics_json(&self, raw: &str) -> Result<CollectedMetrics> {
        #[derive(Deserialize)]
        struct Payload {
            cpu_percent: Option<f64>,
            memory_used_mb: Option<u64>,
            memory_limit_mb: Option<u64>,
            disk_used_gb: Option<f64>,
            disk_total_gb: Option<f64>,
            uptime: Option<String>,
            services: Vec<ServiceEntry>,
        }

        #[derive(Deserialize)]
        struct ServiceEntry {
            name: String,
            is_running: bool,
        }

        let payload: Payload = serde_json::from_str(raw)
            .map_err(|e| VmError::Provider(format!("Failed to parse metrics JSON: {}", e)))?;

        let resources = ResourceUsage {
            cpu_percent: payload.cpu_percent,
            memory_used_mb: payload.memory_used_mb,
            memory_limit_mb: payload.memory_limit_mb,
            disk_used_gb: payload.disk_used_gb,
            disk_total_gb: payload.disk_total_gb,
        };

        let services = payload
            .services
            .into_iter()
            .map(|svc| ServiceStatus {
                name: svc.name,
                is_running: svc.is_running,
                port: None,
                host_port: None,
                metrics: None,
                error: None,
            })
            .collect();

        Ok(CollectedMetrics {
            resources,
            services,
            uptime: payload.uptime,
        })
    }

    fn apply_runtime_config(&self, instance: &str, config: &VmConfig) -> Result<()> {
        if let Some(cpus) = config.vm.as_ref().and_then(|v| v.cpus.as_ref()) {
            if let Some(count) = cpus.to_count() {
                let adjusted_count = Self::adjust_cpu_count(count);
                if adjusted_count != count {
                    warn!(
                        "Requested {} CPUs but only {} are available; applying {} CPUs",
                        count,
                        get_cpu_core_count().unwrap_or(2),
                        adjusted_count
                    );
                }
                info!("Setting CPU count to {}", adjusted_count);
                self.tart_expr(&["set", instance, "--cpu", &adjusted_count.to_string()])
                    .run()
                    .map_err(|e| VmError::Provider(format!("Failed to set CPU: {}", e)))?;
            }
        }

        if let Some(memory) = config.vm.as_ref().and_then(|v| v.memory.as_ref()) {
            if let Some(memory_mb) = memory.to_mb() {
                let adjusted_memory_mb = Self::adjust_memory_mb(memory_mb);
                if adjusted_memory_mb != memory_mb {
                    warn!(
                        "Requested {} MB RAM but only {} GB total memory is available; applying {} MB",
                        memory_mb,
                        get_total_memory_gb().unwrap_or(4),
                        adjusted_memory_mb
                    );
                }
                info!("Setting memory to {}MB", adjusted_memory_mb);
                self.tart_expr(&["set", instance, "--memory", &adjusted_memory_mb.to_string()])
                    .run()
                    .map_err(|e| VmError::Provider(format!("Failed to set memory: {}", e)))?;
            }
        }

        Ok(())
    }

    fn adjust_cpu_count(requested_cpus: u32) -> u32 {
        let system_cpus = get_cpu_core_count().unwrap_or(2);
        if requested_cpus > system_cpus {
            (system_cpus / 2).max(1).min(system_cpus)
        } else {
            requested_cpus
        }
    }

    fn adjust_memory_mb(requested_mb: u32) -> u32 {
        let system_memory_gb = get_total_memory_gb().unwrap_or(4);
        let requested_gb = (requested_mb as u64) / 1024;
        let max_safe_memory_gb = system_memory_gb.saturating_sub(2).max(1);

        if requested_gb > max_safe_memory_gb {
            (max_safe_memory_gb * 1024) as u32
        } else {
            requested_mb
        }
    }

    fn vm_name(&self) -> String {
        extract_project_name(&self.config).to_string()
    }

    /// Create instance manager for multi-instance operations
    fn instance_manager(&self) -> TartInstanceManager<'_> {
        TartInstanceManager::new(&self.config)
    }

    /// Resolve VM name with instance support
    fn vm_name_with_instance(&self, instance: Option<&str>) -> Result<String> {
        match instance {
            Some(name) if self.get_instance_state(name)?.is_some() => Ok(name.to_string()),
            Some(_) => {
                let manager = self.instance_manager();
                manager.resolve_instance_name(instance)
            }
            None => Ok(self.vm_name()), // Use existing default behavior for backward compatibility
        }
    }

    /// Get Tart OCI image with BoxConfig support
    fn get_tart_image(&self, config: &VmConfig) -> Result<String> {
        // Try new vm.box first
        if let Some(vm_settings) = &config.vm {
            if let Some(box_spec) = vm_settings.get_box_spec() {
                let box_config = BoxConfig::parse_for_tart(&box_spec)?;
                return match box_config {
                    BoxConfig::TartImage(image) => Ok(image),
                    BoxConfig::Snapshot(name) => Err(VmError::Config(format!(
                        "Use 'vm revert {}' for snapshots",
                        name
                    ))),
                    _ => Err(VmError::Internal("Invalid box type for Tart".into())),
                };
            }
        }

        // Fall back to deprecated tart.image
        if let Some(tart_config) = &config.tart {
            if let Some(image) = &tart_config.image {
                return Ok(image.clone());
            }
        }

        Ok(DEFAULT_TART_IMAGE.to_string())
    }

    /// Internal VM creation logic shared by create() and create_instance()
    fn create_vm_internal(
        &self,
        vm_name: &str,
        instance_label: Option<&str>,
        config: &VmConfig,
    ) -> Result<()> {
        self.create_vm_internal_with_dir_shares(vm_name, instance_label, config, &[])
    }

    pub(super) fn create_vm_internal_with_dir_shares(
        &self,
        vm_name: &str,
        instance_label: Option<&str>,
        config: &VmConfig,
        extra_dir_shares: &[TartDirShare],
    ) -> Result<()> {
        let progress = ProgressReporter::new();
        let phase_msg = match instance_label {
            Some(label) => format!("Creating Tart VM instance '{}'", label),
            None => "Creating Tart VM".to_string(),
        };
        let main_phase = progress.start_phase(&phase_msg);

        // Check if VM already exists
        ProgressReporter::task(&main_phase, "Checking if VM exists...");
        if self.get_instance_state(vm_name)?.is_some() {
            ProgressReporter::task(&main_phase, "Existing VM found.");
            vm_println!("⚠️  Tart VM '{}' already exists.", vm_name);
            vm_println!(
                "   Use 'vm shell <name>' to connect, 'vm run mac as <name>' to start, or 'vm rm <name>' to recreate."
            );
            ProgressReporter::finish_phase(&main_phase, "VM already exists.");
            return Ok(());
        }
        ProgressReporter::task(&main_phase, "VM not found, proceeding with creation.");

        // Check for orphaned VMs (same project, different instance/suffix)
        let existing_vms = self
            .instance_manager()
            .parse_tart_list()?
            .into_iter()
            .map(|instance| instance.name)
            .collect::<Vec<_>>();
        let project_prefix = format!("{}-", extract_project_name(&self.config));
        let orphans: Vec<String> = existing_vms
            .into_iter()
            .filter(|name| name.starts_with(&project_prefix) && name != vm_name)
            .collect();

        if !orphans.is_empty() {
            warn!("Found potential orphaned VMs from previous runs/instances");
            eprintln!("\n⚠️  Warning: Other VMs for this project detected");
            eprintln!("   These VMs might be from other instances or previous runs:\n");
            for orphan in &orphans {
                eprintln!("   • {}", orphan);
            }
            eprintln!("\n💡 If these are leftovers, you can clean them up with:");
            for orphan in &orphans {
                eprintln!("      tart delete {}", orphan);
            }
            eprintln!();
        }

        // Get image from config using new BoxConfig system
        let image = self.get_tart_image(config)?;

        if (image == DEFAULT_TART_VIBE_BASE || image == DEFAULT_TART_LINUX_VIBE_BASE)
            && !self.tart_image_exists(&image)?
        {
            return Err(VmError::Config(format!(
                "Tart vibe base '{}' was not found. Run `vm system base build vibe --provider tart` first.",
                image
            )));
        }

        // Clone the base image
        ProgressReporter::task(&main_phase, &format!("Cloning image '{}'...", image));
        let clone_result = self.stream_tart_command(&["clone", &image, vm_name]);
        if clone_result.is_err() {
            ProgressReporter::task(&main_phase, "Clone failed.");
            ProgressReporter::finish_phase(&main_phase, "Creation failed.");
            return clone_result;
        }
        ProgressReporter::task(&main_phase, "Image cloned successfully.");

        // Configure VM with memory/CPU settings if specified
        if let Some(vm_config) = &config.vm {
            if let Some(memory) = &vm_config.memory {
                match memory.to_mb() {
                    Some(mb) => {
                        ProgressReporter::task(
                            &main_phase,
                            &format!("Setting memory to {} MB...", mb),
                        );
                        self.stream_tart_command(&["set", vm_name, "--memory", &mb.to_string()])?;
                        ProgressReporter::task(&main_phase, "Memory configured.");
                    }
                    None => {
                        ProgressReporter::task(
                            &main_phase,
                            "Memory set to unlimited (no Tart limit).",
                        );
                    }
                }
            }

            if let Some(cpus) = &vm_config.cpus {
                match cpus.to_count() {
                    Some(count) => {
                        ProgressReporter::task(
                            &main_phase,
                            &format!("Setting CPUs to {}...", count),
                        );
                        self.stream_tart_command(&["set", vm_name, "--cpu", &count.to_string()])?;
                        ProgressReporter::task(&main_phase, "CPUs configured.");
                    }
                    None => {
                        ProgressReporter::task(
                            &main_phase,
                            "CPUs set to unlimited (no Tart limit).",
                        );
                    }
                }
            }
        }

        // Set disk size if specified
        if let Some(tart_config) = &config.tart {
            if let Some(disk_limit) = &tart_config.disk_size {
                if let Some(disk_gb) = disk_limit.to_gb() {
                    ProgressReporter::task(
                        &main_phase,
                        &format!("Setting disk size to {} GB...", disk_gb),
                    );
                    self.stream_tart_command(&[
                        "set",
                        vm_name,
                        "--disk-size",
                        &disk_gb.to_string(),
                    ])?;
                    ProgressReporter::task(&main_phase, "Disk size configured.");
                }
            }
        }

        // Start VM (non-blocking)
        ProgressReporter::task(&main_phase, "Starting VM...");
        let start_result = self.start_vm_background_with_dir_shares(vm_name, extra_dir_shares);
        if start_result.is_err() {
            ProgressReporter::task(&main_phase, "VM start failed.");
            ProgressReporter::finish_phase(&main_phase, "Creation failed.");
            return start_result;
        }
        ProgressReporter::task(&main_phase, "VM started successfully.");

        // Run initial provisioning using the effective config
        ProgressReporter::task(&main_phase, "Running initial provisioning...");
        let provisioner = TartProvisioner::new(
            vm_name.to_string(),
            self.get_sync_directory(),
            self.tart_home(),
        );
        if let Err(e) = provisioner.provision(config) {
            warn!(
                "Initial provisioning failed: {}. The VM is created but may not be fully configured.",
                e
            );
            // This is treated as a hard failure for create, as an un-provisioned VM is not useful.
            ProgressReporter::finish_phase(&main_phase, "Provisioning failed.");
            return Err(VmError::Provider(format!(
                "{}. Tart run log: {}",
                e,
                tart_run_log_path(vm_name)
            )));
        }
        ProgressReporter::task(&main_phase, "Initial provisioning complete.");

        if !extra_dir_shares.is_empty() {
            ProgressReporter::task(&main_phase, "Mounting temporary directories...");
            self.mount_tart_dir_shares_in_guest(vm_name, extra_dir_shares)?;
            ProgressReporter::task(&main_phase, "Temporary directories mounted.");
        }

        ProgressReporter::finish_phase(&main_phase, "Environment ready.");

        info!("{}", MESSAGES.service.provider_tart_created_success);
        info!("{}", MESSAGES.service.provider_tart_connect_hint);
        Ok(())
    }
}

impl Provider for TartProvider {
    fn name(&self) -> &'static str {
        "tart"
    }

    fn create(&self) -> Result<()> {
        self.create_vm_internal(&self.vm_name(), None, &self.config)
    }

    fn create_instance(&self, instance_name: &str) -> Result<()> {
        let vm_name = format!("{}-{}", self.vm_name(), instance_name);
        self.create_vm_internal(&vm_name, Some(instance_name), &self.config)
    }

    fn create_with_context(&self, context: &ProviderContext) -> Result<()> {
        // Apply global config defaults if present, but always use the project VmConfig
        let _ = context; // Global config is not directly applicable to VM creation
        self.create_vm_internal(&self.vm_name(), None, &self.config)
    }

    fn create_instance_with_context(
        &self,
        instance_name: &str,
        context: &ProviderContext,
    ) -> Result<()> {
        // Apply global config defaults if present, but always use the project VmConfig
        let _ = context; // Global config is not directly applicable to VM creation
        let vm_name = format!("{}-{}", self.vm_name(), instance_name);
        self.create_vm_internal(&vm_name, Some(instance_name), &self.config)
    }

    fn start(&self, container: Option<&str>) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;
        if self.is_instance_running(&vm_name).unwrap_or(false) {
            return Ok(());
        }

        self.start_vm_background(&vm_name)
    }

    fn stop(&self, container: Option<&str>) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;
        self.stream_tart_command(&["stop", &vm_name])
    }

    fn destroy(&self, container: Option<&str>) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;

        if self.is_instance_running(&vm_name).unwrap_or(false) {
            self.tart_expr(&["stop", &vm_name]).run().map_err(|e| {
                VmError::Provider(format!("Failed to stop Tart VM before delete: {e}"))
            })?;
        }

        self.stream_tart_command(&["delete", &vm_name])
    }

    fn ssh(&self, container: Option<&str>, relative_path: &Path) -> Result<()> {
        let instance_name = self.resolve_instance_name(container)?;
        let state = self.get_instance_state(&instance_name)?;
        match state.as_deref() {
            Some("running") => {}
            Some(_) => {
                return Err(VmError::Provider(format!(
                    "VM {instance_name} is not running"
                )));
            }
            None => {
                return Err(VmError::Provider(format!(
                    "No such object: Tart VM {instance_name}"
                )));
            }
        }

        if !self.is_guest_agent_ready(&instance_name) {
            if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
                vm_println!("⏳ Waiting for Tart guest agent...");
            }

            if !self.wait_for_guest_agent_ready(&instance_name, Duration::from_secs(45)) {
                return Err(VmError::Provider(format!(
                    "Tart VM '{instance_name}' is running, but the guest agent is not ready. Try again in a few seconds or run `tart restart {instance_name}`."
                )));
            }
        }

        let sync_dir = self.get_sync_directory();
        self.ensure_workspace_mount_ready(&instance_name, &sync_dir)?;
        self.ensure_shell_config_ready(&instance_name, &sync_dir)?;

        let shell = self
            .config
            .terminal
            .as_ref()
            .and_then(|t| t.shell.as_deref())
            .unwrap_or("zsh");

        // Get the sync directory (project root in VM)
        let target_path = SecurityValidator::validate_relative_path(relative_path, &sync_dir)?;
        let target_path = target_path.to_string_lossy().into_owned();

        info!("Opening SSH session in directory: {}", target_path);
        let target_path_escaped = Self::shell_escape_single_quotes(&target_path);

        if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
            let user = self
                .config
                .tart
                .as_ref()
                .and_then(|tart| tart.ssh_user.as_deref())
                .unwrap_or("admin");

            vm_println!(
                "{}",
                msg!(
                    MESSAGES.service.docker_ssh_info,
                    user = user,
                    path = target_path.as_str(),
                    shell = shell
                )
            );
        }

        // Use `tart exec -i -t` for interactive shell session
        let shell_escaped = Self::shell_escape_single_quotes(shell);
        let ssh_command = format!(
            "export VM_TARGET_DIR='{target_path}' && cd \"$VM_TARGET_DIR\" && exec '{shell}' -il",
            target_path = target_path_escaped,
            shell = shell_escaped
        );

        let status = self
            .tart_command()
            .args(["exec", "-i", "-t", &instance_name, "sh", "-c", &ssh_command])
            .status()
            .map_err(|e| VmError::Provider(format!("Exec failed: {e}")))?;

        match status.code() {
            Some(0) | Some(130) => Ok(()),
            Some(code) => Err(VmError::Provider(format!("Shell exited with code {code}"))),
            None => Err(VmError::Provider(
                "Shell terminated unexpectedly".to_string(),
            )),
        }?;

        Ok(())
    }

    fn exec(&self, container: Option<&str>, cmd: &[String]) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;
        let shell = self
            .config
            .terminal
            .as_ref()
            .and_then(|t| t.shell.as_deref())
            .unwrap_or("zsh");
        let sync_dir = self.get_sync_directory();
        self.ensure_workspace_mount_ready(&vm_name, &sync_dir)?;
        self.ensure_shell_config_ready(&vm_name, &sync_dir)?;
        let sync_dir_escaped = Self::shell_escape_single_quotes(&sync_dir);

        let mut args: Vec<String> = vec![
            "exec".to_string(),
            vm_name,
            shell.to_string(),
            "-ilc".to_string(),
            format!(
                "cd '{sync_dir}' && exec \"$@\"",
                sync_dir = sync_dir_escaped
            ),
            "vm-exec".to_string(),
        ];
        args.extend(cmd.iter().cloned());
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.stream_tart_command_visible(&arg_refs)
    }

    fn logs(&self, container: Option<&str>) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;
        // Try to read logs from Tart's configured home directory.
        let tart_home = self.tart_home().unwrap_or_else(|| {
            let home_env = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            format!("{home_env}/.tart")
        });
        let log_path = format!("{}/vms/{}/app.log", tart_home, vm_name);

        // Check if log file exists before attempting to tail
        if !Path::new(&log_path).exists() {
            let error_msg = format!("Log file not found at: {}", log_path);
            error!("{}", error_msg);
            info!("{}", MESSAGES.service.provider_logs_unavailable);
            info!(
                "{}",
                msg!(
                    MESSAGES.service.provider_logs_expected_location,
                    name = vm_name
                )
            );
            return Err(VmError::Internal(error_msg));
        }

        info!(
            "{}",
            msg!(MESSAGES.service.provider_logs_showing, path = &log_path)
        );
        info!("{}", MESSAGES.common.press_ctrl_c_to_stop);

        // Use tail -f to follow the log file
        stream_command("tail", &["-f", &log_path])
    }

    fn copy(&self, source: &str, destination: &str, container: Option<&str>) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;

        // Determine if we're copying to or from the VM
        let (local_path, remote_path, is_upload) = if source.contains(':') {
            // Downloading from VM
            let parts: Vec<&str> = source.splitn(2, ':').collect();
            if parts.len() == 2 {
                (destination, parts[1], false)
            } else {
                return Err(VmError::Provider("Invalid source format".to_string()));
            }
        } else if destination.contains(':') {
            // Uploading to VM
            let parts: Vec<&str> = destination.splitn(2, ':').collect();
            if parts.len() == 2 {
                (source, parts[1], true)
            } else {
                return Err(VmError::Provider("Invalid destination format".to_string()));
            }
        } else {
            // Neither has container prefix, assume uploading to VM
            (source, destination, true)
        };

        // Use scp-like approach via tart exec
        if is_upload {
            // Upload: local -> VM
            let copy_cmd = format!("cat > '{}'", remote_path.replace('\'', "'\"'\"'"));
            let output = cmd!(
                "sh",
                "-c",
                format!(
                    "cat '{}' | tart exec {} sh -c \"{}\"",
                    local_path.replace('\'', "'\"'\"'"),
                    vm_name,
                    copy_cmd
                )
            );
            let output = if let Some(tart_home) = self.tart_home() {
                output.env("TART_HOME", tart_home).run()
            } else {
                output.run()
            };

            output.map_err(|e| VmError::Provider(format!("Failed to copy file to VM: {}", e)))?;
        } else {
            // Download: VM -> local
            let copy_cmd = format!("cat '{}'", remote_path.replace('\'', "'\"'\"'"));
            let result = self
                .tart_expr(&["exec", &vm_name, "sh", "-c", &copy_cmd])
                .stdout_capture()
                .run()
                .map_err(|e| VmError::Provider(format!("Failed to read file from VM: {}", e)))?;

            std::fs::write(local_path, result.stdout)
                .map_err(|e| VmError::Provider(format!("Failed to write local file: {}", e)))?;
        }

        Ok(())
    }

    fn status(&self, container: Option<&str>) -> Result<()> {
        match container {
            Some(_) => {
                // Show specific VM status
                let vm_name = self.vm_name_with_instance(container)?;
                let output = self.tart_command().args(["list"]).output()?;

                if !output.status.success() {
                    return Err(VmError::Internal(
                        "Failed to get Tart VM status. Check that Tart is properly installed"
                            .to_string(),
                    ));
                }

                let list_output = String::from_utf8_lossy(&output.stdout);
                for line in list_output.lines() {
                    if line.contains(&vm_name) {
                        info!("{}", line);
                        return Ok(());
                    }
                }
                info!(
                    "{}",
                    msg!(MESSAGES.service.provider_vm_not_found, name = vm_name)
                );
                Ok(())
            }
            None => {
                // Show all VMs (existing behavior)
                self.stream_tart_command(&["list"])
            }
        }
    }

    fn get_status_report(&self, container: Option<&str>) -> Result<VmStatusReport> {
        let instance_name = self.resolve_instance_name(container)?;

        let Some(state) = self.get_instance_state(&instance_name)? else {
            return Err(VmError::Internal(format!(
                "Tart VM '{}' not found",
                instance_name
            )));
        };

        if state != "running" || !self.is_guest_agent_ready(&instance_name) {
            return Ok(VmStatusReport {
                name: instance_name.clone(),
                provider: "tart".into(),
                is_running: false,
                ..Default::default()
            });
        }

        let metrics = self.collect_metrics(&instance_name)?;

        Ok(VmStatusReport {
            name: instance_name,
            provider: "tart".into(),
            container_id: None,
            is_running: true,
            uptime: metrics.uptime,
            resources: metrics.resources,
            services: metrics.services,
        })
    }

    fn start_with_context(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
        let instance_name = self.resolve_instance_name(container)?;

        // Apply runtime configuration from project config
        if context.global_config.is_some() {
            info!("Applying config updates to Tart VM");
            self.apply_runtime_config(&instance_name, &self.config)?;
        }

        self.start(Some(&instance_name))
    }

    fn restart_with_context(
        &self,
        container: Option<&str>,
        context: &ProviderContext,
    ) -> Result<()> {
        let instance_name = self.resolve_instance_name(container)?;

        // Apply runtime configuration from project config
        if context.global_config.is_some() {
            info!("Applying config updates to Tart VM");
            self.apply_runtime_config(&instance_name, &self.config)?;
        }

        self.restart(Some(&instance_name))
    }

    fn restart(&self, container: Option<&str>) -> Result<()> {
        // Stop then start the VM
        self.stop(container)?;
        self.start(container)
    }

    fn provision(&self, container: Option<&str>) -> Result<()> {
        let instance_name = self.resolve_instance_name(container)?;

        let provisioner = TartProvisioner::new(
            instance_name.clone(),
            self.get_sync_directory(),
            self.tart_home(),
        );

        provisioner.provision(&self.config)?;

        info!("{}", MESSAGES.vm.apply_success);
        Ok(())
    }

    fn list(&self) -> Result<()> {
        // List all Tart VMs
        self.stream_tart_command(&["list"])
    }

    fn kill(&self, container: Option<&str>) -> Result<()> {
        let instance_name = self.resolve_instance_name(container)?;
        warn!("Force killing Tart VM: {}", &instance_name);

        self.tart_expr(&["stop", &instance_name, "--timeout", "0"])
            .run()
            .map_err(|e| VmError::Provider(format!("Failed to force stop VM: {}", e)))?;

        info!("Tart VM force-stopped successfully via CLI");
        Ok(())
    }

    fn as_temp_provider(&self) -> Option<&dyn TempProvider> {
        Some(self)
    }

    fn get_sync_directory(&self) -> String {
        self.effective_sync_directory()
    }

    fn supports_multi_instance(&self) -> bool {
        true
    }

    fn resolve_instance_name(&self, instance: Option<&str>) -> Result<String> {
        if let Some(name) = instance {
            if self.get_instance_state(name)?.is_some() {
                return Ok(name.to_string());
            }
        }

        let manager = self.instance_manager();
        manager.resolve_instance_name(instance)
    }

    fn list_instances(&self) -> Result<Vec<InstanceInfo>> {
        let manager = self.instance_manager();
        manager.list_instances()
    }

    fn clone_box(&self) -> Box<dyn Provider> {
        Box::new(self.clone())
    }
}

#[cfg(any(test, feature = "test-helpers"))]
impl TartProvider {
    /// Test-only helper method to execute commands in a specific path
    pub fn exec_in_path(
        &self,
        container: Option<&str>,
        path: &std::path::Path,
        cmd_parts: &[&str],
    ) -> Result<String> {
        let instance_name = self.resolve_instance_name(container)?;
        let command_str = cmd_parts.join(" ");
        let ssh_command = format!("cd '{}' && {}", path.display(), command_str);

        let output = self
            .tart_expr(&["exec", &instance_name, "sh", "-c", &ssh_command])
            .read()
            .map_err(|e| VmError::Provider(format!("Exec in path failed: {}", e)))?;

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::TartProvider;
    use crate::Provider;
    use vm_config::config::{ProjectConfig, TartConfig, VmConfig};

    #[test]
    fn host_workspace_path_uses_loaded_config_parent() {
        let outer = tempfile::tempdir().unwrap();
        let project_dir = outer.path().join("workspace");
        std::fs::create_dir_all(&project_dir).unwrap();
        let config_path = project_dir.join("vm.yaml");
        std::fs::write(&config_path, "provider: tart\n").unwrap();

        let provider = TartProvider {
            config: VmConfig {
                source_path: Some(config_path),
                ..Default::default()
            },
        };

        let resolved = provider.host_workspace_path().unwrap();
        assert_eq!(resolved, project_dir.canonicalize().unwrap());
    }

    #[test]
    fn host_workspace_path_skips_outer_workspace_wrapper() {
        let temp_dir = tempfile::tempdir().unwrap();
        let outer_workspace = temp_dir.path().join("workspace");
        let inner_workspace = outer_workspace.join("workspace");
        std::fs::create_dir_all(&inner_workspace).unwrap();
        std::fs::write(inner_workspace.join("vm.yaml"), "provider: tart\n").unwrap();

        let resolved = TartProvider::normalize_host_workspace_path(&outer_workspace).unwrap();
        assert_eq!(resolved, inner_workspace.canonicalize().unwrap());
    }

    #[test]
    fn host_workspace_path_keeps_real_project_named_workspace() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace = temp_dir.path().join("workspace");
        std::fs::create_dir_all(workspace.join("workspace")).unwrap();
        std::fs::write(workspace.join("vm.yaml"), "provider: tart\n").unwrap();

        let resolved = TartProvider::normalize_host_workspace_path(&workspace).unwrap();
        assert_eq!(resolved, workspace.canonicalize().unwrap());
    }

    #[test]
    fn macos_guest_uses_writable_default_workspace() {
        let provider = TartProvider {
            config: VmConfig {
                project: Some(ProjectConfig {
                    workspace_path: Some("/workspace".to_string()),
                    ..Default::default()
                }),
                tart: Some(TartConfig {
                    guest_os: Some("macos".to_string()),
                    ssh_user: Some("admin".to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        };

        assert_eq!(provider.get_sync_directory(), "/Users/admin/workspace");
    }

    #[test]
    fn linux_guest_keeps_default_workspace() {
        let provider = TartProvider {
            config: VmConfig {
                project: Some(ProjectConfig {
                    workspace_path: Some("/workspace".to_string()),
                    ..Default::default()
                }),
                tart: Some(TartConfig {
                    guest_os: Some("linux".to_string()),
                    ssh_user: Some("admin".to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        };

        assert_eq!(provider.get_sync_directory(), "/workspace");
    }

    #[test]
    fn macos_guest_respects_custom_workspace() {
        let provider = TartProvider {
            config: VmConfig {
                project: Some(ProjectConfig {
                    workspace_path: Some("/Volumes/work/project".to_string()),
                    ..Default::default()
                }),
                tart: Some(TartConfig {
                    guest_os: Some("macos".to_string()),
                    ssh_user: Some("admin".to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        };

        assert_eq!(provider.get_sync_directory(), "/Volumes/work/project");
    }
}
