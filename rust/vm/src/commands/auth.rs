//! Auth proxy command handlers
//!
//! This module provides command handlers for the VM auth proxy functionality,
//! integrating with the vm-auth-proxy library to provide secure secret storage
//! and environment variable injection for VMs.

use crate::cli::AuthSubcommand;
use crate::error::{VmError, VmResult};
use crate::service_manager::get_service_manager;
use crate::service_registry::get_service_registry;
use vm_cli::msg;
use vm_config::GlobalConfig;
use vm_core::{vm_println, vm_success};
use vm_messages::messages::MESSAGES;

use vm_auth_proxy::{self, check_server_running, start_server_if_needed};

/// Handle auth proxy commands
pub async fn handle_auth_command(
    command: &AuthSubcommand,
    global_config: GlobalConfig,
) -> VmResult<()> {
    match command {
        AuthSubcommand::Status => handle_status(&global_config).await,
        AuthSubcommand::Add {
            name,
            value,
            scope,
            description,
        } => {
            handle_add(
                name,
                value,
                scope.as_deref(),
                description.as_deref(),
                &global_config,
            )
            .await
        }
        AuthSubcommand::List { show_values } => handle_list(*show_values, &global_config).await,
        AuthSubcommand::Remove { name, force } => handle_remove(name, *force, &global_config).await,
        AuthSubcommand::Interactive => handle_interactive(&global_config).await,
    }
}

/// Show auth proxy status with service manager information
async fn handle_status(global_config: &GlobalConfig) -> VmResult<()> {
    let registry = get_service_registry();
    let service_manager = get_service_manager();

    vm_println!("{}", MESSAGES.vm_auth_status_header);

    // Get service status from service manager
    if let Some(service_state) = service_manager.get_service_status("auth_proxy") {
        vm_println!(
            "{}",
            msg!(
                MESSAGES.vm_auth_reference_count,
                count = service_state.reference_count.to_string()
            )
        );
        vm_println!(
            "{}",
            msg!(
                MESSAGES.vm_auth_registered_vms,
                vms = format!("{:?}", service_state.registered_vms)
            )
        );

        let status_line = registry.format_service_status(
            "auth_proxy",
            service_state.is_running,
            service_state.reference_count,
        );
        vm_println!("{}", status_line);
    } else {
        vm_println!("{}", MESSAGES.vm_auth_not_managed);
    }

    // Check actual server status for verification
    let server_url = format!(
        "http://127.0.0.1:{}",
        global_config.services.auth_proxy.port
    );
    vm_println!("{}", msg!(MESSAGES.vm_auth_server_url, url = &server_url));

    if check_server_running(global_config.services.auth_proxy.port).await {
        vm_println!("{}", MESSAGES.vm_auth_health_ok);
    } else {
        vm_println!("{}", MESSAGES.vm_auth_health_failed);
    }

    vm_println!("{}", MESSAGES.vm_auth_auto_managed_info);

    Ok(())
}

/// Add a secret
async fn handle_add(
    name: &str,
    value: &str,
    scope: Option<&str>,
    description: Option<&str>,
    global_config: &GlobalConfig,
) -> VmResult<()> {
    let server_url = format!(
        "http://127.0.0.1:{}",
        global_config.services.auth_proxy.port
    );

    // Ensure server is running before attempting to add secret
    start_server_if_needed(global_config.services.auth_proxy.port)
        .await
        .map_err(VmError::from)?;

    vm_println!("{}", msg!(MESSAGES.vm_auth_adding_secret, name = name));

    vm_auth_proxy::add_secret(&server_url, name, value, scope, description)
        .await
        .map_err(VmError::from)?;

    vm_success!("{}", MESSAGES.vm_auth_secret_added);
    Ok(())
}

/// List secrets
async fn handle_list(show_values: bool, global_config: &GlobalConfig) -> VmResult<()> {
    let server_url = format!(
        "http://127.0.0.1:{}",
        global_config.services.auth_proxy.port
    );

    // Ensure server is running
    start_server_if_needed(global_config.services.auth_proxy.port)
        .await
        .map_err(VmError::from)?;

    vm_auth_proxy::list_secrets(&server_url, show_values)
        .await
        .map_err(VmError::from)?;

    Ok(())
}

/// Remove a secret
async fn handle_remove(name: &str, force: bool, global_config: &GlobalConfig) -> VmResult<()> {
    let server_url = format!(
        "http://127.0.0.1:{}",
        global_config.services.auth_proxy.port
    );

    // Ensure server is running
    start_server_if_needed(global_config.services.auth_proxy.port)
        .await
        .map_err(VmError::from)?;

    vm_println!("{}", msg!(MESSAGES.vm_auth_removing_secret, name = name));

    vm_auth_proxy::remove_secret(&server_url, name, force)
        .await
        .map_err(VmError::from)?;

    vm_success!("{}", MESSAGES.vm_auth_secret_removed);
    Ok(())
}

/// Interactive secret addition
async fn handle_interactive(global_config: &GlobalConfig) -> VmResult<()> {
    let server_url = format!(
        "http://127.0.0.1:{}",
        global_config.services.auth_proxy.port
    );

    // Ensure server is running before attempting interactive session
    start_server_if_needed(global_config.services.auth_proxy.port)
        .await
        .map_err(VmError::from)?;

    vm_println!("{}", MESSAGES.vm_auth_interactive_header);

    use dialoguer::{Input, Password, Select};

    // Get secret name
    let name: String = Input::new()
        .with_prompt("Secret name")
        .interact_text()
        .map_err(|e| VmError::general(e, "Failed to read secret name"))?;

    // Get secret value (hidden input)
    let value: String = Password::new()
        .with_prompt("Secret value")
        .interact()
        .map_err(|e| VmError::general(e, "Failed to read secret value"))?;

    // Get scope
    let scope_options = vec!["Global", "Project", "Instance"];
    let scope_selection = Select::new()
        .with_prompt("Secret scope")
        .items(&scope_options)
        .default(0)
        .interact()
        .map_err(|e| VmError::general(e, "Failed to read scope selection"))?;

    let scope = match scope_selection {
        0 => None, // Global
        1 => {
            let project_name: String = Input::new()
                .with_prompt("Project name")
                .interact_text()
                .map_err(|e| VmError::general(e, "Failed to read project name"))?;
            Some(format!("project:{}", project_name))
        }
        2 => {
            let instance_name: String =
                Input::new()
                    .with_prompt("Instance name")
                    .interact_text()
                    .map_err(|e| VmError::general(e, "Failed to read instance name"))?;
            Some(format!("instance:{}", instance_name))
        }
        _ => None,
    };

    // Get optional description
    let description: Option<String> = Input::new()
        .with_prompt("Description (optional)")
        .allow_empty(true)
        .interact_text()
        .ok()
        .and_then(|s: String| if s.is_empty() { None } else { Some(s) });

    // Add the secret
    vm_auth_proxy::add_secret(
        &server_url,
        &name,
        &value,
        scope.as_deref(),
        description.as_deref(),
    )
    .await
    .map_err(VmError::from)?;

    vm_success!(
        "{}",
        msg!(MESSAGES.vm_auth_interactive_success, name = &name)
    );
    Ok(())
}
