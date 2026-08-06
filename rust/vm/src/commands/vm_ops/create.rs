//! VM creation command handler
//!
//! This module handles VM creation with support for force recreation,
//! multi-instance providers, and service registration.

use tracing::{debug, info_span};

use crate::commands::base;
use crate::error::{VmError, VmResult};
use vm_config::{config::MemoryLimit, config::VmConfig, validator::ConfigValidator, GlobalConfig};
use vm_core::{
    get_cpu_core_count, get_total_memory_gb, vm_hint, vm_println, vm_progress, vm_success,
    vm_warning,
};
use vm_provider::{Provider, ProviderContext};

use super::helpers::{has_enabled_services, print_vm_runtime_details, register_vm_services_helper};
use super::target::canonical_instance_name;

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

                vm_warning!(
                    "Requested {requested_cpus} CPUs; using {safe_cpus} of {system_cpus} available"
                );

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

                vm_warning!(
                    "Requested {requested_gb}GB RAM; using {max_safe_memory}GB of {system_memory_gb}GB available"
                );

                vm_settings.memory = Some(MemoryLimit::Limited(safe_memory_mb));
                adjusted = true;
            }
        }
    }

    if adjusted {
        vm_hint!("Resource adjustments apply to this creation only; vm.yaml was not changed");
    }

    Ok(())
}

/// Handle VM creation
pub async fn handle_create(
    provider: Box<dyn Provider>,
    mut config: VmConfig,
    global_config: GlobalConfig,
    force: bool,
    instance: Option<String>,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "create");
    let _enter = span.enter();
    debug!("Starting VM creation");

    auto_adjust_resources(&mut config)?;
    vm_progress!("Validating configuration...");

    let vm_name = config
        .project
        .as_ref()
        .and_then(|project| project.name.as_deref())
        .unwrap_or("vm-project");
    let target_name = canonical_instance_name(provider.name(), vm_name, instance.as_deref());
    let existing_instance = provider
        .list_instances()?
        .into_iter()
        .find(|candidate| candidate.name == target_name);
    let is_recreate = force && existing_instance.is_some();

    // Fail before expensive provider work when an explicitly mapped port is
    // occupied by something other than the environment being force-recreated.
    if !is_recreate {
        let port_binding = config
            .vm
            .as_ref()
            .and_then(|settings| settings.port_binding.as_deref())
            .unwrap_or("0.0.0.0");
        for mapping in &config.ports.mappings {
            if std::net::TcpListener::bind((port_binding, mapping.host))
                .is_err_and(|error| error.kind() == std::io::ErrorKind::AddrInUse)
            {
                return Err(VmError::validation(
                    format!("Port {} is already in use on host", mapping.host),
                    Some("ports"),
                ));
            }
        }
    }

    let validator = ConfigValidator::new();
    let validation = if is_recreate {
        validator.validate_for_recreate(&config)
    } else {
        validator.validate(&config)
    };
    match validation {
        Ok(report) => {
            if report.has_errors() {
                return Err(VmError::validation(
                    format!("Configuration is invalid:\n{report}"),
                    None::<String>,
                ));
            }
            if !report.warnings.is_empty() || !report.info.is_empty() {
                vm_println!("{}", report);
            }
            vm_success!("Configuration is valid");
        }
        Err(error) => {
            return Err(VmError::validation(
                format!("Unexpected configuration validation error: {error}"),
                None::<String>,
            ));
        }
    }
    if let Some(existing) = existing_instance {
        if !force {
            vm_warning!(
                "Environment '{}' already exists{}",
                target_name,
                if existing.status.to_lowercase().contains("running")
                    || existing.status.to_lowercase().contains("up")
                {
                    " and is running"
                } else {
                    ""
                }
            );
            vm_hint!("Use `vm shell`, `vm start`, or remove it before `vm run`");
            return Ok(());
        }

        vm_progress!("Recreating '{target_name}'...");
        provider.destroy(
            instance.as_deref(),
            &ProviderContext::default().preserve_services(true),
        )?;
    }

    // Check if this is a multi-instance provider and handle accordingly
    if provider.supports_multi_instance() && instance.is_some() {
        debug!(instance = ?instance, project = vm_name, "Creating named instance");
    } else {
        // Standard single-instance creation
        if let Some(instance_name) = &instance {
            vm_warning!(
                "Provider '{}' does not support named instance '{}'; using its default",
                provider.name(),
                instance_name
            );
        }
    }

    base::ensure_configured_tart_base(&config)?;

    vm_progress!("Creating '{target_name}'...");

    // Compose needs service settings before creation; VM providers register
    // services only after the guest exists.
    let register_services_before_create = matches!(provider.name(), "docker" | "podman");
    let has_services = has_enabled_services(&config, &global_config);
    if register_services_before_create && has_services {
        vm_progress!("Configuring services...");
        register_vm_services_helper(&target_name, &config, &global_config).await?;
    }

    let context = ProviderContext::default()
        .with_config(global_config.clone())
        .preserve_services(true);

    // Call the appropriate create method based on whether instance is specified
    let create_result = if let Some(instance_name) = &instance {
        if provider.supports_multi_instance() {
            provider.create_instance(instance_name, &context)
        } else {
            provider.create(&context)
        }
    } else {
        provider.create(&context)
    };

    create_result.map_err(VmError::from)?;
    vm_success!("Created '{target_name}'");
    print_vm_runtime_details(&config, true);

    if !register_services_before_create && has_services {
        vm_progress!("Configuring services...");
        register_vm_services_helper(&target_name, &config, &global_config).await?;
    }

    vm_hint!("Connect with: vm shell {target_name}");

    // Seed database if configured
    if let Some(service_config) = config.services.get("postgresql") {
        if let Some(seed_file) = &service_config.seed_file {
            let default_db_name = format!("{}_dev", vm_name.replace('-', "_"));
            let db_name = service_config
                .database
                .as_deref()
                .unwrap_or(&default_db_name);
            vm_progress!(
                "Seeding database '{db_name}' from {}...",
                seed_file.display()
            );
            if let Err(e) = crate::commands::db::backup::import_db(db_name, seed_file).await {
                vm_warning!("Database seeding failed: {e}");
            }
        }
    }

    Ok(())
}
