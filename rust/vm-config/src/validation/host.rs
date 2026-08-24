// Standard library imports
use std::collections::HashSet;
use std::fmt;
use std::net::TcpListener;

// External crate imports
use anyhow::Result;
// Internal imports
use crate::config::VmConfig;

/// A suggested fix for a validation error
#[derive(Debug, Clone)]
pub struct SuggestedFix {
    pub field: String,
    pub value: String,
    pub description: String,
}

/// A report containing the results of a configuration validation check.
#[derive(Default, Debug)]
pub struct ValidationReport {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub info: Vec<String>,
    pub suggested_fixes: Vec<SuggestedFix>,
}

impl ValidationReport {
    /// Adds an error message to the report.
    pub fn add_error(&mut self, msg: String) {
        self.errors.push(msg);
    }

    /// Adds a warning message to the report.
    pub fn add_warning(&mut self, msg: String) {
        self.warnings.push(msg);
    }

    /// Adds an informational message to the report.
    pub fn add_info(&mut self, msg: String) {
        self.info.push(msg);
    }

    /// Adds a suggested fix to the report.
    pub fn add_fix(&mut self, fix: SuggestedFix) {
        self.suggested_fixes.push(fix);
    }

    /// Returns `true` if the report contains any errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Returns `true` if the report has suggested fixes.
    pub fn has_fixes(&self) -> bool {
        !self.suggested_fixes.is_empty()
    }
}

impl fmt::Display for ValidationReport {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for error in &self.errors {
            writeln!(f, "  ❌ {error}")?;
        }
        for warning in &self.warnings {
            writeln!(f, "  ⚠️  {warning}")?;
        }
        for info in &self.info {
            writeln!(f, "  ℹ️  {info}")?;
        }
        if !self.suggested_fixes.is_empty() {
            writeln!(f)?;
            writeln!(f, "💡 Suggested fixes:")?;
            for fix in &self.suggested_fixes {
                writeln!(f, "  • {} → {}: {}", fix.field, fix.value, fix.description)?;
            }
        }
        Ok(())
    }
}

/// A validator for checking the `VmConfig` against the host system's resources.
struct HostValidator;

pub(super) fn validate(
    config: &VmConfig,
    report: &mut ValidationReport,
    check_port_availability: bool,
    reusable_host_ports: &[u16],
) -> Result<()> {
    let validator = HostValidator::new();
    validator.validate_cpu(config, report)?;
    validator.validate_memory(config, report)?;
    validator.validate_ports(config, report, check_port_availability, reusable_host_ports)?;
    validator.validate_user(config, report)
}

impl Default for HostValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl HostValidator {
    /// Creates a new host validator.
    pub fn new() -> Self {
        Self
    }

    /// Validates that the user is configured.
    fn validate_user(&self, config: &VmConfig, report: &mut ValidationReport) -> Result<()> {
        if let Some(vm_settings) = &config.vm {
            if vm_settings.user.as_deref().unwrap_or("").is_empty() {
                report.add_error(
                    "The 'vm.user' field in your vm.yaml is missing or empty. Please specify a username.".to_string(),
                );
            }
        } else {
            report.add_error(
                "The 'vm' section is missing in your vm.yaml. Please specify a username under 'vm.user'.".to_string(),
            );
        }
        Ok(())
    }

