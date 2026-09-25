use serde::de::DeserializeOwned;
use serde_yaml_ng as yaml;
use vm_core::error::{Result, VmError};
use vm_plugin::PresetContent;

use crate::config::{HostSyncConfig, MountConfig, NetworkingConfig, TerminalConfig, ToolsConfig};

pub(super) struct PluginFields {
    pub networking: Option<NetworkingConfig>,
    pub host_sync: Option<HostSyncConfig>,
    pub terminal: Option<TerminalConfig>,
    pub mounts: Vec<MountConfig>,
    pub tools: ToolsConfig,
}

/// Validate exactly the fields a plugin can merge into a project.
pub fn validate_plugin_preset_content(content: &PresetContent) -> Result<()> {
    parse_plugin_fields(content).map(|_| ())
}

pub(super) fn parse_plugin_fields(content: &PresetContent) -> Result<PluginFields> {
    if !content.provision.is_empty() {
        return Err(VmError::Config(
            "Plugin preset provision scripts are unsupported".into(),
        ));
    }
    let tools: ToolsConfig =
        parse_optional_field("tools", content.tools.as_ref())?.unwrap_or_default();
    tools.validate()?;
    Ok(PluginFields {
        networking: parse_optional_field("networking", content.networking.as_ref())?,
        host_sync: parse_optional_field("host_sync", content.host_sync.as_ref())?,
        terminal: parse_optional_field("terminal", content.terminal.as_ref())?,
        mounts: parse_optional_field("mounts", content.mounts.as_ref())?.unwrap_or_default(),
        tools,
    })
}

fn parse_optional_field<T: DeserializeOwned>(
    field: &str,
    value: Option<&yaml::Value>,
) -> Result<Option<T>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let mut fields = yaml::Mapping::new();
    fields.insert(yaml::Value::String(field.to_string()), value.clone());
    crate::schema::validate_known_keys(&yaml::Value::Mapping(fields), false)?;
    yaml::from_value(value.clone())
        .map(Some)
        .map_err(|error| VmError::Config(format!("Invalid plugin preset field '{field}': {error}")))
}
