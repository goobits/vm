//! VM creation command handler
//!
//! This module handles VM creation with support for force recreation,
//! multi-instance providers, and service registration.

use std::path::Path;
use tracing::{debug, info_span};

use crate::error::{VmError, VmResult};
use vm_cli::msg;
use vm_config::{config::MemoryLimit, config::VmConfig, validator::ConfigValidator, GlobalConfig};
use vm_core::{get_cpu_core_count, get_total_memory_gb, vm_error, vm_println};
use vm_messages::messages::MESSAGES;
use vm_provider::{Provider, ProviderContext};

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
    vm_println!("Validating configuration...");

    let vm_name = config
        .project
        .as_ref()
        .and_then(|project| project.name.as_deref())
        .unwrap_or("vm-project");
    let target_name = target_name(provider.name(), vm_name, instance.as_deref());
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
                vm_error!("Configuration validation failed:");
                vm_println!("{}", report);
                return Err(VmError::validation(
                    "Configuration is invalid, aborting creation.",
                    None::<String>,
                ));
            }
            if !report.warnings.is_empty() || !report.info.is_empty() {
                vm_println!("{}", report);
            }
            vm_println!("✓ Configuration is valid.");
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
            vm_println!(
                "⚠️  Container '{}' already exists{}.",
                target_name,
                if existing.status.to_lowercase().contains("running")
                    || existing.status.to_lowercase().contains("up")
                {
                    " and is running"
                } else {
                    ""
                }
            );
            vm_println!(
                "   Use 'vm ssh' to connect, 'vm start' to start, or 'vm create --force' to recreate."
            );
            return Ok(());
        }

        vm_println!(
            "{}",
            msg!(MESSAGES.vm.create_force_recreating, name = &target_name)
        );
        provider.destroy(
            instance.as_deref(),
            &ProviderContext::default().preserve_services(true),
        )?;
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
    }

    vm_println!("{}", msg!(MESSAGES.vm.create_header, name = vm_name));
    if matches!(provider.name(), "docker" | "podman") {
        vm_println!("{}", MESSAGES.vm.create_progress);
    }

    // Compose needs service settings before creation; VM providers register
    // services only after the guest exists.
    let register_services_before_create = matches!(provider.name(), "docker" | "podman");
    if register_services_before_create {
        vm_println!("{}", MESSAGES.common.configuring_services);
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

    match create_result {
        Ok(()) => {
            vm_println!("{}", MESSAGES.vm.create_success);

            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm.create_info_block,
                    status = MESSAGES.common.status_running,
                    container = &target_name
                )
            );

            print_vm_runtime_details(&config, true);

            if !register_services_before_create {
                vm_println!("{}", MESSAGES.common.configuring_services);
                register_vm_services_helper(&target_name, &config, &global_config).await?;
            }

            if is_first_vm {
                vm_println!("\n🎉 Success! Your VM is ready");
                vm_println!("📝 Next steps:");
                vm_println!("  • ssh into VM:  vm ssh");
                vm_println!("  • Run commands: vm exec -- npm install");
                vm_println!("  • View status:  vm status");
            } else {
                vm_println!("{}", MESSAGES.common.connect_hint);
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

fn target_name(provider: &str, project: &str, instance: Option<&str>) -> String {
    match (provider, instance) {
        ("tart", Some(instance)) => format!("{project}-{instance}"),
        ("tart", None) => project.to_string(),
        (_, Some(instance)) => format!("{project}-{instance}-dev"),
        (_, None) => format!("{project}-dev"),
    }
}

#[cfg(test)]
mod tests {
    use super::target_name;

    #[test]
    fn resolves_provider_specific_instance_container_names() {
        assert_eq!(
            target_name("docker", "sketch-api", Some("feature")),
            "sketch-api-feature-dev"
        );
        assert_eq!(
            target_name("tart", "sketch-api", Some("feature")),
            "sketch-api-feature"
        );
    }
}
