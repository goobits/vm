pub mod discovery;
pub mod types;
pub mod validation;

pub use discovery::{
    discover_plugins, discover_plugins_in_directory, get_preset_plugins, get_service_plugins,
    load_preset_content, load_service_content,
};
pub use types::{Plugin, PluginInfo, PluginType, PresetContent, ServiceContent};
pub use validation::{
    validate_plugin, validate_plugin_with_context, ValidationError, ValidationResult,
};
