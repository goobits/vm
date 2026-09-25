//! Secrets command handlers
//!
//! This module provides command handlers for VM secrets management,
//! integrating with the vm-auth-proxy library to provide secure secret storage
//! and environment variable injection for VMs.

use crate::cli::SecretSubcommand;
use crate::error::{VmError, VmResult};
use crate::services::service_lifecycle;
use dialoguer::Password;
use std::io::{IsTerminal, Read};
use vm_auth_proxy::{self, check_server_running, SecretScope};
use vm_config::{AppConfig, GlobalConfig};
use vm_core::{vm_print, vm_println, vm_progress, vm_success};

pub(super) async fn handle_command(
    command: &SecretSubcommand,
    config_path: Option<std::path::PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    if let SecretSubcommand::Set { stdin, file, .. } = command {
        require_secret_input_source(*stdin, file.as_deref())?;
    }
    let scope = match command {
        SecretSubcommand::Status => None,
        SecretSubcommand::Set { scope, .. }
        | SecretSubcommand::List { scope }
        | SecretSubcommand::Show { scope, .. }
        | SecretSubcommand::Remove { scope, .. } => {
            Some(resolve_scope(scope.as_deref(), config_path, profile)?)
        }
    };
    handle_secrets_command(command, GlobalConfig::load()?, scope.as_deref()).await
}

fn resolve_scope(
    requested: Option<&str>,
    config_path: Option<std::path::PathBuf>,
    profile: Option<String>,
) -> VmResult<String> {
    match requested.unwrap_or("project") {
        "user" => Ok("global".to_string()),
        "project" => {
            let app = AppConfig::load(config_path, profile, None)?;
            super::command_context::require_project_config(&app.vm)?;
            Ok(format!(
                "project:{}",
                super::command_context::project_name(&app.vm)
            ))
        }
        other => Err(VmError::validation(
            format!("Invalid secret scope '{other}'"),
            Some("Use project or user"),
        )),
    }
}

/// Handle secrets commands
async fn handle_secrets_command(
    command: &SecretSubcommand,
    global_config: GlobalConfig,
    scope: Option<&str>,
) -> VmResult<()> {
    match command {
        SecretSubcommand::Status => handle_status(&global_config).await,
        SecretSubcommand::Set {
            name,
            stdin,
            file,
            scope: _,
            description,
        } => {
            let value = read_secret_value(*stdin, file.as_deref())?;
            handle_add(name, &value, scope, description.as_deref(), &global_config).await
        }
        SecretSubcommand::List { .. } => {
            handle_list(scope.expect("scope resolved"), &global_config).await
        }
        SecretSubcommand::Show { name, .. } => {
            handle_show(name, scope.expect("scope resolved"), &global_config).await
        }
        SecretSubcommand::Remove { name, yes, .. } => {
            handle_remove(name, *yes, scope.expect("scope resolved"), &global_config).await
        }
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
    service_lifecycle()?
        .ensure_service_running("auth_proxy", global_config)
        .await
        .map_err(VmError::from)
}

/// Show secrets proxy status with lifecycle information.
async fn handle_status(global_config: &GlobalConfig) -> VmResult<()> {
    let lifecycle = service_lifecycle();

    vm_println!("Auth proxy status");

    let service_status_opt = if let Ok(lifecycle) = lifecycle {
        lifecycle.service_status("auth_proxy")
    } else {
        None
    };

    if let Some(service_state) = service_status_opt {
        vm_println!("  Reference count: {}", service_state.reference_count);
        vm_println!(
            "  Registered environments: {:?}",
            service_state.registered_vms
        );

        let status = if service_state.is_running {
            "running"
        } else {
            "stopped"
        };
        vm_println!(
            "  Status: {status} (port {})",
            global_config.services.auth_proxy.port
        );
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
async fn handle_list(scope: &str, global_config: &GlobalConfig) -> VmResult<()> {
    let server_url = server_url(global_config);
    ensure_server(global_config).await?;
    let list = vm_auth_proxy::list_secrets(&server_url, scope)
        .await
        .map_err(VmError::from)?;

    if list.secrets.is_empty() {
        vm_println!("No secrets found.");
        return Ok(());
    }

    vm_println!("Secrets ({})", list.total);
    let mut secrets = list.secrets;
    secrets.sort_by(|left, right| left.name.cmp(&right.name));
    for secret in secrets {
        let scope = match secret.scope {
            SecretScope::Global => "global".to_string(),
            SecretScope::Project(project) => format!("project:{project}"),
            SecretScope::Instance(instance) => format!("instance:{instance}"),
        };
        let description = secret
            .description
            .map(|value| format!(" - {value}"))
            .unwrap_or_default();
        vm_println!("  {} [{}]{}", secret.name, scope, description);
    }

    Ok(())
}

fn read_secret_value(stdin: bool, file: Option<&std::path::Path>) -> VmResult<String> {
    require_secret_input_source(stdin, file)?;
    let value = if let Some(path) = file {
        std::fs::read_to_string(path)
            .map_err(|error| VmError::general(error, "Failed to read secret file"))?
    } else if stdin {
        let mut value = String::new();
        std::io::stdin()
            .read_to_string(&mut value)
            .map_err(|error| VmError::general(error, "Failed to read secret from stdin"))?;
        value
    } else {
        Password::new()
            .with_prompt("Secret value")
            .interact()
            .map_err(|error| VmError::general(error, "Failed to read secret value"))?
    };
    if value.is_empty() {
        return Err(VmError::validation("Secret value is empty", None::<String>));
    }
    Ok(value)
}

fn require_secret_input_source(stdin: bool, file: Option<&std::path::Path>) -> VmResult<()> {
    if !stdin && file.is_none() && !std::io::stdin().is_terminal() {
        return Err(VmError::validation(
            "Secret input requires a terminal",
            Some("Use --stdin or --file in scripts"),
        ));
    }
    Ok(())
}

async fn handle_show(name: &str, scope: &str, global_config: &GlobalConfig) -> VmResult<()> {
    ensure_server(global_config).await?;
    let value = vm_auth_proxy::get_secret_value(&server_url(global_config), name, scope).await?;
    vm_print!("{value}");
    Ok(())
}

/// Remove a secret
async fn handle_remove(
    name: &str,
    yes: bool,
    scope: &str,
    global_config: &GlobalConfig,
) -> VmResult<()> {
    if !crate::confirmation::destructive(&format!("Remove secret '{name}' from {scope}?"), yes)? {
        vm_println!("Secret removal cancelled.");
        return Ok(());
    }

    let server_url = server_url(global_config);
    ensure_server(global_config).await?;
    vm_progress!("Removing secret '{name}'...");

    vm_auth_proxy::remove_secret(&server_url, name, scope)
        .await
        .map_err(VmError::from)?;

    vm_success!("Removed secret '{name}'");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::resolve_scope;

    #[test]
    fn user_scope_works_without_a_project_and_project_scope_requires_one() {
        let missing = Some(std::path::PathBuf::from("/nonexistent/project/vm.yaml"));
        assert_eq!(
            resolve_scope(Some("user"), missing.clone(), None).unwrap(),
            "global"
        );
        assert!(resolve_scope(None, missing, None).is_err());
    }
}
