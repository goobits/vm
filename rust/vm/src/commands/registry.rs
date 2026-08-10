//! Private package registry command handlers
//!
//! This module provides command handlers for the VM private registry functionality,
//! integrating with the vm-package-server library to provide npm, pip, and cargo
//! package caching and serving capabilities.

use crate::cli::{RegistryConfigAction, RegistrySubcommand};
use crate::error::{VmError, VmResult};
use crate::service_manager::get_service_manager;
use crate::service_registry::get_service_registry;
use crate::utils::confirm_select;
use anyhow::Context;
use vm_config::{ConfigOps, GlobalConfig};
use vm_core::{vm_hint, vm_println, vm_progress, vm_success, vm_warning};

use vm_package_server;

/// Handle registry commands
pub async fn handle_registry_command(
    command: &RegistrySubcommand,
    global_config: GlobalConfig,
) -> VmResult<()> {
    match command {
        RegistrySubcommand::Status { yes } => handle_status(*yes, &global_config).await,
        RegistrySubcommand::Ls { yes } => handle_list(*yes, &global_config).await,
        RegistrySubcommand::Config { action } => handle_config(action, &global_config).await,
        RegistrySubcommand::Use { shell, port } => {
            handle_use(shell.as_deref(), *port, &global_config).await
        }
        RegistrySubcommand::Serve { host, port, data } => {
            handle_serve(host, *port, data, &global_config).await
        }
    }
}

/// Show package registry status with service manager information
async fn handle_status(yes: bool, global_config: &GlobalConfig) -> VmResult<()> {
    let server_url = format!(
        "http://localhost:{}",
        global_config.services.package_registry.port
    );

    // Ensure server is running for complete status information
    start_server_if_needed(global_config, yes).await?;

    let registry = get_service_registry();
    let service_manager_result = get_service_manager();

    vm_println!("Package registry status");

    // Get service status from service manager
    let service_status_opt = if let Ok(sm) = service_manager_result {
        sm.get_service_status("package_registry")
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
            "package_registry",
            service_state.is_running,
            service_state.reference_count,
        );
        vm_println!("{}", status_line);
    } else {
        vm_println!("  Status: not managed");
    }

    // Check actual server status for verification
    if check_server_running_with_url(&server_url).await {
        vm_println!("  Health: responding");
    } else {
        vm_println!("  Health: not responding");
    }

    vm_println!("  Lifecycle: managed automatically by environments");

    let status = package_client(global_config)?
        .status()
        .await
        .map_err(VmError::from)?;
    vm_println!("  Version: {}", status.version);
    vm_println!("  Registries: {}", status.registries.join(", "));
    Ok(())
}

/// List packages in registry
async fn handle_list(yes: bool, global_config: &GlobalConfig) -> VmResult<()> {
    // Ensure server is running for complete package listing
    start_server_if_needed(global_config, yes).await?;

    let packages = package_client(global_config)?
        .packages()
        .await
        .map_err(VmError::from)?;
    for (ecosystem, names) in packages {
        vm_println!("{ecosystem} ({}):", names.len());
        for name in names {
            vm_println!("  {name}");
        }
    }

    Ok(())
}

