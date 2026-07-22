//! Container status reporting.

use super::LifecycleOperations;
use crate::{ResourceUsage, ServiceStatus, VmStatusReport};
use vm_core::error::{Result, VmError};

impl<'a> LifecycleOperations<'a> {
    pub fn status_report(&self, container: Option<&str>) -> Result<VmStatusReport> {
        let container_name = self.resolve_target_container(container)?;
        let inspect_output = std::process::Command::new(self.executable)
            .args(["inspect", &container_name])
            .output()
            .map_err(|error| VmError::Internal(format!("Failed to inspect container: {error}")))?;

        if !inspect_output.status.success() {
            return Err(VmError::Internal(format!(
                "Container '{container_name}' not found"
            )));
        }

        let inspect_data: serde_json::Value = serde_json::from_slice(&inspect_output.stdout)
            .map_err(|error| {
                VmError::Internal(format!("Failed to parse container info: {error}"))
            })?;
        let container_info = inspect_data.get(0).ok_or_else(|| {
            VmError::Internal(format!(
                "Container '{container_name}' inspect returned no results"
            ))
        })?;
        let state = &container_info["State"];
        let is_running = state["Running"].as_bool().unwrap_or(false);

        let uptime = if is_running {
            self.calculate_uptime(state)
        } else {
            None
        };
        let resources = if is_running {
            self.get_container_resources(&container_name)?
        } else {
            ResourceUsage::default()
        };
        let services = if is_running {
            self.check_all_services(&container_name, &container_info["Config"])?
        } else {
            Vec::new()
        };

        Ok(VmStatusReport {
            name: container_name,
            provider: self.executable.to_string(),
            container_id: container_info["Id"].as_str().map(ToString::to_string),
            is_running,
            uptime,
            resources,
            services,
        })
    }

    fn calculate_uptime(&self, state: &serde_json::Value) -> Option<String> {
        let started_at = state["StartedAt"].as_str()?;
        if started_at == "0001-01-01T00:00:00Z" {
            return None;
        }

        let start_time = match chrono::DateTime::parse_from_rfc3339(started_at) {
            Ok(time) => time,
            Err(_) => return Some("unknown".to_string()),
        };
        let duration =
            chrono::Utc::now().signed_duration_since(start_time.with_timezone(&chrono::Utc));

        Some(if duration.num_days() > 0 {
            format!("{}d", duration.num_days())
        } else if duration.num_hours() > 0 {
            format!("{}h", duration.num_hours())
        } else if duration.num_minutes() > 0 {
            format!("{}m", duration.num_minutes())
        } else {
            "now".to_string()
        })
    }

    fn get_container_resources(&self, container_name: &str) -> Result<ResourceUsage> {
        let stats_output = std::process::Command::new(self.executable)
            .args([
                "stats",
                "--no-stream",
                "--format",
                "{{.CPUPerc}}\t{{.MemUsage}}",
                container_name,
            ])
            .output()
            .map_err(|error| {
                VmError::Internal(format!("Failed to get container stats: {error}"))
            })?;

        if !stats_output.status.success() {
            return Ok(ResourceUsage::default());
        }

        let stats = String::from_utf8_lossy(&stats_output.stdout);
        let Some((cpu, memory)) = stats.trim().split_once('\t') else {
            return Ok(ResourceUsage::default());
        };
        let mut memory_values = memory.split('/').map(str::trim);
        let memory_used_mb = memory_values.next().and_then(Self::parse_memory_value);
        let memory_limit_mb = memory_values.next().and_then(Self::parse_memory_value);
        let (disk_used_gb, disk_total_gb) = self.get_disk_usage(container_name);

        Ok(ResourceUsage {
            cpu_percent: cpu.trim_end_matches('%').parse().ok(),
            memory_used_mb,
            memory_limit_mb,
            disk_used_gb,
            disk_total_gb,
        })
    }

