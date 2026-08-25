use anyhow::Result;
use serde_json::Value;
use tracing::{info, warn};
use vm_config::GlobalConfig;

use super::{container_runtime, get_or_generate_password, ManagedService};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ManagedContainerService {
    Postgresql,
    Redis,
    Mongodb,
    Mysql,
}

impl ManagedContainerService {
    pub(super) const ALL: [Self; 4] = [Self::Postgresql, Self::Redis, Self::Mongodb, Self::Mysql];

    pub(super) const fn service_name(self) -> &'static str {
        match self {
            Self::Postgresql => "postgresql",
            Self::Redis => "redis",
            Self::Mongodb => "mongodb",
            Self::Mysql => "mysql",
        }
    }

    const fn container_name(self) -> &'static str {
        match self {
            Self::Postgresql => "vm-postgres-global",
            Self::Redis => "vm-redis-global",
            Self::Mongodb => "vm-mongodb-global",
            Self::Mysql => "vm-mysql-global",
        }
    }

    const fn display_name(self) -> &'static str {
        match self {
            Self::Postgresql => "PostgreSQL",
            Self::Redis => "Redis",
            Self::Mongodb => "MongoDB",
            Self::Mysql => "MySQL",
        }
    }

    const fn image(self) -> &'static str {
        match self {
            Self::Postgresql => "postgres",
            Self::Redis => "redis",
            Self::Mongodb => "mongo",
            Self::Mysql => "mysql",
        }
    }

    const fn guest_port(self) -> u16 {
        match self {
            Self::Postgresql => 5432,
            Self::Redis => 6379,
            Self::Mongodb => 27017,
            Self::Mysql => 3306,
        }
    }

    const fn guest_data_dir(self) -> &'static str {
        match self {
            Self::Postgresql => "/var/lib/postgresql/data",
            Self::Redis => "/data",
            Self::Mongodb => "/data/db",
            Self::Mysql => "/var/lib/mysql",
        }
    }

    fn settings<'a>(self, config: &'a GlobalConfig) -> ContainerServiceSettings<'a> {
        match self {
            Self::Postgresql => ContainerServiceSettings {
                version: &config.services.postgresql.version,
                data_dir: &config.services.postgresql.data_dir,
                host_port: config.services.postgresql.port,
            },
            Self::Redis => ContainerServiceSettings {
                version: &config.services.redis.version,
                data_dir: &config.services.redis.data_dir,
                host_port: config.services.redis.port,
            },
            Self::Mongodb => ContainerServiceSettings {
                version: &config.services.mongodb.version,
                data_dir: &config.services.mongodb.data_dir,
                host_port: config.services.mongodb.port,
            },
            Self::Mysql => ContainerServiceSettings {
                version: &config.services.mysql.version,
                data_dir: &config.services.mysql.data_dir,
                host_port: config.services.mysql.port,
            },
        }
    }

    async fn start_container(self, config: &GlobalConfig) -> Result<()> {
        let settings = self.settings(config);
        let executable = container_runtime(config);
        let spec = ManagedContainerSpec {
            name: self.container_name(),
            service: self.service_name(),
            display_name: self.display_name(),
            image: self.image(),
            version: settings.version,
            host_port: settings.host_port,
            guest_port: self.guest_port(),
        };
        let password = get_or_generate_password(self.service_name()).await?;

        match self {
            Self::Postgresql => {
                start_managed_container(
                    executable,
                    spec,
                    settings.data_dir,
                    self.guest_data_dir(),
                    &[("POSTGRES_PASSWORD", &password)],
                    &[],
                )
                .await
            }
            Self::Redis => {
                start_managed_container(
                    executable,
                    spec,
                    settings.data_dir,
                    self.guest_data_dir(),
                    &[],
                    &["--requirepass", &password],
                )
                .await
            }
            Self::Mongodb => {
                start_managed_container(
                    executable,
                    spec,
                    settings.data_dir,
                    self.guest_data_dir(),
                    &[
                        ("MONGO_INITDB_ROOT_USERNAME", "root"),
                        ("MONGO_INITDB_ROOT_PASSWORD", &password),
                    ],
                    &[],
                )
                .await
            }
            Self::Mysql => {
                start_managed_container(
                    executable,
                    spec,
                    settings.data_dir,
                    self.guest_data_dir(),
                    &[("MYSQL_ROOT_PASSWORD", &password)],
                    &[],
                )
                .await
            }
        }
    }
}

struct ContainerServiceSettings<'a> {
    version: &'a str,
    data_dir: &'a str,
    host_port: u16,
}

#[async_trait::async_trait]
impl ManagedService for ManagedContainerService {
    async fn start(&self, global_config: &GlobalConfig) -> Result<()> {
        self.start_container(global_config).await
    }

    async fn stop(&self) -> Result<()> {
        let executable = crate::utils::configured_container_runtime();
        stop_managed_container(&executable, self.container_name()).await
    }

    async fn check_health(&self, global_config: &GlobalConfig) -> bool {
        loopback_healthy(self.get_port(global_config)).await
    }

    fn name(&self) -> &str {
        self.service_name()
    }

    fn get_port(&self, global_config: &GlobalConfig) -> u16 {
        self.settings(global_config).host_port
    }
}

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
    use std::collections::BTreeSet;

    use super::{container_mismatch, loopback_port, ManagedContainerService, ManagedContainerSpec};

    #[test]
    fn managed_database_definitions_are_distinct() {
        let names = ManagedContainerService::ALL
            .map(ManagedContainerService::service_name)
            .into_iter()
            .collect::<BTreeSet<_>>();
        let containers = ManagedContainerService::ALL
            .map(ManagedContainerService::container_name)
            .into_iter()
            .collect::<BTreeSet<_>>();

        assert_eq!(names.len(), ManagedContainerService::ALL.len());
        assert_eq!(containers.len(), ManagedContainerService::ALL.len());
    }

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
