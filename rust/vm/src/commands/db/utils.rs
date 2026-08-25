//! DB utility functions

use crate::error::{VmError, VmResult};
use crate::services::service_lifecycle;

pub async fn execute_psql_command(command: &str) -> VmResult<String> {
    let lifecycle = service_lifecycle().map_err(|e| {
        VmError::general(
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
            "Service lifecycle not initialized",
        )
    })?;
    let pg_state = lifecycle.service_status("postgresql");

    if !pg_state.is_some_and(|s| s.is_running) {
        return Err(VmError::general(
            std::io::Error::new(
                std::io::ErrorKind::Other,
                "PostgreSQL service is not running.",
            ),
            "Start an environment that uses the PostgreSQL service before running this command.",
        ));
    }

    let executable = crate::utils::configured_container_runtime();
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
