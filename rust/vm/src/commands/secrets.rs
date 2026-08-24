//! Secrets command handlers
//!
//! This module provides command handlers for VM secrets management,
//! integrating with the vm-auth-proxy library to provide secure secret storage
//! and environment variable injection for VMs.

use crate::cli::SecretSubcommand;
use crate::error::{VmError, VmResult};
use crate::service_manager::get_service_manager;
use crate::service_registry::get_service_registry;
use dialoguer::{Confirm, Input, Password, Select};
use vm_auth_proxy::{self, check_server_running, SecretScope};
use vm_config::{AppConfig, GlobalConfig};
use vm_core::{vm_println, vm_progress, vm_success, vm_warning};

pub(super) async fn handle_command(
    command: &SecretSubcommand,
    config_path: Option<std::path::PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    let global_config = AppConfig::load(config_path, profile, None)
        .map(|config| config.global)
        .unwrap_or_default();
    handle_secrets_command(command, global_config).await
}

/// Handle secrets commands
async fn handle_secrets_command(
    command: &SecretSubcommand,
    global_config: GlobalConfig,
) -> VmResult<()> {
    match command {
        SecretSubcommand::Status => handle_status(&global_config).await,
        SecretSubcommand::Add {
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
        SecretSubcommand::Ls { show_values } => handle_list(*show_values, &global_config).await,
        SecretSubcommand::Rm { name, force } => handle_remove(name, *force, &global_config).await,
        SecretSubcommand::Interactive => handle_interactive(&global_config).await,
    }
}

fn server_url(global_config: &GlobalConfig) -> String {
    format!(
        "http://127.0.0.1:{}",
        global_config.services.auth_proxy.port
    )
}

async fn ensure_server(global_config: &GlobalConfig) -> VmResult<()> {
    let port = global_config.services.auth_proxy.port;
    if check_server_running(port).await {
        return Ok(());
    }
    get_service_manager()?
        .ensure_service_running("auth_proxy", global_config)
        .await
        .map_err(VmError::from)
}

/// Show secrets proxy status with service manager information
async fn handle_status(global_config: &GlobalConfig) -> VmResult<()> {
    let registry = get_service_registry();
    let service_manager_result = get_service_manager();

    vm_println!("Auth proxy status");

    // Get service status from service manager
    let service_status_opt = if let Ok(sm) = service_manager_result {
        sm.get_service_status("auth_proxy")
    } else {
        None
    };

    if let Some(service_state) = service_status_opt {
        vm_println!("  Reference count: {}", service_state.reference_count);
        vm_println!(
            "  Registered environments: {:?}",
            service_state.registered_vms
        );

        let status_line = registry.format_service_status(
            "auth_proxy",
            service_state.is_running,
            service_state.reference_count,
        );
        vm_println!("{}", status_line);
    } else {
        vm_println!("  Status: not managed");
    }

    // Check actual server status for verification
    let server_url = server_url(global_config);
    vm_println!("  Server: {server_url}");

    if check_server_running(global_config.services.auth_proxy.port).await {
        vm_println!("  Health: responding");
    } else {
        vm_println!("  Health: not responding");
    }

    vm_println!("  Lifecycle: managed automatically by environments");

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
    let server_url = server_url(global_config);
    ensure_server(global_config).await?;

    vm_progress!("Adding secret '{name}'...");

    vm_auth_proxy::add_secret(&server_url, name, value, scope, description)
        .await
        .map_err(VmError::from)?;

    vm_success!("Added secret '{name}'");
    Ok(())
}

/// List secrets
async fn handle_list(show_values: bool, global_config: &GlobalConfig) -> VmResult<()> {
    let server_url = server_url(global_config);
    ensure_server(global_config).await?;
    let list = vm_auth_proxy::list_secrets(&server_url)
        .await
        .map_err(VmError::from)?;

    if list.secrets.is_empty() {
        vm_println!("No secrets found.");
        return Ok(());
    }

    vm_println!("Secrets ({})", list.total);
    for secret in list.secrets {
        let scope = match secret.scope {
            SecretScope::Global => "global".to_string(),
            SecretScope::Project(project) => format!("project:{project}"),
            SecretScope::Instance(instance) => format!("instance:{instance}"),
        };
        let value = if show_values {
            match vm_auth_proxy::get_secret_value(&server_url, &secret.name).await {
                Ok(value) => format!(" = {}", masked_secret(&value)),
                Err(error) => {
                    vm_warning!("Could not read secret '{}': {}", secret.name, error);
                    " = <error>".to_string()
                }
            }
        } else {
            String::new()
        };
        let description = secret
            .description
            .map(|value| format!(" - {value}"))
            .unwrap_or_default();
        vm_println!("  {} [{}]{}{}", secret.name, scope, value, description);
    }

    Ok(())
}

fn masked_secret(value: &str) -> String {
    if value.chars().count() > 20 {
        format!("{}...", value.chars().take(17).collect::<String>())
    } else {
        value.to_string()
    }
}

/// Remove a secret
async fn handle_remove(name: &str, force: bool, global_config: &GlobalConfig) -> VmResult<()> {
    let server_url = server_url(global_config);
    ensure_server(global_config).await?;

    if !force
        && !Confirm::new()
            .with_prompt(format!("Remove secret '{name}'?"))
            .default(false)
            .interact()
            .map_err(|error| VmError::general(error, "Failed to confirm secret removal"))?
    {
        vm_println!("Secret removal cancelled.");
        return Ok(());
    }

    vm_progress!("Removing secret '{name}'...");

    vm_auth_proxy::remove_secret(&server_url, name)
        .await
        .map_err(VmError::from)?;

    vm_success!("Removed secret '{name}'");
    Ok(())
}

/// Interactive secret addition
async fn handle_interactive(global_config: &GlobalConfig) -> VmResult<()> {
    let server_url = server_url(global_config);
    ensure_server(global_config).await?;

    vm_println!("Add a secret");

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
            Some(format!("project:{project_name}"))
        }
        2 => {
            let instance_name: String =
                Input::new()
                    .with_prompt("Instance name")
                    .interact_text()
                    .map_err(|e| VmError::general(e, "Failed to read instance name"))?;
            Some(format!("instance:{instance_name}"))
        }
        _ => None,
    };

    // Get optional description
    let description: Option<String> = Input::new()
        .with_prompt("Description (optional)")
        .allow_empty(true)
        .interact_text()
        .ok()
        .filter(|s: &String| !s.is_empty());

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

    vm_success!("Added secret '{name}'");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::masked_secret;

    #[test]
    fn masking_is_unicode_safe() {
        assert_eq!(masked_secret("short"), "short");
        assert_eq!(
            masked_secret("🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐"),
            "🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐🔐..."
        );
    }
}
