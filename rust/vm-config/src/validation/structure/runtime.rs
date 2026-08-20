use std::collections::HashSet;

use vm_core::error::{Result, VmError};

use crate::config::{ProviderName, VmConfig};

pub(super) fn validate_resource_limits(config: &VmConfig) -> Result<()> {
    let Some(vm) = &config.vm else {
        return Ok(());
    };
    if vm.cpus.as_ref().and_then(|cpus| cpus.to_count()) == Some(0) {
        return Err(VmError::Config("VM CPU count cannot be 0".to_string()));
    }
    if vm.memory.as_ref().and_then(|memory| memory.to_mb()) == Some(0) {
        return Err(VmError::Config(
            "VM memory allocation cannot be 0".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn validate_versions(config: &VmConfig) -> Result<()> {
    if let Some(versions) = &config.versions {
        if let Some(node) = &versions.node {
            if !is_valid_version(node) {
                return Err(vm_core::error::VmError::Config(format!(
                    "Invalid Node.js version: {node}"
                )));
            }
        }

        if let Some(python) = &versions.python {
            if !is_valid_version(python) {
                return Err(vm_core::error::VmError::Config(format!(
                    "Invalid Python version: {python}"
                )));
            }
        }
    }

    Ok(())
}

fn is_valid_version(version: &str) -> bool {
    if version == "latest" || version == "lts" || version.parse::<u32>().is_ok() {
        return true;
    }

    let parts: Vec<&str> = version.split('.').collect();

    if parts.len() < 2 || parts.len() > 3 {
        return false; // Must have 2-3 parts (X.Y or X.Y.Z)
    }

    for part in parts {
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }

    true
}

pub(super) fn validate_runtime(config: &VmConfig) -> Result<()> {
    if matches!(config.provider, Some(ProviderName::Tart)) {
        if let Some(user) = config
            .tart
            .as_ref()
            .and_then(|tart| tart.ssh_user.as_deref())
        {
            let mut characters = user.chars();
            let valid_first = characters
                .next()
                .is_some_and(|character| character.is_ascii_alphanumeric() || character == '_');
            let valid_rest = characters.all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
            });
            if !valid_first || !valid_rest {
                return Err(VmError::Config(format!(
                    "Invalid tart.ssh_user '{user}': use a Unix username, not SSH options or shell syntax"
                )));
            }
        }
    }

    let Some(vm) = &config.vm else {
        return Ok(());
    };

    if vm.pids_limit == Some(0) {
        return Err(VmError::Config(
            "vm.pids_limit must be greater than zero".to_string(),
        ));
    }
    if vm.stop_grace_period == Some(0) {
        return Err(VmError::Config(
            "vm.stop_grace_period must be greater than zero".to_string(),
        ));
    }

    if let Some(logging) = &vm.logging {
        if !matches!(logging.driver.as_str(), "local" | "json-file") {
            return Err(VmError::Config(
                "vm.logging.driver must be 'local' or 'json-file'".to_string(),
            ));
        }
        if !valid_size_string(&logging.max_size) {
            return Err(VmError::Config(
                "vm.logging.max_size must be a positive size such as '20m'".to_string(),
            ));
        }
        if logging.max_files == 0 {
            return Err(VmError::Config(
                "vm.logging.max_files must be greater than zero".to_string(),
            ));
        }
    }

    if matches!(config.provider, Some(ProviderName::Tart))
        && (vm.pids_limit.is_some() || vm.stop_grace_period.is_some() || vm.logging.is_some())
    {
        return Err(VmError::Config(
            "Container runtime limits and logging are not supported by Tart".to_string(),
        ));
    }

    Ok(())
}

pub(super) fn validate_bootstrap(config: &VmConfig) -> Result<()> {
    let Some(bootstrap) = &config.bootstrap else {
        return Ok(());
    };
    let mut browsers = HashSet::new();
    for browser in &bootstrap.playwright.browsers {
        let valid = !browser.is_empty()
            && browser.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
            });
        if !valid {
            return Err(VmError::Config(format!(
                "Invalid Playwright browser '{browser}': use letters, numbers, dots, dashes, or underscores"
            )));
        }
        if !browsers.insert(browser) {
            return Err(VmError::Config(format!(
                "Duplicate Playwright browser: {browser}"
            )));
        }
    }
    Ok(())
}

fn valid_size_string(value: &str) -> bool {
    let value = value.trim();
    if value.len() < 2 {
        return false;
    }
    let (number, suffix) = value.split_at(value.len() - 1);
    number.parse::<u64>().is_ok_and(|number| number > 0)
        && matches!(suffix.to_ascii_lowercase().as_str(), "k" | "m" | "g")
}
