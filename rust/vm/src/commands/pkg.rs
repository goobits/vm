//! Package registry command handlers
//!
//! This module provides command handlers for the VM package registry functionality,
//! integrating with the vm-package-server library to provide npm, pip, and cargo
//! package caching and serving capabilities.

use crate::cli::{PkgConfigAction, PkgSubcommand};
use crate::error::{VmError, VmResult};
use crate::service_manager::get_service_manager;
use crate::service_registry::get_service_registry;
use anyhow::Context;
use dialoguer::Confirm;
use vm_cli::msg;
use vm_config::GlobalConfig;
use vm_core::{vm_error, vm_println, vm_success};
use vm_messages::messages::MESSAGES;

use vm_package_server;

/// Handle package registry commands
pub async fn handle_pkg_command(
    command: &PkgSubcommand,
    global_config: GlobalConfig,
) -> VmResult<()> {
    match command {
        PkgSubcommand::Status { yes } => handle_status(*yes, &global_config).await,
        PkgSubcommand::Add { r#type, yes } => {
            handle_add(r#type.as_deref(), *yes, &global_config).await
        }
        PkgSubcommand::Remove { force, yes } => handle_remove(*force, *yes, &global_config).await,
        PkgSubcommand::List { yes } => handle_list(*yes, &global_config).await,
        PkgSubcommand::Config { action } => handle_config(action, &global_config).await,
        PkgSubcommand::Use { shell, port } => {
            handle_use(shell.as_deref(), *port, &global_config).await
        }
        PkgSubcommand::Serve { host, port, data } => {
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
    let service_manager = get_service_manager();

    vm_println!("{}", MESSAGES.vm.pkg_registry_status_header);

    // Get service status from service manager
    if let Some(service_state) = service_manager.get_service_status("package_registry") {
        vm_println!(
            "{}",
            msg!(
                MESSAGES.vm.pkg_registry_reference_count,
                count = service_state.reference_count.to_string()
            )
        );
        vm_println!(
            "{}",
            msg!(
                MESSAGES.vm.pkg_registry_registered_vms,
                vms = format!("{:?}", service_state.registered_vms)
            )
        );

        let status_line = registry.format_service_status(
            "package_registry",
            service_state.is_running,
            service_state.reference_count,
        );
        vm_println!("{}", status_line);
    } else {
        vm_println!("{}", MESSAGES.vm.pkg_registry_not_managed);
    }

    // Check actual server status for verification
    if check_server_running_with_url(&server_url).await {
        vm_println!("{}", MESSAGES.vm.pkg_registry_health_ok);
    } else {
        vm_println!("{}", MESSAGES.vm.pkg_registry_health_failed);
    }

    vm_println!("{}", MESSAGES.vm.pkg_registry_auto_managed_info);

    // Show additional package registry info
    vm_package_server::show_status(&server_url).map_err(VmError::from)
}

/// Add package from current directory
async fn handle_add(
    package_type: Option<&str>,
    yes: bool,
    global_config: &GlobalConfig,
) -> VmResult<()> {
    let server_url = format!(
        "http://localhost:{}",
        global_config.services.package_registry.port
    );

    // Ensure server is running before attempting to add package
    start_server_if_needed(global_config, yes).await?;

    vm_println!("{}", MESSAGES.vm.pkg_publishing);

    vm_package_server::client_ops::add_package(&server_url, package_type).map_err(VmError::from)?;

    vm_success!("Package published successfully");
    Ok(())
}

/// Remove package from registry
async fn handle_remove(force: bool, yes: bool, global_config: &GlobalConfig) -> VmResult<()> {
    let server_url = format!(
        "http://localhost:{}",
        global_config.services.package_registry.port
    );

    // Ensure server is running before attempting to remove package
    start_server_if_needed(global_config, yes).await?;

    vm_println!("{}", MESSAGES.vm.pkg_removing);

    vm_package_server::client_ops::remove_package(&server_url, force).map_err(VmError::from)?;

    vm_success!("Package removed successfully");
    Ok(())
}

/// List packages in registry
async fn handle_list(yes: bool, global_config: &GlobalConfig) -> VmResult<()> {
    let server_url = format!(
        "http://localhost:{}",
        global_config.services.package_registry.port
    );

    // Ensure server is running for complete package listing
    start_server_if_needed(global_config, yes).await?;

    vm_package_server::client_ops::list_packages(&server_url).map_err(VmError::from)?;

    Ok(())
}

/// Handle configuration commands
async fn handle_config(action: &PkgConfigAction, global_config: &GlobalConfig) -> VmResult<()> {
    let port = global_config.services.package_registry.port;
    match action {
        PkgConfigAction::Show => {
            vm_println!("{}", MESSAGES.vm.pkg_config_header);
            vm_println!(
                "{}",
                msg!(MESSAGES.vm.pkg_config_port, port = port.to_string())
            );
            vm_println!("{}", msg!(MESSAGES.vm.pkg_config_host, host = "0.0.0.0"));
            vm_println!(
                "{}",
                msg!(MESSAGES.vm.pkg_config_fallback, fallback = "enabled")
            );
            Ok(())
        }
        PkgConfigAction::Get { key } => {
            match key.as_str() {
                "port" => vm_println!("{}", port),
                "host" => vm_println!("0.0.0.0"),
                "fallback" => vm_println!("true"),
                _ => vm_error!("Unknown configuration key: {}", key),
            }
            Ok(())
        }
        PkgConfigAction::Set { key, value } => {
            vm_println!(
                "{}",
                msg!(MESSAGES.vm.pkg_config_setting, key = key, value = value)
            );
            vm_println!("{}", MESSAGES.vm.pkg_config_changes_hint);
            Ok(())
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
                "{}",
                msg!(
                    MESSAGES.vm.pkg_use_bash_config,
                    shell = shell_type,
                    port = actual_port.to_string()
                )
            );
        }
        "fish" => {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm.pkg_use_fish_config,
                    port = actual_port.to_string()
                )
            );
        }
        _ => {
            vm_error!(
                "{}",
                msg!(MESSAGES.vm.pkg_use_unsupported, shell = shell_type)
            );
        }
    }

    Ok(())
}

