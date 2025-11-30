//! Provider context for passing runtime options to providers
//!
//! This module provides a context structure that can be passed to provider
//! methods, allowing for runtime configuration without breaking the API.

use std::env;
use vm_config::GlobalConfig;

/// Runtime context for provider operations
#[derive(Debug, Clone, Default)]
pub struct ProviderContext {
    /// Show detailed/verbose output
    pub verbose: bool,
    /// Global configuration settings
    pub global_config: Option<GlobalConfig>,
    /// Skip Ansible provisioning (used for snapshot builds)
    pub skip_provisioning: bool,
    /// Reuse existing service containers instead of failing
    pub preserve_services: bool,
    /// Using a pre-provisioned snapshot as base image
    pub is_snapshot: bool,
    /// Force refresh of all packages (bypasses cache for security updates)
    pub refresh_packages: bool,
}

impl ProviderContext {
    /// Create a new context with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a context with verbose output enabled
    pub fn with_verbose(verbose: bool) -> Self {
        Self {
            verbose,
            ..Default::default()
        }
    }

    /// Set the global config for the context
    pub fn with_config(mut self, global_config: GlobalConfig) -> Self {
        self.global_config = Some(global_config);
        self
    }

    /// Skip Ansible provisioning (for snapshot builds from Dockerfiles)
    pub fn skip_provisioning(mut self) -> Self {
        self.skip_provisioning = true;
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

    /// Set whether to force refresh all packages (bypasses cache)
    pub fn refresh_packages(mut self, refresh: bool) -> Self {
        self.refresh_packages = refresh;
        self
    }

    /// Check if verbose mode is enabled (CLI flag or environment variable)
    pub fn is_verbose(&self) -> bool {
        self.verbose || env::var("VM_VERBOSE").is_ok() || env::var("VM_DEBUG").is_ok()
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
