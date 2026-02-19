//! VM creation command handler
//!
//! This module handles VM creation with support for force recreation,
//! multi-instance providers, and service registration.

use std::path::Path;
use tracing::{debug, info_span, warn};

use crate::error::{VmError, VmResult};
use vm_cli::msg;
use vm_config::{config::MemoryLimit, config::VmConfig, validator::ConfigValidator, GlobalConfig};
use vm_core::{get_cpu_core_count, get_total_memory_gb, vm_error, vm_println};
use vm_messages::messages::MESSAGES;
use vm_provider::{docker::DockerOps, Provider, ProviderContext};

use super::helpers::{print_vm_runtime_details, register_vm_services_helper};

/// Auto-adjust resource allocation based on system availability
fn auto_adjust_resources(config: &mut VmConfig) -> VmResult<()> {
    // Get system resources (fallback to reasonable defaults if detection fails)
    let system_cpus = get_cpu_core_count().unwrap_or(2);
    let system_memory_gb = get_total_memory_gb().unwrap_or(4);

    let vm_settings = if let Some(settings) = config.vm.as_mut() {
        settings
    } else {
        return Ok(()); // No vm settings to adjust
    };
    let mut adjusted = false;

    // Check and adjust CPU allocation
    if let Some(cpu_limit) = &vm_settings.cpus {
        if let Some(requested_cpus) = cpu_limit.to_count() {
            if requested_cpus > system_cpus {
                // Use 50% of available CPUs, minimum 1, maximum available
                let safe_cpus = (system_cpus / 2).max(1).min(system_cpus);

                vm_println!(
                    "⚠️  Requested {} CPUs but system only has {}.",
                    requested_cpus,
                    system_cpus
                );
                vm_println!("   Auto-adjusting to {} CPUs for this system.", safe_cpus);

                vm_settings.cpus = Some(vm_config::config::CpuLimit::Limited(safe_cpus));
                adjusted = true;
            }
        }
        // If unlimited, no adjustment needed
    }

    // Check and adjust memory allocation
    if let Some(memory_limit) = &vm_settings.memory {
        if let Some(requested_mb) = memory_limit.to_mb() {
            let requested_gb = (requested_mb as u64) / 1024;

            // Leave 2GB for host OS, use up to 75% of remaining
            let max_safe_memory = system_memory_gb.saturating_sub(2);

            // Only adjust if request exceeds available memory (minus headroom)
            if requested_gb > max_safe_memory {
                let safe_memory_mb = (max_safe_memory * 1024) as u32;

                vm_println!(
                    "⚠️  Requested {}GB RAM but only {}GB total available.",
                    requested_gb,
                    system_memory_gb
                );
                vm_println!(
                    "   Auto-adjusting to {}GB RAM for this system (leaving 2GB for host).",
                    max_safe_memory
                );

                vm_settings.memory = Some(MemoryLimit::Limited(safe_memory_mb));
                adjusted = true;
            }
        }
    }

    if adjusted {
        vm_println!("");
        vm_println!("💡 Tip: These auto-adjusted values are temporary for this VM creation.");
        vm_println!("   Your vm.yaml remains unchanged and will work on more powerful machines.");
        vm_println!("");
    }

    Ok(())
}

