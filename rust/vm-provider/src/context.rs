//! Provider context for passing runtime options to providers
//!
//! This module provides a context structure that can be passed to provider
//! methods, allowing for runtime configuration without breaking the API.

use std::env;

/// Runtime context for provider operations
#[derive(Debug, Clone, Default)]
pub struct ProviderContext {
    /// Show detailed/verbose output
    pub verbose: bool,
}

impl ProviderContext {
    /// Create a new context with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a context with verbose output enabled
    pub fn with_verbose(verbose: bool) -> Self {
        Self { verbose }
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