    fn parse_memory_value(value: &str) -> Option<u64> {
        if let Some(mebibytes) = value
            .strip_suffix("MiB")
            .or_else(|| value.strip_suffix("MB"))
        {
            return mebibytes.parse::<f64>().ok().map(|value| value as u64);
        }

        value
            .strip_suffix("GiB")
            .or_else(|| value.strip_suffix("GB"))
            .and_then(|gibibytes| gibibytes.parse::<f64>().ok())
            .map(|value| (value * 1024.0) as u64)
    }

    fn get_disk_usage(&self, container_name: &str) -> (Option<f64>, Option<f64>) {
        let Ok(output) = std::process::Command::new(self.executable)
            .args(["exec", container_name, "df", "-h", "/"])
            .output()
        else {
            return (None, None);
        };
        if !output.status.success() {
            return (None, None);
        }

        let output = String::from_utf8_lossy(&output.stdout);
        let Some(values) = output.lines().nth(1) else {
            return (None, None);
        };
        let values: Vec<_> = values.split_whitespace().collect();
        if values.len() < 4 {
            return (None, None);
        }

        (
            Self::parse_disk_value(values[2]),
            Self::parse_disk_value(values[1]),
        )
    }

    fn parse_disk_value(value: &str) -> Option<f64> {
        if let Some(gigabytes) = value.strip_suffix('G') {
            return gigabytes.parse().ok();
        }
        if let Some(megabytes) = value.strip_suffix('M') {
            return megabytes.parse::<f64>().ok().map(|value| value / 1024.0);
        }

        value
            .strip_suffix('K')
            .and_then(|kilobytes| kilobytes.parse::<f64>().ok())
            .map(|value| value / (1024.0 * 1024.0))
    }

    fn check_all_services(
        &self,
        container_name: &str,
        config: &serde_json::Value,
    ) -> Result<Vec<ServiceStatus>> {
        let Some(exposed_ports) = config["ExposedPorts"].as_object() else {
            return Ok(Vec::new());
        };

        Ok(exposed_ports
            .keys()
            .filter_map(|port_spec| port_spec.split('/').next()?.parse::<u16>().ok())
            .map(|port| {
                let name = Self::identify_service_by_port(port);
                let host_port = self.get_host_port(container_name, port);
                match name.as_str() {
                    "postgresql" => super::health::check_postgres_status(
                        self.executable,
                        container_name,
                        port,
                        host_port,
                    ),
                    "redis" => super::health::check_redis_status(
                        self.executable,
                        container_name,
                        port,
                        host_port,
                    ),
                    "mongodb" => super::health::check_mongodb_status(
                        self.executable,
                        container_name,
                        port,
                        host_port,
                    ),
                    _ => ServiceStatus {
                        name,
                        is_running: true,
                        port: Some(port),
                        host_port,
                        metrics: None,
                        error: None,
                    },
                }
            })
            .collect())
    }

    fn identify_service_by_port(port: u16) -> String {
        match port {
            5432 => "postgresql".to_string(),
            6379 => "redis".to_string(),
            27017 => "mongodb".to_string(),
            3306 => "mysql".to_string(),
            8080 => "http".to_string(),
            3000 => "node".to_string(),
            8000 => "python".to_string(),
            _ => format!("service-{port}"),
        }
    }

    fn get_host_port(&self, container_name: &str, container_port: u16) -> Option<u16> {
        let output = std::process::Command::new(self.executable)
            .args(["port", container_name, &container_port.to_string()])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }

        String::from_utf8_lossy(&output.stdout)
            .trim()
            .split(':')
            .next_back()?
            .parse()
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::LifecycleOperations;

    #[test]
    fn parses_docker_memory_units_as_mebibytes() {
        assert_eq!(LifecycleOperations::parse_memory_value("512MiB"), Some(512));
        assert_eq!(
            LifecycleOperations::parse_memory_value("1.5GiB"),
            Some(1536)
        );
        assert_eq!(LifecycleOperations::parse_memory_value("unknown"), None);
    }

    #[test]
    fn parses_df_units_as_gigabytes() {
        assert_eq!(LifecycleOperations::parse_disk_value("2G"), Some(2.0));
        assert_eq!(LifecycleOperations::parse_disk_value("512M"), Some(0.5));
        assert_eq!(LifecycleOperations::parse_disk_value("unknown"), None);
    }
}
