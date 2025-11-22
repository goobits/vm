//! Redis Service Implementation

use anyhow::Result;
use tracing::{info, warn};
use vm_config::GlobalConfig;

use super::{get_or_generate_password, ManagedService};

/// Redis database service that implements the ManagedService trait
pub struct RedisService;

impl RedisService {
    /// Create a new RedisService instance
    pub fn new() -> Self {
        Self
    }
}

impl Default for RedisService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ManagedService for RedisService {
    async fn start(&self, global_config: &GlobalConfig) -> Result<()> {
        let settings = &global_config.services.redis;
        let container_name = "vm-redis-global";

        let data_dir = shellexpand::tilde(&settings.data_dir).to_string();
        tokio::fs::create_dir_all(&data_dir).await?;

        let password = get_or_generate_password("redis").await?;

        // Reuse existing container if present
        if container_exists(container_name).await? {
            // If configuration mismatches, stop and recreate to avoid conflicts
            if let Some(reason) =
                check_service_mismatch(container_name, "redis", settings.port, &settings.version)
                    .await?
            {
                warn!(
                    "Existing Redis container '{}' mismatches config ({}). Recreating it...",
                    container_name, reason
                );
                let _ = tokio::process::Command::new("docker")
                    .args(["rm", "-f", container_name])
                    .status()
                    .await;
            } else {
                if container_running(container_name).await? {
                    info!(
                        "Redis service already running ({}) - reusing",
                        container_name
                    );
                    return Ok(());
                }

                info!(
                    "Starting existing Redis service container: {}",
                    container_name
                );
                let status = tokio::process::Command::new("docker")
                    .arg("start")
                    .arg(container_name)
                    .status()
                    .await?;
                if status.success() {
                    return Ok(());
                } else {
                    warn!("Failed to start existing Redis container, recreating it...");
                    // fall through to recreate
                }
            }
        }

        let mut cmd = tokio::process::Command::new("docker");
        cmd.arg("run")
            .arg("-d")
            .arg("--name")
            .arg(container_name)
            .arg("-p")
            .arg(format!("{}:6379", settings.port))
            .arg("-v")
            .arg(format!("{data_dir}:/data"))
            .arg(format!("redis:{}", settings.version))
            .arg("--requirepass")
            .arg(password);

        let status = cmd.status().await?;
        if !status.success() {
            return Err(anyhow::anyhow!("Failed to start Redis container"));
        }

        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        let container_name = "vm-redis-global";

        let mut stop_cmd = tokio::process::Command::new("docker");
        stop_cmd.arg("stop").arg(container_name);
        if !stop_cmd.status().await?.success() {
            warn!("Failed to stop Redis container, it may not have been running.");
        }

        let mut rm_cmd = tokio::process::Command::new("docker");
        rm_cmd.arg("rm").arg(container_name);
        if !rm_cmd.status().await?.success() {
            warn!("Failed to remove Redis container.");
        }

        Ok(())
    }

    async fn check_health(&self, global_config: &GlobalConfig) -> bool {
        let port = self.get_port(global_config);
        // For database services, a TCP connection is a reliable health check
        tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .is_ok()
    }

    fn name(&self) -> &str {
        "redis"
    }

    fn get_port(&self, global_config: &GlobalConfig) -> u16 {
        global_config.services.redis.port
    }
}

// Helper: check if a container exists
async fn container_exists(name: &str) -> Result<bool> {
    let output = tokio::process::Command::new("docker")
        .args([
            "ps",
            "-a",
            "--format",
            "{{.Names}}",
            "--filter",
            &format!("name=^{name}$"),
        ])
        .output()
        .await?;
    let names = String::from_utf8_lossy(&output.stdout);
    Ok(names.lines().any(|n| n.trim() == name))
}

// Helper: check if container is running
async fn container_running(name: &str) -> Result<bool> {
    let output = tokio::process::Command::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", name])
        .output()
        .await?;
    if !output.status.success() {
        return Ok(false);
    }
    let state = String::from_utf8_lossy(&output.stdout);
    Ok(state.trim() == "true")
}

// Return Some(reason) when existing container doesn't match desired config (port or image tag)
async fn check_service_mismatch(
    name: &str,
    image_base: &str,
    desired_port: u16,
    desired_version: &str,
) -> Result<Option<String>> {
    let mut reasons = Vec::new();

    // Check port mapping
    if let Ok(output) = tokio::process::Command::new("docker")
        .args(["inspect", "-f", "{{json .NetworkSettings.Ports}}", name])
        .output()
        .await
    {
        if output.status.success() {
            let ports = String::from_utf8_lossy(&output.stdout);
            if !ports.contains(&format!("{desired_port}/tcp")) {
                reasons.push(format!(
                    "ports {} vs desired {}/tcp",
                    ports.trim(),
                    desired_port
                ));
            }
        }
    }

    // Check image tag
    if let Ok(output) = tokio::process::Command::new("docker")
        .args(["inspect", "-f", "{{.Config.Image}}", name])
        .output()
        .await
    {
        if output.status.success() {
            let image = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let desired_image = format!("{image_base}:{desired_version}");
            if image != desired_image {
                reasons.push(format!("image {} vs desired {}", image, desired_image));
            }
        }
    }

    Ok(if reasons.is_empty() {
        None
    } else {
        Some(reasons.join("; "))
    })
}
