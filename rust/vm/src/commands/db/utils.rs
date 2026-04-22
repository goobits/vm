//! DB utility functions

use crate::error::{VmError, VmResult};
use crate::service_manager::get_service_manager;

pub async fn execute_psql_command(command: &str) -> VmResult<String> {
    let service_manager = get_service_manager().map_err(|e| {
        VmError::general(
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
            "Service manager not initialized",
        )
    })?;
    let pg_state = service_manager.get_service_status("postgresql");

    if !pg_state.is_some_and(|s| s.is_running) {
        return Err(VmError::general(
            std::io::Error::new(
                std::io::ErrorKind::Other,
                "PostgreSQL service is not running.",
            ),
            "Please start a VM that uses the PostgreSQL service to use this command.",
        ));
    }

    let executable = detect_container_runtime();
    let output = tokio::process::Command::new(&executable)
        .arg("exec")
        .arg("-i")
        .arg("vm-postgres-global")
        .arg("psql")
        .arg("-U")
        .arg("postgres")
        .arg("-t") // Tuples only, no headers/footers
        .arg("-c")
        .arg(command)
        .output()
        .await
        .map_err(|e| VmError::general(e, "Failed to execute docker command"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(VmError::general(
            std::io::Error::new(std::io::ErrorKind::Other, "Failed to execute psql command."),
            stderr,
        ))
    }
}

fn detect_container_runtime() -> String {
    vm_config::AppConfig::load(None, None)
        .ok()
        .and_then(|config| {
            config
                .vm
                .provider
                .or(config.global.defaults.provider)
                .filter(|provider| matches!(provider.as_str(), "docker" | "podman"))
        })
        .unwrap_or_else(|| "docker".to_string())
}
