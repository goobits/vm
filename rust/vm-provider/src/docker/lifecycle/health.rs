//! Service-specific health checks
use crate::ServiceStatus;

/// Check PostgreSQL service health using pg_isready
pub(super) fn check_postgres_status(
    container_name: &str,
    port: u16,
    host_port: Option<u16>,
) -> ServiceStatus {
    let result = std::process::Command::new("docker")
        .args([
            "exec",
            container_name,
            "pg_isready",
            "-p",
            &port.to_string(),
        ])
        .output();

    match result {
        Ok(output) => {
            let is_running = output.status.success();
            ServiceStatus {
                name: "postgresql".to_string(),
                is_running,
                port: Some(port),
                host_port,
                metrics: if is_running {
                    Some("accepting connections".to_string())
                } else {
                    None
                },
                error: if !is_running {
                    Some(String::from_utf8_lossy(&output.stderr).trim().to_string())
                } else {
                    None
                },
            }
        }
        Err(e) => ServiceStatus {
            name: "postgresql".to_string(),
            is_running: false,
            port: Some(port),
            host_port,
            metrics: None,
            error: Some(format!("Health check failed: {e}")),
        },
    }
}

/// Check Redis service health using redis-cli ping
pub(super) fn check_redis_status(
    container_name: &str,
    port: u16,
    host_port: Option<u16>,
) -> ServiceStatus {
    let result = std::process::Command::new("docker")
        .args([
            "exec",
            container_name,
            "redis-cli",
            "-p",
            &port.to_string(),
            "ping",
        ])
        .output();

    match result {
        Ok(output) => {
            let response = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_lowercase();
            let is_running = output.status.success() && response == "pong";
            ServiceStatus {
                name: "redis".to_string(),
                is_running,
                port: Some(port),
                host_port,
                metrics: if is_running {
                    Some("PONG".to_string())
                } else {
                    None
                },
                error: if !is_running {
                    Some(String::from_utf8_lossy(&output.stderr).trim().to_string())
                } else {
                    None
                },
            }
        }
        Err(e) => ServiceStatus {
            name: "redis".to_string(),
            is_running: false,
            port: Some(port),
            host_port,
            metrics: None,
            error: Some(format!("Health check failed: {e}")),
        },
    }
}

/// Check MongoDB service health using mongosh ping
pub(super) fn check_mongodb_status(
    container_name: &str,
    port: u16,
    host_port: Option<u16>,
) -> ServiceStatus {
    let result = std::process::Command::new("docker")
        .args([
            "exec",
            container_name,
            "mongosh",
            "--port",
            &port.to_string(),
            "--eval",
            "db.adminCommand('ping')",
        ])
        .output();

    match result {
        Ok(output) => {
            let response = String::from_utf8_lossy(&output.stdout);
            let is_running = output.status.success() && response.contains("ok");
            ServiceStatus {
                name: "mongodb".to_string(),
                is_running,
                port: Some(port),
                host_port,
                metrics: if is_running {
                    Some("ping ok".to_string())
                } else {
                    None
                },
                error: if !is_running {
                    Some(String::from_utf8_lossy(&output.stderr).trim().to_string())
                } else {
                    None
                },
            }
        }
        Err(e) => ServiceStatus {
            name: "mongodb".to_string(),
            is_running: false,
            port: Some(port),
            host_port,
            metrics: None,
            error: Some(format!("Health check failed: {e}")),
        },
    }
}
