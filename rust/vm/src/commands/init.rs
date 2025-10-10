// Standard library imports
use std::fs;
use std::path::PathBuf;

// External crate imports
use anyhow::{Context, Result};
use sysinfo::System;
use vm_config::{
    config::{MemoryLimit, ProjectConfig, VmConfig, VmSettings},
    detector::detect_project_name,
};
use vm_core::{vm_println, vm_success};

/// Represents the detected host system resources.
struct HostResources {
    total_cpus: usize,
    total_memory_mb: u64,
    recommended_cpus: u32,
    recommended_memory: MemoryLimit,
}

/// Detects the host's CPU and memory resources and suggests conservative defaults.
fn detect_host_resources() -> HostResources {
    let mut sys = System::new();
    sys.refresh_cpu();
    sys.refresh_memory();

    let total_cpus = sys.cpus().len();
    let total_memory_mb = sys.total_memory() / 1024 / 1024;

    // Use 50% of available CPUs, with a minimum of 1 and a maximum of 4.
    let recommended_cpus = (total_cpus as u32 / 2).clamp(1, 4);

    // Use 50% of available memory, with a minimum of 2GB and a maximum of 8GB.
    let recommended_memory_mb = (total_memory_mb as u32 / 2).clamp(2048, 8192);

    HostResources {
        total_cpus,
        total_memory_mb,
        recommended_cpus,
        recommended_memory: MemoryLimit::Limited(recommended_memory_mb),
    }
}

/// Handles the `vm init` command.
pub fn handle_init(
    file: Option<PathBuf>,
    _services: Option<String>,
    _ports: Option<u16>,
) -> Result<()> {
    let target_file = file.unwrap_or_else(|| PathBuf::from("vm.yaml"));
    if target_file.exists() {
        vm_println!("`vm.yaml` already exists. Skipping initialization.");
        return Ok(());
    }

    let resources = detect_host_resources();
    vm_println!(
        "✓ Detected {} CPU cores and {} GB RAM",
        resources.total_cpus,
        resources.total_memory_mb / 1024
    );

    let project_name =
        detect_project_name().unwrap_or_else(|_| "my-project".to_string());

    let config = VmConfig {
        provider: Some("docker".to_string()),
        project: Some(ProjectConfig {
            name: Some(project_name),
            ..Default::default()
        }),
        vm: Some(VmSettings {
            cpus: Some(resources.recommended_cpus),
            memory: Some(resources.recommended_memory.clone()),
            ..Default::default()
        }),
        ..Default::default()
    };

    // Note: The proposal mentions `--services` and `--ports` flags.
    // The logic to handle them will be added here once the basic generation is working.
    // For now, we focus on the resource detection part.

    let yaml_content =
        serde_yaml_ng::to_string(&config).context("Failed to serialize default config to YAML")?;

    fs::write(&target_file, yaml_content)
        .with_context(|| format!("Failed to write vm.yaml to {}", target_file.display()))?;

    vm_success!(
        "Generated `vm.yaml` with {} CPUs and {}MB memory.",
        resources.recommended_cpus,
        resources.recommended_memory.to_mb().unwrap_or(0)
    );

    Ok(())
}
