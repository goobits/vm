//! Utility functions for the VM crate.

use crate::error::{VmError, VmResult};

// Password generation has been moved to vm_core::secrets module

/// VM-local wrapper around the shared core arrow-key confirmation prompt.
pub fn confirm_select(prompt: &str, default: bool) -> VmResult<bool> {
    vm_core::prompts::confirm_select(prompt, default)
        .map_err(|e| VmError::general(e, "Failed to read user selection"))
}

pub fn configured_container_runtime() -> String {
    vm_config::AppConfig::load(None, None, None)
        .ok()
        .and_then(|config| {
            config
                .vm
                .provider
                .filter(vm_config::config::ProviderName::is_container)
                .map(|provider| provider.to_string())
                .or(config.global.defaults.provider)
                .filter(|provider| matches!(provider.as_str(), "docker" | "podman"))
        })
        .unwrap_or_else(|| "docker".to_string())
}
