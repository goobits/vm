//! Canonical configuration validation entry point.

mod host;
mod structure;

use anyhow::Result;

use crate::config::VmConfig;

pub use host::{SuggestedFix, ValidationReport};

/// The runtime context in which configuration is being validated.
#[derive(Clone, Copy, Debug, Default)]
pub enum ValidationMode<'a> {
    /// Validate configuration without consulting mutable host state.
    #[default]
    Static,
    /// Validate a new environment, allowing ports owned by reusable services.
    Create { reusable_host_ports: &'a [u16] },
    /// Validate an environment that will replace its current runtime instance.
    Recreate,
}

/// Validate configuration through the single supported validation pipeline.
pub fn validate_config(config: &VmConfig, mode: ValidationMode<'_>) -> Result<ValidationReport> {
    let mut report = ValidationReport::default();
    if let Err(error) = structure::validate(config) {
        report.add_error(error.to_string());
        return Ok(report);
    }

    match mode {
        ValidationMode::Static => {}
        ValidationMode::Create {
            reusable_host_ports,
        } => host::validate(config, &mut report, true, reusable_host_ports)?,
        ValidationMode::Recreate => host::validate(config, &mut report, false, &[])?,
    }

    Ok(report)
}

#[cfg(test)]
mod tests;