/// Check if the package registry server is running
async fn check_server_running(global_config: &GlobalConfig) -> bool {
    let server_url = format!(
        "http://localhost:{}",
        global_config.services.package_registry.port
    );
    check_server_running_with_url(&server_url).await
}

/// Check if the package registry server is running at a specific URL
async fn check_server_running_with_url(base_url: &str) -> bool {
    let health_url = format!("{base_url}/health");
    match reqwest::get(&health_url).await {
        Ok(response) => response.status().is_success(),
        Err(_) => false,
    }
}

/// Get the version of the running server
async fn get_server_version(base_url: &str) -> VmResult<String> {
    let status_url = format!("{base_url}/api/status");
    let response = reqwest::get(&status_url)
        .await
        .map_err(|e| VmError::general(e, "Failed to get server status"))?;

    if !response.status().is_success() {
        return Err(VmError::from(anyhow::anyhow!(
            "Server returned error status"
        )));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| VmError::general(e, "Failed to parse server status"))?;

    let version = json["version"]
        .as_str()
        .ok_or_else(|| VmError::from(anyhow::anyhow!("Version not found in response")))?
        .to_string();

    Ok(version)
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
    let confirmed = Confirm::new()
        .with_prompt("Package registry server is not running. Start it now?")
        .default(false)
        .interact()
        .map_err(|e| VmError::general(e, "Failed to prompt user"))?;

    Ok(confirmed)
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
                vm_println!(
                    "{}",
                    msg!(
                        MESSAGES.vm.pkg_version_mismatch,
                        server_version = &server_version,
                        cli_version = cli_version
                    )
                );
                vm_println!("{}", MESSAGES.vm.pkg_restarting);

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
            // Couldn't get version, assume server is good (backward compatibility)
            return Ok(());
        }
    }

    if yes || prompt_start_server()? {
        vm_println!("{}", MESSAGES.vm.pkg_server_starting);

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
                .arg("pkg")
                .arg("serve")
                .arg("--host")
                .arg("0.0.0.0")
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

            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm.pkg_server_logs,
                    log_path = log_file.display().to_string()
                )
            );
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
                .arg("pkg")
                .arg("serve")
                .arg("--host")
                .arg("0.0.0.0")
                .arg("--port")
                .arg(port.to_string())
                .arg("--data")
                .arg(&data_dir)
                .stdout(std::fs::File::create(&log_file)?)
                .stderr(std::fs::File::create(data_dir.join("server.err.log"))?)
                .stdin(std::process::Stdio::null())
                .spawn()
                .context("Failed to spawn package server")?;

            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm.pkg_server_logs,
                    log_path = log_file.display().to_string()
                )
            );
        }

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;

        // Verify it started
        if check_server_running(global_config).await {
            vm_success!("Package registry started successfully on port {}", port);
            vm_println!(
                "{}",
                msg!(MESSAGES.vm.pkg_server_started_info, port = port.to_string())
            );
        } else {
            return Err(VmError::from(anyhow::anyhow!(
                "Server process started but health check failed. Check logs at {}",
                data_dir.join("server.log").display()
            )));
        }
    } else {
        return Err(VmError::from(anyhow::anyhow!(
            "Package registry server is required but not running"
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
    vm_println!(
        "{}",
        msg!(
            MESSAGES.vm.pkg_serve_starting,
            host = host,
            port = port.to_string(),
            data = data.display().to_string()
        )
    );

    // Run the server (blocks until shutdown)
    vm_package_server::server::run_server_background(host.to_string(), port, data.to_path_buf())
        .await
        .context("Failed to run package server")?;

    Ok(())
}