/// Handle configuration commands
async fn handle_config(
    action: &RegistryConfigAction,
    global_config: &GlobalConfig,
) -> VmResult<()> {
    let port = global_config.services.package_registry.port;
    match action {
        RegistryConfigAction::Show => {
            vm_println!("Package registry configuration:");
            vm_println!(
                "  enabled: {}",
                global_config.services.package_registry.enabled
            );
            vm_println!("  port: {port}");
            vm_println!(
                "  max_storage_gb: {}",
                global_config.services.package_registry.max_storage_gb
            );
            Ok(())
        }
        RegistryConfigAction::Get { key } => {
            match key.as_str() {
                "port" => vm_println!("{}", port),
                "enabled" => {
                    vm_println!("{}", global_config.services.package_registry.enabled)
                }
                "max_storage_gb" => {
                    vm_println!("{}", global_config.services.package_registry.max_storage_gb)
                }
                _ => {
                    return Err(VmError::validation(
                        format!("Unknown package registry key '{key}'"),
                        Some("Use: enabled, port, or max_storage_gb"),
                    ))
                }
            }
            Ok(())
        }
        RegistryConfigAction::Set { key, value } => {
            if !matches!(key.as_str(), "enabled" | "port" | "max_storage_gb") {
                return Err(VmError::validation(
                    format!("Unknown package registry key '{key}'"),
                    Some("Use: enabled, port, or max_storage_gb"),
                ));
            }
            ConfigOps::set(
                &format!("services.package_registry.{key}"),
                std::slice::from_ref(value),
                true,
                false,
            )
            .map_err(VmError::from)
        }
    }
}

/// Generate shell configuration
async fn handle_use(shell: Option<&str>, port: u16, global_config: &GlobalConfig) -> VmResult<()> {
    let shell_type = shell.unwrap_or("bash");

    // Use provided port if non-zero, otherwise use global config port
    let actual_port = if port != 0 {
        port
    } else {
        global_config.services.package_registry.port
    };

    match shell_type {
        "bash" | "zsh" => {
            vm_println!(
                "# Package registry configuration for {shell_type}\nexport NPM_CONFIG_REGISTRY=http://localhost:{actual_port}/npm/\nexport PIP_INDEX_URL=http://localhost:{actual_port}/pypi/simple/\nexport PIP_TRUSTED_HOST=localhost"
            );
        }
        "fish" => {
            vm_println!(
                "# Package registry configuration for fish\nset -x NPM_CONFIG_REGISTRY http://localhost:{actual_port}/npm/\nset -x PIP_INDEX_URL http://localhost:{actual_port}/pypi/simple/\nset -x PIP_TRUSTED_HOST localhost"
            );
        }
        _ => {
            return Err(VmError::validation(
                format!("Unsupported shell '{shell_type}'"),
                Some("Use bash, zsh, or fish"),
            ));
        }
    }

    Ok(())
}

/// Check if the package registry server is running
async fn check_server_running(global_config: &GlobalConfig) -> bool {
    match package_client(global_config) {
        Ok(client) => client.is_healthy().await,
        Err(_) => false,
    }
}

/// Check if the package registry server is running at a specific URL
async fn check_server_running_with_url(base_url: &str) -> bool {
    match vm_packages::RegistryEndpoints::new(base_url) {
        Ok(endpoints) => {
            vm_packages::PackageInfrastructureClient::new(endpoints)
                .is_healthy()
                .await
        }
        Err(_) => false,
    }
}

/// Get the version of the running server
async fn get_server_version(base_url: &str) -> VmResult<String> {
    let endpoints = vm_packages::RegistryEndpoints::new(base_url).map_err(VmError::from)?;
    vm_packages::PackageInfrastructureClient::new(endpoints)
        .status()
        .await
        .map(|status| status.version)
        .map_err(VmError::from)
}

fn package_client(
    global_config: &GlobalConfig,
) -> VmResult<vm_packages::PackageInfrastructureClient> {
    let endpoints = vm_packages::RegistryEndpoints::new(format!(
        "http://localhost:{}",
        global_config.services.package_registry.port
    ))
    .map_err(VmError::from)?;
    Ok(vm_packages::PackageInfrastructureClient::new(endpoints))
}

/// Gracefully shutdown the server
async fn shutdown_server(base_url: &str) -> VmResult<()> {
    let shutdown_url = format!("{base_url}/shutdown");
    let client = reqwest::Client::new();
    let _ = client.post(&shutdown_url).send().await;
    Ok(())
}

/// Prompt user to start the server
fn prompt_start_server() -> VmResult<bool> {
    confirm_select(
        "Package registry server is not running. Start it now?",
        false,
    )
}

