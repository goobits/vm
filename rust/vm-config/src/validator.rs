// Standard library imports
use std::fmt;
use std::net::TcpListener;

// External crate imports
use anyhow::Result;
use sysinfo::System;

// Internal imports
use crate::config::VmConfig;

/// A report containing the results of a configuration validation check.
#[derive(Default, Debug)]
pub struct ValidationReport {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub info: Vec<String>,
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

    /// Returns `true` if the report contains any errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

impl fmt::Display for ValidationReport {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for error in &self.errors {
            writeln!(f, "❌ {}", error)?;
        }
        for warning in &self.warnings {
            writeln!(f, "⚠️  {}", warning)?;
        }
        for info in &self.info {
            writeln!(f, "ℹ️  {}", info)?;
        }
        Ok(())
    }
}

/// A validator for checking the `VmConfig` against the host system's resources.
pub struct ConfigValidator {
    system: System,
}

impl ConfigValidator {
    /// Creates a new `ConfigValidator`.
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_cpu();
        system.refresh_memory();
        Self { system }
    }

    /// Validates the given configuration.
    pub fn validate(&self, config: &VmConfig) -> Result<ValidationReport> {
        let mut report = ValidationReport::default();

        self.validate_cpu(config, &mut report)?;
        self.validate_memory(config, &mut report)?;
        self.validate_ports(config, &mut report)?;
        // self.validate_disk_space(&mut report)?; // Placeholder for future implementation

        Ok(report)
    }

    /// Validates the CPU configuration.
    fn validate_cpu(&self, config: &VmConfig, report: &mut ValidationReport) -> Result<()> {
        if let Some(vm_settings) = &config.vm {
            if let Some(requested_cpus) = vm_settings.cpus {
                let available_cpus = self.system.cpus().len() as u32;
                if requested_cpus > available_cpus {
                    report.add_error(format!(
                        "Requested {} CPUs but only {} are available. Please reduce 'vm.cpus' in your vm.yaml.",
                        requested_cpus, available_cpus
                    ));
                } else if requested_cpus > (available_cpus * 3 / 4) {
                    report.add_warning(format!(
                        "Assigning {} of {} available CPUs may impact host performance.",
                        requested_cpus, available_cpus
                    ));
                }
            }
        }
        Ok(())
    }

    /// Validates the memory configuration.
    fn validate_memory(&self, config: &VmConfig, report: &mut ValidationReport) -> Result<()> {
        if let Some(vm_settings) = &config.vm {
            if let Some(memory_limit) = &vm_settings.memory {
                if let Some(requested_mb) = memory_limit.to_mb() {
                    let total_mb = self.system.total_memory() / 1024 / 1024;
                    if requested_mb as u64 > total_mb {
                        report.add_error(format!(
                            "Requested {}MB RAM but only {}MB is available. Please reduce 'vm.memory' in your vm.yaml.",
                            requested_mb, total_mb
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Validates the port mappings.
    fn validate_ports(&self, config: &VmConfig, report: &mut ValidationReport) -> Result<()> {
        let binding_ip = config.vm.as_ref()
            .and_then(|vm| vm.port_binding.as_deref())
            .unwrap_or("127.0.0.1");

        for service in config.services.values() {
            if let Some(port) = service.port {
                let addr = format!("{}:{}", binding_ip, port);
                if TcpListener::bind(&addr).is_err() {
                    report.add_error(format!(
                        "Port {} is already in use on the host. Please change the port for this service in your vm.yaml or free it.",
                        port
                    ));
                }
            }
        }
        Ok(())
    }
}