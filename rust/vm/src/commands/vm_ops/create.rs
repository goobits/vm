//! VM creation command handler
//!
//! This module handles VM creation with support for force recreation,
//! multi-instance providers, and service registration.

use tracing::{debug, info_span};

use crate::commands::base;
use crate::error::{VmError, VmResult};
use vm_config::{
    config::VmConfig,
    validation::{validate_config, ValidationMode},
    GlobalConfig,
};
use vm_core::{vm_hint, vm_println, vm_progress, vm_success, vm_warning};
use vm_provider::{Provider, ProviderContext};

use super::helpers::{has_enabled_services, print_vm_runtime_details, register_vm_services_helper};
use super::target::{
    canonical_instance_name, creation_instance_name, find_runtime_target, resolve_runtime_target,
};

/// Resolve an existing target or create it from the loaded `vm.yaml` configuration.
pub(crate) async fn resolve_or_create_target(
    provider: &dyn Provider,
    config: &VmConfig,
    global_config: &GlobalConfig,
    requested: Option<&str>,
) -> VmResult<String> {
    if let Some(target) = find_runtime_target(provider, config, requested)? {
        return Ok(target.name);
    }

    vm_progress!("No environment found; creating it from vm.yaml...");
    let project = config
        .project
        .as_ref()
        .and_then(|project| project.name.as_deref())
        .unwrap_or("vm-project");
    handle_create(
        provider.clone_box(),
        config.clone(),
        global_config.clone(),
        false,
        creation_instance_name(provider.name(), project, requested),
    )
    .await?;

    resolve_runtime_target(provider, config, requested)
}

/// Handle VM creation
pub async fn handle_create(
    provider: Box<dyn Provider>,
    config: VmConfig,
    global_config: GlobalConfig,
    force: bool,
    instance: Option<String>,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "create");
    let _enter = span.enter();
    debug!("Starting VM creation");

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
    let reusable_host_ports = if is_recreate {
        Vec::new()
    } else {
        provider.reusable_host_ports(&target_name)?
    };

    let mode = if is_recreate {
        ValidationMode::Recreate
    } else {
        ValidationMode::Create {
            reusable_host_ports: &reusable_host_ports,
        }
    };
    let validation = validate_config(&config, mode);
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