/// Start server in background if needed as a detached process
async fn start_server_if_needed(global_config: &GlobalConfig, yes: bool) -> VmResult<()> {
    let server_url = format!(
        "http://localhost:{}",
        global_config.services.package_registry.port
    );

    // Check if server is running
    if check_server_running(global_config).await {
        // Server is running, check if version matches
        if let Ok(server_version) = get_server_version(&server_url).await {
            let cli_version = env!("CARGO_PKG_VERSION");
            if server_version != cli_version {
                vm_warning!(
                    "Package server version mismatch: server={server_version}, cli={cli_version}"
                );
                vm_progress!("Restarting package server...");

                // Attempt graceful shutdown
                let _ = shutdown_server(&server_url).await;

                // Wait a moment for shutdown
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

                // Fall through to start new server
            } else {
                // Version matches, server is good to use
                return Ok(());
            }
        } else {
            // Couldn't get version, assume server is good
            return Ok(());
        }
    }

    if yes || prompt_start_server()? {
        vm_progress!("Starting package registry...");

        let data_dir = vm_core::project::get_package_data_dir()?;
        let port = global_config.services.package_registry.port;

        // Get path to current vm binary
        let vm_bin = std::env::current_exe().context("Failed to get current executable path")?;

        // Spawn server as a detached background process using nohup
        // This ensures it persists after the CLI exits
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            use std::process::Command;

            let log_file = data_dir.join("server.log");
            std::fs::create_dir_all(&data_dir)?;

            // Use nohup to detach the process from the terminal
            let child = Command::new("nohup")
                .arg(vm_bin)
                .arg("system")
                .arg("registry")
                .arg("serve")
                .arg("--host")
                .arg("127.0.0.1")
                .arg("--port")
                .arg(port.to_string())
                .arg("--data")
                .arg(&data_dir)
                .stdout(std::fs::File::create(&log_file)?)
                .stderr(std::fs::File::create(data_dir.join("server.err.log"))?)
                .stdin(std::process::Stdio::null())
                .process_group(0) // Create new process group
                .spawn()
                .context("Failed to spawn package server")?;

            vm_hint!("Server logs: {}", log_file.display());
            drop(child); // Drop handle to detach
        }

        #[cfg(windows)]
        {
            use std::process::Command;

            let log_file = data_dir.join("server.log");
            std::fs::create_dir_all(&data_dir)?;

            // Windows: use START /B for background execution
            Command::new("cmd")
                .args(["/C", "START", "/B"])
                .arg(vm_bin)
                .arg("system")
                .arg("registry")
                .arg("serve")
                .arg("--host")
                .arg("127.0.0.1")
                .arg("--port")
                .arg(port.to_string())
                .arg("--data")
                .arg(&data_dir)
                .stdout(std::fs::File::create(&log_file)?)
                .stderr(std::fs::File::create(data_dir.join("server.err.log"))?)
                .stdin(std::process::Stdio::null())
                .spawn()
                .context("Failed to spawn package server")?;

            vm_hint!("Server logs: {}", log_file.display());
        }

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;

        // Verify it started
        if check_server_running(global_config).await {
            vm_success!("Package registry started on port {port}");
        } else {
            return Err(VmError::from(anyhow::anyhow!(
                "Server process started but health check failed. Check logs at {}",
                data_dir.join("server.log").display()
            )));
        }
    } else {
        return Err(VmError::from(anyhow::anyhow!(
            "Package registry error: server is required but not running"
        )));
    }

    Ok(())
}

/// Handle serve command - run the package server (internal use)
async fn handle_serve(
    host: &str,
    port: u16,
    data: &std::path::Path,
    _global_config: &GlobalConfig,
) -> VmResult<()> {
    vm_progress!(
        "Serving package registry on {host}:{port} with data at {}",
        data.display()
    );

    // Run the server (blocks until shutdown)
    vm_package_server::server::run_server_background(host.to_string(), port, data.to_path_buf())
        .await
        .context("Failed to run package server")?;

    Ok(())
}