/// Handle VM creation
#[allow(clippy::too_many_arguments)]
pub async fn handle_create(
    provider: Box<dyn Provider>,
    mut config: VmConfig,
    global_config: GlobalConfig,
    mut force: bool,
    instance: Option<String>,
    verbose: bool,
    save_as: Option<String>,
    from_dockerfile: Option<std::path::PathBuf>,
    preserve_services: bool,
    refresh_packages: bool,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "create");
    let _enter = span.enter();
    debug!("Starting VM creation");

    // Note: Config modifications for --from-dockerfile and --save-as are now handled
    // in commands/mod.rs before provider creation to avoid container name conflicts

    // Auto-enable force mode for snapshot builds to avoid prompts
    // Note: We still want full resources for snapshot builds, not minimal
    let is_snapshot_build = save_as.is_some() && from_dockerfile.is_some();
    if is_snapshot_build {
        debug!("Auto-enabling force mode for snapshot build from Dockerfile");
        force = true;
    }

    if force && !is_snapshot_build {
        // Regular force mode: use minimal resources and skip validation
        vm_println!("⚡ Force mode: using minimal resources and skipping validation");
        let mut vm_settings = config.vm.take().unwrap_or_default();
        vm_settings.memory = Some(vm_config::config::MemoryLimit::Limited(2048));
        vm_settings.cpus = Some(vm_config::config::CpuLimit::Limited(2));
        config.vm = Some(vm_settings);
    } else {
        // Auto-adjust resources if needed (before validation)
        auto_adjust_resources(&mut config)?;

        // Validate config before proceeding
        vm_println!("Validating configuration...");
        let validator = ConfigValidator::new();
        match validator.validate(&config) {
            Ok(report) => {
                if report.has_errors() {
                    vm_error!("Configuration validation failed:");
                    vm_println!("{}", report);
                    return Err(VmError::validation(
                        "Configuration is invalid, aborting creation.".to_string(),
                        None::<String>,
                    ));
                }
                if !report.warnings.is_empty() || !report.info.is_empty() {
                    vm_println!("{}", report);
                }
                vm_println!("✓ Configuration is valid.");
            }
            Err(e) => {
                return Err(VmError::validation(
                    format!("An unexpected error occurred during validation: {e}"),
                    None::<String>,
                ));
            }
        }
    }
    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    // Fast exit for Docker/Podman only if container already exists (unless --force)
    if !force && matches!(provider.name(), "docker" | "podman") {
        let existing_container = format!("{vm_name}-dev");
        if DockerOps::container_exists(None, &existing_container).unwrap_or(false) {
            let running =
                DockerOps::is_container_running(None, &existing_container).unwrap_or(false);
            vm_println!(
                "⚠️  Container '{}' already exists{}.",
                existing_container,
                if running { " and is running" } else { "" }
            );
            vm_println!(
                "   Use 'vm ssh' to connect, 'vm start' to start, or 'vm destroy' to recreate."
            );
            return Ok(());
        }
    }

    let is_first_vm = !Path::new(".vm").exists();
    if is_first_vm {
        vm_println!("👋 Creating your first VM for this project\n");
        vm_println!("💡 Tip: Edit vm.yaml to customize resources");
        vm_println!("⏱️  This may take 2-3 minutes...\n");
    }

    // Check if this is a multi-instance provider and handle accordingly
    if provider.supports_multi_instance() && instance.is_some() {
        let instance_name = match instance.as_deref() {
            Some(name) => name,
            None => {
                return Err(VmError::general(
                    std::io::Error::new(std::io::ErrorKind::NotFound, "Instance name not found"),
                    "Instance option was None, but was expected to be Some",
                ))
            }
        };
        vm_println!(
            "{}",
            msg!(
                MESSAGES.vm.create_header_instance,
                instance = instance_name,
                name = vm_name
            )
        );

        if force {
            debug!("Force flag set - will destroy existing instance if present");
            // Try to destroy specific instance first
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm.create_force_recreating_instance,
                    name = instance_name
                )
            );
            if let Err(e) = provider.destroy(Some(instance_name)) {
                warn!(
                    "Failed to destroy existing instance '{}' during force create: {}",
                    instance_name, e
                );
                // Continue with creation even if destroy fails
            }
        }
    } else {
        // Standard single-instance creation
        if let Some(instance_name) = &instance {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm.create_multiinstance_warning,
                    instance = instance_name,
                    provider = provider.name()
                )
            );
        }

        if force {
            debug!("Force flag set - attempting to destroy existing VM (if any)");
            warn!("Forcing recreation of VM '{}'", vm_name);
            vm_println!(
                "{}",
                msg!(MESSAGES.vm.create_force_recreating, name = vm_name)
            );

            if let Err(err) = provider.destroy(None) {
                debug!("Force destroy skipped or failed (container may not exist yet): {err}");
            }
        }
    }

    vm_println!("{}", msg!(MESSAGES.vm.create_header, name = vm_name));
    vm_println!("{}", MESSAGES.vm.create_progress);

    // Register VM services BEFORE creating container so docker-compose can inject env vars
    // Skip service registration for snapshot builds (they're just base images, no running services needed)
    let vm_instance_name = if let Some(instance_name) = &instance {
        format!("{vm_name}-{instance_name}")
    } else {
        format!("{vm_name}-dev")
    };

    if save_as.is_none() {
        vm_println!("{}", MESSAGES.common.configuring_services);
        register_vm_services_helper(&vm_instance_name, &config, &global_config).await?;
    } else {
        debug!("Skipping service registration for snapshot build");
    }

    // Create provider context with verbose flag and global config
    // Skip provisioning for snapshot builds (Dockerfile already has everything)
    let mut context = ProviderContext::with_verbose(verbose)
        .with_config(global_config.clone())
        .preserve_services(preserve_services)
        .refresh_packages(refresh_packages);
    if save_as.is_some() {
        context = context.skip_provisioning();
    }

    // Call the appropriate create method based on whether instance is specified
    let create_result = if let Some(instance_name) = &instance {
        if provider.supports_multi_instance() {
            provider.create_instance_with_context(instance_name, &context)
        } else {
            provider.create_with_context(&context)
        }
    } else {
        provider.create_with_context(&context)
    };

    match create_result {
        Ok(()) => {
            vm_println!("{}", MESSAGES.vm.create_success);

            let container_name = if let Some(instance_name) = &instance {
                format!("{vm_name}-{instance_name}")
            } else {
                format!("{vm_name}-dev")
            };
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm.create_info_block,
                    status = MESSAGES.common.status_running,
                    container = &container_name
                )
            );

            print_vm_runtime_details(&config, true);

            // Services were already registered before container creation
            if is_first_vm {
                vm_println!("\n🎉 Success! Your VM is ready");
                vm_println!("📝 Next steps:");
                vm_println!("  • ssh into VM:  vm ssh");
                vm_println!("  • Run commands: vm exec 'npm install'");
                vm_println!("  • View status:  vm status");
            } else {
                vm_println!("{}", MESSAGES.common.connect_hint);
            }

            // Handle --save-as flag (save container as global snapshot)
            if let Some(snapshot_name) = &save_as {
                use crate::commands::snapshot::{
                    manager::SnapshotManager,
                    metadata::{ServiceSnapshot, SnapshotMetadata},
                };

                let (is_global, clean_name) =
                    if let Some(stripped) = snapshot_name.strip_prefix('@') {
                        (true, stripped)
                    } else {
                        (false, snapshot_name.as_str())
                    };

                if !is_global {
                    vm_println!("\n⚠️  Warning: Snapshot name should start with @ for global snapshots (e.g., @vibe-base)");
                    vm_println!("   Saving as @{} instead...", clean_name);
                }

                vm_println!(
                    "\n📸 Saving container as global snapshot '@{}'...",
                    clean_name
                );

                // Create snapshot manager and directory
                let manager = SnapshotManager::new()?;
                let snapshot_dir = manager.get_snapshot_dir(Some("global"), clean_name);

                if snapshot_dir.exists() {
                    vm_println!(
                        "⚠️  Snapshot '@{}' already exists, overwriting...",
                        clean_name
                    );
                    std::fs::remove_dir_all(&snapshot_dir).map_err(|e| {
                        VmError::filesystem(e, snapshot_dir.display().to_string(), "remove_dir_all")
                    })?;
                }

                let images_dir = snapshot_dir.join("images");
                std::fs::create_dir_all(&images_dir).map_err(|e| {
                    VmError::filesystem(e, images_dir.display().to_string(), "create_dir_all")
                })?;

                // Commit container to image
                let image_tag = format!("vm-snapshot/global/{}:latest", clean_name);
                vm_println!("  Creating image from container...");

                // Clone container_name since it was moved earlier
                let container_name_clone = container_name.clone();
                let commit_output = tokio::process::Command::new("docker")
                    .args(["commit", &container_name_clone, &image_tag])
                    .output()
                    .await
                    .map_err(|e| VmError::general(e, "Failed to commit container"))?;

                if !commit_output.status.success() {
                    let stderr = String::from_utf8_lossy(&commit_output.stderr);
                    return Err(VmError::general(
                        std::io::Error::new(std::io::ErrorKind::Other, "Docker commit failed"),
                        format!("Failed to commit container: {}", stderr),
                    ));
                }

                // Save image to tar file
                vm_println!("  Saving image to snapshot directory...");
                let image_file = "base.tar";
                let image_path = images_dir.join(image_file);

                let image_path_str = image_path.to_str().ok_or_else(|| {
                    VmError::general(
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "Invalid UTF-8 in path",
                        ),
                        format!(
                            "Snapshot path contains invalid UTF-8 characters: {}",
                            image_path.display()
                        ),
                    )
                })?;

                let save_output = tokio::process::Command::new("docker")
                    .args(["save", &image_tag, "-o", image_path_str])
                    .output()
                    .await
                    .map_err(|e| VmError::general(e, "Failed to save image"))?;

                if !save_output.status.success() {
                    let stderr = String::from_utf8_lossy(&save_output.stderr);
                    return Err(VmError::general(
                        std::io::Error::new(std::io::ErrorKind::Other, "Docker save failed"),
                        format!("Failed to save image: {}", stderr),
                    ));
                }

                // Get image digest
                let digest_output = tokio::process::Command::new("docker")
                    .args(["inspect", "--format={{.Id}}", &image_tag])
                    .output()
                    .await
                    .map_err(|e| VmError::general(e, "Failed to inspect image"))?;

                let digest = if digest_output.status.success() {
                    Some(
                        String::from_utf8_lossy(&digest_output.stdout)
                            .trim()
                            .to_string(),
                    )
                } else {
                    None
                };

                // Calculate snapshot size
                let snapshot_size = calculate_directory_size(&snapshot_dir)?;

                // Create metadata
                let metadata = SnapshotMetadata {
                    name: clean_name.to_string(),
                    created_at: chrono::Utc::now(),
                    description: Some("Base image snapshot created from Dockerfile".to_string()),
                    project_name: "global".to_string(),
                    project_dir: std::env::current_dir()
                        .unwrap_or_else(|_| std::path::PathBuf::from("."))
                        .to_string_lossy()
                        .to_string(),
                    git_commit: None,
                    git_dirty: false,
                    git_branch: None,
                    services: vec![ServiceSnapshot {
                        name: "base".to_string(),
                        image_tag: image_tag.clone(),
                        image_file: image_file.to_string(),
                        image_digest: digest,
                    }],
                    volumes: vec![],
                    compose_file: "".to_string(),
                    vm_config_file: "".to_string(),
                    total_size_bytes: snapshot_size,
                };

                metadata.save(snapshot_dir.join("metadata.json"))?;

                vm_println!(
                    "  ✓ Snapshot saved ({:.2} MB)",
                    snapshot_size as f64 / (1024.0 * 1024.0)
                );
                vm_println!(
                    "\n🎉 Global snapshot '@{}' created successfully!",
                    clean_name
                );
                vm_println!("\nTo use this base image in other projects:");
                vm_println!("  1. Add to vm.yaml:");
                vm_println!("     vm:");
                vm_println!("       box: @{}", clean_name);
                vm_println!("  2. Run: vm create");
                vm_println!("\nTo export and share:");
                vm_println!("  vm snapshot export @{}", clean_name);

                // Clean up temporary build container
                vm_println!("\n  Cleaning up temporary build container...");
                let cleanup_container_name = if let Some(instance_name) = &instance {
                    format!("{vm_name}-{instance_name}")
                } else {
                    format!("{vm_name}-dev")
                };

                // Stop container
                let _ = tokio::process::Command::new("docker")
                    .args(["stop", &cleanup_container_name])
                    .output()
                    .await;

                // Remove container
                let _ = tokio::process::Command::new("docker")
                    .args(["rm", &cleanup_container_name])
                    .output()
                    .await;

                vm_println!("  ✓ Cleanup complete");
            }

            Ok(())
        }
        Err(e) => {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm.create_troubleshooting,
                    name = vm_name,
                    error = e.to_string()
                )
            );
            Err(VmError::from(e))
        }
    }?;

    // Seed database if configured
    if let Some(service_config) = config.services.get("postgresql") {
        if let Some(seed_file) = &service_config.seed_file {
            let default_db_name = format!("{}_dev", vm_name.replace('-', "_"));
            let db_name = service_config
                .database
                .as_deref()
                .unwrap_or(&default_db_name);
            vm_println!("🌱 Seeding database '{}' from {:?}...", db_name, seed_file);
            if let Err(e) = crate::commands::db::backup::import_db(db_name, seed_file).await {
                vm_println!("Database seeding failed: {}", e);
            }
        }
    }

    Ok(())
}

/// Calculate total size of directory recursively
fn calculate_directory_size(path: &std::path::Path) -> VmResult<u64> {
    let mut total = 0u64;

    if path.is_dir() {
        let entries = std::fs::read_dir(path)
            .map_err(|e| VmError::filesystem(e, path.to_string_lossy(), "read_dir"))?;

        for entry in entries {
            let entry = entry.map_err(|e| VmError::general(e, "Failed to read directory entry"))?;
            let path = entry.path();

            if path.is_dir() {
                total += calculate_directory_size(&path)?;
            } else {
                let metadata = std::fs::metadata(&path)
                    .map_err(|e| VmError::filesystem(e, path.to_string_lossy(), "metadata"))?;
                total += metadata.len();
            }
        }
    }

    Ok(total)
}
