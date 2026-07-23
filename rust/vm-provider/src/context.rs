//! Provider context for passing runtime options to providers
//!
//! This module provides a context structure that can be passed to provider
//! methods, allowing for runtime configuration without breaking the API.

use std::env;
use vm_config::GlobalConfig;

/// Runtime context for provider operations
#[derive(Debug, Clone, Default)]
pub struct ProviderContext {
    /// Global configuration settings
    pub global_config: Option<GlobalConfig>,
    /// Reuse existing service containers instead of failing
    pub preserve_services: bool,
    /// Using a pre-provisioned snapshot as base image
    pub is_snapshot: bool,
}

impl ProviderContext {
    /// Set the global config for the context
    pub fn with_config(mut self, global_config: GlobalConfig) -> Self {
        self.global_config = Some(global_config);
        self
    }

    /// Set whether to preserve/reuse existing service containers
    pub fn preserve_services(mut self, preserve: bool) -> Self {
        self.preserve_services = preserve;
        self
    }

    /// Set whether using a pre-provisioned snapshot as base image
    pub fn with_snapshot(mut self, is_snapshot: bool) -> Self {
        self.is_snapshot = is_snapshot;
        self
    }

    /// Check if verbose mode is enabled through the process environment.
    pub fn is_verbose(&self) -> bool {
        env::var("VM_VERBOSE").is_ok() || env::var("VM_DEBUG").is_ok()
    }

    /// Get the Ansible verbosity flag based on context
    pub fn ansible_verbosity(&self) -> &'static str {
        if self.is_verbose() {
            "-vvv"
        } else {
            ""
        }
    }
}
