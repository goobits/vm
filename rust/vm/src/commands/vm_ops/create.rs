//! VM creation command handler
//!
//! This module handles VM creation with support for force recreation,
//! multi-instance providers, and service registration.

use std::path::Path;
use tracing::{debug, info_span, warn};

use crate::error::{VmError, VmResult};
use vm_cli::msg;
use vm_config::{config::VmConfig, validator::ConfigValidator, GlobalConfig};
use vm_core::{vm_error, vm_println};
use vm_messages::messages::MESSAGES;
use vm_provider::{Provider, ProviderContext};

use super::helpers::register_vm_services_helper;

/// Handle VM creation
pub async fn handle_create(
    provider: Box<dyn Provider>,
    mut config: VmConfig,
    global_config: GlobalConfig,
    force: bool,
    instance: Option<String>,
    verbose: bool,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "create");
    let _enter = span.enter();
    debug!("Starting VM creation");

    if force {
        vm_println!("⚡ Force mode: using minimal resources and skipping validation");
        let mut vm_settings = config.vm.take().unwrap_or_default();
        vm_settings.memory = Some(vm_config::config::MemoryLimit::Limited(2048));
        vm_settings.cpus = Some(2);
        config.vm = Some(vm_settings);
    } else {
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
                    format!("An unexpected error occurred during validation: {}", e),
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

    let is_first_vm = !Path::new(".vm").exists();
    if is_first_vm {
        vm_println!("👋 Creating your first VM for this project\n");
        vm_println!("💡 Tip: Run 'vm init' first to customize resources");
        vm_println!("⏱️  This may take 2-3 minutes...\n");
    }

    // Check if this is a multi-instance provider and handle accordingly
    if provider.supports_multi_instance() && instance.is_some() {
        let instance_name = instance.as_deref().unwrap();
        vm_println!(
            "{}",
            msg!(
                MESSAGES.vm_create_header_instance,
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
                    MESSAGES.vm_create_force_recreating_instance,
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
                    MESSAGES.vm_create_multiinstance_warning,
                    instance = instance_name,
                    provider = provider.name()
                )
            );
        }

        if force {
            debug!("Force flag set - will destroy existing VM if present");
            // Check if VM exists and destroy it first
            if provider.status(None).is_ok() {
                warn!("VM exists, destroying due to --force flag");
                vm_println!(
                    "{}",
                    msg!(MESSAGES.vm_create_force_recreating, name = vm_name)
                );
                provider.destroy(None).map_err(VmError::from)?;
            }
        }
    }

    vm_println!("{}", msg!(MESSAGES.vm_create_header, name = vm_name));
    vm_println!("{}", MESSAGES.vm_create_progress);

    // Create provider context with verbose flag and global config
    let context = ProviderContext::with_verbose(verbose).with_config(global_config.clone());

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
            vm_println!("{}", MESSAGES.vm_create_success);

            let container_name = if let Some(instance_name) = &instance {
                format!("{}-{}", vm_name, instance_name)
            } else {
                format!("{}-dev", vm_name)
            };
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm_create_info_block,
                    status = MESSAGES.common_status_running,
                    container = container_name
                )
            );

            // Show resources if available
            if let Some(cpus) = config.vm.as_ref().and_then(|vm| vm.cpus) {
                if let Some(memory) = config.vm.as_ref().and_then(|vm| vm.memory.as_ref()) {
                    // Format memory display
                    let mem_str = match memory.to_mb() {
                        Some(mb) if mb >= 1024 => format!("{}GB", mb / 1024),
                        Some(mb) => format!("{}MB", mb),
                        None => format!("{:?}", memory),
                    };
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.common_resources_label,
                            cpus = cpus.to_string(),
                            memory = mem_str
                        )
                    );
                }
            }

            // Show services if any are configured
            let services: Vec<String> = config
                .services
                .iter()
                .filter(|(_, svc)| svc.enabled)
                .map(|(name, _)| name.clone())
                .collect();

            if !services.is_empty() {
                vm_println!(
                    "{}",
                    msg!(
                        MESSAGES.common_services_label,
                        services = services.join(", ")
                    )
                );
            }

            // Show port range
            if let Some(range) = &config.ports.range {
                if range.len() == 2 {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.common_ports_label,
                            start = range[0].to_string(),
                            end = range[1].to_string()
                        )
                    );
                }
            }

            // Register VM services (VM is already created and started by provider.create())
            let vm_instance_name = if let Some(instance_name) = &instance {
                format!("{}-{}", vm_name, instance_name)
            } else {
                format!("{}-dev", vm_name)
            };

            vm_println!("{}", MESSAGES.common_configuring_services);
            register_vm_services_helper(&vm_instance_name, &global_config).await?;

            if is_first_vm {
                vm_println!("\n🎉 Success! Your VM is ready");
                vm_println!("📝 Next steps:");
                vm_println!("  • ssh into VM:  vm ssh");
                vm_println!("  • Run commands: vm exec 'npm install'");
                vm_println!("  • View status:  vm status");
            } else {
                vm_println!("{}", MESSAGES.common_connect_hint);
            }
            Ok(())
        }
        Err(e) => {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm_create_troubleshooting,
                    name = vm_name,
                    error = e.to_string()
                )
            );
            Err(VmError::from(e))
        }
    }
}
