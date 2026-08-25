use anyhow::Result;
use serde_json::Value;
use tracing::{info, warn};

fn loopback_port(host: u16, guest: u16) -> String {
    format!("127.0.0.1:{host}:{guest}")
}

#[derive(Clone, Copy)]
pub(super) struct ManagedContainerSpec<'a> {
    pub name: &'a str,
    pub service: &'a str,
    pub display_name: &'a str,
    pub image: &'a str,
    pub version: &'a str,
    pub host_port: u16,
    pub guest_port: u16,
}

impl ManagedContainerSpec<'_> {
    fn image_ref(self) -> String {
        format!("{}:{}", self.image, self.version)
    }

    fn port_key(self) -> String {
        format!("{}/tcp", self.guest_port)
    }
}

pub(super) async fn start_managed_container(
    executable: &str,
    spec: ManagedContainerSpec<'_>,
    configured_data_dir: &str,
    guest_data_dir: &str,
    environment: &[(&str, &str)],
    command: &[&str],
) -> Result<()> {
    let data_dir = shellexpand::tilde(configured_data_dir).into_owned();
    tokio::fs::create_dir_all(&data_dir).await?;
    if reuse_managed_container(executable, spec).await? {
        return Ok(());
    }

    let mut process = tokio::process::Command::new(executable);
    process
        .arg("run")
        .arg("-d")
        .arg("--name")
        .arg(spec.name)
        .args(["--label", "com.vm.managed=true"])
        .args(["--label", &format!("com.vm.service={}", spec.service)])
        .arg("-p")
        .arg(loopback_port(spec.host_port, spec.guest_port))
        .arg("-v")
        .arg(format!("{data_dir}:{guest_data_dir}"));
    for (name, value) in environment {
        process.arg("-e").arg(format!("{name}={value}"));
    }
    let output = process.arg(spec.image_ref()).args(command).output().await?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "failed to start {} container '{}': {}",
            spec.display_name,
            spec.name,
            detail.trim()
        );
    }
    Ok(())
}

pub(super) async fn reuse_managed_container(
    executable: &str,
    spec: ManagedContainerSpec<'_>,
) -> Result<bool> {
    let Some(inspect) = inspect_container(executable, spec.name).await? else {
        return Ok(false);
    };

    if let Some(reason) = container_mismatch(&inspect, spec) {
        warn!(
            "Existing {} container '{}' mismatches config ({}). Recreating it...",
            spec.display_name, spec.name, reason
        );
        remove_container(executable, spec.name).await?;
        return Ok(false);
    }

    if inspect["State"]["Running"].as_bool().unwrap_or(false) {
        info!(
            "{} service already running ({}) - reusing",
            spec.display_name, spec.name
        );
        return Ok(true);
    }

    info!(
        "Starting existing {} service container: {}",
        spec.display_name, spec.name
    );
    if tokio::process::Command::new(executable)
        .args(["start", spec.name])
        .status()
        .await?
        .success()
    {
        return Ok(true);
    }

    warn!(
        "Failed to start existing {} container; recreating it",
        spec.display_name
    );
    remove_container(executable, spec.name).await?;
    Ok(false)
}

pub(super) async fn stop_managed_container(executable: &str, name: &str) -> Result<()> {
    if inspect_container(executable, name).await?.is_some() {
        remove_container(executable, name).await?;
    }
    Ok(())
}

async fn inspect_container(executable: &str, name: &str) -> Result<Option<Value>> {
    let output = tokio::process::Command::new(executable)
        .args(["container", "inspect", name])
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.to_ascii_lowercase().contains("no such") {
            return Ok(None);
        }
        anyhow::bail!(
            "Failed to inspect managed service container '{name}': {}",
            stderr.trim()
        );
    }
    let inspect = serde_json::from_slice::<Vec<Value>>(&output.stdout)?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Container inspection returned no data for '{name}'"))?;
    Ok(Some(inspect))
}

fn container_mismatch(inspect: &Value, spec: ManagedContainerSpec<'_>) -> Option<String> {
    let mut reasons = Vec::new();
    let actual_image = inspect["Config"]["Image"].as_str().unwrap_or("<unknown>");
    let desired_image = spec.image_ref();
    if actual_image != desired_image {
        reasons.push(format!("image {actual_image} vs desired {desired_image}"));
    }

    let desired_host_port = spec.host_port.to_string();
    let binding_matches = inspect["HostConfig"]["PortBindings"][spec.port_key()]
        .as_array()
        .is_some_and(|bindings| {
            bindings.iter().any(|binding| {
                binding["HostIp"].as_str() == Some("127.0.0.1")
                    && binding["HostPort"].as_str() == Some(desired_host_port.as_str())
            })
        });
    if !binding_matches {
        reasons.push(format!(
            "port must bind 127.0.0.1:{}:{}",
            spec.host_port, spec.guest_port
        ));
    }

    (!reasons.is_empty()).then(|| reasons.join("; "))
}

async fn remove_container(executable: &str, name: &str) -> Result<()> {
    let output = tokio::process::Command::new(executable)
        .args(["rm", "-f", name])
        .output()
        .await?;
    if output.status.success() {
        Ok(())
    } else {
        anyhow::bail!(
            "Failed to remove managed service container '{name}': {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }
}

pub(super) async fn loopback_healthy(port: u16) -> bool {
    tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::{container_mismatch, loopback_port, ManagedContainerSpec};

    #[test]
    fn managed_services_bind_only_to_loopback() {
        assert_eq!(loopback_port(3739, 5432), "127.0.0.1:3739:5432");

        let spec = ManagedContainerSpec {
            name: "vm-postgres-global",
            service: "postgresql",
            display_name: "PostgreSQL",
            image: "postgres",
            version: "15",
            host_port: 3739,
            guest_port: 5432,
        };
        let inspect = serde_json::json!({
            "Config": {"Image": "postgres:15"},
            "HostConfig": {"PortBindings": {
                "5432/tcp": [{"HostIp": "127.0.0.1", "HostPort": "3739"}]
            }}
        });
        assert_eq!(container_mismatch(&inspect, spec), None);

        let public = serde_json::json!({
            "Config": {"Image": "postgres:15"},
            "HostConfig": {"PortBindings": {
                "5432/tcp": [{"HostIp": "0.0.0.0", "HostPort": "3739"}]
            }}
        });
        assert!(container_mismatch(&public, spec)
            .unwrap()
            .contains("127.0.0.1"));
    }
}
