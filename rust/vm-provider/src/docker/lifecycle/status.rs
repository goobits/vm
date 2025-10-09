//! Container status reporting and listing
use super::LifecycleOperations;
use crate::{docker::command::DockerCommand, ResourceUsage, ServiceStatus, VmStatusReport};
use tracing::info;
use vm_core::error::{Result, VmError};

impl<'a> LifecycleOperations<'a> {
    #[must_use = "container listing results should be handled"]
    pub fn list_containers(&self) -> Result<()> {
        self.list_containers_with_stats()
    }

    /// Enhanced container listing with CPU/RAM stats in clean minimal format
    #[must_use = "container listing results should be handled"]
    pub fn list_containers_with_stats(&self) -> Result<()> {
        // Get container info
        let ps_output = DockerCommand::new()
            .subcommand("ps")
            .arg("-a")
            .arg("--format")
            .arg("{{.Names}}\t{{.Status}}\t{{.Ports}}\t{{.CreatedAt}}")
            .execute_raw()
            .map_err(|e| VmError::Internal(format!("Failed to get container information from Docker. Ensure Docker is running and accessible: {}", e)))?;

        if !ps_output.status.success() {
            return Err(VmError::Internal(
                "Docker ps command failed. Check that Docker is installed, running, and accessible"
                    .to_string(),
            ));
        }

        // Get stats for running containers
        let stats_output = DockerCommand::new()
            .subcommand("stats")
            .arg("--no-stream")
            .arg("--format")
            .arg("{{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}")
            .execute_raw()
            .map_err(|e| VmError::Internal(format!("Failed to get container resource statistics from Docker. Some containers may not be running: {}", e)))?;

        let stats_data = if stats_output.status.success() {
            String::from_utf8_lossy(&stats_output.stdout)
        } else {
            "".into() // Continue without stats if command fails
        };

        // Parse stats into a map
        let mut stats_map = std::collections::HashMap::new();
        for line in stats_data.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let name = parts[0];
                let cpu = parts[1];
                let memory = parts[2];
                stats_map.insert(name.to_string(), (cpu.to_string(), memory.to_string()));
            }
        }

        info!("vm list");
        info!("-------");

        let mut total_cpu = 0.0;
        let mut total_memory_mb = 0u64;
        let mut running_count = 0;

        let ps_data = String::from_utf8_lossy(&ps_output.stdout);
        for line in ps_data.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 4 {
                let name = parts[0];
                let status = parts[1];
                let ports = parts[2];
                let _created = parts[3];

                // Determine status icon and running state
                let (icon, is_running, duration) = self.parse_container_status(status);

                // Get stats if running
                let (cpu_display, mem_display) = if is_running {
                    self.process_container_stats(
                        name,
                        &stats_map,
                        &mut total_cpu,
                        &mut total_memory_mb,
                        &mut running_count,
                    )
                } else {
                    ("--".to_string(), "--".to_string())
                };

                // Format ports (clean up the display)
                let ports_display = self.format_ports_minimal(ports);

                // Shorten container name (remove -dev suffix for display)
                let display_name = if let Some(stripped) = name.strip_suffix("-dev") {
                    stripped
                } else {
                    name
                };

                // Print in clean minimal format with better alignment
                // Truncate long names to prevent misalignment
                let display_name = if display_name.len() > 20 {
                    &display_name[..20]
                } else {
                    display_name
                };

                info!(
                    "{} {:<20} {:>7} {:>8} {:<13} [{}]",
                    icon, display_name, cpu_display, mem_display, ports_display, duration
                );
            }
        }

        // Print totals
        if running_count > 0 {
            let total_memory_display = if total_memory_mb >= 1024 {
                format!("{:.1}GB", total_memory_mb as f64 / 1024.0)
            } else {
                format!("{}MB", total_memory_mb)
            };

            info!(
                "\nTotal: {:.1}% CPU, {} RAM",
                total_cpu, total_memory_display
            );
        }

        Ok(())
    }

    /// Process container stats and update totals
    #[allow(clippy::excessive_nesting)]
    fn process_container_stats(
        &self,
        name: &str,
        stats_map: &std::collections::HashMap<String, (String, String)>,
        total_cpu: &mut f64,
        total_memory_mb: &mut u64,
        running_count: &mut usize,
    ) -> (String, String) {
        if let Some((cpu, mem)) = stats_map.get(name) {
            *running_count += 1;

            // Parse CPU percentage
            if let Some(cpu_val) = cpu.strip_suffix('%') {
                if let Ok(cpu_num) = cpu_val.parse::<f64>() {
                    *total_cpu += cpu_num;
                }
            }

            // Parse memory (format: "156MiB / 2GiB" or "156MB / 2GB")
            if let Some(mem_used) = mem.split('/').next() {
                let trimmed = mem_used.trim();
                if let Some(mb_val) = trimmed
                    .strip_suffix("MiB")
                    .or_else(|| trimmed.strip_suffix("MB"))
                {
                    if let Ok(mb_num) = mb_val.parse::<f64>() {
                        *total_memory_mb += mb_num.round() as u64;
                    }
                } else if let Some(gb_val) = trimmed
                    .strip_suffix("GiB")
                    .or_else(|| trimmed.strip_suffix("GB"))
                {
                    if let Ok(gb_num) = gb_val.parse::<f64>() {
                        *total_memory_mb += (gb_num * 1024.0).round() as u64;
                    }
                }
            }

            // Clean up memory display
            let mem_display = self.format_memory_display(mem.split('/').next().unwrap_or(mem));
            (cpu.clone(), mem_display)
        } else {
            ("--".to_string(), "--".to_string())
        }
    }

    /// Format memory display to clean format (1.2GB, 156MB)
    fn format_memory_display(&self, mem_str: &str) -> String {
        let trimmed = mem_str.trim();

        // Parse MiB/MB values
        if let Some(mb_val) = trimmed
            .strip_suffix("MiB")
            .or_else(|| trimmed.strip_suffix("MB"))
        {
            if let Ok(mb_num) = mb_val.parse::<f64>() {
                if mb_num >= 1024.0 {
                    return format!("{:.1}GB", mb_num / 1024.0);
                } else {
                    return format!("{}MB", mb_num as u64);
                }
            }
        }

        // Parse GiB/GB values
        if let Some(gb_val) = trimmed
            .strip_suffix("GiB")
            .or_else(|| trimmed.strip_suffix("GB"))
        {
            if let Ok(gb_num) = gb_val.parse::<f64>() {
                return format!("{:.1}GB", gb_num);
            }
        }

        // Return as-is if can't parse
        trimmed.to_string()
    }

    /// Parse container status to extract icon, running state, and duration
    fn parse_container_status(&self, status: &str) -> (&'static str, bool, String) {
        if status.starts_with("Up") {
            let duration = if status.len() > 3 {
                // Extract duration from "Up 3 hours" -> "3h"
                let duration_part = &status[3..];
                self.format_duration_short(duration_part)
            } else {
                "now".to_string()
            };
            ("🟢", true, duration)
        } else if status.starts_with("Exited") {
            // Extract exit info from "Exited (0) 3 seconds ago" -> "stopped"
            if status.contains("ago") {
                ("🔴", false, "stopped".to_string())
            } else {
                ("🔴", false, "exited".to_string())
            }
        } else if status.contains("Created") {
            ("🔴", false, "created".to_string())
        } else {
            ("⚫", false, "unknown".to_string())
        }
    }

    /// Format duration to short form (3 hours -> 3h, 2 days -> 2d)
    fn format_duration_short(&self, duration: &str) -> String {
        let duration = duration.trim();

        if duration.contains("day") {
            if let Some(days) = duration.split_whitespace().next() {
                return format!("{}d", days);
            }
        } else if duration.contains("hour") {
            if let Some(hours) = duration.split_whitespace().next() {
                return format!("{}h", hours);
            }
        } else if duration.contains("minute") {
            if let Some(minutes) = duration.split_whitespace().next() {
                return format!("{}m", minutes);
            }
        } else if duration.contains("second") {
            return "now".to_string();
        }

        // Fallback: try to extract first number + first letter of unit
        if let Some(first_word) = duration.split_whitespace().next() {
            if let Some(unit_word) = duration.split_whitespace().nth(1) {
                if let Some(unit_char) = unit_word.chars().next() {
                    return format!("{}{}", first_word, unit_char);
                }
            }
        }

        duration.to_string()
    }

    /// Format ports for minimal display
    fn format_ports_minimal(&self, ports: &str) -> String {
        if ports.is_empty() {
            return "-".to_string();
        }

        // Parse port mappings and simplify display
        // "0.0.0.0:3100-3101->3100-3101/tcp, [::]:3100-3101->3100-3101/tcp" -> "3100-3101"
        let mut port_ranges = Vec::new();

        for mapping in ports.split(", ") {
            if let Some(port_part) = self.extract_port_from_mapping(mapping) {
                port_ranges.push(port_part);
            }
        }

        // Remove duplicates and join
        port_ranges.sort();
        port_ranges.dedup();

        if port_ranges.is_empty() {
            "-".to_string()
        } else {
            port_ranges.join(",")
        }
    }

    /// Extract port from a Docker port mapping string
    fn extract_port_from_mapping(&self, mapping: &str) -> Option<String> {
        if mapping.contains("->") {
            if let Some(external_part) = mapping.split("->").next() {
                // Extract port range from "0.0.0.0:3100-3101" or "[::]:3100-3101"
                if let Some(port_part) = external_part.split(':').next_back() {
                    return Some(port_part.to_string());
                }
            }
        }
        None
    }

    /// Get comprehensive status report with real-time metrics and service health
    pub fn get_status_report(&self, container: Option<&str>) -> Result<VmStatusReport> {
        let container_name = self.resolve_target_container(container)?;

        // Get basic container info via docker inspect
        let inspect_output = std::process::Command::new("docker")
            .args(["inspect", &container_name])
            .output()
            .map_err(|e| VmError::Internal(format!("Failed to inspect container: {}", e)))?;

        if !inspect_output.status.success() {
            return Err(VmError::Internal(format!(
                "Container '{}' not found",
                container_name
            )));
        }

        let inspect_data: serde_json::Value = serde_json::from_slice(&inspect_output.stdout)
            .map_err(|e| VmError::Internal(format!("Failed to parse container info: {}", e)))?;

        let container_info = &inspect_data[0];
        let state = &container_info["State"];
        let config = &container_info["Config"];

        let is_running = state["Running"].as_bool().unwrap_or(false);
        let container_id = container_info["Id"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        // Calculate uptime
        let uptime = if is_running {
            self.calculate_uptime(state)?
        } else {
            None
        };

        // Get resource usage only if container is running
        let resources = if is_running {
            self.get_container_resources(&container_name)?
        } else {
            ResourceUsage::default()
        };

        // Check service health only if container is running
        let services = if is_running {
            self.check_all_services(&container_name, config)?
        } else {
            vec![]
        };

        Ok(VmStatusReport {
            name: container_name,
            provider: "docker".to_string(),
            container_id: Some(container_id),
            is_running,
            uptime,
            resources,
            services,
        })
    }

    /// Calculate container uptime from Docker inspect data
    fn calculate_uptime(&self, state: &serde_json::Value) -> Result<Option<String>> {
        let Some(started_at) = state["StartedAt"].as_str() else {
            return Ok(None);
        };

        if started_at == "0001-01-01T00:00:00Z" {
            return Ok(None);
        }

        let start_time = match chrono::DateTime::parse_from_rfc3339(started_at) {
            Ok(time) => time,
            Err(_) => return Ok(Some("unknown".to_string())),
        };

        let now = chrono::Utc::now();
        let duration = now.signed_duration_since(start_time.with_timezone(&chrono::Utc));

        let uptime = if duration.num_days() > 0 {
            format!("{}d", duration.num_days())
        } else if duration.num_hours() > 0 {
            format!("{}h", duration.num_hours())
        } else if duration.num_minutes() > 0 {
            format!("{}m", duration.num_minutes())
        } else {
            "now".to_string()
        };

        Ok(Some(uptime))
    }

    /// Get real-time resource usage from docker stats
    fn get_container_resources(&self, container_name: &str) -> Result<ResourceUsage> {
        let stats_output = std::process::Command::new("docker")
            .args([
                "stats",
                "--no-stream",
                "--format",
                "{{.CPUPerc}}\t{{.MemUsage}}\t{{.MemPerc}}",
                container_name,
            ])
            .output()
            .map_err(|e| VmError::Internal(format!("Failed to get container stats: {}", e)))?;

        if !stats_output.status.success() {
            return Ok(ResourceUsage::default());
        }

        let stats_line = String::from_utf8_lossy(&stats_output.stdout);
        let parts: Vec<&str> = stats_line.trim().split('\t').collect();

        if parts.len() >= 2 {
            let cpu_str = parts[0].trim_end_matches('%');
            let memory_parts = parts[1].split('/').collect::<Vec<&str>>();

            let cpu_percent = cpu_str.parse::<f64>().ok();
            let (memory_used_mb, memory_limit_mb) = if memory_parts.len() >= 2 {
                let used = self.parse_memory_value(memory_parts[0].trim());
                let limit = self.parse_memory_value(memory_parts[1].trim());
                (used, limit)
            } else {
                (None, None)
            };

            // Get disk usage from container filesystem
            let (disk_used_gb, disk_total_gb) = self.get_disk_usage(container_name);

            return Ok(ResourceUsage {
                cpu_percent,
                memory_used_mb,
                memory_limit_mb,
                disk_used_gb,
                disk_total_gb,
            });
        }

        Ok(ResourceUsage::default())
    }

    /// Parse memory value from Docker stats (e.g., "123MiB" -> 123, "1.5GiB" -> 1536)
    fn parse_memory_value(&self, value: &str) -> Option<u64> {
        if let Some(mb_val) = value
            .strip_suffix("MiB")
            .or_else(|| value.strip_suffix("MB"))
        {
            mb_val.parse::<f64>().ok().map(|v| v as u64)
        } else if let Some(gb_val) = value
            .strip_suffix("GiB")
            .or_else(|| value.strip_suffix("GB"))
        {
            gb_val.parse::<f64>().ok().map(|v| (v * 1024.0) as u64)
        } else {
            None
        }
    }

    /// Get disk usage from container using df command
    fn get_disk_usage(&self, container_name: &str) -> (Option<f64>, Option<f64>) {
        let df_output = std::process::Command::new("docker")
            .args(["exec", container_name, "df", "-h", "/"])
            .output();

        let Ok(output) = df_output else {
            return (None, None);
        };
        if !output.status.success() {
            return (None, None);
        }

        let df_text = String::from_utf8_lossy(&output.stdout);
        for line in df_text.lines().skip(1) {
            // Skip header
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let used = self.parse_disk_value(parts[2]);
                let total = self.parse_disk_value(parts[1]);
                return (used, total);
            }
        }
        (None, None)
    }

    /// Parse disk value from df output (e.g., "1.5G" -> 1.5, "512M" -> 0.5)
    fn parse_disk_value(&self, value: &str) -> Option<f64> {
        if let Some(gb_val) = value.strip_suffix('G') {
            gb_val.parse::<f64>().ok()
        } else if let Some(mb_val) = value.strip_suffix('M') {
            mb_val.parse::<f64>().ok().map(|v| v / 1024.0)
        } else if let Some(kb_val) = value.strip_suffix('K') {
            kb_val.parse::<f64>().ok().map(|v| v / (1024.0 * 1024.0))
        } else {
            None
        }
    }

    /// Check health of all configured services
    fn check_all_services(
        &self,
        container_name: &str,
        config: &serde_json::Value,
    ) -> Result<Vec<ServiceStatus>> {
        let mut services = Vec::new();

        // Get port mappings from container config
        let Some(exposed_ports) = config["ExposedPorts"].as_object() else {
            return Ok(services);
        };

        for (port_spec, _) in exposed_ports {
            let Some(port_str) = port_spec.split('/').next() else {
                continue;
            };
            let Ok(port) = port_str.parse::<u16>() else {
                continue;
            };

            let service_name = self.identify_service_by_port(port);
            let host_port = self.get_host_port(container_name, port);
            let service_status = match service_name.as_str() {
                "postgresql" => {
                    super::health::check_postgres_status(container_name, port, host_port)
                }
                "redis" => super::health::check_redis_status(container_name, port, host_port),
                "mongodb" => super::health::check_mongodb_status(container_name, port, host_port),
                _ => ServiceStatus {
                    name: service_name,
                    is_running: true, // Assume running if port is exposed
                    port: Some(port),
                    host_port,
                    metrics: None,
                    error: None,
                },
            };
            services.push(service_status);
        }

        Ok(services)
    }

    /// Identify service type by port number
    fn identify_service_by_port(&self, port: u16) -> String {
        match port {
            5432 => "postgresql".to_string(),
            6379 => "redis".to_string(),
            27017 => "mongodb".to_string(),
            3306 => "mysql".to_string(),
            8080 => "http".to_string(),
            3000 => "node".to_string(),
            8000 => "python".to_string(),
            _ => format!("service-{}", port),
        }
    }

    /// Get the host port mapping for a container port
    fn get_host_port(&self, container_name: &str, container_port: u16) -> Option<u16> {
        let port_output = std::process::Command::new("docker")
            .args(["port", container_name, &container_port.to_string()])
            .output()
            .ok()?;

        if port_output.status.success() {
            let port_text = String::from_utf8_lossy(&port_output.stdout);
            // Parse "0.0.0.0:8080" -> 8080
            if let Some(host_port_str) = port_text.trim().split(':').next_back() {
                return host_port_str.parse::<u16>().ok();
            }
        }
        None
    }
}