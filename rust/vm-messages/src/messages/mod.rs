//! Central registry for all user-facing message templates.
//!
//! This module is organized by domain for better maintainability:
//! - `vm` - VM lifecycle messages (create, start, stop, destroy, etc.)
//! - `config` - Configuration messages (init, set, validate, presets, etc.)
//! - `plugin` - Plugin messages (install, list, remove, validate, etc.)
//! - `service` - Service messages (auth, pkg, docker, installer, temp VM, etc.)
//! - `common` - Shared/reusable messages across commands
//!
//! ## Message Access Patterns
//!
//! Messages are accessed through the `MESSAGES` constant:
//!
//! ```rust
//! use vm_messages::MESSAGES;
//!
//! // VM messages
//! let msg = MESSAGES.vm.create_success;
//!
//! // Config messages
//! let msg = MESSAGES.config.validate_header;
//!
//! // Plugin messages
//! let msg = MESSAGES.plugin.install_success;
//!
//! // Service messages
//! let msg = MESSAGES.service.docker_is_running;
//!
//! // Common messages
//! let msg = MESSAGES.common.success;
//! ```

mod common;
mod config;
mod plugin;
mod service;
mod vm;

pub use common::{CommonMessages, COMMON_MESSAGES};
pub use config::{ConfigMessages, CONFIG_MESSAGES};
pub use plugin::{PluginMessages, PLUGIN_MESSAGES};
pub use service::{ServiceMessages, SERVICE_MESSAGES};
pub use vm::{VmMessages, VM_MESSAGES};

/// Unified messages struct containing all domain-specific message modules
pub struct Messages {
    pub vm: VmMessages,
    pub config: ConfigMessages,
    pub plugin: PluginMessages,
    pub service: ServiceMessages,
    pub common: CommonMessages,
}

/// Global messages constant - main entry point for all message templates
pub const MESSAGES: Messages = Messages {
    vm: VM_MESSAGES,
    config: CONFIG_MESSAGES,
    plugin: PLUGIN_MESSAGES,
    service: SERVICE_MESSAGES,
    common: COMMON_MESSAGES,
};