    /// Validates the CPU configuration.
    fn validate_cpu(&self, config: &VmConfig, report: &mut ValidationReport) -> Result<()> {
        if let Some(vm_settings) = &config.vm {
            if let Some(cpu_limit) = &vm_settings.cpus {
                let available_cpus = vm_platform::platform::available_parallelism() as u32;

                // Resolve percentage or get direct value
                let requested_cpus = match cpu_limit {
                    crate::config::CpuLimit::Limited(count) => Some(*count),
                    crate::config::CpuLimit::Percentage(percent) => {
                        // Percentage resolution should always succeed for valid percentage
                        #[allow(clippy::excessive_nesting)]
                        match cpu_limit.resolve_percentage(available_cpus) {
                            Some(resolved) => {
                                report.add_info(format!(
                                    "{}% of {} CPUs = {} CPUs",
                                    percent, available_cpus, resolved
                                ));
                                Some(resolved)
                            }
                            None => {
                                report.add_error(format!(
                                    "Failed to resolve CPU percentage: {}%",
                                    percent
                                ));
                                None
                            }
                        }
                    }
                    crate::config::CpuLimit::Unlimited => None,
                };

                if let Some(requested) = requested_cpus {
                    #[allow(clippy::excessive_nesting)]
                    if requested > available_cpus {
                        report.add_error(format!(
                            "Requested {} CPUs but only {} are available. Please reduce 'vm.cpus' in your vm.yaml.",
                            requested, available_cpus
                        ));
                    } else if requested > (available_cpus * 3 / 4) {
                        report.add_warning(format!(
                            "Assigning {} of {} available CPUs may impact host performance.",
                            requested, available_cpus
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Validates the memory configuration.
    fn validate_memory(&self, config: &VmConfig, report: &mut ValidationReport) -> Result<()> {
        if let Some(vm_settings) = &config.vm {
            if let Some(memory_limit) = &vm_settings.memory {
                let total_mb = vm_platform::platform::total_memory_mb()?;

                // Resolve percentage or get direct value
                let requested_mb = match memory_limit {
                    crate::config::MemoryLimit::Limited(mb) => Some(*mb as u64),
                    crate::config::MemoryLimit::Percentage(percent) => {
                        // Percentage resolution should always succeed for valid percentage
                        #[allow(clippy::excessive_nesting)]
                        match memory_limit.resolve_percentage(total_mb) {
                            Some(resolved) => {
                                report.add_info(format!(
                                    "{}% of {}MB RAM = {}MB",
                                    percent, total_mb, resolved
                                ));
                                Some(resolved as u64)
                            }
                            None => {
                                report.add_error(format!(
                                    "Failed to resolve memory percentage: {}%",
                                    percent
                                ));
                                None
                            }
                        }
                    }
                    crate::config::MemoryLimit::Unlimited => None,
                };

                if let Some(requested) = requested_mb {
                    #[allow(clippy::excessive_nesting)]
                    if requested > total_mb {
                        report.add_error(format!(
                            "Requested {}MB RAM but only {}MB is available. Please reduce 'vm.memory' in your vm.yaml.",
                            requested, total_mb
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Validates the port mappings.
    fn validate_ports(
        &self,
        config: &VmConfig,
        report: &mut ValidationReport,
        check_port_availability: bool,
        reusable_host_ports: &[u16],
    ) -> Result<()> {
        let binding_ip = config
            .vm
            .as_ref()
            .and_then(|vm| vm.port_binding.as_deref())
            .unwrap_or("127.0.0.1");

        // Get port range for suggestions
        let port_range = config.ports.range.as_ref().and_then(|r| {
            if r.len() == 2 {
                Some((r[0], r[1]))
            } else {
                None
            }
        });

        // Collect already-used ports
        let mut used_ports: Vec<u16> = config.services.values().filter_map(|s| s.port).collect();
        used_ports.sort_unstable();

        // Check for enabled services without assigned ports
        for (service_name, service) in &config.services {
            if !service.enabled || service.port.is_some() {
                continue;
            }

            // Skip port validation for services that don't require network ports
            // Docker-in-Docker is accessed via socket, not network
            if service_name == "docker" {
                continue;
            }

            report.add_error(format!(
                "Service '{}' is enabled but has no port specified",
                service_name
            ));

            // Suggest next available port from range
            let Some((start, end)) = port_range else {
                continue;
            };
            let Some(suggested_port) = (start..=end).find(|p| !used_ports.contains(p)) else {
                continue;
            };

            report.add_fix(SuggestedFix {
                field: format!("services.{}.port", service_name),
                value: suggested_port.to_string(),
                description: format!("Assign available port to {}", service_name),
            });
            used_ports.push(suggested_port);
            used_ports.sort_unstable();
        }

        if !check_port_availability {
            return Ok(());
        }

        // Check for port conflicts
        let mut mapped_host_ports = HashSet::new();
        for mapping in &config.ports.mappings {
            if !mapped_host_ports.insert(mapping.host) {
                report.add_error(format!("Duplicate host port mapping: {}", mapping.host));
                continue;
            }

            if mapping.host == 0 || mapping.guest == 0 {
                report.add_error("Port numbers must be greater than 0".to_string());
                continue;
            }

            let addr = format!("{binding_ip}:{}", mapping.host);
            if !reusable_host_ports.contains(&mapping.host) && TcpListener::bind(&addr).is_err() {
                report.add_error(format!(
                    "Configuration error: Port {} is already in use on host",
                    mapping.host
                ));
            }
        }

        for service in config.services.values() {
            if let Some(port) = service.port {
                let addr = format!("{binding_ip}:{port}");
                if !reusable_host_ports.contains(&port) && TcpListener::bind(&addr).is_err() {
                    report.add_error(format!(
                        "Port {port} is already in use on the host. Please change the port for this service in your vm.yaml or free it."
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{validate_config, ValidationMode};
    use crate::config::{ProjectConfig, ServiceConfig, VmConfig, VmSettings};

    fn valid_config() -> VmConfig {
        VmConfig {
            provider: Some("docker".into()),
            project: Some(ProjectConfig {
                name: Some("host-validation-test".to_string()),
                ..Default::default()
            }),
            vm: Some(VmSettings {
                user: Some("developer".to_string()),
                port_binding: Some("127.0.0.1".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn recreate_validation_ignores_only_host_port_occupancy() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut config = valid_config();
        config.services.insert(
            "postgresql".to_string(),
            ServiceConfig {
                enabled: true,
                port: Some(port),
                ..Default::default()
            },
        );

        let normal = validate_config(
            &config,
            ValidationMode::Create {
                reusable_host_ports: &[],
            },
        )
        .unwrap();
        let recreate = validate_config(&config, ValidationMode::Recreate).unwrap();

        assert!(normal.errors.iter().any(|error| error.contains("in use")));
        assert!(recreate.errors.is_empty());
    }

    #[test]
    fn reusable_service_ports_do_not_hide_unrelated_conflicts() {
        let reusable_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let reusable_port = reusable_listener.local_addr().unwrap().port();
        let unrelated_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let unrelated_port = unrelated_listener.local_addr().unwrap().port();
        let mut config = valid_config();
        config.services.insert(
            "postgresql".to_string(),
            ServiceConfig {
                enabled: true,
                port: Some(reusable_port),
                ..Default::default()
            },
        );
        config.services.insert(
            "redis".to_string(),
            ServiceConfig {
                enabled: true,
                port: Some(unrelated_port),
                ..Default::default()
            },
        );

        let report = validate_config(
            &config,
            ValidationMode::Create {
                reusable_host_ports: &[reusable_port],
            },
        )
        .unwrap();

        assert!(!report
            .errors
            .iter()
            .any(|error| error.contains(&reusable_port.to_string())));
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains(&unrelated_port.to_string())));
    }
}
