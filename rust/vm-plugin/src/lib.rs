//! # VM Plugin System
//!
//! Provides plugin discovery and loading for presets and services.
//!
//! ## Architecture
//!
//! - **Preset Plugins**: Define development environments (box or provision types)
//! - **Service Plugins**: Define infrastructure services (PostgreSQL, Redis, etc.)
//!
//! ## Plugin Discovery
//!
//! Plugins are discovered from `~/.vm/plugins/{presets,services}/` directories.
//! Each plugin consists of:
//! - `plugin.yaml`: Metadata (name, version, type, category)
//! - `preset.yaml` or `service.yaml`: Configuration content
//!
//! ## Preset Categories
//!
//! - **Box Presets**: Reference pre-built Docker images (e.g., `@vibe-box`)
//! - **Provision Presets**: Define packages to install at runtime
//!
//! ## Usage
//!
//! ```rust
//! use vm_plugin::{discover_plugins, PresetCategory};
//!
//! let plugins = discover_plugins()?;
//! for plugin in plugins {
//!     println!("{}: {:?}", plugin.info.name, plugin.info.description);
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod discovery;
pub mod types;
pub mod validation;

pub use discovery::{
    discover_plugins, discover_plugins_in_directory, get_preset_plugins, get_service_plugins,
    load_preset_content, load_service_content,
};
pub use types::{Plugin, PluginInfo, PluginType, PresetCategory, PresetContent, ServiceContent};
pub use validation::{
    validate_plugin, validate_plugin_with_context, ValidationError, ValidationResult,
};
